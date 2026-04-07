use std::collections::BTreeMap;
use std::fs::File;
use std::io::{ErrorKind, IoSlice, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::mem::size_of;
use std::ops::Deref;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard};
use std::thread::JoinHandle;
use std::{fs::OpenOptions, io};

use memmap2::{Advice, MmapOptions, MmapRaw, RemapOptions};

use crate::pod;

pub type Result<T, E = io::Error> = std::result::Result<T, E>;

pub trait TableData: pod::TablePod {
    type Header: pod::TablePod;
}

#[derive(Default)]
struct AppenderData {
    appended: AtomicBool,
}
pub struct TableFile<D> {
    file: File,
    mmap: RwLock<MmapRaw>,
    appender: Option<Arc<AppenderData>>,
    _phantom: PhantomData<fn(&D)>,
}

pub struct Appender<D>(File, Arc<AppenderData>, PhantomData<fn(&D)>);

pub struct AppenderJob<D: TableData> {
    counter: usize,
    join: JoinHandle<Result<Appender<D>>>,
    sender: crossbeam_channel::Sender<(usize, Vec<D>)>,
}

pub struct AppenderResultHandle<D: TableData>(usize, crossbeam_channel::Sender<(usize, Vec<D>)>);

pub struct Ref<'l, T: ?Sized>(RwLockReadGuard<'l, MmapRaw>, NonNull<T>);

impl<D: TableData> TableFile<D> {
    const HEADER_SIZE: usize = std::mem::size_of::<D::Header>().next_multiple_of(512);
    const DATA_SIZE: usize = std::mem::size_of::<D>();

    const _ASSERTS: () = {
        pod::assert_valid_table_pod::<D::Header>();
        pod::assert_valid_table_pod::<D>();
    };

    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::new_intern(
            OpenOptions::new().read(true).open(path)?,
            true,
            || unreachable!(),
        )
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self>
    where
        D::Header: Default,
    {
        Self::open_with(path, Default::default)
    }

    pub fn open_with<P: AsRef<Path>>(
        path: P,
        init_header: impl FnOnce() -> D::Header,
    ) -> Result<Self> {
        Self::new_intern(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)?,
            false,
            init_header,
        )
    }

    pub fn open_override<P: AsRef<Path>>(path: P) -> Result<Self>
    where
        D::Header: Default,
    {
        Self::open_override_with(path, Default::default)
    }

    pub fn open_override_with<P: AsRef<Path>>(
        path: P,
        init_header: impl FnOnce() -> D::Header,
    ) -> Result<Self> {
        Self::new_intern(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?,
            false,
            init_header,
        )
    }

    #[inline]
    pub fn new(file: File) -> Result<Self>
    where
        D::Header: Default,
    {
        Self::new_intern(file, false, Default::default)
    }

    #[inline]
    pub fn with(file: File, init_header: impl FnOnce() -> D::Header) -> Result<Self> {
        Self::new_intern(file, false, init_header)
    }

    fn new_intern(
        mut file: File,
        read_only: bool,
        init_header: impl FnOnce() -> D::Header,
    ) -> Result<Self> {
        let mut file_len = file.seek(io::SeekFrom::End(0))?;
        if file_len == 0 && !read_only {
            let mut new_header = init_header();
            file.write_all(pod::as_bytes_mut(std::slice::from_mut(&mut new_header)))?;
            file_len = Self::HEADER_SIZE as u64;
            file.set_len(file_len)?;
            file.seek(io::SeekFrom::Start(file_len))?;
        }
        if file_len < Self::HEADER_SIZE as u64 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if file_len > isize::MAX as u64 {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        if !(file_len - Self::HEADER_SIZE as u64).is_multiple_of(Self::DATA_SIZE as u64) {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "File not aligned",
            ));
        }
        let mut opts = MmapOptions::new();
        opts.len(file_len as usize);
        let mmap = if read_only {
            opts.map_raw_read_only(&file)?
        } else {
            opts.map_raw(&file)?
        };
        mmap.advise(Advice::Random)?;
        Ok(Self {
            file,
            mmap: RwLock::new(mmap),
            appender: None,
            _phantom: PhantomData,
        })
    }

    fn check_no_appender(&self) -> Result<()> {
        match &self.appender {
            Some(a) if Arc::strong_count(a) > 1 => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Appender already created",
            )),
            _ => Ok(()),
        }
    }

    pub fn appender(&mut self) -> Result<Appender<D>> {
        self.check_no_appender()?;
        let data = self.appender.get_or_insert_with(Default::default);
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::End(0))?;
        Ok(Appender(file, Arc::clone(data), PhantomData))
    }

    pub fn flush(&self) -> Result<()> {
        self.mmap
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .flush()?;
        Ok(())
    }

    pub fn header(&self) -> Result<Ref<'_, D::Header>> {
        let mmap = self.mmap.read().unwrap_or_else(PoisonError::into_inner);
        if mmap.len() < size_of::<D::Header>() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let p: *mut D::Header = mmap.as_mut_ptr() as _;
        Ok(Ref(mmap, NonNull::new(p).unwrap()))
    }

    pub fn header_mut(&mut self) -> Result<&mut D::Header> {
        let mmap = self.mmap.get_mut().unwrap_or_else(PoisonError::into_inner);
        if mmap.len() < size_of::<D::Header>() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        unsafe {
            let p: *mut D::Header = mmap.as_mut_ptr() as _;
            Ok(&mut *p)
        }
    }

    fn check_grow(&self) -> bool {
        if let Some(a) = &self.appender {
            a.appended.swap(false, Ordering::AcqRel)
        } else {
            false
        }
    }

    fn grow_mmap(mmap: &mut MmapRaw, new_len: u64) -> Result<()> {
        if new_len > isize::MAX as u64 {
            //TODO: nightly: return Err(io::ErrorKind::FileTooLarge.into());
            return Err(io::ErrorKind::OutOfMemory.into());
        }
        let old_len = mmap.len();
        let new_len = new_len as usize;
        if new_len < old_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "File shrinked"));
        }
        if new_len > old_len {
            unsafe {
                mmap.remap(new_len, RemapOptions::new().may_move(true))?;
            }
        }
        Ok(())
    }

    fn get_mmap_grow_full(&self) -> Result<RwLockReadGuard<'_, MmapRaw>> {
        if self.check_grow() {
            let mut mmap = self.mmap.write().unwrap_or_else(PoisonError::into_inner);
            let new_len = self.file.metadata()?.len();
            Self::grow_mmap(&mut mmap, new_len)?;
        }
        let mmap = self.mmap.read().unwrap_or_else(PoisonError::into_inner);
        Ok(mmap)
    }

    fn get_mmap_grow(&self, required_len: usize) -> Result<RwLockReadGuard<'_, MmapRaw>> {
        {
            let mmap = self.mmap.read().unwrap_or_else(PoisonError::into_inner);
            if required_len <= mmap.len() {
                return Ok(mmap);
            }
        }

        self.get_mmap_grow_full()
    }

    #[inline]
    pub fn len(&self) -> usize {
        let mmap = self.get_mmap_grow_full().unwrap();
        (mmap.len() - Self::HEADER_SIZE) / Self::DATA_SIZE
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Binary-search for an item by its key.
    ///
    /// Returns `Some((table_index, item))` when found, or `None` when not present.
    /// Requires the table to be sorted by key, which is guaranteed after import.
    pub fn find(&self, key: &D::Key) -> Result<Option<(usize, Ref<'_, D>)>>
    where
        D: crate::pod::Item,
    {
        let all = self.get_all()?;
        let result = all.binary_search_by(|d| d.key().cmp(key));
        drop(all);
        match result {
            Ok(idx) => Ok(Some((idx, self.get(idx)?))),
            Err(_) => Ok(None),
        }
    }

    #[inline]
    pub fn get(&self, index: usize) -> Result<Ref<'_, D>> {
        let Ref(mmap, slice) = self.get_slice(index, 1)?;
        Ok(Ref(mmap, unsafe {
            NonNull::new_unchecked(slice.as_ptr() as *mut D)
        }))
    }

    pub fn get_all(&self) -> Result<Ref<'_, [D]>> {
        let mmap = self.get_mmap_grow_full()?;
        let len = (mmap.len() - Self::HEADER_SIZE) / Self::DATA_SIZE;
        let ptr: NonNull<D> =
            unsafe { NonNull::new(mmap.as_ptr().add(Self::HEADER_SIZE) as _).unwrap() };
        let slice = NonNull::slice_from_raw_parts(ptr, len);
        Ok(Ref(mmap, slice))
    }

    pub fn get_all_mut(&mut self) -> Result<&mut [D]> {
        let mmap = self.mmap.get_mut().unwrap_or_else(PoisonError::into_inner);
        let full_len = self.file.metadata()?.len();
        Self::grow_mmap(mmap, full_len)?;
        let len = (mmap.len() - Self::HEADER_SIZE) / Self::DATA_SIZE;
        unsafe {
            let ptr: *mut D = mmap.as_ptr().add(Self::HEADER_SIZE) as _;
            let slice = std::slice::from_raw_parts_mut(ptr, len);
            Ok(slice)
        }
    }

    pub fn get_slice(&self, index: usize, len: usize) -> Result<Ref<'_, [D]>> {
        let offset = Self::HEADER_SIZE + index * Self::DATA_SIZE;
        let offset_end = offset + len * Self::DATA_SIZE;
        let mmap = self.get_mmap_grow(offset_end)?;
        let ptr: NonNull<D> = unsafe { NonNull::new(mmap.as_ptr().add(offset) as _).unwrap() };
        let slice = NonNull::slice_from_raw_parts(ptr, len);
        Ok(Ref(mmap, slice))
    }

    pub fn truncate(&mut self, new_len: usize) -> Result<()> {
        self.check_no_appender()?;
        let new_len_bytes = Self::HEADER_SIZE + new_len * Self::DATA_SIZE;
        let old_len = self.file.metadata()?.len();
        if old_len < new_len_bytes as u64 {
            return Err(ErrorKind::InvalidData.into());
        }
        if old_len > new_len_bytes as u64 {
            let mmap = self.mmap.get_mut().unwrap_or_else(PoisonError::into_inner);
            unsafe {
                mmap.remap(new_len_bytes, RemapOptions::new())?;
            }
            self.file.set_len(new_len_bytes as u64)?;
        }
        Ok(())
    }

    pub fn filter(&mut self, filter_fn: impl Fn(&D) -> bool) -> Result<()> {
        self.check_no_appender()?;

        let mmap = self.mmap.get_mut().unwrap_or_else(PoisonError::into_inner);
        let full_len = self.file.metadata()?.len();
        Self::grow_mmap(mmap, full_len)?;
        let len = mmap.len();

        let (mut dst, e) = unsafe {
            (
                mmap.as_mut_ptr().add(Self::HEADER_SIZE),
                mmap.as_ptr().add(len),
            )
        };
        let mut src: *const u8 = dst;
        let mut p = src;
        let mut l = 0;
        loop {
            let n = unsafe { p.add(Self::DATA_SIZE) };
            let has_next = n <= e;
            if has_next {
                let d: *const D = p as _;
                p = n;
                if filter_fn(unsafe { &*d }) {
                    l += Self::DATA_SIZE;
                    continue;
                }
            }
            // end or not matches
            if l > 0 {
                if src != dst {
                    unsafe {
                        std::ptr::copy(src, dst, l);
                    }
                }
                dst = unsafe { dst.add(l) };
                l = 0;
            }
            src = p;
            if !has_next {
                break;
            }
        }

        unsafe {
            let new_len = dst.offset_from(mmap.as_ptr()) as usize;
            let old_len = mmap.len();
            assert!(new_len <= old_len);
            if new_len != mmap.len() {
                mmap.remap(new_len, RemapOptions::new())?;
                self.file.set_len(new_len as u64)?;
            }
        }

        Ok(())
    }

    // TODO: mutable access to items / would require house-keeping for a locking-mechanism
    // maybe seperate New-Type for this
}

#[inline]
unsafe fn slice_as_bytes<T>(data: &[T]) -> &[u8] {
    let len = std::mem::size_of_val(data);
    let ptr: *const u8 = data.as_ptr() as _;
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

fn write_all_vectored<W: Write>(w: &mut W, mut bufs: &mut [IoSlice<'_>]) -> io::Result<()> {
    IoSlice::advance_slices(&mut bufs, 0);
    while !bufs.is_empty() {
        match w.write_vectored(bufs) {
            Ok(0) => {
                return Err(io::ErrorKind::WriteZero.into());
            }
            Ok(n) => IoSlice::advance_slices(&mut bufs, n),
            Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

impl<D: TableData> Appender<D> {
    pub fn append(&mut self, data: &[D]) -> io::Result<()> {
        self.1.appended.store(true, Ordering::Release);
        let bytes = unsafe { slice_as_bytes(data) };
        self.0.write_all(bytes)
    }

    pub fn append_vectored<'i>(&mut self, data: impl Iterator<Item = &'i [D]>) -> io::Result<()>
    where
        D: 'i,
    {
        self.1.appended.store(true, Ordering::Release);
        let mut bufs = [IoSlice::new(&[]); 64];
        let mut i = 0;
        for data in data {
            if data.is_empty() {
                continue;
            }
            if i >= bufs.len() {
                write_all_vectored(&mut self.0, &mut bufs)?;
                i = 0;
            }
            bufs[i] = IoSlice::new(unsafe { slice_as_bytes(data) });
            i += 1;
        }
        if i > 0 {
            write_all_vectored(&mut self.0, &mut bufs[..i])?;
        }
        Ok(())
    }
}

impl<D: TableData + Send + 'static> Appender<D> {
    pub fn spawn(self) -> AppenderJob<D> {
        let (s, r) = crossbeam_channel::unbounded();
        let join = std::thread::spawn(move || {
            let mut appender = self;
            let mut next = 0;
            let mut sorted = BTreeMap::new();
            let mut bufs = Vec::new();
            while let Ok((i, dat)) = r.recv() {
                sorted.insert(i, dat);
                while let Ok((i, dat)) = r.try_recv() {
                    sorted.insert(i, dat);
                }
                while let Some(e) = sorted.first_entry() {
                    if e.key() == &next {
                        next += 1;
                        let buf: Vec<D> = e.remove();
                        if !buf.is_empty() {
                            bufs.push(buf);
                        }
                    } else {
                        break;
                    }
                }
                if !bufs.is_empty() {
                    if bufs.len() == 1 {
                        appender.append(&bufs[0])?;
                    } else {
                        appender.append_vectored(bufs.iter().map(Deref::deref))?;
                    }
                    bufs.clear();
                }
            }
            Ok(appender)
        });
        AppenderJob {
            counter: 0,
            sender: s,
            join,
        }
    }
}

impl<D: TableData> AppenderJob<D> {
    pub fn start(&mut self) -> AppenderResultHandle<D> {
        let i = self.counter;
        self.counter += 1;
        AppenderResultHandle(i, self.sender.clone())
    }

    pub fn join(self) -> Result<Appender<D>> {
        let AppenderJob { join, sender, .. } = self;
        drop(sender);
        let appender = join.join().unwrap()?;
        Ok(appender)
    }
}

impl<D: TableData> AppenderResultHandle<D> {
    pub fn done(mut self, result: Vec<D>) {
        self.1.send((self.0, result)).unwrap();
        self.0 = usize::MAX;
    }
}

impl<D: TableData> Drop for AppenderResultHandle<D> {
    fn drop(&mut self) {
        if self.0 != usize::MAX {
            self.1.send((self.0, Vec::new())).unwrap();
        }
    }
}

impl<D: ?Sized> Deref for Ref<'_, D> {
    type Target = D;
    #[inline]
    fn deref(&self) -> &D {
        unsafe { self.1.as_ref() }
    }
}

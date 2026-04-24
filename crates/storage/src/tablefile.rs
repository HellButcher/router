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

use memmap2::{Advice, MmapMut, MmapOptions, MmapRaw, RemapOptions};

use crate::pod::{self, IndexInfo, SupportsIndex, TableDataHeader as _};

pub type Result<T, E = io::Error> = std::result::Result<T, E>;

pub trait TableData: pod::TablePod {
    type Header: pod::TableDataHeader;
}

// ── index support ─────────────────────────────────────────────────────────────

/// Target size of the sparse index key array (~4 MB).
const TARGET_INDEX_BYTES: usize = 4 * 1024 * 1024;

/// In-memory representation of the sparse lookup index embedded in the file.
struct TableIndex {
    /// First key of every block of `entries_per_block` data entries.
    keys: Box<[u64]>,
    entries_per_block: usize,
    /// True number of data entries; the file is larger due to the index section.
    num_data_entries: usize,
}

// ── TableFile ─────────────────────────────────────────────────────────────────

#[derive(Default)]
struct AppenderData {
    appended: AtomicBool,
}

pub struct TableFile<D> {
    file: File,
    mmap: RwLock<MmapRaw>,
    appender: Option<Arc<AppenderData>>,
    /// Sparse lookup index, present when [`build_index_sorted`] has been called.
    ///
    /// [`build_index_sorted`]: TableFile::build_index_sorted
    index: Option<TableIndex>,
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

        // Read the header (via a temporary small mapping) to detect an embedded index.
        let index = {
            let header_map = MmapOptions::new()
                .len(Self::HEADER_SIZE)
                .map_raw_read_only(&file)?;
            let header = unsafe { &*(header_map.as_ptr() as *const D::Header) };
            match header.index_info() {
                Some(info) => Some(Self::load_index(&file, file_len, info)?),
                None => None,
            }
        };

        // Validate that the data section is properly aligned.
        let data_bytes = match &index {
            Some(idx) => idx.num_data_entries as u64 * Self::DATA_SIZE as u64,
            None => file_len - Self::HEADER_SIZE as u64,
        };
        if !data_bytes.is_multiple_of(Self::DATA_SIZE as u64) {
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
            index,
            _phantom: PhantomData,
        })
    }

    /// Read and copy the sparse key array from `file` into memory given validated
    /// header metadata.  Also verifies the expected file length.
    fn load_index(file: &File, file_len: u64, info: IndexInfo) -> Result<TableIndex> {
        let n = info.num_data_entries as usize;
        let x = info.entries_per_block as usize;
        let y = info.num_index_entries as usize;

        let data_bytes = n as u64 * Self::DATA_SIZE as u64;
        let index_start = (Self::HEADER_SIZE as u64 + data_bytes).next_multiple_of(512);
        let expected_len = index_start + y as u64 * 8;

        if expected_len != file_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Index size in header does not match file size",
            ));
        }

        // Copy the key array into memory (~4 MB).
        let key_map = MmapOptions::new()
            .offset(index_start)
            .len(y * 8)
            .map_raw_read_only(file)?;
        let keys: Box<[u64]> =
            unsafe { std::slice::from_raw_parts(key_map.as_ptr() as *const u64, y) }.into();

        Ok(TableIndex {
            keys,
            entries_per_block: x,
            num_data_entries: n,
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

    /// Pre-allocate a fresh file for exactly `count` entries, call `fill` with
    /// the zeroed data slice so the caller can write entries (e.g. via rayon
    /// `par_iter_mut`), flush, and return the `TableFile`.
    pub fn create_with_capacity<P: AsRef<Path>>(
        path: P,
        count: usize,
        fill: impl FnOnce(&mut [D]),
    ) -> Result<Self>
    where
        D::Header: Default,
    {
        Self::create_with_capacity_and(path, count, Default::default, fill)
    }

    pub fn create_with_capacity_and<P: AsRef<Path>>(
        path: P,
        count: usize,
        init_header: impl FnOnce() -> D::Header,
        fill: impl FnOnce(&mut [D]),
    ) -> Result<Self> {
        let file_size = Self::HEADER_SIZE + count * Self::DATA_SIZE;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(file_size as u64)?;

        {
            let mut header = init_header();
            let header_bytes = pod::as_bytes_mut(std::slice::from_mut(&mut header));
            let mut mmap = unsafe { MmapMut::map_mut(&file) }?;
            mmap[..header_bytes.len()].copy_from_slice(header_bytes);

            if count > 0 {
                let mut data_mmap = unsafe {
                    MmapOptions::new()
                        .offset(Self::HEADER_SIZE as u64)
                        .len(count * Self::DATA_SIZE)
                        .map_mut(&file)?
                };
                let slice = unsafe {
                    std::slice::from_raw_parts_mut(data_mmap.as_mut_ptr() as *mut D, count)
                };
                fill(slice);
                data_mmap.flush()?;
            }
        }

        // new_intern re-reads the header; file is already correctly sized.
        Self::new_intern(file, false, || unreachable!())
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

    /// Number of data entries.  When an index is present the mmap is larger
    /// than the data section, so we use the count stored in the index instead
    /// of deriving it from the file size.
    #[inline]
    pub fn len(&self) -> usize {
        if let Some(idx) = &self.index {
            return idx.num_data_entries;
        }
        let mmap = self.get_mmap_grow_full().unwrap();
        (mmap.len() - Self::HEADER_SIZE) / Self::DATA_SIZE
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
        let len = if let Some(idx) = &self.index {
            idx.num_data_entries
        } else {
            (mmap.len() - Self::HEADER_SIZE) / Self::DATA_SIZE
        };
        let ptr: NonNull<D> =
            unsafe { NonNull::new(mmap.as_ptr().add(Self::HEADER_SIZE) as _).unwrap() };
        let slice = NonNull::slice_from_raw_parts(ptr, len);
        Ok(Ref(mmap, slice))
    }

    pub fn get_all_mut(&mut self) -> Result<&mut [D]> {
        let mmap = self.mmap.get_mut().unwrap_or_else(PoisonError::into_inner);
        let full_len = self.file.metadata()?.len();
        Self::grow_mmap(mmap, full_len)?;
        let len = if let Some(idx) = &self.index {
            idx.num_data_entries
        } else {
            (mmap.len() - Self::HEADER_SIZE) / Self::DATA_SIZE
        };
        unsafe {
            let ptr: *mut D = mmap.as_ptr().add(Self::HEADER_SIZE) as _;
            Ok(std::slice::from_raw_parts_mut(ptr, len))
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

// ── find (requires IndexKey) ──────────────────────────────────────────────────

impl<D: TableData + pod::Item> TableFile<D> {
    /// Binary-search for an item by its key.
    ///
    /// When a sparse index is present (built via [`build_index_sorted`]) the
    /// search first narrows to a block of `entries_per_block` entries using the
    /// in-memory key array, then binary-searches within that block.  Without an
    /// index the full table is searched.
    ///
    /// Requires the table to be sorted by key, which is guaranteed after import.
    ///
    /// [`build_index_sorted`]: TableFile::build_index_sorted
    pub fn find(&self, key: u64) -> Result<Option<(usize, Ref<'_, D>)>> {
        let all = self.get_all()?;
        let range = if let Some(idx) = &self.index {
            // Find the last block whose first key is ≤ key.
            let block = match idx.keys.binary_search(&key) {
                Ok(i) => i,
                Err(i) => i.saturating_sub(1),
            };
            let start = block * idx.entries_per_block;
            let end = ((block + 1) * idx.entries_per_block).min(all.len());
            start..end
        } else {
            0..all.len()
        };
        let result = all[range.clone()].binary_search_by_key(&key, |d| d.key());
        drop(all);
        match result {
            Ok(rel) => Ok(Some((range.start + rel, self.get(range.start + rel)?))),
            Err(_) => Ok(None),
        }
    }
}

// ── build_index_sorted (requires SupportsIndex) ───────────────────────────────

impl<D: TableData + pod::Item> TableFile<D>
where
    D::Header: SupportsIndex,
{
    /// Build the sparse lookup index and embed its metadata in the file header.
    ///
    /// Must be called after all appenders have finished and before the file is
    /// used for routing.  The index key array (`~4 MB`) is appended after a
    /// 512-byte-aligned gap following the data section; the metadata
    /// (`num_data_entries`, `entries_per_block`, `num_index_entries`) is written
    /// into the file header via the existing mmap.
    pub fn build_index_sorted(&mut self) -> Result<()> {
        self.check_no_appender()?;

        let n = self.len();
        if n == 0 {
            return Ok(());
        }

        // Choose X (entries per block) and Y (number of index entries) so that
        // the key array is approximately TARGET_INDEX_BYTES.
        let target_y = (TARGET_INDEX_BYTES / 8).max(1);
        let x = n.div_ceil(target_y.min(n)); // entries per block
        let y = n.div_ceil(x); // actual number of index entries

        // Skip the index when the table is small enough that it would consist
        // of only a single block — binary search over the full table is just
        // as fast and the extra file section would waste space.
        if y <= 1 {
            return Ok(());
        }

        // Collect the first key of every X-th block.
        let keys: Vec<u64> = {
            let all = self.get_all()?;
            (0..n).step_by(x).map(|i| all[i].key()).collect()
        };
        debug_assert_eq!(keys.len(), y);

        // Append the key array after padding the data section to 512 bytes.
        let data_bytes = n as u64 * Self::DATA_SIZE as u64;
        let index_start = (Self::HEADER_SIZE as u64 + data_bytes).next_multiple_of(512);
        self.file.seek(SeekFrom::Start(index_start))?;
        self.file
            .write_all(unsafe { slice_as_bytes(keys.as_slice()) })?;
        self.file.set_len(index_start + y as u64 * 8)?;

        // Write index metadata into the header through the mmap.
        self.header_mut()?.set_index_info(IndexInfo {
            num_data_entries: n as u64,
            entries_per_block: x as u64,
            num_index_entries: y as u64,
        });
        self.flush()?;

        // Remap to include the index section.
        let new_file_len = index_start + y as u64 * 8;
        {
            let mut mmap = self.mmap.write().unwrap_or_else(PoisonError::into_inner);
            Self::grow_mmap(&mut mmap, new_file_len)?;
        }

        self.index = Some(TableIndex {
            keys: keys.into_boxed_slice(),
            entries_per_block: x,
            num_data_entries: n,
        });

        Ok(())
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

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

// ── Appender / AppenderJob ────────────────────────────────────────────────────

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

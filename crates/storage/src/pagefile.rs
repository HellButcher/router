pub const PAGE_SIZE: usize = 4096;

use std::fs::File;
use std::io::{Seek, Write};
use std::path::Path;
use std::{fs::OpenOptions, io};

use memmap2::{Mmap, MmapMut, MmapOptions};

pub struct PageFile {
    file: File,
    mmap: Mmap,
}

pub struct PageFileMut {
    file: File,
    mmap: Option<MmapMut>,
    len: usize,
}

impl PageFile {
    pub const PAGE_SIZE: usize = PAGE_SIZE;

    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::new(OpenOptions::new().read(true).open(path)?)
    }

    pub fn new(file: File) -> io::Result<Self> {
        let file_len = file.metadata()?.len();
        if file_len < Self::PAGE_SIZE as u64 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        } else if file_len > isize::MAX as u64 {
            //TODO: nightly: return Err(io::ErrorKind::FileTooLarge.into());
            return Err(io::ErrorKind::OutOfMemory.into());
        }
        let mmap = unsafe { MmapOptions::new().len(file_len as usize).map(&file) }?;
        Ok(Self { file, mmap })
    }

    pub fn len(&self) -> usize {
        self.mmap.len() / Self::PAGE_SIZE
    }

    pub fn get(&self, page: usize) -> Option<&[u8]> {
        let offset = page * Self::PAGE_SIZE;
        if self.mmap.len() < offset + Self::PAGE_SIZE {
            None
        } else {
            Some(&self.mmap[offset..offset + Self::PAGE_SIZE])
        }
    }
}

impl PageFileMut {
    pub const PAGE_SIZE: usize = PAGE_SIZE;

    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::new(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)?,
        )
    }

    pub fn new(mut file: File) -> io::Result<Self> {
        let file_len = file.metadata()?.len();
        file.seek(io::SeekFrom::End(0))?;
        if file_len == 0 {
            return Ok(Self {
                file,
                mmap: None,
                len: 0,
            });
        } else if file_len < Self::PAGE_SIZE as u64 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        } else if file_len > isize::MAX as u64 {
            //TODO: nightly: return Err(io::ErrorKind::FileTooLarge.into());
            return Err(io::ErrorKind::OutOfMemory.into());
        }
        let mmap = unsafe { MmapOptions::new().len(file_len as usize).map_mut(&file) }?;
        Ok(Self {
            file,
            mmap: Some(mmap),
            len: file_len as usize / Self::PAGE_SIZE,
        })
    }

    pub fn flush(&self) -> io::Result<()> {
        if let Some(m) = self.mmap.as_ref() {
            m.flush()?
        }
        Ok(())
    }

    pub fn make_read_only(self) -> io::Result<PageFile> {
        let Self {
            file,
            mut mmap,
            len,
        } = self;
        if let Some(mmap) = mmap.take() {
            if len * Self::PAGE_SIZE > mmap.len() {
                drop(mmap);
                PageFile::new(file)
            } else {
                let mmap = mmap.make_read_only()?;
                Ok(PageFile { file, mmap })
            }
        } else {
            PageFile::new(file)
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn get_mut(&mut self, page: usize) -> io::Result<&mut [u8]> {
        if page >= self.len {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let mmap_len = if let Some(mmap) = &self.mmap {
            mmap.len()
        } else {
            0
        };

        let offset = page * Self::PAGE_SIZE;

        if mmap_len < offset + Self::PAGE_SIZE {
            drop(self.mmap.take());
            let mmap = unsafe {
                MmapOptions::new()
                    .len(self.len * PAGE_SIZE)
                    .map_mut(&self.file)
            }?;
            self.mmap = Some(mmap);
        }

        let mmap = self.mmap.as_mut().unwrap();
        Ok(&mut mmap[offset..offset + Self::PAGE_SIZE])
    }

    pub fn write(&mut self, page: &[u8; Self::PAGE_SIZE]) -> io::Result<()> {
        self.file.write_all(page)?;
        self.len += 1;
        Ok(())
    }
}

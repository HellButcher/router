use std::{
    fs::{File, OpenOptions},
    io,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    path::Path,
};

mod write;

use crate::{pagefile::{PageFile, PageFileMut, PAGE_SIZE}, pod::TablePod};
const HEADER_SIZE: usize = std::mem::size_of::<Header>();

const MAGIC_WORD: u32 = 0x698d4cd4;
const FLAG_LEAF: u16 = 0x01;

pub struct BTreeBase<K, V>(PhantomData<(K, V)>);

pub struct BTree<K, V> {
    base: BTreeBase<K, V>,
    pages: PageFile,
}

pub struct BTreeMut<K, V> {
    base: BTreeBase<K, V>,
    pages: PageFileMut,
}

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(C)]
struct Header {
    magic: u32,
    flags: u16,
    num_entries: u16,
}

unsafe impl TablePod for Header {}

impl<K, V> BTreeBase<K, V>
where
    K: Sized,
    V: Sized,
{
    const KEY_SIZE: usize = std::mem::size_of::<K>();
    const VALUE_SIZE: usize = std::mem::size_of::<V>();
    const MAX_ITEMS_PER_LEAF: usize =
        (PAGE_SIZE - HEADER_SIZE) / (Self::KEY_SIZE + Self::VALUE_SIZE);
}

impl<K, V> BTree<K, V> {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::new(OpenOptions::new().read(true).open(path)?)
    }

    pub fn new(file: File) -> io::Result<Self> {
        let pages = PageFile::new(file)?;

        let header = pages.get(0).ok_or(io::ErrorKind::UnexpectedEof)?;
        let header = Header::ref_from_bytes(header).unwrap();
        if header.magic != MAGIC_WORD {
            return Err(io::ErrorKind::InvalidData.into());
        }

        Ok(BTree {
            base: BTreeBase(PhantomData),
            pages,
        })
    }
}

impl<K, V> Deref for BTree<K, V> {
    type Target = BTreeBase<K, V>;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<K, V> BTreeMut<K, V> {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::new(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)?,
        )
    }

    pub fn new(file: File) -> io::Result<Self> {
        let mut pages = PageFileMut::new(file)?;

        if pages.len() == 0 {
            let mut page = [0u8; PageFileMut::PAGE_SIZE];
            let header =
                Header::ref_from_bytes_mut(&mut page[..std::mem::size_of::<Header>()]).unwrap();
            header.magic = MAGIC_WORD;
            pages.write(&page)?;
        } else {
            let page = pages.get_mut(0)?;
            let header = Header::ref_from_bytes(&page[..std::mem::size_of::<Header>()]).unwrap();
            if header.magic != MAGIC_WORD {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }

        Ok(BTreeMut {
            base: BTreeBase(PhantomData),
            pages,
        })
    }
}

impl<K, V> Deref for BTreeMut<K, V> {
    type Target = BTreeBase<K, V>;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<K, V> DerefMut for BTreeMut<K, V> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

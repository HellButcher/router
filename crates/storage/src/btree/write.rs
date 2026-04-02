use crossbeam_channel::{bounded, Sender};
use std::{collections::BTreeMap, thread::JoinHandle};

use crate::pagefile::PageFileMut;

use super::{BTreeBase, BTreeMut};

impl<K, V> BTreeMut<K, V>
where
    K: Ord + Sized + Send + 'static,
    V: Sized + Send + 'static,
{
    pub fn insert(mut self) -> (JoinHandle<Self>, BTreeWriter<K, V>) {
        let (sender, receiver) = bounded(128);
        let join = std::thread::spawn(move || {
            let page = [0u8; PageFileMut::PAGE_SIZE];
            self.pages.write(&page).unwrap();
            while let Ok(_items) = receiver.recv() {
                todo!();
            }
            self
        });
        let writer = BTreeWriter {
            sender,
            map: BTreeMap::new(),
        };
        (join, writer)
    }
}

#[derive(Clone)]
pub struct BTreeWriter<K, V> {
    sender: Sender<BTreeMap<K, V>>,
    map: BTreeMap<K, V>,
}

impl<K, V> BTreeWriter<K, V>
where
    K: Ord + Sized,
    V: Sized,
{
    const MAX_BUFFERED_ITEMS: usize = BTreeBase::<K, V>::MAX_ITEMS_PER_LEAF * 32;

    pub fn insert(&mut self, key: K, value: V) {
        self.map.insert(key, value);
        if self.map.len() >= Self::MAX_BUFFERED_ITEMS {
            let _ = self.sender.send(std::mem::take(&mut self.map));
        }
    }
}

impl<K, V> Drop for BTreeWriter<K, V> {
    fn drop(&mut self) {
        if !self.map.is_empty() {
            let _ = self.sender.send(std::mem::take(&mut self.map));
        }
    }
}

impl<K: Ord, V> Extend<(K, V)> for BTreeWriter<K, V> {
    #[inline]
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        iter.into_iter().for_each(move |(k, v)| {
            self.insert(k, v);
        });
    }
}

impl<'a, K: Ord + Copy, V: Copy> Extend<(&'a K, &'a V)> for BTreeWriter<K, V> {
    fn extend<I: IntoIterator<Item = (&'a K, &'a V)>>(&mut self, iter: I) {
        self.extend(iter.into_iter().map(|(&key, &value)| (key, value)));
    }
}

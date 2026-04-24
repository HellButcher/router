pub mod data;
pub(crate) mod extsort;
pub mod idindex;
pub mod morton;
mod pagefile;
pub mod spatial;
pub mod tablefile;

mod pod {
    unsafe impl<T: bytemuck::AnyBitPattern> TablePod for T {}

    /// # Safety
    ///
    /// Similar to bytemuck::AnyBitPattern, but we allow to use atomics.
    /// * The type must be inhabited (eg: no Infallible).
    /// * The type must be valid for any bit pattern of its backing memory.
    /// * It is disallowed for types to contain pointer types, Cell, UnsafeCell, and any other forms of interior mutability, execpt via atomics.
    pub unsafe trait TablePod {}

    /// An item stored in a [`TableFile`](crate::tablefile::TableFile) that has
    /// a `u64` sort key.
    ///
    /// Using `u64` directly (rather than a generic associated type) keeps the
    /// trait simple and makes the sparse lookup index work without a separate
    /// conversion trait.  Keys must uniquely identify items and must be stored
    /// in ascending order in the file.
    pub trait Item: TablePod {
        fn key(&self) -> u64;
    }

    /// Metadata written into the header when an index has been built.
    pub struct IndexInfo {
        pub num_data_entries: u64,
        pub entries_per_block: u64,
        pub num_index_entries: u64,
    }

    /// Trait that all table headers must satisfy.
    ///
    /// The default implementations return `None` / do nothing, so plain headers
    /// such as [`SimpleHeader`](crate::data::SimpleHeader) get index-unaware
    /// behaviour for free.
    pub trait TableDataHeader: TablePod {
        fn index_info(&self) -> Option<IndexInfo> {
            None
        }
    }

    /// Marker: this header type stores index metadata persistently.
    ///
    /// Implement alongside [`TableDataHeader`] only for header types that have
    /// dedicated index fields (i.e. [`HeaderWithIndex`](crate::data::HeaderWithIndex)).
    /// Gates [`TableFile::build_index_sorted`](crate::tablefile::TableFile::build_index_sorted).
    pub trait SupportsIndex: TableDataHeader {
        fn set_index_info(&mut self, info: IndexInfo);
    }

    // there is no as_bytes (not mut) version, because we we allow atomics, so we can't garantee,
    // that the value doesn't change.
    pub(crate) fn as_bytes_mut<I: TablePod>(items: &mut [I]) -> &mut [u8] {
        assert_valid_table_pod::<I>();
        let len_bytes = std::mem::size_of_val(items);
        unsafe { std::slice::from_raw_parts_mut(items.as_mut_ptr() as _, len_bytes) }
    }

    pub(crate) const fn assert_valid_table_pod<I: TablePod>() {
        assert!(
            std::mem::size_of::<I>() > 0,
            "TablePod must not be zero-sized"
        );
        assert!(
            std::mem::size_of::<I>().is_multiple_of(std::mem::align_of::<I>()),
            "TablePod must not have padding"
        );
    }
}

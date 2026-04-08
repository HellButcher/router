pub mod data;
pub(crate) mod extsort;
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

    pub trait Item: TablePod {
        type Key: Ord;

        fn key(&self) -> &Self::Key;
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

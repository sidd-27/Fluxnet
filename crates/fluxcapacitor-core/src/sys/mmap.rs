use std::ptr::NonNull;
use crate::sys::socket::munmap;
use std::io;

/// A safe wrapper around an mmap'd region.
/// Implements Drop to automatically unmap the memory.
pub struct MmapArea {
    ptr: NonNull<u8>,
    len: usize,
}

unsafe impl Send for MmapArea {}
unsafe impl Sync for MmapArea {}

impl MmapArea {
    /// Create a new MmapArea from a raw pointer and length.
    /// SAFETY: The pointer must be a valid mmap'd region of `len` bytes.
    /// The caller transfers ownership of the mapping to this struct.
    pub unsafe fn from_raw(ptr: *mut u8, len: usize) -> Self {
        Self {
            ptr: NonNull::new(ptr).expect("mmap returned null"),
            len,
        }
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

impl Drop for MmapArea {
    fn drop(&mut self) {
        unsafe {
            let _ = munmap(self.ptr.as_ptr(), self.len);
        }
    }
}

use fluxnet_core::umem::mmap::UmemRegion;
use std::slice;
use std::sync::Arc;
// Note: In a real implementation, we need a way to return the frame to the Fill Ring on Drop.
// This usually requires a shared handle to the ProducerRing.

// For now, we will define the struct layout. 
// Fully implementing Drop requires the System/Engine plumbing to be in place.

use crate::system::shared::SharedFrameState;

pub struct Packet {
    pub(crate) addr: u64,
    pub(crate) len: usize,
    // We need a reference to the UMEM region to access data.
    umem: Arc<UmemRegion>,
    
    // Shared state for recycling frames on Drop
    shared_state: Arc<SharedFrameState>,
}

unsafe impl Send for Packet {}
unsafe impl Sync for Packet {}

impl Packet {
    pub(crate) fn new(addr: u64, len: usize, umem: Arc<UmemRegion>, shared_state: Arc<SharedFrameState>) -> Self {
        Self {
            addr,
            len,
            umem,
            shared_state,
        }
    }
    
    pub fn data(&self) -> &[u8] {
        unsafe {
             let ptr = self.umem.as_ptr().add(self.addr as usize);
             slice::from_raw_parts(ptr, self.len)
        }
    }
    
    pub fn data_mut(&mut self) -> &mut [u8] {
        unsafe {
             let ptr = self.umem.as_ptr().add(self.addr as usize);
             slice::from_raw_parts_mut(ptr, self.len)
        }
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        self.shared_state.recycle(self.addr);
    }
}

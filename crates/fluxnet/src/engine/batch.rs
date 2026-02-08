use crate::packet::{PacketRef, Action};
use fluxnet_core::ring::XDPDesc;
use fluxnet_core::umem::mmap::UmemRegion;

pub struct PacketBatch<'a> {
    descriptors: &'a mut [XDPDesc],
    umem: &'a mut UmemRegion,
    actions: &'a mut [Action],
}

impl<'a> PacketBatch<'a> {
    pub(crate) fn new(descriptors: &'a mut [XDPDesc], umem: &'a mut UmemRegion, actions: &'a mut [Action]) -> Self {
        // Initialize all actions to Drop by default (safe default)
        actions.fill(Action::Drop);
        
        Self {
            descriptors,
            umem,
            actions,
        }
    }
    
    pub fn iter_mut(&mut self) -> BatchIterator<'_> {
        BatchIterator {
            descriptors: self.descriptors,
            umem: self.umem,
            actions: self.actions,
            idx: 0,
        }
    }
}

pub struct BatchIterator<'a> {
    descriptors: &'a [XDPDesc],
    umem: &'a UmemRegion, // Umem is thread-safe/shared usually, or at least we only need read access for ptr
    actions: &'a mut [Action],
    idx: usize,
}

impl<'a> Iterator for BatchIterator<'a> {
    type Item = PacketRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.descriptors.len() {
            return None;
        }

        let desc = self.descriptors[self.idx];
        
        let ptr = unsafe {
            self.umem.as_ptr().add(desc.addr as usize)
        };
        
        // Unsafe cast to extend lifetime of Action mutable reference
        // We are iterating disjoint indices, so this is sound.
        let action_ref = unsafe {
            let action_ptr = &mut self.actions[self.idx] as *mut Action;
            &mut *action_ptr
        };
        
        let packet = unsafe {
             PacketRef::new(ptr, desc.len as usize, desc.addr, action_ref)
        };
        
        self.idx += 1;
        Some(packet)
    }
}


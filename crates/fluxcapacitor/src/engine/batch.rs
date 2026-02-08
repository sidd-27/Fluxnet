use crate::packet::{PacketRef, Action};
use fluxcapacitor_core::ring::XDPDesc;
use fluxcapacitor_core::umem::mmap::UmemRegion;

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

#[cfg(test)]
mod tests {
    use super::*;
    use fluxcapacitor_core::umem::layout::UmemLayout;
    use fluxcapacitor_core::umem::mmap::UmemRegion;

    #[test]
    fn test_packet_batch_iteration() {
        // 1. Setup Umem
        let layout = UmemLayout::new(2048, 16);
        let mut umem = UmemRegion::new(layout).expect("Failed to create umem");
        
        // 2. Setup Descriptors
        // We'll create 3 descriptors
        let mut descriptors = vec![
            XDPDesc { addr: 0, len: 100, options: 0 },
            XDPDesc { addr: 2048, len: 50, options: 0 },
            XDPDesc { addr: 4096, len: 200, options: 0 },
        ];

        // 3. Setup Actions
        let mut actions = vec![Action::Drop; 3];

        // 4. Create Batch
        let mut batch = PacketBatch::new(&mut descriptors, &mut umem, &mut actions);

        // 5. Verify Iteration
        let mut count = 0;
        for (i, packet) in batch.iter_mut().enumerate() {
            count += 1;
            // Verify packet properties match descriptor
            let expected_len = match i {
                0 => 100,
                1 => 50,
                2 => 200,
                _ => 0,
            };
            assert_eq!(packet.len(), expected_len);
        }
        assert_eq!(count, 3);
        
        // Verify actions were reset to Drop
        for action in actions {
            assert_eq!(action, Action::Drop);
        }
    }

    #[test]
    fn test_empty_batch() {
        let layout = UmemLayout::new(2048, 16);
        let mut umem = UmemRegion::new(layout).expect("Failed to create umem");
        let mut descriptors = vec![];
        let mut actions = vec![];

        let mut batch = PacketBatch::new(&mut descriptors, &mut umem, &mut actions);
        assert_eq!(batch.iter_mut().count(), 0);
    }
}


use std::collections::VecDeque;
use crate::umem::layout::UmemLayout;

pub struct UmemAllocator {
    free_frames: VecDeque<u64>,
    layout: UmemLayout,
}

impl UmemAllocator {
    pub fn new(layout: UmemLayout) -> Self {
        let mut free_frames = VecDeque::with_capacity(layout.frame_count as usize);
        for i in 0..layout.frame_count {
            if let Some(addr) = layout.idx_to_addr(i) {
                free_frames.push_back(addr);
            }
        }

        Self {
            free_frames,
            layout,
        }
    }

    pub fn allocate(&mut self) -> Option<u64> {
        self.free_frames.pop_front()
    }

    pub fn release(&mut self, addr: u64) {
        // Basic validation could happen here
        self.free_frames.push_back(addr);
    }
    
    pub fn available(&self) -> usize {
        self.free_frames.len()
    }
}

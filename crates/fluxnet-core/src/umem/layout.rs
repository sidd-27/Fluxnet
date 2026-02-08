use std::num::NonZeroU64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmemLayout {
    pub frame_size: u32,
    pub frame_count: u32,
}

impl UmemLayout {
    pub fn new(frame_size: u32, frame_count: u32) -> Self {
        // Validation: frame_size must be power of 2 (usually 2048 or 4096)
        assert!(frame_size.is_power_of_two(), "Frame size must be power of 2");
        assert!(frame_size >= 2048, "Frame size must be at least 2048");

        Self {
            frame_size,
            frame_count,
        }
    }

    pub fn size(&self) -> usize {
        (self.frame_size as usize) * (self.frame_count as usize)
    }

    #[inline]
    pub fn addr_to_idx(&self, addr: u64) -> Option<u32> {
        if addr >= (self.size() as u64) {
            return None;
        }
        Some((addr / self.frame_size as u64) as u32)
    }

    #[inline]
    pub fn idx_to_addr(&self, idx: u32) -> Option<u64> {
        if idx >= self.frame_count {
            return None;
        }
        Some((idx as u64) * (self.frame_size as u64))
    }
}

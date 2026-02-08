use std::sync::atomic::{AtomicU32, Ordering};
use std::ptr;

pub struct ProducerRing<T> {
    producer: *mut AtomicU32,
    consumer: *const AtomicU32,
    descriptors: *mut T,
    mask: u32,
    size: u32,
    cached_consumer: u32,
}

unsafe impl<T> Send for ProducerRing<T> {}

impl<T: Copy> ProducerRing<T> {
    /// # Safety
    /// Pointers must be valid and mapped from the kernel
    pub unsafe fn new(
        producer: *mut u32,
        consumer: *mut u32,
        descriptors: *mut T,
        size: u32,
    ) -> Self {
        Self {
            producer: producer as *mut AtomicU32,
            consumer: consumer as *const AtomicU32,
            descriptors,
            mask: size - 1,
            size,
            cached_consumer: 0,
        }
    }

    #[inline]
    pub fn available(&self) -> u32 {
        let producer_idx = unsafe { (*self.producer).load(Ordering::Relaxed) };
        let consumer_idx = unsafe { (*self.consumer).load(Ordering::Acquire) };
        self.size - (producer_idx.wrapping_sub(consumer_idx))
    }

    #[inline]
    pub fn len(&self) -> u32 {
        self.size
    }

    #[inline]
    pub fn reserve(&mut self, count: u32) -> Option<u32> {
        let producer_idx = unsafe { (*self.producer).load(Ordering::Relaxed) };
        let consumer_idx = unsafe { (*self.consumer).load(Ordering::Acquire) };
        
        let available = self.size - (producer_idx.wrapping_sub(consumer_idx));
        
        if available < count {
            return None;
        }
        
        Some(producer_idx)
    }

    #[inline]
    pub fn submit(&mut self, idx: u32) {
         unsafe { (*self.producer).store(idx, Ordering::Release) };
    }

    #[inline]
    pub unsafe fn write_at(&mut self, idx: u32, item: T) {
         let offset = (idx & self.mask) as usize;
         ptr::write(self.descriptors.add(offset), item);
    }
}

use std::sync::atomic::{AtomicU32, Ordering};
use std::ptr;

pub struct ConsumerRing<T> {
    producer: *const AtomicU32,
    consumer: *mut AtomicU32,
    descriptors: *const T,
    mask: u32,
    size: u32,
    cached_producer: u32,
}

unsafe impl<T> Send for ConsumerRing<T> {}

impl<T: Copy> ConsumerRing<T> {
    /// # Safety
    /// Pointers must be valid and mapped from the kernel
    pub unsafe fn new(
        producer: *mut u32,
        consumer: *mut u32,
        descriptors: *mut T,
        size: u32,
    ) -> Self {
        Self {
            producer: producer as *const AtomicU32,
            consumer: consumer as *mut AtomicU32,
            descriptors,
            mask: size - 1,
            size,
            cached_producer: 0,
        }
    }

    #[inline]
    pub fn peek(&mut self, count: u32) -> usize {
        let producer_idx = unsafe { (*self.producer).load(Ordering::Acquire) };
        let consumer_idx = unsafe { (*self.consumer).load(Ordering::Relaxed) };
        
        let available = producer_idx - consumer_idx;
        if available == 0 {
             return 0;
        }
        
        std::cmp::min(available as usize, count as usize)
    }

    #[inline]
    pub fn release(&mut self, count: u32) {
        let current = unsafe { (*self.consumer).load(Ordering::Relaxed) };
         unsafe { (*self.consumer).store(current + count, Ordering::Release) };
    }

    #[inline]
    pub unsafe fn read_at(&self, idx: u32) -> T {
         let offset = (idx & self.mask) as usize;
         ptr::read(self.descriptors.add(offset))
    }
    
    #[inline]
    pub fn consumer_idx(&self) -> u32 {
         unsafe { (*self.consumer).load(Ordering::Relaxed) }
    }
}

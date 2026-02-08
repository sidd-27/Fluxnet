pub mod desc;
pub mod producer;
pub mod consumer;

pub use desc::XDPDesc;
pub use producer::ProducerRing;
pub use consumer::ConsumerRing;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_producer_ring_basic_flow() {
        let mut producer_val = 0u32;
        let mut consumer_val = 0u32;
        let mut descriptors = vec![0u64; 4]; // Size 4
        let size = 4;

        let mut ring = unsafe {
            ProducerRing::new(
                &mut producer_val,
                &mut consumer_val,
                descriptors.as_mut_ptr(),
                size,
            )
        };

        // 1. Initial State: Empty
        // producer = 0, consumer = 0. Available = 4.
        
        // 2. Reserve 2 slots
        let idx = ring.reserve(2);
        assert!(idx.is_some());
        assert_eq!(idx.unwrap(), 0);
        
        // Producer index NOT updated yet (only locally cached or returned)
        // Wait, reserve just READS indexes. It returns the current producer index.
        // The user writes to ring, then calls submit.
        
        // 3. Write data
        unsafe {
            ring.write_at(idx.unwrap(), 100);
            ring.write_at(idx.unwrap() + 1, 101);
        }
        
        // 4. Submit
        ring.submit(idx.unwrap() + 2);
        
        // Verify producer index updated
        assert_eq!(producer_val, 2);
        
        // 5. Try to fill remaining 2
        let idx2 = ring.reserve(2);
        assert!(idx2.is_some());
        assert_eq!(idx2.unwrap(), 2);
        
        ring.submit(idx2.unwrap() + 2);
        assert_eq!(producer_val, 4);
        
        // 6. Try to overfill
        let idx3 = ring.reserve(1);
        assert!(idx3.is_none()); // Full (prod=4, cons=0, size=4)
    }

    #[test]
    fn test_consumer_ring_basic_flow() {
        let mut producer_val = 0u32;
        let mut consumer_val = 0u32;
        let mut descriptors = vec![0u64; 4]; 
        let size = 4;

        let mut ring = unsafe {
            ConsumerRing::new(
                &mut producer_val,
                &mut consumer_val,
                descriptors.as_mut_ptr(),
                size,
            )
        };

        // 1. Initially Empty
        assert_eq!(ring.peek(4), 0);

        // 2. Simulate Producer adding 2 items
        producer_val = 2; // Kernel updates this
        
        // 3. Peek
        let count = ring.peek(4);
        assert_eq!(count, 2);
        
        // 4. Read
        // Consumer reads from cached consumer index (initially 0)
        // Actually consumer implementation uses `consumer_val` (shared)
        // `peek` calculates available. 
        // `read_at` uses index passed by user.
        
        let cons_idx = ring.consumer_idx();
        assert_eq!(cons_idx, 0);
        
        // 5. Release
        ring.release(2);
        assert_eq!(consumer_val, 2);
        
        // 6. Peek again (should be empty)
        assert_eq!(ring.peek(4), 0);
    }

    #[test]
    fn test_ring_wrapping() {
        let mut producer_val = u32::MAX - 1; // Near wrap
        let mut consumer_val = u32::MAX - 1;
        let mut descriptors = vec![0u64; 4];
        let size = 4;

        let mut ring = unsafe {
            ProducerRing::new(
                &mut producer_val,
                &mut consumer_val,
                descriptors.as_mut_ptr(),
                size,
            )
        };

        // 1. Reserve 2 slots (should wrap producer index)
        // Available = 4. 
        // prod = MAX-1. 
        // reserve(2) -> returns MAX-1.
        // submit(MAX-1 + 2) -> submit(MAX+1) -> submit(0 wraps to min?) No, u32 wrapping.
        // MAX = 4294967295.
        // MAX + 1 = 0.
        
        let idx = ring.reserve(2);
        assert!(idx.is_some());
        let start_idx = idx.unwrap();
        assert_eq!(start_idx, u32::MAX - 1);
        
        // Write at start_idx (MAX-1) -> offset = (MAX-1) & 3 = 30...010 & 11 = 10 (2)
        // Write at start_idx+1 (MAX) -> offset = MAX & 3 = 11...11 & 11 = 11 (3)
        
        unsafe {
            ring.write_at(start_idx, 10);
            ring.write_at(start_idx.wrapping_add(1), 11);
        }
        
        // Submit (MAX-1 + 2) = (MAX+1) = 0
        let new_prod = start_idx.wrapping_add(2);
        ring.submit(new_prod);
        
        assert_eq!(producer_val, 0); // Wrapped correctly
        
        // 2. Verify Consumer sees it
        // Re-create as ConsumerRing just to test logic against same vars
        let mut cons_ring = unsafe {
            ConsumerRing::new(
                &mut producer_val,
                &mut consumer_val,
                descriptors.as_mut_ptr(),
                size,
            )
        };
        
        // Peek
        // prod = 0. cons = MAX-1.
        // available = prod - cons = 0 - (MAX-1) = 0 - ( -2 signed?) 
        // u32: 0 - (0xFFFFFFFE) = 2. Correct.
        
        let avail = cons_ring.peek(4);
        assert_eq!(avail, 2);
        
        // Release
        cons_ring.release(2);
        // cons = MAX-1 + 2 = 0.
        assert_eq!(consumer_val, 0);
    }
}

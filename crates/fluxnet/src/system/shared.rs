use crossbeam_queue::SegQueue;


/// Shared state between FluxRx (Consumer) and all Packet (Owned) instances.
/// This allows packets dropped in any thread to return their frame indices
/// to the RX thread, which then returns them to the kernel's Fill Ring.
pub(crate) struct SharedFrameState {
    /// Lock-free queue of frame indices that are "free" (dropped by user)
    /// but not yet returned to the kernel.
    pub(crate) free_frames: SegQueue<u64>,
}

impl SharedFrameState {
    pub(crate) fn new() -> Self {
        Self {
            free_frames: SegQueue::new(),
        }
    }

    pub(crate) fn recycle(&self, frame_idx: u64) {
        self.free_frames.push(frame_idx);
    }
}

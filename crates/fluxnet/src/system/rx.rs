use fluxnet_core::sys::mmap::MmapArea;
use fluxnet_core::ring::{ConsumerRing, ProducerRing, XDPDesc};
use fluxnet_core::umem::mmap::UmemRegion;
use std::sync::Arc;
use crate::packet::Packet;
use fluxnet_core::sys::socket::RawFd;
use crate::system::shared::SharedFrameState;

pub struct FluxRx {
    rx: ConsumerRing<XDPDesc>,
    #[allow(dead_code)]
    rx_map: MmapArea,
    fill: ProducerRing<u64>,
    #[allow(dead_code)]
    fill_map: MmapArea,
    umem: Arc<UmemRegion>,
    fd: RawFd,
    shared_state: Arc<SharedFrameState>,
}

unsafe impl Send for FluxRx {}

impl FluxRx {
    pub(crate) fn new(
        rx: ConsumerRing<XDPDesc>, rx_map: MmapArea,
        mut fill: ProducerRing<u64>, fill_map: MmapArea,
        umem: Arc<UmemRegion>, fd: RawFd, shared_state: Arc<SharedFrameState>
    ) -> Self {
        // Initialize Fill Ring with all available frames
        let frame_count = umem.layout().frame_count;
        let frame_size = umem.layout().frame_size;
        
        if let Some(mut prod) = fill.reserve(frame_count) {
             for i in 0..frame_count {
                 let addr = (i * frame_size) as u64;
                 unsafe { fill.write_at(prod, addr) };
                 prod += 1;
             }
             fill.submit(prod);
        }

        Self { rx, rx_map, fill, fill_map, umem, fd, shared_state }
    }
    
    pub fn fd(&self) -> RawFd {
        self.fd
    }
    
    /// Refill the Fill Ring with frames returned by dropped Packets.
    /// This is called automatically by recv(), but can be called manually.
    pub fn refill(&mut self) {
        // We take up to 32 frames at a time to batch updates
        // In a real impl, we'd check ring space first.
        let batch_size = 32;
        let mut count = 0;
        
        let reserve = self.fill.reserve(batch_size);
        if let Some(mut idx) = reserve {
            while count < batch_size {
                 if let Some(frame) = self.shared_state.free_frames.pop() {
                     unsafe { self.fill.write_at(idx, frame) };
                     idx += 1;
                     count += 1;
                 } else {
                     break;
                 }
            }
            if count > 0 {
                self.fill.submit(idx);
            }
        }
    }
    
    pub fn recv(&mut self, max: usize) -> Vec<Packet> {
        // 1. Routine maintenance: put recycled frames back into Fill Ring
        self.refill();
        
        let mut packets = Vec::with_capacity(max);
        
        // 2. Check RX Ring
        let count = self.rx.peek(max as u32);
        if count == 0 {
             return packets;
        }
        
        for i in 0..count {
            let desc = unsafe { self.rx.read_at(self.rx.consumer_idx() + i as u32) };
            
            let packet = Packet::new(
                desc.addr, 
                desc.len as usize, 
                self.umem.clone(), 
                self.shared_state.clone()
            ); 
            packets.push(packet);
        }
        
        self.rx.release(count);
        
        packets
    }
}

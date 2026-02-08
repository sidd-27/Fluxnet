use fluxcapacitor_core::sys::mmap::MmapArea;
use fluxcapacitor_core::ring::{ConsumerRing, ProducerRing, XDPDesc};
use fluxcapacitor_core::umem::mmap::UmemRegion;
use std::sync::Arc;
use crate::packet::Packet;
use fluxcapacitor_core::sys::socket::RawFd;

pub struct FluxTx {
    tx: ProducerRing<XDPDesc>,
    #[allow(dead_code)]
    tx_map: MmapArea,
    comp: ConsumerRing<u64>,
    #[allow(dead_code)]
    comp_map: MmapArea,
    #[allow(dead_code)]
    umem: Arc<UmemRegion>,
    #[allow(dead_code)]
    fd: RawFd,
}

unsafe impl Send for FluxTx {}

impl FluxTx {
    pub(crate) fn new(
        tx: ProducerRing<XDPDesc>, tx_map: MmapArea,
        comp: ConsumerRing<u64>, comp_map: MmapArea,
        umem: Arc<UmemRegion>, fd: RawFd
    ) -> Self {
        Self { tx, tx_map, comp, comp_map, umem, fd }
    }
    
    pub fn send(&mut self, packet: Packet) {
        // 1. Reclaim completed frames
        self.reclaim();
        
        // 2. Put on TX Ring
        if let Some(idx) = self.tx.reserve(1) {
            let desc = XDPDesc {
                addr: packet.addr,
                len: packet.len as u32,
                options: 0,
            };
            
            unsafe { self.tx.write_at(idx, desc) };
            self.tx.submit(idx.wrapping_add(1));
            
            std::mem::forget(packet);
        } else {
            drop(packet); 
        }
    }
    
    pub fn reclaim(&mut self) {
        let n = self.comp.peek(32); // Batch 32
        if n > 0 {
             // Read completed frames
             for i in 0..n {
                 let _addr = unsafe { self.comp.read_at(self.comp.consumer_idx() + i as u32) };
                 // Here we would normally return _addr to the free pool / fill ring.
                 // But FluxTx doesn't have access to Fill Ring! (Rx has it).
                 
                 // Design constraint: "FluxTx: Exclusive owner of Transmit Ring and Completion Ring"
                 // "FluxRx: Exclusive owner of Receive Ring and Fill Ring"
                 
                 // So completed TX frames must be sent back to FluxRx to be put into Fill Ring?
                 // This requires a channel between Tx and Rx.
                 // Or a shared Free List.
             }
             self.comp.release(n as u32);
        }
    }
}

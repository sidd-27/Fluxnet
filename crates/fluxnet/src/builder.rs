use crate::raw::FluxRaw;
use crate::config::Poller;
use crate::engine::FluxEngine;
use fluxnet_core::umem::layout::UmemLayout;
use fluxnet_core::umem::mmap::UmemRegion;
use fluxnet_core::sys::socket::{create_xsk_socket, bind_socket, set_umem_reg, set_ring_size, get_mmap_offsets, mmap_range};
use fluxnet_core::sys::if_xdp::{XDP_UMEM_FILL_RING, XDP_UMEM_COMPLETION_RING, XDP_RX_RING, XDP_TX_RING, XDP_UMEM_PGOFF_FILL_RING, XDP_UMEM_PGOFF_COMPLETION_RING, XDP_PGOFF_RX_RING, XDP_PGOFF_TX_RING};
use fluxnet_core::ring::{ProducerRing, ConsumerRing, XDPDesc};

// Note: Real implementation would need full binding logic here.
// For now we just scaffold the builder.

pub struct FluxBuilder {
    interface: String,
    queue_id: u32,
    frame_count: u32,
    frame_size: u32,
    poller: Poller,
    batch_size: usize,
    bind_flags: u16,
}

impl FluxBuilder {
    pub fn new(interface: &str) -> Self {
        Self {
            interface: interface.to_string(),
            queue_id: 0,
            frame_count: 4096,
            frame_size: 2048,
            poller: Poller::Adaptive,
            batch_size: 64,
            bind_flags: 0,
        }
    }

    pub fn queue_id(mut self, id: u32) -> Self {
        self.queue_id = id;
        self
    }
    
    pub fn bind_flags(mut self, flags: u16) -> Self {
        self.bind_flags = flags;
        self
    }
    
    pub fn umem_pages(mut self, count: u32) -> Self {
        self.frame_count = count;
        self
    }

    pub fn poller(mut self, poller: Poller) -> Self {
        self.poller = poller;
        self
    }

    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    pub fn build_engine(self) -> Result<FluxEngine, std::io::Error> {
        let poller = self.poller;
        let batch_size = self.batch_size;
        let raw = self.build_raw()?;
        Ok(FluxEngine::with_config(raw, batch_size, poller))
    }

    pub fn build_raw(self) -> Result<FluxRaw, std::io::Error> {
        // 1. Create UMEM
        let layout = UmemLayout::new(self.frame_size, self.frame_count);
        let mut umem = UmemRegion::new(layout)?;
        
        // 2. Create Socket
        let fd = create_xsk_socket()?;

        // simulator: link umem to fd so they share same memory
        #[cfg(not(target_os = "linux"))]
        umem.set_fd(fd);
        
        // 3. Register UMEM
        // TODO: Handle headroom properly (currently 0)
        let headroom = 0;
        set_umem_reg(fd, umem.as_ptr() as u64, umem.len() as u64, self.frame_size, headroom)?;
        
        // 4. Set Ring Sizes
        let ring_size = self.frame_count;
        set_ring_size(fd, XDP_UMEM_FILL_RING as i32, ring_size)?;
        set_ring_size(fd, XDP_UMEM_COMPLETION_RING as i32, ring_size)?;
        set_ring_size(fd, XDP_RX_RING as i32, ring_size)?;
        set_ring_size(fd, XDP_TX_RING as i32, ring_size)?;
        
        // 5. Mmap Rings
        let off = get_mmap_offsets(fd)?;
        
        // Fill Ring
        let fill_len = (off.fr.desc + (ring_size as u64) * 8) as usize;
        let fill_ptr = unsafe { mmap_range(fd, fill_len, XDP_UMEM_PGOFF_FILL_RING) }?;
        let fill_map = unsafe { fluxnet_core::sys::mmap::MmapArea::from_raw(fill_ptr, fill_len) };
        let fill = unsafe { ProducerRing::new(
            fill_ptr.add(off.fr.producer as usize) as *mut u32,
            fill_ptr.add(off.fr.consumer as usize) as *mut u32,
            fill_ptr.add(off.fr.desc as usize) as *mut u64,
            ring_size,
        )};
        
        // Completion Ring
        let comp_len = (off.cr.desc + (ring_size as u64) * 8) as usize;
        let comp_ptr = unsafe { mmap_range(fd, comp_len, XDP_UMEM_PGOFF_COMPLETION_RING) }?;
        let comp_map = unsafe { fluxnet_core::sys::mmap::MmapArea::from_raw(comp_ptr, comp_len) };
        let comp = unsafe { ConsumerRing::new(
            comp_ptr.add(off.cr.producer as usize) as *mut u32,
            comp_ptr.add(off.cr.consumer as usize) as *mut u32,
            comp_ptr.add(off.cr.desc as usize) as *mut u64,
            ring_size,
        )};
        
        // RX Ring
        let rx_len = (off.rx.desc + (ring_size as u64) * 16) as usize;
        let rx_ptr = unsafe { mmap_range(fd, rx_len, XDP_PGOFF_RX_RING) }?;
        let rx_map = unsafe { fluxnet_core::sys::mmap::MmapArea::from_raw(rx_ptr, rx_len) };
        let rx = unsafe { ConsumerRing::new(
            rx_ptr.add(off.rx.producer as usize) as *mut u32,
            rx_ptr.add(off.rx.consumer as usize) as *mut u32,
            rx_ptr.add(off.rx.desc as usize) as *mut XDPDesc,
            ring_size,
        )};
        
        // TX Ring
        let tx_len = (off.tx.desc + (ring_size as u64) * 16) as usize;
        let tx_ptr = unsafe { mmap_range(fd, tx_len, XDP_PGOFF_TX_RING) }?;
        let tx_map = unsafe { fluxnet_core::sys::mmap::MmapArea::from_raw(tx_ptr, tx_len) };
        let tx = unsafe { ProducerRing::new(
            tx_ptr.add(off.tx.producer as usize) as *mut u32,
            tx_ptr.add(off.tx.consumer as usize) as *mut u32,
            tx_ptr.add(off.tx.desc as usize) as *mut XDPDesc,
            ring_size,
        )};
        
        // 6. Bind (if interface provided)
        let if_index = fluxnet_core::sys::utils::if_nametoindex(&self.interface)?;
        
        bind_socket(fd, if_index, self.queue_id, self.bind_flags)?;
 
        Ok(FluxRaw::new(
            umem, 
            rx, rx_map, 
            fill, fill_map, 
            tx, tx_map, 
            comp, comp_map, 
            fd
        ))
    }
}
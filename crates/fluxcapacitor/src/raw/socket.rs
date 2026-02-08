use fluxcapacitor_core::sys::mmap::MmapArea;
use fluxcapacitor_core::umem::mmap::UmemRegion;
use fluxcapacitor_core::ring::{ConsumerRing, ProducerRing, XDPDesc};
use fluxcapacitor_core::sys::socket::RawFd;

pub struct FluxRaw {
    pub umem: UmemRegion,
    pub rx: ConsumerRing<XDPDesc>,
    pub rx_map: MmapArea,
    pub fill: ProducerRing<u64>,
    pub fill_map: MmapArea,
    pub tx: ProducerRing<XDPDesc>,
    pub tx_map: MmapArea,
    pub comp: ConsumerRing<u64>,
    pub comp_map: MmapArea,
    fd: RawFd,
    #[cfg(target_os = "linux")]
    pub bpf: Option<aya::Bpf>,
}

impl FluxRaw {
    pub fn new(
        umem: UmemRegion, 
        rx: ConsumerRing<XDPDesc>, rx_map: MmapArea,
        fill: ProducerRing<u64>, fill_map: MmapArea,
        tx: ProducerRing<XDPDesc>, tx_map: MmapArea,
        comp: ConsumerRing<u64>, comp_map: MmapArea,
        fd: RawFd
    ) -> Self {
        Self {
            umem,
            rx, rx_map,
            fill, fill_map,
            tx, tx_map,
            comp, comp_map,
            fd,
            #[cfg(target_os = "linux")]
            bpf: None,
        }
    }
    
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn needs_wakeup_rx(&self) -> bool {
        // TODO: check flags
        false
    }
    
    pub fn wakeup_rx(&self) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
             let _ = fluxcapacitor_core::sys::socket::wait_rx(self.fd, 0)?;
        }
        Ok(())
    }
    
    pub fn needs_wakeup_tx(&self) -> bool {
         // TODO: check flags
         false
    }
    
    pub fn wakeup_tx(&self) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        fluxcapacitor_core::sys::socket::kick_tx(self.fd)?;
        Ok(())
    }

    pub fn debug_rings(&self) {
        println!("--- FluxRaw Ring Debug ---");
        println!("RX Ring:   {}/{}", self.rx.available(), self.rx.len());
        println!("TX Ring:   {}/{}", self.tx.available(), self.tx.len());
        println!("Fill Ring: {}/{}", self.fill.available(), self.fill.len());
        println!("Comp Ring: {}/{}", self.comp.available(), self.comp.len());
    }
}

// Safety: We assert that FluxRaw is safe to send between threads.
// In the simulator, the global socket state is protected by a Mutex.
// The RawFd is just an integer index (cast to pointer).
unsafe impl Send for FluxRaw {}

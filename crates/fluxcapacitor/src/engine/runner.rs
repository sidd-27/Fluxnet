use crate::raw::FluxRaw;
use crate::engine::batch::PacketBatch;
use crate::packet::Action;
use crate::config::Poller;
use fluxcapacitor_core::ring::XDPDesc;
use std::io;
use std::time::{Instant, Duration};

pub struct FluxEngine {
    pub socket: FluxRaw,
    batch_size: usize,
    poller: Poller,
    // Reuse buffers to avoid per-batch allocations
    descs_buf: Vec<XDPDesc>,
    actions_buf: Vec<Action>,
}

impl FluxEngine {
    pub fn new(socket: FluxRaw, batch_size: usize) -> Self {
        Self::with_config(socket, batch_size, Poller::Adaptive)
    }

    pub fn with_config(socket: FluxRaw, batch_size: usize, poller: Poller) -> Self {
        let mut engine = Self {
            socket,
            batch_size: batch_size.max(1),
            poller,
            descs_buf: vec![XDPDesc::default(); batch_size.max(1)],
            actions_buf: vec![Action::Drop; batch_size.max(1)],
        };
        
        // Initialize Fill Ring with all available UMEM frames
        let frame_count = engine.socket.umem.layout().frame_count;
        let frame_size = engine.socket.umem.layout().frame_size;
        
        let to_fill = frame_count; 
        
        if let Some(mut prod) = engine.socket.fill.reserve(to_fill) {
             for i in 0..to_fill {
                 let addr = (i * frame_size) as u64;
                 unsafe { engine.socket.fill.write_at(prod, addr) };
                 prod += 1;
             }
             engine.socket.fill.submit(prod);
        }
        
        engine
    }

    pub fn run<F>(&mut self, stop: &std::sync::atomic::AtomicBool, mut callback: F) -> io::Result<()>
    where
        F: FnMut(&mut PacketBatch),
    {
        match self.poller {
            Poller::Busy => loop {
                if stop.load(std::sync::atomic::Ordering::Relaxed) { break Ok(()); }
                self.process_batch(&mut callback)?;
            },
            Poller::Wait => loop {
                if stop.load(std::sync::atomic::Ordering::Relaxed) { break Ok(()); }
                let count = self.process_batch(&mut callback)?;
                if count == 0 {
                    // Block until next packet.
                    // For now, we use a short sleep to simulate waiting.
                    std::thread::sleep(Duration::from_millis(1));
                }
            },
            Poller::Adaptive => {
                let mut last_packet_time = Instant::now();
                let spin_duration = Duration::from_micros(50);
                
                loop {
                    if stop.load(std::sync::atomic::Ordering::Relaxed) { break Ok(()); }
                    let count = self.process_batch(&mut callback)?;
                    if count > 0 {
                        last_packet_time = Instant::now();
                    } else if last_packet_time.elapsed() > spin_duration {
                        std::thread::sleep(Duration::from_millis(1));
                    } else {
                        std::thread::yield_now();
                    }
                }
            }
        }
    }

    pub fn socket_fd(&self) -> fluxcapacitor_core::sys::socket::RawFd {
        self.socket.fd()
    }

    /// Process a single batch of packets.
    pub fn process_batch<F>(&mut self, callback: &mut F) -> io::Result<usize>
    where
        F: FnMut(&mut PacketBatch),
    {
        // 1. Recycle Completed TX Frames
        {
                let count = self.socket.comp.peek(32);
                if count > 0 {
                    if let Some(mut producer_idx) = self.socket.fill.reserve(count as u32) {
                        for i in 0..count {
                            let addr = unsafe { self.socket.comp.read_at(self.socket.comp.consumer_idx() + i as u32) };
                            unsafe { self.socket.fill.write_at(producer_idx, addr) };
                            producer_idx += 1;
                        }
                        self.socket.fill.submit(producer_idx);
                        self.socket.comp.release(count as u32);
                    } else {
                        self.socket.comp.release(count as u32);
                    }
                }
        }

        // 2. Consume from RX Ring
        let rx_count = {
            let consumer = self.socket.rx.peek(self.batch_size as u32);
            if consumer == 0 {
                if self.socket.needs_wakeup_rx() {
                        let _ = self.socket.wakeup_rx();
                }
                return Ok(0);
            }
            
            let count = consumer;
            for i in 0..count {
                self.descs_buf[i as usize] = unsafe { self.socket.rx.read_at(self.socket.rx.consumer_idx() + i as u32) };
                self.actions_buf[i as usize] = Action::Drop; // Default to drop
            }
            
            self.socket.rx.release(count as u32);
            count
        };

        if rx_count > 0 {
            let active_descs = &mut self.descs_buf[0..rx_count as usize];
            let active_actions = &mut self.actions_buf[0..rx_count as usize];
            
            // 3. User Callback
            {
                let mut batch = PacketBatch::new(active_descs, &mut self.socket.umem, active_actions);
                callback(&mut batch);
            }
            
            // 4. Commit Actions
            let mut tx_needed = 0;
            for a in active_actions.iter() {
                if *a == Action::Tx { tx_needed += 1; }
            }
            
            if tx_needed > 0 {
                if let Some(mut tx_prod) = self.socket.tx.reserve(tx_needed) {
                    for (i, action) in active_actions.iter().enumerate() {
                        if *action == Action::Tx {
                            unsafe { self.socket.tx.write_at(tx_prod, active_descs[i]) };
                            tx_prod += 1;
                        }
                    }
                    self.socket.tx.submit(tx_prod);
                    if self.socket.needs_wakeup_tx() {
                            let _ = self.socket.wakeup_tx();
                    }
                } else {
                    for action in active_actions.iter_mut() {
                        if *action == Action::Tx { *action = Action::Drop; }
                    }
                }
            }
            
            let mut fill_needed = 0;
            for a in active_actions.iter() {
                if *a == Action::Drop { fill_needed += 1; }
            }
            
            if fill_needed > 0 {
                if let Some(mut fill_prod) = self.socket.fill.reserve(fill_needed) {
                        for (i, action) in active_actions.iter().enumerate() {
                        if *action == Action::Drop {
                            unsafe { self.socket.fill.write_at(fill_prod, active_descs[i].addr) };
                            fill_prod += 1;
                        }
                    }
                    self.socket.fill.submit(fill_prod);
                }
            }
        }
        
        Ok(rx_count as usize)
    }
}

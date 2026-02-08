use crate::raw::FluxRaw;
use crate::engine::batch::PacketBatch;
use crate::packet::Action;
use std::io;

pub struct FluxEngine {
    socket: FluxRaw,
    batch_size: usize,
}

impl FluxEngine {
    pub fn new(socket: FluxRaw, batch_size: usize) -> Self {
        let mut engine = Self {
            socket,
            batch_size: batch_size.max(1),
        };
        
        // Initialize Fill Ring with all available UMEM frames
        // This ensures the kernel (or simulator) has buffers to receive packets into.
        let frame_count = engine.socket.umem.layout().frame_count;
        let frame_size = engine.socket.umem.layout().frame_size;
        
        // Reserve space in Fill Ring
        // We try to fill as much as we can, up to frame_count or ring availability.
        // Assuming ring size >= frame_count usually.
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

    pub fn run<F>(&mut self, mut callback: F) -> io::Result<()>
    where
        F: FnMut(&mut PacketBatch),
    {
        loop {
            self.process_batch(&mut callback)?;
        }
    }

    /// Process a single batch of packets.
    /// This is public for testing and advanced usage (e.g. custom poll loops).
    pub fn process_batch<F>(&mut self, callback: &mut F) -> io::Result<usize>
    where
        F: FnMut(&mut PacketBatch),
    {
        let mut descs = vec![Default::default(); self.batch_size];
        let mut actions = vec![Action::Drop; self.batch_size]; // Parallel array for actions
        
        // 1. Recycle Completed TX Frames
        {
                let count = self.socket.comp.peek(32);
                if count > 0 {
                    let fill = self.socket.fill.reserve(count as u32);
                    
                    if let Some(mut producer_idx) = fill {
                        for i in 0..count {
                            // Get completed frame idx
                            let addr = unsafe { self.socket.comp.read_at(self.socket.comp.consumer_idx() + i as u32) };
                            // Push to fill ring for reuse
                            unsafe { self.socket.fill.write_at(producer_idx, addr) };
                            producer_idx += 1;
                        }
                        self.socket.fill.submit(producer_idx);
                        self.socket.comp.release(count as u32);
                    } else {
                        // Fill ring full? Should not happen if size matches.
                        self.socket.comp.release(count as u32);
                    }
                }
        }

        // 2. Consume from RX Ring
        // ... (Reading RX logic)
            let rx_count = {
            let consumer = self.socket.rx.peek(self.batch_size as u32);
            if consumer == 0 {
                if self.socket.needs_wakeup_rx() {
                        let _ = self.socket.wakeup_rx();
                }
                // TODO: Implement proper Poller wait here
                return Ok(0);
            }
            
            let count = consumer;
            for i in 0..count {
                descs[i as usize] = unsafe { self.socket.rx.read_at(self.socket.rx.consumer_idx() + i as u32) };
            }
            
            self.socket.rx.release(count as u32);
            count
        };

        if rx_count > 0 {
            let active_descs = &mut descs[0..rx_count as usize];
            let active_actions = &mut actions[0..rx_count as usize];
            
            // 3. User Callback
            {
                let mut batch = PacketBatch::new(active_descs, &mut self.socket.umem, active_actions);
                callback(&mut batch);
            }
            
            // 4. Commit Actions
            let _tx_count = 0;
            let _fill_count = 0;
            
            // We need to batch-update TX and Fill rings.
            // It's fastest to do two passes or separate them (but order doesn't matter much for different rings).
            
            // Pass 1: TX
            // Filter packets that need TX
            // We need to be careful: if TX ring is full, we must drop instead!
            // For now, assume optimistic TX.
            
            let _maybe_tx_prod = self.socket.tx.reserve(rx_count as u32); // Optimistic: assume all TX? No, wait.
            // We don't know how many TX until we look.
            // But `reserve` needs a count.
            // So we count first.
            
            let mut tx_needed = 0;
            for a in active_actions.iter() {
                if *a == Action::Tx { tx_needed += 1; }
            }
            
            // Reserve TX
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
                    // TX Ring full! Force drop all intended TX
                    for action in active_actions.iter_mut() {
                        if *action == Action::Tx { *action = Action::Drop; }
                    }
                }
            }
            
            // Pass 2: Drop (Fill)
            // Any packet being dropped (or failed TX) goes back to Fill ring.
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

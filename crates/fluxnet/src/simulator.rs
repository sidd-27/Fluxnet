#[cfg(all(feature = "simulator", not(target_os = "linux")))]
use fluxnet_core::windows_stubs::SOCKETS;
#[cfg(all(feature = "simulator", not(target_os = "linux")))]


#[cfg(all(feature = "simulator", not(target_os = "linux")))]
pub mod control {
    use super::*;
    use fluxnet_core::sys::socket::RawFd;
    
    /// Inject a packet into the RX ring of the specified socket.
    /// This mimics a packet arriving from the network card.
    /// 
    /// # Arguments
    /// * `fd` - The socket file descriptor (mocked)
    /// * `data` - The raw packet bytes
    pub fn inject_packet(fd: RawFd, data: &[u8]) -> Result<(), String> {
        let fd_idx = fd as usize;
        let mut sockets = SOCKETS.lock().map_err(|e| e.to_string())?;
        
        let sock = sockets.get_mut(&fd_idx).ok_or("Socket not found")?;
        
        // 1. Get a frame from UMEM (Simulated mechanism)
        // In reality, the user must have put frames in the FILL RING.
        // We need to check the FILL RING to see if user gave us buffers.
        
        // pointers for fill ring
        // Layout: Prod(0), Cons(4)
        let fill_prod_ptr = sock.fill_ring.as_ptr() as *const u32;
        let fill_cons_ptr = unsafe { sock.fill_ring.as_ptr().add(4) } as *mut u32;
        let fill_desc_ptr = unsafe { sock.fill_ring.as_ptr().add(8) } as *const u64; // Fill ring contains u64 addrs
        
        unsafe {
            let fill_prod = *fill_prod_ptr;
            let fill_cons = *fill_cons_ptr;
            
            if fill_cons == fill_prod {
                return Err("RX Dropped: No buffers in Fill Ring".to_string());
            }
            
            // Consume one buffer from Fill Ring
            let mask = 4096 - 1; // Assuming size 4096 for mock
            let idx = fill_cons & mask;
            let addr = *fill_desc_ptr.add(idx as usize);
            
            // Update Fill Consumer
            *fill_cons_ptr = fill_cons + 1;
            
            // 2. Write data to UMEM
            if (addr as usize) + data.len() > sock.umem.len() {
               // Resize UMEM if needed (simple mock behavior)
               // In reality, UMEM is fixed. Mock allows dynamic for ease.
               if (addr as usize) + data.len() > sock.umem.len() {
                   sock.umem.resize((addr as usize) + data.len() + 4096, 0);
               }
            }
            
            // Copy data
            let dest = sock.umem.as_mut_ptr().add(addr as usize);
            std::ptr::copy_nonoverlapping(data.as_ptr(), dest, data.len());
            
            // 3. Publish to RX Ring
            // Layout: Prod(0), Cons(4), Desc(8)
            let rx_prod_ptr = sock.rx_ring.as_mut_ptr() as *mut u32;
            let rx_desc_ptr = sock.rx_ring.as_mut_ptr().add(8) as *mut fluxnet_core::ring::XDPDesc;
            
            let rx_prod = *rx_prod_ptr;
            let rx_idx = rx_prod & mask;
            
            let desc = fluxnet_core::ring::XDPDesc {
                addr,
                len: data.len() as u32,
                options: 0,
            };
            
            *rx_desc_ptr.add(rx_idx as usize) = desc;
            
            // Update RX Producer
            *rx_prod_ptr = rx_prod + 1;
        }
        
        Ok(())
    }
    
    /// Peek at the next packet in the TX ring (sent by the user).
    /// Does NOT consume it (Consumption happens via complete_tx).
    pub fn read_tx_packet(fd: RawFd) -> Result<Vec<u8>, String> {
        let fd_idx = fd as usize;
        let mut sockets = SOCKETS.lock().map_err(|e| e.to_string())?;
        let sock = sockets.get_mut(&fd_idx).ok_or("Socket not found")?;
        
        let tx_prod_ptr = sock.tx_ring.as_ptr() as *const u32;
        let tx_cons_ptr = unsafe { sock.tx_ring.as_ptr().add(4) } as *mut u32; // We simulate kernel consumer
        let tx_desc_ptr = unsafe { sock.tx_ring.as_ptr().add(8) } as *const fluxnet_core::ring::XDPDesc;
        
        unsafe {
            let tx_prod = *tx_prod_ptr;
            let tx_cons = *tx_cons_ptr;
            
            if tx_cons == tx_prod {
                return Err("No packets in TX Ring".to_string());
            }
            
            let mask = 4096 - 1;
            let idx = tx_cons & mask;
            let desc = *tx_desc_ptr.add(idx as usize);
            
            let start = desc.addr as usize;
            let end = start + desc.len as usize;
            
            if end > sock.umem.len() {
                return Err("TX Descriptor out of bounds of UMEM".to_string());
            }
            
            let mut data = vec![0u8; desc.len as usize];
            std::ptr::copy_nonoverlapping(sock.umem.as_ptr().add(start), data.as_mut_ptr(), desc.len as usize);
            
            // Auto-complete the TX (Simulate transmission success)
            *tx_cons_ptr = tx_cons + 1;
            
            // Push to Completion Ring
             let comp_prod_ptr = sock.comp_ring.as_mut_ptr() as *mut u32;
             let comp_desc_ptr = sock.comp_ring.as_mut_ptr().add(8) as *mut u64;
             
             let comp_prod = *comp_prod_ptr;
             let comp_idx = comp_prod & mask;
             
             *comp_desc_ptr.add(comp_idx as usize) = desc.addr;
             *comp_prod_ptr = comp_prod + 1;
             
            Ok(data)
        }
    }
}

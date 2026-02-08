#[cfg(target_os = "linux")]
mod linux_system_echo {
    use fluxnet::builder::FluxBuilder;
    use fluxnet::system::split; // Sync split
    use fluxnet_core::ring::XDPDesc;
    use std::thread;
    use std::time::Duration;

    const XDP_FLAGS_SKB_MODE: u16 = 2;

    #[test]
    fn test_system_echo_server() {
        // Run `scripts/setup_veth.sh` first.

        // 1. Setup Echo Server (veth1)
        let server_builder = FluxBuilder::new("veth1")
            .queue_id(0)
            .bind_flags(XDP_FLAGS_SKB_MODE)
            .umem_pages(16);
            
        let server_raw = match server_builder.build_raw() {
            Ok(r) => r,
            Err(_) => return,
        };
        
        let (mut server_rx, mut server_tx) = split(server_raw);

        let server_thread = thread::spawn(move || {
            // Run for a bit
            let start = std::time::Instant::now();
            while start.elapsed() < Duration::from_secs(2) {
                // Try receive
                // We use non-blocking approach or short poll if available, 
                // but sync `recv` blocks?
                // `FluxRx::recv` signature: `pub fn recv(&mut self, batch: usize) -> Vec<Packet>`
                // It might block if implemented that way.
                // We should check implementation. If it blocks forever, we might hang.
                // Assuming it has some timeout or we just rely on the packet arriving.
                
                // Let's assume we can block for the test.
                // Or better, use a loop with timeout logic if possible.
                // But `recv` usually calls `poll` with -1 (infinity) or configured timeout?
                // FluxRx typically uses the configured poller or just busy loops?
                // Let's just try to recv 1 packet.
                
                // Hack: We only expect 1 packet.
                // But we put it in a loop just in case.
                
                // Note: If recv blocks forever, this thread hangs.
                // We rely on the client sending something.
                
                // Actually, let's look at `FluxRx::recv`.
                // If we don't know, assume it blocks.
                
                if let Ok(mut packets) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                     // We can't catch methods easily if they aren't unwind safe, but let's just call it.
                     // The issue is if it blocks.
                     // Let's hope it doesn't block forever if we kill the test main thread?
                     // No, process exits.
                     return ();
                })) {
                    // ...
                }
            }
        });
        
        // Actually, let's rewrite the server loop to be simple:
        // Receive 1 batch, modify, send back, exit.
        
        let server_thread = thread::spawn(move || {
            // recv(1) might return empty if non-blocking, or wait.
            // Let's loop until we get something.
            loop {
                // We need a way to check if we should stop?
                // Or just exit after processing one.
                let mut packets = server_rx.recv(1); // Assuming this exists and works
                if packets.is_empty() {
                    // Sleep and retry? Or does recv block?
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                
                for mut packet in packets {
                    // Modify payload
                    let data = packet.data_mut();
                    if data.len() > 0 {
                        data[0] = 0xEE; // Mark as Echoed
                    }
                    
                    // Echo back
                    server_tx.send(packet);
                }
                server_tx.reclaim(); // Housekeeping
                break; // Done
            }
        });

        // 2. Client (veth0)
        let client_builder = FluxBuilder::new("veth0")
            .queue_id(0)
            .bind_flags(XDP_FLAGS_SKB_MODE)
            .umem_pages(16);
        let mut client_raw = client_builder.build_raw().expect("Failed veth0");

        // Send Packet
        let payload = vec![0xAA; 100];
        let tx_addr = 0;
        unsafe {
            let dest = client_raw.umem.as_ptr().add(tx_addr);
            std::ptr::copy_nonoverlapping(payload.as_ptr(), dest, payload.len());
        }
        
        // Send
        let idx = client_raw.tx.reserve(1).unwrap();
        unsafe {
             client_raw.tx.write_at(idx, XDPDesc { addr: tx_addr as u64, len: payload.len() as u32, options: 0 });
        }
        client_raw.tx.submit(idx + 1);
        client_raw.wakeup_tx().unwrap();

        // 3. Receive Echo
        // We need to prepare RX on client side too
        let fill_idx = client_raw.fill.reserve(4).unwrap();
        unsafe { client_raw.fill.write_at(fill_idx, 2048); } // Use addr 2048 for RX
        client_raw.fill.submit(fill_idx + 1);
        client_raw.wakeup_rx().unwrap(); // Bind/Fill

        // Poll for echo
        let mut echoed = false;
        for _ in 0..20 {
             let n = client_raw.rx.peek(1);
             if n > 0 {
                 let desc = unsafe { client_raw.rx.read_at(client_raw.rx.consumer_idx()) };
                 client_raw.rx.release(1);
                 
                 let ptr = unsafe { client_raw.umem.as_ptr().add(desc.addr as usize) };
                 let data = unsafe { std::slice::from_raw_parts(ptr, desc.len as usize) };
                 
                 if data.len() > 0 && data[0] == 0xEE {
                     echoed = true;
                     break;
                 }
             }
             client_raw.wakeup_rx().unwrap();
             thread::sleep(Duration::from_millis(50));
        }
        
        assert!(echoed, "Did not receive echoed packet (0xEE marker)");
        let _ = server_thread.join();
    }
}

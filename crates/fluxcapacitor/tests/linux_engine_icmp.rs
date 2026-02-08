#[cfg(target_os = "linux")]
mod linux_engine_icmp {
    use fluxcapacitor::builder::FluxBuilder;
    use fluxcapacitor::engine::FluxEngine;
    use fluxcapacitor_core::ring::XDPDesc;
    use std::thread;
    use std::time::Duration;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    const XDP_FLAGS_SKB_MODE: u16 = 2;

    #[test]
    fn test_engine_icmp_detection() {
        // Run `scripts/setup_veth.sh` first.

        // 1. Setup Engine on veth1
        let rx_builder = FluxBuilder::new("veth1")
            .queue_id(0)
            .bind_flags(XDP_FLAGS_SKB_MODE)
            .umem_pages(16);
            
        let mut engine = match rx_builder.build_engine() {
            Ok(e) => e,
            Err(_) => return, // Skip if no veth
        };

        let found_icmp = Arc::new(AtomicBool::new(false));
        let found_clone = found_icmp.clone();
        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_signal.clone();

        let engine_thread = thread::spawn(move || {
            engine.run(&stop_clone, |batch| {
                for packet in batch.iter_mut() {
                    // Check if it's our ICMP packet
                    // We can use the helper methods if they work, or check raw bytes
                    if let Some(icmp) = packet.icmp() {
                        if icmp.icmp_type == 8 { // Echo Request
                            found_clone.store(true, Ordering::Relaxed);
                            packet.drop(); // Done with it
                        }
                    }
                }
            }).unwrap();
        });

        // 2. Send ICMP Packet from veth0
        let tx_builder = FluxBuilder::new("veth0")
            .queue_id(0)
            .bind_flags(XDP_FLAGS_SKB_MODE)
            .umem_pages(16);
        let mut tx_raw = tx_builder.build_raw().expect("Failed veth0");

        // Construct ICMP Echo Request
        // Eth (14)
        let mut pkt = vec![
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // Dst
            0x02, 0x00, 0x00, 0x00, 0x00, 0x01, // Src
            0x08, 0x00 // Type IP
        ];
        // IP (20)
        pkt.extend_from_slice(&[
            0x45, 0x00, 0x00, 28, // Ver, Len (20+8)
            0x00, 0x00, 0x00, 0x00, // ID, Frag
            0x40, 0x01, 0x00, 0x00, // TTL, Proto=1 (ICMP), Csum (0 for now)
            0xC0, 0xA8, 0x64, 0x01, // Src 192.168.100.1
            0xC0, 0xA8, 0x64, 0x02  // Dst 192.168.100.2
        ]);
        // ICMP (8)
        pkt.extend_from_slice(&[
            0x08, 0x00, 0xF7, 0xFF, // Type=8, Code=0, Csum (approx)
            0x00, 0x01, 0x00, 0x01  // ID, Seq
        ]);

        // Calc IP Csum (Lazy: just use valid-ish packet or ignore validation on RX side if not strict)
        // fluxcapacitor parsing doesn't validate checksum by default unless asked.

        // Write to TX UMEM
        let tx_addr = 0;
        unsafe {
            let dest = tx_raw.umem.as_ptr().add(tx_addr);
            std::ptr::copy_nonoverlapping(pkt.as_ptr(), dest, pkt.len());
        }
        
        // Send
        let idx = tx_raw.tx.reserve(1).unwrap();
        unsafe {
            tx_raw.tx.write_at(idx, XDPDesc { addr: tx_addr as u64, len: pkt.len() as u32, options: 0 });
        }
        tx_raw.tx.submit(idx + 1);
        tx_raw.wakeup_tx().unwrap();

        // 3. Wait for detection
        thread::sleep(Duration::from_millis(100));
        
        stop_signal.store(true, Ordering::Relaxed);
        let _ = engine_thread.join();

        assert!(found_icmp.load(Ordering::Relaxed), "Engine did not detect ICMP packet");
    }
}

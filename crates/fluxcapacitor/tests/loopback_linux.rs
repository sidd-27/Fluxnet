#[cfg(target_os = "linux")]
mod linux_loopback {
    use fluxcapacitor::builder::FluxBuilder;
    use fluxcapacitor::system::split;
    use fluxcapacitor_core::ring::XDPDesc;
    use std::thread;
    use std::time::Duration;
    use std::sync::atomic::{Ordering, AtomicU32};

    // XDP_FLAGS_SKB_MODE = 1 << 1 = 2
    const XDP_FLAGS_SKB_MODE: u16 = 2;

    #[test]
    fn test_veth_loopback_raw() {
        // This test requires `veth0` and `veth1` to be up.
        // Run `scripts/setup_veth.sh` first.

        // 1. Setup Receiver (veth1)
        let rx_builder = FluxBuilder::new("veth1")
            .queue_id(0)
            .bind_flags(XDP_FLAGS_SKB_MODE)
            .umem_pages(16)
            .load_xdp(true);
            
        let mut rx_raw = match rx_builder.build_raw() {
            Ok(r) => r,
            Err(e) => {
                panic!("Failed to bind to veth1: {}. Make sure veth interfaces exist (run scripts/setup_veth.sh) and you have root/CAP_NET_RAW.", e);
            }
        };
        
        // Prepare RX: Fill the Fill Ring with some buffers
        // We give it buffers at 0, 2048, 4096...
        let fill_count = 8;
        let idx = rx_raw.fill.reserve(fill_count).expect("Failed to reserve fill ring");
        
        for i in 0..fill_count {
            let addr = (i as u64) * 2048;
            unsafe { rx_raw.fill.write_at(idx + i, addr) };
        }
        rx_raw.fill.submit(idx + fill_count);
        // Kick RX to notify kernel
        rx_raw.wakeup_rx().expect("Failed to wakeup RX");

        // 2. Setup Sender (veth0)
        let tx_builder = FluxBuilder::new("veth0")
            .queue_id(0)
            .bind_flags(XDP_FLAGS_SKB_MODE)
            .umem_pages(16);
            
        let mut tx_raw = tx_builder.build_raw().expect("Failed to bind to veth0");

        // 3. Prepare Packet in TX UMEM
        let payload = b"Hello Linux Loopback";
        // Ethernet frame: Dest (Broadcast), Src (Arbitrary), Type (Test), Payload
        let mut frame_data = vec![0xFF; 6]; // Dest: Broadcast
        frame_data.extend_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x01]); // Src
        frame_data.extend_from_slice(&[0xAA, 0xBB]); // Type: 0xAABB (Test)
        frame_data.extend_from_slice(payload);

        let tx_addr = 0; // Use first frame of TX UMEM
        unsafe {
            let dest = tx_raw.umem.as_ptr().add(tx_addr as usize);
            std::ptr::copy_nonoverlapping(frame_data.as_ptr(), dest, frame_data.len());
        }

        // 4. Send Packet
        let tx_idx = tx_raw.tx.reserve(1).expect("TX Ring full");
        let desc = XDPDesc {
            addr: tx_addr,
            len: frame_data.len() as u32,
            options: 0,
        };
        unsafe { tx_raw.tx.write_at(tx_idx, desc) };
        tx_raw.tx.submit(tx_idx + 1);
        tx_raw.wakeup_tx().expect("Failed to kick TX");

        // 5. Receive Packet (Poll loop)
        let mut received = false;
        for _ in 0..10 { // Retry a few times
            let n = rx_raw.rx.peek(1);
            if n > 0 {
                let desc = unsafe { rx_raw.rx.read_at(rx_raw.rx.consumer_idx()) };
                rx_raw.rx.release(1);
                
                // Verify content
                let ptr = unsafe { rx_raw.umem.as_ptr().add(desc.addr as usize) };
                let len = desc.len as usize;
                let data = unsafe { std::slice::from_raw_parts(ptr, len) };
                
                if len >= frame_data.len() && &data[..frame_data.len()] == frame_data.as_slice() {
                    received = true;
                    break;
                } else {
                    println!("Received unexpected packet of len {}", len);
                }
            }
            
            // Wait/Poll
            rx_raw.wakeup_rx().unwrap(); // Wait for packet
            thread::sleep(Duration::from_millis(50));
        }

        assert!(received, "Did not receive the packet on veth1");
    }
}

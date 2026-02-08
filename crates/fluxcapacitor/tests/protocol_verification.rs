#[cfg(feature = "simulator")]
#[cfg(test)]
mod protocol_tests {
    use fluxcapacitor::builder::FluxBuilder;
    use fluxcapacitor::engine::FluxEngine;
    use fluxcapacitor::simulator::control;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn setup_engine() -> (FluxEngine, std::os::windows::io::RawHandle) {
        let builder = FluxBuilder::new("eth0").queue_id(0).umem_pages(32);
        let raw = builder.build_raw().expect("Failed to build raw");
        let fd = raw.fd();
        let engine = FluxEngine::new(raw, 32);
        (engine, fd)
    }

    #[test]
    fn test_protocol_stack_parsing() {
        let (mut engine, fd) = setup_engine();
        
        // 1. Construct UDP Packet
        // Eth + IP + UDP + Payload
        let mut pkt = Vec::new();
        // Eth
        pkt.extend_from_slice(&[0xFF; 6]); // Dst
        pkt.extend_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x01]); // Src
        pkt.extend_from_slice(&[0x08, 0x00]); // Type: IPv4
        
        // IPv4 (20 bytes)
        pkt.extend_from_slice(&[0x45, 0x00, 0x00, 48]); // Ver/IHL, Total Len (20+8+20)
        pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // ID, Frag
        pkt.extend_from_slice(&[0x40, 17, 0x00, 0x00]); // TTL, Proto=17 (UDP), Csum
        pkt.extend_from_slice(&[192, 168, 1, 1]); // Src
        pkt.extend_from_slice(&[192, 168, 1, 100]); // Dst

        // UDP (8 bytes)
        pkt.extend_from_slice(&1234u16.to_be_bytes()); // Src Port
        pkt.extend_from_slice(&80u16.to_be_bytes());   // Dst Port
        pkt.extend_from_slice(&28u16.to_be_bytes());   // Len (8 + 20)
        pkt.extend_from_slice(&[0x00, 0x00]);          // Checksum

        // Payload (20 bytes)
        pkt.extend_from_slice(&[0xAA; 20]);

        // 2. Inject
        control::inject_packet(fd, &pkt).expect("Injection failed");

        // 3. Process and Verify
        let success = Arc::new(AtomicBool::new(false));
        let success_clone = success.clone();

        engine.process_batch(&mut |batch| {
            for packet in batch.iter_mut() {
                // Verify Ethernet
                let eth = packet.ethernet().expect("Failed to parse Ethernet");
                assert_eq!(eth.eth_type(), 0x0800);

                // Verify IPv4
                let ip = packet.ipv4().expect("Failed to parse IPv4");
                assert_eq!(ip.proto, 17);
                assert_eq!(ip.src(), 0xC0A80101); // 192.168.1.1

                // Verify UDP
                let udp = packet.udp().expect("Failed to parse UDP");
                assert_eq!(udp.src_port(), 1234);
                assert_eq!(udp.dst_port(), 80);

                success_clone.store(true, Ordering::Relaxed);
            }
        }).expect("Processing failed");

        assert!(success.load(Ordering::Relaxed), "Packet was not correctly parsed");
    }

    #[test]
    fn test_tcp_icmp_parsing() {
        let (mut engine, fd) = setup_engine();

        // 1. Inject TCP SYN
        let mut tcp_pkt = Vec::new();
        tcp_pkt.extend_from_slice(&[0; 12]); tcp_pkt.extend_from_slice(&[0x08, 0x00]); // Eth
        tcp_pkt.extend_from_slice(&[0x45, 0x00, 0x00, 40, 0,0,0,0, 64, 6, 0,0, 1,1,1,1, 2,2,2,2]); // IP (Proto 6=TCP)
        tcp_pkt.extend_from_slice(&0x1234u16.to_be_bytes()); // Src Port
        tcp_pkt.extend_from_slice(&0x0050u16.to_be_bytes()); // Dst Port
        tcp_pkt.extend_from_slice(&[0; 8]); // Seq, Ack
        tcp_pkt.extend_from_slice(&[0x50, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // Flags (SYN=0x02), Win, Csum, Urp

        control::inject_packet(fd, &tcp_pkt).expect("TCP inject failed");

        // 2. Inject ICMP Echo
        let mut icmp_pkt = Vec::new();
        icmp_pkt.extend_from_slice(&[0; 12]); icmp_pkt.extend_from_slice(&[0x08, 0x00]); // Eth
        icmp_pkt.extend_from_slice(&[0x45, 0x00, 0x00, 28, 0,0,0,0, 64, 1, 0,0, 1,1,1,1, 2,2,2,2]); // IP (Proto 1=ICMP)
        icmp_pkt.extend_from_slice(&[0x08, 0x00, 0x00, 0x00, 0x12, 0x34, 0x00, 0x01]); // ICMP Echo Req

        control::inject_packet(fd, &icmp_pkt).expect("ICMP inject failed");

        // 3. Verify
        let found_tcp = Arc::new(AtomicBool::new(false));
        let found_icmp = Arc::new(AtomicBool::new(false));
        
        let tcp_clone = found_tcp.clone();
        let icmp_clone = found_icmp.clone();

        engine.process_batch(&mut |batch| {
            for packet in batch.iter_mut() {
                if let Some(tcp) = packet.tcp() {
                    if tcp.src_port() == 0x1234 {
                        tcp_clone.store(true, Ordering::Relaxed);
                    }
                }
                if let Some(icmp) = packet.icmp() {
                    if icmp.kind == 8 {
                        icmp_clone.store(true, Ordering::Relaxed);
                    }
                }
            }
        }).expect("Batch failed");

        assert!(found_tcp.load(Ordering::Relaxed), "TCP SYN not found");
        assert!(found_icmp.load(Ordering::Relaxed), "ICMP Echo not found");
    }

    #[test]
    fn test_packet_mutators_adjust_head() {
        let (mut engine, fd) = setup_engine();

        // Packet with VLAN tag (extra 4 bytes)
        let mut vlan_pkt = Vec::new();
        vlan_pkt.extend_from_slice(&[0xFF; 6]); // Dst
        vlan_pkt.extend_from_slice(&[0x02; 6]); // Src
        vlan_pkt.extend_from_slice(&[0x81, 0x00, 0x00, 0x64]); // VLAN TPID 0x8100, ID 100
        vlan_pkt.extend_from_slice(&[0x08, 0x00]); // Real EthType IPv4
        vlan_pkt.extend_from_slice(&[0x45; 20]); // Dummy IP

        control::inject_packet(fd, &vlan_pkt).expect("VLAN inject failed");

        let success = Arc::new(AtomicBool::new(false));
        let success_clone = success.clone();

        engine.process_batch(&mut |batch| {
            for mut packet in batch.iter_mut() {
                // Initial parse: it should NOT be valid IPv4 because of VLAN tag
                assert!(packet.ipv4().is_none());

                // Strip VLAN tag (4 bytes) manually
                // PacketRef::adjust_head(offset)
                // We need to "move the head" forward by 12 (MACs) + 4 (VLAN) = 16 bytes?
                // No, adjust_head is relative to current ptr.
                
                // Let's strip the first 14 (Eth) + 4 (VLAN) = 18 bytes
                packet.adjust_head(18);
                
                // Now it should start with IP header
                let data = packet.data();
                assert_eq!(data[0], 0x45); // IP version
                
                success_clone.store(true, Ordering::Relaxed);
            }
        }).expect("Batch failed");

        assert!(success.load(Ordering::Relaxed));
    }
}

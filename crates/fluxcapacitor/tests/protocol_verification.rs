#![cfg(not(target_os = "linux"))]

#[cfg(test)]
mod tests {
    use fluxcapacitor::builder::FluxBuilder;
    use fluxcapacitor::engine::FluxEngine;
    use fluxcapacitor::simulator::control;

    fn setup_engine() -> (FluxEngine, std::os::windows::io::RawHandle) {
        let builder = FluxBuilder::new("eth0").queue_id(0).umem_pages(16);
        let mut flux_raw = builder.build_raw().unwrap();
        
        let fd = flux_raw.fd; // Extract raw handle before engine takes ownership
        let engine = FluxEngine::new(flux_raw).unwrap();
        (engine, fd)
    }

    #[test]
    fn test_icmp_parsing() {
        let (mut engine, fd) = setup_engine();
        
        // 1. Inject an ICMP Echo Request Packet
        let icmp_packet: Vec<u8> = vec![
            // Ethernet Header (14 bytes)
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Dst Mac
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Src Mac
            0x08, 0x00,                         // Type: IPv4
            
            // IPv4 Header (20 bytes)
            0x45, 0x00, 0x00, 0x1C, // Ver, TOS, Total Len
            0x00, 0x00, 0x00, 0x00, // ID, Flags
            0x40, 0x01, 0x00, 0x00, // TTL, Proto (ICMP), Checksum
            0x7F, 0x00, 0x00, 0x01, // Src IP (127.0.0.1)
            0x7F, 0x00, 0x00, 0x01, // Dst IP (127.0.0.1)
            
            // ICMP Header (8 bytes)
            0x08, 0x00, 0xF7, 0xFF, // Type (Echo Req), Code, Checksum
            0x00, 0x00, 0x00, 0x00, // ID, Seq
        ];
        
        control::inject_packet(fd, icmp_packet);
        
        // 2. Process Packet
        engine.consume(1, |batch| {
            assert_eq!(batch.len(), 1);
            let pkt = &batch[0];
            
            // Verify Headers
            assert!(pkt.ethernet().is_some());
            assert!(pkt.ipv4().is_some());
            
            let icmp = pkt.icmp();
            assert!(icmp.is_some());
            
            let h = icmp.unwrap();
            assert_eq!(h.icmp_type, 8); // Echo Request
            assert_eq!(h.icmp_code, 0);
        });
    }
}
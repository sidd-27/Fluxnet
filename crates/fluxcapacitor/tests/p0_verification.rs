#[cfg(test)]
mod tests {
    use fluxcapacitor::builder::FluxBuilder;
    use fluxcapacitor::system::split;

    #[test]
    fn test_p0_verification_flow() {
        // 1. Test Builder (Mocked/Stubbed on Windows)
        let builder = FluxBuilder::new("eth0")
            .queue_id(0)
            .umem_pages(16); // Small size for test
            
        let flux_raw = builder.build_raw().expect("Failed to build FluxRaw");
        
        // 2. Test Split (Resource Ownership Transfer)
        let (rx, tx) = split(flux_raw);
        
        // 3. Test fd access
        assert!(rx.fd() >= 0);
        
        // 4. Test Drop (Resource Cleanup)
        // Explicitly drop to verify no panic in MmapArea::drop
        drop(rx);
        drop(tx);
        
        println!("P0 Verification Flow Passed!");
    }
}

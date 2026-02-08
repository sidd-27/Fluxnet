#[cfg(all(feature = "simulator", not(target_os = "linux")))]
#[cfg(test)]
mod raw_tests {
    use fluxcapacitor::builder::FluxBuilder;

    #[test]
    fn test_raw_mode_debug() {
        let builder = FluxBuilder::new("eth0").queue_id(0).umem_pages(64);
        let raw = builder.build_raw().expect("Failed to build raw");

        // Verify we can access rings directly (Safety Guardrails would suggest being careful)
        assert_eq!(raw.rx.len(), 64);
        assert_eq!(raw.tx.len(), 64);

        // Call debug helper
        raw.debug_rings();
        
        // Ensure initial state is sane in simulator
        // In simulator, producer/consumer are initialized to 0 usually.
        // For RX, available is 0.
        // For TX, available is size (64).
        assert_eq!(raw.rx.available(), 0);
        assert_eq!(raw.tx.available(), 64);
    }
}

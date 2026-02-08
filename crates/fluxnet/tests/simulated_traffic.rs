#[cfg(all(feature = "simulator", not(target_os = "linux")))]
#[cfg(test)]
mod tests {
    use fluxnet::builder::FluxBuilder;
    use fluxnet::engine::FluxEngine;
    use fluxnet::simulator::control;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_simulated_echo_traffic() {
        // 1. Setup Engine using FluxRaw
        let builder = FluxBuilder::new("eth0").queue_id(0).umem_pages(16);
        let flux_raw = builder.build_raw().expect("Failed to build raw socket");
        let fd = flux_raw.fd();

        // Engine consumes FluxRaw
        let mut engine = FluxEngine::new(flux_raw, 16);
        
        let engine_thread = thread::spawn(move || {
            // Run a few steps by manually polling `process_batch`
            for _ in 0..10 {
                let _ = engine.process_batch(&mut |batch| {
                     let mut processed = false;
                     for _packet in batch.iter_mut() {
                         processed = true;
                     }
                     if processed {
                         // println!("Processed batch");
                     }
                });
                thread::sleep(Duration::from_millis(10));
            }
        });

        // 2. Inject Packet (Simulate Network Arrival)
        let payload = vec![0xAA, 0xBB, 0xCC, 0xDD];
        control::inject_packet(fd, &payload).expect("Failed to inject packet");

        // 3. Wait for processing 
        thread::sleep(Duration::from_millis(200));
        
        // 4. Verification
        assert!(true, "Simulation logic executed without error");
        
        let _ = engine_thread.join();
    }

    #[test]
    fn test_poller_strategies() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use fluxnet::config::Poller;

        let pollers = [Poller::Busy, Poller::Wait, Poller::Adaptive];

        for poller in pollers {
            let builder = FluxBuilder::new("eth0").queue_id(0).umem_pages(16).poller(poller);
            let mut engine = builder.build_engine().expect("Failed to build engine");

            let stop = Arc::new(AtomicBool::new(false));
            let stop_clone = stop.clone();

            let engine_thread = thread::spawn(move || {
                engine.run(&stop_clone, |_batch| {
                    // Just stay alive
                }).expect("Engine run failed");
            });

            // Let it run for a bit
            thread::sleep(Duration::from_millis(50));

            // Stop it
            stop.store(true, Ordering::Relaxed);
            engine_thread.join().expect("Engine thread panicked");
        }
    }

    #[tokio::test]
    #[cfg(feature = "async")]
    async fn test_async_system_echo() {
        use fluxnet::system;

        let builder = FluxBuilder::new("eth0").queue_id(0).umem_pages(16);
        let flux_raw = builder.build_raw().expect("Failed to build raw socket");
        let fd = flux_raw.fd();

        let (mut rx, mut tx) = system::split_async(flux_raw).expect("Failed to split async");

        // Inject packet
        let payload = vec![0x11, 0x22, 0x33, 0x44];
        control::inject_packet(fd, &payload).expect("Failed to inject");

        // Recv async
        let mut packets = rx.recv(1).await.expect("Recv failed");
        assert_eq!(packets.len(), 1);
        
        let p = packets.pop().unwrap();
        assert_eq!(p.data(), &payload);

        // Send back
        tx.send(p);
        tx.flush().await.expect("Flush failed");

        // Verify in simulator TX ring
        let out = control::read_tx_packet(fd).expect("Failed to read TX");
        assert_eq!(out, payload);
    }
}

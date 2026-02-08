# Fluxnet Production Readiness TODO

This document outlines the remaining work required to bring `fluxnet` from its current prototype state to a production-ready crate.

## ✅ Completed (P0: Critical Correctness)

- [x] **FluxSystem Frame Recycling**: Implemented thread-safe `SharedFrameState` (SegQueue) for `Packet` recycling.
- [x] **Error Handling Audit**: Replaced `unwrap()`/`expect()` with proper `FluxError` propagation.
- [x] **Resource Cleanup**: Implemented `Drop` for `MmapArea` to prevent memory leaks.

## 1. Feature Completeness (P1)

- [x] **Windows Simulator for Testing**:
    -   Upgrade `windows_stubs.rs` to stateful mocks (simulating kernel rings and UMEM).
    -   Add `fluxnet::simulator` module (test-only) to inject/inspect packets.
    -   Enable functional `FluxEngine` tests on Windows.
- [x] **Poller Implementation**:
    -   Implement the `Adaptive` polling strategy in `FluxEngine::run`.
    -   Support `BusyPoll` (spin-loop) and `Syscall` (wait_for_rx) modes.
- [x] **Async Runtime Integration (Reactor)**:
    -   Implement `AsyncFluxRaw` using `tokio::io::unix::AsyncFd`.
    -   Add `recv_async` and `send_async` to `FluxRx`/`FluxTx` for non-blocking integration.
- [x] **RSS / Multi-Queue Support**:
    -   Verify `FluxBuilder::queue_id` correctly binds to specific hardware queues.
    -   Add support for configuring RSS (Receive Side Scaling) via eBPF maps if needed.
- [ ] **FluxRaw "Bare Metal" Mode**:
    -   Expose `FluxRaw` public module with documented "Safety Guardrails".
    -   Add `debug_rings()` helper for inspecting ring state.

## 2. Protocol Support (P2)

- [x] **L4 Protocols**:
    -   Implement `UdpHeader` and `TcpHeader` parsing/serializing in `fluxnet-proto`.
    -   Add `checksum()` validation methods.
- [x] **ICMP Support**:
    -   Add basic ICMP parsing for ping/traceroute utilities.
- [x] **Zero-Copy Mutators**:
    -   Ensure `PacketRef` has methods like `adjust_head(offset)` to strip/add VLAN tags without copying.

## 3. Testing & Verification (P3)

- [ ] **Integration Tests (Linux Required)**:
    -   `loopback_test`: Use `veth` pairs to send packets from `fluxnet` to kernel and back.
    -   `fuzz_engine`: Flood `FluxEngine` with random data to test stability.
- [ ] **Unit Tests**:
    -   Test `PacketBatch` iterator logic (empty batch, full batch, wrap-around).
    -   Test Ring arithmetic (producer/consumer pointer wrapping).

## 4. Documentation & Examples (P3)

- [ ] **Examples**:
    -   Create `examples/firewall_engine.rs` (Mode A).
    -   Create `examples/load_balancer_system.rs` (Mode B).
    -   Create `examples/manual_packet_pump.rs` (Mode C).
- [ ] **Rustdocs**:
    -   Add doc comments to all public APIs in `fluxnet`.
    -   Include "Safety" sections for all `unsafe` functions in `fluxnet-core`.

## 5. Performance Optimization (P4)

- [ ] **Prefetching**:
    -   Implement `_mm_prefetch` in the hot loop.
- [ ] **Batch Size Tuning**:
    -   Benchmark different batch sizes (16, 32, 64) to find the sweet spot.
- [ ] **Hugepages**:
    -   Support hugepage allocation for UMEM to reduce TLB misses.

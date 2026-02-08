# LLM Context & Developer Guide for Fluxnet

This document provides comprehensive context for AI assistants and developers working on the `Fluxnet` project. It covers architectural decisions, project structure, build instructions (including Windows simulation), and current development status.

---

## 1. Project Overview

**Fluxnet** is a high-performance, idiomatic Rust library for Linux AF_XDP (XDP sockets). It aims to provide raw packet I/O performance comparable to DPDK but with safe Rust abstractions.

### Core Goals
1.  **Safety**: Safe abstractions over unsafe `mmap` regions and raw pointers.
2.  **Performance**: Zero-copy packet handling, batch processing, and minimal overhead.
3.  **Cross-Platform Development**: While AF_XDP is Linux-only, we support **Windows** development via a full-featured simulator.

---

## 2. Architecture

The workspace is split into three crates:

### A. `crates/fluxnet-core` (The Engine Room)
-   **Role**: Handles low-level, unsafe system interaction.
-   **Key Components**:
    -   `sys`: Raw FFI bindings to kernel structures (xdp_desc, setsockopt, mmap).
    -   `umem`: Manages `UmemRegion` (hugepage-aligned memory blocks).
    -   `ring`: Type-safe wrappers around `ProducerRing` and `ConsumerRing` (circular buffers).
    -   **Windows Stubs**: When compiled on non-Linux, this crate provides a **Simulator Layer**. It mocks kernel rings and UMEM in memory (`lazy_static` HashMap), allowing functional testing of the upper layers without a Linux kernel.

### B. `crates/fluxnet` (The User Interface)
-   **Role**: Safe, high-level API for network applications.
-   **Key Interactions**:
    -   **FluxRaw**: A handle to the raw socket and rings. Explicitly `Send` to allow passing between threads.
    -   **FluxEngine**: The main event loop driver. Runs on a dedicated thread, polling RX rings and invoking user callbacks.
    -   **Packet / PacketRef**:
        -   `Packet` (Owned): Used in the `FluxSystem` channel-based API. Uses `SharedFrameState` to automatically recycle frames back to the Fill Ring when dropped.
        -   `PacketRef` (Zero-Copy): Used in the `FluxEngine` callback API. Direct reference to UMEM data, valid only during the callback.
    -   **FluxBuilder**: Fluent builder pattern for creating generic AF_XDP sockets.

### C. `crates/fluxnet-ebpf` (The Kernel Hook)
-   **Role**: The eBPF program running in the kernel XDP hook.
-   **Function**: Redirects packets to the specific AF_XDP socket map (`xsks_map`).
-   **Build**: Compiled via `aya-bpf`.

---

## 3. Development Workflow

### A. Windows Development (The Simulator)
Since AF_XDP is Linux-specific, we use a **Simulator** for local development on Windows.

-   **Mechanism**: `fluxnet-core/src/windows_stubs.rs` implements a stateful mock of the kernel. It allocates memory for rings and UMEM on the heap and keys them by a "fake" file descriptor.
-   **Running Tests**: You MUST enable the `simulator` feature.
    ```bash
    cargo test -p fluxnet --features simulator
    ```
-   **Key Test**: `tests/simulated_traffic.rs`. This test spins up a `FluxEngine`, injects fake packets into the mock RX ring, and verifies that the engine processes them and echoes them back to the TX ring.

### B. Linux Development (Real Hardware)
-   **Prerequisites**: Kernel 5.4+, libxdp-dev (optional but good).
-   **Build**: Standard cargo build works. The stubbing logic is `#[cfg(not(target_os = "linux"))]`.

---

## 4. Key Technical Decisions & Patterns

### Thread Safety (`Send`/`Sync`)
-   **FluxRaw**: Contains raw pointers (`*mut u32`, etc.). It is explicitly marked `unsafe impl Send` because:
    1.  On Linux, separate pointers are used for Prod/Cons, and ownership is transferred carefully.
    2.  On Windows Simulator, the backend state is protected by a global `Mutex`.
-   **SegQueue**: Used for the "channel" based API (`FluxSystem`) to allow multiple producer/single consumer verification handling without locks in the hot path.

### Memory Model
-   **UMEM**: A single contiguous memory region.
-   **Frames**: Fixed-size blocks (default 2048/4096 bytes).
-   **Recycling**:
    -   **FluxEngine**: Explicitly manages recycling. Completed TX frames are moved to the Fill Ring.
    -   **FluxSystem**: `Packet` Drop implementation pushes the frame index back to a shared queue (`free_frames`).

### Error Handling
-   **FluxError**: Centralized error enum. We avoid `unwrap()` in library code.

---

## 5. Current Status (as of Feb 2026)

-   **P0 (Critical Infrastructure)**: ✅ Complete.
    -   Core memory management, ring interaction, and frame recycling logic are solid.
-   **Simulator**: ✅ Complete.
    -   Windows dev environment is fully functional for logic testing.
-   **Next Up (P1)**:
    -   **Poller**: Implementing intelligent polling (busy-wait vs syscall) to reduce CPU usage.
    -   **Async Integration**: Tokio `AsyncFd` support.

---

## 6. Useful Commands

**Run Simulator Tests (Windows):**
```bash
cargo test -p fluxnet --features simulator -- --nocapture
```

**Check Compilation:**
```bash
cargo check
cargo check -p fluxnet-ebpf
```

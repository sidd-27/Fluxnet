# Fluxcapacitor: Project Context & Architecture

fluxcapacitor is a high-performance Linux AF_XDP (XDP sockets) library written in Rust. It provides a safe, idiomatic, and zero-copy interface for kernel-bypass networking, rivaling DPDK performance while leveraging Rust's safety and modern concurrency primitives.

## 1. Project Structure

The project is organized as a Cargo Workspace:

### `crates/fluxcapacitor` (The User Interface)
- **Role:** High-level, safe developer API.
- **Key Modules:**
    - `builder`: Fluent API (`FluxBuilder`) for socket configuration.
    - `engine`: Mode A (Throughput-first). Managed hot loop with callbacks.
    - `system`: Mode B (Control-first). Ownership-based `Rx/Tx` handles with async support.
    - `raw`: Mode C (Bare metal). Direct access to ring primitives with safety guardrails.
    - `packet`: Safe abstractions for UMEM frames (`PacketRef` and `Packet`).

### `crates/fluxcapacitor-core` (The Hardware Abstraction Layer)
- **Role:** Low-level FFI, memory management, and ring buffer primitives.
- **Key Modules:**
    - `sys`: Raw Linux AF_XDP syscalls and C-binding offsets.
    - `umem`: Management of contiguous memory regions and hugepages.
    - `ring`: Type-safe `ProducerRing` and `ConsumerRing` implementations.
    - `windows_stubs`: Stateful simulator for non-Linux development.

### `crates/fluxcapacitor-proto` (The Protocol Layer)
- **Role:** Zero-copy protocol views and parsers.
- **Support:** Ethernet, IPv4, UDP, TCP, and ICMP.
- **Feature:** Includes checksum validation and `adjust_head` for zero-copy header manipulation.

### `crates/fluxcapacitor-ebpf` (The Kernel Hook)
- **Role:** XDP program that redirects incoming packets from the kernel to AF_XDP sockets using an `XskMap`.

---

## 2. Architectural Modes

fluxcapacitor provides three distinct ways to interact with the network:

| Mode | Name | API Style | Best For |
| :--- | :--- | :--- | :--- |
| **Mode A** | `FluxEngine` | Inversion of Control (Callback) | Firewalls, IDS, high-throughput filtering. |
| **Mode B** | `FluxSystem` | Split-Ownership (`Rx`/`Tx`) | TCP stacks, Load balancers, Async/Tokio apps. |
| **Mode C** | `FluxRaw` | Bare-Metal Rings | HFT, custom allocators, research. |

---

## 3. Data Structures & Relations

### Core Socket Container: `FluxRaw`
The root structure for any mode. It contains:
- `UmemRegion`: The shared memory area for packet data.
- 4 Rings: `Rx`, `Tx` (Consumers) and `Fill`, `Completion` (Producers).
- `RawFd`: The underlying AF_XDP socket file descriptor.

### Managed Loop: `FluxEngine`
Wraps `FluxRaw` and manages a hot loop.
- Uses `PacketBatch` to provide `PacketRef` (borrowed views) to a user callback.
- Automatically handles ring updates and poller strategies (Busy, Wait, Adaptive).

### Ownership Handles: `FluxRx` & `FluxTx`
Created by `split(FluxRaw)`.
- **`FluxRx`**: Owns the `Rx` and `Fill` rings.
- **`FluxTx`**: Owns the `Tx` and `Completion` rings.
- **`AsyncFluxRx/Tx`**: Wrappers using `tokio::io::unix::AsyncFd` for non-blocking I/O.

### Packet Abstractions
- **`PacketRef<'a>`**: A transient, zero-copy view of a frame. Used in `FluxEngine`.
- **`Packet`**: An owned, `Send`-able object. Used in `FluxSystem`.
    - **Lifecycle:** When `Packet` is dropped, its frame index is sent to a `SharedFrameState` (SegQueue). `FluxRx` then pulls from this queue to refill the `Fill Ring`.

---

## 4. Memory & Lifecycle Model

- **UMEM:** A single large allocation divided into fixed-size "frames" (usually 2KB or 4KB).
- **Zero-Copy:** Data never leaves UMEM. "Parsing" is just casting pointers to protocol structs (`EthHeader`, etc.).
- **Recycling Loop:**
    1. **Kernel** puts data in a UMEM frame and publishes its index to the **RX Ring**.
    2. **User** reads index from **RX Ring**, processes data.
    3. **User** either:
        - Sends: Pushes index to **TX Ring**. Kernel sends it, then puts it in **Completion Ring**.
        - Drops: Returns index directly to **Fill Ring** (via `SharedFrameState`).
    4. **Kernel** uses indices in **Fill Ring** for next incoming packets.

---

## 5. Cross-Platform Development (Simulator)

fluxcapacitor is designed to be developed on **Windows/macOS** using the **Simulator**:
- **Backend:** `fluxcapacitor-core/src/windows_stubs.rs` mocks the kernel state in a global `SOCKETS` mutex.
- **Rings:** Mapped to `Box<[u8]>` with stable pointers to mimic `mmap`.
- **Injection:** `fluxcapacitor::simulator::control` allows tests to manually inject packets into mock RX rings and read from mock TX rings.

---

## 6. Development & Test Commands

### Core Development
- **Check Build:** `cargo check -p fluxcapacitor`
- **Verify eBPF:** `cargo check -p fluxcapacitor-ebpf`
- **Build Docs:** `cargo doc --no-deps`

### Testing (Simulator)
Most development happens using the simulator feature:
```bash
cargo test -p fluxcapacitor --features "simulator async"
```

### Protocol Tests
```bash
cargo test -p fluxcapacitor-proto
```

## 7. Instructional Context for Gemini

- **Safety:** Always encapsulate `unsafe` in `fluxcapacitor-core`. Public APIs in `fluxcapacitor` should be safe.
- **Performance:** Avoid heap allocations or `Vec<u8>` in the hot path. Use `PacketRef::data_mut()` for in-place modification.
- **Endianness:** Always use `.to_be()`/`from_be()` or `from_be_bytes()` when dealing with protocol headers.
- **Async:** When using `AsyncFluxRx`, remember it yields to the executor if the ring is empty.

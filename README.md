# Fluxcapacitor

> Packet handling so fast, it feels like time travel.

Fluxcapacitor is a high-performance Linux AF_XDP (XDP sockets) library written in Rust. It provides a safe, idiomatic, and zero-copy interface for kernel-bypass networking, rivaling DPDK performance while leveraging Rust's safety and modern concurrency primitives.

## Features

- **Extreme Performance**: Zero-copy data path using Linux AF_XDP.
- **Safety First**: Encapsulated `unsafe` code in core modules, providing a safe public API.
- **Multiple Architectural Modes**:
    - **Mode A (FluxEngine)**: Throughput-first, managed hot loop with callbacks.
    - **Mode B (FluxSystem)**: Control-first, split-ownership (Rx/Tx) handles with async support.
    - **Mode C (FluxRaw)**: Bare-metal access to ring primitives.
- **Protocol Support**: Zero-copy parsers for Ethernet, IPv4, UDP, TCP, and ICMP.
- **Simulator**: Develop and test on Windows/macOS using a stateful kernel simulator.

## Project Structure

- `crates/fluxcapacitor`: The main user-facing API.
- `crates/fluxcapacitor-core`: Low-level FFI, memory management, and ring buffer primitives.
- `crates/fluxcapacitor-proto`: Zero-copy protocol views and parsers.
- `crates/fluxcapacitor-ebpf`: XDP program for kernel-space packet redirection.

## Getting Started

### Prerequisites

- Rust (latest stable)
- Linux with AF_XDP support (for production)
- Windows/macOS (for development using the simulator)

### Running Tests (Simulator)

```bash
cargo test -p fluxcapacitor --features "simulator async"
```

## License

This project is licensed under the MIT License.

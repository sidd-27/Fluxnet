# Fluxcapacitor: Project Context & Architecture

**Tagline:** Packet handling so fast, it feels like time travel.

Fluxcapacitor is a high-performance Linux AF_XDP (XDP sockets) library written in Rust. It provides a safe, idiomatic, and zero-copy interface for kernel-bypass networking.

## 1. Development Environment: WSL-First
Moving forward, **all core development and networking verification must be done inside WSL (Linux)**. The Windows simulator is available for high-level logic, but the AF_XDP and eBPF components require a Linux kernel.

### WSL Setup Requirements:
- **Toolchain:** `rustup toolchain install nightly && rustup component add rust-src --toolchain nightly`
- **Linker:** `cargo install bpf-linker`
- **Permissions:** Real AF_XDP tests must be run with `sudo -E env "PATH=$PATH"`.

## 2. Project Structure (Renamed)
- `crates/fluxcapacitor`: The main safe API (Engine, System, and Raw modes).
- `crates/fluxcapacitor-core`: HAL, Ring Buffer primitives, and eBPF build logic.
- `crates/fluxcapacitor-proto`: Zero-copy protocol parsers (Eth, IPv4, ICMP, UDP, TCP).
- `crates/fluxcapacitor-ebpf`: The kernel-space XDP program.

## 3. Key Architectural Features
- **Automated XDP Loading:** `FluxBuilder::load_xdp(true)` handles the entire eBPF lifecycle: finding the ELF, loading, attaching to the interface, and mapping the socket FD to the `XSK_MAP`.
- **Memory Model:** Zero-copy UMEM frames. The kernel and user-space share memory; only descriptors (pointers/indices) move across the ring buffers.
- **Modes:**
    - **Engine:** Managed loop with a batch callback (best for high throughput).
    - **System:** Rx/Tx handles with async/Tokio support (best for complex apps).
    - **Raw:** Direct access to AF_XDP rings.

## 4. Current Workflows
- **Setup Veth:** `sudo bash scripts/setup_veth.sh`
- **Run Tests:** `sudo -E env "PATH=$PATH" cargo test -p fluxcapacitor --features async`
- **Build eBPF:** Handled automatically by `fluxcapacitor-core/build.rs` during cargo build/check.
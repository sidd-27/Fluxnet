
### **1. High-Level Architecture**

The system is designed around the **AF_XDP** socket model, which relies on a **Shared Memory Region (UMEM)** and **Four Ring Buffers**.

1. **UMEM (The Arena):** A massive, contiguous chunk of RAM divided into fixed-size "Frames" (e.g., 4KB each). The NIC DMAs packets directly here.
    
2. **RX Ring:** The Kernel puts "Descriptors" (indices pointing to Frames in UMEM) here when packets arrive.
    
3. **Fill Ring:** You put empty Frame indices here to tell the Kernel "Use this space for the next incoming packet."
    
4. **TX Ring:** You put Frame indices here when you want to send a packet.
    
5. **Completion Ring:** The Kernel puts Frame indices here after it has finished sending them, so you can reuse the memory.
    

---

### **2. Project Folder Structure**

We will use a **Cargo Workspace** to separate the "Unsafe Engine" from the "Safe User API."


```
fluxnet/
├── Cargo.toml                          # Workspace definition
├── README.md                           # Architecture diagrams
├── xtask/                              # Build automation (eBPF compilation)
│
├── crates/
│   │
│   ├── fluxnet-core/                   # (INTERNAL) The Hardware Abstraction Layer
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── sys/                    # OS & FFI Boundaries
│   │       │   ├── libbpf.rs           # XDP socket syscalls
│   │       │   └── ioctl.rs            # NIC Queue configuration
│   │       │
│   │       ├── umem/                   # Memory Management
│   │       │   ├── mmap.rs             # HugeTLB / mmap logic
│   │       │   ├── allocator.rs        # The physical page manager
│   │       │   └── layout.rs           # Frame math (address <-> index)
│   │       │
│   │       ├── ring/                   # The Circular Queue Primitives
│   │       │   ├── shared.rs           # Common ring math (masking/wrapping)
│   │       │   ├── producer.rs         # Logic for Fill & TX Rings
│   │       │   ├── consumer.rs         # Logic for RX & Completion Rings
│   │       │   └── desc.rs             # struct xdp_desc (The 16-byte atom)
│   │       │
│   │       └── lib.rs                  # Exports `XskContext` for internal use
│   │
│   ├── fluxnet-proto/                  # (SHARED) Zero-Copy Protocol Parsers
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── view.rs                 # Trait: `PacketView` (BigEndian reads)
│   │       ├── ethernet.rs             # L2 Parsing
│   │       ├── ipv4.rs                 # L3 Parsing & Checksum validation
│   │       ├── ipv6.rs
│   │       ├── udp.rs                  # L4 Parsing
│   │       ├── tcp.rs
│   │       └── lib.rs
│   │
│   └── fluxnet/                        # (PUBLIC) The Developer API
│       ├── Cargo.toml
│       └── src/
│           ├── builder.rs              # The Common Entry Point
│           ├── config.rs               # Configuration Enums
│           ├── error.rs                # Unified Error Handling
│           │
│           ├── memory/                 # Safe Memory Handles
│           │   └── region.rs           # `UmemRegion` (The safe view of RAM)
│           │
│           ├── packet/                 # High-Level Data Abstractions
│           │   ├── raw.rs              # `PacketRef<'a>` (Borrowed / Mode A)
│           │   └── owned.rs            # `Packet` (Owned / Mode B)
│           │
│           ├── engine/                 # MODE A: FluxEngine (Throughput First)
│           │   ├── batch.rs            # Iterator logic for zero-copy loops
│           │   ├── runner.rs           # The IoC "Hot Loop" implementation
│           │   └── mod.rs
│           │
│           ├── system/                 # MODE B: FluxSystem (Control First)
│           │   ├── rx.rs               # `FluxRx` (Consumer Handle)
│           │   ├── tx.rs               # `FluxTx` (Producer Handle)
│           │   └── reactor.rs          # Async/Tokio integration
│           │
│           ├── raw/                    # MODE C: FluxRaw (Low-Level / Learning)
│           │   ├── socket.rs           # `struct FluxRaw` (The 4-Ring Container)
│           │   ├── accessors.rs        # `ProducerView` & `ConsumerView` traits
│           │   └── mod.rs              # Exports the "Mechanic's Toolset"
│           │
│           └── lib.rs                  # Re-exports all 3 modes
│
└── examples/
    ├── firewall_engine.rs              # Mode A: Simple drop/pass logic
    ├── load_balancer_system.rs         # Mode B: Threaded worker pipeline
    ├── tcp_stack_async.rs              # Mode B: Async/Await integration
    └── manual_packet_pump.rs           # Mode C: Manual ring management (Educational)
```

---

### **3. Data Structures (The "Hot Path")**

This is where the performance lives. We avoid objects that own data (`Vec<u8>`). We deal almost exclusively with **Indices** (`u64`) and **Raw Pointers**.

#### **A. The UMEM Arena (The Memory Pool)**

Located in `fluxnet-core/src/umem.rs`.


``` rust
pub struct Umem {
    // The raw mmap'd pointer to the huge memory region
    base_addr: *mut u8,
    // Size of the region
    size: usize,
    // Configuration for frame sizes (usually 2048 or 4096)
    frame_size: u32,
    
    // THE FREE LIST: A fast stack of indices that are currently unused.
    // We avoid allocating/freeing. We just push/pop from this stack.
    free_frames: Vec<u64>, 
}
```

#### **B. The Ring Buffers (Producer/Consumer)**

Located in `fluxnet-core/src/ring.rs`. The kernel and user-space communicate purely by updating integer counters (`producer_idx` and `consumer_idx`).


``` rust
// Generic Ring for Rx, Tx, Fill, Completion
pub struct XskRing<T> {
    // Pointer to the ring structure in kernel memory
    producer: *mut u32,
    consumer: *mut u32,
    descriptors: *mut T, // The actual data (indices)
    mask: u32,           // For fast modulo (size - 1)
}

// A Descriptor is tiny (16 bytes). Fits in CPU cache lines perfectly.
#[repr(C)]
pub struct XDPDesc {
    pub addr: u64, // Offset in UMEM
    pub len: u32,  // Length of packet
    pub options: u32,
}
```

---

### **4. The "Safe" User-Facing Structures**

This is how we enforce safety without the user knowing they are touching raw memory.

#### **The `Packet` Struct (Zero-Copy View)**

Located in `fluxnet/src/packet.rs`. This struct does not hold the data. It holds a **pointer** to the data inside the UMEM.


``` rust
use std::marker::PhantomData;

// 'a binds the Packet to the lifetime of the Batch processing loop.
// The user CANNOT store this Packet in a global variable.
pub struct Packet<'a> {
    raw_ptr: *mut u8,     // Points to UMEM + offset
    len: usize,           // Length of valid data
    umem_idx: u64,        // The index ID (needed to free it later)
    
    // PhantomData tells Rust: "I act like I hold a slice of bytes"
    _marker: PhantomData<&'a mut [u8]>,
    
    // Status flag: Did the user ask to Drop or Tx this?
    action: Action,
}

impl<'a> Packet<'a> {
    // Fast, unchecked view for internal use
    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.raw_ptr, self.len) }
    }

    // Mutable view for modifying headers
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.raw_ptr, self.len) }
    }
}
```

---

### **5. The Execution Model (Batch Processing)**

To get 10Gbps+ (14.8M pps), we cannot process one packet at a time. We use **Batching**.

**The Logic Flow:**

1. **Reserve** space in the `Fill Ring` (give buffers to kernel).
    
2. **Poll** the `Rx Ring` (ask "Got anything?").
    
3. **Batch Read:** Read up to 16 descriptors at once into a local array.
    
4. **User Callback:** Loop through the 16 items, wrapping them in `Packet<'a>` and calling the user's closure.
    
5. **Batch Commit:**
    
    - If user said `Tx`: Push index to `Tx Ring`.
        
    - If user said `Drop`: Push index back to `Fill Ring` (reuse immediately).
        
6. **Kick:** Notify kernel (syscall) only once per batch.
    

**The Loop Implementation (Pseudocode):**


``` rust
pub fn run<F>(&mut self, mut callback: F) 
where F: FnMut(Packet) -> Action 
{
    let mut batch = [XDPDesc::default(); 16]; // Stack allocated, fast

    loop {
        // 1. Consume up to 16 packets from RX Ring
        let count = self.rx_ring.read_batch(&mut batch);

        if count > 0 {
            for i in 0..count {
                let desc = batch[i];
                
                // Create the Safe View
                let mut pkt = Packet::new(self.umem, desc);
                
                // 2. Run User Logic (Inlined)
                let action = callback(pkt);
                
                // 3. Handle Result
                match action {
                    Action::Tx => self.tx_ring.push(desc),
                    Action::Drop => self.fill_ring.push(desc.addr), // Recycle
                }
            }
            
            // 4. Notify Kernel (One syscall for 16 packets)
            self.tx_ring.submit();
            self.fill_ring.submit();
        }
    }
}
```

### **6. Safety & Security Strategy**

- **Boundary:** The `fluxnet-core` crate contains all the `unsafe` code. The `fluxnet` crate contains _zero_ unsafe code in the public API.
    
- **Lifetime Pinning:** The `Packet<'a>` lifetime ensures no one can access the UMEM region after the frame has been submitted back to the kernel.
    
- **Bounds Checking:** The `Packet` struct methods (`as_slice`) implicitly use the length provided by the kernel descriptor, preventing buffer over-reads.


We use the **Builder Pattern** for setup and a **Closure-based Event Loop** for packet processing. This ensures the user cannot accidentally hold onto a packet longer than the batch lifetime (enforced by the borrow checker).

---

Here is the revised **FluxNet Design and Architecture Document**, structured to clearly distinguish the two usage modes: **The Engine** (High Throughput) and **The System** (Fine-Grained Control).

---

# FluxNet: Architecture & Design Document

## 1. Architectural Strategy & Implementation

FluxNet is designed as a dual-mode network library. It shares a common hardware abstraction layer but offers two distinct runtime models depending on user needs: **speed vs. control**.

### 1.1 Shared Foundation (The Core)

Both modes share the same initialization path via the `FluxBuilder`.

- **UMEM Management:** A contiguous chunk of memory is allocated via `mmap` for Zero-Copy packet handling.
    
- **AF_XDP Binding:** The library binds to specific hardware queues on the Network Interface Card (NIC).
    
- **Safety:** Both modes rely on a shared `Packet` abstraction that wraps raw UMEM pointers, ensuring memory safety without overhead.
    

### 1.2 Mode A: The FluxEngine (Throughput First)

Designed for "Serverless-style" packet processing where raw speed is paramount (e.g., Firewalls, DDoS mitigation).

- **Architecture:** **Inversion of Control (IoC)**. The library owns the main loop.
    
- **Implementation:**
    
    - **Batching:** Fetches packets in fixed-size batches (e.g., 16 or 32) to maximize Instruction Cache (I-Cache) locality.
        
    - **Syscall Coalescing:** Performs only one system call (`sendto`) _after_ processing the entire batch, drastically reducing user/kernel mode switching.
        
    - **Pipeline:** `RX Poll -> Batch Fetch -> User Callback -> Batch Commit -> TX Flush`.
        

### 1.3 Mode B: The FluxSystem (Control First)

Designed for complex applications requiring asynchronous logic, buffering, or precise timing (e.g., TCP stacks, load balancers with external lookups).

- **Architecture:** **Split-Ownership Model**. The library returns separate `Rx` and `Tx` handles, giving the user full control over the execution flow.
    
- **Implementation:**
    
    - **Decoupled Rings:** The Receive (Rx) and Transmit (Tx) rings are separated, allowing them to be moved to different threads or used in `async` contexts.
        
    - **Manual Lifecycle:** The user explicitly decides when to fetch packets and when to flush the socket. This allows holding a packet for future transmission (buffering) or dropping it silently.
        

---

## 2. Developer API Reference

### 2.1 Common Configuration (Entry Point)

All interactions start here. The builder creates the socket context.

**`struct FluxBuilder`**

- `fn interface(self, name: &str) -> Self`: Selects the network interface (e.g., "eth0").
    
- `fn queue_id(self, id: u32) -> Self`: Binds to a specific hardware queue (RSS).
    
- `fn bind_engine(self) -> FluxEngine`: Finalizes setup for **Mode A**.
    
- `fn bind_system(self) -> Result<(FluxRx, FluxTx), FluxError>`: Finalizes setup for **Mode B**.
    

---

# FluxEngine: Developer Experience & API Design Specification

## 1. Core Philosophy: "The Pit of Success"

The API is designed to guide developers toward high-performance patterns naturally.

- **Safe by Default:** If you forget to handle a packet, it is automatically recycled (dropped safely) rather than leaked.
    
- **Zero-Copy by Default:** The API exposes mutable slices to raw memory, discouraging expensive heap allocations.
    
- **Batching by Default:** The primary interface forces batch processing, preventing the "per-packet syscall" performance trap.
    

---

## 2. The Entry Point: `FluxBuilder`

A fluent builder pattern used to configure the engine before the hot loop starts. This separates _configuration_ (threading, polling) from _logic_ (packet handling).

Rust

``` rust
use fluxnet::{FluxBuilder, Poller};

fn main() -> Result<(), FluxError> {
    let engine = FluxBuilder::new("eth0")
        // Hardware Binding
        .queue_id(0)               // Bind to RSS Queue 0
        .umem_pages(4096)          // Allocate 16MB ring buffer
        
        // Performance Tuning
        .poller(Poller::Adaptive)  // Hybrid Busy/Wait strategy
        .batch_size(64)            // Process 64 packets per loop
        
        .build_engine()?;
        
    // ... start the loop
    Ok(())
}
```

---

## 3. The Hot Loop: `FluxEngine`

The engine takes a closure. This inversion of control allows the library to manage the lifecycle of the `PacketBatch`, ensuring `Drop` is called at the correct time to commit ring updates.

### 3.1 The `PacketBatch` Iterator

The `PacketBatch` implements `Iterator`, making it feel like a standard Rust collection.

Rust

``` rust
engine.run(|mut batch| {
    // Standard Rust iterator pattern
    for packet in batch.iter_mut() {
        
        // 1. Zero-Copy Parsing
        // The packet exposes helper methods to jump to headers without copying.
        let eth_header = packet.ethernet(); 
        
        if eth_header.is_ipv6() {
            // 2. Action: Drop
            // Explicitly mark for recycling.
            packet.drop(); 
        } else {
            // 3. Action: Modify & Send
            // Mutable access to raw bytes
            packet.data_mut()[0] = 0xFF; 
            packet.send();
        }
    }
    // IMPLICIT: Batch is dropped here. Rings are updated in bulk.
});
```

---

## 4. The Data Unit: `PacketRef`

This is a temporary view into a slot within the batch. It is lightweight and cannot outlive the batch (enforced by Rust lifetimes).

**Public Methods:**

| **Method**                | **Description**                                                 |
| ------------------------- | --------------------------------------------------------------- |
| `data() -> &[u8]`         | Returns a read-only slice of the packet payload.                |
| `data_mut() -> &mut [u8]` | Returns a mutable slice for in-place modification.              |
| `len() -> usize`          | Current packet length.                                          |
| `set_len(usize)`          | Truncate or extend the packet (within UMEM frame limits).       |
| `send()`                  | Marks this slot for Transmission (TX Ring).                     |
| `drop()`                  | Marks this slot for Recycling (Fill Ring). This is the default. |
| `adjust_head(i32)`        | Moves the data pointer (e.g., to strip VLAN tags).              |

---

## 5. The Escape Hatch: `extract()`

For scenarios where a packet must leave the hot loop (e.g., Async processing, buffering), the API provides an ownership transfer mechanism.

Rust

``` rust
engine.run(|mut batch| {
    // Must use enumerate() or index loop to use extract()
    for (i, packet) in batch.iter_mut().enumerate() {
        
        if packet.needs_async_processing() {
            // Returns an owned `Packet` struct.
            // This slot is removed from the Batch's bulk commit logic.
            let owned_packet = batch.extract(i);
            
            // Send to channel / thread / async task
            sender_channel.send(owned_packet);
        }
    }
});
```

**The Owned `Packet` Struct:**

Unlike `PacketRef`, the `Packet` struct owns the frame resource.

- **Impls `Drop`:** Automatically returns the frame to the Fill Ring if dropped.
    
- **Impls `Send`:** Can be moved to other threads.
    

---

## 6. Configuration Enums

To keep the API self-documenting, behavior is controlled via Enums rather than magic numbers.

Rust

``` rust
/// Controls how the engine waits for new packets.
pub enum Poller {
    /// Burns 100% CPU. Latency: <10us.
    Busy,
    /// Sleeps immediately. Latency: >50us. Saves Power.
    Wait,
    /// Spins for 50us, then sleeps. Best general-purpose balance.
    Adaptive,
}
```

``` rust
/// Controls ring behavior when full.
pub enum CongestionStrategy {
    /// Return an error immediately.
    DropNew,
    /// Block the thread until space is available.
    Block,
}
```

---

## 7. Error Handling

The API uses a unified `FluxError` type to abstract away low-level OS errors (`errno`).

Rust

``` rust
pub enum FluxError {
    /// The kernel does not support XDP on this interface.
    InterfaceNotSupported,
    /// Permission denied (requires CAP_NET_RAW).
    PermissionDenied,
    /// The ring buffer is broken/desynchronized.
    RingCorruption,
    /// Wrapped IO error.
    Io(std::io::Error),
}
```

---

## 2. Mode B: The FluxSystem (Deep Dive)

### 2.1 Philosophy: The "Unshackled" Packet

Unlike the Engine mode, where packet lifetime is strictly bound to a callback scope, **FluxSystem** treats a `Packet` as a standalone resource (a "move semantics" object).

- **Ownership:** The user takes full ownership of the packet struct.
    
- **Mobility:** Packets are `Send`, meaning they can be moved across thread boundaries (e.g., from an RX polling thread to a specialized worker thread).
    
- **Lifecycle:** The packet memory is automatically recycled into the UMEM Fill Ring when the `Packet` struct is dropped or consumed by the TX ring.
    

### 2.2 Architecture: Split-Ring Topology

When `bind_system()` is called, the underlying socket is essentially "cut in half."

1. **FluxRx (Consumer):** Exclusive owner of the Receive Ring and the Fill Ring (used to return empty frames to the kernel).
    
2. **FluxTx (Producer):** Exclusive owner of the Transmit Ring and the Completion Ring (used to reclaim sent frames).
    

This separation allows for **lock-free concurrency**. The RX thread can poll without contending with the TX thread.

### 2.3 Multi-Threaded Patterns

#### Pattern A: The Pipeline (Fan-Out / Fan-In)

This is the standard pattern for complex processing (e.g., Deep Packet Inspection, TCP Termination).

1. **The Ingress Thread (RX):** Holds `FluxRx`. It does nothing but poll the driver, stripping packets off the wire and sending them down a fast channel (e.g., `crossbeam-channel` or `flume`) to worker threads.
    
2. **The Worker Pool:** Multiple threads receive `Packet`s from the Ingress channel. They perform heavy CPU tasks (parsing, crypto, state lookups).
    
3. **The Egress Thread (TX):** Holds `FluxTx`. It listens on an "outgoing" channel. When workers finish, they send the `Packet` to this channel. The Egress thread batches them onto the NIC and manages the `flush` cycle.
    

#### Pattern B: Async Reactor (Tokio/Epoll Integration)

Because `FluxRx` and `FluxTx` expose the underlying File Descriptors (FDs), they can be registered with an `AsyncFd` (Tokio) or `Event` (Mio).

- The `recv()` method becomes `async recv()`.
    
- This allows the network stack to yield execution when the ring is empty, sharing the core with other async tasks (like HTTP handling).
    

---

## 3. Developer API Reference (Expanded)

### 3.1 The System Handles

#### `struct FluxRx`

- **Traits:** `Send`, `!Sync` (Cannot be shared, must be moved).
    
- `fn recv(&mut self, max: usize) -> Vec<Packet>`
    
    - **Behavior:** Non-blocking. Pops up to `max` available descriptors from the RX ring.
        
    - **Returns:** A vector of `Packet` objects owned by the caller.
        
- `fn fd(&self) -> RawFd`
    
    - **Behavior:** Returns the socket FD for polling/event loops (epoll/kqueue).
        

#### `struct FluxTx`

- **Traits:** `Send`, `!Sync`.
    
- `fn send(&mut self, packet: Packet)`
    
    - **Behavior:** Consumes the `Packet`. The underlying memory frame is queued for transmission.
        
    - **Note:** This does _not_ trigger a syscall immediately. It just updates the ring.
        
- `fn flush(&mut self) -> Result<(), FluxError>`
    
    - **Behavior:** Issues the `sendto` syscall (Doorbell). This notifies the NIC that new packets are ready in the TX ring.
        
    - **Optimization:** Call this once per batch, not per packet.
        

#### `struct Packet`

- **Traits:** `Send`, `Sync` (Safe to read concurrently, mutable access requires ownership).
    
- `fn adjust_head(&mut self, offset: i32)`
    
    - **Behavior:** Moves the data pointer forward/backward (e.g., to strip VLAN tags or add encapsulation) without copying memory.
        
- `fn len(&self) -> usize`
    
- `fn into_raw(self) -> UMEMFrame` (Advanced unsafe API).
    

---

## 4. Implementation Details & Safety

### 4.1 The Drop Guard

In **FluxSystem**, the risk of memory leaks is handled via Rust's `Drop` trait.

- If a user drops a `Packet` (e.g., `let _ = packet;` or it goes out of scope), the `impl Drop for Packet` logic fires.
    
- **Drop Logic:** The frame index is securely added back to the **Fill Ring** (via an internal shared producer handle). This ensures that even if a worker thread panics, the packet memory is returned to the NIC to keep the reception loop alive.
    

### 4.2 Thread-Safe Ring Access

While `FluxRx` and `FluxTx` are single-threaded, the UMEM allocators internally use **MPMC (Multi-Producer Multi-Consumer)** semantics for the Fill and Completion rings. This allows:

- A Worker thread to drop a packet (Fill Ring update) safely without locking the main Ingress thread.
    

---

## 5. Usage Example: Multi-Threaded Pipeline

``` rust
use std::thread;
use std::sync::mpsc;

// 1. Setup
let (mut rx, mut tx) = FluxSystem::builder()
    .interface("eth0")
    .queue_id(0)
    .bind_system()?;

// Channels for passing ownership of packets between threads
let (to_worker, from_ingress) = mpsc::sync_channel(1024);
let (to_egress, from_worker) = mpsc::sync_channel(1024);

// 2. Ingress Thread (RX)
thread::spawn(move || {
    loop {
        // Fetch raw batches
        let packets = rx.recv(32); 
        for p in packets {
            // Move packet to worker (fast, pointer move only)
            let _ = to_worker.send(p);
        }
    }
});

// 3. Worker Thread (Heavy Logic)
thread::spawn(move || {
    while let Ok(mut pkt) = from_ingress.recv() {
        // Expensive CPU work
        if is_malicious(&pkt) {
            // Drop explicitly (returns to Fill Ring automatically)
            drop(pkt); 
        } else {
            modify_headers(&mut pkt);
            // Pass to egress
            let _ = to_egress.send(pkt);
        }
    }
});

// 4. Egress Thread (TX)
thread::spawn(move || {
    while let Ok(pkt) = from_worker.recv() {
        tx.send(pkt);
        
        // Smart flushing strategy (e.g., flush if ring is half full or timer ticks)
        if tx.pending() > 16 {
            tx.flush();
        }
    }
});
```

# FluxRaw: The Bare-Metal Mode

## 1. Design Philosophy: "Trust the Developer"

**FluxRaw** is the unfiltered hardware interface. No helpers, no hand-holding, no opinions.

- **Target Audience:** Kernel developers learning AF_XDP, HFT engineers optimizing for nanoseconds, researchers implementing custom zero-copy protocols.
- **Core Promise:** "We give you type-safe access to the raw ring primitives. You control every memory movement, every syscall, every cache line."
- **The Contract:** FluxRaw provides **safety guardrails** (preventing undefined behavior), not **ergonomic guardrails** (preventing bad performance).

---

## 2. Mental Model: The Four-Ring State Machine

Unlike traditional sockets (single pipe metaphor), AF_XDP is a **bidirectional state machine** with explicit memory management.

### 2.1 The Components

```
┌─────────────────────────────────────────────────────────────┐
│                    UMEM (Shared Memory Arena)                │
│  ┌────────┬────────┬────────┬────────┬────────┬────────┐   │
│  │Frame 0 │Frame 1 │Frame 2 │Frame 3 │  ...   │Frame N │   │
│  └────────┴────────┴────────┴────────┴────────┴────────┘   │
└─────────────────────────────────────────────────────────────┘
                              ▲
                              │
        ┌─────────────────────┴─────────────────────┐
        │                                           │
   ┌────▼────┐  ┌──────────┐  ┌──────────┐  ┌──────▼──────┐
   │RX Ring  │  │Fill Ring │  │TX Ring   │  │Completion   │
   │(Consumer)│  │(Producer)│  │(Producer)│  │Ring         │
   │         │  │          │  │          │  │(Consumer)   │
   │Kernel→U │  │U→Kernel  │  │U→Kernel  │  │Kernel→U     │
   └─────────┘  └──────────┘  └──────────┘  └─────────────┘
```

**1. UMEM (The Memory Pool)**

- Single contiguous region, divided into fixed-size frames (e.g., 4096 bytes)
- Shared between kernel and userspace (via `mmap`)
- YOU manage which frames are in use vs. free

**2. RX Ring (Consumer)**

- Kernel writes descriptors here when packets arrive
- Descriptor = `{ frame_index, length }`
- You read descriptors and process the memory

**3. Fill Ring (Producer)**

- You write frame indices here
- Tells kernel: "These frames are empty, use them for incoming packets"
- If this empties, packet reception STOPS

**4. TX Ring (Producer)**

- You write descriptors here when sending packets
- Descriptor = `{ frame_index, length, options }`

**5. Completion Ring (Consumer)**

- Kernel writes frame indices here after transmission completes
- You read these to reclaim memory for reuse

### 2.2 The Invariant

**The Golden Rule:** Every frame is in exactly ONE state at any time:

1. **Free** (in your allocator)
2. **Owned by Kernel** (submitted to Fill/TX ring, not yet consumed)
3. **Owned by User** (received from RX/Completion ring, not yet returned)

Violating this causes silent corruption, packet loss, or kernel panics.

---

## 3. API Structure: Zero-Cost Abstractions

FluxRaw uses Rust's type system to enforce ring invariants WITHOUT runtime overhead.

### 3.1 The Socket Container

```rust
pub struct FluxRaw {
    /// The shared memory region
    pub umem: UmemRegion,
    
    /// The four hardware rings (PUBLIC for full access)
    pub rx: RxRing,
    pub fill: FillRing,
    pub tx: TxRing,
    pub comp: CompletionRing,
    
    // Private: socket file descriptor
    fd: RawFd,
}

impl FluxRaw {
    /// Create socket bound to interface and queue
    pub fn new(interface: &str, queue_id: u32, config: UmemConfig) 
        -> Result<Self, FluxError>;
    
    /// Get the raw file descriptor (for epoll/io_uring integration)
    pub fn as_raw_fd(&self) -> RawFd;
    
    /// Check if kernel needs wakeup (syscall required)
    pub fn needs_wakeup_rx(&self) -> bool;
    pub fn needs_wakeup_tx(&self) -> bool;
    
    /// Explicit syscall to notify kernel (use sparingly)
    pub fn wakeup_rx(&self) -> Result<(), FluxError>;
    pub fn wakeup_tx(&self) -> Result<(), FluxError>;
}
```

**Design Note:** Rings are public fields. No getters. Direct struct access compiles to zero instructions.

---

### 3.2 UMEM: The Memory Abstraction

```rust
pub struct UmemRegion {
    base: *mut u8,
    len: usize,
    frame_size: usize,
    frame_count: usize,
}

impl UmemRegion {
    /// Access frame by index (bounds-checked in debug builds)
    #[inline(always)]
    pub fn frame(&self, index: FrameIndex) -> &[u8] {
        debug_assert!((index as usize) < self.frame_count);
        let offset = index as usize * self.frame_size;
        unsafe {
            std::slice::from_raw_parts(
                self.base.add(offset),
                self.frame_size
            )
        }
    }
    
    /// Mutable access (for packet modification)
    #[inline(always)]
    pub fn frame_mut(&mut self, index: FrameIndex) -> &mut [u8] {
        debug_assert!((index as usize) < self.frame_count);
        let offset = index as usize * self.frame_size;
        unsafe {
            std::slice::from_raw_parts_mut(
                self.base.add(offset),
                self.frame_size
            )
        }
    }
    
    /// Access arbitrary slice within a frame (for parsing)
    /// SAFETY: Caller must ensure addr + len is within bounds
    #[inline(always)]
    pub unsafe fn slice_unchecked(&self, addr: u64, len: usize) -> &[u8] {
        std::slice::from_raw_parts(
            self.base.add(addr as usize),
            len
        )
    }
    
    /// Get frame size configuration
    pub fn frame_size(&self) -> usize { self.frame_size }
    pub fn frame_count(&self) -> usize { self.frame_count }
}
```

**Safety Contract:**

- `frame()` and `frame_mut()` are safe because indices are validated (debug mode)
- `slice_unchecked()` is unsafe - caller must guarantee validity
- All methods inline to pointer arithmetic in release builds

---

### 3.3 Ring Primitives: Producer Pattern

Used by **Fill Ring** and **TX Ring**.

```rust
pub struct ProducerRing {
    // Ring metadata (kernel-shared memory)
    producer: *mut u32,  // Your write pointer
    consumer: *const u32, // Kernel's read pointer (read-only)
    
    // Ring data
    descriptors: *mut u64, // For Fill Ring (just frame indices)
    // OR
    descriptors: *mut XdpDesc, // For TX Ring (addr + len + options)
    
    mask: u32, // Ring size - 1 (for fast modulo)
    cached_consumer: u32, // Cached copy to avoid atomic reads
}

impl ProducerRing {
    /// Reserve N slots for writing
    /// Returns actual number available (may be less than requested)
    pub fn reserve(&mut self, desired: usize) -> Producer<'_> {
        // Read kernel's consumer index (atomic load)
        let cons = unsafe { (*self.consumer).load(Ordering::Acquire) };
        self.cached_consumer = cons;
        
        let prod = unsafe { (*self.producer).load(Ordering::Relaxed) };
        
        // Calculate available space
        let available = (self.mask + 1) - (prod - cons);
        let count = available.min(desired as u32);
        
        Producer {
            ring: self,
            start_idx: prod,
            count,
            written: 0,
        }
    }
    
    /// Check available space without claiming
    pub fn available(&self) -> usize {
        let cons = unsafe { (*self.consumer).load(Ordering::Acquire) };
        let prod = unsafe { (*self.producer).load(Ordering::Relaxed) };
        ((self.mask + 1) - (prod - cons)) as usize
    }
}
```

**The Producer Guard (RAII Commit):**

```rust
pub struct Producer<'a> {
    ring: &'a mut ProducerRing,
    start_idx: u32,
    count: u32,
    written: u32,
}

impl<'a> Producer<'a> {
    /// Write descriptor at relative index (0..count)
    #[inline(always)]
    pub fn write_fill(&mut self, index: usize, frame_idx: FrameIndex) {
        debug_assert!(index < self.count as usize);
        let ring_idx = (self.start_idx + index as u32) & self.ring.mask;
        unsafe {
            *self.ring.descriptors.add(ring_idx as usize) = frame_idx;
        }
    }
    
    /// Write TX descriptor
    #[inline(always)]
    pub fn write_tx(&mut self, index: usize, desc: XdpDesc) {
        debug_assert!(index < self.count as usize);
        let ring_idx = (self.start_idx + index as u32) & self.ring.mask;
        unsafe {
            *self.ring.descriptors.add(ring_idx as usize) = desc;
        }
    }
    
    /// Commit N descriptors (partial commit allowed)
    pub fn commit(self, n: usize) {
        debug_assert!(n <= self.count as usize);
        let new_prod = self.start_idx + n as u32;
        unsafe {
            (*self.ring.producer).store(new_prod, Ordering::Release);
        }
        // Drop without running Drop::drop
        std::mem::forget(self);
    }
    
    /// How many slots were reserved?
    pub fn capacity(&self) -> usize { self.count as usize }
}

impl Drop for Producer<'_> {
    fn drop(&mut self) {
        // Auto-commit all reserved slots if not manually committed
        let new_prod = self.start_idx + self.count;
        unsafe {
            (*self.ring.producer).store(new_prod, Ordering::Release);
        }
    }
}
```

**Key Insight:** The `Producer` guard ensures you cannot forget to update the producer index. Either you call `commit(n)` explicitly, or Drop commits all reserved slots.

---

### 3.4 Ring Primitives: Consumer Pattern

Used by **RX Ring** and **Completion Ring**.

```rust
pub struct ConsumerRing {
    producer: *const u32, // Kernel's write pointer (read-only)
    consumer: *mut u32,   // Your read pointer
    
    descriptors: *const XdpDesc, // RX Ring
    // OR
    descriptors: *const u64, // Completion Ring (just frame indices)
    
    mask: u32,
    cached_producer: u32,
}

impl ConsumerRing {
    /// Claim available descriptors for reading
    pub fn consume(&mut self) -> Consumer<'_> {
        let prod = unsafe { (*self.producer).load(Ordering::Acquire) };
        self.cached_producer = prod;
        
        let cons = unsafe { (*self.consumer).load(Ordering::Relaxed) };
        let count = prod - cons;
        
        Consumer {
            ring: self,
            start_idx: cons,
            count,
        }
    }
    
    /// Peek at available count without consuming
    pub fn available(&self) -> usize {
        let prod = unsafe { (*self.producer).load(Ordering::Acquire) };
        let cons = unsafe { (*self.consumer).load(Ordering::Relaxed) };
        (prod - cons) as usize
    }
}
```

**The Consumer Iterator:**

```rust
pub struct Consumer<'a> {
    ring: &'a mut ConsumerRing,
    start_idx: u32,
    count: u32,
}

impl<'a> Consumer<'a> {
    /// Read descriptor at relative index
    #[inline(always)]
    pub fn read_rx(&self, index: usize) -> XdpDesc {
        debug_assert!(index < self.count as usize);
        let ring_idx = (self.start_idx + index as u32) & self.ring.mask;
        unsafe { *self.ring.descriptors.add(ring_idx as usize) }
    }
    
    /// Read completion frame index
    #[inline(always)]
    pub fn read_comp(&self, index: usize) -> FrameIndex {
        debug_assert!(index < self.count as usize);
        let ring_idx = (self.start_idx + index as u32) & self.ring.mask;
        unsafe { *self.ring.descriptors.add(ring_idx as usize) }
    }
    
    /// Release N descriptors (partial release allowed)
    pub fn release(self, n: usize) {
        debug_assert!(n <= self.count as usize);
        let new_cons = self.start_idx + n as u32;
        unsafe {
            (*self.ring.consumer).store(new_cons, Ordering::Release);
        }
        std::mem::forget(self);
    }
    
    /// How many descriptors are available?
    pub fn len(&self) -> usize { self.count as usize }
}

impl Drop for Consumer<'_> {
    fn drop(&mut self) {
        // Auto-release all consumed descriptors
        let new_cons = self.start_idx + self.count;
        unsafe {
            (*self.ring.consumer).store(new_cons, Ordering::Release);
        }
    }
}
```

---

### 3.5 The Descriptor Types

```rust
/// Frame index (offset in UMEM, usually multiple of frame_size)
pub type FrameIndex = u64;

/// TX/RX Descriptor (16 bytes, cache-line friendly)
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct XdpDesc {
    pub addr: u64,    // Frame offset in UMEM
    pub len: u32,     // Packet length
    pub options: u32, // TX flags (checksum offload, etc.)
}

impl XdpDesc {
    #[inline(always)]
    pub const fn new(addr: u64, len: u32) -> Self {
        Self { addr, len, options: 0 }
    }
    
    #[inline(always)]
    pub const fn with_options(mut self, opts: u32) -> Self {
        self.options = opts;
        self
    }
}

// TX Option Flags
pub const XDP_TXMD_FLAGS_CHECKSUM: u32 = 1 << 0;
```

---

## 4. Usage Patterns: The Hot Loop

### 4.1 Minimal Loop (Learning/Debugging)

This is the "Hello World" of FluxRaw - demonstrates all four rings explicitly.

```rust
use fluxraw::*;

fn main() -> Result<(), FluxError> {
    let mut socket = FluxRaw::new("eth0", 0, UmemConfig::default())?;
    let mut free_list: Vec<FrameIndex> = (0..2048).collect();
    
    // === PRIME THE PUMP ===
    // Give kernel 1024 buffers to receive into
    {
        let mut producer = socket.fill.reserve(1024);
        for i in 0..producer.capacity() {
            let frame = free_list.pop().unwrap();
            producer.write_fill(i, frame);
        }
        // Auto-commit on drop
    }
    
    loop {
        // === 1. RECLAIM SENT FRAMES ===
        {
            let consumer = socket.comp.consume();
            for i in 0..consumer.len() {
                let frame_idx = consumer.read_comp(i);
                free_list.push(frame_idx);
            }
            // Auto-release on drop
        }
        
        // === 2. RECEIVE PACKETS ===
        {
            let consumer = socket.rx.consume();
            for i in 0..consumer.len() {
                let desc = consumer.read_rx(i);
                
                // Access packet data
                let packet = unsafe {
                    socket.umem.slice_unchecked(desc.addr, desc.len as usize)
                };
                
                // Your logic here
                println!("RX: {} bytes", packet.len());
                
                // Decision: Drop (recycle) or Forward (TX)?
                free_list.push(desc.addr); // Dropping
            }
            // Auto-release on drop
        }
        
        // === 3. REFILL RX BUFFERS ===
        if !free_list.is_empty() {
            let mut producer = socket.fill.reserve(32);
            let to_fill = producer.capacity().min(free_list.len());
            
            for i in 0..to_fill {
                let frame = free_list.pop().unwrap();
                producer.write_fill(i, frame);
            }
            // Auto-commit on drop
        }
        
        // === 4. KICK KERNEL (if needed) ===
        if socket.needs_wakeup_rx() {
            socket.wakeup_rx()?;
        }
    }
}
```

---

### 4.2 Optimized Loop (Production)

Demonstrates batching, syscall coalescing, and zero-copy forwarding.

```rust
const BATCH_SIZE: usize = 64;

fn packet_forwarder(mut socket: FluxRaw) -> Result<(), FluxError> {
    let mut free_list: Vec<FrameIndex> = (0..2048).collect();
    let mut pending_tx = Vec::with_capacity(BATCH_SIZE);
    
    // Prime the pump
    {
        let mut fill = socket.fill.reserve(1024);
        for i in 0..fill.capacity() {
            fill.write_fill(i, free_list.pop().unwrap());
        }
    }
    
    loop {
        // === RECLAIM COMPLETED TX ===
        {
            let comp = socket.comp.consume();
            for i in 0..comp.len() {
                free_list.push(comp.read_comp(i));
            }
        }
        
        // === RECEIVE BATCH ===
        let rx_count = {
            let rx = socket.rx.consume();
            let count = rx.len().min(BATCH_SIZE);
            
            for i in 0..count {
                let desc = rx.read_rx(i);
                
                // ZERO-COPY: Reuse frame for TX without copying
                pending_tx.push(desc);
            }
            
            rx.release(count); // Partial release
            count
        };
        
        // === TRANSMIT BATCH ===
        if !pending_tx.is_empty() {
            let mut tx = socket.tx.reserve(pending_tx.len());
            let tx_count = tx.capacity();
            
            for i in 0..tx_count {
                let desc = pending_tx[i];
                
                // Optional: Modify packet in-place
                let packet = socket.umem.frame_mut(desc.addr);
                swap_mac_addresses(packet);
                
                tx.write_tx(i, desc);
            }
            
            pending_tx.drain(..tx_count);
        }
        
        // === REFILL RX ===
        {
            let batch = free_list.len().min(BATCH_SIZE);
            if batch > 0 {
                let mut fill = socket.fill.reserve(batch);
                for i in 0..fill.capacity() {
                    fill.write_fill(i, free_list.pop().unwrap());
                }
            }
        }
        
        // === SYSCALL OPTIMIZATION ===
        // Only wake kernel if needed AND we have work pending
        if socket.needs_wakeup_tx() && pending_tx.is_empty() {
            socket.wakeup_tx()?;
        }
        if socket.needs_wakeup_rx() {
            socket.wakeup_rx()?;
        }
    }
}

fn swap_mac_addresses(packet: &mut [u8]) {
    if packet.len() >= 12 {
        packet[..6].swap_with_slice(&mut packet[6..12]);
    }
}
```

---

### 4.3 Advanced: Custom Allocator

Replace `Vec<FrameIndex>` with a lock-free ring buffer for multi-threaded scenarios.

```rust
use std::sync::atomic::{AtomicU64, Ordering};

/// Lock-free SPSC ring buffer for frame indices
pub struct FrameAllocator {
    frames: Vec<FrameIndex>,
    head: AtomicU64, // Next allocation index
    tail: AtomicU64, // Next free index
    mask: u64,
}

impl FrameAllocator {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two());
        Self {
            frames: (0..capacity as u64).collect(),
            head: AtomicU64::new(0),
            tail: AtomicU64::new(0),
            mask: (capacity - 1) as u64,
        }
    }
    
    /// Allocate a frame (returns None if empty)
    #[inline]
    pub fn alloc(&self) -> Option<FrameIndex> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        
        if head - tail >= self.frames.len() as u64 {
            return None; // Empty
        }
        
        let idx = (head & self.mask) as usize;
        let frame = self.frames[idx];
        
        self.head.store(head + 1, Ordering::Release);
        Some(frame)
    }
    
    /// Free a frame (must not be called concurrently)
    #[inline]
    pub fn free(&self, frame: FrameIndex) {
        let tail = self.tail.load(Ordering::Relaxed);
        let idx = (tail & self.mask) as usize;
        
        // SAFETY: Only one thread can free (producer owns this)
        unsafe {
            let ptr = self.frames.as_ptr() as *mut FrameIndex;
            *ptr.add(idx) = frame;
        }
        
        self.tail.store(tail + 1, Ordering::Release);
    }
}
```

---

## 5. Performance Considerations

### 5.1 Batching

**Rule:** Never call syscalls inside a `for` loop.

```rust
// ❌ BAD: 64 syscalls
for desc in rx.consume() {
    // process...
}
socket.wakeup_rx()?; // Per-packet syscall

// ✅ GOOD: 1 syscall per batch
let rx = socket.rx.consume();
for i in 0..rx.len() {
    // process...
}
drop(rx); // Release all at once
socket.wakeup_rx()?; // One syscall
```

### 5.2 Cache Line Awareness

Descriptors are 16 bytes. Fetch them in multiples of 4 for cache-line alignment (64 bytes).

```rust
const CACHE_LINE_DESCS: usize = 4; // 64 bytes / 16 bytes

let rx = socket.rx.consume();
let count = rx.len();

// Process in cache-line aligned chunks
for chunk_start in (0..count).step_by(CACHE_LINE_DESCS) {
    let chunk_end = (chunk_start + CACHE_LINE_DESCS).min(count);
    
    for i in chunk_start..chunk_end {
        let desc = rx.read_rx(i);
        // All 4 descriptors are hot in L1 cache
    }
}
```

### 5.3 Syscall Avoidance

The kernel sets `XDP_RING_NEED_WAKEUP` flag when it goes to sleep.

```rust
// Check flag before syscall
if socket.needs_wakeup_rx() {
    socket.wakeup_rx()?;
}

// In tight loops, check less frequently
static mut LOOP_COUNTER: u64 = 0;
unsafe {
    LOOP_COUNTER += 1;
    if LOOP_COUNTER % 1000 == 0 && socket.needs_wakeup_rx() {
        socket.wakeup_rx()?;
    }
}
```

### 5.4 Memory Prefetching

Hint the CPU to load packet data before processing.

```rust
use std::arch::x86_64::_mm_prefetch;

let rx = socket.rx.consume();
for i in 0..rx.len() {
    let desc = rx.read_rx(i);
    
    // Prefetch next packet while processing current
    if i + 1 < rx.len() {
        let next_desc = rx.read_rx(i + 1);
        unsafe {
            let next_ptr = socket.umem.frame(next_desc.addr).as_ptr();
            _mm_prefetch(next_ptr as *const i8, 3); // PREFETCH_T0
        }
    }
    
    // Process current packet (data is hot in cache)
    let packet = socket.umem.frame(desc.addr);
    process(packet);
}
```

---

## 6. Integration Patterns

### 6.1 Async/Tokio Integration

```rust
use tokio::io::unix::AsyncFd;

pub struct AsyncFluxRaw {
    socket: FluxRaw,
    async_fd: AsyncFd<RawFd>,
}

impl AsyncFluxRaw {
    pub fn new(socket: FluxRaw) -> io::Result<Self> {
        let fd = socket.as_raw_fd();
        let async_fd = AsyncFd::new(fd)?;
        Ok(Self { socket, async_fd })
    }
    
    pub async fn recv_batch(&mut self) -> Result<Vec<XdpDesc>, FluxError> {
        loop {
            // Try non-blocking receive
            let rx = self.socket.rx.consume();
            if rx.len() > 0 {
                let mut batch = Vec::with_capacity(rx.len());
                for i in 0..rx.len() {
                    batch.push(rx.read_rx(i));
                }
                return Ok(batch);
            }
            
            // Wait for readability
            let mut guard = self.async_fd.readable().await?;
            guard.clear_ready();
            
            // Kick kernel if needed
            if self.socket.needs_wakeup_rx() {
                self.socket.wakeup_rx()?;
            }
        }
    }
}
```

### 6.2 Multi-Queue (RSS) Pattern

Bind one socket per CPU core, pin threads.

```rust
use std::thread;
use core_affinity::CoreId;

fn main() {
    let core_ids = core_affinity::get_core_ids().unwrap();
    
    for (queue_id, core_id) in core_ids.iter().enumerate() {
        thread::spawn(move || {
            // Pin thread to core
            core_affinity::set_for_current(*core_id);
            
            // Bind socket to hardware queue
            let socket = FluxRaw::new("eth0", queue_id as u32, 
                                      UmemConfig::default()).unwrap();
            
            // Run isolated hot loop (no cross-core synchronization)
            packet_loop(socket);
        });
    }
}
```

---

## 7. Safety Guarantees & Unsafe Boundaries

### 7.1 What FluxRaw Prevents

✅ **Ring Index Overflow:** Masking is handled internally  
✅ **Producer/Consumer Desync:** RAII guards ensure atomic updates  
✅ **Double-Commit:** `commit()` consumes the guard  
✅ **Forgetting to Commit:** Drop impl commits automatically

### 7.2 What YOU Must Guarantee

❌ **Frame Lifecycle:** Don't submit a frame to TX while it's in Fill ring  
❌ **UMEM Bounds:** Don't access `frame(9999)` if you only have 2048 frames  
❌ **Concurrent Access:** Don't share `FluxRaw` across threads (use channels)

### 7.3 The Unsafe Surface Area

All `unsafe` code is marked explicitly:

```rust
// SAFE (bounds-checked in debug)
let packet = socket.umem.frame(desc.addr);

// UNSAFE (caller guarantees validity)
let packet = unsafe {
    socket.umem.slice_unchecked(desc.addr, desc.len as usize)
};
```

**When to use `slice_unchecked()`:**

- You've validated the descriptor came from the kernel
- Hot path where debug assertions are too slow
- You're implementing a custom validation layer

---

## 8. Debugging & Verification

### 8.1 Ring State Inspection

```rust
impl FluxRaw {
    /// Dump current ring state (for debugging)
    pub fn debug_rings(&self) {
        println!("RX: {} available", self.rx.available());
        println!("FILL: {} free slots", self.fill.available());
        println!("TX: {} free slots", self.tx.available());
        println!("COMP: {} completed", self.comp.available());
    }
}
```

### 8.2 Common Pitfalls

**Problem:** Packets stop arriving  
**Cause:** Fill ring is empty (kernel has no buffers)  
**Fix:** Monitor `fill.available()`, maintain minimum threshold

**Problem:** TX ring fills up, `reserve()` returns 0  
**Cause:** Not consuming completion ring  
**Fix:** Process completions before every TX batch

**Problem:** Silent corruption or wrong packet data  
**Cause:** Reusing a frame before kernel finished with it  
**Fix:** Only free frames from RX/Completion rings

---

## 9. Comparison with Other Modes

|Feature|FluxRaw|FluxEngine|FluxSystem|
|---|---|---|---|
|**Ring Access**|Direct public fields|Hidden, batched|RAII handles|
|**Allocator**|Manual (you provide)|Internal (fast path)|RAII (Drop trait)|
|**Syscall Control**|Explicit `wakeup_*()`|Auto-coalesced|Manual `flush()`|
|**Packet Lifetime**|No abstraction|Scoped to batch|Owned (Send)|
|**Error on Full Ring**|`reserve()` returns less|Blocks or errors|Blocks or errors|
|**Async Support**|Manual (AsyncFd)|N/A (sync only)|Built-in (Tokio)|
|**Use Case**|Learning, HFT, research|High-throughput filters|Complex stateful apps|

---

## 10. When to Use FluxRaw

**Choose FluxRaw when:**

- You're implementing a kernel bypass tutorial or research paper
- You need sub-microsecond latency (HFT, market data)
- You're benchmarking different allocation strategies
- You're integrating with custom async runtimes (not Tokio)
- You need to instrument/trace every ring operation

**Don't use FluxRaw if:**

- You just want fast packet processing → Use FluxEngine
- You need background workers or async → Use FluxSystem
- You're prototyping → Start with FluxEngine, optimize later

---

## 11. Complete Example: Minimal Firewall

```rust
use fluxraw::*;

fn main() -> Result<(), FluxError> {
    let mut socket = FluxRaw::new("eth0", 0, UmemConfig {
        frame_size: 2048,
        frame_count: 2048,
        ..Default::default()
    })?;
    
    let mut free_list: Vec<FrameIndex> = (0..2048).collect();
    
    // Prime Fill ring
    {
        let mut fill = socket.fill.reserve(1024);
        for i in 0..fill.capacity() {
            fill.write_fill(i, free_list.pop().unwrap());
        }
    }
    
    println!("Firewall running on eth0, press Ctrl+C to stop");
    
    loop {
        // Reclaim TX completions
        {
            let comp = socket.comp.consume();
            for i in 0..comp.len() {
                free_list.push(comp.read_comp(i));
            }
        }
        
        // Process RX batch
        {
            let rx = socket.rx.consume();
            for i in 0..rx.len() {
                let desc = rx.read_rx(i);
                let packet = unsafe {
                    socket.umem.slice_unchecked(desc.addr, desc.len as usize)
                };
                
                // Simple IPv4 firewall logic
                if packet.len() >= 14 {
                    let ethertype = u16::from_be_bytes([packet[12], packet[13]]);
                    
                    if ethertype == 0x0800 { // IPv4
                        println!("ALLOW: IPv4 packet ({} bytes)", packet.len());
                        // Forward to TX (zero-copy)
                        let mut tx = socket.tx.reserve(1);
                        if tx.capacity() > 0 {
                            tx.write_tx(0, desc);
                        }
                    } else {
                        println!("DROP: Non-IPv4 packet");
                        free_list.push(desc.addr); // Recycle
                    }
                } else {
                    free_list.push(desc.addr); // Malformed
                }
            }
        }
        
        // Refill RX
        {
            let to_fill = free_list.len().min(socket.fill.available());
            if to_fill > 0 {
                let mut fill = socket.fill.reserve(to_fill);
                for i in 0..fill.capacity() {
                    fill.write_fill(i, free_list.pop().unwrap());
                }
            }
        }
        
        // Kick kernel
        if socket.needs_wakeup_rx() {
            socket.wakeup_rx()?;
        }
        if socket.needs_wakeup_tx() {
            socket.wakeup_tx()?;
        }
    }
}
```

---

## Summary

FluxRaw gives you **direct ring access** with **zero-cost safety guardrails**:

1. **RAII Guards** prevent forgetting to commit/release
2. **Type System** prevents ring index errors
3. **Explicit Control** over every syscall and memory movement
4. **Public Fields** compile to zero abstraction overhead

You write the allocator. You manage the hot loop. You control the performance.

**"We trust you to go fast. We prevent you from going unsafe."**
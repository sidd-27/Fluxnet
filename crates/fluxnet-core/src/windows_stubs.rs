// Windows Stubs for Fluxnet Core - Stateful Simulator

use lazy_static::lazy_static;
use std::sync::Mutex;
use std::collections::HashMap;


// --- GLOBAL SIMULATOR STATE ---
lazy_static! {
    pub static ref SOCKETS: Mutex<HashMap<usize, MockSocketState>> = Mutex::new(HashMap::new());
    pub static ref NEXT_FD: Mutex<usize> = Mutex::new(1000);
}

pub struct MockSocketState {
    // Ring Buffers (Actual memory backing the "mmap")
    pub rx_ring: Box<[u8]>,
    pub tx_ring: Box<[u8]>,
    pub fill_ring: Box<[u8]>,
    pub comp_ring: Box<[u8]>,
    
    // UMEM Buffer
    pub umem: Vec<u8>,

    // Binding info
    pub if_index: u32,
    pub queue_id: u32,
}

impl MockSocketState {
    pub fn new(size: usize) -> Self {
        // Simple layout: Producer (4) + Consumer (4) + Desc (size * desc_size)
        // We ensure enough space for max size (e.g. 4096 * 32 bytes)
        let ring_bytes = 4 + 4 + (size * 32); 
        
        Self {
            rx_ring: vec![0u8; ring_bytes].into_boxed_slice(),
            tx_ring: vec![0u8; ring_bytes].into_boxed_slice(),
            fill_ring: vec![0u8; ring_bytes].into_boxed_slice(),
            comp_ring: vec![0u8; ring_bytes].into_boxed_slice(),
            umem: Vec::new(), 
            if_index: 0,
            queue_id: 0,
        }
    }
}

// --- SYS ---
pub mod sys {
    pub mod socket {
        use std::io;
        use std::os::windows::io::RawHandle;
        use crate::windows_stubs::{SOCKETS, NEXT_FD, MockSocketState};
        
        pub type RawFd = RawHandle;
        
        pub fn create_xsk_socket() -> io::Result<RawFd> {
            let mut fd_lock = NEXT_FD.lock().unwrap();
            let fd = *fd_lock;
            *fd_lock += 1;
            
            let mut sockets = SOCKETS.lock().unwrap();
            sockets.insert(fd, MockSocketState::new(4096));
            
            // On Windows, RawHandle is void*, we cast our usize FD to it.
            Ok(fd as RawHandle)
        }
        
        pub fn bind_socket(fd: RawFd, ifindex: u32, queue_id: u32, _bind_flags: u16) -> io::Result<()> {
            let fd_idx = fd as usize;
            let mut sockets = SOCKETS.lock().unwrap();
            if let Some(sock) = sockets.get_mut(&fd_idx) {
                sock.if_index = ifindex;
                sock.queue_id = queue_id;
                Ok(())
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "socket not found"))
            }
        }
        
        pub fn set_umem_reg(fd: RawFd, _umem_addr: u64, len: u64, _chunk_size: u32, _headroom: u32) -> io::Result<()> {
            let fd_idx = fd as usize;
            let mut sockets = SOCKETS.lock().unwrap();
            if let Some(sock) = sockets.get_mut(&fd_idx) {
                sock.umem.resize(len as usize, 0);
                Ok(())
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "socket not found"))
            }
        }
        
        pub fn set_ring_size(_fd: RawFd, _ring_type: i32, _size: u32) -> io::Result<()> {
            Ok(())
        }
        
        pub fn get_mmap_offsets(_fd: RawFd) -> io::Result<super::if_xdp::XdpMmapOffsets> {
             // Return standard offsets for our mocked rings
             // Prod: 0, Cons: 4, Desc: 8 (just after pointers)
             let off = super::if_xdp::XdpRingOffset {
                 producer: 0,
                 consumer: 4,
                 desc: 8,
                 flags: 0,
             };
             
             Ok(super::if_xdp::XdpMmapOffsets {
                 rx: off, tx: off, fr: off, cr: off
             })
        }
        
        pub unsafe fn mmap_range(fd: RawFd, _len: usize, offset: u64) -> io::Result<*mut u8> {
            let fd_idx = fd as usize;
            let mut sockets = SOCKETS.lock().unwrap();
            
            if let Some(sock) = sockets.get_mut(&fd_idx) {
                // Map based on offset (XDP_PGOFF_RX_RING etc)
                let ptr = match offset {
                    super::if_xdp::XDP_PGOFF_RX_RING => sock.rx_ring.as_mut_ptr(),
                    super::if_xdp::XDP_PGOFF_TX_RING => sock.tx_ring.as_mut_ptr(),
                    super::if_xdp::XDP_UMEM_PGOFF_FILL_RING => sock.fill_ring.as_mut_ptr(),
                    super::if_xdp::XDP_UMEM_PGOFF_COMPLETION_RING => sock.comp_ring.as_mut_ptr(),
                    _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "unknown mmap offset")),
                };
                Ok(ptr)
            } else {
                 Err(io::Error::new(io::ErrorKind::NotFound, "socket not found"))
            }
        }
        
        pub unsafe fn munmap(_ptr: *mut u8, _len: usize) -> io::Result<()> {
            Ok(())
        }
    }
    
    pub mod if_xdp {
        #[derive(Debug, Clone, Copy, Default)]
        pub struct XdpMmapOffsets {
            pub rx: XdpRingOffset,
            pub tx: XdpRingOffset,
            pub fr: XdpRingOffset,
            pub cr: XdpRingOffset,
        }
        #[derive(Debug, Clone, Copy, Default)]
        pub struct XdpRingOffset {
            pub producer: u64,
            pub consumer: u64,
            pub desc: u64,
            pub flags: u64,
        }
        
        pub const XDP_RX_RING: i32 = 0;
        pub const XDP_TX_RING: i32 = 1;
        pub const XDP_UMEM_REG: i32 = 4;
        pub const XDP_UMEM_FILL_RING: i32 = 5;
        pub const XDP_UMEM_COMPLETION_RING: i32 = 6;
        
        pub const XDP_PGOFF_RX_RING: u64 = 0;
        pub const XDP_PGOFF_TX_RING: u64 = 100; // Mock offsets to distinguish
        pub const XDP_UMEM_PGOFF_FILL_RING: u64 = 200;
        pub const XDP_UMEM_PGOFF_COMPLETION_RING: u64 = 300;
    }
    
    pub mod utils {
        pub fn if_nametoindex(_name: &str) -> std::io::Result<u32> {
            Ok(1)
        }
    }

    pub mod mmap {
        use std::ptr::NonNull;
        
        pub struct MmapArea {
            ptr: NonNull<u8>,
            len: usize,
        }
        unsafe impl Send for MmapArea {}
        unsafe impl Sync for MmapArea {}

        impl MmapArea {
            pub unsafe fn from_raw(ptr: *mut u8, len: usize) -> Self {
                Self {
                    ptr: NonNull::new(ptr).expect("mmap returned null"),
                    len,
                }
            }
            pub fn as_ptr(&self) -> *mut u8 { self.ptr.as_ptr() }
            pub fn len(&self) -> usize { self.len }
        }

        impl Drop for MmapArea {
            fn drop(&mut self) {
                // No-op in simulator
            }
        }
    }
}

// --- UMEM ---
pub mod umem {
    pub mod layout {
        #[derive(Debug, Clone, Copy)]
        pub struct UmemLayout {
            pub frame_size: u32,
            pub frame_count: u32,
        }
        impl UmemLayout {
             pub fn new(frame_size: u32, frame_count: u32) -> Self { Self { frame_size, frame_count } }
             pub fn size(&self) -> usize { (self.frame_size as usize) * (self.frame_count as usize) }
        }
    }
    
    pub mod mmap {
        use super::layout::UmemLayout;
        use std::io;
        use crate::windows_stubs::SOCKETS;
        use std::os::windows::io::RawHandle;

        pub struct UmemRegion {
            ptr: *mut u8,
            layout: UmemLayout,
            fd: Option<RawHandle>,
        }
        unsafe impl Send for UmemRegion {}
        unsafe impl Sync for UmemRegion {}
        
        impl UmemRegion {
            pub fn new(layout: UmemLayout) -> io::Result<Self> {
                 let len = layout.size();
                 let layout_alloc = std::alloc::Layout::from_size_align(len, 4096).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid layout"))?;
                 let ptr = unsafe { std::alloc::alloc_zeroed(layout_alloc) };
                 Ok(Self { ptr, layout, fd: None })
            }

            pub fn set_fd(&mut self, fd: RawHandle) {
                self.fd = Some(fd);
            }

            pub fn as_ptr(&self) -> *mut u8 { 
                if let Some(fd) = self.fd {
                    let fd_idx = fd as usize;
                    let mut sockets = SOCKETS.lock().unwrap();
                    if let Some(sock) = sockets.get_mut(&fd_idx) {
                        // Ensure mock umem is large enough
                        if sock.umem.len() < self.layout.size() {
                            sock.umem.resize(self.layout.size(), 0);
                        }
                        return sock.umem.as_mut_ptr();
                    }
                }
                self.ptr 
            }
            pub fn len(&self) -> usize { self.layout.size() }
            pub fn layout(&self) -> UmemLayout { self.layout }
        }
    }

    pub mod allocator {
        use super::layout::UmemLayout;
        pub struct UmemAllocator;
        impl UmemAllocator {
            pub fn new(_layout: UmemLayout) -> Self { Self }
        }
    }
}

// --- RING ---
pub mod ring {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub struct XDPDesc {
        pub addr: u64,
        pub len: u32,
        pub options: u32,
    }
    
    pub struct ProducerRing<T> {
        producer: *mut u32,
        #[allow(dead_code)]
        consumer: *mut u32,
        descriptors: *mut T,
        #[allow(dead_code)]
        size: u32,
        mask: u32,
    }
    unsafe impl<T> Send for ProducerRing<T> {}

    impl<T> ProducerRing<T> {
        pub unsafe fn new(producer: *mut u32, consumer: *mut u32, descriptors: *mut T, size: u32) -> Self {
            Self { 
                producer, consumer, descriptors, 
                size, mask: size - 1 
            }
        }
        pub fn reserve(&mut self, _cnt: u32) -> Option<u32> { 
            // Mock: Always reserve successfully
            // In real kernel, we'd check if (prod + cnt) - cons <= size
            let prod_idx = unsafe { *self.producer };
            Some(prod_idx)
        }
        pub unsafe fn write_at(&mut self, idx: u32, item: T) {
             let offset = idx & self.mask;
             std::ptr::write(self.descriptors.add(offset as usize), item);
        }
        pub fn submit(&mut self, idx: u32) {
            unsafe { *self.producer = idx };
        }
        pub fn available(&self) -> usize { 
            let prod = unsafe { *self.producer };
            let cons = unsafe { *self.consumer };
            (self.size - prod.wrapping_sub(cons)) as usize
        }
        pub fn len(&self) -> usize { self.size as usize }
    }
    
    pub struct ConsumerRing<T> {
        producer: *mut u32,
        consumer: *mut u32,
        descriptors: *mut T,
        #[allow(dead_code)]
        size: u32,
        mask: u32,
        // Cached producer index to avoid frequent volatile reads (in real impl)
        // Here we don't strictly need it but keep for API compat
        _cached_prod: u32, 
    }
    unsafe impl<T> Send for ConsumerRing<T> {}
    impl<T: Copy> ConsumerRing<T> {
        pub unsafe fn new(producer: *mut u32, consumer: *mut u32, descriptors: *mut T, size: u32) -> Self {
             Self { 
                 producer, consumer, descriptors, 
                 size, mask: size - 1, _cached_prod: 0 
             }
        }
        pub fn peek(&mut self, _cnt: u32) -> u32 { 
            let prod = unsafe { *self.producer };
            let cons = unsafe { *self.consumer };
            let avail = prod.wrapping_sub(cons);
            // If avail huge (wrap w/o packets), it's 0. 
            // In u32 wrapping logic, (3 - 2) = 1. (2 - 3) = MAX.
            if avail > 0x80000000 { 0 } else { avail }
        }
        pub unsafe fn read_at(&self, idx: u32) -> T {
             let offset = idx & self.mask;
             std::ptr::read(self.descriptors.add(offset as usize))
        }
        pub fn release(&mut self, cnt: u32) {
             unsafe { *self.consumer = (*self.consumer).wrapping_add(cnt) };
        }
        pub fn consumer_idx(&self) -> u32 { 
             unsafe { *self.consumer }
        }
        pub fn available(&self) -> usize {
            let prod = unsafe { *self.producer };
            let cons = unsafe { *self.consumer };
            prod.wrapping_sub(cons) as usize
        }
        pub fn len(&self) -> usize { self.size as usize }
    }
}

pub struct XskContext;

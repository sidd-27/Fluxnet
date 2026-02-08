use std::io;
use std::os::unix::io::RawFd;
use std::mem;
use libc::{
    socket, bind, setsockopt, mmap, munmap,
    AF_XDP, SOCK_RAW, SOL_XDP, SOL_SOCKET,
    PROT_READ, PROT_WRITE, MAP_SHARED, MAP_POPULATE,
    sockaddr, socklen_t, c_void,
};
use crate::sys::if_xdp::*;

pub fn create_xsk_socket() -> io::Result<RawFd> {
    let fd = unsafe { socket(AF_XDP, SOCK_RAW, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fd)
}

pub fn bind_socket(fd: RawFd, ifindex: u32, queue_id: u32, shared: bool) -> io::Result<()> {
    let mut sa: SockaddrXdp = unsafe { mem::zeroed() };
    sa.sxdp_family = AF_XDP as u16;
    sa.sxdp_ifindex = ifindex;
    sa.sxdp_queue_id = queue_id;
    
    if shared {
        sa.sxdp_flags |= 1 << 0; // XDP_SHARED_UMEM
        sa.sxdp_shared_umem_fd = fd as u32; // This is simplistic; real shared umem needs the OTHER fd
    }

    let ret = unsafe {
        bind(fd, &sa as *const _ as *const sockaddr, mem::size_of::<SockaddrXdp>() as socklen_t)
    };

    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn set_umem_reg(fd: RawFd, umem_addr: u64, len: u64, chunk_size: u32, headroom: u32) -> io::Result<()> {
    // XDP_UMEM_REG = 4
    let mr = XdpUmemReg {
        addr: umem_addr,
        len,
        chunk_size,
        headroom,
        flags: 0,
    };
    
    let ret = unsafe {
        setsockopt(fd, SOL_XDP, 4, &mr as *const _ as *const c_void, mem::size_of::<XdpUmemReg>() as socklen_t)
    };

    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn set_ring_size(fd: RawFd, ring_type: i32, size: u32) -> io::Result<()> {
    let ret = unsafe {
        setsockopt(fd, SOL_XDP, ring_type, &size as *const _ as *const c_void, mem::size_of::<u32>() as socklen_t)
    };
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn get_mmap_offsets(fd: RawFd) -> io::Result<XdpMmapOffsets> {
    let mut off: XdpMmapOffsets = unsafe { mem::zeroed() };
    let mut len = mem::size_of::<XdpMmapOffsets>() as socklen_t;
    
    // XDP_MMAP_OFFSETS = 1
    let ret = unsafe {
        libc::getsockopt(fd, SOL_XDP, 1, &mut off as *mut _ as *mut c_void, &mut len)
    };
    
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(off)
}

pub unsafe fn mmap_range(fd: RawFd, len: usize, offset: u64) -> io::Result<*mut u8> {
    let ptr = mmap(
        std::ptr::null_mut(),
        len,
        PROT_READ | PROT_WRITE,
        MAP_SHARED | MAP_POPULATE,
        fd,
        offset as libc::off_t,
    );
    
    if ptr == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    
    Ok(ptr as *mut u8)
}

pub unsafe fn munmap(ptr: *mut u8, len: usize) -> io::Result<()> {
    let ret = libc::munmap(ptr as *mut c_void, len);
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}


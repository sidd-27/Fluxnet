use crate::system::rx::FluxRx;
use crate::system::tx::FluxTx;
use crate::packet::Packet;
use std::io;
use std::task::{Context, Poll};

#[cfg(all(target_os = "linux", feature = "async"))]
use tokio::io::unix::AsyncFd;

/// Asynchronous wrapper for FluxRx
pub struct AsyncFluxRx {
    inner: FluxRx,
    #[cfg(all(target_os = "linux", feature = "async"))]
    async_fd: AsyncFd<std::os::unix::io::RawFd>,
}

impl AsyncFluxRx {
    #[cfg(all(target_os = "linux", feature = "async"))]
    pub fn new(inner: FluxRx) -> io::Result<Self> {
        let fd = inner.fd() as std::os::unix::io::RawFd;
        Ok(Self {
            inner,
            async_fd: AsyncFd::new(fd)?,
        })
    }

    #[cfg(all(not(target_os = "linux"), feature = "async"))]
    pub fn new(inner: FluxRx) -> io::Result<Self> {
        Ok(Self { inner })
    }

    pub async fn recv(&mut self, max: usize) -> io::Result<Vec<Packet>> {
        #[cfg(all(target_os = "linux", feature = "async"))]
        {
            loop {
                let mut guard = self.async_fd.readable().await?;
                let packets = self.inner.recv(max);
                if !packets.is_empty() {
                    return Ok(packets);
                }
                guard.clear_ready();
            }
        }
        #[cfg(all(not(target_os = "linux"), feature = "async"))]
        {
            // In simulator, just poll once.
            // A better mock would yield if empty.
            Ok(self.inner.recv(max))
        }
    }

    pub fn poll_recv(&mut self, cx: &mut Context<'_>, max: usize) -> Poll<io::Result<Vec<Packet>>> {
         #[cfg(all(target_os = "linux", feature = "async"))]
         {
            match self.async_fd.poll_read_ready(cx) {
                Poll::Ready(Ok(mut guard)) => {
                    let packets = self.inner.recv(max);
                    if packets.is_empty() {
                        guard.clear_ready();
                        Poll::Pending
                    } else {
                        Poll::Ready(Ok(packets))
                    }
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
         }
         #[cfg(all(not(target_os = "linux"), feature = "async"))]
         {
             let _ = cx;
             Poll::Ready(Ok(self.inner.recv(max)))
         }
    }
}

/// Asynchronous wrapper for FluxTx
pub struct AsyncFluxTx {
    inner: FluxTx,
    #[cfg(all(target_os = "linux", feature = "async"))]
    async_fd: AsyncFd<std::os::unix::io::RawFd>,
}

impl AsyncFluxTx {
    #[cfg(all(target_os = "linux", feature = "async"))]
    pub fn new(inner: FluxTx) -> io::Result<Self> {
        let fd = inner.fd() as std::os::unix::io::RawFd;
        Ok(Self {
            inner,
            async_fd: AsyncFd::new(fd)?,
        })
    }

    #[cfg(all(not(target_os = "linux"), feature = "async"))]
    pub fn new(inner: FluxTx) -> io::Result<Self> {
        Ok(Self { inner })
    }

    pub fn send(&mut self, packet: Packet) {
        self.inner.send(packet);
    }

    // Flush TX ring to NIC
    pub async fn flush(&mut self) -> io::Result<()> {
        #[cfg(all(target_os = "linux", feature = "async"))]
        {
            let mut guard = self.async_fd.writable().await?;
            // On Linux, we might need a syscall if NEEDS_WAKEUP is set.
            // FluxTx doesn't have a public wakeup() yet, but we should add it.
            guard.clear_ready();
            Ok(())
        }
        #[cfg(all(not(target_os = "linux"), feature = "async"))]
        {
            Ok(())
        }
    }
}

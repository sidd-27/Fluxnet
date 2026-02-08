#[cfg(target_os = "linux")]
pub mod sys;
#[cfg(target_os = "linux")]
pub mod umem;
#[cfg(target_os = "linux")]
pub mod ring;

// Expose real ring implementation for testing on non-Linux platforms
#[cfg(all(test, not(target_os = "linux")))]
#[path = "ring/mod.rs"]
pub mod real_ring;

#[cfg(not(target_os = "linux"))]
pub mod windows_stubs;

#[cfg(not(target_os = "linux"))]
pub use windows_stubs::*;

#[cfg(target_os = "linux")]
pub struct XskContext;

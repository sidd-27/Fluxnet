#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct XDPDesc {
    pub addr: u64,
    pub len: u32,
    pub options: u32,
}

use std::marker::PhantomData;
use std::slice;

/// A zero-copy view into a packet existing in UMEM.
/// 
/// This struct is tied to the lifetime of the batch processing loop 'a.
/// It cannot outlive the batch.
#[allow(dead_code)]
pub struct PacketRef<'a> {
    ptr: *mut u8,
    len: usize,
    addr: u64,
    _marker: PhantomData<&'a mut [u8]>,
    action: &'a mut Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Drop,
    Tx,
}

#[allow(dead_code)]
impl<'a> PacketRef<'a> {
    /// # Safety
    /// The pointer must be valid and point to a UMEM frame.
    /// The lifetime 'a must ensure exclusive access during the batch.
    pub unsafe fn new(ptr: *mut u8, len: usize, addr: u64, action: &'a mut Action) -> Self {
        Self {
            ptr,
            len,
            addr,
            _marker: PhantomData,
            action, 
        }
    }

    #[inline(always)]
    pub fn data(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }

    #[inline(always)]
    pub fn data_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr, self.len) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn set_len(&mut self, len: usize) {
        // TODO: Validate against frame size
        self.len = len;
    }

    #[inline]
    pub fn send(&mut self) {
        *self.action = Action::Tx;
    }

    #[inline]
    pub fn drop(&mut self) {
        *self.action = Action::Drop;
    }
    
    // Internal accessors for the engine
    pub(crate) fn action(&self) -> Action {
        *self.action
    }
    
    pub(crate) fn addr(&self) -> u64 {
        self.addr
    }
    
    // Header parsing helpers
    pub fn ethernet(&self) -> Option<&fluxnet_proto::EthHeader> {
        fluxnet_proto::parse_eth(self.data()).map(|(h, _)| h)
    }
    
    pub fn ipv4(&self) -> Option<&fluxnet_proto::Ipv4Header> {
        let (_, payload) = fluxnet_proto::parse_eth(self.data())?;
        fluxnet_proto::parse_ipv4(payload).map(|(h, _)| h)
    }
}


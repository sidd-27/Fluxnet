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

    /// Move the start of the packet buffer by `offset` bytes.
    /// Positive offset shrinks the packet (strips header).
    /// Negative offset expands the packet (adds header), assuming headroom exists.
    #[inline]
    pub fn adjust_head(&mut self, offset: isize) {
        if offset > 0 {
             let u_off = offset as usize;
             if u_off <= self.len {
                 unsafe { self.ptr = self.ptr.add(u_off) };
                 self.len -= u_off;
             } else {
                 self.len = 0;
             }
        } else {
             let u_off = (-offset) as usize;
             unsafe { self.ptr = self.ptr.sub(u_off) };
             self.len += u_off;
        }
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
    pub fn ethernet(&self) -> Option<&fluxcapacitor_proto::EthHeader> {
        fluxcapacitor_proto::parse_eth(self.data()).map(|(h, _)| h)
    }
    
    pub fn ipv4(&self) -> Option<&fluxcapacitor_proto::Ipv4Header> {
        let (_, payload) = fluxcapacitor_proto::parse_eth(self.data())?;
        fluxcapacitor_proto::parse_ipv4(payload).map(|(h, _)| h)
    }

    pub fn udp(&self) -> Option<&fluxcapacitor_proto::UdpHeader> {
        let (_, ip_payload) = fluxcapacitor_proto::parse_eth(self.data())?;
        let (ip_header, l4_payload) = fluxcapacitor_proto::parse_ipv4(ip_payload)?;
        
        if ip_header.proto != 17 { // UDP
            return None;
        }
        
        fluxcapacitor_proto::parse_udp(l4_payload).map(|(h, _)| h)
    }

    pub fn tcp(&self) -> Option<&fluxcapacitor_proto::TcpHeader> {
        let (_, ip_payload) = fluxcapacitor_proto::parse_eth(self.data())?;
        let (ip_header, l4_payload) = fluxcapacitor_proto::parse_ipv4(ip_payload)?;
        
        if ip_header.proto != 6 { // TCP
            return None;
        }
        
        fluxcapacitor_proto::parse_tcp(l4_payload).map(|(h, _)| h)
    }

    pub fn icmp(&self) -> Option<&fluxcapacitor_proto::IcmpHeader> {
        let (_, ip_payload) = fluxcapacitor_proto::parse_eth(self.data())?;
        let (ip_header, l4_payload) = fluxcapacitor_proto::parse_ipv4(ip_payload)?;
        
        if ip_header.proto != 1 { // ICMP
            return None;
        }
        
        fluxcapacitor_proto::parse_icmp(l4_payload).map(|(h, _)| h)
    }
}


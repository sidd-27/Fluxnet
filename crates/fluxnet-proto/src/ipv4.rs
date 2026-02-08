#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct Ipv4Header {
    pub ver_ihl: u8,
    pub tos: u8,
    pub total_len: u16,
    pub id: u16,
    pub frag_off: u16,
    pub ttl: u8,
    pub proto: u8,
    pub check: u16,
    pub src: u32,
    pub dst: u32,
}

impl Ipv4Header {
    pub fn version(&self) -> u8 {
        self.ver_ihl >> 4
    }

    pub fn ihl(&self) -> u8 {
        self.ver_ihl & 0x0F
    }
    
    pub fn header_len(&self) -> usize {
        (self.ihl() as usize) * 4
    }
}

pub fn parse_ipv4(data: &[u8]) -> Option<(&Ipv4Header, &[u8])> {
    if data.len() < std::mem::size_of::<Ipv4Header>() {
        return None;
    }
    
    let ptr = data.as_ptr() as *const Ipv4Header;
    let header = unsafe { &*ptr };
    
    let header_len = header.header_len();
    if data.len() < header_len {
        return None;
    }

    let payload = &data[header_len..];
    Some((header, payload))
}



#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct EthHeader {
    pub dst: [u8; 6],
    pub src: [u8; 6],
    pub eth_type: u16,
}

pub const ETH_P_IP: u16 = 0x0800;
pub const ETH_P_IPV6: u16 = 0x86DD;
pub const ETH_P_ARP: u16 = 0x0806;

impl EthHeader {
    pub fn eth_type(&self) -> u16 {
        u16::from_be(self.eth_type)
    }
}

pub fn parse_eth(data: &[u8]) -> Option<(&EthHeader, &[u8])> {
    if data.len() < std::mem::size_of::<EthHeader>() {
        return None;
    }
    
    let ptr = data.as_ptr() as *const EthHeader;
    let header = unsafe { &*ptr };
    let payload = &data[std::mem::size_of::<EthHeader>()..];
    
    Some((header, payload))
}



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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eth_parsing() {
        let mut data = [0u8; 18];
        data[0..6].copy_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06]); // dst
        data[6..12].copy_from_slice(&[0x11, 0x12, 0x13, 0x14, 0x15, 0x16]); // src
        data[12..14].copy_from_slice(&0x0800u16.to_be_bytes()); // type (IPv4)
        data[14..18].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]); // payload

        let (header, payload) = parse_eth(&data).expect("Should parse eth");
        assert_eq!(header.dst, [0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        assert_eq!(header.src, [0x11, 0x12, 0x13, 0x14, 0x15, 0x16]);
        assert_eq!(header.eth_type(), 0x0800);
        assert_eq!(payload, &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn test_eth_too_short() {
        let data = [0u8; 13];
        assert!(parse_eth(&data).is_none());
    }
}

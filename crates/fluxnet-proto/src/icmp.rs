#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IcmpHeader {
    pub kind: u8,
    pub code: u8,
    pub check: u16,
}

impl IcmpHeader {
    pub fn checksum(&self) -> u16 {
        u16::from_be(self.check)
    }
}

pub fn parse_icmp(data: &[u8]) -> Option<(&IcmpHeader, &[u8])> {
    if data.len() < std::mem::size_of::<IcmpHeader>() {
        return None;
    }
    
    let ptr = data.as_ptr() as *const IcmpHeader;
    let header = unsafe { &*ptr };
    let payload = &data[std::mem::size_of::<IcmpHeader>()..];
    
    Some((header, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icmp_parsing() {
        let mut data = [0u8; 8];
        data[0] = 8; // Echo Request
        data[1] = 0;
        data[2..4].copy_from_slice(&0xf7feu16.to_be_bytes()); // checksum
        data[4..8].copy_from_slice(&[0x11, 0x22, 0x33, 0x44]); // body
        
        let (header, payload) = parse_icmp(&data).expect("Should parse icmp");
        assert_eq!(header.kind, 8);
        assert_eq!(header.code, 0);
        assert_eq!(payload, &[0x11, 0x22, 0x33, 0x44]);
    }
}

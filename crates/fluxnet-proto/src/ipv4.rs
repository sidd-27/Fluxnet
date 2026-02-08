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

    pub fn is_valid(&self) -> bool {
         let len = self.header_len();
         let ptr = self as *const Ipv4Header as *const u8;
         let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
         crate::checksum(slice) == 0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipv4_parsing() {
        let mut data = [0u8; 24];
        data[0] = 0x45; // Version 4, IHL 5 (20 bytes)
        data[2..4].copy_from_slice(&24u16.to_be_bytes()); // Total length
        data[9] = 17; // Protocol UDP
        data[10..12].copy_from_slice(&0x0000u16.to_be_bytes()); // Placeholder checksum
        data[12..16].copy_from_slice(&[192, 168, 1, 1]); // src
        data[16..20].copy_from_slice(&[192, 168, 1, 100]); // dst
        data[20..24].copy_from_slice(&[0x11, 0x22, 0x33, 0x44]); // payload

        // Fix checksum
        let csum = crate::checksum(&data[0..20]);
        data[10..12].copy_from_slice(&csum.to_be_bytes());

        let (header, payload) = parse_ipv4(&data).expect("Should parse ipv4");
        assert_eq!(header.version(), 4);
        assert_eq!(header.ihl(), 5);
        assert_eq!(header.header_len(), 20);
        assert_eq!(header.proto, 17);
        assert!(header.is_valid());
        assert_eq!(payload, &[0x11, 0x22, 0x33, 0x44]);
    }

    #[test]
    fn test_ipv4_with_options() {
        let mut data = [0u8; 28];
        data[0] = 0x47; // Version 4, IHL 7 (28 bytes)
        data[2..4].copy_from_slice(&28u16.to_be_bytes());
        
        let (header, payload) = parse_ipv4(&data).expect("Should parse ipv4");
        assert_eq!(header.header_len(), 28);
        assert_eq!(payload.len(), 0);
    }
}

use crate::ipv4::Ipv4Header;

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub data_off_res_flags: u16, 
    pub window: u16,
    pub check: u16,
    pub urg_ptr: u16,
}

impl TcpHeader {
    pub fn src_port(&self) -> u16 {
        u16::from_be(self.src_port)
    }

    pub fn dst_port(&self) -> u16 {
        u16::from_be(self.dst_port)
    }

    pub fn sequence_number(&self) -> u32 {
        u32::from_be(self.seq)
    }

    pub fn acknowledgment_number(&self) -> u32 {
        u32::from_be(self.ack)
    }

    // Data offset in 32-bit words
    pub fn data_offset(&self) -> u8 {
        let val = u16::from_be(self.data_off_res_flags);
        ((val >> 12) & 0xF) as u8
    }

    pub fn header_len(&self) -> usize {
        (self.data_offset() as usize) * 4
    }

    pub fn flags(&self) -> u16 {
        u16::from_be(self.data_off_res_flags) & 0x01FF
    }

    pub fn verify_checksum(&self, ip: &Ipv4Header, _payload: &[u8]) -> bool {
        // TCP Length = IP Total Len - IP Header Len
        let ip_len = u16::from_be(ip.total_len) as usize;
        let ip_hdr_len = ip.header_len();
        if ip_len < ip_hdr_len { return false; }
        
        let tcp_seg_len = ip_len - ip_hdr_len;
        
        let mut sum: u32 = 0;
        
        // Pseudo Header
        let src = ip.src.to_be_bytes();
        sum += u16::from_be_bytes([src[0], src[1]]) as u32;
        sum += u16::from_be_bytes([src[2], src[3]]) as u32;
        
        let dst = ip.dst.to_be_bytes();
        sum += u16::from_be_bytes([dst[0], dst[1]]) as u32;
        sum += u16::from_be_bytes([dst[2], dst[3]]) as u32;
        
        sum += ip.proto as u32; 
        sum += tcp_seg_len as u32;
        
        // TCP Header + Payload
        let ptr = self as *const TcpHeader as *const u8;
        // Total bytes
        let tcp_bytes = unsafe { std::slice::from_raw_parts(ptr, tcp_seg_len) };
        
        let mut i = 0;
        while i + 1 < tcp_bytes.len() {
            let word = u16::from_be_bytes([tcp_bytes[i], tcp_bytes[i+1]]);
            sum += word as u32;
            i += 2;
        }
        if i < tcp_bytes.len() {
            sum += (tcp_bytes[i] as u32) << 8;
        }
        
        while (sum >> 16) != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        
        !sum as u16 == 0
    }
}

pub fn parse_tcp(data: &[u8]) -> Option<(&TcpHeader, &[u8])> {
    if data.len() < std::mem::size_of::<TcpHeader>() {
        return None;
    }
    
    let ptr = data.as_ptr() as *const TcpHeader;
    let header = unsafe { &*ptr };
    
    let header_len = header.header_len();
    // Safety check: header_len must be at least 20 bytes (5 words)
    if header_len < 20 || data.len() < header_len {
        return None;
    }

    let payload = &data[header_len..];
    Some((header, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcp_parsing() {
        let mut data = [0u8; 24];
        data[0..2].copy_from_slice(&1234u16.to_be_bytes());
        data[2..4].copy_from_slice(&80u16.to_be_bytes());
        data[12] = 0x60; // Offset 6 (24 bytes)
        data[13] = 0x02; // SYN flag
        
        let (header, payload) = parse_tcp(&data).expect("Should parse tcp");
        assert_eq!(header.src_port(), 1234);
        assert_eq!(header.dst_port(), 80);
        assert_eq!(header.data_offset(), 6);
        assert_eq!(header.header_len(), 24);
        assert_eq!(header.flags(), 0x002); // SYN
        assert_eq!(payload.len(), 0);
    }
}

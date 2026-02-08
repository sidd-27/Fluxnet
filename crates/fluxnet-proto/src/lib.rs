pub mod ethernet;
pub mod ipv4;
pub mod udp;
pub mod tcp;
pub mod icmp;

pub use ethernet::{EthHeader, parse_eth};
pub use ipv4::{Ipv4Header, parse_ipv4};
pub use udp::{UdpHeader, parse_udp};
pub use tcp::{TcpHeader, parse_tcp};
pub use icmp::{IcmpHeader, parse_icmp};

pub trait PacketView {
    fn len(&self) -> usize;
}

pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i+1]]);
        sum += word as u32;
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    
    !sum as u16
}

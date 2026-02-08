pub mod ethernet;
pub mod ipv4;

pub use ethernet::{EthHeader, parse_eth};
pub use ipv4::{Ipv4Header, parse_ipv4};

pub trait PacketView {
    fn len(&self) -> usize;
}

use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FluxError {
    #[error("Interface not supported or not found")]
    InterfaceNotSupported,

    #[error("Permission denied (requires CAP_NET_RAW)")]
    PermissionDenied,

    #[error("Ring buffer corruption or desynchronization")]
    RingCorruption,

    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
}

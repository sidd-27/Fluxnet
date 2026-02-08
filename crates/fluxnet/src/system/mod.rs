pub mod rx;
pub mod tx;
pub mod shared;

pub use rx::FluxRx;
pub use tx::FluxTx;

use crate::raw::FluxRaw;
use std::sync::Arc;


pub fn split(socket: FluxRaw) -> (FluxRx, FluxTx) {
    let fd = socket.fd();
    let umem = Arc::new(socket.umem);
    let shared_state = Arc::new(shared::SharedFrameState::new());
    
    // Perform partial partial moves to extract fields
    let rx = FluxRx::new(socket.rx, socket.rx_map, socket.fill, socket.fill_map, umem.clone(), fd, shared_state);
    let tx = FluxTx::new(socket.tx, socket.tx_map, socket.comp, socket.comp_map, umem, fd);
    
    (rx, tx)
}

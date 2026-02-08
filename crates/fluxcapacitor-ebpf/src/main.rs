#![no_std]
#![no_main]

use aya_ebpf::{
    bindings::xdp_action,
    macros::{xdp, map},
    programs::XdpContext,
    maps::XskMap,
};

#[map]
static XSK_MAP: XskMap = XskMap::with_max_entries(64, 0);

#[xdp]
pub fn fluxcapacitor(ctx: XdpContext) -> u32 {
    match try_fluxcapacitor(ctx) {
        Ok(ret) => ret,
        Err(_) => xdp_action::XDP_ABORTED,
    }
}

fn try_fluxcapacitor(ctx: XdpContext) -> Result<u32, u32> {
    let queue_id = unsafe { (*ctx.ctx).rx_queue_index };
    
    // Redirect to XSK socket bound to this queue
    if XSK_MAP.redirect(queue_id, 0).is_ok() {
         return Ok(xdp_action::XDP_REDIRECT);
    }

    Ok(xdp_action::XDP_PASS)
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

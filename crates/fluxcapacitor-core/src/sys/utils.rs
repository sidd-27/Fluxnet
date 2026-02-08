use std::ffi::CString;
use std::io;

pub fn if_nametoindex(name: &str) -> io::Result<u32> {
    let name_cstr = CString::new(name).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid interface name"))?;
    let idx = unsafe { libc::if_nametoindex(name_cstr.as_ptr()) };
    if idx == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(idx)
}

use std::env;
use std::path::PathBuf;

fn main() {
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let cargo_manifest_dir = PathBuf::from(cargo_manifest_dir);
    let _ebpf_dir = cargo_manifest_dir.parent().unwrap().join("fluxcapacitor-ebpf");

    #[cfg(target_os = "linux")]
    if let Err(e) = aya_build::build_ebpf(_ebpf_dir) {
        panic!("failed to build ebpf program: {}", e);
    }
    
    #[cfg(not(target_os = "linux"))]
    println!("cargo:warning=Skipping eBPF build on non-Linux platform");
}

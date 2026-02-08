use std::env;
use std::path::PathBuf;

fn main() {
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let cargo_manifest_dir = PathBuf::from(cargo_manifest_dir);
    let _ebpf_dir = cargo_manifest_dir.parent().unwrap().join("fluxcapacitor-ebpf");

    #[cfg(target_os = "linux")]
    {
        use aya_build::{Package, Toolchain};
        let root_dir = _ebpf_dir.to_str().expect("_ebpf_dir is not valid UTF-8");
        let packages = [Package {
            name: "fluxcapacitor-ebpf",
            root_dir,
            ..Default::default()
        }];
        if let Err(e) = aya_build::build_ebpf(packages, Toolchain::Nightly) {
            panic!("failed to build ebpf program: {}", e);
        }
    }
    
    #[cfg(not(target_os = "linux"))]
    println!("cargo:warning=Skipping eBPF build on non-Linux platform");
}

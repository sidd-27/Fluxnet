use aya::programs::{Xdp, XdpFlags};
use aya::{include_bytes_aligned, Bpf};
use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <interface>", args[0]);
        process::exit(1);
    }
    let iface = &args[1];

    // Load the eBPF program
    // We assume the binary is available. In a real scenario, we'd embed it.
    // For this helper, we'll try to load from the build artifact path or expect it embedded.
    // Since we are running this with cargo run, we can use the aya macro if we were inside the project structure properly.
    
    // But to keep it simple and dynamic, let's load from the file system.
    // We need to find the file.
    
    let path = match find_bpf_program() {
        Some(p) => p,
        None => {
            eprintln!("Could not find fluxcapacitor eBPF object file.");
            process::exit(1);
        }
    };

    println!("Loading eBPF program from: {:?}", path);
    let mut bpf = Bpf::load_file(&path).expect("Failed to load eBPF file");

    let program: &mut Xdp = bpf.program_mut("fluxcapacitor").unwrap().try_into().unwrap();
    program.load().expect("Failed to load program");
    program.attach(iface, XdpFlags::default())
        .expect("Failed to attach XDP program");

    println!("XDP program attached to {}. Press Ctrl+C to exit and detach.", iface);
    
    // Keep running to keep it attached
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn find_bpf_program() -> Option<std::path::PathBuf> {
    let target_dir = std::path::Path::new("target");
    // Simple search pattern
    for entry in walkdir::WalkDir::new(target_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.path().to_string_lossy().ends_with("bpfel-unknown-none/release/fluxcapacitor") {
            return Some(entry.path().to_path_buf());
        }
    }
    None
}

#!/bin/bash
set -e

# Attach XDP program to veth1 using the Rust helper
# This ensures Aya-compatible loading.

if [ "$EUID" -ne 0 ]; then
  echo "Please run as root"
  exit 1
fi

IFACE=${1:-veth1}

echo "Attaching XDP program to $IFACE using Rust loader..."
echo "This will block until you press Ctrl+C. Run in a separate terminal or background it."

# We need to run cargo as the user, preserving env, but with root permissions for the network
# OR we can run the binary directly after building it.

# Let's build it first as normal user if possible, but here we just run it.
# Assuming we are in the project root.

# Note: We need to pass the absolute path to the binary if we wanted to run it directly,
# but cargo run is easier if environment allows.

cargo run -p fluxcapacitor --bin attach_xdp --features "async" -- $IFACE

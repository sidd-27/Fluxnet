#!/bin/bash
set -e

# Setup veth pair for Fluxnet loopback test
# Use sudo to run this

if [ "$EUID" -ne 0 ]; then
  echo "Please run as root"
  exit 1
fi

echo "Creating veth pair veth0 <-> veth1..."

# Delete if exists
ip link del veth0 2>/dev/null || true

# Add pair
ip link add veth0 type veth peer name veth1

# Set up
ip link set veth0 up
ip link set veth1 up

# Assign IPs (Optional but good for verification)
ip addr add 192.168.100.1/24 dev veth0
ip addr add 192.168.100.2/24 dev veth1

# Turn off offloading to ensure XDP compatibility in some environments
ethtool -K veth0 tx off rx off 2>/dev/null || true
ethtool -K veth1 tx off rx off 2>/dev/null || true

echo "Done. Interfaces ready."
ip addr show veth0
ip addr show veth1

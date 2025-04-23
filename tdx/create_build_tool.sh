#!/bin/bash
# A small script to download and build the tool for building the TDX enclave code.
# This assumes that the current directory is the root of the Kassandra service

# Check if tool is already built. If so, do nothing
if test -f tdx/cargo-osdk; then
  exit 0
fi
# Create a temporary workspace to clone the source code into
mkdir tdx/osdk_tmp
cd tdx/osdk_tmp
# We are using a fork of the OSDK build tool
git clone -b bat/add-custom-targets --single-branch https://github.com/heliaxdev/asterinas
cd asterinas/osdk
cargo build -r
# Move the binary to the top level of the tdx sub-directory
cp target/release/cargo-osdk ../../..
cd ../../..
# Clean up
rm -rf osdk_tmp

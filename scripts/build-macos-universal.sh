#!/bin/sh
set -eu

export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$HOME=/build"

OUTPUT=${1:-dist/skribi-universal}
mkdir -p "$(dirname "$OUTPUT")"
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
lipo -create \
  target/aarch64-apple-darwin/release/skribi \
  target/x86_64-apple-darwin/release/skribi \
  -output "$OUTPUT"
lipo -info "$OUTPUT"

#!/bin/sh
# Run with an already-running native ARM64 Docker engine (e.g. Colima).
set -eu
cd "$(dirname "$0")/.."
docker --context colima run --rm --platform linux/arm64 -v "$PWD:/source" -w /source rust:1.95-bullseye bash -euxc '
dpkg --add-architecture amd64
sed -i "s|http://deb.debian.org/debian-security|https://archive.debian.org/debian-security|g; s|http://deb.debian.org|https://deb.debian.org|g" /etc/apt/sources.list
rm -rf /var/lib/apt/lists/*
APT="-o Acquire::Check-Valid-Until=false -o Acquire::Retries=8 -o Acquire::http::Timeout=60 -o Acquire::https::Timeout=60"
apt-get $APT update
apt-get $APT install -y gcc-x86-64-linux-gnu g++-x86-64-linux-gnu pkg-config libx11-dev:amd64 libxkbcommon-dev:amd64 libwayland-dev:amd64 libegl1-mesa-dev:amd64 libssl-dev:amd64
rustup target add x86_64-unknown-linux-gnu
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc
export CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc
export CXX_x86_64_unknown_linux_gnu=x86_64-linux-gnu-g++
export PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig
export RUSTFLAGS="--remap-path-prefix=/source=/build --remap-path-prefix=/usr/local/cargo=/cargo"
cargo build --locked --release --target x86_64-unknown-linux-gnu -j4
x86_64-linux-gnu-readelf --version-info target/x86_64-unknown-linux-gnu/release/skribi | grep -o "GLIBC_[0-9.]*" | sort -Vu | tail -1
'

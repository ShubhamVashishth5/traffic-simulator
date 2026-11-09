#!/usr/bin/env bash
# Build a static (musl) `marg` binary for HPC clusters and Apptainer images
# (plan P3.5). Needs the musl target and a musl C compiler (zstd is C):
#   rustup target add x86_64-unknown-linux-musl
#   apt-get install musl-tools        # provides musl-gcc
# Output: dist/marg-x86_64-linux-musl (stripped, no dynamic dependencies).
# About 7% slower than the glibc build on the reference corridor (musl malloc);
# the hot loop does not allocate, so the gap is in setup and output writing.
set -euo pipefail
cd "$(dirname "$0")/.."
TARGET=x86_64-unknown-linux-musl
CC_x86_64_unknown_linux_musl=${CC_x86_64_unknown_linux_musl:-musl-gcc} \
  cargo build --release --locked --target "$TARGET" -p marg-cli
mkdir -p dist
cp "target/$TARGET/release/marg" dist/marg-x86_64-linux-musl
strip dist/marg-x86_64-linux-musl
if ldd dist/marg-x86_64-linux-musl 2>&1 | grep -q "=>"; then
  echo "error: binary is dynamically linked" >&2
  exit 1
fi
dist/marg-x86_64-linux-musl version
ls -la dist/marg-x86_64-linux-musl

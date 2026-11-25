#!/bin/sh
# Bundle the vendored crates so sarda can be built without network access.
#
#   scripts/dist.sh            -> dist/sarda-vendor-<version>.tar.xz
#
# On the offline machine: extract the tarball at the repository root, then
#   cargo build --release --offline
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
mkdir -p dist .cargo
cargo vendor --locked vendor > .cargo/config.toml
tar -cJf "dist/sarda-vendor-$version.tar.xz" vendor .cargo/config.toml
rm -rf vendor .cargo/config.toml
rmdir .cargo 2>/dev/null || true
echo "dist/sarda-vendor-$version.tar.xz"

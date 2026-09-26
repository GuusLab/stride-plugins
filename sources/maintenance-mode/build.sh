#!/bin/sh
# The only way this plugin is built. The registry rebuilds it from source in a
# clean container and refuses the submission unless the module it gets is byte
# for byte the one published, so nothing here may depend on where the checkout
# happens to live or on which toolchain happens to be installed:
#
#   * the channel is pinned in rust-toolchain.toml;
#   * Cargo.lock is committed;
#   * --remap-path-prefix erases the absolute path of this directory, of the
#     cargo registry and of the rustup toolchain from the module. All three
#     are needed: docs/plugin-registry.md mentions --remap-path-prefix without
#     saying that the standard library's own paths leak through the sysroot,
#     and one leaked home directory is enough to fail check 4.
#
# docs/plugin-registry.md, check 4, is what all of that is for.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-3}
CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}
sysroot=$(rustc --print sysroot)
RUSTFLAGS="--remap-path-prefix=$here=/build"
RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=$CARGO_HOME=/cargo"
RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=$sysroot=/rustup"
# stride-pdk is vendored in this repository (pdk-rust/) and reached by a
# relative path; remapping it keeps its absolute path out of the module too.
pdk=$(cd "$here/../../pdk-rust" && pwd)
RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=$pdk=/pdk"
export RUSTFLAGS

cargo build --locked --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/maintenance_mode.wasm plugin.wasm

if command -v shasum >/dev/null 2>&1; then
  shasum -a 256 plugin.wasm
fi
wc -c < plugin.wasm

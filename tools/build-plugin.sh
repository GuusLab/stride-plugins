#!/bin/sh
# Usage: tools/build-plugin.sh <id>
#
# Reproducibly builds sources/<id> for wasm32-unknown-unknown and copies the
# module to sources/<id>/plugin.wasm, then prints its SHA-256 and size.
#
# The registry (and .github/workflows/verify.yml) rebuilds every plugin with
# this exact script and compares the hash byte for byte with the release's
# contentHash, so nothing may depend on where the checkout lives:
#   * the toolchain is pinned by sources/<id>/rust-toolchain.toml
#     (falls back to tools/rust-toolchain.toml, Stride's reading-time pin);
#   * Cargo.lock is committed and the build runs with --locked;
#   * --remap-path-prefix erases the source dir, the vendored PDK, CARGO_HOME
#     and the rustup sysroot from the module.
#
# The reference build is x86_64-unknown-linux-gnu, as on GitHub's ubuntu
# runners: Cargo mixes the host triple into its crate metadata, so a build on
# another host (say aarch64-apple-darwin) can differ by a few bytes. The
# verify workflow uploads its rebuilt modules as the "rebuilt-modules"
# artifact; those are the bytes to publish.
set -eu
[ $# -eq 1 ] || { echo "usage: $0 <id>" >&2; exit 2; }
id=$1
root=$(cd "$(dirname "$0")/.." && pwd)
src="$root/sources/$id"
[ -f "$src/Cargo.toml" ] || { echo "no such plugin source: $src" >&2; exit 1; }
cd "$src"

if [ ! -f rust-toolchain.toml ]; then
  cp "$root/tools/rust-toolchain.toml" rust-toolchain.toml
  echo "note: copied tools/rust-toolchain.toml into sources/$id" >&2
fi
channel=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)
if command -v rustup >/dev/null 2>&1; then
  rustup toolchain install "$channel" --profile minimal --target wasm32-unknown-unknown >/dev/null 2>&1 || true
  rustup target add --toolchain "$channel" wasm32-unknown-unknown >/dev/null
fi

CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}
sysroot=$(rustc --print sysroot)
pdk="$root/pdk-rust"
RUSTFLAGS="--remap-path-prefix=$src=/build"
RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=$pdk=/pdk"
RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=$CARGO_HOME=/cargo"
RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=$sysroot=/rustup"
# With the rust-src component installed, std's panic locations point into it;
# without it they read /rustc/<commit>. Map the first onto the second so both
# kinds of machine build the same bytes (the last matching remap wins).
commit=$(rustc -vV | sed -n 's/^commit-hash: //p')
RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=$sysroot/lib/rustlib/src/rust=/rustc/$commit"
export RUSTFLAGS
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-3}
unset CARGO_TARGET_DIR

cargo build --locked --release --target wasm32-unknown-unknown

name=$(sed -n '/^\[package\]/,/^\[/s/^name *= *"\(.*\)"/\1/p' Cargo.toml | head -n1 | tr '-' '_')
cp "target/wasm32-unknown-unknown/release/$name.wasm" plugin.wasm

if command -v sha256sum >/dev/null 2>&1; then
  hash=$(sha256sum plugin.wasm | cut -d' ' -f1)
else
  hash=$(shasum -a 256 plugin.wasm | cut -d' ' -f1)
fi
size=$(wc -c < plugin.wasm | tr -d ' ')
host=$(rustc -vV | sed -n 's/^host: //p')
[ "$host" = x86_64-unknown-linux-gnu ] || echo "note: built on $host; the registry's reference host is x86_64-unknown-linux-gnu" >&2
echo "sources/$id/plugin.wasm"
echo "sha256 $hash"
echo "size   $size"

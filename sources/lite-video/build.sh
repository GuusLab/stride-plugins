#!/bin/sh
# The only way this plugin is built: the registry's own reproducible build.
# Pinned toolchain (rust-toolchain.toml), committed Cargo.lock, --locked, and
# --remap-path-prefix for every absolute path that could leak into the module.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
exec "$here/../../tools/build-plugin.sh" lite-video

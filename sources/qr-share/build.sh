#!/bin/sh
# The only way this plugin is built: the registry's reproducible build script.
# It pins the toolchain (rust-toolchain.toml), builds --locked, remaps every
# absolute path out of the module and writes plugin.wasm next to this file.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
exec "$here/../../tools/build-plugin.sh" qr-share

#!/usr/bin/env bash

set -x
set -e
set -u
set -o pipefail

zig version

# Make SDKROOT explicit (don't rely on image defaults)
export SDKROOT="${SDKROOT:-/opt/MacOSX11.3.sdk}"

# Force Zig/rustc to search frameworks inside the SDK
# This is the key bit for "unable to find framework CoreFoundation"
export CARGO_ENCODED_RUSTFLAGS="-Clink-arg=--sysroot=${SDKROOT} -Clink-arg=-F${SDKROOT}/System/Library/Frameworks -Clink-arg=-F${SDKROOT}/System/Library/PrivateFrameworks"

# Workaround for sysroot-prefixed absolute search paths
mkdir -p "${SDKROOT}/root"
ln -sfn /root/.cache "${SDKROOT}/root/.cache"

# Fix: c_src/mimalloc/src/options.c:215:9: error: expansion of date or time macro is not reproducible [-Werror,-Wdate-time]
export CFLAGS="${CFLAGS-} -Wno-error=date-time"

# Github actions requires to mark the current git repository as safe
git config --global --add safe.directory "$(pwd)"

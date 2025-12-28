#!/usr/bin/env bash

set -x
set -e
set -u
set -o pipefail

echo 'deb [trusted=yes] https://repo.goreleaser.com/apt/ /' | tee /etc/apt/sources.list.d/goreleaser.list
apt update && apt upgrade -y
apt install -y git goreleaser mingw-w64

ZIG_VERSION=0.15.2
ZIG_NAME="zig-$(uname -m)-linux-${ZIG_VERSION}"

curl -L "https://ziglang.org/download/${ZIG_VERSION}/${ZIG_NAME}.tar.xz" | tar -J -x -C /usr/local
rm -f /usr/local/bin/zig
ln -s "/usr/local/${ZIG_NAME}/zig" /usr/local/bin/zig
zig version

# Make SDKROOT explicit (don't rely on image defaults)
export SDKROOT="${SDKROOT:-/opt/MacOSX11.3.sdk}"

# fix: c_src/mimalloc/src/options.c:215:9: error: expansion of date or time macro is not reproducible [-Werror,-Wdate-time]
export CFLAGS="${CFLAGS-} -Wno-error=date-time"

# github actions requires to mark the current git repository as safe
git config --global --add safe.directory "$(pwd)"

#!/usr/bin/env bash

set -x
set -e
set -u
set -o pipefail

echo 'deb [trusted=yes] https://repo.goreleaser.com/apt/ /' | tee /etc/apt/sources.list.d/goreleaser.list
apt update && apt upgrade -y
apt install -y git goreleaser mingw-w64

# Install latest zig
ZIG_VERSION=0.14.0
# ZIG_VERSION=0.13.0
curl -L "https://ziglang.org/download/${ZIG_VERSION}/zig-linux-$(uname -m)-${ZIG_VERSION}.tar.xz" | tar -J -x -C /usr/local
rm -f /usr/local/bin/zig
ln -s "/usr/local/zig-linux-$(uname -m)-${ZIG_VERSION}/zig" /usr/local/bin/zig

# fix: c_src/mimalloc/src/options.c:215:9: error: expansion of date or time macro is not reproducible [-Werror,-Wdate-time]
export CFLAGS="${CFLAGS-} -Wno-error=date-time"

# github actions requires to mark the current git repository as safe
git config --global --add safe.directory "$(pwd)"

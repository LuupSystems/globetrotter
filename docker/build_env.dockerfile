FROM ghcr.io/rust-cross/cargo-zigbuild

RUN <<EOT
echo 'deb [trusted=yes] https://repo.goreleaser.com/apt/ /' | tee /etc/apt/sources.list.d/goreleaser.list
apt update && apt upgrade -y
apt install -y git goreleaser mingw-w64
EOT

RUN <<EOT
# export ZIG_VERSION=0.15.2
# export ZIG_NAME="zig-$(uname -m)-linux-${ZIG_VERSION}"
#
# curl -L "https://ziglang.org/download/${ZIG_VERSION}/${ZIG_NAME}.tar.xz" | tar -J -x -C /usr/local
# rm -f /usr/local/bin/zig
# ln -s "/usr/local/${ZIG_NAME}/zig" /usr/local/bin/zig
# zig version
EOT

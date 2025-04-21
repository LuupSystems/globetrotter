FROM lukemathwalker/cargo-chef:latest-rust-1.86-alpine AS chef
WORKDIR /app

# cache rust toolchains
RUN rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl

# plan
FROM chef AS planner
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./crates ./crates
RUN apk add tree && tree ./
RUN cargo chef prepare \
    --recipe-path ./recipe.json \
    --bin globetrotter

# --package globetrotter-cli \

# build
FROM chef AS builder
# map buildkit arch (amd64/arm64) to rust target triple
ARG TARGETARCH

# RUN set -eux; \
#     case "${TARGETARCH}" in \
#       amd64) export RUST_TARGET_TRIPLE=x86_64-unknown-linux-musl;; \
#       arm64) export RUST_TARGET_TRIPLE=aarch64-unknown-linux-musl;; \
#       *) echo "unsupported arch ${TARGETARCH}" >&2; exit 1;; \
#     esac;

# map buildkit arch (amd64/arm64) to rust target triple
# ARG TARGETARCH
# ARG RUST_TARGET_TRIPLE
#
# RUN set -eux; \
#     case "${TARGETARCH}" in \
#       amd64) echo "RUST_TARGET_TRIPLE=x86_64-unknown-linux-musl" >> /tmp/rt.env;; \
#       arm64) echo "RUST_TARGET_TRIPLE=aarch64-unknown-linux-musl" >> /tmp/rt.env;; \
#       *) exit 1;; \
#     esac;
#
# # then load & persist it
# RUN --mount=type=bind,source=/tmp/rt.env,target=/tmp/rt.env \
#     eval "$(cat /tmp/rt.env)" && \
#     echo "ENV RUST_TARGET_TRIPLE=${RUST_TARGET_TRIPLE}" >> /usr/local/env-vars
#
# # Now actually set the ENV for following layers
# ENV RUST_TARGET_TRIPLE="${RUST_TARGET_TRIPLE}"
#
# RUN echo "${TARGETARCH} => ${RUST_TARGET_TRIPLE}"

# install musl and add the musl targets
RUN apk add --no-cache musl-dev
# RUN apt-get update \
#  && apt-get install -y --no-install-recommends musl-tools \
#  && rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl \
#  && rm -rf /var/lib/apt/lists/*

# WORKDIR /app
COPY --from=planner /app/recipe.json ./recipe.json
# RUN cargo chef cook --release --recipe-path ./recipe.json --bin "$BIN_NAME"
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    set -eux; \
    cargo chef cook \
	--recipe-path ./recipe.json \
	--target $(case "${TARGETARCH}" in \
	    amd64) echo x86_64-unknown-linux-musl;; \
	    arm64) echo aarch64-unknown-linux-musl;; \
	esac)
	# --package globetrotter-cli \
	# --bin globetrotter

# --package globetrotter-cli \
# --target "${RUST_TARGET_TRIPLE}" \

# now copy full source and build the actual binary
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    set -eux; \
    case "${TARGETARCH}" in \
      amd64) export RUST_TARGET_TRIPLE=x86_64-unknown-linux-musl;; \
      arm64) export RUST_TARGET_TRIPLE=aarch64-unknown-linux-musl;; \
      *) exit 1;; \
    esac; \
    cargo build \
	--target "${RUST_TARGET_TRIPLE}" \
	--package globetrotter-cli \
	--bin globetrotter \
    && mv /app/target/${RUST_TARGET_TRIPLE}/debug/globetrotter /app/globetrotter

# --release 

# --target $(case "${TARGETARCH}" in \
#     amd64) echo x86_64-unknown-linux-musl;; \
#     arm64) echo aarch64-unknown-linux-musl;; \
# esac) \
 #    RUN set -eux; \
	# && /app/target/${RUST_TARGET_TRIPLE}/release/globetrotter /app/globetrotter

# --package globetrotter-cli \
# --target "${RUST_TARGET_TRIPLE}" \

# # map buildkit arch (amd64/arm64) to rust target triple
# ARG TARGETARCH
# RUN set -eux; \
#     case "${TARGETARCH}" in \
#       amd64) export RUST_TARGET_TRIPLE=x86_64-unknown-linux-musl;; \
#       arm64) export RUST_TARGET_TRIPLE=aarch64-unknown-linux-musl;; \
#       *) echo "unsupported arch ${TARGETARCH}" >&2; exit 1;; \
#     esac && ls -liah /app/target/ && mv /app/target/${RUST_TARGET_TRIPLE}/release/globetrotter /app/globetrotter

# RUN mv /app/target/${RUST_TARGET_TRIPLE}/release/globetrotter /app/globetrotter

# copy in our dependency‐only recipe
# COPY --from=chef /usr/src/app/recipe.json recipe.json

# FROM rust:1.86 AS chef
# WORKDIR /usr/src/app
#
# # install cargo-chef
# RUN cargo install cargo-chef --locked
#
# # copy just the manifest(s) for recipe generation
# COPY Cargo.toml Cargo.lock ./
#
# # create a recipe.json that describes your dependency graph
# RUN cargo chef prepare --recipe-path recipe.json
#
# ###############################################################################
# # 2) “Build” stage: compile dependencies for the target arch
# ###############################################################################
# FROM rust:1.74 AS builder
# # allow buildkit to pass TARGETARCH (e.g. x86_64 or aarch64)
# ARG TARGETARCH
#
# # install musl and add the musl targets
# RUN apt-get update \
#  && apt-get install -y --no-install-recommends musl-tools \
#  && rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl \
#  && rm -rf /var/lib/apt/lists/*
#
# WORKDIR /usr/src/app
#
# # copy in our dependency‐only recipe
# COPY --from=chef /usr/src/app/recipe.json recipe.json
#
# # build just the deps into cache (cached by recipe.json + TARGETARCH)
# RUN --mount=type=cache,target=/usr/local/cargo/registry \
#     --mount=type=cache,target=/usr/src/app/target \
#     cargo chef cook --release \
#       --target ${TARGETARCH}-unknown-linux-musl \
#       --recipe-path recipe.json
#
# # now copy your full source and build the actual binary
# COPY . .
#
# RUN --mount=type=cache,target=/usr/local/cargo/registry \
#     --mount=type=cache,target=/usr/src/app/target \
#     cargo build --release \
#       --target ${TARGETARCH}-unknown-linux-musl

# runtime
FROM scratch AS runtime
COPY --from=builder /app/globetrotter /usr/local/bin/globetrotter 
ENTRYPOINT ["/usr/local/bin/globetrotter"]

FROM lukemathwalker/cargo-chef:latest-rust-1.86-alpine AS chef
WORKDIR /app

# cache rust toolchains
RUN rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl

# plan
FROM chef AS planner
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./crates ./crates

RUN cargo chef prepare \
    --recipe-path ./recipe.json \
    --bin globetrotter

# build
FROM chef AS builder

RUN apk add --no-cache musl-dev

COPY --from=planner /app/recipe.json ./recipe.json
ARG TARGETARCH

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    set -eux; \
    cargo chef cook \
	--release \
	--recipe-path ./recipe.json \
	--target $(case "${TARGETARCH}" in \
	    amd64) echo x86_64-unknown-linux-musl;; \
	    arm64) echo aarch64-unknown-linux-musl;; \
	esac)

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
	--release \
	--target "${RUST_TARGET_TRIPLE}" \
	--package globetrotter-cli \
	--bin globetrotter \
    && mv /app/target/${RUST_TARGET_TRIPLE}/release/globetrotter /app/globetrotter

# runtime
FROM scratch AS runtime
COPY --from=builder /app/globetrotter /usr/local/bin/globetrotter 
ENTRYPOINT ["/usr/local/bin/globetrotter"]

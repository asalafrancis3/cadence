# ---- Stage 1: Build ----
FROM rust:1.82-bookworm AS builder

WORKDIR /app

# Copy workspace root files needed by Cargo
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# Copy contract crate (needed for workspace resolution)
COPY contracts/ contracts/

# Copy keeper crate
COPY keeper/ keeper/

# Build the keeper binary in release mode
RUN cargo build --release -p keeper

# ---- Stage 2: Runtime ----
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/keeper /usr/local/bin/keeper

# Non-root user for security
RUN useradd --create-home keeper
USER keeper

ENTRYPOINT ["keeper"]

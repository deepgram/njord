# Use the official Rust image as a parent image for building
FROM rust:1.85.1 as chef
RUN cargo install cargo-chef
WORKDIR /app

# Prepare the dependency list
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Build dependencies - this layer will be cached unless dependencies change
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies (this is the caching step)
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
# Build the application with static linking
ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN cargo build --release --target x86_64-unknown-linux-gnu

# Runtime stage - use scratch for minimal image
FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/njord /njord
ENTRYPOINT ["/njord"]

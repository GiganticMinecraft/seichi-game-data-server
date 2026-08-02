# syntax=docker/dockerfile:1
FROM lukemathwalker/cargo-chef:latest-rust-1.97.1 AS chef
WORKDIR /app

FROM chef AS planner
COPY --link . .
RUN cargo chef prepare --recipe-path recipe.json

FROM bufbuild/buf:1.72.0 AS buf

FROM chef AS build-env
COPY --from=buf --link /usr/local/bin/buf /usr/local/bin/

# We need protoc because cargo chef cook will require it to build some modules (e.g. pbjson-types)
RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler && rm -rf /var/lib/apt/lists/*

# protoc plugins used by `buf generate` (see infra/grpc/buf.gen.yaml); versions must match the CI workflow
RUN cargo install protoc-gen-prost@0.5.0 protoc-gen-tonic@0.5.0 protoc-gen-prost-crate@0.5.0

COPY --from=planner --link /app/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY --link . .
RUN cargo build --release

FROM gcr.io/distroless/cc
LABEL org.opencontainers.image.source=https://github.com/GiganticMinecraft/seichi-game-api
COPY --from=build-env --link /app/target/release/seichi-game-api /
CMD ["./seichi-game-api"]

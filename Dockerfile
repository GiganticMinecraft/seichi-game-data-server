# syntax=docker/dockerfile:1.13
FROM lukemathwalker/cargo-chef:latest-rust-1.82.0 AS chef
WORKDIR /app

FROM chef AS planner
COPY --link . .
RUN cargo chef prepare --recipe-path recipe.json

FROM bufbuild/buf:1.50.0 as buf

FROM namely/protoc:1.42_2 as protoc

FROM chef AS build-env
COPY --from=planner --link /app/recipe.json recipe.json
COPY --from=buf --link /usr/local/bin/buf /usr/local/bin/
COPY --from=protoc --link /usr/local/bin/protoc /usr/local/bin/

# We need these because cargo chef cook will require protoc to build some modules
ARG PROTOC_NO_VENDOR=true
ARG PROTOC=/usr/local/bin/protoc

# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY --link . .
RUN cargo build --release

FROM gcr.io/distroless/cc
LABEL org.opencontainers.image.source=https://github.com/GiganticMinecraft/seichi-game-api
COPY --from=build-env --link /app/target/release/seichi-game-api /
CMD ["./seichi-game-api"]

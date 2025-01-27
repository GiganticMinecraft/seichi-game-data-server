# syntax=docker/dockerfile:1

FROM rust:1.82.0-slim AS build

WORKDIR /app

COPY --from=bufbuild/buf:1.50.0 /usr/local/bin/buf /usr/local/bin/
COPY --from=namely/protoc:1.42_2 /usr/local/bin/protoc /usr/local/bin/

RUN --mount=type=bind,source=.,target=/app/src \
    --mount=type=cache,target=/app/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    cargo build --locked --release \
        --manifest-path /app/src/Cargo.toml \
        --target-dir /app/build/target

FROM gcr.io/distroless/cc AS final

USER nonroot:nonroot

LABEL org.opencontainers.image.source="https://github.com/GiganticMinecraft/seichi-game-data-server"

COPY --from=build /app/build/target/release/seichi-game-data-server /bin/seichi-game-data-server

CMD ["/bin/seichi-game-data-server"]

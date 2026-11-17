# syntax=docker/dockerfile:1
FROM rust:1.95-alpine3.23 AS builder
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
ARG RUSTFLAGS=""
RUN cargo build --release --locked

FROM scratch
ARG VERSION=0.1.0
LABEL org.opencontainers.image.title="cadvisor-rs" \
      org.opencontainers.image.description="Lightweight Linux cgroup and container metrics exporter" \
      org.opencontainers.image.source="https://github.com/Primexz/cadvisor-rs" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${VERSION}"
COPY --from=builder /src/target/release/cadvisor-rs /cadvisor-rs
COPY LICENSE /LICENSE
USER 0:0
EXPOSE 8080
ENTRYPOINT ["/cadvisor-rs"]

# syntax=docker/dockerfile:1@sha256:4edf897a3ffa55b89f906fc8cc78afdb3f1834cc9c7083565e611a8a7d5fe99e
FROM rust:1.100-alpine3.23@sha256:9577a4edf61be25721f5fa4006ad463b83ea52c0a17c6e6b673c72c794aee738 AS builder
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

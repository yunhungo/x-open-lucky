# syntax=docker/dockerfile:1
# @created 2026-10-11
# @description 构建包含 React、Rust 和固定 ACME 客户端的生产镜像。
# @author yunhungo
FROM --platform=$BUILDPLATFORM node:22-bookworm-slim AS frontend
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY frontend ./
RUN npm run build

FROM --platform=$BUILDPLATFORM rust:1.89-bookworm AS backend
RUN apt-get update && apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu libc6-dev-amd64-cross \
    && rm -rf /var/lib/apt/lists/* && rustup target add x86_64-unknown-linux-gnu
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
    CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY backend backend
ARG BUILD_SHA=dev
ENV BUILD_SHA=$BUILD_SHA
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/target \
    cargo build --release --locked --target x86_64-unknown-linux-gnu -j 2 && cp target/x86_64-unknown-linux-gnu/release/openlucky /openlucky

FROM goacme/lego:v4.35.2 AS acme
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* && groupadd -g 10001 openlucky \
    && useradd -u 10001 -g 10001 -M -s /usr/sbin/nologin openlucky \
    && mkdir -p /data && chown 10001:10001 /data
COPY --from=backend /openlucky /usr/local/bin/openlucky
COPY --from=acme /lego /usr/local/bin/lego
COPY --from=frontend /src/frontend/dist /app/frontend
COPY THIRD_PARTY_NOTICES.md /app/THIRD_PARTY_NOTICES.md
ARG BUILD_SHA=dev
LABEL org.opencontainers.image.title="OpenLucky" \
    org.opencontainers.image.source="https://github.com/yunhungo/x-open-lucky" \
    org.opencontainers.image.revision=$BUILD_SHA
ENV OPENLUCKY_DATA=/data OPENLUCKY_FRONTEND=/app/frontend
WORKDIR /app
USER 10001:10001
EXPOSE 56002 58081 58444
HEALTHCHECK --interval=20s --timeout=5s --start-period=20s --retries=3 \
    CMD curl --fail --silent http://127.0.0.1:56002/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/openlucky"]

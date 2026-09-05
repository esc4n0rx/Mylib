# syntax=docker/dockerfile:1

# 1. Build the web frontend.
FROM node:22-bookworm-slim AS web-builder
WORKDIR /web
COPY web/package.json web/package-lock.json* ./
RUN npm ci
COPY web/ ./
RUN npm run build

# 2. Build the Rust server with the frontend embedded.
FROM rust:1.98-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
COPY migrations ./migrations
COPY --from=web-builder /web/dist ./web/dist
RUN cargo build --release --locked

# 3. Minimal runtime image — no node, npm, vite or rust toolchain.
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home mylib \
    && mkdir -p /data && chown mylib:mylib /data
COPY --from=builder /src/target/release/mylib-server /usr/local/bin/mylib-server
USER mylib
ENV MYLIB_HOST=0.0.0.0 MYLIB_PORT=8096 MYLIB_DATA_DIR=/data MYLIB_LOG_LEVEL=info
EXPOSE 8096
VOLUME ["/data"]
ENTRYPOINT ["mylib-server"]

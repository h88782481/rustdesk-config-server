# ---- build stage ----
# 用完整 rust:bookworm(自带 gcc/链接器),避免 alpine/musl 的链接坑,提高一次构建成功率
FROM rust:1-bookworm AS builder

WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

# ---- runtime stage ----
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends wget ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rustdesk-config-server /usr/local/bin/rustdesk-config-server

ENV BIND_ADDR=0.0.0.0:21120 \
    DATA_DIR=/data
EXPOSE 21120
VOLUME ["/data"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget -qO- http://127.0.0.1:21120/healthz || exit 1

ENTRYPOINT ["rustdesk-config-server"]

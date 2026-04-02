# ── Build stage ──────────────────────────────────────────────────────────────
FROM rust:1.75-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .
RUN cargo build --release -p nexa-registry

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/nexa-registry /usr/local/bin/nexa-registry
COPY --from=builder /app/crates/registry/migrations   /migrations

EXPOSE 4000

ENV DATABASE_URL=""
ENV JWT_SECRET=""

CMD ["nexa-registry"]

FROM rust:1.93-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked --bin code-indexer

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 10001 appuser

WORKDIR /workspace

COPY --from=builder /app/target/release/code-indexer /usr/local/bin/code-indexer

USER appuser

ENV RUST_LOG=code_indexer=info

ENTRYPOINT ["code-indexer"]
CMD ["--help"]

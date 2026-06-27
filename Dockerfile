FROM rust:slim-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
COPY resources/ ./resources/

RUN apt-get update && apt-get install -y pkg-config libssl-dev && \
    cargo build --release --locked && \
    strip target/release/raven

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates openssl && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/raven /usr/local/bin/raven
COPY --from=builder /app/resources/data.json /root/.local/share/raven/resources/data.json

ENV CODENAME=raven

ENTRYPOINT ["raven"]
CMD ["--help"]

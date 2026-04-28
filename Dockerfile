FROM rust:1.88-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /home/agent agent

COPY --from=builder /app/target/release/pingpongkong-k8s-agent /usr/local/bin/pingpongkong-k8s-agent

USER 10001
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/pingpongkong-k8s-agent"]

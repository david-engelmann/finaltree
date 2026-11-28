FROM rust:1.85-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin finaltree

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates && rm -rf /var/lib/apt/lists/*
RUN git config --system --add safe.directory '*'
RUN groupadd -g 65532 nonroot && useradd -u 65532 -g 65532 -m -s /usr/sbin/nologin nonroot
COPY --from=builder /src/target/release/finaltree /usr/local/bin/finaltree
USER 65532:65532
WORKDIR /work
ENTRYPOINT ["/usr/local/bin/finaltree"]

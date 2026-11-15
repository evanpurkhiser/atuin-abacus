FROM rust:1.96-bookworm AS builder

RUN apt-get update \
  && apt-get install -y --no-install-recommends libssl-dev pkg-config \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release

FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --create-home --uid 10001 abacus \
  && install -d -o abacus -g abacus /var/lib/atuin-abacus

COPY --from=builder /app/target/release/atuin-abacus /usr/local/bin/atuin-abacus

USER abacus
ENV HOST=0.0.0.0
ENV PORT=8888
ENV ABACUS_DATA_DIR=/var/lib/atuin-abacus
EXPOSE 8888
VOLUME ["/var/lib/atuin-abacus"]

ENTRYPOINT ["atuin-abacus"]

#!/bin/sh

FROM lukemathwalker/cargo-chef:latest-rust-1.88-trixie AS chef
WORKDIR /app

FROM chef AS planner
RUN apt-get update -y
RUN apt-get install libclang-dev cmake -y
COPY . .
RUN cargo +nightly chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update -y
RUN apt-get install libclang-dev cmake -y
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo +nightly chef cook --release --recipe-path recipe.json
# Build application
COPY . .

RUN cargo +nightly build --release --bin main

FROM ubuntu:noble AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/main /usr/local/bin
COPY --from=builder /app/entrypoint.sh /app/entrypoint.sh
COPY --from=builder /app/run.sh /app/run.sh

RUN chmod +x /app/entrypoint.sh
ENV DOCKER=1

# add Inter font
RUN apt-get update -y
RUN apt-get install -y fonts-inter fontconfig
# Install litestream
RUN apt-get install -y ca-certificates fuse3 sqlite3
ADD https://github.com/benbjohnson/litestream/releases/download/v0.3.8/litestream-v0.3.8-linux-amd64-static.tar.gz /tmp/litestream.tar.gz
RUN tar -C /usr/local/bin -xzf /tmp/litestream.tar.gz
COPY --from=builder /app/litestream.yml /etc/litestream.yml

ENV RUST_BACKTRACE=1
ENV OTEL_SERVICE_NAME="eldemite-server"
ENTRYPOINT ["sh"]
CMD ["run.sh"]

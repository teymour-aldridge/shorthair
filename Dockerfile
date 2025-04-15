#!/bin/sh

FROM rust AS builder

COPY . /app

WORKDIR /app

RUN apt-get update
RUN apt-get install libclang-dev cmake -y

RUN cargo +nightly build --release

FROM debian:bookworm-slim AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/main /usr/local/bin
COPY --from=builder /app/entrypoint.sh /app/entrypoint.sh
COPY --from=builder /app/run.sh /app/run.sh

RUN chmod +x /app/entrypoint.sh
ENV DOCKER=1

# Install litestream
RUN apt-get update -y && apt-get install -y ca-certificates fuse3 sqlite3
ADD https://github.com/benbjohnson/litestream/releases/download/v0.3.8/litestream-v0.3.8-linux-amd64-static.tar.gz /tmp/litestream.tar.gz
RUN tar -C /usr/local/bin -xzf /tmp/litestream.tar.gz
COPY --from=builder /app/litestream.yml /etc/litestream.yml

ENTRYPOINT ["sh"]
CMD ["run.sh"]

# syntax=docker/dockerfile:1
# 1) Build the web panel  2) build xbncd with the panel embedded
# 3) run on a slim image as an unprivileged user. Data lives in /data.

FROM node:22-bookworm-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
# The panel shows the version of the Rust workspace.
COPY Cargo.toml /Cargo.toml
RUN npm run build

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY docs/openapi.json ./docs/openapi.json
COPY --from=web /web/dist ./web/dist
RUN cargo build --release --locked -p xbncd

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --user-group --home-dir /data --shell /usr/sbin/nologin xbnc \
 && install -d -m 0700 -o xbnc -g xbnc /data
COPY --from=build /src/target/release/xbncd /usr/local/bin/xbncd
COPY docker/entrypoint.sh /usr/local/bin/xbnc-entrypoint
USER xbnc
ENV XBNC_CONFIG=/data/xbnc.toml
VOLUME ["/data"]
EXPOSE 6667 6697 8080 8443
ENTRYPOINT ["xbnc-entrypoint"]
CMD ["run"]

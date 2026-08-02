FROM rust:1.89-bookworm AS builder
WORKDIR /build
COPY shared/pmc-protocol /build/shared/pmc-protocol
COPY server /build/server
WORKDIR /build/server
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 pmc && mkdir -p /data && chown pmc:pmc /data
COPY --from=builder /build/server/target/release/pmc-server /usr/local/bin/pmc-server
USER pmc
ENV PMC_DATA_DIR=/data PMC_SERVER_BIND=0.0.0.0:7412
EXPOSE 7412
VOLUME ["/data"]
ENTRYPOINT ["pmc-server"]

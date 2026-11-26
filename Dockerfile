# Mandate Warden and its agent-side binaries in one image.
#
#   docker run ghcr.io/1888-labs/mandate                  # the Warden (default)
#   docker run ghcr.io/1888-labs/mandate mandate-agent "…" # the agent
#   docker run ghcr.io/1888-labs/mandate warden-call keygen
#
# See docs/self-hosting.md.

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo build --release --locked -p mandate-warden

FROM debian:bookworm-slim
LABEL org.opencontainers.image.source="https://github.com/1888-Labs/Mandate" \
      org.opencontainers.image.description="Mandate Warden: verifiable, revocable authority for AI agents, in front of every call they make" \
      org.opencontainers.image.licenses="MIT"
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --uid 10001 --create-home --shell /usr/sbin/nologin mandate \
  && mkdir -p /data && chown mandate /data
COPY --from=build /src/target/release/mandate-warden /src/target/release/warden-call /src/target/release/mandate-agent /usr/local/bin/
USER mandate
WORKDIR /home/mandate
# The owner key, mandates, revocations and decision log live here: keep it on a volume.
VOLUME /data
ENV WARDEN_DATA=/data PORT=8080 RUST_LOG=info
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD curl -fsS "http://127.0.0.1:${PORT}/health" || exit 1
CMD ["mandate-warden"]

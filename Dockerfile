# Mandate's hosted API: mandate-server (Rust) in front of arkiv-adapter (Node),
# in one container. scripts/container-start.sh runs both. See docs/deploy.md.

FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY examples examples
RUN cargo build --release -p mandate-server

FROM node:24-bookworm-slim AS web
WORKDIR /src/apps/web
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci
COPY apps/web ./
RUN npm run build

FROM node:24-bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY apps/arkiv-adapter/package.json apps/arkiv-adapter/package-lock.json apps/arkiv-adapter/
RUN cd apps/arkiv-adapter && npm ci --omit=dev
COPY apps/arkiv-adapter/src apps/arkiv-adapter/src
COPY --from=server /src/target/release/mandate-server /usr/local/bin/mandate-server
# The server also serves the UI, so the Render URL works on its own too.
COPY --from=web /src/apps/web/dist apps/web/dist
COPY scripts/container-start.sh /usr/local/bin/container-start
USER node
ENV HOST=0.0.0.0 PORT=8080 WEB_DIR=/app/apps/web/dist RUST_LOG=info
EXPOSE 8080
CMD ["container-start"]

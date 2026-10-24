# The Cantara server, with the web build it serves.
#
#   docker build -t cantara-server .
#
# Run it with `deploy/docker-compose.yml`, which brings PostgreSQL along; every
# setting is an environment variable, listed in `deploy/.env.example`. What the
# server is and why it is built so: `docs/specs/0008-server-version.md`.

# ── Build ─────────────────────────────────────────────────────────────────────
# Debian 13: the prebuilt dx that `cargo binstall` fetches is linked against
# glibc 2.39, which bookworm (2.36) does not have.
FROM rust:1-trixie AS build

# The dx that builds the web app has to be the version of the `dioxus` crate —
# see the README. npm brings the stylesheets the app embeds.
ARG DIOXUS_CLI_VERSION=0.8.0-alpha.3
RUN apt-get update \
 && apt-get install -y --no-install-recommends nodejs npm \
 && rm -rf /var/lib/apt/lists/* \
 && rustup target add wasm32-unknown-unknown \
 && curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
 && cargo binstall -y --locked "dioxus-cli@${DIOXUS_CLI_VERSION}"

WORKDIR /src
COPY package.json package-lock.json ./
RUN npm ci
COPY . .

RUN cargo build --release --locked -p cantara-server

# The browser build, in its server flavour: it talks to the server it was
# loaded from, and lives at the root rather than under `/Cantara` as the
# GitHub Pages demo does.
RUN dx build --platform web --release --locked --features server-client --base-path / \
 && cp -r target/dx/cantara/release/web/public /web

# ── Run ───────────────────────────────────────────────────────────────────────
FROM debian:trixie-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /data cantara \
 && mkdir -p /data/blobs \
 && chown -R cantara /data

COPY --from=build /src/target/release/cantara-server /usr/local/bin/cantara-server
COPY --from=build /web /app/web

ENV CANTARA_LISTEN=0.0.0.0:8080 \
    CANTARA_WEB_DIR=/app/web \
    CANTARA_BLOB_DIR=/data/blobs \
    CANTARA_LOG_FORMAT=json

USER cantara
VOLUME /data
EXPOSE 8080
CMD ["cantara-server"]

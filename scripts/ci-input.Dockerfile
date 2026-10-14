FROM rust:1.99.0-bookworm@sha256:114c7a4425406451c2866b6aafe69fe29b1b298832db1277d411ac73c82d04d6 AS rust
WORKDIR /input
COPY input/ /input/
RUN cargo vendor --locked /vendor > /cargo-config.toml && \
    cargo build --locked --release -p forge-ci-input

FROM mcr.microsoft.com/playwright:v1.64.0-noble@sha256:06a9939e57531807f8d5fd76ce44b53165ffb7d7501d87ab10e285c20b1e971f
ARG SOURCE_REVISION
ARG MANIFEST_DIGEST
LABEL numera.forge.input.manifest=$MANIFEST_DIGEST
LABEL numera.forge.input.source=$SOURCE_REVISION
USER root
RUN apt-get update && apt-get install -y --no-install-recommends build-essential git python3 && \
    rm -rf /var/lib/apt/lists/* /root/.cache /root/.npm
COPY --from=rust /usr/local/cargo/bin /usr/local/cargo/bin
COPY --from=rust /usr/local/rustup /usr/local/rustup
COPY --from=rust /vendor /deps/vendor
COPY --from=rust /cargo-config.toml /deps/cargo-config.toml
COPY --from=rust /input/target/release/forge-ci-input /usr/local/bin/forge-ci-input
COPY input/ /input/
COPY manifest.json /input/manifest.json
WORKDIR /input/apps/desktop
RUN npm ci --ignore-scripts && rm -rf /root/.npm && \
    sed -i 's|/vendor|/deps/vendor|g' /deps/cargo-config.toml && \
    mkdir -p /scratch /input/.ci-work /input/.forge-state /input/apps/desktop/dist \
      /input/apps/desktop/coverage /input/apps/desktop/node_modules/.vite /input/apps/desktop/node_modules/.vite-temp && \
    chmod -R a+rX /deps /usr/local/cargo /usr/local/rustup && \
    chmod 755 /usr/local/bin/forge-ci-input && \
    touch /input/control-write-probe /deps/control-write-probe && \
    chmod 666 /input/control-write-probe /deps/control-write-probe
USER 1000:1000
WORKDIR /input
ENTRYPOINT ["/usr/local/bin/forge-ci-input"]

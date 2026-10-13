# Build context is scripts/, never the repository or a personal home.
# Official node:24.21.0-bookworm-slim Linux amd64 manifest verified 2026-10-12 UTC.
FROM node:26.11.1-bookworm-slim@sha256:86f07bc9c5dce4578cf37e5a418b7bfc7f817cda25cde66e2b66e95ed86c4567 AS dependencies
WORKDIR /opt/pi
COPY host-activation/pi/package.json host-activation/pi/package-lock.json ./
RUN npm ci --ignore-scripts --no-audit --no-fund --registry=https://registry.npmjs.org \
    && node -e 'const fs=require("node:fs"),p=JSON.parse(fs.readFileSync("node_modules/@earendil-works/pi-coding-agent/package.json","utf8"));if(p.name!=="@earendil-works/pi-coding-agent"||p.version!=="1.2.0")throw Error("Pi identity mismatch")'
WORKDIR /opt/gemini
COPY host-activation/gemini/package.json host-activation/gemini/package-lock.json ./
RUN npm ci --ignore-scripts --no-audit --no-fund --registry=https://registry.npmjs.org \
    && npm audit --ignore-scripts --registry=https://registry.npmjs.org \
    && node -e 'const fs=require("node:fs"),p=JSON.parse(fs.readFileSync("node_modules/@google/gemini-cli-core/package.json","utf8"));if(p.name!=="@google/gemini-cli-core"||p.version!=="0.63.0")throw Error("Gemini identity mismatch")'

FROM node:26.11.1-bookworm-slim@sha256:86f07bc9c5dce4578cf37e5a418b7bfc7f817cda25cde66e2b66e95ed86c4567
COPY --from=dependencies /opt/pi /opt/pi
COPY --from=dependencies /opt/gemini /opt/gemini
COPY host-activation-probe.mjs /opt/forge/host-activation-probe.mjs
RUN mkdir /scratch && chmod 1777 /scratch
USER 1000:1000
WORKDIR /opt/forge
# Runtime credentials/settings are never inherited from image or Docker defaults.
ENTRYPOINT ["/usr/bin/env", "-i", "PATH=/usr/local/bin:/usr/bin:/bin", "HOME=/scratch/home", "LANG=C.UTF-8", "LC_ALL=C.UTF-8", "TMPDIR=/scratch/tmp", "XDG_CONFIG_HOME=/scratch/config", "XDG_CACHE_HOME=/scratch/cache", "XDG_DATA_HOME=/scratch/data", "PI_OFFLINE=1", "PI_CODING_AGENT_DIR=/scratch/pi-agent", "GEMINI_CLI_HOME=/scratch/gemini-home", "GEMINI_CLI_NO_RELAUNCH=1", "GEMINI_CLI_SYSTEM_SETTINGS_PATH=/scratch/config/system.json", "GEMINI_CLI_SYSTEM_DEFAULTS_PATH=/scratch/config/defaults.json", "/usr/local/bin/node", "/opt/forge/host-activation-probe.mjs"]

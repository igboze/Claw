# One image = one code hash. The attestation covers both the brain (IronClaw) and the
# signer (claw-bridge), plus the clawdao WASM tools and the agent's skills.
#
# Reproducibility: every base image is pinned by digest, Cargo/pnpm lockfiles are
# used with --locked/--frozen-lockfile, and SOURCE_DATE_EPOCH is fixed. Release CI
# refuses to build if IRONCLAW_IMAGE is not pinned by digest.
#
# Build from the repo root:
#   docker build -f agent/Dockerfile \
#     --build-arg IRONCLAW_IMAGE=ghcr.io/nearai/ironclaw@sha256:<digest> -t clawdao-agent .

ARG IRONCLAW_IMAGE
ARG SOURCE_DATE_EPOCH=1767225600

# ---- clawdao WASM tools ----
FROM rust:1.93-bookworm AS tools
RUN rustup target add wasm32-wasip2
WORKDIR /src
COPY agent/tools/ agent/tools/
RUN cd agent/tools && cargo build --release --locked --target wasm32-wasip2 \
    && mkdir -p /out && cp target/wasm32-wasip2/release/*.wasm /out/

# ---- claw-bridge ----
FROM node:22-bookworm-slim AS bridge
RUN corepack enable
WORKDIR /src
COPY package.json pnpm-workspace.yaml pnpm-lock.yaml tsconfig.base.json ./
COPY packages/types packages/types
COPY packages/near packages/near
COPY agent/bridge agent/bridge
RUN pnpm install --frozen-lockfile --filter @clawdao/claw-bridge... \
    && pnpm --filter @clawdao/types --filter @clawdao/near --filter @clawdao/claw-bridge build \
    && pnpm --filter @clawdao/claw-bridge deploy --prod /out

# ---- runtime: upstream IronClaw + bridge + tools ----
FROM ${IRONCLAW_IMAGE} AS runtime
USER root
COPY --from=bridge /usr/local/bin/node /usr/local/bin/node
COPY --from=bridge /out /opt/claw-bridge
COPY --from=tools /out /opt/clawdao/tools/dist
COPY agent/tools/manifest.toml /opt/clawdao/tools/manifest.toml
COPY agent/ironclaw/ /opt/clawdao/ironclaw/
COPY agent/entrypoint.sh /usr/local/bin/clawdao-entrypoint
RUN chmod +x /usr/local/bin/clawdao-entrypoint

# The bridge listens on 127.0.0.1 only; the IronClaw web UI stays on localhost too.
ENV BRIDGE_PORT=3140 \
    MARKET_API_URL=https://market.near.ai \
    IRONCLAW_REBORN_SERVE_HOST=127.0.0.1 \
    ROUTINES_ENABLED=true \
    ROUTINES_CRON_INTERVAL=60

ENTRYPOINT ["clawdao-entrypoint"]

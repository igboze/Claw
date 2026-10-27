# Image for the TypeScript services (api, jobs, notifier, bounty-intake). The compose
# file picks the entrypoint per service.
FROM node:22-bookworm-slim AS build
RUN corepack enable
WORKDIR /app
COPY . .
RUN pnpm install --frozen-lockfile \
    && pnpm --filter "./packages/*" --filter "./services/*" build

FROM node:22-bookworm-slim
RUN corepack enable
WORKDIR /app
COPY --from=build /app /app
ENV NODE_ENV=production
USER node

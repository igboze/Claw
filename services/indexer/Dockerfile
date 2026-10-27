FROM rust:1.96-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/clawdao-indexer /usr/local/bin/clawdao-indexer
# NEAR Lake is a requester-pays S3 bucket: provide AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY.
ENTRYPOINT ["clawdao-indexer"]

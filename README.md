# clawdao — IronClaw Coin

IronClaw Coin (`CLAW`) is a NEP-141 token whose holders govern a fully autonomous
[IronClaw](https://github.com/nearai/ironclaw) agent through an on-chain, tiered
constitution. After launch no person holds a key that can change the agent, its
prompt, or its treasury.

- **No human keys after launch.** Admin keys are deleted; the DAO owns upgrades and code hashes.
- **Chain is the source of truth.** The database is a cache of chain state and can be rebuilt from block zero.
- **Verifiable by anyone.** Every agent action carries the constitution hash and mission ID and links to a public reasoning log.
- **Contract beats prompt.** Guardrails live in contracts, so a jailbreak cannot move more than the caps allow.
- **One language per layer.** Rust for contracts and agent tools, TypeScript for the API, bridge and web app.

See [docs/architecture.md](docs/architecture.md) for the full design and
[docs/threat-model.md](docs/threat-model.md) for the risks it defends against.

## Layout

```
contracts/      Rust Cargo workspace: token, ve, governor, constitution, agent (+ common, tests)
agent/          IronClaw config + skills, custom WASM tools, claw-bridge signer, Dockerfile
services/       indexer (Rust), api, jobs, notifier, bounty-intake (TypeScript)
apps/web/       Next.js dApp
packages/       db (Drizzle schema + SQL migrations), types, near (contract clients)
constitution/   genesis.md — Tier 0 to 3 launch text
infra/          docker-compose, Phala/Dstack config, Terraform
docs/           architecture, threat model, runbooks
```

## Quick start (local)

Requirements: Rust (see `rust-toolchain.toml`), `cargo-near`, Node 22+, pnpm 9, Docker.

```bash
pnpm install
docker compose -f infra/docker-compose.yml up -d postgres redis
pnpm db:migrate
cd contracts && cargo test          # contract unit tests
cargo near build                    # per contract, reproducible wasm
pnpm dev                            # api, jobs, web
```

Integration tests (`contracts/tests`) need the wasm builds and download a NEAR sandbox:

```bash
cd contracts && ./build.sh && cargo test -p clawdao-integration-tests -- --ignored
```

## Contracts

| Contract | Account | Changed by |
| --- | --- | --- |
| Token | `claw.near` | Nobody (keys deleted) |
| Vote escrow | `ve.claw.near` | Governor, Tier 1 |
| Governor | `gov.claw.near` | Governor, Tier 1 |
| Constitution | `constitution.claw.near` | Tier 0 nobody; Tiers 1–3 Governor; Tier 4 agent contract |
| Agent | `agent.claw.near` | Code hashes and guardrails: Governor only |

## Status

Pre-audit. Do not deploy to mainnet until every item in the
[launch checklist](docs/launch-checklist.md) is done.

## License

MIT. See [LICENSE](LICENSE).

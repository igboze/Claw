# Security policy

clawdao controls a treasury through contracts and an autonomous agent. We take
reports seriously and pay for valid findings from the bug bounty pool.

## Reporting

- Email **security@clawdao.org** (PGP key in `docs/security-pgp.asc` once published), or
- open a private advisory: GitHub → Security → "Report a vulnerability".

Please do not open public issues for vulnerabilities. Include the affected
contract or service, a description, and reproduction steps. We aim to reply
within 72 hours.

## Scope

In scope: everything under `contracts/`, `agent/bridge/`, `agent/tools/`, and the
reproducible build pipeline. Prompt-injection findings count when they produce an
action that passes `spend_check` but violates the constitution.

Out of scope: third-party services (Phala, NEAR AI, Vercel), social engineering,
denial of service against public RPC endpoints.

## Bounty

Payouts are made by a Tier 3 "security" mission and appear as public receipts.
Severity follows the Immunefi v2.3 classification.

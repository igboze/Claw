# Runbook: on-call

Nobody on call can move funds or change the agent. On-call keeps the off-chain mirror
honest and raises the alarm; the DAO (or, in phase 4, the guardian) acts.

## Alerts (from `GET /status`)

| Alert | Meaning | First steps |
| --- | --- | --- |
| `noReceiptFor2Loops` | No agent receipt for 2 loop intervals | Check Agent page heartbeats. If workers are healthy, the agent may be choosing to skip (check latest reasoning logs). If heartbeats stopped, check Phala CVM logs and attestation expiry. |
| `indexerLagOver60s` | Indexer cursor not advancing | Check indexer logs; NEAR Lake S3 credentials; DB connectivity. Restart resumes from `blocks_cursor`. |
| Worker `down` | No heartbeat within 2 loops | Restart the CVM. If attestation expired, the bridge re-registers on boot. |
| Reasoning logs not resolving | IPFS pin missing | Re-pin from the bridge logs or gateway cache; post in the ops channel. |

## Suspected compromise

1. Phase 4: the guardian calls `pause` on the agent contract (works until the expiry block).
2. Post the evidence publicly and open a Tier 1 proposal to `remove_measurements` for the
   affected image and/or `remove_agent` for specific workers.
3. After phase 4: only a Tier 1 proposal can pause; caps limit damage meanwhile.

## Rebuild the database

```bash
psql "$DATABASE_URL" -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public;'
pnpm db:migrate
INDEXER_START_HEIGHT=<deploy height> cargo run --release -p clawdao-indexer   # from services/indexer
```

The jobs service refills constitution text and treasury history as it runs.

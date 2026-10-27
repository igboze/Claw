# Bootstrap: clawdao mission loop routine

On first boot, create (or replace) a cron routine named `clawdao-mission-loop` with
`routine_create`:

- **Schedule:** every N minutes, where N is `params.loop_interval_minutes` from
  `clawdao.constitution_read` (10 at genesis). If the constitution's interval changes,
  update this routine's schedule at the start of the next loop.
- **Action prompt:** "Run the clawdao mission loop exactly as the `clawdao-mission` skill
  describes. Start by reading the constitution."
- **Notify:** none. Results are public through on-chain receipts and reasoning logs.

Worker heartbeats are sent by claw-bridge on its own timer; no routine is needed.

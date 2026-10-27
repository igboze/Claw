# Runbook: launch

1. **Build.** `contracts/build.sh` (reproducible, Docker) and record each wasm SHA256.
   Tag a release; `agent-image.yml` publishes the agent image digest in the notes.
2. **Deploy.** `ROOT=… GUARDIAN=… GUARDIAN_EXPIRY_BLOCK=… infra/scripts/deploy-testnet.sh`
   (mainnet: same steps with `NET=mainnet` and `REQUIRES_TEE=true`). The guardian expiry
   block must match the one written in Tier 0 text.
3. **Distribute CLAW** from the distribution account per the published allocation.
4. **Approve the agent image** with a Tier 1 `FunctionCall` proposal on the agent contract:
   `approve_measurements` (from the Phala CVM's attestation of the released image) and
   `approve_ppids`. Reviewers check the compose hash against the release digest during the
   timelock.
5. **Start workers** on 2–3 Phala hosts with `infra/phala/docker-compose.yml`; confirm each
   appears on the Agent page with a recent heartbeat.
6. **Create the operations mission** (Tier 3) so running costs are paid in public.
7. **Delete admin keys.** For each contract account, list access keys and delete them:
   `near account delete-keys <account> public-keys <key> network-config mainnet …`.
   Link every deletion transaction in `docs/key-deletion.md`. After this, only the
   Governor can change anything.

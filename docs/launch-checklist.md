# Roadmap and launch checklist

No phase starts until the gate before it is passed. The agent only gets full control in
phase 5.

| Phase | What | Gate to pass |
| --- | --- | --- |
| 1. Build | Contracts, bridge, tools, indexer, web on local + sandbox | Unit and integration tests green |
| 2. Testnet | Full stack on testnet with real TEE workers | A proposal changes a mission and the agent picks it up within one loop |
| 3. Audit | External contract audit; second review of the bridge | All high and medium findings fixed |
| 4. Guarded mainnet | Mainnet with small caps; guardian may pause | No incidents for the guardian period; guardian expiry block written in Tier 0 |
| 5. Autonomy | Guardian expires; caps raised by vote | — |

The guardian in phase 4 can only pause, never move funds, and its expiry block is written
into Tier 0 at launch.

## Checklist

- [ ] GitHub organization created; repo public with SECURITY.md and license
- [ ] Contracts pass near-workspaces tests, including stale-hash and timelock cases
- [ ] Reproducible builds verified for all five contracts and the agent image
- [ ] Tier 0 text final and reviewed by the community for at least two weeks
- [ ] Red-team prompt suite runs clean on testnet
- [ ] External audit done and all high and medium findings fixed
- [ ] Legal opinion on the token for the UK and Nigeria
- [ ] Status page, alerts, and on-call runbook live
- [ ] Bug bounty published
- [ ] Admin keys deleted; deletion transactions linked in the docs
- [ ] Confirm with NEAR AI whether NEAR AI Cloud attestation works with the Shade agent contract
- [ ] Bounty submissions require a NEP-413 signature from the payee account
- [ ] Rhea/Ref swap message and fee-on-transfer sells verified on testnet; trade page quotes with `net_of_fee`
- [ ] CLAW whitelisted on Rhea/Ref; CLAW/wNEAR pool created; `launch_set_pools` called on the fee router
- [ ] Fee router registered on wNEAR and Rhea (`setup`); keeper relayer funded with gas
- [ ] Legal opinion covers the 2% fee, the 20% dev share and holder rewards
- [ ] Confirm the treasury's contract-owned lockup earns HSP-003 veNEAR rewards

import { and, eq, isNull, ne, or } from "drizzle-orm";
import { connect, KeyPair, keyStores } from "near-api-js";
import { hosMandates, hosProposals, treasurySnapshots } from "@clawdao/db";
import { HosViews, NearRpc, type HosProposal } from "@clawdao/near";
import type { Deps } from "./deps.js";

const NETWORK = process.env.NEAR_NETWORK ?? "testnet";
const VENEAR = process.env.VENEAR_CONTRACT ?? (NETWORK === "mainnet" ? "venear.dao" : "");
const VOTING = process.env.HOS_VOTING_CONTRACT ?? (NETWORK === "mainnet" ? "vote.dao" : "");

function hosViews(rpcUrl: string) {
  if (!VENEAR || !VOTING) return null;
  return new HosViews(new NearRpc(rpcUrl), VENEAR, VOTING);
}

const nsToDate = (ns?: string | null) => (ns ? new Date(Number(BigInt(ns) / 1_000_000n)) : null);

/** Mirror vote.dao proposals so the web app can list them next to clawdao's mandate. */
export async function syncHosProposals({ db, views }: Deps) {
  const hos = hosViews(views.rpc.url);
  if (!hos) return 0;
  const total = await hos.numProposals();
  const from = Math.max(0, total - 100);
  const rows: HosProposal[] = [];
  for (let i = from; i < total; i += 50) rows.push(...(await hos.proposals(i, 50)));

  for (const p of rows) {
    const start = nsToDate(p.voting_start_time_ns);
    const ends = start ? new Date(start.getTime() + Number(BigInt(p.voting_duration_ns) / 1_000_000n)) : null;
    const values = {
      id: p.id,
      title: p.title ?? null,
      description: p.description ?? null,
      link: p.link ?? null,
      proposer: p.proposer_id,
      status: p.status,
      snapshotBlock: p.snapshot_and_state?.snapshot.block_height ?? null,
      votingStart: start,
      votingEnds: ends,
      votes: p.votes,
      updatedAt: new Date(),
    };
    await db.insert(hosProposals).values(values).onConflictDoUpdate({ target: hosProposals.id, set: values });
  }
  return rows.length;
}

/**
 * Relay CLAW holders' mandates to vote.dao. Anyone could do this: the agent contract only
 * casts the stored mandate and vote.dao verifies the proof. The relayer key only pays gas.
 */
export async function relayHosVotes({ db, views }: Deps) {
  const archival = process.env.ARCHIVAL_RPC_URL ?? views.rpc.url;
  const hos = hosViews(archival);
  const relayerId = process.env.RELAYER_ACCOUNT_ID;
  const relayerKey = process.env.RELAYER_PRIVATE_KEY;
  if (!hos || !relayerId || !relayerKey) return 0;

  const due = await db
    .select({ m: hosMandates, p: hosProposals })
    .from(hosMandates)
    .innerJoin(hosProposals, eq(hosProposals.id, hosMandates.hosProposalId))
    .where(and(eq(hosProposals.status, "Voting"), or(isNull(hosMandates.castVote), ne(hosMandates.castVote, hosMandates.vote))));
  if (!due.length) return 0;

  const keyStore = new keyStores.InMemoryKeyStore();
  await keyStore.setKey(NETWORK, relayerId, KeyPair.fromString(relayerKey as `ed25519:${string}`));
  const near = await connect({ networkId: NETWORK, nodeUrl: views.rpc.url, keyStore } as unknown as Parameters<typeof connect>[0]);
  const relayer = await near.account(relayerId);

  let sent = 0;
  for (const { m, p } of due) {
    if (!p.snapshotBlock) continue;
    try {
      // Proof of the treasury's veNEAR in the snapshot taken when HoS approved the proposal.
      const [merkleProof, vAccount] = await hos.proof(views.accounts.agent, p.snapshotBlock);
      await relayer.functionCall({
        contractId: views.accounts.agent,
        methodName: "hos_vote",
        args: { proposal_id: m.hosProposalId, merkle_proof: merkleProof, v_account: vAccount },
        gas: 100_000_000_000_000n,
      });
      sent++;
    } catch (e) {
      console.error(`hos relay for proposal ${m.hosProposalId} failed:`, (e as Error).message);
    }
  }
  return sent;
}

/** Add the treasury's House of Stake position to the hourly treasury snapshot. */
export async function snapshotHosPosition({ db, views }: Deps) {
  const hos = hosViews(views.rpc.url);
  if (!hos) return null;
  const info = await views.hosInfo().catch(() => null);
  if (!info?.lockup_account_id) return null;
  const [venear, lockupBalance] = await Promise.all([
    hos.venearBalance(views.accounts.agent).catch(() => "0"),
    views.rpc.view<string>(info.lockup_account_id, "get_balance").catch(() => "0"),
  ]);
  await db.insert(treasurySnapshots).values([
    { chain: "near", asset: "near:hos-lockup", amount: lockupBalance, usdValue: null },
    { chain: "near", asset: "venear", amount: venear, usdValue: null },
  ]);
  return { venear, lockupBalance };
}

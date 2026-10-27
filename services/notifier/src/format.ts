import { formatAmount } from "@clawdao/near";

type P = Record<string, unknown>;
const str = (v: unknown) => (v === undefined || v === null ? "" : String(v));

/** Turn an indexed event into a short public message, or null to stay quiet. */
export function formatEvent(kind: string, d: P, web: string): string | null {
  switch (kind) {
    case "proposal_created":
      return `🗳 New Tier ${str(d.tier)} proposal #${str(d.id)}: "${str(d.title)}". ${web}/proposals/${str(d.id)}`;
    case "proposal_finalized":
      return d.status === "Queued"
        ? `✅ Proposal #${str(d.proposal_id)} passed. Timelock running; executable after it ends. ${web}/proposals/${str(d.proposal_id)}`
        : `❌ Proposal #${str(d.proposal_id)} was defeated.`;
    case "proposal_executed":
      return d.success ? `⚙️ Proposal #${str(d.proposal_id)} executed on-chain.` : `⚠️ Proposal #${str(d.proposal_id)} execution failed.`;
    case "receipt":
      return `🧾 Agent action on mission ${str(d.mission_id)}: ${formatAmount(str(d.amount) || "0")}. Reasoning: ${str(d.reasoning_uri)} ${web}/missions/${encodeURIComponent(str(d.mission_id))}`;
    case "mission_set":
      return `🎯 Mission "${str(d.title)}" set: budget ${formatAmount(str(d.budget) || "0")} ${str(d.asset)}. ${web}/missions/${encodeURIComponent(str(d.id))}`;
    case "constitution_updated":
      return d.reason === "genesis" ? null : `📜 Constitution now v${str(d.version)} (${str(d.hash).slice(0, 12)}…). ${web}/constitution`;
    case "hos_mandate":
      return `🏛 CLAW holders decided the treasury votes ${str(d.vote)} on House of Stake proposal #${str(d.hos_proposal_id)}. ${web}/hos`;
    case "hos_vote":
      return d.success ? `🏛 Treasury veNEAR vote ${str(d.vote)} cast on House of Stake proposal #${str(d.hos_proposal_id)}.` : null;
    case "fee_mode_unlocked":
      return `📈 CLAW held a $100k market cap for 3 days. Holders can now vote on where trading fees go. ${web}/fees`;
    case "fee_mode_set":
      return `🗳 Trading fees now go to: ${str(d.mode)}. ${web}/fees`;
    case "buyback":
      return Number(d.claw_bought ?? 0) > 0 ? `🔥 Buyback burned ${formatAmount(str(d.claw_bought))} CLAW.` : null;
    case "paused":
      return `⏸ Agent paused by ${str(d.by)}.`;
    default:
      return null;
  }
}

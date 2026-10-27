// Mirrors of the on-chain JSON shapes. Reference ABIs come from `cargo near abi`
// in each contract; keep these in sync when a contract changes.

export type U128 = string;
export type U64 = string;
export type Tier = 0 | 1 | 2 | 3 | 4;
export type Asset = string;

export interface Article {
  id: string;
  title: string;
  body: string;
}
export interface TieredArticle extends Article {
  tier: Tier;
}
export interface ConstitutionParams {
  loop_interval_minutes: number;
  daily_caps: [Asset, U128][];
}
export interface ConstitutionView {
  version: number;
  hash: string;
  articles: TieredArticle[];
  params: ConstitutionParams;
}

export type MissionStatus = "Active" | "Exhausted" | "Expired" | "Cancelled";
export interface MissionInput {
  id: string;
  title: string;
  goal: string;
  asset: Asset;
  budget: U128;
  max_per_action: U128;
  deadline: U64;
  metric: string;
  allow_chain_sig: boolean;
}
export interface MissionView extends MissionInput {
  spent: U128;
  cancelled: boolean;
  status: MissionStatus;
}

export interface Receipt {
  id: number;
  task_id: string;
  mission_id: string;
  constitution_hash: string;
  action_hash: string;
  reasoning_uri: string;
  amount: U128;
  block_height: U64;
  timestamp: U64;
}

export type KeyType = "Ecdsa" | "Eddsa";
export type AgentAction =
  | { NativeTransfer: { receiver_id: string; amount: U128 } }
  | { FtTransfer: { token_id: string; receiver_id: string; amount: U128 } }
  | {
      ChainSignature: {
        path: string;
        payload_hex: string;
        key_type: KeyType;
        asset: Asset;
        value: U128;
      };
    };

export type ProposalStatus = "Active" | "Defeated" | "Queued" | "Executing" | "Executed" | "Failed";
export type Choice = "For" | "Against" | "Abstain";
export type ProposalKind =
  | { Amend: { article_json: string } }
  | { RemoveArticle: { article_id: string } }
  | { SetParams: { params_json: string } }
  | { SetMission: { mission_json: string } }
  | { CancelMission: { mission_id: string } }
  | {
      FunctionCall: {
        receiver_id: string;
        actions: { method_name: string; args: string; deposit: U128; gas: U64 }[];
      };
    }
  | { HosManage: { method: HosManageMethod; args_json: string } }
  | { HosVote: { hos_proposal_id: number } };

/** Agent contract methods a Tier 2 HosManage proposal may call. */
export const HOS_MANAGE_METHODS = ["hos_lock", "hos_begin_unlock", "hos_end_unlock", "hos_withdraw", "hos_staking"] as const;
export type HosManageMethod = (typeof HOS_MANAGE_METHODS)[number];

export interface HosInfo {
  config: { venear_id: string; voting_id: string; max_locked: U128; vote_storage_fee: U128 } | null;
  lockup_account_id: string | null;
  locked: U128;
}

export interface HosMandate {
  vote: Choice;
  clawdao_proposal_id: number;
  cast: Choice | null;
  pending: boolean;
}

export interface Proposal {
  id: number;
  proposer: string;
  tier: Tier;
  kind: ProposalKind;
  title: string;
  body_uri: string;
  bond: U128;
  snapshot_ts: U64;
  ends_at: U64;
  eta: U64 | null;
  votes_for: U128;
  votes_against: U128;
  votes_abstain: U128;
  status: ProposalStatus;
  /** HosVote winner, set at finalize. */
  outcome: Choice | null;
}

export interface LockView {
  account_id: string;
  amount: U128;
  unlock_at: U64;
  power: U128;
}

/** NEP-297 event envelope emitted by every clawdao contract. */
export interface ClawEvent<T = Record<string, unknown>> {
  standard: string;
  version: string;
  event: string;
  data: T[];
}

/** loop_epoch for task_id = sha256(mission_id ":" loop_epoch): every worker derives the same id per loop. */
export function loopEpoch(nowMs: number, intervalMinutes: number): number {
  return Math.floor(nowMs / (intervalMinutes * 60_000));
}

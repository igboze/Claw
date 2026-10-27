export const NETWORK = (process.env.NEXT_PUBLIC_NEAR_NETWORK ?? "testnet") as "testnet" | "mainnet";
export const API_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:4000";
export const BOUNTY_URL = process.env.NEXT_PUBLIC_BOUNTY_URL ?? "http://localhost:4100";
export const REPO_URL = "https://github.com/igboze/Claw";

const suffix = NETWORK === "mainnet" ? "near" : "testnet";
export const CONTRACTS = {
  token: process.env.NEXT_PUBLIC_TOKEN_CONTRACT ?? `claw.${suffix}`,
  ve: process.env.NEXT_PUBLIC_VE_CONTRACT ?? `ve.claw.${suffix}`,
  governor: process.env.NEXT_PUBLIC_GOVERNOR_CONTRACT ?? `gov.claw.${suffix}`,
  constitution: process.env.NEXT_PUBLIC_CONSTITUTION_CONTRACT ?? `constitution.claw.${suffix}`,
  agent: process.env.NEXT_PUBLIC_AGENT_CONTRACT ?? `agent.claw.${suffix}`,
};

export const EXPLORER = NETWORK === "mainnet" ? "https://nearblocks.io" : "https://testnet.nearblocks.io";
export const IPFS_GATEWAY = process.env.NEXT_PUBLIC_IPFS_GATEWAY ?? "https://ipfs.io/ipfs/";

export const TIER_NAMES: Record<number, string> = {
  0: "Tier 0 · Core (immutable)",
  1: "Tier 1 · Constitution",
  2: "Tier 2 · Operations",
  3: "Tier 3 · Missions",
  4: "Tier 4 · Receipts",
};

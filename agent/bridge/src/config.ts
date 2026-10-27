import { accountsFromEnv, type Accounts } from "@clawdao/near";

export interface BridgeConfig {
  network: "testnet" | "mainnet";
  rpcUrl: string;
  accounts: Accounts;
  port: number;
  sponsor?: { accountId: string; privateKey: string };
  localDerivationPath?: string;
  mpcContractId: string;
  ipfsApiUrl?: string;
  ipfsJwt?: string;
  evmRpcUrls: Record<string, string>;
  /** Agent Market `aat_` token for the listed self-hosted agent. Unset = market disabled. */
  marketToken?: string;
  marketApiUrl: string;
}

export function loadConfig(env = process.env): BridgeConfig {
  const network = (env.NEAR_NETWORK ?? "testnet") as BridgeConfig["network"];
  const sponsor =
    env.SPONSOR_ACCOUNT_ID && env.SPONSOR_PRIVATE_KEY
      ? { accountId: env.SPONSOR_ACCOUNT_ID, privateKey: env.SPONSOR_PRIVATE_KEY }
      : undefined;
  // EVM_RPC_<CHAIN>=url, e.g. EVM_RPC_BASE=https://mainnet.base.org
  const evmRpcUrls = Object.fromEntries(
    Object.entries(env)
      .filter(([k, v]) => k.startsWith("EVM_RPC_") && v)
      .map(([k, v]) => [k.slice("EVM_RPC_".length).toLowerCase(), v as string]),
  );
  return {
    network,
    rpcUrl: env.NEAR_RPC_URL ?? `https://rpc.${network}.near.org`,
    accounts: accountsFromEnv(env),
    port: Number(env.BRIDGE_PORT ?? 3140),
    sponsor,
    localDerivationPath: env.LOCAL_DERIVATION_PATH || undefined,
    mpcContractId: env.MPC_CONTRACT_ID ?? (network === "mainnet" ? "v1.signer" : "v1.signer-prod.testnet"),
    ipfsApiUrl: env.IPFS_API_URL,
    ipfsJwt: env.IPFS_JWT,
    evmRpcUrls,
    marketToken: env.MARKET_AGENT_TOKEN || undefined,
    marketApiUrl: env.MARKET_API_URL ?? "https://market.near.ai",
  };
}

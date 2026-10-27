import { connect, KeyPair, keyStores } from "near-api-js";

const NETWORK = process.env.NEAR_NETWORK ?? "testnet";

type Relayer = Awaited<ReturnType<Awaited<ReturnType<typeof connect>>["account"]>>;
let cached: Relayer | null | undefined;

/**
 * A gas-only key that triggers permissionless contract steps (HoS vote relays, fee router
 * steps). It holds no authority: every call it makes is one anyone could make.
 */
export async function relayer(nodeUrl: string): Promise<Relayer | null> {
  if (cached !== undefined) return cached;
  const id = process.env.RELAYER_ACCOUNT_ID;
  const key = process.env.RELAYER_PRIVATE_KEY;
  if (!id || !key) return (cached = null);
  const keyStore = new keyStores.InMemoryKeyStore();
  await keyStore.setKey(NETWORK, id, KeyPair.fromString(key as `ed25519:${string}`));
  const near = await connect({ networkId: NETWORK, nodeUrl, keyStore } as unknown as Parameters<typeof connect>[0]);
  return (cached = await near.account(id));
}

export const TGAS = 1_000_000_000_000n;

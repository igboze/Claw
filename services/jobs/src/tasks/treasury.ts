import { treasurySnapshots } from "@clawdao/db";
import type { Deps } from "./deps.js";

const COINGECKO = "https://api.coingecko.com/api/v3/simple/price";
// Map NEP-141 token accounts to CoinGecko ids for USD values. Unknown assets get no price.
const PRICE_IDS: Record<string, { id: string; decimals: number }> = {
  near: { id: "near", decimals: 24 },
  "wrap.near": { id: "near", decimals: 24 },
  "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1": { id: "usd-coin", decimals: 6 },
};

async function prices(ids: string[]): Promise<Record<string, number>> {
  if (ids.length === 0) return {};
  const res = await fetch(`${COINGECKO}?ids=${[...new Set(ids)].join(",")}&vs_currencies=usd`);
  if (!res.ok) return {};
  const body = (await res.json()) as Record<string, { usd: number }>;
  return Object.fromEntries(Object.entries(body).map(([k, v]) => [k, v.usd]));
}

/** Hourly: NEAR balance of the agent contract plus every capped NEP-141 asset. */
export async function snapshotTreasury({ db, views }: Deps) {
  const agent = views.accounts.agent;
  const { params } = await views.constitution();
  const assets = [...new Set(["near", ...params.daily_caps.map(([a]) => a)])].filter((a) => !a.startsWith("chain:"));

  const balances = await Promise.all(
    assets.map(async (asset) => {
      const amount =
        asset === "near"
          ? await views.rpc.accountBalance(agent)
          : BigInt(await views.rpc.view<string>(asset, "ft_balance_of", { account_id: agent }).catch(() => "0"));
      return { asset, amount };
    }),
  );
  const px = await prices(balances.map((b) => PRICE_IDS[b.asset]?.id).filter((x): x is string => !!x));

  const rows = balances.map(({ asset, amount }) => {
    const meta = PRICE_IDS[asset];
    const usd = meta && px[meta.id] !== undefined ? (Number(amount) / 10 ** meta.decimals) * px[meta.id]! : null;
    return { chain: "near", asset, amount: amount.toString(), usdValue: usd === null ? null : usd.toFixed(2) };
  });
  if (rows.length) await db.insert(treasurySnapshots).values(rows);
  return rows.length;
}

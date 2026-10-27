import type { Deps } from "./deps.js";
import { relayer, TGAS } from "./relayer.js";

const MIN_DISTRIBUTE = 10n ** 23n; // 0.1 NEAR, the contract's minimum

/**
 * Drives the fee router's permissionless steps in order: sell collected CLAW for wNEAR,
 * unwrap, distribute 80/20 by the voted mode, and run queued buybacks. Each step is skipped
 * when there is nothing to do, so a failed step simply retries next run.
 */
export async function runFeeRouter({ views }: Deps) {
  const fees = views.accounts.fees;
  const acct = await relayer(views.rpc.url);
  if (!fees || !acct) return null;
  const call = (methodName: string, gasT = 300n) =>
    acct.functionCall({ contractId: fees, methodName, args: {}, gas: gasT * TGAS }).then(
      () => methodName,
      (e: Error) => `${methodName} failed: ${e.message.slice(0, 200)}`,
    );

  const done: string[] = [];
  const state = await views.feeState();
  const claw = BigInt(await views.rpc.view<string>(views.accounts.token, "ft_balance_of", { account_id: fees }));
  if (claw > 0n && state.dex.claw_pool_id > 0) done.push(await call("swap_fees"));

  const wnear = BigInt(await views.rpc.view<string>(state.dex.wrap_id, "ft_balance_of", { account_id: fees }).catch(() => "0"));
  if (wnear > 0n) done.push(await call("unwrap", 100n));

  const after = await views.feeState();
  if (BigInt(after.pending_near) >= MIN_DISTRIBUTE) done.push(await call("distribute", 200n));
  if (BigInt(after.buyback_near) > 0n && after.dex.claw_pool_id > 0) done.push(await call("buyback"));
  return done;
}

/** At least daily: the market-cap gate counts consecutive UTC days at or above $100k. */
export async function observeMarketCap({ views }: Deps) {
  const fees = views.accounts.fees;
  const acct = await relayer(views.rpc.url);
  if (!fees || !acct) return null;
  const state = await views.feeState();
  if (state.mode_unlocked || state.dex.claw_pool_id === 0) return state.mode_unlocked ? "unlocked" : "no pool yet";
  await acct.functionCall({ contractId: fees, methodName: "observe_market_cap", args: {}, gas: 100n * TGAS });
  return (await views.feeState()).streak_days;
}

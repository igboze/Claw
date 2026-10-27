import { formatAmount } from "@clawdao/near";
import { IPFS_GATEWAY } from "./config";

export { formatAmount };

export const short = (s: string, n = 8) => (s.length > n * 2 + 1 ? `${s.slice(0, n)}…${s.slice(-4)}` : s);

/** Nanosecond string (chain) or Date/ISO string (API) to a readable date. */
export function when(v: string | Date | null | undefined): string {
  if (!v) return "—";
  const d = typeof v === "string" && /^\d{16,}$/.test(v) ? new Date(Number(BigInt(v) / 1_000_000n)) : new Date(v);
  return d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export function countdown(to: string | Date | null | undefined): string {
  if (!to) return "—";
  const ms = new Date(to).getTime() - Date.now();
  if (ms <= 0) return "ended";
  const d = Math.floor(ms / 86_400_000);
  const h = Math.floor((ms % 86_400_000) / 3_600_000);
  const m = Math.floor((ms % 3_600_000) / 60_000);
  return d > 0 ? `${d}d ${h}h` : `${h}h ${m}m`;
}

export function uriToHttp(uri: string): string {
  if (uri.startsWith("ipfs://")) return IPFS_GATEWAY + uri.slice(7);
  if (uri.startsWith("ar://")) return `https://arweave.net/${uri.slice(5)}`;
  return uri;
}

export function pct(part: string | number, whole: string | number): number {
  const w = Number(whole);
  return w > 0 ? Math.min(100, (Number(part) / w) * 100) : 0;
}

/** Parse a decimal string ("12.5") to a raw integer string with `decimals`. */
export function toRaw(amount: string, decimals = 24): string {
  const [whole = "0", frac = ""] = amount.trim().split(".");
  if (!/^\d*$/.test(whole) || !/^\d*$/.test(frac)) throw new Error("Not a number");
  return (BigInt(whole || "0") * 10n ** BigInt(decimals) + BigInt((frac + "0".repeat(decimals)).slice(0, decimals) || "0")).toString();
}

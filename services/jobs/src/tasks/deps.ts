import type { Queue } from "bullmq";
import type { Redis } from "ioredis";
import type { Db } from "@clawdao/db";
import type { ClawViews } from "@clawdao/near";

export interface Deps {
  db: Db;
  views: ClawViews;
  redis: Redis;
  notify: Queue;
}

export const IPFS_GATEWAY = process.env.IPFS_GATEWAY ?? "https://ipfs.io/ipfs/";

export function gatewayUrl(uri: string): string | null {
  if (uri.startsWith("ipfs://")) return IPFS_GATEWAY + uri.slice("ipfs://".length);
  if (uri.startsWith("ar://")) return `https://arweave.net/${uri.slice("ar://".length)}`;
  if (uri.startsWith("https://")) return uri;
  return null;
}

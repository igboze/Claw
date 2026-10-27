"use client";

import { createTRPCClient, httpBatchLink } from "@trpc/client";
import { useCallback, useEffect, useState } from "react";
import type { AppRouter } from "@clawdao/api/router";
import { API_URL } from "./config";

export const api = createTRPCClient<AppRouter>({
  links: [httpBatchLink({ url: `${API_URL}/trpc` })],
});

/** Tiny fetch hook: data, error, loading, and a reload. Polls when `refreshMs` is set. */
export function useQuery<T>(fn: () => Promise<T>, deps: unknown[] = [], refreshMs?: number) {
  const [data, setData] = useState<T | undefined>();
  const [error, setError] = useState<string | undefined>();
  const [loading, setLoading] = useState(true);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const load = useCallback(fn, deps);
  const reload = useCallback(() => {
    load()
      .then((d) => {
        setData(d);
        setError(undefined);
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, [load]);
  useEffect(() => {
    reload();
    if (!refreshMs) return;
    const t = setInterval(reload, refreshMs);
    return () => clearInterval(t);
  }, [reload, refreshMs]);
  return { data, error, loading, reload };
}

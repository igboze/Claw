"use client";

import { diffLines } from "diff";
import { useMemo, useState } from "react";
import type { TieredArticle } from "@clawdao/types";
import { api, useQuery } from "@/lib/api";
import { TIER_NAMES } from "@/lib/config";
import { short } from "@/lib/format";
import { Card, CardTitle, State, cn } from "@/components/ui";

function render(articles: unknown, params: unknown): string {
  const list = ((articles ?? []) as TieredArticle[]).slice().sort((a, b) => a.tier - b.tier || a.id.localeCompare(b.id));
  const text = list.map((a) => `[Tier ${a.tier}] ${a.id}: ${a.title}\n${a.body}\n`).join("\n");
  return `${text}\n[params]\n${JSON.stringify(params ?? {}, null, 2)}\n`;
}

export default function ConstitutionPage() {
  const current = useQuery(() => api.constitution.current.query(), []);
  const versions = useQuery(() => api.constitution.versions.query(), []);
  const [from, setFrom] = useState<number | null>(null);
  const [to, setTo] = useState<number | null>(null);

  const a = versions.data?.find((v) => v.version === from);
  const b = versions.data?.find((v) => v.version === to);
  const diff = useMemo(
    () => (a?.articles && b?.articles ? diffLines(render(a.articles, a.params), render(b.articles, b.params)) : null),
    [a, b],
  );

  const tiers = [0, 1, 2] as const;
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Constitution</h1>
        {current.data && (
          <p className="mt-1 text-sm text-muted-foreground">
            Version {current.data.version} · hash <span className="font-mono">{current.data.hash}</span> · read live from chain
          </p>
        )}
      </div>
      <State loading={current.loading} error={current.error} />

      {current.data &&
        tiers.map((tier) => {
          const list = current.data!.articles.filter((x) => x.tier === tier);
          if (!list.length) return null;
          return (
            <Card key={tier}>
              <CardTitle>{TIER_NAMES[tier]}</CardTitle>
              <div className="space-y-5">
                {list.map((x) => (
                  <article key={x.id}>
                    <h3 className="font-medium">
                      {x.title} <span className="font-mono text-xs text-muted-foreground">#{x.id}</span>
                    </h3>
                    <p className="mt-1 whitespace-pre-wrap text-sm leading-relaxed">{x.body}</p>
                  </article>
                ))}
              </div>
            </Card>
          );
        })}

      {current.data && (
        <Card>
          <CardTitle>{TIER_NAMES[2]} · parameters</CardTitle>
          <dl className="grid gap-2 text-sm sm:grid-cols-2">
            <dt className="text-muted-foreground">Loop interval</dt>
            <dd>{current.data.params.loop_interval_minutes} minutes</dd>
            {current.data.params.daily_caps.map(([asset, cap]) => (
              <div key={asset} className="contents">
                <dt className="text-muted-foreground">Daily cap · {asset}</dt>
                <dd className="font-mono">{cap}</dd>
              </div>
            ))}
          </dl>
        </Card>
      )}

      <Card>
        <CardTitle>History and diff</CardTitle>
        <State loading={versions.loading} error={versions.error} />
        <p className="mb-3 text-sm text-muted-foreground">Pick two versions to compare.</p>
        <ul className="divide-y divide-border text-sm">
          {versions.data?.map((v) => (
            <li key={v.version} className="flex flex-wrap items-center gap-3 py-2">
              <span className="w-12 font-medium">v{v.version}</span>
              <span className="font-mono text-xs text-muted-foreground">{short(v.hash, 8)}</span>
              <span className="text-muted-foreground">{v.reason}</span>
              <span className="text-xs text-muted-foreground">block {v.height}</span>
              <span className="ml-auto flex gap-2">
                {(["from", "to"] as const).map((k) => {
                  const sel = k === "from" ? from === v.version : to === v.version;
                  return (
                    <button
                      key={k}
                      disabled={!v.articles}
                      onClick={() => (k === "from" ? setFrom(v.version) : setTo(v.version))}
                      className={cn("rounded border border-border px-2 py-0.5 text-xs disabled:opacity-40", sel && "bg-primary text-primary-foreground")}
                    >
                      {k}
                    </button>
                  );
                })}
              </span>
            </li>
          ))}
        </ul>
        {diff && (
          <pre className="mt-4 max-h-[32rem] overflow-auto rounded-md bg-muted p-3 text-xs leading-relaxed">
            {diff.map((part, i) => (
              <span key={i} className={cn(part.added && "bg-success/20", part.removed && "bg-danger/20 line-through")}>
                {part.value}
              </span>
            ))}
          </pre>
        )}
      </Card>
    </div>
  );
}

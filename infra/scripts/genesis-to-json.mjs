#!/usr/bin/env node
// Converts constitution/genesis.md into the `articles` argument for constitution.new.
// Usage: node infra/scripts/genesis-to-json.mjs constitution/genesis.md > genesis.json

import { readFileSync } from "node:fs";

const src = readFileSync(process.argv[2] ?? "constitution/genesis.md", "utf8");
const articles = [];
let tier = null;
let current = null;

for (const line of src.split(/\r?\n/)) {
  const t = line.match(/^## Tier (\d)/);
  if (t) {
    tier = Number(t[1]);
    current = null;
    continue;
  }
  const a = line.match(/^### ([a-z0-9-]+) — (.+)$/);
  if (a && tier !== null && tier <= 2) {
    current = { tier, id: a[1], title: a[2].trim(), body: "" };
    articles.push(current);
    continue;
  }
  if (line.startsWith("**Parameters")) {
    current = null;
    continue;
  }
  if (current) current.body += (current.body ? "\n" : "") + line;
}

for (const a of articles) a.body = a.body.trim().replace(/\n(?!\n)/g, " ");
if (!articles.some((a) => a.tier === 0)) throw new Error("genesis needs Tier 0 articles");
process.stdout.write(JSON.stringify(articles, null, 2) + "\n");

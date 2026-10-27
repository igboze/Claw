/** @type {import('next').NextConfig} */
const config = {
  // `pnpm export:ipfs` builds a static copy for the IPFS mirror, so the dApp survives
  // if the main host goes down. That copy reads everything client-side from the API/RPC.
  output: process.env.NEXT_OUTPUT === "export" ? "export" : undefined,
  transpilePackages: ["@clawdao/near", "@clawdao/types"],
  images: { unoptimized: true },
};

export default config;

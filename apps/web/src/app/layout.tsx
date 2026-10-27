import type { Metadata } from "next";
import type { ReactNode } from "react";
import { Nav } from "@/components/nav";
import { WalletProvider } from "@/lib/wallet";
import "./globals.css";

export const metadata: Metadata = {
  title: "clawdao",
  description: "IronClaw Coin: holders govern an autonomous agent through an on-chain constitution.",
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body>
        <WalletProvider>
          <Nav />
          <main className="mx-auto max-w-6xl px-4 py-8">{children}</main>
          <footer className="mx-auto max-w-6xl px-4 pb-10 text-xs text-muted-foreground">
            Facts on this site (votes, spend, receipts) come from indexed chain data. AI summaries are labelled.
          </footer>
        </WalletProvider>
      </body>
    </html>
  );
}

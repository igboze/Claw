"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useWallet } from "@/lib/wallet";
import { short } from "@/lib/format";
import { Button, cn } from "./ui";

const LINKS = [
  ["/", "Home"],
  ["/constitution", "Constitution"],
  ["/missions", "Missions"],
  ["/proposals", "Proposals"],
  ["/lock", "Lock"],
  ["/hos", "House of Stake"],
  ["/agent", "Agent"],
  ["/bounties", "Bounties"],
] as const;

export function Nav() {
  const path = usePathname();
  const { accountId, signIn, signOut } = useWallet();
  return (
    <header className="border-b border-border">
      <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-x-6 gap-y-2 px-4 py-3">
        <Link href="/" className="font-semibold tracking-tight">
          <span className="text-primary">◆</span> clawdao
        </Link>
        <nav className="flex flex-1 flex-wrap gap-1 text-sm">
          {LINKS.map(([href, label]) => (
            <Link
              key={href}
              href={href}
              className={cn(
                "rounded-md px-2.5 py-1.5 text-muted-foreground hover:bg-muted hover:text-foreground",
                (href === "/" ? path === "/" : path.startsWith(href)) && "bg-muted text-foreground",
              )}
            >
              {label}
            </Link>
          ))}
        </nav>
        {accountId ? (
          <Button variant="outline" onClick={signOut} title="Sign out">
            {short(accountId, 10)}
          </Button>
        ) : (
          <Button onClick={signIn}>Connect wallet</Button>
        )}
      </div>
    </header>
  );
}

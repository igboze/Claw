"use client";

import { setupWalletSelector, type WalletModuleFactory, type WalletSelector } from "@near-wallet-selector/core";
import { setupModal, type WalletSelectorModal } from "@near-wallet-selector/modal-ui";
import { setupMyNearWallet } from "@near-wallet-selector/my-near-wallet";
import { setupMeteorWallet } from "@near-wallet-selector/meteor-wallet";
import "@near-wallet-selector/modal-ui/styles.css";
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { CONTRACTS, NETWORK } from "./config";

const TGAS = 1_000_000_000_000n;

interface Call {
  receiverId: string;
  method: string;
  args: Record<string, unknown>;
  deposit?: string; // yoctoNEAR
  gasTgas?: number;
}

interface WalletCtx {
  accountId: string | null;
  signIn: () => void;
  signOut: () => Promise<void>;
  /** Voting and locking always happen here, signed by the holder's own wallet. */
  call: (c: Call) => Promise<unknown>;
}

const Ctx = createContext<WalletCtx | null>(null);

export function WalletProvider({ children }: { children: ReactNode }) {
  const [selector, setSelector] = useState<WalletSelector>();
  const [modal, setModal] = useState<WalletSelectorModal>();
  const [accountId, setAccountId] = useState<string | null>(null);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    setupWalletSelector({ network: NETWORK, modules: [setupMyNearWallet() as WalletModuleFactory, setupMeteorWallet() as WalletModuleFactory] }).then((s) => {
      setSelector(s);
      setModal(setupModal(s, { contractId: CONTRACTS.governor }));
      const sync = () => setAccountId(s.store.getState().accounts.find((a) => a.active)?.accountId ?? null);
      sync();
      unsub = s.store.observable.subscribe(sync).unsubscribe;
    });
    return () => unsub?.();
  }, []);

  const value: WalletCtx = {
    accountId,
    signIn: () => modal?.show(),
    signOut: async () => {
      if (!selector) return;
      await (await selector.wallet()).signOut();
    },
    call: async ({ receiverId, method, args, deposit = "0", gasTgas = 100 }) => {
      if (!selector || !accountId) throw new Error("Connect a wallet first");
      const wallet = await selector.wallet();
      return wallet.signAndSendTransaction({
        receiverId,
        actions: [
          {
            type: "FunctionCall",
            params: { methodName: method, args, gas: (BigInt(gasTgas) * TGAS).toString(), deposit },
          },
        ],
      });
    },
  };
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useWallet(): WalletCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("useWallet outside WalletProvider");
  return c;
}

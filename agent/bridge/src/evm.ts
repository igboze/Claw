// EVM support for Chain Signature actions: derive the treasury's address on a chain,
// prepare an unsigned tx, and broadcast it once the MPC signature comes back.

import { chainAdapters, contracts, utils, type MPCSignature } from "chainsig.js";
import { createPublicClient, http } from "viem";

const { toRSV, uint8ArrayToHex } = utils.cryptography;

export interface PreparedTx {
  chain: string;
  payloadHex: string;
  transaction: unknown;
  createdAt: number;
}

export class EvmSigner {
  private readonly pending = new Map<string, PreparedTx>();
  private readonly adapters = new Map<string, any>();
  private readonly mpc: InstanceType<typeof contracts.ChainSignatureContract>;

  constructor(
    network: "testnet" | "mainnet",
    mpcContractId: string,
    private readonly rpcUrls: Record<string, string>,
    private readonly agentContractId: string,
  ) {
    this.mpc = new contracts.ChainSignatureContract({ networkId: network, contractId: mpcContractId });
  }

  private adapter(chain: string): any {
    const url = this.rpcUrls[chain];
    if (!url) throw new Error(`No RPC configured for chain "${chain}" (set EVM_RPC_${chain.toUpperCase()})`);
    let a = this.adapters.get(chain);
    if (!a) {
      a = new chainAdapters.evm.EVM({ publicClient: createPublicClient({ transport: http(url) }), contract: this.mpc });
      this.adapters.set(chain, a);
    }
    return a;
  }

  /** Treasury address on `chain` for derivation `path`: owned by the agent contract via MPC. */
  async address(chain: string, path: string): Promise<string> {
    const { address } = await this.adapter(chain).deriveAddressAndPublicKey(this.agentContractId, path);
    return address;
  }

  async prepare(chain: string, path: string, to: string, data: string | undefined, value: bigint): Promise<PreparedTx> {
    const from = await this.address(chain, path);
    const { transaction, hashesToSign } = await this.adapter(chain).prepareTransactionForSigning({ from, to, data, value });
    const payloadHex = uint8ArrayToHex(hashesToSign[0]);
    const prepared = { chain, payloadHex, transaction, createdAt: Date.now() };
    this.pending.set(payloadHex, prepared);
    this.gc();
    return prepared;
  }

  has(payloadHex: string): boolean {
    return this.pending.has(payloadHex);
  }

  async broadcast(payloadHex: string, signature: MPCSignature): Promise<{ chain: string; txHash: string }> {
    const p = this.pending.get(payloadHex);
    if (!p) throw new Error("No prepared transaction for this payload");
    const adapter = this.adapter(p.chain);
    const signed = adapter.finalizeTransactionSigning({ transaction: p.transaction, rsvSignatures: [toRSV(signature)] });
    const { hash } = await adapter.broadcastTx(signed);
    this.pending.delete(payloadHex);
    return { chain: p.chain, txHash: hash };
  }

  private gc() {
    const cutoff = Date.now() - 30 * 60_000;
    for (const [k, v] of this.pending) if (v.createdAt < cutoff) this.pending.delete(k);
  }
}

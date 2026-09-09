export const RPC_URL = process.env.COSMOS_RPC_URL ?? "http://127.0.0.1:8545";

export async function rpc<T>(
  method: string,
  params: unknown[] = [],
): Promise<T> {
  const url = new URL(RPC_URL);
  if (!["127.0.0.1", "localhost", "[::1]"].includes(url.hostname)) {
    throw new Error("experiment RPC must use localhost");
  }
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    signal: AbortSignal.timeout(15000),
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const body = (await response.json()) as {
    result: T;
    error?: { message: string };
  };
  if (body.error) throw new Error(`${method}: ${body.error.message}`);
  return body.result;
}

export interface Receipt {
  transactionHash: string;
  blockHash: string;
  blockNumber: string;
  from: string;
  to: string;
  status: string;
  gasUsed: string;
  effectiveGasPrice: string;
  type: string;
}

export async function receipt(hash: string): Promise<Receipt> {
  for (let i = 0; i < 100; i++) {
    const result = await rpc<Receipt | null>("eth_getTransactionReceipt", [
      hash,
    ]);
    if (result) return result;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`transaction not included within 10 seconds: ${hash}`);
}

export async function balance(address: string) {
  return BigInt(await rpc<string>("eth_getBalance", [address, "latest"]));
}

export async function nonce(address: string) {
  return BigInt(
    await rpc<string>("eth_getTransactionCount", [address, "latest"]),
  );
}

import assert from "node:assert/strict";
import { Wallet, hexlify } from "ethers";
import { CHAIN_ID, RECIPIENT, TEST_KEY } from "./protocol.js";
import { balance, nonce, receipt, rpc } from "./rpc.js";

export async function ethereumTransfer() {
  const wallet = new Wallet(hexlify(TEST_KEY));
  const before = await balance(wallet.address);
  const receiver = await balance(RECIPIENT);
  const n = await nonce(wallet.address);
  const raw = await wallet.signTransaction({
    chainId: CHAIN_ID,
    type: 2,
    nonce: Number(n),
    to: RECIPIENT,
    value: 42n,
    gasLimit: 21000n,
    maxFeePerGas: 2000000000n,
    maxPriorityFeePerGas: 100000000n,
  });
  const hash = await rpc<string>("eth_sendRawTransaction", [raw]);
  const r = await receipt(hash);
  assert.equal(r.status, "0x1");
  assert.equal(r.from.toLowerCase(), wallet.address.toLowerCase());
  assert.equal(
    await balance(wallet.address),
    before - 42n - BigInt(r.gasUsed) * BigInt(r.effectiveGasPrice),
  );
  assert.equal(await balance(RECIPIENT), receiver + 42n);
  assert.equal(await nonce(wallet.address), n + 1n);
  console.log("Ethereum transfer verified", hash);
  return r;
}

if (process.argv[1]?.endsWith("ethereum.ts")) await ethereumTransfer();

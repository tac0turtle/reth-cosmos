import assert from "node:assert/strict";
import { encodeRlp, getBytes, hexlify } from "ethers";
import { ethereumTransfer } from "./ethereum.js";
import {
  CHAIN_ID,
  RECIPIENT,
  RECORDER,
  REVERTER,
  operation,
  payload,
  sign,
  type Operation,
} from "./protocol.js";
import { balance, nonce, receipt, rpc, type Receipt } from "./rpc.js";

export const GENESIS_HASH =
  "0x09171e117af661185d9a4a4ff7cf83d29b4b99893bc95b204c0e6e0a6d6a5272";

async function submitAndVerify(
  op: Operation,
  status = "0x1",
  duplicate = false,
) {
  const tx = await sign(op);
  const before = await balance(tx.sender);
  const toBefore = await balance(op.to);
  if (duplicate) {
    const results = await Promise.allSettled([
      rpc<string>("cosmos_sendRawTransaction", [tx.raw]),
      rpc<string>("cosmos_sendRawTransaction", [tx.raw]),
    ]);
    assert(results.some((r) => r.status === "fulfilled"));
    for (const r of results)
      if (r.status === "fulfilled") assert.equal(r.value, tx.hash);
  } else {
    assert.equal(
      await rpc<string>("cosmos_sendRawTransaction", [tx.raw]),
      tx.hash,
    );
  }
  const r = await receipt(tx.hash);
  assert.equal(r.status, status);
  assert.equal(r.from.toLowerCase(), tx.sender);
  assert.equal(r.type, "0x7e");
  const fee = BigInt(r.gasUsed) * BigInt(r.effectiveGasPrice);
  assert(fee > 0n);
  assert.equal(
    await balance(tx.sender),
    before - fee - (status === "0x1" ? op.value : 0n),
  );
  assert.equal(
    await balance(op.to),
    toBefore + (status === "0x1" ? op.value : 0n),
  );
  assert.equal(await nonce(tx.sender), op.nonce + 1n);
  const stored = await rpc<Record<string, unknown>>(
    "eth_getTransactionByHash",
    [tx.hash],
  );
  assert.equal(stored.type, "0x7e");
  assert.equal(stored.cosmosSignature, tx.signature);
  assert.equal(stored.publicKey, tx.publicKey);
  for (const field of ["v", "r", "s", "yParity"])
    assert.equal(stored[field], undefined);
  console.log(
    JSON.stringify(
      {
        cosmos: tx.cosmos,
        evm: tx.sender,
        hash: tx.hash,
        feeWei: fee.toString(),
        receipt: r,
      },
      null,
      2,
    ),
  );
  return { tx, receipt: r };
}

async function negativeCases(op: Operation) {
  const signed = await sign(op);
  const altered = getBytes(signed.raw);
  altered[altered.length - 1] = (altered[altered.length - 1] ?? 0) ^ 1;
  await assert.rejects(rpc("cosmos_sendRawTransaction", [hexlify(altered)]));
  for (const changed of [
    { ...op, chainId: CHAIN_ID + 1n },
    { ...op, genesisHash: "0x" + "22".repeat(32) },
    { ...op, value: 1n << 255n },
  ]) {
    const bad = await sign(changed);
    await assert.rejects(rpc("cosmos_sendRawTransaction", [bad.raw]));
  }
  const substituted =
    "0x7e" +
    encodeRlp([
      payload({ ...op, to: RECORDER }),
      signed.publicKey,
      signed.signature,
    ]).slice(2);
  await assert.rejects(rpc("cosmos_sendRawTransaction", [substituted]));
  for (const raw of [
    "0x",
    "0x7e",
    "0x7effffffffffffffffff",
    "0x7ezz",
    "0x7e" + "00".repeat(34000),
  ]) {
    await assert.rejects(rpc("cosmos_sendRawTransaction", [raw]));
  }
  assert.equal(await nonce(op.sender), op.nonce);
  console.log(
    "Rejected tampering, wrong domain, changed operation, malformed input, oversized input, and insufficient funds",
  );
}

export async function snapshot(receipts: Receipt[]) {
  const sender = operation(GENESIS_HASH).sender;
  return {
    head: await rpc("eth_getBlockByNumber", ["latest", false]),
    cosmosBalance: (await balance(sender)).toString(),
    cosmosNonce: (await nonce(sender)).toString(),
    ethBalance: (
      await balance("0x7e5f4552091a69125d5dfcb7b8c2659029395bdf")
    ).toString(),
    ethNonce: (
      await nonce("0x7e5f4552091a69125d5dfcb7b8c2659029395bdf")
    ).toString(),
    recipientBalance: (await balance(RECIPIENT)).toString(),
    recorder: await rpc("eth_getStorageAt", [RECORDER, "0x0", "latest"]),
    receipts: await Promise.all(
      receipts.map((r) =>
        rpc("eth_getTransactionReceipt", [r.transactionHash]),
      ),
    ),
    transactions: await Promise.all(
      receipts.map((r) => rpc("eth_getTransactionByHash", [r.transactionHash])),
    ),
  };
}

export async function demo() {
  assert.equal(BigInt(await rpc<string>("eth_chainId")), CHAIN_ID);
  assert.equal(
    (await rpc<{ hash: string }>("eth_getBlockByNumber", ["0x0", false])).hash,
    GENESIS_HASH,
  );
  const ethReceipt = await ethereumTransfer();
  const op = operation(GENESIS_HASH);
  op.nonce = await nonce(op.sender);
  const before = await balance(op.sender);
  await negativeCases(op);
  assert.equal(await balance(op.sender), before);
  const transfer = await submitAndVerify(op, "0x1", true);
  await assert.rejects(rpc("cosmos_sendRawTransaction", [transfer.tx.raw]));
  assert.equal(await nonce(op.sender), op.nonce + 1n);
  const call = await submitAndVerify({
    ...op,
    nonce: op.nonce + 1n,
    to: RECORDER,
    value: 0n,
    gasLimit: 60000n,
  });
  const recorded = await rpc<string>("eth_getStorageAt", [
    RECORDER,
    "0x0",
    "latest",
  ]);
  assert.equal(BigInt(recorded), BigInt(op.sender));
  const revert = await submitAndVerify(
    { ...op, nonce: op.nonce + 2n, to: REVERTER, gasLimit: 50000n },
    "0x0",
  );
  const receipts = [ethReceipt, transfer.receipt, call.receipt, revert.receipt];
  console.log(
    "Verified msg.sender, fees, nonces, duplicate protection, replay rejection, and reverted-call accounting",
  );
  return { receipts, state: await snapshot(receipts) };
}

if (process.argv[1]?.endsWith("demo.ts")) await demo();

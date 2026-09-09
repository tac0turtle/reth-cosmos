import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { getBytes, hexlify } from "ethers";
import { serializeSignDoc } from "@cosmjs/amino";
import { operation, payload, sign, signDoc } from "./protocol.js";

test("independent canonical Cosmos fixture", async () => {
  const fixture = JSON.parse(
    await readFile(
      new URL("../../fixtures/adr036-v1.json", import.meta.url),
      "utf8",
    ),
  ) as Record<string, unknown>;
  const op = operation("0x" + "11".repeat(32));
  const signed = await sign(op);
  for (const [field, value] of Object.entries(signed))
    assert.deepEqual(value, fixture[field]);
  assert.equal(payload(op), fixture.payload);
  assert.equal(hexlify(serializeSignDoc(signDoc(op))), fixture.signBytes);
  assert.equal(getBytes(signed.signature).length, 64);
});

test("protocol bounds and integer encoding", () => {
  const op = operation("0x" + "11".repeat(32));
  for (const changed of [
    { ...op, nonce: -1n },
    { ...op, nonce: (1n << 64n) - 1n },
    { ...op, value: 1n << 256n },
    { ...op, gasLimit: 30000001n },
    { ...op, maxFeePerGas: 1n },
    { ...op, input: "0x" + "00".repeat(32769) },
    { ...op, sender: "0x01" },
  ])
    assert.throws(() => payload(changed));
});

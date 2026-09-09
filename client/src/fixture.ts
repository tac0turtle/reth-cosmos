import { writeFile } from "node:fs/promises";
import { operation, payload, sign } from "./protocol.js";

const op = operation("0x" + "11".repeat(32));
const signed = await sign(op);
const fixture = { operation: op, payload: payload(op), ...signed };
await writeFile(
  new URL("../../fixtures/adr036-v1.json", import.meta.url),
  JSON.stringify(
    fixture,
    (_, v: unknown) => (typeof v === "bigint" ? v.toString() : v),
    2,
  ) + "\n",
);
console.log("Wrote independent CosmJS fixture");

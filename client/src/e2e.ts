import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdir, mkdtemp, writeFile, readFile, cp } from "node:fs/promises";
import { createWriteStream } from "node:fs";
import { resolve } from "node:path";
import { getBytes, toBeHex } from "ethers";

// Configure before loading modules that capture the URL.
process.env.COSMOS_RPC_URL = "http://127.0.0.1:18545";
const { demo, snapshot } = await import("./demo.js");
const { rpc } = await import("./rpc.js");
const root = resolve(import.meta.dirname, "../..");
await mkdir(resolve(root, ".artifacts"), { recursive: true });
const directory = await mkdtemp(resolve(root, ".artifacts/e2e-"));
let node: ChildProcess | undefined;

async function start(datadir: string, name: string) {
  const log = createWriteStream(resolve(directory, `${name}.log`));
  const child = spawn("sh", ["scripts/start.sh"], {
    cwd: root,
    env: {
      ...process.env,
      COSMOS_DATADIR: datadir,
      COSMOS_RPC_PORT: "18545",
      COSMOS_AUTH_PORT: "18551",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout?.pipe(log);
  child.stderr?.pipe(log);
  child.on("exit", () => log.end());
  node = child;
  for (let i = 0; i < 150; i++) {
    if (child.exitCode !== null)
      throw new Error(`node exited; see ${name}.log`);
    try {
      await rpc("eth_chainId");
      return;
    } catch {
      /* bounded startup polling */
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`node startup timeout; see ${name}.log`);
}

async function stop() {
  if (!node || node.exitCode !== null || node.signalCode !== null) {
    node = undefined;
    return;
  }
  const child = node;
  await new Promise<void>((resolveExit, reject) => {
    const timer = setTimeout(
      () => reject(new Error("node did not shut down within 30 seconds")),
      30000,
    );
    child.once("exit", () => {
      clearTimeout(timer);
      resolveExit();
    });
    child.kill("SIGINT");
  });
  node = undefined;
}

async function importBlocks(datadir: string, file: string, name = "import") {
  const args = [
    "--log.file.directory",
    resolve(directory, `${name}-logs`),
    "import",
    "--chain",
    resolve(root, "genesis.json"),
    "--datadir",
    datadir,
    file,
  ];
  const log = createWriteStream(resolve(directory, `${name}.log`));
  const child = spawn(resolve(root, "target/debug/reth-cosmos-dev"), args, {
    cwd: root,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout?.pipe(log);
  child.stderr?.pipe(log);
  await new Promise<void>((resolveExit, reject) => {
    const timer = setTimeout(() => {
      child.kill("SIGINT");
      reject(new Error("import timed out after 60 seconds"));
    }, 60000);
    child.on("error", (error) => {
      clearTimeout(timer);
      log.end();
      reject(error);
    });
    child.on("exit", (code) => {
      clearTimeout(timer);
      log.end();
      if (code === 0) resolveExit();
      else reject(new Error(`import exited ${code}; see ${name}.log`));
    });
  });
}

try {
  const original = resolve(directory, "original");
  await start(original, "first-start");
  const result = await demo();
  await writeFile(
    resolve(directory, "verified-state.json"),
    JSON.stringify(result, null, 2) + "\n",
  );
  const height = Number(BigInt(await rpc<string>("eth_blockNumber")));
  assert(height > 0 && height <= 10, "unexpected demo height");
  const blocks = [];
  for (let i = 1; i <= height; i++) {
    blocks.push(getBytes(await rpc<string>("debug_getRawBlock", [toBeHex(i)])));
  }
  const file = resolve(directory, "blocks.rlp");
  await writeFile(file, Buffer.concat(blocks));
  await stop();
  await start(original, "restart");
  assert.deepEqual(await snapshot(result.receipts), result.state);
  console.log(
    "Restart preserved transactions, receipts, state root, balances, and nonces",
  );
  await stop();
  const replayed = resolve(directory, "reexecuted");
  await importBlocks(replayed, file);
  await start(replayed, "reexecuted-start");
  assert.deepEqual(await snapshot(result.receipts), result.state);
  console.log(
    "Reth block import re-executed the chain and reproduced the state root and complete snapshot",
  );
  await stop();
  const legacy = JSON.parse(
    await readFile(
      resolve(root, "fixtures/pre-migration-chain/verified-state.json"),
      "utf8",
    ),
  ) as typeof result;
  const legacyImport = resolve(directory, "pre-migration-import");
  await importBlocks(
    legacyImport,
    resolve(root, "fixtures/pre-migration-chain/blocks.rlp"),
    "pre-migration-import",
  );
  await start(legacyImport, "pre-migration-start");
  assert.deepEqual(await snapshot(legacy.receipts), legacy.state);
  console.log(
    "Pre-migration blocks reproduced the original state and RPC responses",
  );
  await stop();
  if (process.env.COSMOS_LEGACY_DATADIR) {
    const legacyCopy = resolve(directory, "pre-migration-database");
    await cp(resolve(process.env.COSMOS_LEGACY_DATADIR), legacyCopy, {
      recursive: true,
      errorOnExist: true,
      force: false,
    });
    await start(legacyCopy, "pre-migration-database-start");
    assert.deepEqual(await snapshot(legacy.receipts), legacy.state);
    console.log(
      "Pre-migration database opened with identical transactions, receipts, balances, nonces, and state root",
    );
    await stop();
  }
  await writeFile(
    resolve(directory, "PASS"),
    "All end-to-end assertions passed.\n",
  );
  console.log(`Evidence retained at ${directory}`);
} finally {
  await stop();
}

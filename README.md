# Reth dev chain with Cosmos authorization

This local chain executes EVM transactions authorized by Cosmos secp256k1 accounts using ADR-036 Amino signatures. A Cosmos account pays gas and uses its EVM balance and nonce directly. The transaction stores the public key and original Cosmos signature under experimental EIP-2718 type `0x7e`.

**This changes transaction authorization and block validity. Use this custom node only for the experiment.** It has no Cosmos SDK modules, CometBFT, IBC, multisig, browser wallet integration, or protobuf `SIGN_MODE_DIRECT` support.

## Run

Prerequisites: Rust via rustup, Node.js 22.18+ (tested with 24.12), pnpm 10.30.3, Python 3, Git, Clang and a C/C++ build toolchain. The Rust toolchain is pinned to 1.97.1. Install nightly rustfmt for the format checks. Reth's first build is large; allow several minutes and roughly 15 GB of build storage.

```sh
sh scripts/bootstrap.sh
pnpm install --frozen-lockfile
cargo build --locked
pnpm start
```

In another terminal:

```sh
pnpm demo
```

The demonstration verifies an Ethereum transfer, rejects invalid Cosmos transactions, sends a Cosmos transfer and recorder call, and verifies a reverted call. It prints both account encodings, transaction hashes, receipts, and paid fees. Every claimed state change has an assertion.

Run the complete check suite, including an isolated node, restart, and block re-execution:

```sh
pnpm check
```

Run only the isolated experiment:

```sh
pnpm e2e
```

`e2e` creates a unique directory under `.artifacts/`, starts localhost RPC on ports 18545/18551, shuts the node down, restarts it, and compares transactions, receipts, balances, nonces, storage, and the block state root. It then imports the exported RLP blocks through Reth's sync pipeline into a second fresh database and repeats the comparison. Logs, raw blocks, a verified state snapshot, and a `PASS` marker remain in that directory. Those ports must be free. No existing data directory is deleted.

## Accounts and contracts

The disposable private key is the 32-byte integer **1**, intentionally public in `client/src/protocol.ts`. Never fund it on a real network. Both account derivations are prefunded with `10^24` wei:

| Derivation | Address |
| --- | --- |
| Cosmos account bytes, expressed as EVM hex | `0x751e76e8199196d454941c45d1b3a323f1433bd6` |
| Same account in Bech32 | `cosmos1w508d6qejxtdg4y5r3zarvary0c5xw7k6ah60c` |
| Ethereum derivation of the same public key | `0x7e5f4552091a69125d5dfcb7b8c2659029395bdf` |

The genesis recorder at `0x0000000000000000000000000000000000001000` contains `CALLER; PUSH1 0; SSTORE; STOP` (`3360005500`). Read slot zero with `eth_getStorageAt` to inspect the last caller. The contract at `0x0000000000000000000000000000000000001001` always reverts (`60006000fd`).

## Node and restart behavior

`pnpm start` uses chain ID `366036`, the committed `genesis.json`, on-demand dev block production, localhost HTTP/Engine/P2P listeners, no discovery or NAT resolution, and zero inbound/outbound peers. The default persistent directory is `./data`; logs and the local Engine API JWT remain there too.

Stop with Ctrl-C and run the same command to resume. The demo reads current nonces and can run again against existing data. To start a separate chain, set `COSMOS_DATADIR` to a new directory. Optional `COSMOS_RPC_PORT` and `COSMOS_AUTH_PORT` override the launch ports; set `COSMOS_RPC_URL` for the demo. The client rejects non-localhost URLs.

The launch script bounds each pool subpool to 1,024 transactions and 16 MB, with 16 slots per account and no local-account exemptions. HTTP requests are capped at 1 MB and connections at 32. The Cosmos RPC permits eight concurrent requests and checks capacity before signature recovery. Protocol decoding rejects more than 32 KiB of calldata, gas limits outside 21,000–30,000,000, invalid fee ordering, and nonce `2^64 - 1`.

## Source and validation boundaries

Reth is pinned to [v2.4.1](https://github.com/paradigmxyz/reth/releases/tag/v2.4.1), commit `8eb210175687c9f0c889a3b6795c16781d830e3a`. `scripts/bootstrap.sh` fetches that revision and applies `patches/reth-v2.4.1.patch`. It preserves an existing checkout and fails on unexpected changes. `scripts/verify-source.py` checks the exact source diff.

The executable uses Reth's node builder and RPC extension APIs. Reth's default node fixes its primitives to Alloy's Ethereum envelope, so this experiment also vendors five small dependencies to extend that envelope through storage, EVM conversion, and RPC. [VENDOR.md](VENDOR.md) identifies those changes. Upstream Reth itself has three focused changes: pool type registration, independent consensus authorization checks, and transaction-type metrics.

Authorization is checked during sender recovery, including the unchecked and buffered paths, and again during pre-execution block validation. Block import validates signatures even without pool admission. Execution uses ordinary EIP-1559 fee rules and the recovered Cosmos sender; receipts retain type `0x7e`. Existing Ethereum transaction types retain their original authorization.

See [PROTOCOL.md](PROTOCOL.md) for the exact encoding and RPC representation. `fixtures/adr036-v1.json` is generated independently with CosmJS; Rust tests compare its sign bytes, deterministic signature, account derivation, payload, wire bytes, and transaction hash.

The suite tests malformed input and tampering, domain separation, sender recovery, compact storage and RPC round trips, pool rejection, duplicate/replay behavior, fees and nonces, contract caller identity, reverts, restart, and import re-execution. This is an experimental authorization format, not a public-network protocol or a security audit.

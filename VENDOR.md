# Upstream dependencies and local adapters

Reth is a Cargo Git dependency fetched directly from `https://github.com/paradigmxyz/reth`, selected with `tag = "v2.5.2"`. Cargo stores the checkout in its normal dependency cache and records the resolved commit in `Cargo.lock`. Alloy and `reth-codecs` come from crates.io with checksums in the lockfile. There are no Cargo dependency patches, repository-local upstream checkouts, or vendored dependency crates. `scripts/verify-source.py` checks the manifest pins and lockfile sources. The optional bootstrap script runs this check and `cargo fetch --locked`.

The chain owns its extension types and implementations:

| Component | Location | Upstream behavior reused |
| --- | --- | --- |
| Transaction envelope, Cosmos authorization container, transaction type | `src/primitives/` | Unmodified `EthereumTxEnvelope` for all Ethereum variants |
| Persistent encoding | `src/primitives/storage.rs` | `CompactEnvelope`, `Envelope`, `ToTxCompact`, `FromTxCompact` |
| Pool transaction and node builders | `src/pool.rs`, `src/node.rs` | Ethereum validation, ordering, blob handling, pool maintenance, networking |
| Consensus authorization | `src/consensus.rs` | `EthBeaconConsensus` for Ethereum header/body and post-execution rules |
| EVM and receipts | `src/evm.rs` | Ethereum executor, environment configuration, block assembler, generic `EthereumReceipt` |
| Payload and Engine API adapters | `src/payload/` | Ethereum payload validation and Engine API response containers |
| RPC representations and conversions | `src/rpc.rs` | Generic Ethereum transaction/receipt containers and `EthApi` |

`CosmosSigned` is immutable and does not implement Alloy's generic signable Ethereum transaction interface. Its cached hash and wire encoding cannot diverge through public mutation. Every Cosmos recovery method, including unchecked/buffered methods, runs the ADR-036 verifier. Ethereum variants delegate to upstream implementations. The CLI uses the same custom EVM and consensus components for node startup, block import, and re-execution.

## Adapter provenance and upgrade cost

`src/payload/builder.rs`, `src/payload/built.rs`, and `src/payload/engine.rs` adapt the corresponding Reth v2.4.1 Ethereum payload, engine-primitives, and node engine implementations to the chain's primitives. The Engine API environment methods in `src/evm.rs` also follow the pinned Ethereum EVM implementation. These portions retain upstream execution and validation behavior; they are maintained local adapters, not upstream extension hooks. Their source is licensed under MIT OR Apache-2.0; copies of both licenses are in `licenses/`.

Upgrades still require reviewing these adapters against upstream, plus trait/API changes and the complete compatibility suite. The migration removes modifications to upstream transaction enums and recovery machinery; it does not eliminate the maintenance cost of a custom chain.

The adapters were reviewed against Reth v2.5.2. Its Ethereum payload builder and Engine API adapters are unchanged from v2.4.1. The local EVM continues to verify senders directly without enabling the new optional sender-recovery cache. The RPC adapter supplies the newly required log response type. Direct dependency versions follow this release: Alloy 2.3.0, alloy-evm 0.38.0, revm 42.0.1, and Reth codecs/primitives-traits 0.6.0.

## Compatibility evidence

`fixtures/pre-migration-storage.json` was generated with the vendored dependencies from commit `aa3fcdb`. It covers calldata lengths 0, 64, and 32,768 (uncompressed and compressed storage), plus a custom receipt. Rust tests require identical storage and wire bytes and verify sender recovery.

`fixtures/pre-migration-chain/` contains the original four-block end-to-end chain export and its verified RPC/state snapshot. Every end-to-end run imports those original blocks and compares transactions, receipts, balances, nonces, contract storage, and state root. The optional `COSMOS_LEGACY_DATADIR` check copies the database that produced this fixture into the run's artifact directory and opens it using the migrated node; the source database remains untouched.

The protocol version, genesis, transaction hashes, compact database encoding, and RPC representation are unchanged. Switching from local paths to Git dependencies does not require a chain reset. Existing local checkouts are unused and left untouched.

The dev profile optimizes the executable crate at level 1 to reduce the size of Reth's generic node machinery. Cargo may report the upstream `proc-macro-error2 2.0.1` future-compatibility notice; it is not a failed project check.

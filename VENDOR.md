# Pinned source changes

`Cargo.lock` and `pnpm-lock.yaml` fix dependency resolution. The Rust toolchain is pinned separately. The root Cargo manifest patches these crates to their committed local source:

| Crate | Upstream version | Experiment changes |
| --- | --- | --- |
| alloy-consensus | 2.1.1 | `TxCosmos`, type `0x7e`, canonical encoding, Cosmos signature recovery, explicit RPC serialization, receipt envelope support |
| alloy-evm | 0.37.1 | Convert authorized operations to EIP-1559 execution inputs; preserve the outer receipt type |
| alloy-rpc-types-eth | 2.1.1 | Represent custom transactions/receipts; reject constructing Cosmos authorization through Ethereum transaction requests |
| alloy-network | 2.1.1 | Carry the custom typed envelope; reject unsigned Ethereum-style Cosmos requests |
| reth-codecs | 0.5.2 | Store and recover the custom type, operation, public key, and signature through the extended compact type identifier |

The copied source retains upstream license files, manifests, and VCS metadata where distributed. The shared authorization implementation lives in `crates/cosmos-auth`; there is no custom elliptic-curve implementation.

Reth v2.4.1 is fetched separately to avoid committing a second full repository. Its exact commit and the three-file patch are verified by `scripts/verify-source.py`. Reapply source changes through the tracked patch when reproducing the checkout.

These patches are deliberately tied to these versions. Upgrading Reth or Alloy requires reviewing every exhaustive transaction/receipt match, encoding path, recovery method, storage codec, and block validation path, followed by the complete check suite. An unmodified node cannot validate or import the custom chain.

The dev profile optimizes the executable crate at level 1 to reduce the size of Reth’s generic node machinery. Dependency profiles remain unchanged. Cargo may report the upstream `proc-macro-error2 2.0.1` future-compatibility notice; it is not a failed project check.

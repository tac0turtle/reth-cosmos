#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
python3 scripts/verify-source.py
cargo +nightly fmt -p reth-cosmos-dev -p cosmos-auth --check
python3 scripts/check-vendor-format.py
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --locked
cargo test --workspace --locked
pnpm --dir client format:check
pnpm --dir client lint
pnpm --dir client typecheck
pnpm --dir client build
pnpm --dir client test
pnpm --dir client e2e

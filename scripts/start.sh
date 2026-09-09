#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
exec ./target/debug/reth-cosmos-dev --log.file.directory "${COSMOS_DATADIR:-./data}/logs" node \
  --dev --chain ./genesis.json --datadir "${COSMOS_DATADIR:-./data}" \
  --http --http.addr 127.0.0.1 --http.port "${COSMOS_RPC_PORT:-8545}" \
  --http.api eth,net,web3,debug --authrpc.addr 127.0.0.1 \
  --authrpc.port "${COSMOS_AUTH_PORT:-8551}" \
  --addr 127.0.0.1 --port 0 --nat none --disable-discovery --max-outbound-peers 0 --max-inbound-peers 0 \
  --rpc.max-request-size 1 --rpc.max-connections 32 \
  --txpool.pending-max-count 1024 --txpool.pending-max-size 16 \
  --txpool.queued-max-count 1024 --txpool.queued-max-size 16 \
  --txpool.basefee-max-count 1024 --txpool.basefee-max-size 16 \
  --txpool.max-account-slots 16 --txpool.nolocals

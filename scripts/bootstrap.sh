#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
python3 scripts/verify-source.py
cargo fetch --locked

#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
revision=8eb210175687c9f0c889a3b6795c16781d830e3a
if [ ! -e vendor/reth ]; then
  git clone --depth 1 --branch v2.4.1 https://github.com/paradigmxyz/reth.git vendor/reth
fi
actual=$(git -C vendor/reth rev-parse HEAD)
if [ "$actual" != "$revision" ]; then
  echo "vendor/reth has revision $actual; expected $revision. Existing files were preserved." >&2
  exit 1
fi
patch=$(pwd)/patches/reth-v2.4.1.patch
if git -C vendor/reth apply --reverse --check "$patch" 2>/dev/null; then
  echo "Reth experiment patch already applied."
else
  git -C vendor/reth apply --check "$patch"
  git -C vendor/reth apply "$patch"
fi
python3 scripts/verify-source.py

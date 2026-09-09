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
# Remove only the exact patch shipped by the first implementation.
# Unexpected user changes are preserved and rejected by verify-source.py.
python3 - <<'PYTHON'
import hashlib
import subprocess
patch = subprocess.check_output(["git", "-C", "vendor/reth", "diff", "--binary", "HEAD"])
if hashlib.sha256(patch).hexdigest() == "8ca2f82e8255f7bd196d835576c5ebc2961a5c097786fa574f9160d2c689b035":
    subprocess.run(["git", "-C", "vendor/reth", "apply", "--reverse", "--check", "-"], input=patch, check=True)
    subprocess.run(["git", "-C", "vendor/reth", "apply", "--reverse", "-"], input=patch, check=True)
    print("Removed the recognized legacy Reth experiment patch.")
PYTHON
python3 scripts/verify-source.py

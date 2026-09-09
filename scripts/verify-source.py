"""Fail if the Reth checkout differs from the recorded revision and patch."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent.parent
revision = subprocess.check_output(
    ["git", "-C", str(root / "vendor/reth"), "rev-parse", "HEAD"], text=True
).strip()
if revision != "8eb210175687c9f0c889a3b6795c16781d830e3a":
    raise SystemExit("Reth revision mismatch; run scripts/bootstrap.sh")
actual = subprocess.check_output(
    ["git", "-C", str(root / "vendor/reth"), "diff", "--binary", "HEAD"]
)
if actual != (root / "patches/reth-v2.4.1.patch").read_bytes():
    raise SystemExit("Reth source differs from patches/reth-v2.4.1.patch")
print("Reth v2.4.1 revision and experiment patch verified")

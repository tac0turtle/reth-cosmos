"""Require the exact, unmodified Reth release and registry Alloy dependencies."""
from pathlib import Path
import subprocess
import tomllib

root = Path(__file__).resolve().parent.parent
revision = subprocess.check_output(
    ["git", "-C", str(root / "vendor/reth"), "rev-parse", "HEAD"], text=True
).strip()
if revision != "8eb210175687c9f0c889a3b6795c16781d830e3a":
    raise SystemExit("Reth revision mismatch; run scripts/bootstrap.sh")
actual = subprocess.check_output(
    ["git", "-C", str(root / "vendor/reth"), "diff", "--binary", "HEAD"]
)
if actual:
    raise SystemExit("Reth source is modified; run scripts/bootstrap.sh")
manifest = tomllib.loads((root / "Cargo.toml").read_text())
if manifest.get("patch") or manifest.get("replace"):
    raise SystemExit("Upstream dependencies must not be patched or replaced")
lock = tomllib.loads((root / "Cargo.lock").read_text())
for package in lock["package"]:
    if package["name"].startswith("alloy-") or package["name"] == "reth-codecs":
        if not package.get("source", "").startswith("registry+"):
            raise SystemExit(f"{package['name']} must come from the registry")
print("Unmodified Reth v2.4.1 and registry Alloy/codecs verified")

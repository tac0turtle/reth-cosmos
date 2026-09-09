"""Require the Reth release tag from GitHub and registry Alloy/codecs."""
from pathlib import Path
import re
import tomllib

root = Path(__file__).resolve().parent.parent
tag = "v2.5.2"
repository = "https://github.com/paradigmxyz/reth"
source_prefix = f"git+{repository}?tag={tag}#"
manifest = tomllib.loads((root / "Cargo.toml").read_text())
if manifest.get("patch") or manifest.get("replace"):
    raise SystemExit("Upstream dependencies must not be patched or replaced")
for name, dependency in manifest["dependencies"].items():
    if name.startswith("reth-") and name not in {"reth-codecs", "reth-primitives-traits"}:
        if not isinstance(dependency, dict) or (
            dependency.get("git") != repository
            or dependency.get("tag") != tag
            or any(key in dependency for key in ("path", "branch", "rev"))
        ):
            raise SystemExit(f"{name} must use the Reth GitHub tag {tag}")
lock = tomllib.loads((root / "Cargo.lock").read_text())
resolved_sources = set()
for package in lock["package"]:
    if package["name"].startswith("reth-") and package["version"] == "2.5.2":
        source = package.get("source", "")
        if not re.fullmatch(re.escape(source_prefix) + r"[0-9a-f]{40}", source):
            raise SystemExit(f"{package['name']} must resolve from the Reth GitHub tag {tag}")
        resolved_sources.add(source)
    if package["name"].startswith("alloy-") or package["name"] == "reth-codecs":
        if not package.get("source", "").startswith("registry+"):
            raise SystemExit(f"{package['name']} must come from the registry")
if len(resolved_sources) != 1:
    raise SystemExit("Reth packages must resolve to a single release commit in Cargo.lock")
print("Reth GitHub tag v2.5.2 and registry Alloy/codecs verified")

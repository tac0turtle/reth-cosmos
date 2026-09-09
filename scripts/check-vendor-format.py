"""Format-check only vendor Rust files modified for this experiment."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent.parent
files = (root / "patches/formatted-files.txt").read_text().splitlines()
for file in files:
    subprocess.run(["rustfmt", "+nightly", "--edition", "2024", "--config", "skip_children=true", "--check", str(root / file)], check=True)

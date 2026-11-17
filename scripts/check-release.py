"""Reject release tags that do not match the crate version."""

import os
import re
import tomllib
from pathlib import Path

tag = os.environ.get("GITHUB_REF_NAME", "")
version = tomllib.loads(Path("Cargo.toml").read_text())["package"]["version"]
if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", tag):
    raise SystemExit("Run releases from a vMAJOR.MINOR.PATCH tag.")
if tag != f"v{version}":
    raise SystemExit(f"Tag {tag} does not match Cargo.toml version {version}.")
print(f"Validated {tag}")

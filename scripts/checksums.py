#!/usr/bin/env python3
"""Write deterministic SHA-256 checksums for release artifacts."""

from pathlib import Path
import hashlib
import sys

if len(sys.argv) < 3:
    raise SystemExit(f"usage: {sys.argv[0]} OUTPUT ARTIFACT [ARTIFACT ...]")

output = Path(sys.argv[1])
artifacts = sorted((Path(value) for value in sys.argv[2:]), key=lambda path: path.name)
lines = []
for artifact in artifacts:
    if not artifact.is_file():
        raise SystemExit(f"artifact not found: {artifact}")
    with artifact.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    lines.append(f"{digest}  {artifact.name}")
output.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(output)

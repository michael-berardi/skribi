#!/usr/bin/env python3
"""Build the public-domain Meditations.skribi example deterministically."""

from pathlib import Path
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "examples" / "meditations"
OUTPUT = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "dist" / "Meditations.skribi"
FILES = [SOURCE / "library.json", SOURCE / "content" / "meditations.md"]

OUTPUT.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(OUTPUT, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for path in FILES:
        info = zipfile.ZipInfo(path.relative_to(SOURCE).as_posix(), (1980, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = 0o100644 << 16
        archive.writestr(info, path.read_bytes())

print(OUTPUT)

#!/usr/bin/env python3
"""Package a previously built Skribi binary for Windows or Linux."""

from pathlib import Path
import argparse
import gzip
import io
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PUBLIC_FILES = [ROOT / "README.md", ROOT / "LICENSE", ROOT / "PRIVACY.md", ROOT / "SECURITY.md", ROOT / "THIRD_PARTY_NOTICES.md"]
FONT_LICENSES = sorted((ROOT / "assets" / "fonts").glob("OFL-*.txt"))
TIMESTAMP = (1980, 1, 1, 0, 0, 0)

parser = argparse.ArgumentParser()
parser.add_argument("platform", choices=["windows-x86_64", "linux-x86_64"])
parser.add_argument("version")
parser.add_argument("binary", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()

if not args.binary.is_file():
    raise SystemExit(f"binary not found: {args.binary}")
args.output.mkdir(parents=True, exist_ok=True)

if args.platform == "windows-x86_64":
    destination = args.output / f"Skribi-{args.version}-windows-x86_64.zip"
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        entries = [
            (args.binary, "skribi.exe"),
            *((path, path.name) for path in PUBLIC_FILES),
            *((path, f"licenses/{path.name}") for path in FONT_LICENSES),
        ]
        for source, name in entries:
            info = zipfile.ZipInfo(name, TIMESTAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, source.read_bytes())
else:
    destination = args.output / f"Skribi-{args.version}-linux-x86_64.tar.gz"
    payload = io.BytesIO()
    with tarfile.open(fileobj=payload, mode="w", format=tarfile.PAX_FORMAT) as archive:
        entries = [
            (args.binary, "skribi"),
            *((path, path.name) for path in PUBLIC_FILES),
            *((path, f"licenses/{path.name}") for path in FONT_LICENSES),
        ]
        for source, name in entries:
            data = source.read_bytes()
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o755 if name == "skribi" else 0o644
            info.mtime = 0
            archive.addfile(info, io.BytesIO(data))
    with destination.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9) as compressed:
            compressed.write(payload.getvalue())

print(destination)

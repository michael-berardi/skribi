# Contributing to Skribi

Skribi favors small, direct changes that keep Markdown portable and the application understandable.

## Before opening a change

1. Search existing issues and open a focused issue for material behavior changes.
2. Keep the implementation local-first. Do not add accounts, telemetry, remote services, or proprietary storage.
3. Preserve unknown YAML front-matter fields and ordinary Markdown readability.
4. Avoid new dependencies when the Rust standard library or an existing dependency is sufficient.
5. Do not add GitHub Actions; project checks run locally.

## Verify a change

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

For interface changes, also run the native application against `examples/meditations/content` and inspect the Library, Mouseia catalog, Reader, Editor, and Retex views at common window sizes.

## Linux release cross-build

With the default Colima profile running on Apple Silicon, run
`scripts/build-linux-cross.sh`. It uses a native ARM64 Rust 1.95 Debian 11
container and the AMD64 cross toolchain/native libraries, with at most four
build jobs; the container is removed on exit. Bullseye security packages now
come from Debian's archive (whose expired metadata is allowed, but signatures
are still verified). The script prints the binary's highest required GLIBC
symbol version; update the README download table to match before packaging
with `scripts/package-release.py linux-x86_64 VERSION BINARY OUTPUT`.

## Pull requests

Describe the user-visible problem, the chosen fix, affected file formats, and exact verification performed. Include native-resolution screenshots for visual changes. Never commit credentials, personal paths, generated release artifacts, or private operational documentation.

By contributing, you agree that your contribution is licensed under this repository’s MIT License.

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

## Pull requests

Describe the user-visible problem, the chosen fix, affected file formats, and exact verification performed. Include native-resolution screenshots for visual changes. Never commit credentials, personal paths, generated release artifacts, or private operational documentation.

By contributing, you agree that your contribution is licensed under this repository’s MIT License.

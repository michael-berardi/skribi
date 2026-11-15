# Skribi

<p align="center">
  <a href="https://github.com/michael-berardi/skribi/releases/latest"><img src="https://img.shields.io/github/v/release/michael-berardi/skribi?label=release" alt="Latest release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/michael-berardi/skribi" alt="MIT License" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="macOS, Windows and Linux" />
  <img src="https://img.shields.io/badge/built%20with-Rust-orange" alt="Built with Rust" />
</p>

Skribi is a desktop reader and editor for Markdown libraries and Retex vaults.
Read long works in a continuous, chapter-aware view while keeping notes as plain
files. Open an existing Markdown or Obsidian folder, or import a Notion export.

<p align="center">
  <img src="tests/snapshots/reader_dark.png" alt="Skribi's continuous reader showing Book I of Marcus Aurelius's Meditations on a true-black background" width="820" />
</p>

Skribi is free and open-source software by [Pareto Cybernetics](https://paretocybernetics.com/software/).

## Install

[Download Skribi](https://github.com/michael-berardi/skribi/releases/latest)
and [SHA256SUMS](https://github.com/michael-berardi/skribi/releases/latest/download/SHA256SUMS) for macOS, Windows, or Linux; no source build is needed.
Verify the download against the checksum before opening it.

| System | Download |
| --- | --- |
| macOS 13+ (Apple Silicon and Intel) | [Skribi-0.1.4-macos-universal.dmg](https://github.com/michael-berardi/skribi/releases/download/v0.1.4/Skribi-0.1.4-macos-universal.dmg) |
| Windows x86_64 | [Skribi-0.1.4-windows-x86_64.zip](https://github.com/michael-berardi/skribi/releases/download/v0.1.4/Skribi-0.1.4-windows-x86_64.zip) |
| Linux x86_64 (glibc 2.35+) | [Skribi-0.1.4-linux-x86_64.tar.gz](https://github.com/michael-berardi/skribi/releases/download/v0.1.4/Skribi-0.1.4-linux-x86_64.tar.gz) |

On macOS, open the disk image and copy Skribi to Applications. On Windows,
extract the archive and run `skribi.exe`. On Linux, extract the archive and run
`./skribi` in a graphical desktop session. The macOS packaging
script signs and notarizes the app and disk image.

## First use

1. Select **Open library folder** and choose a folder containing Markdown files.
2. Select a document, then use **Read / Edit**. The reader offers chapter navigation,
   edition selection, adjustable text size, and six typefaces.
3. Use **Retex → Open Retex vault** for a separate notes workspace.
4. Use **Export library** to save a portable `.skribi` package.

For an example, download `Meditations.skribi` from the release and choose
**Import → Skribi library**. The same reading library is in
`examples/meditations/content`.

## Imports and storage

- Open Obsidian vaults in place, or use **Import → Obsidian or Markdown folder**
  to copy their Markdown and attachments. Dot-prefixed files and folders,
  including `.obsidian` settings, are omitted from folder imports.
  Wikilink text is preserved; this is not a promise of Obsidian plugin support.
- **Import → Notion ZIP** accepts Markdown & CSV exports. It removes export IDs
  from names, rewrites matching names in text, preserves assets, and creates
  Markdown tables alongside CSV files.
- Imports require a new or empty destination. ZIP imports reject symlink entries
  and unsafe paths, with limits of 100,000 entries, 64 MiB per entry, and 1 GiB
  expanded size. An interrupted import can leave a partially populated destination.
- A `.skribi` package is a ZIP containing `library.json` (format version 1) and
  `content/` with Markdown and assets. Extract `content/` to recover plain files.
  Exports omit dot-prefixed paths and `target` directories.
- Edits preserve unknown front-matter fields and use atomic writes, stale-write
  checks, and Retex-compatible `.retex/history.jsonl` entries.

Skribi reads Markdown; it does not render EPUB or PDF. Its light and dark palettes
follow the system theme. The Mouseia catalog offers works and their available
translations, with locally hidden recommendations.

## Privacy

No account, analytics, advertising, document upload, or cloud sync. Opening a
Library fetches the public catalog from [mouseia.org](https://mouseia.org/);
adding a work fetches its text. External Markdown links may open a browser.
Settings remember workspace paths and hidden recommendations; ZIP imports use
temporary storage. See [PRIVACY.md](PRIVACY.md) and [SECURITY.md](SECURITY.md).

## Build from source

Requires Rust 1.95+ and the native window-system packages needed by `eframe`.

```sh
git clone https://github.com/michael-berardi/skribi.git
cd skribi
cargo build --release -j4
```

Run `target/release/skribi` (`target/release/skribi.exe` on Windows). An optional
first argument selects the library folder. Without it, `SKRIBI_LIBRARY` takes
precedence over the saved library path. `SKRIBI_RETEX` selects a separate vault.

## Development

```sh
cargo fmt --check
cargo test -j4
cargo clippy -j4 --all-targets -- -D warnings
cargo build --release -j4
```

The headless GPU test `headless_visual_matrix` checks reader, editor, library,
Retex, and welcome states at 1280 × 800 using the bundled reading corpus.
Run it alone with `cargo test -j4 headless_visual_matrix -- --nocapture`.
After intentional visual changes, prefix that command with
`UPDATE_SNAPSHOTS=true` and inspect every generated image before accepting it.

See [CONTRIBUTING.md](CONTRIBUTING.md). Licensed under [MIT](LICENSE).

---

Part of [Pareto Cybernetics](https://paretocybernetics.com/): free, private, open-source tools for working with AI agents.

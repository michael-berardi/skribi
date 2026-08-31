# Skribi

**A calm, local-first home for Markdown libraries and notes.** Skribi reads the files you already own, gives long-form documents a focused reader, keeps editing simple, and adds a practical Kanban board without turning a folder into a proprietary database.

Skribi is free and open-source software by [Implose Cybernetics](https://implosecybernetics.com/).

## What ships in 0.1.0

- **Ordinary Markdown:** open any folder directly; files remain readable in every text editor.
- **Retex compatibility:** YAML front matter, `.retex/config.json` board columns, atomic edits, stale-write protection, and `.retex/history.jsonl` undo entries use Retex-compatible contracts. Retex is not required.
- **Obsidian support:** open a vault in place or import a clean copy while preserving Markdown, folders, attachments, and wikilinks. `.obsidian` settings are not copied.
- **Notion support:** import a Notion Markdown & CSV ZIP export; remove export IDs from names, repair affected links, preserve assets, and convert CSV databases to Markdown tables.
- **Reading mode:** distraction-free CommonMark rendering, adjustable text size, translation selection, and chapter navigation for MOUSEIA-style complete books.
- **Adaptive appearance:** the light palette and true-black OLED-friendly dark palette follow the operating-system theme.
- **Library packages:** export and import `.skribi` files. They are safe, bounded ZIP archives containing `library.json`, ordinary Markdown, and assets.
- **Kanban:** read Retex columns and persist status changes back to YAML front matter.
- **Local by default:** no account, server, sync service, telemetry, or runtime network dependency.

Skribi 0.1.0 supports macOS, Windows, and Linux. It reads Markdown and Notion CSV tables; it does not render EPUB or PDF files.

## Install

Download the archive for your platform and the optional `Meditations.skribi` example from the [latest release](https://github.com/michael-berardi/skribi/releases/latest). Verify it against `SHA256SUMS` before opening it.

### Build from source

Requirements: Rust 1.95 or newer and the native window-system packages required by `eframe` on your platform.

```sh
git clone https://github.com/michael-berardi/skribi.git
cd skribi
cargo run --release
```

Open a specific Markdown, Retex, MOUSEIA, or Obsidian folder at launch:

```sh
cargo run --release -- /path/to/library
```

## Start quickly

1. Select **Open folder** to use a Markdown, Retex, MOUSEIA, or Obsidian folder in place.
2. Select **Import → Notion ZIP** for a Notion `Markdown & CSV` export.
3. Select **Import → Skribi library** for a `.skribi` package.
4. Use **Read / Edit** for the selected document and **Board** for front-matter workflow states.
5. Select **Export** to make a portable `.skribi` package without changing the source folder.

Skribi refuses import symlinks, path traversal, more than 100,000 entries, files above 64 MiB, and archives expanding beyond 1 GiB. Import destinations must be new or empty, so an import cannot silently overwrite existing work.

## Portable library format

A `.skribi` file is a ZIP-compatible archive:

```text
library.json
content/
  document.md
  images/
    cover.png
```

`library.json` identifies format version 1 and lists Markdown documents. Extracting `content/` produces a normal folder; no Skribi runtime is needed to recover or edit it.

The repository includes `examples/meditations`, a public-domain reading library with Marcus Aurelius in the original Greek plus the C. R. Haines (1916) and George Long (1862) translations. The release publishes the same content as `Meditations.skribi`.

## Data and privacy

Skribi reads and writes only folders you choose. It has no analytics, advertising, account system, cloud sync, or background network service. Opening a Markdown link may hand that URL to the operating system. See [PRIVACY.md](PRIVACY.md) and [SECURITY.md](SECURITY.md).

## Development

The deterministic headless UI harness renders the real `eframe` app at 1280 × 800 against the complete 774 KiB *Meditations* corpus: original Greek plus both translations, all twelve books. It exercises the welcome, reader, Book XII, full-text editor, and Kanban states in both palettes, checks that primary content intersects the viewport, and compares GPU snapshots.

```sh
cargo test headless_visual_matrix -- --nocapture
```

After an intentional visual change, inspect every generated image before accepting new baselines:

```sh
UPDATE_SNAPSHOTS=true cargo test headless_visual_matrix -- --nocapture
```

Run the complete local gate before release:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

GitHub Actions are intentionally not used. Release artifacts are built, signed where the platform supports signing, checked locally, and uploaded with immutable SHA-256 checksums.

Contributions are welcome under [CONTRIBUTING.md](CONTRIBUTING.md). Skribi is licensed under the [MIT License](LICENSE).

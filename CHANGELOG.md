# Changelog

## 0.2.0

- Skribi tells you when a newer release is out: a banner with the release notes, **Install Update**, **Remind Me Later**, **Skip This Version**, and an opt-in checkbox for automatic installs.
- New Settings window (More → Settings…, or Cmd/Ctrl+,). Under Updates: automatic checks (on), automatic installs (off), Check Now, and the version.
- The check is one anonymous request a day to GitHub's public release record. It stays off in CI, under coding agents, and with `SKRIBI_NO_UPDATE_CHECK=1`.
- Updates are checked against `SHA256SUMS` before anything is replaced. On macOS the new app must also carry the same Developer ID Team ID and pass Gatekeeper. Skribi reopens only after the open note is saved, and keeps one `.previous` copy until the new version starts.

## 0.1.6

- Leaving the editor saves your changes, so Read always shows what you wrote.
- A save that fails now says why in the editor bar and keeps the note open, instead of switching to another note.
- Opening or importing another folder saves the open note first. Reload from disk is labelled "Discard edits and reload" when there are unsaved edits.
- One unreadable subfolder no longer stops a library from opening; skipped files are named.
- A note whose text starts with its own title shows the title once.
- Search fields have a visible outline. Empty libraries, empty vaults and searches with no matches say so.
- Messages in the top bar show their text instead of "Saved" or "Issue".
- Shortcuts: Cmd+O open a folder, Cmd+F search, Cmd+N new note, Cmd+E switch between Read and Edit.
- Long titles in the reader bar are shortened instead of wrapping.

## 0.1.5

- Security: updated the archive, Markdown and TLS libraries. No behaviour change.

## 0.1.4

- Fix a crash when importing Notion files with long non-ASCII names.
- Clarify import exclusions, storage behavior, and installation instructions.
- macOS universal, Windows x86_64, and Linux x86_64 (glibc 2.35+) downloads are available.

To upgrade, replace the installed application with the new download.

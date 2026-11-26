# Updates

<!-- update-contract: v1 -->

Skribi tells you when a newer stable release exists and installs it in one step. It needs no account and sends no tracking data.

## Checking

With **Check for updates automatically** on (the default), Skribi asks GitHub's public "latest release" record for this repository at most once every 24 hours. The check runs in the background with a 4-second limit. It is one anonymous `GET` with a fixed `skribi-update-check` user agent and carries no version, identifier, machine, document, or usage data. Drafts, prereleases, and anything that is not a newer `x.y.z` release are ignored. A failed check is recorded with its cause and tried again the next day; it never interrupts you. **Check Now** checks immediately and reports the result, including why a check failed.

Automatic checks stay off when `CI` is set, under coding agents, and when `SKRIBI_NO_UPDATE_CHECK=1` is set. `SKRIBI_UPDATE_API` points the check at a local test feed.

## The notice

A banner below the toolbar says "Skribi 0.3.0 is available — you have 0.2.0." and shows the release notes. It offers **Install Update**, **Remind Me Later**, and **Skip This Version**, plus the checkbox **Automatically download and install updates in the future** (off by default). Remind Me Later hides the banner until the next daily check. A skipped version stays hidden until a newer one is released. Check Now always shows what it finds.

**Settings → Updates** (More → Settings…, or Cmd/Ctrl+,) has **Check for updates automatically** (on), **Install updates automatically** (off), the current version, and **Check Now**.

## Installing

**Install Update** saves the open note, downloads the package for your system, and checks it against the release's `SHA256SUMS` before anything is replaced. Once no Mouseia download is running, Skribi closes and reopens on the new version. If the note cannot be saved, Skribi keeps running and asks you to save and reopen.

- **macOS:** Skribi mounts the disk image read-only and copies the app next to `Skribi.app` as `Skribi.app.new`. The copy must pass `codesign --verify`, carry the same Developer ID Team ID as the running app, and be accepted by Gatekeeper. Only then is `Skribi.app` renamed to `Skribi.app.previous` and the new copy moved into place.
- **Windows and Linux:** Skribi takes only `skribi.exe` or `skribi` from the archive and swaps it in, keeping `skribi.exe.previous` or `skribi.previous`. Other files in the folder are never touched.

If any step fails, the current version stays in place and keeps working, and the notice returns with the cause. Skribi does not overwrite a copy that runs from a temporary (translocated) location, sits in a read-only folder, is unsigned, or belongs to a package manager (Homebrew, Cargo, system packages). It names the command to use or opens the release page instead.

**Install updates automatically** (opt-in) installs only when nothing is unsaved and no download is running. It never restarts Skribi: the new version starts the next time you open it. A failed automatic install is logged and falls back to the notice.

To roll back before the new version has started, quit Skribi, delete the new copy, and rename the `.previous` copy back.

## No debris

- The check writes one small `update-check.json` (check time, latest version, notice time, failure cause) to Skribi's cache folder: `~/Library/Caches/skribi` on macOS, `~/.cache/skribi` on Linux, `%LOCALAPPDATA%\skribi` on Windows. Update choices live in Skribi's normal settings.
- Downloads go to a temporary folder that is deleted after success or failure.
- When the new version starts, it deletes the `.previous` copy and any leftover `.new` copy, so there is never more than one rollback copy.
- Automatic installs log to one `updates.log` in the cache folder, capped at 256 KB with the oldest lines dropped.
- Turning off **Check for updates automatically** deletes `update-check.json` and `updates.log`. To uninstall, delete the app, the cache folder above, and Skribi's settings in the operating system's application-data folder.

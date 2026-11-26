# Privacy

Skribi runs locally. It has no account, telemetry, analytics, crash reporting,
advertising, cloud sync, or background network service. Two features reach the
network: an anonymous daily update check and the Mouseia catalog you choose to
browse.

## What leaves the computer

Nothing during ordinary reading, writing, importing, exporting, or note-taking.
Fonts are bundled.

**Update check.** With **Check for updates automatically** on (the default),
Skribi sends one anonymous request at most once a day to GitHub's public
latest-release record for this repository. It carries a fixed
`skribi-update-check` user agent and no version, identifier, machine, document,
or usage data. GitHub sees the IP address and request data that any public web
request reveals. Automatic checks stay off in CI, under coding agents, with
`SKRIBI_NO_UPDATE_CHECK=1`, and when you turn off **Settings → Updates → Check
for updates automatically**. Installing an update downloads the package and its
`SHA256SUMS` from the same public release. See [docs/updates.md](docs/updates.md).

**Mouseia.** The catalog is off on every launch. If you choose **Browse Mouseia
catalog**, Skribi makes a public HTTPS request to
`https://mouseia.org/api/works`. If you choose **Add** on a work, it requests
`https://mouseia.org/api/works/<slug>` and writes that text into your library.
These requests carry no account, document contents, file paths, or analytics
identifiers. Mouseia sees the IP address and request data that any public web
request reveals.

**Links.** Clicking **mouseia.org** or a link in a Markdown document hands the
URL to your operating system and default browser; that destination's privacy
policy applies.

## What is stored locally

- Saved workspace paths, hidden Mouseia recommendations, update choices, and a
  skipped version in Skribi's eframe/egui application-data folder. On Unix,
  Skribi sets this folder and its files to 0700/0600.
- When you edit a note, Retex-compatible `.retex/history.jsonl` inside the
  library stores previous note text, capped at 50 versions per file. On Unix,
  `.retex` is 0700 and the history and lock files are 0600.
- Packages you export and files you import, in folders you choose. Files Skribi
  writes are 0600 on Unix. ZIP imports unpack into a temporary folder under the
  operating system's temp directory (0700 on Unix) and delete it after import.
- One small `update-check.json` in Skribi's cache folder
  (`~/Library/Caches/skribi`, `$XDG_CACHE_HOME/skribi` or `~/.cache/skribi`, or
  `%LOCALAPPDATA%\skribi`) holds the last check time, latest version, notice
  time, and any failure cause. Automatic installs write one `updates.log` there,
  capped at 256 KB. Both files are 0600 on Unix. Turning automatic checks off
  deletes both files.
- Update downloads go to a temporary folder that is deleted after success or
  failure. One `.previous` rollback copy is kept beside the app until the new
  version starts.
- Skribi does not write transcripts, tokens, or credentials.

## Deleting local data

Delete `.retex` inside a library to remove its edit history. Delete exported
packages and imported files you no longer want. Delete Skribi's application-data
folder through your operating system to forget saved workspace paths, hidden
recommendations, and update choices. Delete the cache folder above to remove the
update record and log. There is no remote account or server-side data to delete.

## Third parties

GitHub supplies the public release record and download used by updates. Mouseia
is the only content service Skribi contacts, and only after you choose to browse
or add a work. Links you open are handled by your browser and follow the
destination's policy. No third-party fonts, scripts, CDNs, model providers, or
error-reporting services are bundled or contacted by the app.

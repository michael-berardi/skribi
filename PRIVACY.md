# Privacy

Skribi is local-first software.

- No account or sign-in.
- No analytics, telemetry, advertising, or tracking.
- No background network service.
- No document upload or cloud sync.
- No collection by Pareto Cybernetics through the application.

Skribi opens document folders selected through the file picker, launch argument, environment variables, or saved workspace paths. Imports write into the selected destination; ZIP imports also extract into temporary storage. It stores the last opened Library and Retex paths plus hidden Mouseia recommendation IDs in the operating system’s normal application-data location. Retex-compatible edits may create `.retex/history.jsonl` and lock files inside the selected folder.

Opening a Library requests the public work index from `https://mouseia.org/api/works` for the catalog UI. Choosing **Add** requests that work and writes it into the selected Library. Skribi sends no document contents, account identity, analytics identifier, or telemetry to Mouseia. Opening an external Markdown link may pass the URL to your operating system and default browser. Resulting requests are governed by the destination’s privacy policy.

**Updates:** with **Check for updates automatically** on (the default), Skribi sends one anonymous request at most once a day to GitHub's public latest-release record for this repository. It carries a fixed `skribi-update-check` user agent and no version, identifier, machine, document, or usage data. Automatic checks stay off in CI, under coding agents, and with `SKRIBI_NO_UPDATE_CHECK=1`. Installing an update downloads the package and its `SHA256SUMS` from the same public release. Update choices and a skipped version stay in Skribi's settings. One small `update-check.json` in the cache folder (`~/Library/Caches/skribi`, `~/.cache/skribi`, or `%LOCALAPPDATA%\skribi`) holds the last check time, latest version, notice time, and any failure cause. Automatic installs write one local `updates.log` there, capped at 256 KB. Turning automatic checks off deletes both files. See [docs/updates.md](docs/updates.md).

Because Skribi does not operate a service, there is no remote account or server-side personal data to delete. Delete the application’s local settings through the operating system and remove any files you no longer want to retain.

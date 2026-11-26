# Privacy

Skribi runs locally. It has no account, telemetry, analytics, crash reporting,
advertising, cloud sync, automatic update check, or background network service.

## What leaves the computer

Nothing during ordinary reading, writing, importing, exporting, or note-taking.
Skribi does not check for updates and bundles its fonts.

The optional Mouseia catalog is off on every launch. If you choose **Browse
Mouseia catalog**, Skribi makes a public HTTPS request to
`https://mouseia.org/api/works`. If you choose **Add** on a work, it requests
`https://mouseia.org/api/works/<slug>` and writes that text into your library.
These requests carry no account, document contents, file paths, or analytics
identifiers. Mouseia sees the IP address and request data that any public web
request reveals. Clicking **mouseia.org** or a link in a Markdown document hands
the URL to your operating system and default browser; that destination's
privacy policy applies.

## What is stored locally

- Saved workspace paths and hidden Mouseia recommendations in Skribi's
  eframe/egui application-data folder. On Unix, Skribi sets this folder and its
  files to 0700/0600.
- When you edit a note, Retex-compatible `.retex/history.jsonl` inside the
  library stores previous note text, capped at 50 versions per file. On Unix,
  `.retex` is 0700 and the history and lock files are 0600.
- Packages you export and files you import, in folders you choose. Files Skribi
  writes are 0600 on Unix. ZIP imports unpack into a temporary folder under the
  operating system's temp directory (0700 on Unix) and delete it after import.
- Skribi does not write logs, transcripts, tokens, or credentials.

## Deleting local data

Delete `.retex` inside a library to remove its edit history. Delete exported
packages and imported files you no longer want. Delete Skribi's application-data
folder through your operating system to forget saved workspace paths and hidden
recommendations. There is no remote account or server-side data to delete.

## Third parties

Mouseia is the only network service Skribi contacts, and only after you choose
to browse or add a work. Links you open are handled by your browser and follow
the destination's policy. No third-party fonts, scripts, CDNs, model providers,
or error-reporting services are bundled or contacted by the app.

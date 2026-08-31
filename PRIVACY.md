# Privacy

Skribi is local-first software.

- No account or sign-in.
- No analytics, telemetry, advertising, or tracking.
- No background network service.
- No document upload or cloud sync.
- No collection by Implose Cybernetics through the application.

Skribi reads and writes only the folders and files you choose through the operating-system file picker, launch argument, or an explicit Mouseia download. It stores the last opened Library and Retex paths plus hidden Mouseia recommendation IDs in the operating system’s normal application-data location. Retex-compatible edits may create `.retex/history.jsonl` and lock files inside the selected folder.

Opening a Library requests the public work index from `https://mouseia.org/api/works` for the catalog UI. Choosing **Add** requests that work and writes it into the selected Library. Skribi sends no document contents, account identity, analytics identifier, or telemetry to Mouseia. Opening an external Markdown link may pass the URL to your operating system and default browser. Resulting requests are governed by the destination’s privacy policy.

Because Skribi does not operate a service, there is no remote account or server-side personal data to delete. Delete the application’s local settings through the operating system and remove any files you no longer want to retain.

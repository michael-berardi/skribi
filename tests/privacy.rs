//! Regression tests for the privacy promise: no telemetry dependencies or
//! endpoints, and no network request from simply opening a local library.

use std::{fs, path::PathBuf};

const MARKERS: &[&str] = &[
    "analytics",
    "sentry",
    "posthog",
    "segment.io",
    "segment.com",
    "mixpanel",
    "amplitude",
    "datadog",
    "bugsnag",
    "opentelemetry",
    "googletagmanager",
    "google-analytics",
    "doubleclick",
    "plausible.io",
    "umami.is",
    "hotjar",
    "matomo",
    "huggingface.co",
    "hf.co",
    "jsdelivr",
    "unpkg.com",
];

fn audited_files() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<PathBuf> = [
        "Cargo.toml",
        "Cargo.lock",
        "src/app.rs",
        "src/app/headless_tests.rs",
        "src/document.rs",
        "src/importer.rs",
        "src/lib.rs",
        "src/main.rs",
        "src/mouseia.rs",
        "src/package.rs",
        "src/theme.rs",
        "src/vault.rs",
        "scripts/build-linux-cross.sh",
        "scripts/build-macos-universal.sh",
        "scripts/checksums.py",
        "scripts/package-example.py",
        "scripts/package-macos.sh",
        "scripts/package-release.py",
    ]
    .into_iter()
    .map(|name| root.join(name))
    .collect();
    files.sort();
    files
}

#[test]
fn no_telemetry_packages_or_endpoints_are_reintroduced() {
    for path in audited_files() {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        let lowered = source.to_ascii_lowercase();
        for marker in MARKERS {
            assert!(
                !lowered.contains(marker),
                "{} contains telemetry marker {marker:?}",
                path.display()
            );
        }
    }
}

#[test]
fn opening_a_library_does_not_start_a_catalog_fetch() {
    let app = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/app.rs"))
        .expect("app source");
    let open_folder = app
        .split("fn open_folder(")
        .nth(1)
        .expect("open_folder implementation")
        .split("\n    fn ")
        .next()
        .expect("end of open_folder");
    assert!(
        !open_folder.contains("load_catalog"),
        "opening a local library must not fetch the Mouseia catalog"
    );
}

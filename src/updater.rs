//! Anonymous release checks and verified, reversible updates for the installed app.
//!
//! Network and file work never runs on the UI thread. Skribi keeps one small check cache,
//! its update choices in the normal app settings, and one bounded log for background installs.

use std::{
    collections::BTreeMap,
    env, fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;
pub const CHECK_KEY: &str = "skribi.updates.check";
pub const INSTALL_KEY: &str = "skribi.updates.install";
pub const SKIP_KEY: &str = "skribi.updates.skipped";
const CHECK_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_RELEASE_BYTES: u64 = 512 * 1024;
const MAX_SUMS_BYTES: u64 = 64 * 1024;
const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;
const MAX_LOG_BYTES: usize = 256 * 1024;
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "-update-check");
const CACHE_FILE: &str = "update-check.json";
const LOG_FILE: &str = "updates.log";

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Only stable `x.y.z` tags count; a prerelease or build suffix is not an update.
    pub fn parse(tag: &str) -> Option<Self> {
        let number = |part: Option<&str>| {
            let part = part?;
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let mut parts = tag.strip_prefix('v').unwrap_or(tag).split('.');
        let version = Self {
            major: number(parts.next())?,
            minor: number(parts.next())?,
            patch: number(parts.next())?,
        };
        parts.next().is_none().then_some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseInfo {
    pub version: Version,
    pub notes: String,
    pub assets: Vec<ReleaseAsset>,
}

impl ReleaseInfo {
    pub fn asset_url(&self, name: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.url.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckOutcome {
    Current,
    Available(ReleaseInfo),
}

/// The one small file the check writes.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateCache {
    pub checked_at: Option<u64>,
    pub failure: Option<String>,
    pub latest_version: Option<String>,
    pub last_shown_version: Option<String>,
    pub last_shown_at: Option<u64>,
}

impl UpdateCache {
    pub fn due(&self, now: u64) -> bool {
        self.checked_at
            .is_none_or(|checked| now.saturating_sub(checked) >= CHECK_INTERVAL_SECS)
    }

    pub fn record_outcome(&mut self, outcome: &Result<CheckOutcome, String>, now: u64) {
        self.checked_at = Some(now);
        match outcome {
            Ok(CheckOutcome::Current) => self.failure = None,
            Ok(CheckOutcome::Available(release)) => {
                self.failure = None;
                self.latest_version = Some(release.version.to_string());
            }
            Err(cause) => self.failure = Some(cause.clone()),
        }
    }

    /// A notice for one version appears at most once a day.
    pub fn should_show(&self, version: &str, now: u64) -> bool {
        self.last_shown_version.as_deref() != Some(version)
            || self
                .last_shown_at
                .is_none_or(|shown| now.saturating_sub(shown) >= CHECK_INTERVAL_SECS)
    }

    pub fn mark_shown(&mut self, version: &str, now: u64) {
        self.last_shown_version = Some(version.to_owned());
        self.last_shown_at = Some(now);
    }
}

/// Automatic checks stay off in CI, under coding agents, and when opted out.
pub fn automatic_checks_allowed(var: impl Fn(&str) -> Option<String>) -> bool {
    let set =
        |name: &str| var(name).is_some_and(|value| !matches!(value.trim(), "" | "0" | "false"));
    ![
        "SKRIBI_NO_UPDATE_CHECK",
        "CI",
        "CLAUDECODE",
        "CODEX_SANDBOX",
        "AGENT",
        "ULTRATERM_RUN",
    ]
    .into_iter()
    .any(set)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdatePreferences {
    pub check_automatically: bool,
    pub install_automatically: bool,
    pub skipped_version: Option<String>,
}

impl Default for UpdatePreferences {
    fn default() -> Self {
        Self {
            check_automatically: true,
            install_automatically: false,
            skipped_version: None,
        }
    }
}

impl UpdatePreferences {
    pub fn load(storage: &dyn eframe::Storage) -> Self {
        let flag = |key: &str, default: bool| match storage.get_string(key).as_deref() {
            Some("false") => false,
            Some("true") => true,
            _ => default,
        };
        Self {
            check_automatically: flag(CHECK_KEY, true),
            install_automatically: flag(INSTALL_KEY, false),
            skipped_version: storage
                .get_string(SKIP_KEY)
                .filter(|version| !version.is_empty()),
        }
    }

    pub fn store(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(CHECK_KEY, self.check_automatically.to_string());
        storage.set_string(INSTALL_KEY, self.install_automatically.to_string());
        storage.set_string(SKIP_KEY, self.skipped_version.clone().unwrap_or_default());
    }

    pub fn is_skipped(&self, version: &str) -> bool {
        self.skipped_version.as_deref() == Some(version)
    }
}

fn repository() -> &'static str {
    env!("CARGO_PKG_REPOSITORY")
        .trim_end_matches('/')
        .trim_end_matches(".git")
}

fn repository_slug() -> &'static str {
    repository()
        .strip_prefix("https://github.com/")
        .unwrap_or(repository())
}

/// `SKRIBI_UPDATE_API` points the check at a local test feed.
pub fn check_url() -> String {
    env::var("SKRIBI_UPDATE_API").unwrap_or_else(|_| {
        format!(
            "https://api.github.com/repos/{}/releases/latest",
            repository_slug()
        )
    })
}

pub fn release_page_url() -> String {
    format!("{}/releases/latest", repository())
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cache_dir_for(os: &str, var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let path = |name: &str| {
        var(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let base = match os {
        "macos" => path("HOME").map(|home| home.join("Library").join("Caches")),
        "windows" => path("LOCALAPPDATA"),
        _ => path("XDG_CACHE_HOME").or_else(|| path("HOME").map(|home| home.join(".cache"))),
    };
    base.map(|base| base.join(env!("CARGO_PKG_NAME")))
}

fn cache_dir() -> Option<PathBuf> {
    cache_dir_for(env::consts::OS, |name| env::var(name).ok())
}

pub fn load_cache() -> UpdateCache {
    cache_dir()
        .and_then(|directory| fs::read_to_string(directory.join(CACHE_FILE)).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_cache(cache: &UpdateCache) -> io::Result<()> {
    let directory = cache_dir().ok_or_else(|| io::Error::other("No cache folder is available."))?;
    fs::create_dir_all(&directory)?;
    let text = serde_json::to_vec(cache).map_err(io::Error::other)?;
    write_private(&directory.join(CACHE_FILE), &text)
}

/// Turning off automatic checks removes the check cache and the install log.
pub fn clear_cache_files() {
    if let Some(directory) = cache_dir() {
        let _ = clear_state_files(&directory);
    }
}

fn clear_state_files(directory: &Path) -> io::Result<()> {
    for name in [CACHE_FILE, LOG_FILE] {
        match fs::remove_file(directory.join(name)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    let _ = fs::remove_dir(directory); // Only succeeds when nothing else is in it.
    Ok(())
}

/// Atomic write with owner-only permissions; the temporary file never outlives a failure.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Keep at most `MAX_LOG_BYTES`, dropping the oldest whole lines first.
fn bounded_log(existing: &str, message: &str) -> String {
    let text = format!("{existing}{message}\n");
    if text.len() <= MAX_LOG_BYTES {
        return text;
    }
    let mut start = text.len() - MAX_LOG_BYTES;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    match text[start..].find('\n') {
        Some(offset) if start + offset + 1 < text.len() => text[start + offset + 1..].to_owned(),
        _ => text[start..].to_owned(),
    }
}

/// Local record of background installs. It never leaves the machine.
pub fn log(message: &str) {
    let Some(directory) = cache_dir() else {
        return;
    };
    let path = directory.join(LOG_FILE);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let text = bounded_log(&existing, &format!("{} {message}", now_unix()));
    let _ = fs::create_dir_all(&directory).and_then(|()| write_private(&path, text.as_bytes()));
}

pub fn parse_release(value: &serde_json::Value) -> Option<ReleaseInfo> {
    let flag = |name: &str| value.get(name).and_then(serde_json::Value::as_bool) == Some(true);
    if flag("draft") || flag("prerelease") {
        return None;
    }
    let version = Version::parse(value.get("tag_name")?.as_str()?)?;
    let notes = value
        .get("body")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let assets = value
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|asset| {
            Some(ReleaseAsset {
                name: asset.get("name")?.as_str()?.to_owned(),
                url: asset.get("browser_download_url")?.as_str()?.to_owned(),
            })
        })
        .collect();
    Some(ReleaseInfo {
        version,
        notes,
        assets,
    })
}

/// One anonymous GET: a fixed user agent, no version, identifier, or machine data.
pub fn fetch_latest(url: &str) -> Result<CheckOutcome, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(CHECK_TIMEOUT))
        .build()
        .into();
    let body = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| format!("Could not reach GitHub: {error}"))?
        .body_mut()
        .with_config()
        .limit(MAX_RELEASE_BYTES)
        .read_to_string()
        .map_err(|error| format!("Could not read the release record: {error}"))?;
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("The release record is not valid JSON: {error}"))?;
    match parse_release(&value) {
        Some(release) if Some(release.version) > Version::parse(CURRENT_VERSION) => {
            Ok(CheckOutcome::Available(release))
        }
        _ => Ok(CheckOutcome::Current),
    }
}

pub fn target_asset_name(version: &str, os: &str, arch: &str) -> Result<String, String> {
    let suffix = match (os, arch) {
        ("macos", "aarch64" | "x86_64") => "macos-universal.dmg",
        ("windows", "x86_64") => "windows-x86_64.zip",
        ("linux", "x86_64") => "linux-x86_64.tar.gz",
        _ => {
            return Err(format!(
                "No release package is published for {os} on {arch}."
            ));
        }
    };
    Ok(format!("Skribi-{version}-{suffix}"))
}

pub fn parse_sha256_sums(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (hash, name) = line.trim().split_once(char::is_whitespace)?;
            let name = name.trim().trim_start_matches('*');
            (hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .then(|| (name.to_owned(), hash.to_ascii_lowercase()))
        })
        .collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn verify_asset(
    bytes: &[u8],
    sums: &BTreeMap<String, String>,
    name: &str,
) -> Result<(), String> {
    let expected = sums
        .get(name)
        .ok_or_else(|| format!("{name} is missing from SHA256SUMS."))?;
    if &sha256_hex(bytes) != expected {
        return Err(format!("{name} does not match SHA256SUMS."));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallTarget {
    /// The signed `Skribi.app` bundle on macOS.
    MacApp(PathBuf),
    /// The `skribi` or `skribi.exe` binary from a release archive. Only this file is replaced,
    /// never the folder around it, because the archives are extracted into folders users own.
    Executable(PathBuf),
}

impl InstallTarget {
    pub fn path(&self) -> &Path {
        match self {
            Self::MacApp(path) | Self::Executable(path) => path,
        }
    }
}

/// Where the running copy lives, or why Skribi must not overwrite it.
pub fn classify_install_path(exe: &Path, os: &str, version: &str) -> Result<InstallTarget, String> {
    let normalized = exe.to_string_lossy().replace('\\', "/").to_lowercase();
    let parts: Vec<&str> = normalized.split('/').collect();
    let has = |names: &[&str]| parts.iter().any(|part| names.contains(part));
    if normalized.contains("/apptranslocation/") {
        return Err(
            "Skribi is running from a temporary location. Move it to Applications, then update."
                .into(),
        );
    }
    if has(&["cellar", "homebrew", ".linuxbrew"]) {
        return Err(format!(
            "Skribi was installed with Homebrew. Update it with: brew upgrade {}",
            env!("CARGO_PKG_NAME")
        ));
    }
    if has(&[".cargo"]) {
        return Err(format!(
            "Skribi was installed with Cargo. Update it with: cargo install --locked --git {} --tag v{version}",
            repository()
        ));
    }
    if ["/usr/", "/nix/", "/snap/", "/var/lib/flatpak/"]
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
    {
        return Err("This copy of Skribi is managed by a package manager. Update it there.".into());
    }
    if os == "macos" {
        exe.ancestors()
            .find(|path| path.extension().is_some_and(|extension| extension == "app"))
            .map(|app| InstallTarget::MacApp(app.to_path_buf()))
            .ok_or_else(|| "Only the signed Skribi.app can update itself.".into())
    } else {
        Ok(InstallTarget::Executable(exe.to_path_buf()))
    }
}

fn writable(directory: &Path) -> bool {
    let probe = directory.join(format!(".skribi-write-{}", std::process::id()));
    let created = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    let _ = fs::remove_file(probe);
    created
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallOutcome {
    Installed { version: String, path: PathBuf },
    Manual { reason: String },
}

/// Download, verify, and swap in a release. Returns `Manual` when this copy must be updated by hand.
pub fn install_release(release: &ReleaseInfo) -> Result<InstallOutcome, String> {
    let version = release.version.to_string();
    let exe = env::current_exe().map_err(|error| format!("Could not find Skribi: {error}"))?;
    let target = match classify_install_path(&exe, env::consts::OS, &version) {
        Ok(target) => target,
        Err(reason) => return Ok(InstallOutcome::Manual { reason }),
    };
    if !target.path().parent().is_some_and(writable) {
        return Ok(InstallOutcome::Manual {
            reason: "Skribi cannot replace itself in a read-only folder. Move it to a folder you own, then update."
                .into(),
        });
    }
    // An unsigned build has no Team ID to match, so it cannot accept a signed update.
    let team = match &target {
        InstallTarget::MacApp(app) => match codesign_team_id(app) {
            Some(team) => Some(team),
            None => {
                return Ok(InstallOutcome::Manual {
                    reason: "This copy of Skribi has no Developer ID signature, so it cannot update itself."
                        .into(),
                });
            }
        },
        InstallTarget::Executable(_) => None,
    };
    let name = target_asset_name(&version, env::consts::OS, env::consts::ARCH)?;
    with_temp_workdir(|workdir| {
        let archive = download_verified(release, &name, workdir)?;
        match (&target, &team) {
            (InstallTarget::MacApp(app), Some(team)) => install_mac(&archive, app, team, workdir),
            (target, _) => install_executable(&archive, target.path(), workdir),
        }
    })?;
    Ok(InstallOutcome::Installed {
        version,
        path: target.path().to_path_buf(),
    })
}

fn fetch_bytes(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .into();
    agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("Could not download the update: {error}"))?
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|error| format!("Could not download the update: {error}"))
}

/// Nothing is written outside the temporary folder until the checksum matches.
fn download_verified(release: &ReleaseInfo, name: &str, workdir: &Path) -> Result<PathBuf, String> {
    let asset_url = release
        .asset_url(name)
        .ok_or_else(|| format!("The release does not include {name}."))?;
    let sums_url = release
        .asset_url("SHA256SUMS")
        .ok_or_else(|| "The release does not include SHA256SUMS.".to_owned())?;
    let sums = String::from_utf8(fetch_bytes(sums_url, MAX_SUMS_BYTES)?)
        .map_err(|_| "SHA256SUMS is not valid text.".to_owned())?;
    let bytes = fetch_bytes(asset_url, MAX_ASSET_BYTES)?;
    verify_asset(&bytes, &parse_sha256_sums(&sums), name)?;
    let archive = workdir.join(name);
    fs::write(&archive, &bytes).map_err(|error| format!("Could not save the download: {error}"))?;
    Ok(archive)
}

/// The download folder is removed whether the install succeeds or fails.
fn with_temp_workdir<T>(action: impl FnOnce(&Path) -> Result<T, String>) -> Result<T, String> {
    let workdir = env::temp_dir().join(format!(
        "{}-update-{}-{}",
        env!("CARGO_PKG_NAME"),
        std::process::id(),
        now_unix()
    ));
    fs::create_dir_all(&workdir)
        .map_err(|error| format!("Could not prepare a download: {error}"))?;
    let result = action(&workdir);
    let _ = fs::remove_dir_all(&workdir);
    result
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

fn remove_path(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Once the new version is running, drop its one rollback copy and any abandoned staging copy.
fn remove_install_debris(installed: &Path) -> io::Result<()> {
    for suffix in [".previous", ".new"] {
        remove_path(&with_suffix(installed, suffix))?;
    }
    Ok(())
}

pub fn cleanup_install_debris(exe: &Path) {
    if let Ok(target) = classify_install_path(exe, env::consts::OS, CURRENT_VERSION) {
        let _ = remove_install_debris(target.path());
    }
}

/// Rename within one folder, keeping exactly one rollback. If the second rename fails the
/// first is undone, so a working installation is always left in place.
fn swap_with_rollback(
    installed: &Path,
    staged: &Path,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let previous = with_suffix(installed, ".previous");
    remove_path(&previous)?;
    rename(installed, &previous)?;
    if let Err(error) = rename(staged, installed) {
        let _ = rename(&previous, installed);
        return Err(error);
    }
    Ok(())
}

fn install_executable(archive: &Path, exe: &Path, workdir: &Path) -> Result<(), String> {
    let is_zip = archive
        .extension()
        .is_some_and(|extension| extension == "zip");
    let binary = workdir.join(if is_zip { "skribi.exe" } else { "skribi" });
    if is_zip {
        let mut zip = zip::ZipArchive::new(File::open(archive).map_err(|error| error.to_string())?)
            .map_err(|error| format!("The update archive is unreadable: {error}"))?;
        let mut entry = zip
            .by_name("skribi.exe")
            .map_err(|_| "The update archive does not contain skribi.exe.".to_owned())?;
        let mut output = File::create(&binary).map_err(|error| error.to_string())?;
        io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
    } else {
        run(
            Command::new("tar")
                .arg("-xzf")
                .arg(archive)
                .arg("-C")
                .arg(workdir)
                .arg("skribi"),
            "The update archive does not contain skribi.",
        )?;
    }
    let staged = with_suffix(exe, ".new");
    let result = fs::copy(&binary, &staged)
        .and_then(|_| {
            #[cfg(unix)]
            fs::set_permissions(&staged, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
            swap_with_rollback(exe, &staged, |from, to| fs::rename(from, to))
        })
        .map_err(|error| format!("Could not replace Skribi: {error}"));
    if result.is_err() {
        let _ = remove_path(&staged);
    }
    result
}

fn run(command: &mut Command, failure: &str) -> Result<(), String> {
    match command.stdin(Stdio::null()).output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(failure.to_owned()),
    }
}

pub fn team_id_from_codesign_output(output: &str) -> Option<&str> {
    let team = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("TeamIdentifier="))?
        .trim();
    (!team.is_empty() && team != "not set").then_some(team)
}

fn codesign_team_id(app: &Path) -> Option<String> {
    let output = Command::new("codesign")
        .args(["-dv", "--verbose=4"])
        .arg(app)
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    team_id_from_codesign_output(&String::from_utf8_lossy(&output.stderr)).map(str::to_owned)
}

/// The staged bundle must be intact, signed by the same team as the running app, and accepted
/// by Gatekeeper (Developer ID and notarization) before it may replace anything.
fn verify_mac_app(app: &Path, team: &str) -> Result<(), String> {
    run(
        Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(app),
        "The update's code signature is not valid.",
    )?;
    match codesign_team_id(app) {
        None => return Err("The update has no Developer ID signature.".into()),
        Some(update_team) if update_team != team => {
            return Err("The update is signed by a different developer (Team ID).".into());
        }
        Some(_) => {}
    }
    run(
        Command::new("spctl")
            .args(["--assess", "--type", "execute"])
            .arg(app),
        "macOS Gatekeeper did not accept the update.",
    )
}

fn install_mac(dmg: &Path, app: &Path, team: &str, workdir: &Path) -> Result<(), String> {
    let mount = workdir.join("volume");
    let staged = with_suffix(app, ".new");
    fs::create_dir_all(&mount).map_err(|error| error.to_string())?;
    remove_path(&staged).map_err(|error| error.to_string())?;
    run(
        Command::new("hdiutil")
            .args([
                "attach",
                "-nobrowse",
                "-noautoopen",
                "-readonly",
                "-mountpoint",
            ])
            .arg(&mount)
            .arg(dmg),
        "The disk image could not be opened.",
    )?;
    let copied = fs::read_dir(&mount)
        .ok()
        .and_then(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        })
        .ok_or_else(|| "The disk image does not contain Skribi.app.".to_owned())
        .and_then(|source| {
            run(
                Command::new("ditto").arg(source).arg(&staged),
                "Could not copy the new Skribi.app.",
            )
        });
    let _ = Command::new("hdiutil")
        .args(["detach", "-force"])
        .arg(&mount)
        .output();
    let result = copied
        .and_then(|()| verify_mac_app(&staged, team))
        .and_then(|()| {
            swap_with_rollback(app, &staged, |from, to| fs::rename(from, to))
                .map_err(|error| format!("Could not replace Skribi.app: {error}"))
        });
    if result.is_err() {
        let _ = remove_path(&staged);
    }
    result
}

pub fn open_release_page() -> Result<(), String> {
    let url = release_page_url();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let result = Command::new("explorer").arg(url).spawn();
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let result = Command::new("xdg-open").arg(url).spawn();
    result.map(|_| ()).map_err(|error| error.to_string())
}

/// Start the installed copy once this process has exited, so settings are saved first and
/// the new version never runs beside the old one.
pub fn relaunch_after_exit(installed: &Path) -> Result<(), String> {
    let pid = std::process::id().to_string();
    #[cfg(unix)]
    let mut command = {
        let start = if cfg!(target_os = "macos") {
            "exec open \"$1\""
        } else {
            "exec \"$1\""
        };
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(format!(
                "while kill -0 \"$0\" 2>/dev/null; do sleep 0.2; done; {start}"
            ))
            .arg(&pid)
            .arg(installed);
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command
    };
    #[cfg(windows)]
    let mut command = {
        let path = installed.to_string_lossy().replace('\'', "''");
        let mut command = Command::new("powershell");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "Wait-Process -Id {pid} -ErrorAction SilentlyContinue; Start-Process -FilePath '{path}'"
            ),
        ]);
        std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000);
        command
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read as _, net::TcpListener, thread};

    struct MemoryStorage(BTreeMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    fn scratch(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!("skribi-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    /// Serve fixed paths over HTTP for `requests` connections; returns the base URL and the requests seen.
    fn serve(
        routes: Vec<(&'static str, Vec<u8>)>,
        requests: usize,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..requests {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 4096];
                let length = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..length]).into_owned();
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                let body = routes
                    .iter()
                    .find(|(route, _)| *route == path)
                    .map(|(_, body)| body.clone());
                let (status, body) =
                    body.map_or(("404 Not Found", Vec::new()), |body| ("200 OK", body));
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
                seen.push(request);
            }
            seen
        });
        (base, server)
    }

    #[test]
    fn versions_compare_only_stable_triples() {
        let version = |text| Version::parse(text).unwrap();
        assert!(version("v0.3.0") > version("0.2.0"));
        assert!(version("0.2.10") > version("0.2.9"));
        assert_eq!(version("0.2.0"), version("v0.2.0"));
        assert!(version("v0.1.6") < version("0.2.0"));
        for invalid in [
            "0.3.0-beta.1",
            "v0.3",
            "garbage",
            "v1.2.3.4",
            "+1.2.3",
            "1..3",
            "",
        ] {
            assert!(Version::parse(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn release_parsing_ignores_drafts_prereleases_and_bad_tags() {
        let release = serde_json::json!({
            "tag_name": "v0.3.0",
            "body": "- A calm update\n",
            "draft": false,
            "prerelease": false,
            "assets": [
                {"name": "SHA256SUMS", "browser_download_url": "http://127.0.0.1/sums"},
                {"name": "notes.txt"}
            ]
        });
        let parsed = parse_release(&release).expect("stable release");
        assert_eq!(parsed.version.to_string(), "0.3.0");
        assert_eq!(parsed.notes, "- A calm update");
        assert_eq!(
            parsed.asset_url("SHA256SUMS"),
            Some("http://127.0.0.1/sums")
        );
        assert_eq!(parsed.assets.len(), 1);
        for rejected in [
            serde_json::json!({"tag_name": "v0.3.0", "prerelease": true}),
            serde_json::json!({"tag_name": "v0.3.0", "draft": true}),
            serde_json::json!({"tag_name": "v0.3.0-rc1"}),
            serde_json::json!({"name": "no tag"}),
        ] {
            assert!(parse_release(&rejected).is_none());
        }
    }

    #[test]
    fn sums_parsing_and_verification_reject_mismatches() {
        let good = "0".repeat(64);
        let text = format!(
            "# comment\n{good}  good.bin\n{} *star.bin\nshort other.bin\n",
            "A".repeat(64)
        );
        let sums = parse_sha256_sums(&text);
        assert_eq!(sums.get("good.bin"), Some(&good));
        assert_eq!(sums.get("star.bin"), Some(&"a".repeat(64)));
        assert_eq!(sums.len(), 2);
        let valid = BTreeMap::from([("asset.zip".to_owned(), sha256_hex(b"payload"))]);
        assert!(verify_asset(b"payload", &valid, "asset.zip").is_ok());
        assert!(verify_asset(b"tampered", &valid, "asset.zip").is_err());
        assert!(verify_asset(b"payload", &valid, "missing.zip").is_err());
    }

    #[test]
    fn cache_interval_notice_frequency_and_failure_cause() {
        let mut cache = UpdateCache::default();
        let now = 1_000_000;
        assert!(cache.due(now));
        cache.record_outcome(&Ok(CheckOutcome::Current), now);
        assert!(!cache.due(now + CHECK_INTERVAL_SECS - 1));
        assert!(cache.due(now + CHECK_INTERVAL_SECS));
        assert!(cache.should_show("0.3.0", now));
        cache.mark_shown("0.3.0", now);
        assert!(!cache.should_show("0.3.0", now + 60));
        assert!(cache.should_show("0.4.0", now + 60));
        assert!(cache.should_show("0.3.0", now + CHECK_INTERVAL_SECS));
        cache.record_outcome(&Err("offline".into()), now + 5);
        assert_eq!(cache.failure.as_deref(), Some("offline"));
        assert!(!cache.due(now + 6), "a failure is not retried in a loop");
    }

    #[test]
    fn preferences_default_to_checks_on_installs_off_and_persist() {
        let mut storage = MemoryStorage(BTreeMap::new());
        let mut preferences = UpdatePreferences::load(&storage);
        assert_eq!(preferences, UpdatePreferences::default());
        assert!(preferences.check_automatically);
        assert!(!preferences.install_automatically);
        preferences.install_automatically = true;
        preferences.skipped_version = Some("0.3.0".into());
        preferences.store(&mut storage);
        let restored = UpdatePreferences::load(&storage);
        assert_eq!(restored, preferences);
        assert!(restored.is_skipped("0.3.0"));
        assert!(
            !restored.is_skipped("0.4.0"),
            "a newer release notifies again"
        );
        let mut off = restored;
        off.install_automatically = false;
        off.store(&mut storage);
        assert!(!UpdatePreferences::load(&storage).install_automatically);
    }

    #[test]
    fn automatic_checks_are_silent_in_ci_agents_and_when_opted_out() {
        let with = |pairs: &'static [(&'static str, &'static str)]| {
            automatic_checks_allowed(move |name| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| (*value).to_owned())
            })
        };
        assert!(with(&[]));
        assert!(with(&[("CI", "0"), ("SKRIBI_NO_UPDATE_CHECK", "")]));
        assert!(!with(&[("SKRIBI_NO_UPDATE_CHECK", "1")]));
        assert!(!with(&[("CI", "true")]));
        assert!(!with(&[("CLAUDECODE", "1")]));
        assert!(!with(&[("CODEX_SANDBOX", "seatbelt")]));
    }

    #[test]
    fn cache_folder_follows_each_platform() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| (*value).to_owned())
            }
        };
        assert_eq!(
            cache_dir_for("macos", env(&[("HOME", "/h")])),
            Some(PathBuf::from("/h/Library/Caches/skribi"))
        );
        assert_eq!(
            cache_dir_for("linux", env(&[("HOME", "/h")])),
            Some(PathBuf::from("/h/.cache/skribi"))
        );
        assert_eq!(
            cache_dir_for("linux", env(&[("HOME", "/h"), ("XDG_CACHE_HOME", "/x")])),
            Some(PathBuf::from("/x/skribi"))
        );
        assert_eq!(
            cache_dir_for("windows", env(&[("LOCALAPPDATA", "C:/L")])),
            Some(PathBuf::from("C:/L/skribi"))
        );
        assert_eq!(cache_dir_for("linux", env(&[])), None);
    }

    #[test]
    fn asset_names_match_published_release_assets() {
        assert_eq!(
            target_asset_name("0.3.0", "macos", "aarch64").unwrap(),
            "Skribi-0.3.0-macos-universal.dmg"
        );
        assert_eq!(
            target_asset_name("0.3.0", "windows", "x86_64").unwrap(),
            "Skribi-0.3.0-windows-x86_64.zip"
        );
        assert_eq!(
            target_asset_name("0.3.0", "linux", "x86_64").unwrap(),
            "Skribi-0.3.0-linux-x86_64.tar.gz"
        );
        assert!(target_asset_name("0.3.0", "linux", "aarch64").is_err());
    }

    #[test]
    fn install_kinds_are_detected_and_managed_copies_name_their_command() {
        let classify = |path: &str, os| classify_install_path(Path::new(path), os, "0.3.0");
        assert_eq!(
            classify("/Applications/Skribi.app/Contents/MacOS/skribi", "macos"),
            Ok(InstallTarget::MacApp("/Applications/Skribi.app".into()))
        );
        assert_eq!(
            classify("/home/a/Downloads/skribi", "linux"),
            Ok(InstallTarget::Executable("/home/a/Downloads/skribi".into()))
        );
        let translocated =
            "/private/var/folders/x/T/AppTranslocation/1/d/Skribi.app/Contents/MacOS/skribi";
        assert!(classify(translocated, "macos").is_err());
        assert!(classify("/Users/a/target/release/skribi", "macos").is_err());
        let brew = classify("/opt/homebrew/Cellar/skribi/0.2.0/bin/skribi", "macos").unwrap_err();
        assert!(brew.ends_with("brew upgrade skribi"), "{brew}");
        let cargo = classify("/home/a/.cargo/bin/skribi", "linux").unwrap_err();
        assert!(
            cargo.contains("cargo install --locked --git https://github.com/"),
            "{cargo}"
        );
        assert!(cargo.ends_with("--tag v0.3.0"), "{cargo}");
        assert!(classify("/usr/bin/skribi", "linux").is_err());
    }

    #[test]
    fn team_ids_are_read_from_codesign_output() {
        let signed = "Identifier=org.example\nTeamIdentifier=ABCDE12345\n";
        assert_eq!(team_id_from_codesign_output(signed), Some("ABCDE12345"));
        assert_eq!(
            team_id_from_codesign_output("TeamIdentifier=not set\n"),
            None
        );
        assert_eq!(team_id_from_codesign_output("no signature"), None);
    }

    #[test]
    fn failed_swap_restores_the_working_installation() {
        let root = scratch("swap");
        let installed = root.join("skribi");
        let staged = root.join("skribi.new");
        let previous = root.join("skribi.previous");
        fs::write(&installed, b"old").unwrap();
        fs::write(&staged, b"new").unwrap();
        let mut calls = 0;
        let result = swap_with_rollback(&installed, &staged, |from, to| {
            calls += 1;
            if calls == 2 {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "blocked"));
            }
            fs::rename(from, to)
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&installed).unwrap(), b"old");
        assert!(!previous.exists());
        swap_with_rollback(&installed, &staged, |from, to| fs::rename(from, to)).unwrap();
        assert_eq!(fs::read(&installed).unwrap(), b"new");
        assert_eq!(fs::read(&previous).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }

    /// Mock release server, real archive, real swap: a checksum mismatch changes nothing and a
    /// verified install replaces only the binary and keeps exactly one rollback copy.
    #[cfg(unix)]
    #[test]
    fn install_verifies_before_swapping_and_keeps_one_rollback() {
        let root = scratch("install");
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("skribi"), b"new build").unwrap();
        fs::write(source.join("README.md"), b"readme").unwrap();
        let archive = root.join("Skribi-9.9.9-linux-x86_64.tar.gz");
        assert!(
            Command::new("tar")
                .arg("-czf")
                .arg(&archive)
                .arg("-C")
                .arg(&source)
                .args(["skribi", "README.md"])
                .status()
                .unwrap()
                .success()
        );
        let bytes = fs::read(&archive).unwrap();
        let install = root.join("Downloads");
        fs::create_dir_all(&install).unwrap();
        let exe = install.join("skribi");
        fs::write(&exe, b"old build").unwrap();
        fs::write(install.join("thesis.md"), b"user file").unwrap();
        let name = "Skribi-9.9.9-linux-x86_64.tar.gz";
        let attempt = |sums: String| {
            let (base, server) = serve(
                vec![("/sums", sums.into_bytes()), ("/asset", bytes.clone())],
                2,
            );
            let release = ReleaseInfo {
                version: Version::parse("9.9.9").unwrap(),
                notes: String::new(),
                assets: vec![
                    ReleaseAsset {
                        name: "SHA256SUMS".into(),
                        url: format!("{base}/sums"),
                    },
                    ReleaseAsset {
                        name: name.into(),
                        url: format!("{base}/asset"),
                    },
                ],
            };
            let mut workdir = PathBuf::new();
            let result = with_temp_workdir(|directory| {
                workdir = directory.to_path_buf();
                let archive = download_verified(&release, name, directory)?;
                install_executable(&archive, &exe, directory)
            });
            server.join().unwrap();
            assert!(!workdir.exists(), "temporary download removed");
            result
        };

        let mismatch = attempt(format!("{}  {name}\n", "0".repeat(64)));
        assert!(mismatch.unwrap_err().contains("does not match SHA256SUMS"));
        assert_eq!(fs::read(&exe).unwrap(), b"old build");
        assert!(!with_suffix(&exe, ".previous").exists());
        assert!(!with_suffix(&exe, ".new").exists());

        attempt(format!("{}  {name}\n", sha256_hex(&bytes))).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"new build");
        assert_eq!(
            fs::read(with_suffix(&exe, ".previous")).unwrap(),
            b"old build"
        );
        attempt(format!("{}  {name}\n", sha256_hex(&bytes))).unwrap();
        assert_eq!(
            fs::read(with_suffix(&exe, ".previous")).unwrap(),
            b"new build"
        );
        let mut names: Vec<_> = fs::read_dir(&install)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["skribi", "skribi.previous", "thesis.md"],
            "only the binary changes"
        );

        remove_install_debris(&exe).unwrap();
        assert!(!with_suffix(&exe, ".previous").exists());
        assert_eq!(fs::read(install.join("thesis.md")).unwrap(), b"user file");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn windows_archive_swaps_only_skribi_exe() {
        let root = scratch("zip");
        let archive = root.join("Skribi-9.9.9-windows-x86_64.zip");
        let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
        for (name, body) in [
            ("skribi.exe", b"new exe".as_slice()),
            ("README.md", b"readme"),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
        let exe = root.join("skribi.exe");
        fs::write(&exe, b"old exe").unwrap();
        let workdir = root.join("work");
        fs::create_dir_all(&workdir).unwrap();
        install_executable(&archive, &exe, &workdir).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"new exe");
        assert_eq!(
            fs::read(with_suffix(&exe, ".previous")).unwrap(),
            b"old exe"
        );
        assert!(
            !root.join("README.md").exists(),
            "only the binary is installed"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_cleanup_removes_rollback_and_staging_copies() {
        let root = scratch("cleanup");
        let app = root.join("Skribi.app");
        fs::create_dir_all(app.join("Contents")).unwrap();
        fs::create_dir_all(with_suffix(&app, ".previous").join("Contents")).unwrap();
        fs::create_dir_all(with_suffix(&app, ".new").join("Contents")).unwrap();
        remove_install_debris(&app).unwrap();
        assert!(app.join("Contents").is_dir());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn temp_download_dir_is_removed_on_success_and_failure() {
        let mut seen = Vec::new();
        for fail in [false, true] {
            let result = with_temp_workdir(|directory| {
                seen.push(directory.to_path_buf());
                fs::write(directory.join("asset"), b"bytes").unwrap();
                if fail {
                    Err("checksum mismatch".to_owned())
                } else {
                    Ok(())
                }
            });
            assert_eq!(result.is_err(), fail);
        }
        assert!(seen.iter().all(|directory| !directory.exists()));
    }

    #[test]
    fn log_stays_one_bounded_file() {
        let old = "old line\n".repeat(40_000);
        let bounded = bounded_log(&old, "latest line");
        assert!(bounded.len() <= MAX_LOG_BYTES);
        assert!(bounded.ends_with("old line\nlatest line\n"));
        assert!(bounded.starts_with("old line\n"), "cut at a line boundary");
        assert_eq!(bounded_log("a\n", "b"), "a\nb\n");
    }

    #[test]
    fn cache_is_one_small_private_file_and_clearing_removes_it_and_the_log() {
        let root = scratch("cache");
        let path = root.join(CACHE_FILE);
        let cache = UpdateCache {
            checked_at: Some(42),
            failure: Some("offline".into()),
            latest_version: Some("0.3.0".into()),
            ..UpdateCache::default()
        };
        write_private(&path, &serde_json::to_vec(&cache).unwrap()).unwrap();
        write_private(&path, &serde_json::to_vec(&cache).unwrap()).unwrap();
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        assert!(fs::metadata(&path).unwrap().len() < 512);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let restored: UpdateCache = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(restored, cache);
        fs::write(root.join(LOG_FILE), b"line\n").unwrap();
        clear_state_files(&root).unwrap();
        assert!(!root.exists(), "an emptied cache folder is removed too");
    }

    #[test]
    fn check_is_one_anonymous_request() {
        let body = serde_json::json!({
            "tag_name": "v9.9.9",
            "body": "New release",
            "assets": []
        })
        .to_string()
        .into_bytes();
        let (base, server) = serve(vec![("/latest", body)], 1);
        let outcome = fetch_latest(&format!("{base}/latest")).unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        let request = requests[0].to_lowercase();
        assert!(request.starts_with("get /latest http/1.1"));
        assert!(request.contains("user-agent: skribi-update-check\r\n"));
        assert!(!request.contains(CURRENT_VERSION), "no version is sent");
        assert!(!request.contains("cookie") && !request.contains("authorization"));
        assert!(
            matches!(outcome, CheckOutcome::Available(release) if release.version.to_string() == "9.9.9")
        );

        let current = serde_json::json!({"tag_name": format!("v{CURRENT_VERSION}"), "assets": []})
            .to_string()
            .into_bytes();
        let (base, server) = serve(vec![("/latest", current)], 1);
        assert_eq!(
            fetch_latest(&format!("{base}/latest")),
            Ok(CheckOutcome::Current)
        );
        server.join().unwrap();
        let (base, server) = serve(Vec::new(), 1);
        assert!(fetch_latest(&format!("{base}/latest")).is_err());
        server.join().unwrap();
    }

    /// Real-artifact check of the macOS swap, run by hand:
    /// `SKRIBI_E2E_DMG=/path/Skribi-x.y.z-macos-universal.dmg cargo test macos_swap -- --ignored`.
    /// A copy from the same disk image plays the running app; a re-signed (ad-hoc) image must be
    /// refused on Team ID before anything is swapped.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "needs a signed release disk image"]
    fn macos_swap_checks_team_id_before_replacing_the_app() {
        let dmg = PathBuf::from(env::var("SKRIBI_E2E_DMG").expect("SKRIBI_E2E_DMG"));
        let root = scratch("macos-e2e");
        let mount = root.join("mount");
        fs::create_dir_all(&mount).unwrap();
        let attach = |image: &Path| {
            run(
                Command::new("hdiutil")
                    .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
                    .arg(&mount)
                    .arg(image),
                "attach",
            )
            .unwrap()
        };
        let detach = || {
            let _ = Command::new("hdiutil")
                .args(["detach", "-force"])
                .arg(&mount)
                .output();
        };
        let applications = root.join("Applications");
        fs::create_dir_all(&applications).unwrap();
        let app = applications.join("Skribi.app");
        attach(&dmg);
        run(
            Command::new("ditto")
                .arg(mount.join("Skribi.app"))
                .arg(&app),
            "ditto",
        )
        .unwrap();
        detach();
        let team = codesign_team_id(&app).expect("signed release");
        let wrong = with_temp_workdir(|workdir| install_mac(&dmg, &app, "ZZZZZZZZZZ", workdir));
        assert!(wrong.unwrap_err().contains("Team ID"), "other team refused");
        assert!(!with_suffix(&app, ".new").exists() && !with_suffix(&app, ".previous").exists());

        // Re-signed image: same bytes, ad-hoc signature, so no Team ID.
        let unsigned = root.join("unsigned");
        fs::create_dir_all(&unsigned).unwrap();
        run(
            Command::new("ditto")
                .arg(&app)
                .arg(unsigned.join("Skribi.app")),
            "ditto",
        )
        .unwrap();
        run(
            Command::new("codesign")
                .args(["--force", "--deep", "--sign", "-"])
                .arg(unsigned.join("Skribi.app")),
            "adhoc",
        )
        .unwrap();
        let unsigned_dmg = root.join("unsigned.dmg");
        run(
            Command::new("hdiutil")
                .args(["create", "-quiet", "-format", "UDZO", "-srcfolder"])
                .arg(&unsigned)
                .arg(&unsigned_dmg),
            "create",
        )
        .unwrap();
        let before = fs::metadata(app.join("Contents/Info.plist"))
            .unwrap()
            .modified()
            .unwrap();
        let refused = with_temp_workdir(|workdir| install_mac(&unsigned_dmg, &app, &team, workdir));
        assert!(
            refused.unwrap_err().contains("Developer ID"),
            "ad-hoc update refused"
        );
        assert_eq!(
            fs::metadata(app.join("Contents/Info.plist"))
                .unwrap()
                .modified()
                .unwrap(),
            before
        );
        assert!(!with_suffix(&app, ".new").exists() && !with_suffix(&app, ".previous").exists());

        with_temp_workdir(|workdir| install_mac(&dmg, &app, &team, workdir)).unwrap();
        assert!(app.join("Contents/Info.plist").is_file());
        assert!(
            with_suffix(&app, ".previous")
                .join("Contents/Info.plist")
                .is_file()
        );
        assert!(!with_suffix(&app, ".new").exists());
        remove_install_debris(&app).unwrap();
        assert_eq!(fs::read_dir(&applications).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}

use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use atomic_write_file::AtomicWriteFile;
#[cfg(unix)]
use fs2::FileExt;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::fs::OpenOptions;

use crate::{
    Error, Result,
    document::{Document, fingerprint},
};

const HISTORY_CAPACITY_PER_FILE: usize = 50;
const COCOA_REFERENCE_DATE: u64 = 978_307_200;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BoardColumn {
    pub title: String,
    pub statuses: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VaultConfig {
    pub columns: Vec<BoardColumn>,
    #[serde(default)]
    pub views: Vec<serde_json::Value>,
    #[serde(rename = "recordTypes", default)]
    pub record_types: Vec<serde_json::Value>,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            columns: vec![
                BoardColumn {
                    title: "Inbox".into(),
                    statuses: vec!["Inbox".into(), "New".into()],
                },
                BoardColumn {
                    title: "Qualified".into(),
                    statuses: vec!["Qualified".into()],
                },
                BoardColumn {
                    title: "Proposal".into(),
                    statuses: vec!["Proposal".into(), "Negotiation".into()],
                },
                BoardColumn {
                    title: "Won".into(),
                    statuses: vec!["Won".into()],
                },
            ],
            views: Vec::new(),
            record_types: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Vault {
    pub root: PathBuf,
    pub documents: Vec<Document>,
    pub config: VaultConfig,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct HistoryEntry {
    path: String,
    #[serde(rename = "previousSource")]
    previous_source: String,
    timestamp: f64,
}

impl Vault {
    pub fn open(root: PathBuf) -> Result<Self> {
        let metadata = fs::symlink_metadata(&root)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(Error::Invalid(format!(
                "Library is not a safe directory: {}",
                root.display()
            )));
        }
        let canonical_root = root.canonicalize()?;
        let config = load_config(&canonical_root);
        let mut diagnostics = Vec::new();
        let mut paths = Vec::new();
        inventory_markdown(
            &canonical_root,
            &canonical_root,
            &mut paths,
            &mut diagnostics,
        )?;
        let mut documents = Vec::with_capacity(paths.len());
        for path in paths {
            match Document::load(&path, &canonical_root) {
                Ok(document) => documents.push(document),
                Err(error) => diagnostics.push(format!("{}: {error}", path.display())),
            }
        }
        documents.sort_by(|left, right| {
            right
                .modified
                .cmp(&left.modified)
                .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
        });
        Ok(Self {
            root: canonical_root,
            documents,
            config,
            diagnostics,
        })
    }

    pub fn reload(&mut self) -> Result<()> {
        let reopened = Self::open(self.root.clone())?;
        self.documents = reopened.documents;
        self.config = reopened.config;
        self.diagnostics = reopened.diagnostics;
        Ok(())
    }

    pub fn document(&self, path: &Path) -> Option<&Document> {
        self.documents.iter().find(|document| document.path == path)
    }

    pub fn save_document(
        &mut self,
        path: &Path,
        expected_fingerprint: &str,
        title: &str,
        status: &str,
        body: &str,
    ) -> Result<Document> {
        let root = self.root.clone();
        let path = path.to_owned();
        mutate_with_history(&root, &path, |current| {
            if fingerprint(current) != expected_fingerprint {
                return Err(Error::StaleDocument);
            }
            let current_document = Document::load(&path, &root)?;
            Ok(current_document.render(title, status, body))
        })?;
        let document = Document::load(&path, &root)?;
        if let Some(existing) = self
            .documents
            .iter_mut()
            .find(|candidate| candidate.path == path)
        {
            *existing = document.clone();
        }
        Ok(document)
    }

    pub fn set_status(&mut self, path: &Path, status: &str) -> Result<Document> {
        let document = self
            .document(path)
            .cloned()
            .ok_or_else(|| Error::Invalid("The selected document no longer exists.".into()))?;
        self.save_document(
            path,
            &document.fingerprint,
            &document.title,
            status,
            &document.body,
        )
    }

    pub fn create_note(&mut self) -> Result<Document> {
        let notes = self.root.join("Notes");
        fs::create_dir_all(&notes)?;
        let mut suffix = 1;
        let path = loop {
            let name = if suffix == 1 {
                "Untitled.md".into()
            } else {
                format!("Untitled {suffix}.md")
            };
            let candidate = notes.join(name);
            if !candidate.exists() {
                break candidate;
            }
            suffix += 1;
        };
        atomic_write(
            &path,
            "---\ntitle: Untitled\nstatus: Inbox\ntype: note\n---\n\n",
        )?;
        let document = Document::load(&path, &self.root)?;
        self.documents.insert(0, document.clone());
        Ok(document)
    }
}

fn inventory_markdown(
    root: &Path,
    current: &Path,
    paths: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<String>,
) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                diagnostics.push(error.to_string());
                continue;
            }
        };
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let metadata = match entry.file_type() {
            Ok(metadata) => metadata,
            Err(error) => {
                diagnostics.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        if metadata.is_symlink() {
            diagnostics.push(format!("Skipped symlink: {}", path.display()));
        } else if metadata.is_dir() {
            inventory_markdown(root, &path, paths, diagnostics)?;
        } else if metadata.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            let canonical = path.canonicalize()?;
            if canonical.starts_with(root) {
                paths.push(canonical);
            } else {
                diagnostics.push(format!("Skipped path outside library: {}", path.display()));
            }
        }
    }
    Ok(())
}

fn load_config(root: &Path) -> VaultConfig {
    let path = root.join(".retex").join("config.json");
    let Ok(source) = fs::read_to_string(path) else {
        return VaultConfig::default();
    };
    serde_json::from_str::<VaultConfig>(&source)
        .ok()
        .filter(|config| !config.columns.is_empty())
        .unwrap_or_default()
}

fn mutate_with_history<F>(root: &Path, path: &Path, prepare: F) -> Result<()>
where
    F: FnOnce(&str) -> Result<String>,
{
    let history_dir = root.join(".retex");
    fs::create_dir_all(&history_dir)?;
    let journal = history_dir.join("history.jsonl");
    with_journal_lock(&journal, || {
        let current = fs::read_to_string(path)?;
        let next = prepare(&current)?;
        if current == next {
            return Ok(());
        }
        let mut entries = read_history(&journal)?;
        let previous_entries = entries.clone();
        entries.push(HistoryEntry {
            path: path.to_string_lossy().into_owned(),
            previous_source: current.clone(),
            timestamp: cocoa_timestamp(),
        });
        trim_history(&mut entries, path);
        write_history(&journal, &entries)?;
        if let Err(error) = atomic_write(path, &next) {
            let _ = write_history(&journal, &previous_entries);
            return Err(error);
        }
        Ok(())
    })
}

fn read_history(path: &Path) -> Result<Vec<HistoryEntry>> {
    let mut source = String::new();
    match File::open(path) {
        Ok(mut file) => {
            file.read_to_string(&mut source)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    }
    source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(Error::from))
        .collect()
}

fn write_history(path: &Path, entries: &[HistoryEntry]) -> Result<()> {
    let mut source = String::new();
    for entry in entries {
        source.push_str(&serde_json::to_string(entry)?);
        source.push('\n');
    }
    atomic_write(path, &source)
}

fn trim_history(entries: &mut Vec<HistoryEntry>, path: &Path) {
    let path = path.to_string_lossy();
    let matching = entries.iter().filter(|entry| entry.path == path).count();
    let mut remove = matching.saturating_sub(HISTORY_CAPACITY_PER_FILE);
    entries.retain(|entry| {
        if entry.path == path && remove > 0 {
            remove -= 1;
            false
        } else {
            true
        }
    });
}

fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    let mut file = AtomicWriteFile::options().open(path)?;
    file.write_all(contents.as_bytes())?;
    file.commit()?;
    Ok(())
}

fn cocoa_timestamp() -> f64 {
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    unix.as_secs_f64() - COCOA_REFERENCE_DATE as f64
}

#[cfg(unix)]
fn with_journal_lock<T>(journal: &Path, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let lock_path = PathBuf::from(format!("{}.lock", journal.display()));
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock_exclusive()?;
    let result = operation();
    let unlock_result = lock.unlock();
    result.and_then(|value| {
        unlock_result?;
        Ok(value)
    })
}

#[cfg(windows)]
fn with_journal_lock<T>(journal: &Path, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let lock_dir = PathBuf::from(format!("{}.lockdir", journal.display()));
    let deadline = SystemTime::now() + Duration::from_secs(10);
    loop {
        match fs::create_dir(&lock_dir) {
            Ok(()) => break,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = fs::metadata(&lock_dir)
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                    .is_some_and(|age| age > Duration::from_secs(30));
                if stale {
                    let _ = fs::remove_dir(&lock_dir);
                    continue;
                }
                if SystemTime::now() >= deadline {
                    return Err(Error::Invalid(
                        "Timed out waiting for Retex history lock.".into(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error.into()),
        }
    }
    let result = operation();
    let cleanup = fs::remove_dir(&lock_dir);
    result.and_then(|value| {
        cleanup?;
        Ok(value)
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{HISTORY_CAPACITY_PER_FILE, HistoryEntry, Vault, trim_history};

    #[test]
    fn scans_and_saves_with_retex_history() {
        let root = std::env::temp_dir().join(format!("skribi-vault-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("note.md"),
            "---\ntitle: Note\ncustom: yes\n---\nOld",
        )
        .unwrap();
        let mut vault = Vault::open(root.clone()).unwrap();
        let note = vault.documents[0].clone();
        let saved = vault
            .save_document(&note.path, &note.fingerprint, "Renamed", "Qualified", "New")
            .unwrap();
        assert_eq!(saved.title, "Renamed");
        assert!(saved.source.contains("custom: yes"));
        let history = fs::read_to_string(root.join(".retex/history.jsonl")).unwrap();
        assert!(history.contains("previousSource"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_only_newest_entries_for_each_file() {
        let path = std::path::Path::new("/note.md");
        let mut entries = (0..HISTORY_CAPACITY_PER_FILE + 3)
            .map(|index| HistoryEntry {
                path: "/note.md".into(),
                previous_source: index.to_string(),
                timestamp: index as f64,
            })
            .collect();
        trim_history(&mut entries, path);
        assert_eq!(entries.len(), HISTORY_CAPACITY_PER_FILE);
        assert_eq!(entries[0].previous_source, "3");
    }
}

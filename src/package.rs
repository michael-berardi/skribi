use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{Error, Result};

const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct LibraryManifest {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub description: String,
    pub created_by: String,
    pub documents: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
}

pub fn export_library(
    root: &Path,
    output: &Path,
    name: &str,
    description: &str,
) -> Result<LibraryManifest> {
    let root = root.canonicalize()?;
    let mut files = Vec::new();
    inventory(&root, &root, &mut files)?;
    let mut total = 0_u64;
    for path in &files {
        total = total
            .checked_add(fs::metadata(path)?.len())
            .ok_or_else(|| Error::Invalid("Package size overflow.".into()))?;
        if total > MAX_TOTAL_BYTES {
            return Err(Error::Invalid(
                "Package contents exceed the 1 GiB limit.".into(),
            ));
        }
    }
    let documents = files
        .iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        })
        .map(|path| {
            path.strip_prefix(&root)
                .expect("inventory only returns descendants")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    let manifest = LibraryManifest {
        format: "skribi-library".into(),
        version: 1,
        name: name.trim().to_owned(),
        description: description.trim().to_owned(),
        created_by: "Skribi by Pareto Cybernetics".into(),
        documents,
        license: None,
    };

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let atomic = AtomicWriteFile::options().open(output)?;
    let mut archive = ZipWriter::new(atomic);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    archive.start_file("library.json", options)?;
    archive.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
    archive.write_all(b"\n")?;

    for path in files {
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| Error::Invalid(format!("Unsafe package path: {}", path.display())))?;
        let archive_name = format!("content/{}", relative.to_string_lossy().replace('\\', "/"));
        archive.start_file(archive_name, options)?;
        let mut source = File::open(path)?;
        std::io::copy(&mut source, &mut archive)?;
    }
    let atomic = archive.finish()?;
    atomic.commit()?;
    Ok(manifest)
}

pub fn import_library(package: &Path, destination: &Path) -> Result<LibraryManifest> {
    require_empty_destination(destination)?;
    let file = File::open(package)?;
    if file.metadata()?.len() > MAX_TOTAL_BYTES {
        return Err(Error::Invalid("Package exceeds the 1 GiB limit.".into()));
    }
    let mut archive = ZipArchive::new(file)?;
    validate_archive(&mut archive)?;
    let manifest = read_manifest(&mut archive)?;
    if manifest.format != "skribi-library" || manifest.version != 1 {
        return Err(Error::Invalid("Unsupported Skribi package version.".into()));
    }

    fs::create_dir_all(destination)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(Error::Invalid(format!(
                "Unsafe package entry: {}",
                entry.name()
            )));
        };
        let Ok(relative) = enclosed.strip_prefix("content") else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = AtomicWriteFile::options().open(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.commit()?;
    }
    Ok(manifest)
}

pub fn inspect_manifest(package: &Path) -> Result<LibraryManifest> {
    let file = File::open(package)?;
    let mut archive = ZipArchive::new(file)?;
    validate_archive(&mut archive)?;
    read_manifest(&mut archive)
}

fn inventory(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let kind = entry.file_type()?;
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if kind.is_symlink() {
            return Err(Error::Invalid(format!(
                "Packages cannot contain symlinks: {}",
                path.display()
            )));
        }
        if kind.is_dir() {
            inventory(root, &path, files)?;
        } else if kind.is_file() {
            let metadata = entry.metadata()?;
            if metadata.len() > MAX_ENTRY_BYTES {
                return Err(Error::Invalid(format!(
                    "Package file exceeds 64 MiB: {}",
                    path.display()
                )));
            }
            let canonical = path.canonicalize()?;
            if !canonical.starts_with(root) {
                return Err(Error::Invalid(format!(
                    "Package path escapes the library: {}",
                    path.display()
                )));
            }
            files.push(canonical);
            if files.len() > MAX_ENTRIES {
                return Err(Error::Invalid("Package contains too many files.".into()));
            }
        }
    }
    Ok(())
}

fn validate_archive(archive: &mut ZipArchive<File>) -> Result<()> {
    if archive.len() > MAX_ENTRIES {
        return Err(Error::Invalid("Archive contains too many entries.".into()));
    }
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name();
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| Error::Invalid(format!("Unsafe archive entry: {name}")))?;
        if enclosed.is_absolute()
            || enclosed
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(Error::Invalid(format!("Unsafe archive entry: {name}")));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(Error::Invalid(format!(
                "Archive contains a symlink: {name}"
            )));
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(Error::Invalid(format!(
                "Archive entry exceeds 64 MiB: {name}"
            )));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| Error::Invalid("Archive size overflow.".into()))?;
        if total > MAX_TOTAL_BYTES {
            return Err(Error::Invalid(
                "Archive expands beyond the 1 GiB limit.".into(),
            ));
        }
    }
    Ok(())
}

fn read_manifest(archive: &mut ZipArchive<File>) -> Result<LibraryManifest> {
    let mut entry = archive
        .by_name("library.json")
        .map_err(|_| Error::Invalid("This ZIP is not a Skribi library package.".into()))?;
    if entry.size() > 1024 * 1024 {
        return Err(Error::Invalid("Package manifest is too large.".into()));
    }
    let mut source = String::new();
    entry.read_to_string(&mut source)?;
    Ok(serde_json::from_str(&source)?)
}

fn require_empty_destination(destination: &Path) -> Result<()> {
    if destination.exists()
        && (!destination.is_dir() || fs::read_dir(destination)?.next().is_some())
    {
        return Err(Error::Invalid(
            "Import destination must be a new or empty folder.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{export_library, import_library};

    #[test]
    fn package_roundtrip_preserves_markdown_and_assets() {
        let base = std::env::temp_dir().join(format!("skribi-package-{}", std::process::id()));
        let source = base.join("source");
        let destination = base.join("destination");
        let package = base.join("library.skribi");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(source.join("images")).unwrap();
        fs::write(source.join("book.md"), "# Book\n\nText").unwrap();
        fs::write(source.join("images/cover.txt"), "cover").unwrap();
        export_library(&source, &package, "Book", "A test library").unwrap();
        let manifest = import_library(&package, &destination).unwrap();
        assert_eq!(manifest.name, "Book");
        assert_eq!(
            fs::read_to_string(destination.join("book.md")).unwrap(),
            "# Book\n\nText"
        );
        assert_eq!(
            fs::read_to_string(destination.join("images/cover.txt")).unwrap(),
            "cover"
        );
        let _ = fs::remove_dir_all(base);
    }
}

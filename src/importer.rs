use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::Write,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use atomic_write_file::AtomicWriteFile;
use zip::ZipArchive;

use crate::{Error, Result};

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat {
    Auto,
    Notion,
    Obsidian,
    Markdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportResult {
    pub format: ImportFormat,
    pub notes: usize,
    pub assets: usize,
    pub converted_tables: usize,
    pub destination: PathBuf,
}

pub fn import_source(
    source: &Path,
    destination: &Path,
    requested: ImportFormat,
) -> Result<ImportResult> {
    require_empty_destination(destination)?;
    let is_zip = source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
    let temporary = std::env::temp_dir().join(format!(
        "skribi-import-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let import_root = if is_zip {
        extract_zip(source, &temporary)?;
        single_content_root(&temporary)
    } else {
        let metadata = fs::symlink_metadata(source)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(Error::Invalid(
                "Import source must be a folder or Notion ZIP export.".into(),
            ));
        }
        source.canonicalize()?
    };

    let result = (|| {
        let format = match requested {
            ImportFormat::Auto if is_zip => ImportFormat::Notion,
            ImportFormat::Auto if import_root.join(".obsidian").is_dir() => ImportFormat::Obsidian,
            ImportFormat::Auto => ImportFormat::Markdown,
            explicit => explicit,
        };
        let mut files = Vec::new();
        inventory(&import_root, &import_root, &mut files, &mut 0_u64)?;
        let paths = normalized_paths(&files, &import_root, format == ImportFormat::Notion)?;
        fs::create_dir_all(destination)?;
        let mut notes = 0;
        let mut assets = 0;
        let mut converted_tables = 0;

        for file in files {
            let relative = relative_slash(&file, &import_root)?;
            let mapped = paths
                .get(&relative)
                .ok_or_else(|| Error::Invalid(format!("Unsafe import path: {}", file.display())))?;
            let target = destination.join(mapped);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let extension = file
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if extension == "md" {
                let mut markdown = fs::read_to_string(&file)?;
                if format == ImportFormat::Notion {
                    markdown = rewrite_notion_links(&markdown, &paths);
                }
                atomic_write(&target, markdown.as_bytes())?;
                notes += 1;
            } else {
                atomic_copy(&file, &target)?;
                assets += 1;
                if format == ImportFormat::Notion && extension == "csv" {
                    let markdown_target = target.with_extension("md");
                    if !markdown_target.exists() {
                        let title = markdown_target
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or("Table");
                        let table = notion_table(&fs::read_to_string(&file)?, title);
                        atomic_write(&markdown_target, table.as_bytes())?;
                        notes += 1;
                        converted_tables += 1;
                    }
                }
            }
        }
        Ok(ImportResult {
            format,
            notes,
            assets,
            converted_tables,
            destination: destination.to_owned(),
        })
    })();

    if is_zip {
        let _ = fs::remove_dir_all(temporary);
    }
    result
}

fn extract_zip(source: &Path, destination: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_TOTAL_BYTES
    {
        return Err(Error::Invalid(
            "Notion ZIP exceeds the 1 GiB archive limit or is unsafe.".into(),
        ));
    }
    let file = File::open(source)?;
    let mut archive = ZipArchive::new(file)?;
    if archive.len() > MAX_ENTRIES {
        return Err(Error::Invalid("ZIP contains too many entries.".into()));
    }
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        validate_zip_entry(&entry)?;
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| Error::Invalid("ZIP size overflow.".into()))?;
        if total > MAX_TOTAL_BYTES {
            return Err(Error::Invalid("ZIP expands beyond the 1 GiB limit.".into()));
        }
    }
    fs::create_dir_all(destination)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| Error::Invalid(format!("Unsafe ZIP entry: {}", entry.name())))?;
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = AtomicWriteFile::options().open(&target)?;
            std::io::copy(&mut entry, &mut output)?;
            output.commit()?;
        }
    }
    Ok(destination.to_owned())
}

fn validate_zip_entry(entry: &zip::read::ZipFile<'_, File>) -> Result<()> {
    let name = entry.name();
    let enclosed = entry
        .enclosed_name()
        .ok_or_else(|| Error::Invalid(format!("Unsafe ZIP entry: {name}")))?;
    if enclosed.is_absolute()
        || enclosed
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
        || entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
    {
        return Err(Error::Invalid(format!("Unsafe ZIP entry: {name}")));
    }
    if entry.size() > MAX_FILE_BYTES {
        return Err(Error::Invalid(format!("ZIP entry exceeds 64 MiB: {name}")));
    }
    Ok(())
}

fn inventory(root: &Path, current: &Path, files: &mut Vec<PathBuf>, total: &mut u64) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(Error::Invalid(format!(
                "Import contains a symlink: {}",
                path.display()
            )));
        }
        if kind.is_dir() {
            inventory(root, &path, files, total)?;
        } else if kind.is_file() {
            let size = entry.metadata()?.len();
            *total = total
                .checked_add(size)
                .ok_or_else(|| Error::Invalid("Import size overflow.".into()))?;
            if size > MAX_FILE_BYTES || *total > MAX_TOTAL_BYTES {
                return Err(Error::Invalid(format!(
                    "Import exceeds the 64 MiB per-file or 1 GiB total limit: {}",
                    path.display()
                )));
            }
            let canonical = path.canonicalize()?;
            if !canonical.starts_with(root) {
                return Err(Error::Invalid(format!(
                    "Import path escapes its source: {}",
                    path.display()
                )));
            }
            files.push(canonical);
            if files.len() > MAX_ENTRIES {
                return Err(Error::Invalid("Import contains too many files.".into()));
            }
        }
    }
    Ok(())
}

fn normalized_paths(
    files: &[PathBuf],
    root: &Path,
    notion: bool,
) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let mut used = HashSet::new();
    for file in files {
        let relative = relative_slash(file, root)?;
        let normalized = relative
            .split('/')
            .map(|component| {
                if notion {
                    strip_notion_id(component)
                } else {
                    component.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("/");
        let base = if normalized.is_empty() {
            file.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Untitled")
                .to_owned()
        } else {
            normalized
        };
        let mut candidate = base.clone();
        let mut suffix = 2;
        while !used.insert(candidate.to_ascii_lowercase()) {
            candidate = suffixed_path(&base, suffix);
            suffix += 1;
        }
        result.insert(relative, candidate);
    }
    Ok(result)
}

fn strip_notion_id(component: &str) -> String {
    let (stem, extension) = component
        .rsplit_once('.')
        .map_or((component, ""), |(stem, extension)| (stem, extension));
    let stripped = if stem.len() >= 33 {
        let split = stem.len() - 32;
        let (prefix, candidate) = stem.split_at(split);
        if candidate.bytes().all(|byte| byte.is_ascii_hexdigit()) && prefix.ends_with([' ', '-']) {
            prefix.trim_end_matches([' ', '-']).trim()
        } else {
            stem
        }
    } else {
        stem
    };
    if extension.is_empty() {
        stripped.to_owned()
    } else {
        format!("{stripped}.{extension}")
    }
}

fn rewrite_notion_links(markdown: &str, paths: &BTreeMap<String, String>) -> String {
    let mut rewritten = markdown.to_owned();
    for (source, destination) in paths
        .iter()
        .filter(|(source, destination)| source != destination)
    {
        let source_name = source.rsplit('/').next().unwrap_or(source);
        let destination_name = destination.rsplit('/').next().unwrap_or(destination);
        rewritten = rewritten.replace(source_name, destination_name);
        rewritten = rewritten.replace(
            &percent_encode(source_name),
            &percent_encode(destination_name),
        );
    }
    rewritten
}

fn notion_table(source: &str, title: &str) -> String {
    let rows = parse_csv(source);
    let Some(header) = rows.first().filter(|row| !row.is_empty()) else {
        return format!("# {title}\n");
    };
    let width = header.len();
    let cells = |row: &[String]| {
        let mut values = row.to_vec();
        values.resize(width, String::new());
        format!(
            "| {} |",
            values[..width]
                .iter()
                .map(|value| value.replace('|', "\\|").replace('\n', "<br>"))
                .collect::<Vec<_>>()
                .join(" | ")
        )
    };
    let mut markdown = vec![
        format!("# {title}"),
        String::new(),
        cells(header),
        cells(&vec!["---".into(); width]),
    ];
    markdown.extend(rows.iter().skip(1).map(|row| cells(row)));
    markdown.join("\n") + "\n"
}

fn parse_csv(source: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = source.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.peek() == Some(&'"') => {
                field.push('"');
                characters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut field)),
            '\r' | '\n' if !quoted => {
                if character == '\r' && characters.peek() == Some(&'\n') {
                    characters.next();
                }
                row.push(std::mem::take(&mut field));
                if row.iter().any(|value| !value.is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            _ => field.push(character),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        if row.iter().any(|value| !value.is_empty()) {
            rows.push(row);
        }
    }
    rows
}

fn single_content_root(root: &Path) -> PathBuf {
    let Ok(entries) =
        fs::read_dir(root).and_then(|entries| entries.collect::<std::io::Result<Vec<_>>>())
    else {
        return root.to_owned();
    };
    if entries.len() == 1 && entries[0].file_type().is_ok_and(|kind| kind.is_dir()) {
        entries[0].path()
    } else {
        root.to_owned()
    }
}

fn suffixed_path(path: &str, suffix: usize) -> String {
    let candidate = Path::new(path);
    let stem = candidate
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(path);
    let file_name = match candidate.extension().and_then(|value| value.to_str()) {
        Some(extension) => format!("{stem}-{suffix}.{extension}"),
        None => format!("{stem}-{suffix}"),
    };
    candidate
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or(file_name.clone(), |parent| {
            format!(
                "{}/{}",
                parent.to_string_lossy().replace('\\', "/"),
                file_name
            )
        })
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn relative_slash(path: &Path, root: &Path) -> Result<String> {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| Error::Invalid(format!("Path is outside import root: {}", path.display())))
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

fn atomic_copy(source: &Path, destination: &Path) -> Result<()> {
    let mut input = File::open(source)?;
    let mut output = AtomicWriteFile::options().open(destination)?;
    std::io::copy(&mut input, &mut output)?;
    output.commit()?;
    Ok(())
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let mut output = AtomicWriteFile::options().open(path)?;
    output.write_all(contents)?;
    output.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::{ImportFormat, import_source, strip_notion_id};

    #[test]
    fn strips_notion_identifiers_without_touching_normal_names() {
        assert_eq!(
            strip_notion_id("Page 0123456789abcdef0123456789abcdef.md"),
            "Page.md"
        );
        assert_eq!(
            strip_notion_id("chapter-0123456789abcdef0123456789abcdef"),
            "chapter"
        );
        assert_eq!(strip_notion_id("2026 notes.md"), "2026 notes.md");
    }

    #[test]
    fn imports_obsidian_folders_without_copying_settings() {
        let base = std::env::temp_dir().join(format!("skribi-obsidian-{}", std::process::id()));
        let source = base.join("source");
        let destination = base.join("destination");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(source.join(".obsidian")).unwrap();
        fs::write(source.join("Note.md"), "# Note").unwrap();
        fs::write(source.join(".obsidian/app.json"), "{}").unwrap();
        let result = import_source(&source, &destination, ImportFormat::Auto).unwrap();
        assert_eq!(result.format, ImportFormat::Obsidian);
        assert!(destination.join("Note.md").is_file());
        assert!(!destination.join(".obsidian").exists());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn imports_notion_zip_names_links_assets_and_csv_tables() {
        let base = std::env::temp_dir().join(format!("skribi-notion-{}", std::process::id()));
        let archive_path = base.join("notion.zip");
        let destination = base.join("destination");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let file = fs::File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        archive
            .start_file("Export/Page 0123456789abcdef0123456789abcdef.md", options)
            .unwrap();
        archive
            .write_all(b"# Page\n\n[Table](Table%200123456789abcdef0123456789abcdef.csv)")
            .unwrap();
        archive
            .start_file("Export/Table 0123456789abcdef0123456789abcdef.csv", options)
            .unwrap();
        archive.write_all(b"Name,Value\nAlpha,1\n").unwrap();
        archive.finish().unwrap();

        let result = import_source(&archive_path, &destination, ImportFormat::Auto).unwrap();
        assert_eq!(result.format, ImportFormat::Notion);
        assert_eq!(result.notes, 2);
        assert_eq!(result.assets, 1);
        assert_eq!(result.converted_tables, 1);
        let page = fs::read_to_string(destination.join("Page.md")).unwrap();
        assert!(page.contains("[Table](Table.csv)"));
        assert!(destination.join("Table.csv").is_file());
        assert!(
            fs::read_to_string(destination.join("Table.md"))
                .unwrap()
                .contains("| Name | Value |")
        );
        let _ = fs::remove_dir_all(base);
    }
}

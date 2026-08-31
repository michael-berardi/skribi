use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use sha2::{Digest, Sha256};

use crate::{Error, Result};

const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Document {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub title: String,
    pub metadata: BTreeMap<String, String>,
    pub tags: Vec<String>,
    pub body: String,
    pub source: String,
    pub fingerprint: String,
    pub modified: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingSection<'a> {
    pub title: &'a str,
    pub body: &'a str,
}

impl Document {
    pub fn load(path: &Path, root: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(Error::Invalid(format!(
                "Refusing unsafe document: {}",
                path.display()
            )));
        }
        if metadata.len() > MAX_DOCUMENT_BYTES {
            return Err(Error::Invalid(format!(
                "Document exceeds 64 MiB: {}",
                path.display()
            )));
        }

        let source = fs::read_to_string(path)?;
        let (properties, body) = parse_source(&source);
        let title = properties
            .get("title")
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .or_else(|| first_heading(&body))
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Untitled")
                    .to_owned()
            });
        let tags = properties
            .get("tags")
            .map(|value| parse_tags(value))
            .unwrap_or_default();
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |value| value.as_secs());
        let relative_path = path
            .strip_prefix(root)
            .map_err(|_| {
                Error::Invalid(format!(
                    "Document is outside the library: {}",
                    path.display()
                ))
            })?
            .to_owned();

        Ok(Self {
            path: path.to_owned(),
            relative_path,
            title,
            metadata: properties,
            tags,
            body,
            fingerprint: fingerprint(&source),
            source,
            modified,
        })
    }

    pub fn metadata_value(&self, key: &str) -> &str {
        self.metadata.get(key).map_or("", String::as_str)
    }

    pub fn is_book(&self) -> bool {
        matches!(
            self.metadata_value("type").to_ascii_lowercase().as_str(),
            "book" | "article"
        ) || !self.metadata_value("author").is_empty()
    }

    pub fn reading_sections(&self) -> Vec<ReadingSection<'_>> {
        reading_sections(&self.body)
    }

    pub fn render(&self, title: &str, status: &str, body: &str) -> String {
        let mut source = replace_body(&self.source, body);
        source = upsert_property(&source, "title", title.trim());
        if !status.trim().is_empty() {
            source = upsert_property(&source, "status", status.trim());
        }
        source
    }
}

pub fn fingerprint(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

pub fn parse_source(source: &str) -> (BTreeMap<String, String>, String) {
    let Some((front, body)) = split_front_matter(source) else {
        return (BTreeMap::new(), source.to_owned());
    };

    let mut properties = BTreeMap::new();
    let mut multiline_key: Option<String> = None;
    let mut multiline_values = Vec::new();
    for line in front.lines() {
        let trimmed = line.trim();
        if let Some(key) = multiline_key.as_ref() {
            if let Some(value) = trimmed.strip_prefix("- ") {
                multiline_values.push(clean_scalar(value));
                continue;
            }
            properties.insert(key.clone(), multiline_values.join(", "));
            multiline_key = None;
            multiline_values.clear();
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        if value.trim().is_empty() {
            multiline_key = Some(key);
        } else {
            properties.insert(key, clean_scalar(value));
        }
    }
    if let Some(key) = multiline_key {
        properties.insert(key, multiline_values.join(", "));
    }
    (properties, body.to_owned())
}

pub fn reading_sections(body: &str) -> Vec<ReadingSection<'_>> {
    let sections = split_heading_sections(body, "## ");
    if sections
        .iter()
        .filter(|section| is_reading_section(section.title))
        .count()
        < 2
    {
        return Vec::new();
    }
    sections
        .into_iter()
        .filter(|section| is_reading_section(section.title))
        .collect()
}

pub fn reading_chapters(body: &str) -> Vec<ReadingSection<'_>> {
    split_heading_sections(body, "### ")
}

fn split_heading_sections<'a>(body: &'a str, prefix: &str) -> Vec<ReadingSection<'a>> {
    let mut sections = Vec::new();
    let mut title = None;
    let mut content_start = 0;
    let mut offset = 0;

    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if let Some(next_title) = trimmed.strip_prefix(prefix) {
            if let Some(current_title) = title.take() {
                sections.push(ReadingSection {
                    title: current_title,
                    body: body[content_start..offset].trim(),
                });
            }
            title = Some(next_title.trim());
            content_start = offset + line.len();
        }
        offset += line.len();
    }
    if let Some(current_title) = title {
        sections.push(ReadingSection {
            title: current_title,
            body: body[content_start..].trim(),
        });
    }
    sections
}

fn is_reading_section(title: &str) -> bool {
    title.eq_ignore_ascii_case("original")
        || title
            .split(|character: char| !character.is_alphanumeric())
            .any(|word| word.eq_ignore_ascii_case("translation"))
}

fn split_front_matter(source: &str) -> Option<(&str, &str)> {
    let normalized_start = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))?;
    let mut offset = source.len() - normalized_start.len();
    for line in normalized_start.split_inclusive('\n') {
        let without_newline = line.trim_end_matches(['\r', '\n']);
        if without_newline == "---" {
            let front = &source[(source.len() - normalized_start.len())..offset];
            let body_start = offset + line.len();
            return Some((front.trim_end_matches(['\r', '\n']), &source[body_start..]));
        }
        offset += line.len();
    }
    None
}

fn replace_body(source: &str, body: &str) -> String {
    if let Some((front, _)) = split_front_matter(source) {
        let newline = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        return format!("---{newline}{front}{newline}---{newline}{body}");
    }
    body.to_owned()
}

fn upsert_property(source: &str, key: &str, value: &str) -> String {
    let safe_value = serde_json::to_string(&value.replace(['\r', '\n'], " "))
        .expect("serializing a string cannot fail");
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    if let Some((front, body)) = split_front_matter(source) {
        let mut found = false;
        let mut lines = Vec::new();
        for line in front.lines() {
            let matches = line
                .split_once(':')
                .is_some_and(|(candidate, _)| candidate.trim().eq_ignore_ascii_case(key));
            if matches && !found {
                lines.push(format!("{key}: {safe_value}"));
                found = true;
            } else {
                lines.push(line.to_owned());
            }
        }
        if !found {
            lines.push(format!("{key}: {safe_value}"));
        }
        format!(
            "---{newline}{}{newline}---{newline}{body}",
            lines.join(newline)
        )
    } else {
        format!("---{newline}{key}: {safe_value}{newline}---{newline}{newline}{source}")
    }
}

fn clean_scalar(value: &str) -> String {
    let value = value.trim();
    if value.starts_with('"') && value.ends_with('"') {
        serde_json::from_str(value).unwrap_or_else(|_| value.trim_matches('"').to_owned())
    } else {
        value.trim_matches('\'').replace("''", "'")
    }
}

fn parse_tags(value: &str) -> Vec<String> {
    value
        .trim_matches(['[', ']'])
        .split(',')
        .map(clean_scalar)
        .filter(|tag| !tag.is_empty())
        .collect()
}

fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{parse_source, reading_chapters, reading_sections, upsert_property};

    #[test]
    fn preserves_unknown_front_matter_when_updating() {
        let source = "---\ntitle: Old\ncustom: keep me\n---\nBody";
        let updated = upsert_property(source, "title", "New: \"quoted\"");
        assert_eq!(
            updated,
            "---\ntitle: \"New: \\\"quoted\\\"\"\ncustom: keep me\n---\nBody"
        );
        assert_eq!(parse_source(&updated).0["title"], "New: \"quoted\"");
    }

    #[test]
    fn parses_retex_inline_and_multiline_tags() {
        let (inline, _) = parse_source("---\ntags: [one, two]\n---\nText");
        assert_eq!(inline["tags"], "[one, two]");
        let (multiline, _) = parse_source("---\ntags:\n  - one\n  - two\n---\nText");
        assert_eq!(multiline["tags"], "one, two");
    }

    #[test]
    fn finds_mouseia_translation_sections() {
        let sections =
            reading_sections("Intro\n## Original\nGreek\n## Curated translation\nEnglish");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[1].body, "English");
    }

    #[test]
    fn splits_book_chapters_for_navigation() {
        let chapters = reading_chapters("### Book I\nFirst\n### Book II\nSecond");
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "Book I");
        assert_eq!(chapters[1].body, "Second");
    }
}

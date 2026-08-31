use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use atomic_write_file::AtomicWriteFile;
use serde::Deserialize;

use crate::{Error, Result};

pub const SITE_URL: &str = "https://mouseia.org";
const CATALOG_URL: &str = "https://mouseia.org/api/works";
const MAX_CATALOG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_WORK_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WorkSummary {
    pub slug: String,
    pub title: String,
    pub author: String,
    pub tradition: String,
    pub language: String,
    pub genre: Option<String>,
    pub period: Option<String>,
    pub translations: usize,
}

#[derive(Debug, Deserialize)]
struct CatalogResponse {
    schema_version: u32,
    works: Vec<WorkSummary>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Work {
    pub slug: String,
    pub title: String,
    pub author: String,
    pub tradition: String,
    pub language: String,
    pub genre: Option<String>,
    pub period: Option<String>,
    pub source_url: Option<String>,
    pub original: Edition,
    pub translations: Vec<Translation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Edition {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Translation {
    pub translator: String,
    pub kind: String,
    pub note: Option<String>,
    pub text: String,
}

#[derive(Debug, Deserialize)]
struct WorkResponse {
    schema_version: u32,
    work: Work,
}

pub fn fetch_catalog() -> std::result::Result<Vec<WorkSummary>, String> {
    let response: CatalogResponse = get_json(CATALOG_URL, MAX_CATALOG_BYTES)?;
    if response.schema_version != 1 {
        return Err("Mouseia returned an unsupported catalog version.".into());
    }
    Ok(response.works)
}

pub fn fetch_work(slug: &str) -> std::result::Result<Work, String> {
    validate_slug(slug).map_err(|error| error.to_string())?;
    let url = format!("{SITE_URL}/api/works/{slug}");
    let response: WorkResponse = get_json(&url, MAX_WORK_BYTES)?;
    if response.schema_version != 1 || response.work.slug != slug {
        return Err("Mouseia returned an unexpected work package.".into());
    }
    Ok(response.work)
}

pub fn save_work(root: &Path, work: &Work) -> Result<PathBuf> {
    validate_slug(&work.slug)?;
    let root = root.canonicalize()?;
    let directory = root.join("Mouseia");
    if directory.exists() && fs::symlink_metadata(&directory)?.file_type().is_symlink() {
        return Err(Error::Invalid(
            "Refusing an unsafe Mouseia library folder.".into(),
        ));
    }
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{}.md", work.slug));
    if path.exists() && fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(Error::Invalid(
            "Refusing an unsafe Mouseia document path.".into(),
        ));
    }
    let mut output = AtomicWriteFile::options().open(&path)?;
    use std::io::Write as _;
    output.write_all(render_work(work).as_bytes())?;
    output.commit()?;
    Ok(path)
}

fn get_json<T: for<'de> Deserialize<'de>>(
    url: &str,
    max_bytes: u64,
) -> std::result::Result<T, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .get(url)
        .header("Accept", "application/json")
        .call()
        .map_err(|error| format!("Could not reach Mouseia: {error}"))?;
    let body = response
        .body_mut()
        .with_config()
        .limit(max_bytes)
        .read_to_string()
        .map_err(|error| format!("Could not read Mouseia’s response: {error}"))?;
    serde_json::from_str(&body).map_err(|error| format!("Mouseia returned invalid data: {error}"))
}

fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty()
        || slug.len() > 120
        || slug.starts_with('-')
        || slug.ends_with('-')
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(Error::Invalid(
            "Mouseia returned an unsafe work identifier.".into(),
        ));
    }
    Ok(())
}

fn render_work(work: &Work) -> String {
    let scalar = |value: &str| serde_json::to_string(value).expect("strings always serialize");
    let mut source = format!(
        "---\ntype: book\nstatus: Library\ntitle: {}\nauthor: {}\ntradition: {}\nlanguage: {}\ngenre: {}\nperiod: {}\nsource_url: {}\nmouseia_slug: {}\nmouseia_url: {}\n---\n\n# {}\n\n## Original\n\n{}\n",
        scalar(&work.title),
        scalar(&work.author),
        scalar(&work.tradition),
        scalar(&work.language),
        scalar(work.genre.as_deref().unwrap_or_default()),
        scalar(work.period.as_deref().unwrap_or_default()),
        scalar(work.source_url.as_deref().unwrap_or_default()),
        scalar(&work.slug),
        scalar(&format!("{SITE_URL}/texts/{}", work.slug)),
        work.title,
        work.original.text.trim(),
    );
    for translation in &work.translations {
        let label = match translation.kind.as_str() {
            "curated" => "Musean".to_owned(),
            "plain" => "Plain English".to_owned(),
            "reference" if !translation.translator.is_empty() => {
                format!("Reference · {}", translation.translator)
            }
            _ if !translation.translator.is_empty() => translation.translator.clone(),
            _ => "Translation".to_owned(),
        };
        source.push_str(&format!("\n\n## Translation — {label}\n\n"));
        if let Some(note) = translation
            .note
            .as_deref()
            .filter(|note| !note.trim().is_empty())
        {
            source.push_str("> ");
            source.push_str(&note.replace('\n', "\n> "));
            source.push_str("\n\n");
        }
        source.push_str(translation.text.trim());
    }
    source.push('\n');
    source
}

#[cfg(test)]
mod tests {
    use super::{Edition, Translation, Work, render_work, validate_slug};
    use crate::document::parse_source;

    fn sample_work() -> Work {
        Work {
            slug: "carpe-diem".into(),
            title: "Carpe Diem".into(),
            author: "Horace".into(),
            tradition: "roman".into(),
            language: "la".into(),
            genre: Some("lyric".into()),
            period: Some("1st c. BCE".into()),
            source_url: Some("https://example.com/source".into()),
            original: Edition {
                text: "Latin".into(),
            },
            translations: vec![
                Translation {
                    translator: "Mouseia".into(),
                    kind: "curated".into(),
                    note: Some("Direct from the Latin.".into()),
                    text: "Musean text".into(),
                },
                Translation {
                    translator: "Mouseia".into(),
                    kind: "plain".into(),
                    note: None,
                    text: "Plain text".into(),
                },
            ],
        }
    }

    #[test]
    fn renders_complete_work_as_reader_editions() {
        let source = render_work(&sample_work());
        let (metadata, body) = parse_source(&source);
        assert_eq!(
            metadata.get("mouseia_slug").map(String::as_str),
            Some("carpe-diem")
        );
        assert!(body.contains("## Original\n\nLatin"));
        assert!(
            body.contains("## Translation — Musean\n\n> Direct from the Latin.\n\nMusean text")
        );
        assert!(body.contains("## Translation — Plain English\n\nPlain text"));
    }

    #[test]
    fn rejects_path_like_work_identifiers() {
        assert!(validate_slug("../escape").is_err());
        assert!(validate_slug("Mixed-Case").is_err());
        assert!(validate_slug("safe-slug-2").is_ok());
    }
}

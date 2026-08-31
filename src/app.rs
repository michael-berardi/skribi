use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
};

use eframe::egui::{
    self, Align, FontId, Key, Layout, RichText, ScrollArea, Sense, TextEdit, TextStyle, Vec2,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};

use crate::{
    document::{Document, reading_chapters},
    importer::{self, ImportFormat},
    mouseia::{self, WorkSummary},
    package, theme,
    vault::Vault,
};

const LIBRARY_KEY: &str = "skribi.library";
const RETEX_KEY: &str = "skribi.retex";
const MOUSEIA_DISMISSED_KEY: &str = "skribi.mouseia.dismissed";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
enum View {
    #[default]
    Library,
    Notes,
}

#[derive(Debug)]
enum Action {
    OpenFolder,
    OpenRetexFolder,
    ImportPackage,
    ImportNotion,
    ImportFolder,
    ExportPackage,
    NewNote,
    Select(PathBuf),
    OpenReader(PathBuf),
    CloseReader,
    Save,
    Reload,
    DownloadMouseia(String),
    CloseActiveWorkspace,
}
#[derive(Debug)]
enum MouseiaMessage {
    Catalog(std::result::Result<Vec<WorkSummary>, String>),
    Work(String, Box<std::result::Result<mouseia::Work, String>>),
}

#[derive(Debug)]
struct MouseiaLibrary {
    works: Vec<WorkSummary>,
    error: Option<String>,
    loading_catalog: bool,
    downloading: Option<String>,
    dismissed: BTreeSet<String>,
    sender: Sender<MouseiaMessage>,
    receiver: Receiver<MouseiaMessage>,
}

impl Default for MouseiaLibrary {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            works: Vec::new(),
            error: None,
            loading_catalog: false,
            downloading: None,
            dismissed: BTreeSet::new(),
            sender,
            receiver,
        }
    }
}

impl MouseiaLibrary {
    fn load_catalog(&mut self) {
        if self.loading_catalog || !self.works.is_empty() {
            return;
        }
        self.loading_catalog = true;
        self.error = None;
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let _ = sender.send(MouseiaMessage::Catalog(mouseia::fetch_catalog()));
        });
    }

    fn download(&mut self, slug: String) {
        if self.downloading.is_some() {
            return;
        }
        self.downloading = Some(slug.clone());
        self.error = None;
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = Box::new(mouseia::fetch_work(&slug));
            let _ = sender.send(MouseiaMessage::Work(slug, result));
        });
    }
}
#[derive(Clone, Debug)]
struct TreeDocument {
    path: PathBuf,
    title: String,
}

#[derive(Clone, Debug, Default)]
struct VaultTree {
    folders: BTreeMap<String, VaultTree>,
    documents: Vec<TreeDocument>,
}

impl VaultTree {
    fn from_vault(vault: &Vault) -> Self {
        let mut tree = Self::default();
        for document in &vault.documents {
            let mut node = &mut tree;
            if let Some(parent) = document.relative_path.parent() {
                for component in parent.components() {
                    let name = component.as_os_str().to_string_lossy().into_owned();
                    node = node.folders.entry(name).or_default();
                }
            }
            node.documents.push(TreeDocument {
                path: document.path.clone(),
                title: document.title.clone(),
            });
        }
        tree.sort();
        tree
    }

    fn sort(&mut self) {
        self.documents
            .sort_by_cached_key(|document| document.title.to_lowercase());
        for folder in self.folders.values_mut() {
            folder.sort();
        }
    }
}
fn show_vault_tree(
    ui: &mut egui::Ui,
    tree: &VaultTree,
    selected: Option<&PathBuf>,
    action: &mut Option<Action>,
    prefix: &str,
) {
    for (name, folder) in &tree.folders {
        let id = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        egui::CollapsingHeader::new(RichText::new(name).strong())
            .id_salt(&id)
            .show(ui, |ui| show_vault_tree(ui, folder, selected, action, &id));
    }
    for document in &tree.documents {
        let label = compact_title(&document.title, 64);
        let response = ui.selectable_label(selected == Some(&document.path), label);
        if response.double_clicked() {
            *action = Some(Action::OpenReader(document.path.clone()));
        } else if response.clicked() {
            *action = Some(Action::Select(document.path.clone()));
        }
    }
}

fn compact_title(title: &str, limit: usize) -> String {
    if title.chars().count() <= limit {
        return title.to_owned();
    }
    let mut compact = title
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    compact.push('…');
    compact
}

fn numbered_paragraphs(markdown: &str) -> Option<Vec<(&str, &str)>> {
    let paragraphs = markdown
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| {
            let (number, body) = paragraph.split_once(". ")?;
            number
                .chars()
                .all(|character| character.is_ascii_digit())
                .then_some((number, body))
        })
        .collect::<Option<Vec<_>>>()?;
    (!paragraphs.is_empty()).then_some(paragraphs)
}

fn reader_markdown(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    markdown: &str,
    zoom: f32,
    secondary: egui::Color32,
    font: theme::ReaderFont,
) {
    if let Some(paragraphs) = numbered_paragraphs(markdown) {
        for (number, body) in paragraphs {
            ui.horizontal_top(|ui| {
                ui.add_sized(
                    [34.0, 24.0 * zoom],
                    egui::Label::new(
                        RichText::new(format!("{number}."))
                            .family(theme::reader_family(font))
                            .size(13.0 * zoom)
                            .color(secondary),
                    )
                    .halign(Align::RIGHT),
                );
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    CommonMarkViewer::new().show(ui, cache, body);
                });
            });
            ui.add_space(14.0 * zoom);
        }
    } else {
        CommonMarkViewer::new().show(ui, cache, markdown);
    }
}

const READER_CHUNK_BYTES: usize = 12_000;

struct ReaderBlock {
    section: usize,
    chapter: usize,
    section_title: Option<String>,
    chapter_title: Option<String>,
    body: String,
    book_header: bool,
}

impl ReaderBlock {
    fn from_document(document: &Document, selected_section: usize) -> Vec<Self> {
        let sections = document.reading_sections();
        let mut blocks = Vec::new();
        if sections.is_empty() {
            for (chunk_index, body) in reader_chunks(&document.body).into_iter().enumerate() {
                blocks.push(Self {
                    section: 0,
                    chapter: 0,
                    section_title: None,
                    chapter_title: None,
                    body,
                    book_header: chunk_index == 0,
                });
            }
            return blocks;
        }

        let section_index = selected_section.min(sections.len() - 1);
        let section = &sections[section_index];
        let chapters = reading_chapters(section.body);
        if chapters.is_empty() {
            for (chunk_index, body) in reader_chunks(section.body).into_iter().enumerate() {
                blocks.push(Self {
                    section: section_index,
                    chapter: 0,
                    section_title: (chunk_index == 0).then(|| section.title.to_owned()),
                    chapter_title: None,
                    body,
                    book_header: chunk_index == 0,
                });
            }
            return blocks;
        }

        for (chapter_index, chapter) in chapters.into_iter().enumerate() {
            for (chunk_index, body) in reader_chunks(chapter.body).into_iter().enumerate() {
                blocks.push(Self {
                    section: section_index,
                    chapter: chapter_index,
                    section_title: (chapter_index == 0 && chunk_index == 0)
                        .then(|| section.title.to_owned()),
                    chapter_title: (chunk_index == 0).then(|| chapter.title.to_owned()),
                    body,
                    book_header: blocks.is_empty(),
                });
            }
        }
        blocks
    }

    fn estimated_height(&self, zoom: f32) -> f32 {
        let characters_per_line = (72.0 / zoom).max(36.0);
        let text_height = self
            .body
            .split("\n\n")
            .map(str::trim)
            .filter(|paragraph| !paragraph.is_empty())
            .map(|paragraph| {
                let lines = (paragraph.chars().count() as f32 / characters_per_line)
                    .ceil()
                    .max(1.0);
                lines * 24.0 * zoom + 14.0 * zoom
            })
            .sum::<f32>();
        text_height
            + if self.book_header { 150.0 } else { 0.0 }
            + if self.section_title.is_some() {
                42.0
            } else {
                0.0
            }
            + if self.chapter_title.is_some() {
                76.0
            } else {
                0.0
            }
            + 120.0
    }
}

fn reader_chunks(markdown: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    for paragraph in markdown
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
    {
        if !chunk.is_empty() && chunk.len() + paragraph.len() + 2 > READER_CHUNK_BYTES {
            chunks.push(std::mem::take(&mut chunk));
        }
        if !chunk.is_empty() {
            chunk.push_str("\n\n");
        }
        chunk.push_str(paragraph);
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

#[derive(Default)]
struct ReaderOutline {
    sections: Vec<OutlineSection>,
}

struct OutlineSection {
    title: String,
    chapters: Vec<String>,
}

impl ReaderOutline {
    fn from_document(document: &Document) -> Self {
        let sections = document.reading_sections();
        if sections.is_empty() {
            return Self {
                sections: vec![OutlineSection {
                    title: String::new(),
                    chapters: reading_chapters(&document.body)
                        .into_iter()
                        .map(|chapter| chapter.title.to_owned())
                        .collect(),
                }],
            };
        }
        Self {
            sections: sections
                .into_iter()
                .map(|section| OutlineSection {
                    title: section.title.to_owned(),
                    chapters: reading_chapters(section.body)
                        .into_iter()
                        .map(|chapter| chapter.title.to_owned())
                        .collect(),
                })
                .collect(),
        }
    }

    fn has_navigation(&self) -> bool {
        self.sections
            .iter()
            .map(|section| section.chapters.len().max(1))
            .sum::<usize>()
            > 1
    }
}

pub struct SkribiApp {
    vault: Option<Vault>,
    retex_vault: Option<Vault>,
    retex_tree: VaultTree,
    view: View,
    selected: Option<PathBuf>,
    search: String,
    editing: bool,
    reader_open: bool,
    reader_sidebar_open: bool,
    editor_title: String,
    editor_status: String,
    editor_body: String,
    editor_fingerprint: String,
    dirty: bool,
    reading_section: usize,
    reading_chapter: usize,
    reader_zoom: f32,
    reader_font: theme::ReaderFont,
    reader_generation: u64,
    reader_jump: Option<(usize, usize)>,
    reader_outline: ReaderOutline,
    reader_blocks: Vec<ReaderBlock>,
    reader_block_heights: Vec<f32>,
    markdown_cache: CommonMarkCache,
    mouseia: MouseiaLibrary,
    notice: Option<(String, bool)>,
}

impl SkribiApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        context.egui_ctx.set_theme(egui::ThemePreference::System);
        theme::apply(&context.egui_ctx);
        let preferred = std::env::args_os()
            .nth(1)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("SKRIBI_LIBRARY")
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from)
            })
            .or_else(|| {
                context
                    .storage
                    .and_then(|storage| storage.get_string(LIBRARY_KEY))
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from)
            });
        let preferred_retex = std::env::var_os("SKRIBI_RETEX")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                context
                    .storage
                    .and_then(|storage| storage.get_string(RETEX_KEY))
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from)
            });
        let mut app = Self::empty();
        if let Some(dismissed) = context
            .storage
            .and_then(|storage| storage.get_string(MOUSEIA_DISMISSED_KEY))
            .and_then(|stored| serde_json::from_str(&stored).ok())
        {
            app.mouseia.dismissed = dismissed;
        }
        if let Some(path) = preferred {
            app.open_folder(path);
        }
        if let Some(path) = preferred_retex {
            app.open_retex_folder(path);
        }
        if app.vault.is_some() {
            app.view = View::Library;
            app.reader_open = false;
            let first = app
                .vault
                .as_ref()
                .and_then(|vault| vault.documents.first())
                .map(|document| document.path.clone());
            if let Some(path) = first {
                app.load_selected(path);
            }
        }
        app
    }

    fn empty() -> Self {
        Self {
            vault: None,
            retex_vault: None,
            retex_tree: VaultTree::default(),
            view: View::Library,
            selected: None,
            search: String::new(),
            editing: false,
            reader_open: false,
            reader_sidebar_open: false,
            editor_title: String::new(),
            editor_status: String::new(),
            editor_body: String::new(),
            editor_fingerprint: String::new(),
            dirty: false,
            reading_section: 0,
            reading_chapter: 0,
            reader_zoom: 1.0,
            reader_font: theme::ReaderFont::default(),
            reader_generation: 0,
            reader_jump: None,
            reader_outline: ReaderOutline::default(),
            reader_blocks: Vec::new(),
            reader_block_heights: Vec::new(),
            markdown_cache: CommonMarkCache::default(),
            mouseia: MouseiaLibrary::default(),
            notice: None,
        }
    }

    #[cfg(test)]
    fn new_for_test(
        context: &eframe::CreationContext<'_>,
        root: PathBuf,
        view: View,
        editing: bool,
    ) -> Self {
        theme::apply(&context.egui_ctx);
        let mut app = Self::empty();
        if view == View::Library {
            app.open_folder(root);
            let fixture_book = app.vault.as_ref().and_then(|vault| {
                vault
                    .documents
                    .iter()
                    .find(|document| document.title == "Meditations")
                    .map(|document| document.path.clone())
            });
            if let Some(path) = fixture_book {
                app.load_selected(path);
            }
        } else {
            app.open_retex_folder(root);
        }
        app.view = view;
        app.editing = editing;
        app.reader_open = view == View::Library;
        app.notice = None;
        app
    }

    fn open_folder(&mut self, path: PathBuf) {
        match Vault::open(path) {
            Ok(vault) => {
                let is_obsidian = vault.root.join(".obsidian").is_dir();
                let first = vault
                    .documents
                    .iter()
                    .find(|document| document.is_book())
                    .or_else(|| vault.documents.first())
                    .map(|document| document.path.clone());
                self.vault = Some(vault);
                self.view = View::Library;
                self.selected = None;
                self.reader_open = false;
                self.reader_sidebar_open = false;
                self.dirty = false;
                if let Some(path) = first {
                    self.load_selected(path);
                }
                self.notice = is_obsidian.then(|| (
                    "Obsidian vault opened directly. Markdown, folders, attachments, and wikilinks stay in place.".into(),
                    true,
                ));
                #[cfg(not(test))]
                self.mouseia.load_catalog();
            }
            Err(error) => self.notice = Some((error.to_string(), false)),
        }
    }
    fn open_retex_folder(&mut self, path: PathBuf) {
        match Vault::open(path) {
            Ok(vault) => {
                let first = vault
                    .documents
                    .first()
                    .map(|document| document.path.clone());
                self.retex_tree = VaultTree::from_vault(&vault);
                self.retex_vault = Some(vault);
                self.view = View::Notes;
                self.selected = None;
                self.reader_open = false;
                self.reader_sidebar_open = false;
                self.dirty = false;
                if let Some(path) = first {
                    self.load_selected(path);
                }
                self.notice = Some(("Retex vault opened in place.".into(), true));
            }
            Err(error) => self.notice = Some((error.to_string(), false)),
        }
    }

    fn document(&self, path: &Path) -> Option<&Document> {
        self.vault
            .as_ref()
            .and_then(|vault| vault.document(path))
            .or_else(|| {
                self.retex_vault
                    .as_ref()
                    .and_then(|vault| vault.document(path))
            })
    }

    fn document_vault_mut(&mut self, path: &Path) -> Option<&mut Vault> {
        if self
            .vault
            .as_ref()
            .is_some_and(|vault| vault.document(path).is_some())
        {
            self.vault.as_mut()
        } else {
            self.retex_vault.as_mut()
        }
    }

    fn active_vault(&self) -> Option<&Vault> {
        match self.view {
            View::Library => self.vault.as_ref(),
            View::Notes => self.retex_vault.as_ref(),
        }
    }

    fn active_vault_mut(&mut self) -> Option<&mut Vault> {
        match self.view {
            View::Library => self.vault.as_mut(),
            View::Notes => self.retex_vault.as_mut(),
        }
    }
    fn poll_mouseia(&mut self) {
        while let Ok(message) = self.mouseia.receiver.try_recv() {
            match message {
                MouseiaMessage::Catalog(result) => {
                    self.mouseia.loading_catalog = false;
                    match result {
                        Ok(works) => {
                            self.mouseia.works = works;
                            self.mouseia.error = None;
                        }

                        Err(error) => self.mouseia.error = Some(error),
                    }
                }
                MouseiaMessage::Work(slug, result) => {
                    if self.mouseia.downloading.as_deref() == Some(&slug) {
                        self.mouseia.downloading = None;
                    }
                    match *result {
                        Ok(work) => {
                            let imported: crate::Result<PathBuf> = (|| {
                                let vault = self.vault.as_mut().ok_or_else(|| {
                                    crate::Error::Invalid(
                                        "Open a local library before adding a Mouseia work.".into(),
                                    )
                                })?;
                                let path = mouseia::save_work(&vault.root, &work)?;
                                vault.reload()?;
                                Ok(path)
                            })();
                            match imported {
                                Ok(path) => {
                                    self.load_selected(path);
                                    self.editing = false;
                                    self.reader_open = true;
                                    self.reader_sidebar_open = false;
                                    self.notice = Some((
                                        "Added the complete Mouseia work to this library.".into(),
                                        true,
                                    ));
                                }
                                Err(error) => {
                                    self.mouseia.error = Some(error.to_string());
                                    self.notice = Some((error.to_string(), false));
                                }
                            }
                        }
                        Err(error) => {
                            self.mouseia.error = Some(error.clone());
                            self.notice = Some((error, false));
                        }
                    }
                }
            }
        }
    }
    fn refresh_retex_tree(&mut self) {
        self.retex_tree = self
            .retex_vault
            .as_ref()
            .map(VaultTree::from_vault)
            .unwrap_or_default();
    }

    fn load_selected(&mut self, path: PathBuf) {
        if self.dirty && self.save_current().is_err() {
            return;
        }
        let document = self.document(&path).cloned();
        if let Some(document) = document {
            self.selected = Some(path);
            self.reader_generation = self.reader_generation.wrapping_add(1);
            self.reader_outline = ReaderOutline::from_document(&document);
            self.reader_blocks = ReaderBlock::from_document(&document, 0);
            self.reader_block_heights = self
                .reader_blocks
                .iter()
                .map(|block| block.estimated_height(self.reader_zoom))
                .collect();
            self.editor_status = document.metadata_value("status").to_owned();
            self.editor_title = document.title;
            self.editor_body = document.body;
            self.editor_fingerprint = document.fingerprint;
            self.reading_section = 0;
            self.reading_chapter = 0;
            self.reader_jump = None;
            self.dirty = false;
            self.markdown_cache = CommonMarkCache::default();
        }
    }
    fn select_translation(&mut self, section: usize) {
        let section = section.min(self.reader_outline.sections.len().saturating_sub(1));
        let blocks = self
            .selected
            .as_ref()
            .and_then(|path| self.document(path))
            .map(|document| ReaderBlock::from_document(document, section));
        if let Some(blocks) = blocks {
            self.reading_section = section;
            self.reading_chapter = 0;
            self.reader_blocks = blocks;
            self.reader_block_heights = self
                .reader_blocks
                .iter()
                .map(|block| block.estimated_height(self.reader_zoom))
                .collect();
            self.reader_jump = None;
            self.reader_generation = self.reader_generation.wrapping_add(1);
            self.markdown_cache = CommonMarkCache::default();
        }
    }

    fn save_current(&mut self) -> crate::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let Some(path) = self.selected.clone() else {
            return Ok(());
        };
        let fingerprint = self.editor_fingerprint.clone();
        let title = self.editor_title.clone();
        let status = self.editor_status.clone();
        let body = self.editor_body.clone();
        let Some(vault) = self.document_vault_mut(&path) else {
            return Ok(());
        };
        match vault.save_document(&path, &fingerprint, &title, &status, &body) {
            Ok(document) => {
                self.reader_outline = ReaderOutline::from_document(&document);
                self.reader_blocks = ReaderBlock::from_document(&document, self.reading_section);
                self.reader_block_heights = self
                    .reader_blocks
                    .iter()
                    .map(|block| block.estimated_height(self.reader_zoom))
                    .collect();
                self.markdown_cache = CommonMarkCache::default();
                self.editor_status = document.metadata_value("status").to_owned();
                self.editor_fingerprint = document.fingerprint;
                self.editor_title = document.title;
                self.refresh_retex_tree();
                self.dirty = false;
                self.notice = Some(("Saved locally.".into(), true));
                Ok(())
            }
            Err(error) => {
                self.notice = Some((error.to_string(), false));
                Err(error)
            }
        }
    }

    fn perform(&mut self, action: Action) {
        match action {
            Action::OpenFolder => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Open Markdown, Retex, or Obsidian folder")
                    .pick_folder()
                {
                    self.open_folder(path);
                }
            }
            Action::OpenRetexFolder => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Open a Retex, Obsidian, or Markdown vault")
                    .pick_folder()
                {
                    self.open_retex_folder(path);
                }
            }
            Action::ImportPackage => {
                let source = rfd::FileDialog::new()
                    .set_title("Choose a Skribi library")
                    .add_filter("Skribi library", &["skribi"])
                    .pick_file();
                let destination = source.as_ref().and_then(|_| {
                    rfd::FileDialog::new()
                        .set_title("Choose a new or empty destination folder")
                        .pick_folder()
                });
                if let (Some(source), Some(destination)) = (source, destination) {
                    match package::import_library(&source, &destination) {
                        Ok(manifest) => {
                            self.open_folder(destination);
                            self.notice = Some((format!("Imported “{}”.", manifest.name), true));
                        }
                        Err(error) => self.notice = Some((error.to_string(), false)),
                    }
                }
            }
            Action::ImportNotion => {
                let source = rfd::FileDialog::new()
                    .set_title("Choose a Notion Markdown & CSV ZIP export")
                    .add_filter("Notion export", &["zip"])
                    .pick_file();
                let destination = source.as_ref().and_then(|_| {
                    rfd::FileDialog::new()
                        .set_title("Choose a new or empty destination folder")
                        .pick_folder()
                });
                if let (Some(source), Some(destination)) = (source, destination) {
                    match importer::import_source(&source, &destination, ImportFormat::Notion) {
                        Ok(result) => {
                            self.open_retex_folder(destination);
                            self.notice = Some((
                                format!(
                                    "Imported {} notes and {} assets from Notion; converted {} CSV tables.",
                                    result.notes, result.assets, result.converted_tables
                                ),
                                true,
                            ));
                        }
                        Err(error) => self.notice = Some((error.to_string(), false)),
                    }
                }
            }
            Action::ImportFolder => {
                let source = rfd::FileDialog::new()
                    .set_title("Choose an Obsidian or Markdown folder")
                    .pick_folder();
                let destination = source.as_ref().and_then(|_| {
                    rfd::FileDialog::new()
                        .set_title("Choose a new or empty destination folder")
                        .pick_folder()
                });
                if let (Some(source), Some(destination)) = (source, destination) {
                    match importer::import_source(&source, &destination, ImportFormat::Auto) {
                        Ok(result) => {
                            self.open_retex_folder(destination);
                            self.notice = Some((
                                format!(
                                    "Imported {} notes and {} assets.",
                                    result.notes, result.assets
                                ),
                                true,
                            ));
                        }
                        Err(error) => self.notice = Some((error.to_string(), false)),
                    }
                }
            }
            Action::ExportPackage => {
                let Some(vault) = self.vault.as_ref() else {
                    return;
                };
                let suggested = vault
                    .root
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Library");
                let output = rfd::FileDialog::new()
                    .set_title("Export portable Skribi library")
                    .set_file_name(format!("{suggested}.skribi"))
                    .add_filter("Skribi library", &["skribi"])
                    .save_file();
                if let Some(output) = output {
                    match package::export_library(
                        &vault.root,
                        &output,
                        suggested,
                        "Portable Markdown library",
                    ) {
                        Ok(_) => {
                            self.notice = Some((format!("Exported {}.", output.display()), true))
                        }
                        Err(error) => self.notice = Some((error.to_string(), false)),
                    }
                }
            }
            Action::NewNote => {
                if self.save_current().is_err() {
                    return;
                }
                if let Some(vault) = self.retex_vault.as_mut() {
                    match vault.create_note() {
                        Ok(document) => {
                            self.refresh_retex_tree();
                            self.view = View::Notes;
                            self.load_selected(document.path);
                            self.editing = true;
                            self.reader_open = true;
                            self.reader_sidebar_open = false;
                        }
                        Err(error) => self.notice = Some((error.to_string(), false)),
                    }
                }
            }
            Action::Select(path) => self.load_selected(path),
            Action::OpenReader(path) => {
                self.load_selected(path);
                self.editing = false;
                self.reader_open = true;
                self.reader_sidebar_open = false;
            }
            Action::CloseReader => {
                if self.dirty && self.save_current().is_err() {
                    return;
                }
                self.editing = false;
                self.reader_open = false;
                self.reader_sidebar_open = false;
            }
            Action::Save => {
                let _ = self.save_current();
            }
            Action::CloseActiveWorkspace => {
                if self.dirty && self.save_current().is_err() {
                    return;
                }
                match self.view {
                    View::Library => self.vault = None,
                    View::Notes => {
                        self.retex_vault = None;
                        self.retex_tree = VaultTree::default();
                    }
                }
                self.selected = None;
                self.reader_open = false;
                self.reader_sidebar_open = false;
                self.editing = false;
                self.dirty = false;
            }
            Action::DownloadMouseia(slug) => self.mouseia.download(slug),
            Action::Reload => {
                if let Some(vault) = self.active_vault_mut() {
                    match vault.reload() {
                        Ok(()) => {
                            self.refresh_retex_tree();
                            if let Some(path) = self.selected.clone() {
                                self.dirty = false;
                                self.load_selected(path);
                            }
                            self.notice = Some(("Reloaded from disk.".into(), true));
                        }
                        Err(error) => self.notice = Some((error.to_string(), false)),
                    }
                }
            }
        }
    }

    fn collection_bar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let mut next_view = None;
        let colors = theme::palette(ui);
        egui::Panel::top("collection-bar")
            .exact_size(48.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.raised)
                    .inner_margin(egui::Margin::symmetric(14, 7))
                    .stroke(egui::Stroke::new(1.0, colors.hairline)),
            )
            .show(ui, |ui| {
                ui.columns(3, |columns| {
                    columns[0].horizontal(|ui| {
                        theme::brand_mark(ui, 26.0);
                        ui.label(RichText::new("Skribi").size(18.0).strong());
                        if let Some((message, success)) = self.notice.as_ref() {
                            ui.label(
                                RichText::new(if *success { "Saved" } else { "Issue" })
                                    .size(11.0)
                                    .color(if *success { colors.green } else { colors.red }),
                            )
                            .on_hover_text(message);
                        }
                    });

                    columns[1].with_layout(
                        Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.horizontal(|ui| {
                                for (view, label) in
                                    [(View::Library, "Library"), (View::Notes, "Retex")]
                                {
                                    if ui.selectable_label(self.view == view, label).clicked() {
                                        next_view = Some(view);
                                    }
                                }
                            });
                        },
                    );

                    columns[2].with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.menu_button("More", |ui| {
                            let (open_label, open_action) = if self.view == View::Library {
                                ("Open library…", Action::OpenFolder)
                            } else {
                                ("Open Retex vault…", Action::OpenRetexFolder)
                            };
                            if ui.button(open_label).clicked() {
                                action = Some(open_action);
                                ui.close();
                            }
                            if self.view == View::Library
                                && self.vault.is_some()
                                && ui.button("Export library…").clicked()
                            {
                                action = Some(Action::ExportPackage);
                                ui.close();
                            }
                            if self.active_vault().is_some()
                                && ui.button("Reload from disk").clicked()
                            {
                                action = Some(Action::Reload);
                                ui.close();
                            }
                            if self.active_vault().is_some() {
                                let close_label = if self.view == View::Library {
                                    "Close library"
                                } else {
                                    "Unload Retex vault"
                                };
                                if ui.button(close_label).clicked() {
                                    action = Some(Action::CloseActiveWorkspace);
                                    ui.close();
                                }
                            }
                        });
                        if self.view == View::Notes
                            && self.retex_vault.is_some()
                            && ui.button("New note").clicked()
                        {
                            action = Some(Action::NewNote);
                        }
                        ui.menu_button("Import", |ui| {
                            if self.view == View::Library {
                                if ui.button("Skribi library…").clicked() {
                                    action = Some(Action::ImportPackage);
                                    ui.close();
                                }
                            } else {
                                if ui.button("Notion ZIP…").clicked() {
                                    action = Some(Action::ImportNotion);
                                    ui.close();
                                }
                                if ui.button("Obsidian or Markdown folder…").clicked() {
                                    action = Some(Action::ImportFolder);
                                    ui.close();
                                }
                            }
                        });
                    });
                });
            });
        if let Some(view) = next_view {
            if self.dirty && self.save_current().is_err() {
                return action;
            }
            self.view = view;
            self.editing = false;
            self.reader_open = false;
            self.reader_sidebar_open = false;
            self.search.clear();
            self.selected = None;
            let first = self
                .active_vault()
                .and_then(|vault| vault.documents.first())
                .map(|document| document.path.clone());
            if let Some(path) = first {
                self.load_selected(path);
            }
        }
        action
    }

    fn reader_bar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let colors = theme::palette(ui);
        let title = self
            .selected
            .as_ref()
            .and_then(|path| self.document(path))
            .map(|document| document.title.clone())
            .unwrap_or_default();
        let back_label = match self.view {
            View::Library => "Library",
            View::Notes => "Retex",
        };

        egui::Panel::top("reader-bar")
            .exact_size(48.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.paper)
                    .inner_margin(egui::Margin::symmetric(14, 7))
                    .stroke(egui::Stroke::new(1.0, colors.hairline)),
            )
            .show(ui, |ui| {
                ui.columns(3, |columns| {
                    columns[0].horizontal(|ui| {
                        if ui
                            .button(back_label)
                            .on_hover_text("Back to collection")
                            .clicked()
                        {
                            action = Some(Action::CloseReader);
                        }
                        if !self.editing
                            && self.reader_outline.has_navigation()
                            && ui
                                .selectable_label(self.reader_sidebar_open, "Contents")
                                .clicked()
                        {
                            self.reader_sidebar_open = !self.reader_sidebar_open;
                        }
                    });
                    columns[1].vertical_centered(|ui| {
                        ui.label(RichText::new(title).size(13.0).strong());
                    });
                    columns[2].with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.editing && self.dirty && ui.button("Save").clicked() {
                            action = Some(Action::Save);
                        }
                        if !self.editing {
                            ui.menu_button("Aa", |ui| {
                                ui.set_min_width(220.0);
                                ui.label(RichText::new("Reading size").strong());
                                let zoom_changed = ui
                                    .add(
                                        egui::Slider::new(&mut self.reader_zoom, 0.8..=1.6)
                                            .show_value(false),
                                    )
                                    .changed();
                                ui.label(
                                    RichText::new(format!(
                                        "{}%",
                                        (self.reader_zoom * 100.0).round()
                                    ))
                                    .size(11.0)
                                    .color(colors.secondary),
                                );
                                ui.separator();
                                ui.label(RichText::new("Typeface").strong());
                                let previous_font = self.reader_font;
                                egui::ComboBox::from_id_salt("reader-font")
                                    .selected_text(self.reader_font.label())
                                    .width(ui.available_width())
                                    .show_ui(ui, |ui| {
                                        for font in theme::READER_FONTS {
                                            ui.selectable_value(
                                                &mut self.reader_font,
                                                font,
                                                font.label(),
                                            );
                                        }
                                    });
                                if zoom_changed || self.reader_font != previous_font {
                                    self.reader_block_heights = self
                                        .reader_blocks
                                        .iter()
                                        .map(|block| block.estimated_height(self.reader_zoom))
                                        .collect();
                                    self.reader_generation = self.reader_generation.wrapping_add(1);
                                    self.markdown_cache = CommonMarkCache::default();
                                }
                            });
                        }
                        ui.selectable_value(&mut self.editing, true, "Edit");
                        ui.selectable_value(&mut self.editing, false, "Read");
                    });
                });
            });
        action
    }

    fn reader_sidebar(&mut self, ui: &mut egui::Ui) {
        if !self.reader_sidebar_open || self.editing {
            return;
        }
        let colors = theme::palette(ui);
        let metadata = self
            .selected
            .as_ref()
            .and_then(|path| self.document(path))
            .map(|document| {
                (
                    document.title.clone(),
                    document.metadata_value("author").to_owned(),
                )
            })
            .unwrap_or_default();
        let mut selected_translation = self.reading_section;
        let mut selected_chapter = None;
        egui::Panel::left("reader-contents")
            .exact_size(260.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.sidebar)
                    .inner_margin(egui::Margin::symmetric(16, 18))
                    .stroke(egui::Stroke::new(1.0, colors.hairline)),
            )
            .show(ui, |ui| {
                ui.label(RichText::new(metadata.0).size(16.0).strong());
                if !metadata.1.is_empty() {
                    ui.label(RichText::new(metadata.1).size(12.0).color(colors.secondary));
                }
                if self.reader_outline.sections.len() > 1 {
                    ui.add_space(18.0);
                    ui.label(
                        RichText::new("TRANSLATION")
                            .size(10.5)
                            .strong()
                            .color(colors.secondary),
                    );
                    let selected_label = self
                        .reader_outline
                        .sections
                        .get(selected_translation)
                        .map(|section| section.title.as_str())
                        .filter(|title| !title.is_empty())
                        .unwrap_or("Full text");
                    egui::ComboBox::from_id_salt("reader-translation")
                        .selected_text(selected_label)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (index, section) in self.reader_outline.sections.iter().enumerate()
                            {
                                let label = if section.title.is_empty() {
                                    "Full text"
                                } else {
                                    &section.title
                                };
                                ui.selectable_value(
                                    &mut selected_translation,
                                    index,
                                    RichText::new(label).strong(),
                                );
                            }
                        });
                }
                ui.add_space(20.0);
                ui.label(
                    RichText::new("CONTENTS")
                        .size(10.5)
                        .strong()
                        .color(colors.secondary),
                );
                ui.add_space(6.0);
                ScrollArea::vertical().show(ui, |ui| {
                    if let Some(section) = self.reader_outline.sections.get(selected_translation) {
                        for (chapter_index, chapter) in section.chapters.iter().enumerate() {
                            if ui
                                .selectable_label(
                                    self.reading_section == selected_translation
                                        && self.reading_chapter == chapter_index,
                                    chapter,
                                )
                                .clicked()
                            {
                                selected_chapter = Some(chapter_index);
                            }
                        }
                    }
                });
            });
        if selected_translation != self.reading_section {
            self.select_translation(selected_translation);
        } else if let Some(chapter) = selected_chapter {
            self.reading_chapter = chapter;
            self.reader_jump = Some((self.reading_section, chapter));
        }
    }

    fn document_list(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        if self.view != View::Notes || self.reader_open || self.retex_vault.is_none() {
            return None;
        }
        let mut action = None;
        let colors = theme::palette(ui);
        let vault_name = self
            .retex_vault
            .as_ref()
            .and_then(|vault| vault.root.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Retex vault")
            .to_owned();
        egui::Panel::left("document-list")
            .default_size(300.0)
            .size_range(260.0..=360.0)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(colors.sidebar)
                    .inner_margin(egui::Margin::symmetric(12, 14))
                    .stroke(egui::Stroke::new(1.0, colors.hairline)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&vault_name).size(18.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let count =
                            filtered_documents(self.retex_vault.as_ref(), View::Notes, "").count();
                        ui.label(RichText::new(count.to_string()).color(colors.secondary));
                    });
                });
                ui.add_space(8.0);
                ui.add(
                    TextEdit::singleline(&mut self.search)
                        .hint_text("Search vault")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);
                ScrollArea::vertical()
                    .id_salt(("retex-tree", std::process::id()))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.search.trim().is_empty() {
                            show_vault_tree(
                                ui,
                                &self.retex_tree,
                                self.selected.as_ref(),
                                &mut action,
                                "",
                            );
                        } else {
                            for document in filtered_documents(
                                self.retex_vault.as_ref(),
                                self.view,
                                &self.search,
                            ) {
                                let response = ui.selectable_label(
                                    self.selected.as_ref() == Some(&document.path),
                                    compact_title(&document.title, 64),
                                );
                                if response.double_clicked() {
                                    action = Some(Action::OpenReader(document.path.clone()));
                                } else if response.clicked() {
                                    action = Some(Action::Select(document.path.clone()));
                                }
                            }
                        }
                    });
            });
        action
    }

    fn central(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let colors = theme::palette(ui);
        let margin = if self.reader_open { 0 } else { 24 };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(colors.paper)
                    .inner_margin(egui::Margin::same(margin)),
            )
            .show(ui, |ui| {
                if self.active_vault().is_none() {
                    action = self.welcome(ui);
                } else if self.reader_open {
                    if self.editing {
                        self.editor(ui);
                    } else {
                        self.reader(ui);
                    }
                } else {
                    match self.view {
                        View::Library => action = self.library_shelf(ui),
                        View::Notes => action = self.note_preview(ui),
                    }
                }
            });
        action
    }

    fn welcome(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let colors = theme::palette(ui);
        let (title, subtitle, button, hint, open_action) = if self.view == View::Library {
            (
                "A library for complete works.",
                "Keep local books separate from working notes and research.",
                "Open library folder",
                "Mouseia works can be added here with every available translation.",
                Action::OpenFolder,
            )
        } else {
            (
                "Your Retex vault, in plain sight.",
                "Read, write, search, and organize Markdown without leaving the file system.",
                "Open Retex vault",
                "Retex, Obsidian, and ordinary Markdown folders open in place.",
                Action::OpenRetexFolder,
            )
        };
        ui.vertical_centered(|ui| {
            ui.add_space(118.0);
            theme::brand_mark(ui, 68.0);
            ui.add_space(22.0);
            ui.label(RichText::new(title).size(34.0).strong());
            ui.add_space(8.0);
            ui.label(RichText::new(subtitle).size(16.0).color(colors.secondary));
            ui.add_space(28.0);
            if ui.button(button).clicked() {
                action = Some(open_action);
            }
            ui.add_space(12.0);
            ui.label(RichText::new(hint).size(13.0).color(colors.secondary));
        });
        action
    }

    fn library_shelf(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let colors = theme::palette(ui);
        let books: Vec<_> = filtered_documents(self.vault.as_ref(), View::Library, &self.search)
            .cloned()
            .collect();
        let books_count = filtered_documents(self.vault.as_ref(), View::Library, "").count();
        let count_label = if books_count == 1 {
            "1 book".to_owned()
        } else {
            format!("{books_count} books")
        };
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Library").size(30.0).strong());
                ui.label(
                    RichText::new(count_label)
                        .size(13.0)
                        .color(colors.secondary),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.search)
                        .hint_text("Search library")
                        .desired_width(260.0),
                );
            });
        });
        ui.add_space(24.0);

        ScrollArea::vertical()
            .id_salt(("library-shelf", std::process::id()))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if !books.is_empty() {
                    ui.label(
                        RichText::new("YOUR BOOKS")
                            .size(10.5)
                            .strong()
                            .color(colors.secondary),
                    );
                    ui.add_space(10.0);
                    ui.spacing_mut().item_spacing = Vec2::new(22.0, 24.0);
                    ui.horizontal_wrapped(|ui| {
                        for document in &books {
                            let response = ui
                                .allocate_ui(Vec2::new(178.0, 250.0), |ui| {
                                    egui::Frame::new()
                                        .fill(colors.raised)
                                        .stroke(egui::Stroke::new(1.0, colors.hairline))
                                        .corner_radius(egui::CornerRadius::same(6))
                                        .inner_margin(egui::Margin::same(18))
                                        .show(ui, |ui| {
                                            ui.set_min_size(Vec2::new(142.0, 166.0));
                                            ui.vertical_centered(|ui| {
                                                let initial = document
                                                    .title
                                                    .chars()
                                                    .next()
                                                    .unwrap_or('S')
                                                    .to_string();
                                                ui.label(
                                                    RichText::new(initial)
                                                        .size(38.0)
                                                        .strong()
                                                        .color(colors.ochre),
                                                );
                                                ui.add_space(12.0);
                                                let tradition =
                                                    document.metadata_value("tradition");
                                                if !tradition.is_empty() {
                                                    ui.label(
                                                        RichText::new(
                                                            tradition.to_ascii_uppercase(),
                                                        )
                                                        .size(10.5)
                                                        .color(colors.ochre),
                                                    );
                                                    ui.add_space(5.0);
                                                }
                                                ui.label(
                                                    RichText::new(&document.title)
                                                        .size(17.0)
                                                        .strong(),
                                                );
                                            });
                                        });
                                    ui.add_space(8.0);
                                    let author = document.metadata_value("author");
                                    if !author.is_empty() {
                                        ui.label(
                                            RichText::new(author)
                                                .size(13.0)
                                                .color(colors.secondary),
                                        );
                                    }
                                })
                                .response
                                .interact(Sense::click());
                            if response.clicked() {
                                action = Some(Action::OpenReader(document.path.clone()));
                            }
                        }
                    });
                    ui.add_space(26.0);
                }
                if action.is_none() {
                    action = self.mouseia_shelf(ui);
                }
            });
        action
    }

    fn mouseia_shelf(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let colors = theme::palette(ui);
        let mut action = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Mouseia").size(20.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.hyperlink_to("mouseia.org ↗", mouseia::SITE_URL);
            });
        });
        ui.add_space(10.0);

        if self.mouseia.loading_catalog {
            ui.spinner();
            return None;
        }
        if self.mouseia.works.is_empty() {
            if let Some(error) = &self.mouseia.error {
                ui.label(RichText::new(error).size(13.0).color(colors.red));
            }
            if ui.button("Retry Mouseia").clicked() {
                self.mouseia.load_catalog();
            }
            return None;
        }

        let query = self.search.trim();
        let works: Vec<_> = self
            .mouseia
            .works
            .iter()
            .filter(|work| !self.mouseia.dismissed.contains(&work.slug))
            .filter(|work| {
                query.is_empty()
                    || contains_ascii_case_insensitive(&work.title, query)
                    || contains_ascii_case_insensitive(&work.author, query)
                    || contains_ascii_case_insensitive(&work.tradition, query)
                    || work
                        .genre
                        .as_deref()
                        .is_some_and(|genre| contains_ascii_case_insensitive(genre, query))
            })
            .cloned()
            .collect();
        if works.is_empty() {
            if query.is_empty()
                && !self.mouseia.dismissed.is_empty()
                && ui.small_button("Show hidden").clicked()
            {
                self.mouseia.dismissed.clear();
            }
            return None;
        }

        let rows: [(&str, &[&str]); 3] = [
            ("Ancient worlds", &["egyptian", "greek", "roman"]),
            (
                "Wisdom traditions",
                &["arabic", "chinese", "hindu", "persian", "alchemy"],
            ),
            ("American foundations", &["american"]),
        ];
        for (row_index, (title, traditions)) in rows.into_iter().enumerate() {
            let row: Vec<_> = works
                .iter()
                .filter(|work| traditions.contains(&work.tradition.as_str()))
                .cloned()
                .collect();
            if row.is_empty() {
                continue;
            }
            ui.label(
                RichText::new(title.to_ascii_uppercase())
                    .size(10.0)
                    .strong()
                    .color(colors.secondary),
            );
            ui.add_space(5.0);
            if action.is_none() {
                action = self.mouseia_row(ui, row_index, &row);
            } else {
                let _ = self.mouseia_row(ui, row_index, &row);
            }
            ui.add_space(14.0);
        }
        action
    }

    fn mouseia_row(
        &mut self,
        ui: &mut egui::Ui,
        row_index: usize,
        works: &[WorkSummary],
    ) -> Option<Action> {
        let mut action = None;
        let mut dismissed = None;
        ScrollArea::horizontal()
            .id_salt(("mouseia-library", row_index))
            .auto_shrink([false, false])
            .max_height(132.0)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.horizontal_top(|ui| {
                    for work in works {
                        let installed = self.vault.as_ref().and_then(|vault| {
                            vault
                                .documents
                                .iter()
                                .find(|document| {
                                    document.metadata_value("mouseia_slug") == work.slug
                                })
                                .map(|document| document.path.clone())
                        });
                        let downloading =
                            self.mouseia.downloading.as_deref() == Some(work.slug.as_str());
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(226.0, 126.0), Sense::click());
                        let response = response.on_hover_text(if installed.is_some() {
                            "Read"
                        } else {
                            "Add the complete work and read"
                        });
                        let fill = mouseia_tile_color(&work.tradition, response.hovered());
                        ui.painter().rect(
                            rect,
                            egui::CornerRadius::same(7),
                            fill,
                            egui::Stroke::new(1.0, fill.gamma_multiply(1.3)),
                            egui::StrokeKind::Inside,
                        );
                        ui.scope_builder(
                            egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(14.0, 12.0))),
                            |ui| {
                                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                                    ui.set_max_width(176.0);
                                    if response.hovered() {
                                        ui.label(
                                            RichText::new(work.tradition.to_ascii_uppercase())
                                                .size(9.5)
                                                .strong()
                                                .color(egui::Color32::WHITE.gamma_multiply(0.72)),
                                        );
                                        ui.add_space(12.0);
                                    } else {
                                        ui.add_space(28.0);
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&work.title)
                                                .size(16.0)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .wrap(),
                                    );
                                    if response.hovered() {
                                        ui.add_space(5.0);
                                        ui.label(
                                            RichText::new(if downloading {
                                                "Adding…"
                                            } else if installed.is_some() {
                                                "Read"
                                            } else {
                                                "Add"
                                            })
                                            .size(11.0)
                                            .strong()
                                            .color(egui::Color32::WHITE.gamma_multiply(0.86)),
                                        );
                                    }
                                });
                            },
                        );
                        if response.hovered() {
                            let close_rect = egui::Rect::from_min_size(
                                rect.right_top() + egui::vec2(-30.0, 6.0),
                                egui::vec2(24.0, 24.0),
                            );
                            if ui
                                .put(close_rect, egui::Button::new("×").frame(true))
                                .on_hover_text("Hide this recommendation")
                                .clicked()
                            {
                                dismissed = Some(work.slug.clone());
                            }
                        }
                        if response.clicked() && dismissed.is_none() && !downloading {
                            action = installed
                                .map(Action::OpenReader)
                                .or_else(|| Some(Action::DownloadMouseia(work.slug.clone())));
                        }
                    }
                });
            });
        if let Some(slug) = dismissed {
            self.mouseia.dismissed.insert(slug);
        }
        action
    }

    fn note_preview(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let path = self.selected.clone()?;
        let colors = theme::palette(ui);
        let relative_path = self
            .document(&path)
            .map(|document| document.relative_path.display().to_string())
            .unwrap_or_default();
        let mut action = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(relative_path)
                    .size(11.0)
                    .color(colors.secondary),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Edit").clicked() {
                    self.reader_open = true;
                    self.editing = true;
                }
                if ui.button("Open reader").clicked() {
                    action = Some(Action::OpenReader(path.clone()));
                }
            });
        });
        ui.add_space(8.0);
        ui.separator();
        self.reader(ui);
        action
    }

    fn reader(&mut self, ui: &mut egui::Ui) {
        let colors = theme::palette(ui);
        let Some(path) = self.selected.as_ref() else {
            ui.centered_and_justified(|ui| {
                ui.label("Choose a document.");
            });
            return;
        };
        let Some((relative_path, title, author, period)) = self.document(path).map(|document| {
            (
                document.relative_path.clone(),
                document.title.clone(),
                document.metadata_value("author").to_owned(),
                document.metadata_value("period").to_owned(),
            )
        }) else {
            return;
        };
        let jump = self.reader_jump.take();
        let target_offset = jump.and_then(|location| {
            self.reader_blocks
                .iter()
                .position(|block| (block.section, block.chapter) == location)
                .map(|index| self.reader_block_heights[..index].iter().sum())
        });
        let reader_id = (
            "reader",
            &relative_path,
            self.reader_open,
            self.reader_generation,
            std::process::id(),
        );
        let reader_height = (ui.available_height() - 8.0).max(200.0);
        let reader_family = theme::reader_family(self.reader_font);
        let byline = match (author.is_empty(), period.is_empty()) {
            (false, false) => format!("{author}  ·  {period}"),
            (false, true) => author,
            (true, false) => period,
            (true, true) => String::new(),
        };
        let blocks = &self.reader_blocks;
        let mut layout_heights = self.reader_block_heights.clone();
        let total_height = layout_heights.iter().sum::<f32>().max(reader_height);
        let mut scroll_area = ScrollArea::vertical()
            .id_salt(reader_id)
            .auto_shrink([false, false])
            .max_height(reader_height)
            .min_scrolled_height(reader_height);
        if let Some(offset) = target_offset {
            scroll_area = scroll_area.vertical_scroll_offset(offset);
        }

        scroll_area.show_viewport(ui, |ui, viewport| {
            ui.set_height(total_height);
            let origin = ui.max_rect().left_top();
            let width = ui.available_width();
            let buffered_viewport = viewport.expand(900.0);
            let mut block_top = 0.0;
            for (index, block) in blocks.iter().enumerate() {
                let height = layout_heights[index];
                let relative_rect = egui::Rect::from_min_size(
                    egui::pos2(0.0, block_top),
                    egui::vec2(width, height),
                );
                if relative_rect.intersects(buffered_viewport) {
                    let block_rect = egui::Rect::from_min_size(
                        origin + egui::vec2(0.0, block_top),
                        egui::vec2(width, height),
                    );
                    let rendered =
                        ui.scope_builder(egui::UiBuilder::new().max_rect(block_rect), |ui| {
                            ui.vertical_centered(|ui| {
                                ui.set_max_width(680.0);
                                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                                    if block.book_header {
                                        ui.add_space(if self.reader_open { 52.0 } else { 24.0 });
                                        ui.label(
                                            RichText::new(&title)
                                                .family(reader_family.clone())
                                                .size(40.0)
                                                .strong(),
                                        );
                                        if !byline.is_empty() {
                                            ui.add_space(5.0);
                                            ui.label(
                                                RichText::new(&byline)
                                                    .family(reader_family.clone())
                                                    .size(14.5)
                                                    .color(colors.secondary),
                                            );
                                        }
                                        ui.add_space(48.0);
                                    }
                                    ui.scope(|ui| {
                                        let mut style = (**ui.style()).clone();
                                        for (text_style, font) in &mut style.text_styles {
                                            font.size *= self.reader_zoom;
                                            if *text_style != TextStyle::Monospace {
                                                font.family = reader_family.clone();
                                            }
                                        }
                                        if let Some(body) =
                                            style.text_styles.get_mut(&TextStyle::Body)
                                        {
                                            body.size = 18.0 * self.reader_zoom;
                                        }
                                        style.spacing.item_spacing.y = 8.0;
                                        ui.set_style(style);
                                        if let Some(section_title) = &block.section_title
                                            && !section_title.is_empty()
                                        {
                                            ui.label(
                                                RichText::new(section_title.to_ascii_uppercase())
                                                    .size(11.5 * self.reader_zoom)
                                                    .strong()
                                                    .color(colors.ochre),
                                            );
                                            ui.add_space(12.0);
                                        }
                                        if let Some(chapter_title) = &block.chapter_title {
                                            ui.label(
                                                RichText::new(chapter_title)
                                                    .family(reader_family.clone())
                                                    .size(29.0 * self.reader_zoom)
                                                    .strong(),
                                            );
                                            ui.add_space(20.0);
                                        }
                                        reader_markdown(
                                            ui,
                                            &mut self.markdown_cache,
                                            &block.body,
                                            self.reader_zoom,
                                            colors.secondary,
                                            self.reader_font,
                                        );
                                    });
                                });
                            })
                            .response
                            .rect
                            .height()
                        });
                    let measured_height = rendered.inner + 64.0;
                    if measured_height > height {
                        layout_heights[index] = measured_height;
                    }
                }
                block_top += layout_heights[index];
            }
            ui.set_height(block_top.max(reader_height));
        });
        if self.reader_block_heights != layout_heights {
            self.reader_block_heights = layout_heights;
            ui.ctx().request_repaint();
        }
    }
    fn editor(&mut self, ui: &mut egui::Ui) {
        let colors = theme::palette(ui);
        if self.selected.is_none() {
            return;
        }
        ui.vertical_centered(|ui| {
            ui.add_space(34.0);
            let editor_width = ui.available_width().min(760.0);
            ui.set_width(editor_width);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                if ui
                    .add(
                        TextEdit::singleline(&mut self.editor_title)
                            .font(FontId::proportional(34.0))
                            .hint_text("Untitled")
                            .frame(egui::Frame::NONE)
                            .desired_width(editor_width),
                    )
                    .changed()
                {
                    self.dirty = true;
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let statuses = self
                        .vault
                        .as_ref()
                        .map(|vault| {
                            vault
                                .config
                                .columns
                                .iter()
                                .flat_map(|column| column.statuses.iter().cloned())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    egui::ComboBox::from_id_salt("editor-status")
                        .selected_text(if self.editor_status.is_empty() {
                            "No status"
                        } else {
                            &self.editor_status
                        })
                        .show_ui(ui, |ui| {
                            for status in statuses {
                                if ui
                                    .selectable_value(
                                        &mut self.editor_status,
                                        status.clone(),
                                        status,
                                    )
                                    .changed()
                                {
                                    self.dirty = true;
                                }
                            }
                        });
                    ui.label(
                        RichText::new(if self.dirty {
                            "Edited locally"
                        } else {
                            "Saved locally"
                        })
                        .size(11.0)
                        .color(colors.secondary),
                    );
                });
                ui.add_space(18.0);
                ui.separator();
                ui.add_space(14.0);
                let editor_height = (ui.available_height() - 30.0).max(240.0);
                let changed = ScrollArea::vertical()
                    .scroll_bar_visibility(
                        egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    )
                    .id_salt("editor-body")
                    .max_height(editor_height)
                    .min_scrolled_height(editor_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut self.editor_body)
                                .font(FontId::monospace(15.5))
                                .hint_text("Write in Markdown…")
                                .frame(egui::Frame::NONE)
                                .desired_width(editor_width)
                                .desired_rows(34)
                                .lock_focus(true),
                        )
                        .changed()
                    })
                    .inner;
                if changed {
                    self.dirty = true;
                }
            });
        });
    }

    #[cfg(test)]
    fn selected_document(&self) -> Option<&Document> {
        let path = self.selected.as_ref()?;
        self.document(path)
    }
}

fn filtered_documents<'a>(
    vault: Option<&'a Vault>,
    view: View,
    query: &'a str,
) -> impl Iterator<Item = &'a Document> {
    let query = query.trim();
    vault
        .into_iter()
        .flat_map(|vault| vault.documents.iter())
        .filter(move |document| {
            (view != View::Library || document.is_book())
                && (query.is_empty()
                    || contains_ascii_case_insensitive(&document.title, query)
                    || contains_ascii_case_insensitive(&document.body, query)
                    || document
                        .metadata
                        .values()
                        .any(|value| contains_ascii_case_insensitive(value, query)))
        })
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|candidate| candidate.eq_ignore_ascii_case(needle.as_bytes()))
}
fn mouseia_tile_color(tradition: &str, hovered: bool) -> egui::Color32 {
    let color = match tradition {
        "egyptian" => egui::Color32::from_rgb(111, 79, 34),
        "greek" => egui::Color32::from_rgb(43, 69, 117),
        "roman" => egui::Color32::from_rgb(112, 53, 62),
        "american" => egui::Color32::from_rgb(52, 74, 105),
        "arabic" => egui::Color32::from_rgb(43, 88, 91),
        "chinese" => egui::Color32::from_rgb(31, 87, 72),
        "hindu" => egui::Color32::from_rgb(139, 77, 28),
        "persian" => egui::Color32::from_rgb(93, 59, 111),
        "alchemy" => egui::Color32::from_rgb(74, 70, 60),
        _ => egui::Color32::from_rgb(61, 66, 76),
    };
    if hovered {
        color.gamma_multiply(1.16)
    } else {
        color
    }
}

#[cfg(test)]
mod headless_tests;

impl eframe::App for SkribiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_mouseia();
        if self.mouseia.loading_catalog || self.mouseia.downloading.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(80));
        }
        let save_shortcut = ui.input(|input| input.modifiers.command && input.key_pressed(Key::S));
        let escape = ui.input(|input| input.key_pressed(Key::Escape));
        let mut action = if self.reader_open {
            self.reader_bar(ui)
        } else {
            self.collection_bar(ui)
        };
        if self.reader_open {
            self.reader_sidebar(ui);
        }
        let list_action = self.document_list(ui);
        let central_action = self.central(ui);
        action = action.or(list_action).or(central_action);
        if save_shortcut {
            action = Some(Action::Save);
        } else if escape {
            if self.reader_sidebar_open {
                self.reader_sidebar_open = false;
            } else if self.reader_open {
                action = Some(Action::CloseReader);
            }
        }
        if let Some(action) = action {
            self.perform(action);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let _ = self.save_current();
        storage.set_string(
            LIBRARY_KEY,
            self.vault
                .as_ref()
                .map_or_else(String::new, |vault| vault.root.display().to_string()),
        );
        storage.set_string(
            RETEX_KEY,
            self.retex_vault
                .as_ref()
                .map_or_else(String::new, |vault| vault.root.display().to_string()),
        );
        if let Ok(dismissed) = serde_json::to_string(&self.mouseia.dismissed) {
            storage.set_string(MOUSEIA_DISMISSED_KEY, dismissed);
        }
    }
}

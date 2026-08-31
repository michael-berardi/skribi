use std::path::PathBuf;

use eframe::egui::{self, Align, FontId, Key, Layout, RichText, ScrollArea, Sense, TextEdit, Vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};

use crate::{
    document::{Document, reading_chapters},
    importer::{self, ImportFormat},
    package, theme,
    vault::Vault,
};

const LIBRARY_KEY: &str = "skribi.library";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
enum View {
    #[default]
    Library,
    Notes,
    Board,
}

#[derive(Debug)]
enum Action {
    OpenFolder,
    ImportPackage,
    ImportNotion,
    ImportFolder,
    ExportPackage,
    NewNote,
    Select(PathBuf),
    Save,
    Move(PathBuf, String),
    Reload,
}

pub struct SkribiApp {
    vault: Option<Vault>,
    view: View,
    selected: Option<PathBuf>,
    search: String,
    editing: bool,
    editor_title: String,
    editor_status: String,
    editor_body: String,
    editor_fingerprint: String,
    dirty: bool,
    reading_section: usize,
    reading_chapter: usize,
    reader_zoom: f32,
    markdown_cache: CommonMarkCache,
    notice: Option<(String, bool)>,
}

impl SkribiApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        context.egui_ctx.set_theme(egui::ThemePreference::System);
        theme::apply(&context.egui_ctx);
        let preferred = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("SKRIBI_LIBRARY").map(PathBuf::from))
            .or_else(|| {
                context
                    .storage
                    .and_then(|storage| storage.get_string(LIBRARY_KEY))
                    .map(PathBuf::from)
            });
        let mut app = Self::empty();
        if let Some(path) = preferred {
            app.open_folder(path);
        }
        app
    }

    fn empty() -> Self {
        Self {
            vault: None,
            view: View::Library,
            selected: None,
            search: String::new(),
            editing: false,
            editor_title: String::new(),
            editor_status: String::new(),
            editor_body: String::new(),
            editor_fingerprint: String::new(),
            dirty: false,
            reading_section: 0,
            reading_chapter: 0,
            reader_zoom: 1.0,
            markdown_cache: CommonMarkCache::default(),
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
        app.open_folder(root);
        app.view = view;
        app.editing = editing;
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
                self.selected = None;
                self.dirty = false;
                if let Some(path) = first {
                    self.load_selected(path);
                }
                self.notice = is_obsidian.then(|| (
                    "Obsidian vault opened directly. Markdown, folders, attachments, and wikilinks stay in place.".into(),
                    true,
                ));
            }
            Err(error) => self.notice = Some((error.to_string(), false)),
        }
    }

    fn load_selected(&mut self, path: PathBuf) {
        if self.dirty && self.save_current().is_err() {
            return;
        }
        let document = self
            .vault
            .as_ref()
            .and_then(|vault| vault.document(&path))
            .cloned();
        if let Some(document) = document {
            self.selected = Some(path);
            self.editor_status = document.metadata_value("status").to_owned();
            self.editor_title = document.title;
            self.editor_body = document.body;
            self.editor_fingerprint = document.fingerprint;
            self.reading_section = 0;
            self.reading_chapter = 0;
            self.dirty = false;
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
        let Some(vault) = self.vault.as_mut() else {
            return Ok(());
        };
        match vault.save_document(
            &path,
            &self.editor_fingerprint,
            &self.editor_title,
            &self.editor_status,
            &self.editor_body,
        ) {
            Ok(document) => {
                self.editor_status = document.metadata_value("status").to_owned();
                self.editor_fingerprint = document.fingerprint;
                self.editor_title = document.title;
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
                            self.open_folder(destination);
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
                            self.open_folder(destination);
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
                if let Some(vault) = self.vault.as_mut() {
                    match vault.create_note() {
                        Ok(document) => {
                            self.view = View::Notes;
                            self.load_selected(document.path);
                            self.editing = true;
                        }
                        Err(error) => self.notice = Some((error.to_string(), false)),
                    }
                }
            }
            Action::Select(path) => self.load_selected(path),
            Action::Save => {
                let _ = self.save_current();
            }
            Action::Move(path, status) => {
                if self.dirty
                    && self.selected.as_ref() == Some(&path)
                    && self.save_current().is_err()
                {
                    return;
                }
                if let Some(vault) = self.vault.as_mut() {
                    match vault.set_status(&path, &status) {
                        Ok(document) => {
                            if self.selected.as_ref() == Some(&path) {
                                self.editor_status = document.metadata_value("status").to_owned();
                                self.editor_fingerprint = document.fingerprint;
                            }
                            self.notice = Some((format!("Moved to {status}."), true));
                        }
                        Err(error) => self.notice = Some((error.to_string(), false)),
                    }
                }
            }
            Action::Reload => {
                if let Some(vault) = self.vault.as_mut() {
                    match vault.reload() {
                        Ok(()) => {
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

    fn top_bar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let colors = theme::palette(ui);
        egui::Panel::top("toolbar")
            .exact_size(54.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.raised)
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .stroke(egui::Stroke::new(1.0, colors.hairline)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    theme::brand_mark(ui, 30.0);
                    ui.label(RichText::new("Skribi").size(20.0).strong());
                    ui.label(
                        RichText::new("by Implose Cybernetics")
                            .size(11.0)
                            .color(colors.secondary),
                    );
                    ui.add_space(14.0);
                    if ui.button("Open folder").clicked() {
                        action = Some(Action::OpenFolder);
                    }
                    ui.menu_button("Import", |ui| {
                        if ui.button("Skribi library…").clicked() {
                            action = Some(Action::ImportPackage);
                            ui.close();
                        }
                        if ui.button("Notion ZIP…").clicked() {
                            action = Some(Action::ImportNotion);
                            ui.close();
                        }
                        if ui.button("Obsidian or Markdown folder…").clicked() {
                            action = Some(Action::ImportFolder);
                            ui.close();
                        }
                    });
                    if ui
                        .add_enabled(self.vault.is_some(), egui::Button::new("Export"))
                        .clicked()
                    {
                        action = Some(Action::ExportPackage);
                    }
                    if ui
                        .add_enabled(self.vault.is_some(), egui::Button::new("New note"))
                        .clicked()
                    {
                        action = Some(Action::NewNote);
                    }
                    if ui
                        .add_enabled(self.vault.is_some(), egui::Button::new("Refresh"))
                        .clicked()
                    {
                        action = Some(Action::Reload);
                    }
                    if let Some((message, success)) = self.notice.as_ref() {
                        ui.label(
                            RichText::new(if *success { "Done" } else { "Error" })
                                .size(11.0)
                                .color(if *success { colors.green } else { colors.red }),
                        )
                        .on_hover_text(message);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(self.dirty, egui::Button::new("Save"))
                            .clicked()
                        {
                            action = Some(Action::Save);
                        }
                        if self.selected.is_some() {
                            ui.selectable_value(&mut self.editing, true, "Edit");
                            ui.selectable_value(&mut self.editing, false, "Read");
                        }
                    });
                });
            });
        action
    }

    fn navigation(&mut self, ui: &mut egui::Ui) {
        let colors = theme::palette(ui);
        egui::Panel::left("navigation")
            .exact_size(192.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.sidebar)
                    .inner_margin(egui::Margin::symmetric(14, 18))
                    .stroke(egui::Stroke::new(1.0, colors.hairline)),
            )
            .show(ui, |ui| {
                ui.label(
                    RichText::new("COLLECTION")
                        .size(11.0)
                        .strong()
                        .color(colors.secondary),
                );
                ui.add_space(5.0);
                nav_button(
                    ui,
                    &mut self.view,
                    View::Library,
                    "Library",
                    "Books and long-form reading",
                );
                nav_button(
                    ui,
                    &mut self.view,
                    View::Notes,
                    "Notes",
                    "Every Markdown document",
                );
                nav_button(
                    ui,
                    &mut self.view,
                    View::Board,
                    "Board",
                    "Retex-compatible workflow",
                );
                ui.add_space(18.0);
                ui.separator();
                ui.add_space(10.0);
                let (count, books) = self.vault.as_ref().map_or((0, 0), |vault| {
                    (
                        vault.documents.len(),
                        vault
                            .documents
                            .iter()
                            .filter(|document| document.is_book())
                            .count(),
                    )
                });
                ui.label(RichText::new(format!("{count} documents")).color(colors.secondary));
                ui.label(RichText::new(format!("{books} reading works")).color(colors.secondary));
                if let Some(vault) = self.vault.as_ref() {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(vault.root.display().to_string())
                            .size(11.0)
                            .color(colors.secondary),
                    );
                }
            });
    }

    fn document_list(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        if self.view == View::Board || self.vault.is_none() {
            return None;
        }
        let mut action = None;
        let colors = theme::palette(ui);
        egui::Panel::left("document-list")
            .default_size(284.0)
            .size_range(240.0..=360.0)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(colors.paper)
                    .inner_margin(egui::Margin::symmetric(12, 14))
                    .stroke(egui::Stroke::new(1.0, colors.hairline)),
            )
            .show(ui, |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.search)
                        .hint_text("Search titles, text, metadata…")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);
                let documents = filtered_documents(self.vault.as_ref(), self.view, &self.search);
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for document in documents {
                            let selected = self.selected.as_ref() == Some(&document.path);
                            let response = theme::card(ui)
                                .fill(if selected {
                                    colors.active
                                } else {
                                    colors.raised
                                })
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(RichText::new(&document.title).strong());
                                    let detail = if document.metadata_value("author").is_empty() {
                                        document.relative_path.display().to_string()
                                    } else {
                                        document.metadata_value("author").to_owned()
                                    };
                                    ui.label(
                                        RichText::new(detail).size(12.0).color(colors.secondary),
                                    );
                                })
                                .response
                                .interact(Sense::click());
                            if response.clicked() {
                                action = Some(Action::Select(document.path.clone()));
                            }
                            ui.add_space(6.0);
                        }
                    });
            });
        action
    }

    fn central(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let colors = theme::palette(ui);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(colors.paper)
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ui, |ui| {
                if self.vault.is_none() {
                    action = self.welcome(ui);
                } else if self.view == View::Board {
                    action = self.board(ui);
                } else if self.view == View::Library && self.selected.is_none() {
                    action = self.library_shelf(ui);
                } else if self.editing {
                    self.editor(ui);
                } else {
                    self.reader(ui);
                }
            });
        action
    }

    fn welcome(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let colors = theme::palette(ui);
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            theme::brand_mark(ui, 72.0);
            ui.add_space(18.0);
            ui.heading("Your words, without the machinery.");
            ui.label(
                RichText::new("Read, write, organize, and carry ordinary Markdown anywhere.")
                    .size(17.0)
                    .color(colors.secondary),
            );
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                if ui.button("Open Markdown folder").clicked() {
                    action = Some(Action::OpenFolder);
                }
                if ui.button("Import Notion ZIP").clicked() {
                    action = Some(Action::ImportNotion);
                }
                if ui.button("Import Skribi library").clicked() {
                    action = Some(Action::ImportPackage);
                }
            });
            ui.add_space(16.0);
            ui.label(
                RichText::new(
                    "Retex and Obsidian folders open directly. No account, conversion, or lock-in.",
                )
                .size(13.0)
                .color(colors.secondary),
            );
        });
        action
    }

    fn library_shelf(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let colors = theme::palette(ui);
        ui.heading("Library");
        ui.label(
            RichText::new("Long-form documents, books, and translation sets in your folder.")
                .color(colors.secondary),
        );
        ui.add_space(16.0);
        let books = filtered_documents(self.vault.as_ref(), self.view, &self.search);
        ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for document in books {
                    let response = theme::card(ui)
                        .show(ui, |ui| {
                            ui.set_min_size(Vec2::new(210.0, 120.0));
                            ui.label(
                                RichText::new(
                                    document.metadata_value("tradition").to_ascii_uppercase(),
                                )
                                .size(10.0)
                                .color(colors.ochre),
                            );
                            ui.label(RichText::new(&document.title).size(18.0).strong());
                            ui.label(
                                RichText::new(document.metadata_value("author"))
                                    .color(colors.secondary),
                            );
                            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                                ui.label(
                                    RichText::new("Open reader →")
                                        .size(12.0)
                                        .color(colors.ochre),
                                );
                            });
                        })
                        .response
                        .interact(Sense::click());
                    if response.clicked() {
                        action = Some(Action::Select(document.path.clone()));
                    }
                }
            });
        });
        action
    }

    fn reader(&mut self, ui: &mut egui::Ui) {
        let colors = theme::palette(ui);
        let Some(path) = self.selected.as_ref() else {
            ui.centered_and_justified(|ui| {
                ui.label("Choose a document from the list.");
            });
            return;
        };
        let Some(document) = self.vault.as_ref().and_then(|vault| vault.document(path)) else {
            ui.centered_and_justified(|ui| {
                ui.label("Choose a document from the list.");
            });
            return;
        };
        let sections = document.reading_sections();
        let reader_id = (
            "reader",
            &document.relative_path,
            self.reading_section,
            self.reading_chapter,
        );
        ScrollArea::vertical()
            .id_salt(reader_id)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(760.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .button("A+")
                                .on_hover_text("Increase text size")
                                .clicked()
                            {
                                self.reader_zoom = (self.reader_zoom + 0.1).min(1.8);
                            }
                            if ui
                                .button("A−")
                                .on_hover_text("Decrease text size")
                                .clicked()
                            {
                                self.reader_zoom = (self.reader_zoom - 0.1).max(0.8);
                            }
                            ui.label(
                                RichText::new(format!(
                                    "Text {}%",
                                    (self.reader_zoom * 100.0).round()
                                ))
                                .size(12.0)
                                .color(colors.secondary),
                            );
                        });
                    });
                    ui.add_space(14.0);
                    ui.label(RichText::new(&document.title).size(34.0).strong());
                    let byline = [
                        document.metadata_value("author"),
                        document.metadata_value("period"),
                    ]
                    .into_iter()
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                    if !byline.is_empty() {
                        ui.label(RichText::new(byline).size(15.0).color(colors.secondary));
                    }

                    self.reading_section =
                        self.reading_section.min(sections.len().saturating_sub(1));
                    let selected_body = sections
                        .get(self.reading_section)
                        .map_or(document.body.as_str(), |section| section.body);
                    let initial_chapters = reading_chapters(selected_body);
                    self.reading_chapter = self
                        .reading_chapter
                        .min(initial_chapters.len().saturating_sub(1));

                    if !sections.is_empty() || !initial_chapters.is_empty() {
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            if !sections.is_empty() {
                                let previous = self.reading_section;
                                egui::ComboBox::from_id_salt("reading-section")
                                    .selected_text(sections[self.reading_section].title)
                                    .show_ui(ui, |ui| {
                                        for (index, section) in sections.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut self.reading_section,
                                                index,
                                                section.title,
                                            );
                                        }
                                    });
                                if self.reading_section != previous {
                                    self.reading_chapter = 0;
                                    self.markdown_cache = CommonMarkCache::default();
                                }
                            }

                            let current_body = sections
                                .get(self.reading_section)
                                .map_or(document.body.as_str(), |section| section.body);
                            let chapters = reading_chapters(current_body);
                            if !chapters.is_empty() {
                                self.reading_chapter = self.reading_chapter.min(chapters.len() - 1);
                                let previous = self.reading_chapter;
                                egui::ComboBox::from_id_salt("reading-chapter")
                                    .selected_text(chapters[self.reading_chapter].title)
                                    .show_ui(ui, |ui| {
                                        for (index, chapter) in chapters.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut self.reading_chapter,
                                                index,
                                                chapter.title,
                                            );
                                        }
                                    });
                                if self.reading_chapter != previous {
                                    self.markdown_cache = CommonMarkCache::default();
                                }
                            }
                        });
                    }

                    ui.add_space(18.0);
                    ui.separator();
                    ui.add_space(20.0);
                    let selected_body = sections
                        .get(self.reading_section)
                        .map_or(document.body.as_str(), |section| section.body);
                    let chapters = reading_chapters(selected_body);
                    let selected_chapter = chapters.get(self.reading_chapter);
                    let markdown = selected_chapter.map_or(selected_body, |chapter| chapter.body);
                    ui.scope(|ui| {
                        let mut style = (**ui.style()).clone();
                        for font in style.text_styles.values_mut() {
                            font.size *= self.reader_zoom;
                        }
                        ui.set_style(style);
                        ui.with_layout(Layout::top_down(Align::Min), |ui| {
                            ui.set_max_width(720.0);
                            if let Some(chapter) = selected_chapter {
                                ui.heading(chapter.title);
                                ui.add_space(8.0);
                            }
                            CommonMarkViewer::new().show(ui, &mut self.markdown_cache, markdown);
                        });
                    });
                    ui.add_space(80.0);
                });
            });
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let colors = theme::palette(ui);
        if self.selected.is_none() {
            return;
        }
        ui.vertical_centered(|ui| {
            let editor_width = ui.available_width().min(840.0);
            ui.set_width(editor_width);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.label(
                    RichText::new("TITLE")
                        .size(11.0)
                        .strong()
                        .color(colors.secondary),
                );
                if ui
                    .add(
                        TextEdit::singleline(&mut self.editor_title)
                            .font(FontId::proportional(28.0))
                            .desired_width(editor_width),
                    )
                    .changed()
                {
                    self.dirty = true;
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Status").color(colors.secondary));
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
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("⌘S to save")
                                .size(11.0)
                                .color(colors.secondary),
                        );
                    });
                });
                ui.add_space(10.0);
                ui.label(
                    RichText::new("BODY")
                        .size(11.0)
                        .strong()
                        .color(colors.secondary),
                );
                let editor_height = (ui.available_height() - 20.0).max(200.0);
                let changed = egui::Frame::new()
                    .fill(colors.raised)
                    .stroke(egui::Stroke::new(1.0, colors.hairline))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .scroll_bar_visibility(
                                egui::containers::scroll_area::ScrollBarVisibility::AlwaysVisible,
                            )
                            .id_salt("editor-body")
                            .max_height(editor_height)
                            .min_scrolled_height(editor_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add(
                                    TextEdit::multiline(&mut self.editor_body)
                                        .font(FontId::monospace(15.0))
                                        .desired_width(editor_width - 12.0)
                                        .desired_rows(32)
                                        .lock_focus(true),
                                )
                                .changed()
                            })
                            .inner
                    })
                    .inner;
                if changed {
                    self.dirty = true;
                }
            });
        });
    }

    fn board(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let colors = theme::palette(ui);
        ui.heading("Board");
        ui.label(
            RichText::new("Status changes write directly to Retex-compatible YAML front matter.")
                .color(colors.secondary),
        );
        ui.add_space(14.0);
        let vault = self.vault.as_ref()?;
        let columns = vault.config.columns.clone();
        let documents = &vault.documents;
        let gap = ui.spacing().item_spacing.x;
        let column_height = ui.available_height().max(220.0);
        let column_width = (((ui.available_width() - 48.0)
            - gap * columns.len().saturating_sub(1) as f32)
            / columns.len().max(1) as f32
            - 28.0)
            .clamp(190.0, 250.0);
        ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for (column_index, column) in columns.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.set_width(column_width + 28.0);
                            theme::card(ui).fill(colors.sidebar).show(ui, |ui| {
                                ui.set_width(column_width);
                                ui.set_min_height((column_height - 28.0).max(180.0));
                                let matches_column = |document: &&Document| {
                                    let status = document.metadata_value("status");
                                    column
                                        .statuses
                                        .iter()
                                        .any(|candidate| candidate.eq_ignore_ascii_case(status))
                                        || (column_index == 0 && status.is_empty())
                                };
                                let card_count = documents.iter().filter(matches_column).count();
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&column.title).strong());
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(card_count.to_string())
                                                .color(colors.secondary),
                                        );
                                    });
                                });
                                ui.add_space(8.0);
                                ScrollArea::vertical()
                                    .id_salt(format!("board-{}", column.title))
                                    .max_height(ui.available_height() - 30.0)
                                    .show(ui, |ui| {
                                        for document in documents.iter().filter(matches_column) {
                                            theme::card(ui).show(ui, |ui| {
                                                if ui
                                                    .link(
                                                        RichText::new(&document.title)
                                                            .strong()
                                                            .color(colors.ink),
                                                    )
                                                    .clicked()
                                                {
                                                    action =
                                                        Some(Action::Select(document.path.clone()));
                                                }
                                                let kind = document.metadata_value("type");
                                                if !kind.is_empty() {
                                                    ui.label(
                                                        RichText::new(kind)
                                                            .size(11.0)
                                                            .color(colors.secondary),
                                                    );
                                                }
                                                ui.horizontal(|ui| {
                                                    if column_index > 0
                                                        && ui
                                                            .small_button("Move left")
                                                            .on_hover_text(format!(
                                                                "Move to {}",
                                                                columns[column_index - 1].title
                                                            ))
                                                            .clicked()
                                                    {
                                                        action = Some(Action::Move(
                                                            document.path.clone(),
                                                            columns[column_index - 1].statuses[0]
                                                                .clone(),
                                                        ));
                                                    }
                                                    if column_index + 1 < columns.len()
                                                        && ui
                                                            .small_button("Move right")
                                                            .on_hover_text(format!(
                                                                "Move to {}",
                                                                columns[column_index + 1].title
                                                            ))
                                                            .clicked()
                                                    {
                                                        action = Some(Action::Move(
                                                            document.path.clone(),
                                                            columns[column_index + 1].statuses[0]
                                                                .clone(),
                                                        ));
                                                    }
                                                });
                                            });
                                            ui.add_space(7.0);
                                        }
                                    });
                            });
                        });
                    }
                });
            });
        if matches!(action, Some(Action::Select(_))) {
            self.view = View::Notes;
        }
        action
    }

    #[cfg(test)]
    fn selected_document(&self) -> Option<&Document> {
        let path = self.selected.as_ref()?;
        self.vault.as_ref()?.document(path)
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

#[cfg(test)]
mod headless_tests;

impl eframe::App for SkribiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let save_shortcut = ui.input(|input| input.modifiers.command && input.key_pressed(Key::S));
        let mut action = self.top_bar(ui);
        self.navigation(ui);
        let list_action = self.document_list(ui);
        let central_action = self.central(ui);
        action = central_action.or(list_action).or(action);
        if save_shortcut {
            action = Some(Action::Save);
        }
        if let Some(action) = action {
            self.perform(action);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let _ = self.save_current();
        if let Some(vault) = self.vault.as_ref() {
            storage.set_string(LIBRARY_KEY, vault.root.display().to_string());
        }
    }
}

fn nav_button(ui: &mut egui::Ui, current: &mut View, view: View, label: &str, description: &str) {
    let selected = *current == view;
    let response = ui.selectable_label(selected, RichText::new(label).size(15.0));
    if response.clicked() {
        *current = view;
    }
    response.on_hover_text(description);
}

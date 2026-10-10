use super::*;
use eframe::egui::{Pos2, Rect, Theme, Vec2};
use egui_kittest::{Harness, SnapshotResults, kittest::Queryable as _};
use std::path::PathBuf;

const SCREEN_SIZE: Vec2 = Vec2::new(1280.0, 800.0);

fn book_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/meditations/content")
}

fn retex_fixture() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library");
    pin_fixture_times(&root);
    root
}

/// The vault lists notes newest first, and a fresh checkout gives every fixture the same time, so the first
/// note (and every snapshot of it) would depend on the checkout. Pin the order the snapshots were taken with.
fn pin_fixture_times(root: &std::path::Path) {
    let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_767_225_600);
    for (offset, name) in [
        (0, "commonplace.md"),
        (60, "meditations.md"),
        (120, "proposal.md"),
    ] {
        let file = std::fs::File::options()
            .write(true)
            .open(root.join(name))
            .expect("fixture note");
        file.set_modified(base + std::time::Duration::from_secs(offset))
            .expect("set fixture time");
    }
}

fn harness(root: PathBuf, theme: Theme, view: View) -> Harness<'static, SkribiApp> {
    Harness::builder()
        .with_size(SCREEN_SIZE)
        .with_theme(theme)
        .wgpu()
        .build_eframe(move |context| SkribiApp::new_for_test(context, root, view, false))
}

fn empty_harness(theme: Theme) -> Harness<'static, SkribiApp> {
    Harness::builder()
        .with_size(SCREEN_SIZE)
        .with_theme(theme)
        .wgpu()
        .build_eframe(|context| {
            theme::apply(&context.egui_ctx);
            SkribiApp::empty()
        })
}

fn assert_visible(harness: &Harness<'_, SkribiApp>, label: &str) {
    let viewport = Rect::from_min_size(Pos2::ZERO, SCREEN_SIZE);
    let rect = harness.get_by_label(label).rect();
    assert!(
        viewport.intersects(rect),
        "{label:?} must intersect the viewport, got {rect:?}"
    );
}

fn assert_visible_contains(harness: &Harness<'_, SkribiApp>, label: &str) {
    let viewport = Rect::from_min_size(Pos2::ZERO, SCREEN_SIZE);
    let rect = harness.get_by_label_contains(label).rect();
    assert!(
        viewport.intersects(rect),
        "a label containing {label:?} must intersect the viewport, got {rect:?}"
    );
}

#[test]
fn vault_tree_preserves_hierarchy_and_compacts_unicode_titles() {
    let document = |relative: &str, title: &str| Document {
        path: PathBuf::from("/vault").join(relative),
        relative_path: PathBuf::from(relative),
        title: title.to_owned(),
        metadata: std::collections::BTreeMap::new(),
        tags: Vec::new(),
        body: String::new(),
        source: String::new(),
        fingerprint: String::new(),
        modified: 0,
    };
    let vault = Vault {
        root: PathBuf::from("/vault"),
        documents: vec![
            document("Root.md", "Root"),
            document("Projects/Alpha/Notes.md", "Notes"),
        ],
        config: Default::default(),
        diagnostics: Vec::new(),
    };

    let tree = VaultTree::from_vault(&vault);

    assert_eq!(tree.documents[0].title, "Root");
    assert_eq!(
        tree.folders["Projects"].folders["Alpha"].documents[0].title,
        "Notes"
    );
    let compact = compact_title("道德經超長文件名稱", 6);
    assert_eq!(compact.chars().count(), 6);
    assert!(compact.ends_with('…'));
}

#[test]
fn headless_visual_matrix() {
    let mut results = SnapshotResults::new();
    let mut app = harness(book_fixture(), Theme::Light, View::Library);
    app.state_mut().mouseia.works = ["greek", "chinese", "american"]
        .into_iter()
        .enumerate()
        .map(|(index, tradition)| WorkSummary {
            slug: format!("sample-{index}"),
            title: ["Enchiridion", "Daodejing", "Federalist No. 10"][index].into(),
            author: ["Epictetus", "Laozi", "James Madison"][index].into(),
            tradition: tradition.into(),
            language: String::new(),
            genre: None,
            period: None,
            translations: 2,
        })
        .collect();

    assert_eq!(app.ctx.theme(), Theme::Light);
    {
        let document = app
            .state()
            .selected_document()
            .expect("the complete Meditations book must be selected");
        assert!(
            document.body.len() > 700_000,
            "the visual harness must exercise the complete book corpus"
        );
        let sections = document.reading_sections();
        assert_eq!(sections.len(), 3);
        assert!(
            sections
                .iter()
                .all(|section| section.body.contains("### Book XII")),
            "every language/translation must include all twelve books"
        );
    }
    assert_visible(&app, "Contents");
    assert_visible_contains(&app, "Παρὰ τοῦ πάππου");
    results.add(app.try_snapshot("reader_light"));

    app.state_mut().select_translation(1);
    app.run();
    assert_visible_contains(&app, "CURATED TRANSLATION");
    assert_visible_contains(&app, "From my Grandfather Verus");

    app.state_mut().reader_open = false;
    app.run();
    assert_visible_contains(&app, "book");
    assert_visible(&app, "Marcus Aurelius");
    results.add(app.try_snapshot("library_light"));

    app.state_mut().reader_open = true;
    app.run();
    app.get_by_label("Edit").click();
    app.run();
    assert_visible(&app, "Saved locally");
    assert_visible_contains(&app, "Meditations");
    results.add(app.try_snapshot("editor_light"));
    let mut notes = harness(retex_fixture(), Theme::Light, View::Notes);
    assert_visible(&notes, "Open reader");
    results.add(notes.try_snapshot("notes_light"));

    app.ctx.set_theme(Theme::Dark);
    app.state_mut().editing = false;
    app.state_mut().reader_open = true;
    app.state_mut().select_translation(2);
    assert!(
        app.state()
            .reader_blocks
            .iter()
            .all(|block| block.section == 2)
    );
    app.run();
    assert_visible_contains(&app, "From my grandfather Verus");
    results.add(app.try_snapshot("reader_dark"));

    app.state_mut().select_translation(0);
    app.state_mut().reading_chapter = 11;
    app.state_mut().reader_jump = Some((0, 11));
    app.run();
    assert_visible_contains(&app, "Πάντα ἐκεῖνα");
    results.add(app.try_snapshot("reader_book_xii_dark"));

    let mut welcome = empty_harness(Theme::Light);
    assert_visible(&welcome, "Open library folder");
    results.add(welcome.try_snapshot("welcome_light"));
    welcome.ctx.set_theme(Theme::Dark);
    welcome.run();
    assert_visible(&welcome, "Open library folder");
    results.add(welcome.try_snapshot("welcome_dark"));
    welcome.state_mut().view = View::Notes;
    welcome.run();
    assert_visible(&welcome, "Open Retex vault");
    results.unwrap();
}

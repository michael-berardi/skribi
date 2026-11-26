use super::*;
use crate::updater::Version;
use eframe::egui::{Pos2, Rect, Theme, Vec2};
use egui_kittest::{
    Harness, SnapshotResults,
    kittest::{NodeT as _, Queryable as _},
};
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

fn sample_update() -> ReleaseInfo {
    let current = Version::parse(updater::CURRENT_VERSION).expect("stable current version");
    ReleaseInfo {
        version: Version {
            major: current.major,
            minor: current.minor + 1,
            patch: 0,
        },
        notes: "- Notes stay local.\n- Updates verify SHA256SUMS and keep a rollback.\n".into(),
        assets: Vec::new(),
    }
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

fn toggled(harness: &Harness<'_, SkribiApp>, label: &str) -> bool {
    harness.get_by_label(label).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}

/// Contract §7: banner text leading and actions trailing on one row, ending on the toolbar's
/// trailing edge; Settings toggles one per row with Check Now on the version row, sharing the
/// switches' trailing edge.
#[test]
fn update_ui_alignment_matrix() {
    let mut results = SnapshotResults::new();
    for (theme, suffix) in [(Theme::Light, "light"), (Theme::Dark, "dark")] {
        let mut app = empty_harness(theme);
        app.state_mut().update_state = UpdateState::Available(sample_update());
        app.run();
        assert_visible_contains(&app, "is available — you have");
        let install = app.get_by_label("Install Update").rect();
        let later = app.get_by_label("Remind Me Later").rect();
        let skip = app.get_by_label("Skip This Version").rect();
        let more = app.get_by_label("More").rect();
        let heading = app.get_by_label_contains("is available — you have").rect();
        assert!(
            (install.right() - more.right()).abs() <= 1.0,
            "{install:?} vs {more:?}"
        );
        assert!(skip.right() < later.left() && later.right() < install.left());
        assert!((skip.center().y - install.center().y).abs() <= 1.0);
        assert!(
            heading.right() < skip.left(),
            "text leads, actions trail on one row"
        );
        assert!(!toggled(
            &app,
            "Automatically download and install updates in the future"
        ));
        results.add(app.try_snapshot(format!("update_banner_{suffix}")));

        app.state_mut().update_state = UpdateState::Idle;
        app.state_mut().settings_open = true;
        app.run();
        assert_visible(&app, "Settings");
        let check = app.get_by_label("Check for updates automatically").rect();
        let auto = app.get_by_label("Install updates automatically").rect();
        let now = app.get_by_label("Check Now").rect();
        let version = app
            .get_by_label(&format!("Version {}", updater::CURRENT_VERSION))
            .rect();
        assert!(
            (check.right() - now.right()).abs() <= 1.0,
            "{check:?} vs {now:?}"
        );
        assert!((auto.right() - now.right()).abs() <= 1.0);
        assert!((version.left() - check.left()).abs() <= 1.0);
        assert!(
            (version.center().y - now.center().y).abs() <= 1.0,
            "one version row"
        );
        assert!(check.bottom() <= auto.top() && auto.bottom() <= now.top());
        assert!(toggled(&app, "Check for updates automatically"));
        assert!(!toggled(&app, "Install updates automatically"));
        results.add(app.try_snapshot(format!("update_settings_{suffix}")));
    }
    results.unwrap();
}

#[test]
fn update_banner_choices_skip_remind_and_opt_in() {
    let mut app = empty_harness(Theme::Light);
    let release = sample_update();
    let version = release.version.to_string();
    app.state_mut().update_state = UpdateState::Available(release.clone());
    app.run();
    app.get_by_label("Automatically download and install updates in the future")
        .click();
    app.run();
    assert!(app.state().updates.install_automatically);
    app.get_by_label("Remind Me Later").click();
    app.run();
    assert_eq!(app.state().update_state, UpdateState::Idle);
    assert_eq!(app.state().updates.skipped_version, None);

    app.state_mut().update_state = UpdateState::Available(release);
    app.run();
    app.get_by_label("Skip This Version").click();
    app.run();
    assert_eq!(app.state().update_state, UpdateState::Idle);
    assert!(app.state().updates.is_skipped(&version));

    app.state_mut().settings_open = true;
    app.run();
    app.get_by_label("Install updates automatically").click();
    app.run();
    assert!(!app.state().updates.install_automatically);
}

#[test]
fn headless_visual_matrix() {
    let mut results = SnapshotResults::new();
    let mut app = harness(book_fixture(), Theme::Light, View::Library);
    // The visual fixture shows a browsed catalog; the app never fetches it on its own.
    app.state_mut().mouseia.catalog_enabled = true;
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

#[test]
fn opening_a_library_does_not_fetch_the_mouseia_catalog() {
    let mut app = SkribiApp::empty();
    app.open_folder(book_fixture());
    assert!(!app.mouseia.catalog_enabled);
    assert!(!app.mouseia.loading_catalog);
    assert!(app.mouseia.works.is_empty());
    // Loading remains a no-op until the user explicitly browses the catalog.
    app.mouseia.load_catalog();
    assert!(!app.mouseia.loading_catalog);
    app.mouseia.enable_catalog();
    assert!(app.mouseia.catalog_enabled);
}

#[test]
fn a_leading_heading_that_repeats_the_title_is_not_shown_twice() {
    assert_eq!(
        without_leading_title("# Reading Plan\n\nCompare.", "Reading Plan"),
        "\nCompare."
    );
    assert_eq!(
        without_leading_title("# Other\n\nBody", "Reading Plan"),
        "# Other\n\nBody"
    );
    assert_eq!(without_leading_title("Body only", "Body only"), "Body only");
}

#[test]
fn leaving_the_editor_saves_and_a_failed_save_keeps_the_selection() {
    let root = std::env::temp_dir().join(format!("skribi-edit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp vault");
    std::fs::write(root.join("a.md"), "# Alpha\n\nfirst\n").expect("note a");
    std::fs::write(root.join("b.md"), "# Beta\n\nsecond\n").expect("note b");
    let mut app = harness(root.clone(), Theme::Light, View::Notes);
    let (a, b) = {
        let vault = app.state().retex_vault.as_ref().expect("vault");
        let path = |title: &str| {
            vault
                .documents
                .iter()
                .find(|document| document.title == title)
                .expect("note")
                .path
                .clone()
        };
        (path("Alpha"), path("Beta"))
    };
    assert!(app.state_mut().load_selected(a.clone()));
    app.state_mut().reader_open = true;
    app.state_mut().editing = true;
    app.state_mut().editor_body = "changed in the editor\n".into();
    app.state_mut().dirty = true;
    app.run();
    app.get_by_label("Read").click();
    app.run();
    assert!(!app.state().editing && !app.state().dirty);
    assert!(
        std::fs::read_to_string(&a)
            .expect("saved note")
            .contains("changed in the editor")
    );

    // The file changes on disk under an unsaved edit: the save is refused, so the note stays open.
    app.state_mut().editor_body = "second edit\n".into();
    app.state_mut().dirty = true;
    std::fs::write(&a, "# Alpha\n\nchanged elsewhere\n").expect("external edit");
    assert!(!app.state_mut().load_selected(b));
    assert_eq!(app.state().selected.as_ref(), Some(&a));
    app.run();
    assert_visible_contains(&app, "changed on disk");
    let _ = std::fs::remove_dir_all(&root);
}

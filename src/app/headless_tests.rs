use super::*;
use eframe::egui::{Pos2, Rect, Theme, Vec2};
use egui_kittest::{Harness, SnapshotResults, kittest::Queryable as _};
use std::path::PathBuf;

const SCREEN_SIZE: Vec2 = Vec2::new(1280.0, 800.0);

fn book_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/meditations/content")
}

fn board_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library")
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

fn assert_fully_visible(harness: &Harness<'_, SkribiApp>, label: &str) {
    let viewport = Rect::from_min_size(Pos2::ZERO, SCREEN_SIZE);
    let rect = harness.get_by_label(label).rect();
    assert!(
        viewport.contains(rect.min) && viewport.contains(rect.max),
        "{label:?} must fit inside the viewport, got {rect:?}"
    );
}

#[test]
fn headless_visual_matrix() {
    let mut results = SnapshotResults::new();
    let mut app = harness(book_fixture(), Theme::Light, View::Library);

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
    assert_visible_contains(&app, "Παρὰ τοῦ πάππου");
    results.add(app.try_snapshot("reader_light"));

    app.get_by_label("Edit").click();
    app.run();
    assert_visible(&app, "TITLE");
    assert_visible(&app, "BODY");
    results.add(app.try_snapshot("editor_light"));

    let mut board = harness(board_fixture(), Theme::Light, View::Board);
    results.add(board.try_snapshot("board_light"));
    assert_visible(&board, "Inbox");
    assert_visible(&board, "Reading");
    assert_visible(&board, "Proposal");
    assert_fully_visible(&board, "Done");

    board.ctx.set_theme(Theme::Dark);
    board.run();
    assert_eq!(board.ctx.theme(), Theme::Dark);
    results.add(board.try_snapshot("board_dark"));

    app.ctx.set_theme(Theme::Dark);
    app.state_mut().editing = false;
    app.state_mut().reading_section = 2;
    app.state_mut().reading_chapter = 0;
    app.run();
    assert_visible_contains(&app, "From my grandfather Verus");
    results.add(app.try_snapshot("reader_dark"));

    app.state_mut().reading_section = 0;
    app.state_mut().reading_chapter = 11;
    app.run();
    assert_visible_contains(&app, "Πάντα ἐκεῖνα");
    results.add(app.try_snapshot("reader_book_xii_dark"));

    let mut welcome = empty_harness(Theme::Light);
    assert_visible(&welcome, "Open Markdown folder");
    results.add(welcome.try_snapshot("welcome_light"));
    welcome.ctx.set_theme(Theme::Dark);
    welcome.run();
    assert_visible(&welcome, "Open Markdown folder");
    results.add(welcome.try_snapshot("welcome_dark"));
    results.unwrap();
}

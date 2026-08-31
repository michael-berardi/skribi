use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    TextStyle, Vec2,
};
use fontdb::{Database, Family, Query, Stretch, Style, Weight};

#[derive(Clone, Copy)]
pub struct Palette {
    pub paper: Color32,
    pub raised: Color32,
    pub sidebar: Color32,
    pub ink: Color32,
    pub secondary: Color32,
    pub hairline: Color32,
    pub ochre: Color32,
    pub green: Color32,
    pub red: Color32,
    pub hover: Color32,
    pub active: Color32,
}

pub fn palette(ui: &egui::Ui) -> Palette {
    palette_for(ui.visuals().dark_mode)
}

pub fn palette_for(dark: bool) -> Palette {
    if dark {
        Palette {
            paper: Color32::from_rgb(7, 7, 7),
            raised: Color32::from_rgb(15, 15, 15),
            sidebar: Color32::from_rgb(11, 11, 11),
            ink: Color32::from_rgb(239, 237, 232),
            secondary: Color32::from_rgb(172, 168, 160),
            hairline: Color32::from_rgb(38, 37, 34),
            ochre: Color32::from_rgb(214, 150, 68),
            green: Color32::from_rgb(130, 180, 139),
            red: Color32::from_rgb(224, 121, 109),
            hover: Color32::from_rgb(24, 23, 21),
            active: Color32::from_rgb(43, 32, 19),
        }
    } else {
        Palette {
            paper: Color32::from_rgb(246, 244, 239),
            raised: Color32::from_rgb(253, 251, 247),
            sidebar: Color32::from_rgb(239, 236, 230),
            ink: Color32::from_rgb(31, 29, 26),
            secondary: Color32::from_rgb(96, 91, 83),
            hairline: Color32::from_rgb(218, 213, 204),
            ochre: Color32::from_rgb(174, 111, 34),
            green: Color32::from_rgb(78, 110, 83),
            red: Color32::from_rgb(157, 71, 63),
            hover: Color32::from_rgb(241, 236, 226),
            active: Color32::from_rgb(235, 223, 203),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReaderFont {
    System,
    #[default]
    Literata,
    GentiumPlus,
    NotoSerif,
    IbmPlexSans,
    IbmPlexMono,
}

pub const READER_FONTS: [ReaderFont; 6] = [
    ReaderFont::System,
    ReaderFont::Literata,
    ReaderFont::GentiumPlus,
    ReaderFont::NotoSerif,
    ReaderFont::IbmPlexSans,
    ReaderFont::IbmPlexMono,
];

impl ReaderFont {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Literata => "Literata",
            Self::GentiumPlus => "Gentium Plus",
            Self::NotoSerif => "Noto Serif",
            Self::IbmPlexSans => "IBM Plex Sans",
            Self::IbmPlexMono => "IBM Plex Mono",
        }
    }
}

pub fn reader_family(font: ReaderFont) -> FontFamily {
    match font {
        ReaderFont::System => FontFamily::Proportional,
        _ => FontFamily::Name(font.label().into()),
    }
}

fn install_reader_font(fonts: &mut FontDefinitions, font: ReaderFont, bytes: &'static [u8]) {
    let name = font.label().to_owned();
    fonts
        .font_data
        .insert(name.clone(), Arc::new(FontData::from_static(bytes)));
    let mut family_fonts = vec![name];
    if let Some(fallbacks) = fonts.families.get(&FontFamily::Proportional) {
        family_fonts.extend(fallbacks.iter().cloned());
    }
    fonts.families.insert(reader_family(font), family_fonts);
}
fn install_system_fallbacks(fonts: &mut FontDefinitions) {
    let mut database = Database::new();
    database.load_system_fonts();
    let groups: &[&[&str]] = &[
        &[
            "Hiragino Sans GB",
            "STHeiti",
            "PingFang SC",
            "Noto Sans CJK SC",
            "Microsoft YaHei",
            "WenQuanYi Zen Hei",
        ],
        &[
            "Kohinoor Devanagari",
            "Noto Sans Devanagari",
            "Nirmala UI",
            "Mangal",
        ],
        &["Geeza Pro", "Noto Sans Arabic", "Nirmala UI", "Tahoma"],
    ];
    let mut fallbacks = Vec::new();
    for (group_index, candidates) in groups.iter().enumerate() {
        let face = candidates.iter().find_map(|candidate| {
            database.query(&Query {
                families: &[Family::Name(candidate)],
                weight: Weight::NORMAL,
                stretch: Stretch::Normal,
                style: Style::Normal,
            })
        });
        let Some(face) = face else {
            continue;
        };
        let Some((data, face_index)) =
            database.with_face_data(face, |data, face_index| (data.to_vec(), face_index))
        else {
            continue;
        };
        let name = format!("Skribi system fallback {group_index}");
        let mut font = FontData::from_owned(data);
        font.index = face_index;
        fonts.font_data.insert(name.clone(), Arc::new(font));
        fallbacks.push(name);
    }
    for family in fonts.families.values_mut() {
        family.extend(fallbacks.iter().cloned());
    }
}

pub fn apply(context: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    install_reader_font(
        &mut fonts,
        ReaderFont::Literata,
        include_bytes!("../assets/fonts/Literata.ttf"),
    );
    install_reader_font(
        &mut fonts,
        ReaderFont::GentiumPlus,
        include_bytes!("../assets/fonts/GentiumPlus.ttf"),
    );
    install_reader_font(
        &mut fonts,
        ReaderFont::NotoSerif,
        include_bytes!("../assets/fonts/NotoSerif.ttf"),
    );
    install_reader_font(
        &mut fonts,
        ReaderFont::IbmPlexSans,
        include_bytes!("../assets/fonts/IBMPlexSans.ttf"),
    );
    install_reader_font(
        &mut fonts,
        ReaderFont::IbmPlexMono,
        include_bytes!("../assets/fonts/IBMPlexMono.ttf"),
    );
    install_system_fallbacks(&mut fonts);
    context.set_fonts(fonts);

    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let palette = palette_for(theme == egui::Theme::Dark);
        let mut style = (*context.style_of(theme)).clone();
        style.visuals = if theme == egui::Theme::Dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        style.visuals.panel_fill = palette.paper;
        style.visuals.window_fill = palette.raised;
        style.visuals.extreme_bg_color = palette.raised;
        style.visuals.faint_bg_color = palette.sidebar;
        style.visuals.override_text_color = Some(palette.ink);
        style.visuals.selection.bg_fill = palette.ochre.gamma_multiply(0.30);
        style.visuals.selection.stroke = Stroke::new(1.0, palette.ochre);
        style.visuals.widgets.noninteractive.bg_fill = palette.paper;
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.hairline);
        style.visuals.widgets.inactive.bg_fill = palette.paper;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(0.0, Color32::TRANSPARENT);
        style.visuals.widgets.hovered.bg_fill = palette.hover;
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.hairline);
        style.visuals.widgets.active.bg_fill = palette.active;
        style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.ochre);
        style.visuals.widgets.open.bg_fill = palette.active;
        style.visuals.widgets.open.bg_stroke = Stroke::new(1.0, palette.ochre);
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        style.spacing.interact_size.y = 32.0;
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(30.0, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(17.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(13.5, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(11.5, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(15.0, FontFamily::Monospace),
        );
        context.set_style_of(theme, style);
    }
}

pub fn card(ui: &egui::Ui) -> egui::Frame {
    let palette = palette(ui);
    egui::Frame::new()
        .fill(palette.raised)
        .stroke(Stroke::new(1.0, palette.hairline))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(12))
}

pub fn brand_mark(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter();
    let center = rect.center();
    let palette = palette(ui);
    let stroke = Stroke::new((size * 0.065).max(1.2), palette.ink);
    let accent = Stroke::new((size * 0.075).max(1.4), palette.ochre);
    let left = vec![
        rect.left_top() + Vec2::new(size * 0.12, size * 0.24),
        center + Vec2::new(-size * 0.04, -size * 0.08),
        center + Vec2::new(-size * 0.04, size * 0.34),
        rect.left_bottom() + Vec2::new(size * 0.12, -size * 0.12),
    ];
    let right = vec![
        center + Vec2::new(size * 0.04, -size * 0.08),
        rect.right_top() + Vec2::new(-size * 0.12, size * 0.24),
        rect.right_bottom() + Vec2::new(-size * 0.12, -size * 0.12),
        center + Vec2::new(size * 0.04, size * 0.34),
    ];
    painter.add(egui::Shape::line(left, stroke));
    painter.add(egui::Shape::line(right, stroke));
    painter.line_segment(
        [
            rect.left_bottom() + Vec2::new(size * 0.16, -size * 0.18),
            rect.right_top() + Vec2::new(-size * 0.06, size * 0.08),
        ],
        accent,
    );
}

pub fn window_icon() -> egui::IconData {
    let size = 64_u32;
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let index = ((y * size + x) * 4) as usize;
            let dx = x as i32 - 32;
            let dy = y as i32 - 32;
            let inside = dx * dx + dy * dy < 30 * 30;
            if inside {
                rgba[index..index + 4].copy_from_slice(&[247, 244, 237, 255]);
                let page = (x > 13 && x < 31 && y > 14 && y < 49)
                    || (x > 33 && x < 51 && y > 14 && y < 49);
                if page {
                    rgba[index..index + 4].copy_from_slice(&[36, 34, 30, 255]);
                }
                let inner = (x > 17 && x < 30 && y > 18 && y < 45)
                    || (x > 34 && x < 47 && y > 18 && y < 45);
                if inner {
                    rgba[index..index + 4].copy_from_slice(&[247, 244, 237, 255]);
                }
                if (x as i32 + y as i32 - 63).abs() <= 2 && x > 14 && x < 52 {
                    rgba[index..index + 4].copy_from_slice(&[185, 120, 36, 255]);
                }
            }
        }
    }
    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

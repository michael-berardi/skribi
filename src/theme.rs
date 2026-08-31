use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Vec2,
};

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
            paper: Color32::BLACK,
            raised: Color32::from_rgb(10, 10, 10),
            sidebar: Color32::from_rgb(5, 5, 5),
            ink: Color32::from_rgb(242, 240, 235),
            secondary: Color32::from_rgb(172, 168, 160),
            hairline: Color32::from_rgb(38, 37, 34),
            ochre: Color32::from_rgb(224, 158, 70),
            green: Color32::from_rgb(137, 190, 146),
            red: Color32::from_rgb(232, 128, 116),
            hover: Color32::from_rgb(23, 22, 20),
            active: Color32::from_rgb(39, 30, 18),
        }
    } else {
        Palette {
            paper: Color32::from_rgb(247, 244, 237),
            raised: Color32::from_rgb(255, 253, 248),
            sidebar: Color32::from_rgb(238, 234, 225),
            ink: Color32::from_rgb(36, 34, 30),
            secondary: Color32::from_rgb(111, 106, 97),
            hairline: Color32::from_rgb(216, 209, 196),
            ochre: Color32::from_rgb(185, 120, 36),
            green: Color32::from_rgb(88, 116, 91),
            red: Color32::from_rgb(163, 78, 69),
            hover: Color32::from_rgb(244, 238, 226),
            active: Color32::from_rgb(238, 225, 204),
        }
    }
}

pub fn apply(context: &egui::Context) {
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
        style.visuals.widgets.inactive.bg_fill = palette.raised;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.hairline);
        style.visuals.widgets.hovered.bg_fill = palette.hover;
        style.visuals.widgets.hovered.bg_stroke =
            Stroke::new(1.0, palette.ochre.gamma_multiply(0.72));
        style.visuals.widgets.active.bg_fill = palette.active;
        style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.ochre);
        style.visuals.widgets.open.bg_fill = palette.active;
        style.visuals.widgets.open.bg_stroke = Stroke::new(1.0, palette.ochre);
        style.spacing.item_spacing = Vec2::new(10.0, 9.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        style.spacing.interact_size.y = 36.0;
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(28.0, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(16.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
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
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
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

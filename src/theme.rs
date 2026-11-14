use eframe::egui::{self, Align, Color32, CornerRadius, Margin, Stroke, Vec2};

pub const BG_CHROME: Color32 = Color32::from_rgb(30, 30, 32);
pub const BG_PANEL: Color32 = Color32::from_rgb(37, 37, 40);
pub const BG_SURFACE: Color32 = Color32::from_rgb(46, 46, 50);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(58, 58, 64);
pub const BG_INPUT: Color32 = Color32::from_rgb(24, 24, 27);
pub const BG_VIEWPORT: Color32 = Color32::from_rgb(42, 44, 48);

pub const BORDER_LINE: Color32 = Color32::from_rgb(53, 53, 58);
pub const BORDER_INPUT: Color32 = Color32::from_rgb(72, 74, 82);
pub const BORDER_HOVER: Color32 = Color32::from_rgb(102, 106, 118);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(226, 228, 232);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(156, 158, 166);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(139, 142, 152);

pub const ACCENT: Color32 = Color32::from_rgb(76, 114, 156);
pub const SELECTION_BG: Color32 = Color32::from_rgb(54, 72, 94);

pub const STATUS_MODIFIED: Color32 = Color32::from_rgb(209, 154, 69);
pub const STATUS_ADDED: Color32 = Color32::from_rgb(92, 160, 114);
pub const STATUS_REMOVED: Color32 = Color32::from_rgb(196, 91, 91);

pub const CONTROL_H: f32 = 24.0;
pub const ROW_H: f32 = 24.0;
pub const HEADER_H: f32 = 24.0;

mod icons;
pub use icons::{Icon, direction_icon, icon, icon_sized};

pub fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG_PANEL;
    visuals.window_fill = BG_SURFACE;
    visuals.extreme_bg_color = BG_INPUT;
    visuals.faint_bg_color = Color32::from_rgb(41, 41, 45);
    visuals.code_bg_color = BG_INPUT;
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.window_corner_radius = CornerRadius::same(3);
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER_INPUT);
    visuals.menu_corner_radius = CornerRadius::same(3);
    visuals.popup_shadow = egui::Shadow {
        offset: [0, 3],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };

    visuals.selection.bg_fill = SELECTION_BG;
    visuals.selection.stroke = Stroke::NONE;

    visuals.widgets.noninteractive.bg_fill = BG_PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = BG_PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_LINE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_SECONDARY);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(3);

    visuals.widgets.inactive.bg_fill = BG_SURFACE;
    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_LINE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(3);

    visuals.widgets.hovered.bg_fill = BG_ELEVATED;
    visuals.widgets.hovered.weak_bg_fill = BG_ELEVATED;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, BORDER_HOVER);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(3);

    visuals.widgets.active.bg_fill = SELECTION_BG;
    visuals.widgets.active.weak_bg_fill = SELECTION_BG;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(3);

    visuals.widgets.open.bg_fill = BG_ELEVATED;
    visuals.widgets.open.weak_bg_fill = BG_ELEVATED;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.open.corner_radius = CornerRadius::same(3);

    visuals.striped = false;
    visuals
}

pub fn apply_style(ctx: &egui::Context) {
    ctx.set_visuals(visuals());
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.menu_margin = Margin::same(4);
    style.spacing.indent = 14.0;
    style.spacing.interact_size.y = CONTROL_H;
    style.spacing.combo_height = 240.0;
    style.spacing.scroll.floating = false;
    style.spacing.scroll.bar_width = 10.0;
    style.spacing.scroll.handle_min_length = 24.0;
    style.spacing.scroll.bar_inner_margin = 4.0;
    style.spacing.scroll.bar_outer_margin = 6.0;
    style.interaction.resize_grab_radius_side = 3.0;
    style.interaction.selectable_labels = false;
    style.interaction.tooltip_delay = 0.4;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(12.5));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(11.5));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
    ctx.set_style(style);
}

pub fn hline(ui: &mut egui::Ui) {
    let width = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0_f32, BORDER_LINE),
    );
}

pub fn vsep(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(4.0, CONTROL_H), egui::Sense::hover());
    let top = egui::pos2(rect.center().x, rect.center().y - 8.0);
    let bottom = egui::pos2(rect.center().x, rect.center().y + 8.0);
    ui.painter()
        .line_segment([top, bottom], Stroke::new(1.0_f32, BORDER_LINE));
}

pub fn percent_slider(ui: &mut egui::Ui, value: &mut f32, width: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(width, CONTROL_H - 4.0),
        egui::Sense::click_and_drag(),
    );
    let mut changed = false;
    if (resp.clicked() || resp.dragged())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let usable = (rect.width() - 8.0).max(1.0);
        let next = ((pos.x - (rect.left() + 4.0)) / usable).clamp(0.0, 1.0);
        if (next - *value).abs() > 1e-3 {
            *value = next;
            changed = true;
        }
    }
    let hovering = resp.hovered() || resp.dragged();
    if hovering {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, BG_INPUT);
    let inner = rect.shrink(2.0);
    let fill_w = (inner.width() * value.clamp(0.0, 1.0)).max(0.0);
    if fill_w > 0.5 {
        let fill_rect = egui::Rect::from_min_size(inner.min, Vec2::new(fill_w, inner.height()));
        painter.rect_filled(fill_rect, 2.0, if hovering { ACCENT } else { SELECTION_BG });
    }
    let thumb_x = inner.left() + inner.width() * value.clamp(0.0, 1.0);
    painter.line_segment(
        [
            egui::pos2(
                thumb_x.clamp(inner.left() + 1.0, inner.right() - 1.0),
                inner.top() + 1.0,
            ),
            egui::pos2(
                thumb_x.clamp(inner.left() + 1.0, inner.right() - 1.0),
                inner.bottom() - 1.0,
            ),
        ],
        Stroke::new(
            2.0_f32,
            if hovering {
                Color32::WHITE
            } else {
                TEXT_PRIMARY
            },
        ),
    );
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0_f32, if hovering { BORDER_HOVER } else { BORDER_INPUT }),
        egui::StrokeKind::Inside,
    );
    let pct = (*value * 100.0).round() as u32;
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{pct}%"),
        egui::FontId::proportional(11.0),
        if *value > 0.45 || hovering {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        },
    );
    changed
}

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} Б")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} КБ", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} МБ", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub fn format_thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

pub fn draw_checkerboard(painter: &egui::Painter, rect: egui::Rect, tile: f32) {
    let clipped = rect.intersect(painter.clip_rect());
    if !clipped.is_positive() {
        return;
    }
    let start_x = ((clipped.left() - rect.left()) / tile).floor().max(0.0) as i32;
    let start_y = ((clipped.top() - rect.top()) / tile).floor().max(0.0) as i32;
    let end_x = ((clipped.right() - rect.left()) / tile).ceil() as i32;
    let end_y = ((clipped.bottom() - rect.top()) / tile).ceil() as i32;
    for y in start_y..end_y {
        for x in start_x..end_x {
            let cell = egui::Rect::from_min_size(
                rect.min + Vec2::new(x as f32 * tile, y as f32 * tile),
                Vec2::splat(tile),
            )
            .intersect(rect);
            painter.rect_filled(
                cell,
                0.0,
                Color32::from_gray(if (x + y) % 2 == 0 { 64 } else { 84 }),
            );
        }
    }
}

pub fn section_bar_fill(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, BG_CHROME);
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, BORDER_LINE),
    );
}

pub fn section_bar(ui: &mut egui::Ui, title: &str, count: usize) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, BG_CHROME);
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, BORDER_LINE),
    );
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(8.0, 0.0))),
        |ui| {
            ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                ui.add(icon_sized(Icon::Archive, 13.0).tint(TEXT_SECONDARY));
                ui.label(egui::RichText::new(title).size(12.0).color(TEXT_PRIMARY));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format_thousands(count as u64))
                            .size(11.5)
                            .color(TEXT_MUTED),
                    );
                });
            });
        },
    );
}

mod controls;
pub use controls::*;

use super::*;

#[derive(Clone, Copy)]
pub enum CommandStyle {
    Quiet,
    Bordered,
    Primary,
}

pub fn command_button(
    ui: &mut egui::Ui,
    icon_kind: Option<Icon>,
    label: &str,
    enabled: bool,
    style: CommandStyle,
    tint: Option<Color32>,
) -> egui::Response {
    let foreground = if !enabled {
        TEXT_MUTED
    } else if matches!(style, CommandStyle::Primary) {
        Color32::WHITE
    } else {
        TEXT_PRIMARY
    };
    let text = egui::RichText::new(label).size(12.5).color(foreground);
    let button = if let Some(kind) = icon_kind {
        egui::Button::image_and_text(icon(kind).tint(tint.unwrap_or(foreground)), text)
    } else {
        egui::Button::new(text)
    };
    let button = button
        .wrap_mode(egui::TextWrapMode::Extend)
        .min_size(Vec2::new(0.0, CONTROL_H))
        .corner_radius(3.0);
    let button = match style {
        CommandStyle::Quiet => button.frame(false),
        CommandStyle::Bordered => button
            .fill(BG_SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER_INPUT)),
        CommandStyle::Primary => button
            .fill(if enabled { ACCENT } else { BG_SURFACE })
            .stroke(Stroke::NONE),
    };
    ui.add_enabled(enabled, button)
}

pub fn tool_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    command_button(ui, Some(icon), label, enabled, CommandStyle::Quiet, None).on_hover_text(tooltip)
}
pub fn bordered_icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    command_button(ui, Some(icon), label, enabled, CommandStyle::Bordered, None)
        .on_hover_text(tooltip)
}
pub fn primary_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    command_button(ui, Some(icon), label, enabled, CommandStyle::Primary, None)
        .on_hover_text(tooltip)
}

pub fn icon_button(
    ui: &mut egui::Ui,
    icon_kind: Icon,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    let tint = if enabled {
        Color32::from_rgb(212, 214, 222)
    } else {
        TEXT_MUTED
    };
    let button = egui::Button::image(icon_sized(icon_kind, 14.0).tint(tint))
        .frame(false)
        .min_size(Vec2::splat(CONTROL_H));
    ui.add_enabled(enabled, button).on_hover_text(tooltip)
}

pub fn link_button(ui: &mut egui::Ui, icon_kind: Icon, label: &str, url: &str) -> egui::Response {
    let resp = ui
        .scope(|ui| {
            let inactive = &mut ui.style_mut().visuals.widgets.inactive;
            inactive.weak_bg_fill = BG_SURFACE;
            inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_INPUT);
            inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
            ui.add(
                egui::Button::image_and_text(
                    icon_sized(icon_kind, 14.0).tint(TEXT_PRIMARY),
                    egui::RichText::new(label).size(12.5),
                )
                .corner_radius(3.0)
                .min_size(Vec2::new(0.0, CONTROL_H + 2.0)),
            )
        })
        .inner
        .on_hover_text(url)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    resp
}

pub fn bordered_button(ui: &mut egui::Ui, label: &str, min_size: Vec2) -> egui::Response {
    ui.allocate_ui_with_layout(min_size, *ui.layout(), |ui| {
        command_button(ui, None, label, true, CommandStyle::Bordered, None)
    })
    .inner
}

pub fn segmented_toggle(ui: &mut egui::Ui, labels: &[&str], active: usize) -> Option<usize> {
    let mut clicked = None;
    let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for position in 0..labels.len() {
            let index = if rtl {
                labels.len() - 1 - position
            } else {
                position
            };
            if tab_toggle(ui, index == active, labels[index]).clicked() {
                clicked = Some(index);
            }
        }
    });
    clicked
}

pub fn tab_toggle(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    ui.scope(|ui| {
        let hovered = &mut ui.style_mut().visuals.widgets.hovered;
        hovered.bg_stroke = Stroke::new(1.0_f32, if selected { BORDER_HOVER } else { BORDER_LINE });
        let inactive = &mut ui.style_mut().visuals.widgets.inactive;
        if selected {
            inactive.weak_bg_fill = BG_ELEVATED;
            inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_INPUT);
            inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
        } else {
            inactive.weak_bg_fill = Color32::TRANSPARENT;
            inactive.bg_stroke = Stroke::NONE;
            inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_SECONDARY);
        }
        ui.add(
            egui::Button::new(egui::RichText::new(label).size(12.0))
                .wrap_mode(egui::TextWrapMode::Extend)
                .corner_radius(3.0)
                .min_size(Vec2::new(0.0, CONTROL_H)),
        )
    })
    .inner
}

pub fn panel_menu<R>(
    ui: &mut egui::Ui,
    title: impl AsRef<str>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) {
    let label = format!("{}    ", title.as_ref());
    let button = egui::Button::new(egui::RichText::new(label).size(12.5))
        .wrap_mode(egui::TextWrapMode::Extend)
        .corner_radius(3.0)
        .min_size(Vec2::new(0.0, CONTROL_H));
    ui.scope(|ui| {
        ui.style_mut().spacing.interact_size.y = CONTROL_H;
        ui.style_mut().spacing.button_padding = egui::vec2(8.0, 3.0);
        let inactive = &mut ui.style_mut().visuals.widgets.inactive;
        inactive.weak_bg_fill = BG_SURFACE;
        inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_INPUT);
        inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
        let inner = egui::menu::menu_custom_button(ui, button, add_contents);
        let rect = inner.response.rect;
        let cx = rect.right() - 10.0;
        let cy = rect.center().y + 0.5;
        let arrow_color = if inner.response.hovered() || inner.inner.is_some() {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        };
        icon_sized(Icon::ChevronDown, 10.0)
            .tint(arrow_color)
            .paint_at(
                ui,
                egui::Rect::from_center_size(egui::pos2(cx, cy), Vec2::splat(10.0)),
            );
    });
}

pub fn search_input(
    ui: &mut egui::Ui,
    value: &mut String,
    id: &str,
    hint: &str,
    width: f32,
) -> egui::Response {
    ui.push_id(id, |ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(width, CONTROL_H),
            egui::Layout::left_to_right(Align::Center),
            |ui| {
                let response = ui.add_sized(
                    [(width - CONTROL_H - 6.0).max(60.0), CONTROL_H],
                    egui::TextEdit::singleline(value)
                        .id_salt("query")
                        .hint_text(hint),
                );
                if icon_button(ui, Icon::Close, "Очистить поиск", !value.is_empty()).clicked()
                {
                    value.clear();
                    response.request_focus();
                }
                response
            },
        )
        .inner
    })
    .inner
}

#[cfg(feature = "mods")]
pub fn control_group<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .fill(BG_INPUT)
        .corner_radius(3.0)
        .inner_margin(Margin::same(2))
        .show(ui, contents)
}

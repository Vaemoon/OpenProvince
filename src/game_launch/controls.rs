use crate::theme;
use eframe::egui::{self, Align, Color32, Vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Play,
    Clean,
    Path,
    Stop,
}

pub(crate) fn control(ui: &mut egui::Ui, count: usize, running: bool, stopping: bool) -> Action {
    let mut action = Action::None;
    let label = if stopping {
        "Остановка…"
    } else if running {
        "Остановить"
    } else {
        "Играть"
    };
    let icon = if running {
        theme::Icon::Stop
    } else {
        theme::Icon::Play
    };
    let color = if running {
        Color32::from_rgb(222, 127, 127)
    } else {
        Color32::from_rgb(115, 191, 139)
    };
    ui.allocate_ui_with_layout(
        Vec2::new(if running { 132.0 } else { 108.0 }, theme::CONTROL_H),
        egui::Layout::left_to_right(Align::Center),
        |ui| {
            theme::control_group(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if theme::command_button(
                        ui,
                        Some(icon),
                        label,
                        !stopping,
                        theme::CommandStyle::Quiet,
                        Some(color),
                    )
                    .on_hover_text(if running {
                        "Завершить текущую игру"
                    } else {
                        "Запустить игру с выбранным набором модификаций"
                    })
                    .clicked()
                    {
                        action = if running { Action::Stop } else { Action::Play };
                    }
                    ui.add_enabled_ui(!running, |ui| {
                        ui.menu_image_button(
                            theme::icon_sized(theme::Icon::ChevronDown, 10.0)
                                .tint(theme::TEXT_SECONDARY),
                            |ui| {
                                ui.set_min_width(230.0);
                                if ui
                                    .button(format!("Запустить с выбранным набором ({count})"))
                                    .clicked()
                                {
                                    action = Action::Play;
                                    ui.close_menu();
                                }
                                if ui.button("Восстановить оригинал и запустить").clicked()
                                {
                                    action = Action::Clean;
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button("Выбрать Multi Theft Auto.exe…").clicked() {
                                    action = Action::Path;
                                    ui.close_menu();
                                }
                            },
                        )
                        .response
                        .on_hover_text("Варианты запуска");
                    });
                });
            });
        },
    );
    action
}

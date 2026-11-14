use crate::theme;
use eframe::egui::{self, Color32, Margin};
use province_mods::ModCategory;

impl crate::Browser {
    pub(in crate::ui) fn mods_panel_ui(&mut self, ctx: &egui::Context) {
        let mut to_install = false;
        let mut to_install_folder = false;
        let mut to_apply = false;
        let mut to_restore = false;
        let mut to_open_folder = false;
        let mut to_export = false;
        let mut to_delete = false;

        let installed_count = self.mod_manager.registry.mods().len();
        let active_count = self
            .mod_manager
            .registry
            .mods()
            .iter()
            .filter(|m| m.enabled)
            .count();
        egui::TopBottomPanel::top("mods_subnav_panel")
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_CHROME)
                    .inner_margin(Margin::symmetric(10, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if theme::tab_toggle(
                        ui,
                        self.mod_manager.active_subtab == 0,
                        &format!("Установленные ({installed_count})"),
                    )
                    .clicked()
                    {
                        self.mod_manager.active_subtab = 0;
                    }
                    if theme::tab_toggle(ui, self.mod_manager.active_subtab == 1, "Каталог")
                        .clicked()
                    {
                        self.mod_manager.active_subtab = 1;
                    }
                    ui.add_space(14.0);
                    ui.add_enabled_ui(!self.mod_manager.is_busy, |ui| {
                        if theme::tool_button(
                            ui,
                            theme::Icon::Install,
                            "Установить…",
                            "Выбрать пакет модификации",
                            true,
                        )
                        .clicked()
                        {
                            to_install = true;
                        }
                        ui.menu_button("Ещё", |ui| {
                            if ui.button("Установить из папки…").clicked() {
                                to_install_folder = true;
                                ui.close_menu();
                            }
                            if ui.button("Открыть папку модификаций").clicked()
                            {
                                to_open_folder = true;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Восстановить исходные файлы игры").clicked()
                            {
                                to_restore = true;
                                ui.close_menu();
                            }
                        });
                    });
                    if self.mod_manager.active_subtab == 0 {
                        ui.add_space(10.0);
                        if theme::tool_button(
                            ui,
                            theme::Icon::Redo,
                            "Применить",
                            "Применить выбранный набор к архивам игры",
                            !self.mod_manager.is_busy,
                        )
                        .clicked()
                        {
                            to_apply = true;
                        }
                        ui.label(
                            egui::RichText::new(format!("Выбрано: {active_count}"))
                                .small()
                                .color(theme::TEXT_SECONDARY),
                        );
                    }
                });
            });
        if self.mod_manager.banner_message.is_some() {
            egui::TopBottomPanel::top("mods_notice")
                .frame(
                    egui::Frame::new()
                        .fill(theme::BG_PANEL)
                        .inner_margin(Margin::symmetric(12, 8)),
                )
                .show(ctx, |ui| {
                    if let Some((message, is_error)) = self.mod_manager.banner_message.clone() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(message).color(if is_error {
                                Color32::from_rgb(239, 147, 147)
                            } else {
                                theme::TEXT_SECONDARY
                            }));
                            if theme::icon_button(ui, theme::Icon::Close, "Скрыть сообщение", true)
                                .clicked()
                            {
                                self.mod_manager.banner_message = None;
                            }
                        });
                        ui.add_space(12.0);
                    }
                });
        }
        if self.mod_manager.active_subtab == 1 {
            self.catalog_ui(ctx);
        } else {
            egui::SidePanel::left("mods_list_panel")
                .resizable(true)
                .default_width(340.0)
                .min_width(280.0)
                .max_width(520.0)
                .frame(
                    egui::Frame::new()
                        .fill(theme::BG_PANEL)
                        .inner_margin(Margin::ZERO),
                )
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .inner_margin(Margin::symmetric(10, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let width = ui.available_width();
                                theme::search_input(
                                    ui,
                                    &mut self.mod_manager.search_query,
                                    "mods_search",
                                    "Найти модификацию…",
                                    width,
                                );
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Категория")
                                        .small()
                                        .color(theme::TEXT_SECONDARY),
                                );
                                egui::ComboBox::from_id_salt("mods_category")
                                    .selected_text(
                                        self.mod_manager
                                            .category_filter
                                            .as_ref()
                                            .map_or("Все", ModCategory::display_name),
                                    )
                                    .width(150.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.mod_manager.category_filter,
                                            None,
                                            "Все",
                                        );
                                        for category in [
                                            ModCategory::Vehicles,
                                            ModCategory::Skins,
                                            ModCategory::Tuning,
                                            ModCategory::Weapons,
                                            ModCategory::Map,
                                            ModCategory::Audio,
                                            ModCategory::Other,
                                        ] {
                                            let label = category.display_name();
                                            ui.selectable_value(
                                                &mut self.mod_manager.category_filter,
                                                Some(category),
                                                label,
                                            );
                                        }
                                    });
                            });
                        });
                    theme::hline(ui);
                    let query = self.mod_manager.search_query.trim().to_lowercase();
                    let filtered: Vec<_> = self
                        .mod_manager
                        .registry
                        .mods()
                        .iter()
                        .filter(|m| {
                            self.mod_manager
                                .category_filter
                                .as_ref()
                                .is_none_or(|c| c == &m.manifest.category)
                                && (query.is_empty()
                                    || [
                                        &m.manifest.name,
                                        &m.id,
                                        &m.manifest.author,
                                        &m.manifest.description,
                                    ]
                                    .iter()
                                    .any(|s| s.to_lowercase().contains(&query))
                                    || m.manifest
                                        .replaces
                                        .iter()
                                        .any(|r| r.file.to_lowercase().contains(&query)))
                        })
                        .cloned()
                        .collect();
                    let height = (ui.available_height() - 32.0).max(80.0);
                    egui::ScrollArea::vertical()
                        .id_salt("mods_scroll_list")
                        .auto_shrink([false, false])
                        .max_height(height)
                        .min_scrolled_height(height)
                        .show(ui, |ui| {
                            if filtered.is_empty() {
                                egui::Frame::new()
                                    .inner_margin(Margin::same(14))
                                    .show(ui, |ui| {
                                        ui.label(if installed_count == 0 {
                                            "Модификаций пока нет"
                                        } else {
                                            "Ничего не найдено"
                                        });
                                        ui.label(
                                            egui::RichText::new(if installed_count == 0 {
                                                "Установите пакет или перетащите его в окно."
                                            } else {
                                                "Измените запрос или категорию."
                                            })
                                            .color(theme::TEXT_SECONDARY),
                                        );
                                    });
                            }
                            for m in filtered {
                                ui.push_id(&m.id, |ui| {
                                    let selected =
                                        self.mod_manager.selected_mod_id.as_deref() == Some(&m.id);
                                    egui::Frame::new()
                                        .fill(if selected {
                                            theme::SELECTION_BG
                                        } else {
                                            Color32::TRANSPARENT
                                        })
                                        .inner_margin(Margin::symmetric(10, 8))
                                        .show(ui, |ui| {
                                            let row_width = ui.available_width();
                                            ui.set_width(row_width);
                                            ui.horizontal(|ui| {
                                                let mut enabled = m.enabled;
                                                if ui
                                                    .checkbox(&mut enabled, "")
                                                    .on_hover_text("Включить в набор модификаций")
                                                    .changed()
                                                {
                                                    self.mod_manager.toggle_mod(&m.id);
                                                }
                                                ui.vertical(|ui| {
                                                    if ui
                                                        .add(
                                                            egui::Label::new(
                                                                egui::RichText::new(
                                                                    &m.manifest.name,
                                                                )
                                                                .color(theme::TEXT_PRIMARY),
                                                            )
                                                            .truncate()
                                                            .sense(egui::Sense::click()),
                                                        )
                                                        .clicked()
                                                    {
                                                        self.mod_manager.selected_mod_id =
                                                            Some(m.id.clone());
                                                    }
                                                    let metadata = format!(
                                                        "{} · {}",
                                                        m.manifest.category.display_name(),
                                                        m.manifest.author
                                                    );
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(metadata)
                                                                .small()
                                                                .color(theme::TEXT_SECONDARY),
                                                        )
                                                        .truncate(),
                                                    );
                                                    if m.enabled
                                                        && self.mod_manager.conflicts.iter().any(
                                                            |c| {
                                                                c.conflicting_mod_ids
                                                                    .contains(&m.id)
                                                            },
                                                        )
                                                    {
                                                        ui.label(
                                                            egui::RichText::new("Конфликт файлов")
                                                                .small()
                                                                .color(theme::STATUS_MODIFIED),
                                                        );
                                                    }
                                                });
                                            });
                                        });
                                });
                            }
                        });
                    egui::Frame::new()
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{installed_count} установлено · {active_count} выбрано"
                                ))
                                .small()
                                .color(theme::TEXT_SECONDARY),
                            );
                        });
                });
            egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG_PANEL).inner_margin(Margin::same(20))).show(ctx, |ui| {
                let selected = self.mod_manager.selected_mod_id.as_deref().and_then(|id| self.mod_manager.registry.find_mod(id)).cloned();
                if let Some(m) = selected {
                    self.mod_manager.update_preview_texture(ctx, &m);
                    egui::ScrollArea::vertical().id_salt("mod_details_scroll").show(ui, |ui| {
                        ui.label(egui::RichText::new(&m.manifest.name).size(18.0).strong());
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for text in [m.manifest.category.display_name().to_string(), format!("Версия {}", m.manifest.version), m.manifest.author.clone()] {
                                ui.label(egui::RichText::new(text).color(theme::TEXT_SECONDARY)); ui.add_space(8.0);
                            }
                        });
                        ui.add_space(12.0);
                        ui.horizontal_wrapped(|ui| {
                            let mut enabled = m.enabled;
                            if ui.checkbox(&mut enabled, "Включить модификацию").changed() {self.mod_manager.toggle_mod(&m.id);}
                            ui.add_space(12.0);
                            if theme::tool_button(ui, theme::Icon::Archive, "Экспорт…", "Сохранить пакет .opmod", true).clicked() {to_export = true;}
                            ui.menu_button("Ещё", |ui| {
                                if ui.button("Удалить модификацию…").clicked() {to_delete = true; ui.close_menu();}
                            });
                        });
                        ui.add_space(16.0);
                        if let Some((_, texture)) = &self.mod_manager.preview_texture {
                            ui.add(egui::Image::new(texture).max_width(ui.available_width().min(720.0)).max_height(280.0).maintain_aspect_ratio(true));
                            ui.add_space(16.0);
                        }
                        if !m.manifest.description.is_empty() {ui.label(&m.manifest.description); ui.add_space(20.0);}
                        let conflicts: Vec<_> = self.mod_manager.conflicts.iter().filter(|c| c.conflicting_mod_ids.contains(&m.id)).collect();
                        if !conflicts.is_empty() {
                            ui.label(egui::RichText::new("Конфликт файлов").strong().color(theme::STATUS_MODIFIED));
                            ui.label(egui::RichText::new("Эти файлы также заменяются другими включёнными модификациями.").color(theme::TEXT_SECONDARY));
                            for c in conflicts {ui.label(format!("{} — {}", c.file, c.conflicting_mod_ids.iter().filter(|id| **id != m.id).cloned().collect::<Vec<_>>().join(", ")));}
                            ui.add_space(20.0);
                        }
                        ui.label(egui::RichText::new(format!("Заменяемые файлы ({})", m.manifest.replaces.len())).strong());
                        ui.add_space(8.0);
                        egui_extras::TableBuilder::new(ui).id_salt("mod_replacement_table").column(egui_extras::Column::remainder()).column(egui_extras::Column::remainder()).header(theme::HEADER_H, |mut header| {
                            header.col(|ui| {ui.label(egui::RichText::new("Файл").small().color(theme::TEXT_SECONDARY));});
                            header.col(|ui| {ui.label(egui::RichText::new("Архив").small().color(theme::TEXT_SECONDARY));});
                        }).body(|mut body| {
                            for replacement in &m.manifest.replaces {
                                body.row(theme::ROW_H, |mut row| {
                                    row.col(|ui| {ui.add(egui::Label::new(&replacement.file).truncate()).on_hover_text(&replacement.file);});
                                    row.col(|ui| {ui.add(egui::Label::new(egui::RichText::new(replacement.archive.as_deref().unwrap_or("Автоопределение")).color(theme::TEXT_SECONDARY)).truncate());});
                                });
                            }
                        });
                    });
                } else {
                    ui.add_space(24.0);
                    ui.label(egui::RichText::new("Выберите модификацию").size(18.0));
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Здесь появятся описание, предпросмотр и список заменяемых файлов.").color(theme::TEXT_SECONDARY));
                }
            });
        }

        if to_install {
            if let Some(file_path) = rfd::FileDialog::new()
                .set_title("Выберите архив модификации")
                .add_filter(
                    "Архивы модификаций (*.opmod, *.zip, *.rar, *.7z)",
                    &["opmod", "zip", "rar", "7z"],
                )
                .pick_file()
            {
                match self.mod_manager.install_package(&file_path) {
                    Ok(msg) => {
                        self.mod_manager.banner_message = Some((msg, false));
                        self.status = "Мод успешно установлен.".into();
                    }
                    Err(err) => {
                        self.mod_manager.banner_message = Some((err.clone(), true));
                        self.status = err;
                    }
                }
            }
        }

        if to_install_folder {
            if let Some(folder_path) = rfd::FileDialog::new()
                .set_title("Выберите папку с файлами модификации (.dff, .txd, .col, .fxp)")
                .pick_folder()
            {
                match self.mod_manager.install_package(&folder_path) {
                    Ok(msg) => {
                        self.mod_manager.banner_message = Some((msg, false));
                        self.status = "Модификация из папки успешно импортирована.".into();
                    }
                    Err(err) => {
                        self.mod_manager.banner_message = Some((err.clone(), true));
                        self.status = err;
                    }
                }
            }
        }

        if to_apply {
            match self.mod_manager.apply_active_mods() {
                Ok(summary) => {
                    let msg = format!(
                        "Моды успешно применены! Обновлено архивов: {}, заменено файлов: {}, сохранено бэкапов: {} ({} мс)",
                        summary.archives_patched,
                        summary.files_replaced,
                        summary.backups_stored,
                        summary.duration_ms
                    );
                    self.mod_manager.banner_message = Some((msg.clone(), false));
                    self.status = msg;
                }
                Err(err) => {
                    self.mod_manager.banner_message = Some((err.clone(), true));
                    self.status = err;
                }
            }
        }

        if to_restore {
            match self.mod_manager.restore_stock() {
                Ok(summary) => {
                    let msg = format!(
                        "Исходные файлы восстановлены. Заменено файлов: {} в {} архивах ({} мс)",
                        summary.files_replaced, summary.archives_patched, summary.duration_ms
                    );
                    self.mod_manager.banner_message = Some((msg.clone(), false));
                    self.status = msg;
                }
                Err(err) => {
                    self.mod_manager.banner_message = Some((err.clone(), true));
                    self.status = err;
                }
            }
        }

        if to_open_folder {
            self.mod_manager.open_mods_folder();
        }

        if to_export {
            if let Some(mod_id) = &self.mod_manager.selected_mod_id {
                let default_name = format!("{mod_id}.opmod");
                if let Some(dest) = rfd::FileDialog::new()
                    .set_file_name(&default_name)
                    .add_filter("Пакет мода (*.opmod)", &["opmod"])
                    .save_file()
                {
                    match self.mod_manager.export_selected_mod(&dest) {
                        Ok(()) => {
                            let msg = format!("Мод успешно экспортирован в {}", dest.display());
                            self.mod_manager.banner_message = Some((msg.clone(), false));
                            self.status = msg;
                        }
                        Err(err) => {
                            self.mod_manager.banner_message = Some((err.clone(), true));
                            self.status = err;
                        }
                    }
                }
            }
        }

        if to_delete && rfd::MessageDialog::new()
            .set_title("Удаление модификации")
            .set_description("Удалить выбранную модификацию с диска? Чтобы обновить файлы игры после удаления, примените оставшийся набор.")
            .set_buttons(rfd::MessageButtons::YesNo)
            .set_level(rfd::MessageLevel::Warning)
            .show() == rfd::MessageDialogResult::Yes {
            match self.mod_manager.delete_selected_mod() {
                Ok(()) => {
                    let msg = "Модификация успешно удалена.".to_string();
                    self.mod_manager.banner_message = Some((msg.clone(), false));
                    self.status = msg;
                }
                Err(err) => {
                    self.mod_manager.banner_message = Some((err.clone(), true));
                    self.status = err;
                }
            }
        }
    }
}

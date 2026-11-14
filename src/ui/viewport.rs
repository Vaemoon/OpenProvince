use super::{Command, file_icon};
use crate::{Browser, theme};
use eframe::egui::{self, Align, Margin, Stroke, Vec2};

impl Browser {
    pub(super) fn preview_ui(&mut self, ctx: &egui::Context) {
        let standalone = self.standalone_txd.as_ref().and_then(|path| {
            let editor = self.txd_editor.as_ref()?;
            editor.disk_path.as_ref()?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("texture.txd")
                .to_owned();
            Some((path.clone(), name, editor.disk_dirty))
        });
        let selected_entry = if standalone.is_some() {
            None
        } else {
            self.selected.and_then(|sel| {
                if let Some(a) = self.archives.get(sel.archive) {
                    a.entries()
                        .get(sel.index)
                        .map(|e| (sel, e.name.clone(), e.size, self.archive_label(sel.archive)))
                } else if let Some(file) = self.folder_files.get(sel.index) {
                    let name = file
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("файл")
                        .to_owned();
                    let size = std::fs::metadata(&file.path).map(|m| m.len()).unwrap_or(0);
                    let label = file
                        .path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("Папка")
                        .to_owned();
                    Some((sel, name, size, label))
                } else {
                    None
                }
            })
        };
        let paired = selected_entry
            .as_ref()
            .and_then(|(sel, ..)| self.find_paired_asset(*sel));
        let mut jump_to = None;
        let mut trigger_save = false;
        let mut close_standalone = false;
        let has_selected_entry = selected_entry.is_some();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_PANEL)
                    .inner_margin(Margin::same(10)),
            )
            .show(ctx, |ui| {
                ui.painter()
                    .rect_filled(ui.max_rect(), egui::CornerRadius::ZERO, theme::BG_PANEL);
                ui.expand_to_include_rect(ui.max_rect());
                let mut chosen_variant = self.variant_selected;
                if let Some((path, name, dirty)) = standalone {
                    let parent_hint = path
                        .parent()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Внешний TXD".into());
                    let seg_w = 148.0;
                    let save_w = if dirty { 104.0 } else { 0.0 };
                    let close_w = theme::CONTROL_H + 6.0;
                    let right_w = seg_w + save_w + close_w + if dirty { 14.0 } else { 0.0 };
                    let left_w = (ui.available_width() - right_w - 8.0).max(80.0);
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(left_w, theme::CONTROL_H),
                            egui::Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                file_icon(ui, false, "TXD");
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&name).size(14.0).strong(),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(path.display().to_string());
                                if dirty {
                                    ui.label(
                                        egui::RichText::new("изменён")
                                            .size(11.5)
                                            .color(theme::STATUS_MODIFIED),
                                    );
                                }
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("· {parent_hint}"))
                                            .size(12.0)
                                            .color(theme::TEXT_SECONDARY),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(path.display().to_string());
                            },
                        );
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if theme::icon_button(
                                ui,
                                theme::Icon::Close,
                                "Закрыть TXD (Ctrl+W)",
                                !self.busy,
                            )
                            .clicked()
                            {
                                close_standalone = true;
                            }
                            if let Some(tab) = theme::segmented_toggle(
                                ui,
                                &["Просмотр", "Сведения"],
                                usize::from(self.show_details),
                            ) {
                                self.show_details = tab == 1;
                            }
                            if dirty {
                                ui.separator();
                                if theme::primary_button(
                                    ui,
                                    theme::Icon::Save,
                                    "Сохранить",
                                    "Сохранить файл TXD (Ctrl+S)",
                                    !self.busy,
                                )
                                .clicked()
                                {
                                    trigger_save = true;
                                }
                            }
                        });
                    });
                    ui.add_space(4.0);
                    theme::hline(ui);
                    ui.add_space(4.0);
                } else if let Some((_, entry_name, entry_size, arch_label)) = selected_entry {
                    let edit_tag = self
                        .selected
                        .and_then(|sel| self.pending_edit_tag(sel.archive, &entry_name));
                    let kind = entry_name
                        .rsplit('.')
                        .next()
                        .unwrap_or_default()
                        .to_ascii_uppercase();
                    let meta_full = format!("{arch_label} · {}", theme::format_size(entry_size));
                    let seg_w = 148.0;
                    let paired_w = paired.as_ref().map_or(0.0, |(_, paired_name)| {
                        let text_w = ui
                            .painter()
                            .layout_no_wrap(
                                paired_name.clone(),
                                egui::FontId::proportional(12.5),
                                theme::TEXT_PRIMARY,
                            )
                            .size()
                            .x;
                        text_w + 38.0
                    });
                    let right_w = seg_w + paired_w + 8.0;
                    let left_w = (ui.available_width() - right_w - 8.0).max(80.0);

                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(left_w, theme::CONTROL_H),
                            egui::Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                file_icon(ui, false, &kind);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&entry_name).size(14.0).strong(),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&entry_name);
                                if let Some((tag, color)) = edit_tag {
                                    let text = if tag == "ЗАМ" {
                                        "изменён"
                                    } else {
                                        "удаляется"
                                    };
                                    ui.label(egui::RichText::new(text).size(11.5).color(color));
                                }
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("· {meta_full}"))
                                            .size(12.0)
                                            .color(theme::TEXT_SECONDARY),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&meta_full);
                            },
                        );
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if let Some(tab) = theme::segmented_toggle(
                                ui,
                                &["Просмотр", "Сведения"],
                                usize::from(self.show_details),
                            ) {
                                self.show_details = tab == 1;
                            }
                            if let Some((paired_target, paired_name)) = &paired {
                                ui.separator();
                                if theme::tool_button(
                                    ui,
                                    theme::Icon::ScanEye,
                                    paired_name,
                                    &format!("Перейти к парному ресурсу {paired_name}"),
                                    !self.busy,
                                )
                                .clicked()
                                {
                                    jump_to = Some(*paired_target);
                                }
                            }
                        });
                    });
                    ui.add_space(4.0);
                    theme::hline(ui);
                    ui.add_space(4.0);
                } else if let Some(fxp) = &self.fxp_editor {
                    let name = fxp
                        .doc
                        .path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("effects.fxp");
                    let parent_hint = fxp
                        .doc
                        .path
                        .as_ref()
                        .and_then(|p| p.parent())
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("Редактор FXP");
                    let dirty = fxp.doc.dirty;
                    let close_w = theme::CONTROL_H + 6.0;
                    let left_w = (ui.available_width() - close_w - 8.0).max(80.0);
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(left_w, theme::CONTROL_H),
                            egui::Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                file_icon(ui, false, "FXP");
                                ui.add(
                                    egui::Label::new(egui::RichText::new(name).size(14.0).strong())
                                        .truncate(),
                                );
                                if dirty {
                                    ui.label(
                                        egui::RichText::new("изменён")
                                            .size(11.5)
                                            .color(theme::STATUS_MODIFIED),
                                    );
                                }
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("· {parent_hint}"))
                                            .size(12.0)
                                            .color(theme::TEXT_SECONDARY),
                                    )
                                    .truncate(),
                                );
                            },
                        );
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if theme::icon_button(
                                ui,
                                theme::Icon::Close,
                                "Закрыть FXP (Ctrl+W)",
                                !self.busy,
                            )
                            .clicked()
                            {
                                close_standalone = true;
                            }
                        });
                    });
                    ui.add_space(4.0);
                    theme::hline(ui);
                    ui.add_space(4.0);
                } else {
                    let avail = ui.available_rect_before_wrap();
                    let box_w = 420.0_f32.min(avail.width().max(180.0));
                    let box_h = if self.archives.is_empty() {
                        112.0
                    } else {
                        36.0
                    };
                    let center_rect =
                        egui::Rect::from_center_size(avail.center(), Vec2::new(box_w, box_h));
                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(center_rect), |ui| {
                        ui.vertical_centered(|ui| {
                            if self.archives.is_empty() {
                                ui.add(
                                    theme::icon_sized(theme::Icon::Archive, 24.0)
                                        .tint(theme::TEXT_MUTED),
                                );
                                ui.add_space(8.0);
                            }
                            ui.label(
                                egui::RichText::new(if self.archives.is_empty() {
                                    "Откройте IMG-архив, файл TXD или перетащите их в окно"
                                } else {
                                    "Выберите ресурс в списке слева"
                                })
                                .size(13.0)
                                .color(theme::TEXT_SECONDARY),
                            );
                            if self.archives.is_empty() {
                                ui.add_space(12.0);
                                let row_w = 248.0_f32.min(box_w);
                                ui.allocate_ui_with_layout(
                                    Vec2::new(row_w, theme::CONTROL_H + 4.0),
                                    egui::Layout::left_to_right(Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;
                                        if theme::bordered_icon_button(
                                            ui,
                                            theme::Icon::FolderOpen,
                                            "Открыть…",
                                            "Открыть IMG или TXD (Ctrl+O)",
                                            !self.busy,
                                        )
                                        .clicked()
                                        {
                                            self.command(Command::Open, ctx);
                                        }
                                        if theme::bordered_icon_button(
                                            ui,
                                            theme::Icon::Folder,
                                            "Открыть папку…",
                                            "Выбрать папку на диске",
                                            !self.busy,
                                        )
                                        .clicked()
                                        {
                                            self.command(Command::Folder, ctx);
                                        }
                                    },
                                );
                            }
                        });
                    });
                    return;
                }

                if self.show_details {
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new("Копировать").frame(false))
                            .clicked()
                        {
                            ctx.copy_text(self.preview.clone());
                            self.status = "Сведения скопированы в буфер обмена.".into();
                        }
                    });
                    ui.add_space(4.0);
                    egui::Frame::new()
                        .fill(theme::BG_INPUT)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_LINE))
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            egui::ScrollArea::both().show(ui, |ui| {
                                ui.monospace(&self.preview);
                            });
                        });
                } else if let Some(editor) = &mut self.txd_editor {
                    let can_edit = !self.busy
                        && (editor.disk_path.is_some()
                            || self.edits.is_empty()
                            || self.edited_archive == Some(editor.target.archive));
                    editor.show(ui, can_edit);
                } else if self.model.is_some() {
                    if let Some(player) = &mut self.animation_player {
                        player.advance(ctx.input(|i| i.stable_dt));
                        player.ui(ui);
                        ui.add_space(4.0);
                        theme::hline(ui);
                        ui.add_space(4.0);
                    }
                    if self.vinyl_capable {
                        self.ensure_vinyls();
                    }
                    let vinyl_handle = self.vinyl_texture.clone();
                    let mut pick_vinyl: Option<Option<usize>> = None;
                    let mut pick_dir = false;
                    let mut load_external_txd = false;
                    let images = &self.images;
                    let variants = &self.variants;
                    let vinyl_capable = self.vinyl_capable;
                    let vinyl_on_primary = self.vinyl_on_primary;
                    let vinyls = &self.vinyls;
                    let vinyl_selected = self.vinyl_selected;
                    let vinyl_filter = &mut self.vinyl_filter;
                    let player_skin_mode = self.player_skin_mode;
                    let variant_selected = self.variant_selected;
                    let loading_textures = self.loading_textures;
                    let model = self.model.as_mut().expect("checked");

                    let response = model.show(
                        ui,
                        images,
                        variant_selected != 0,
                        vinyl_handle.as_ref(),
                        vinyl_on_primary,
                        |ui| {
                            theme::panel_menu(
                                ui,
                                format!("Текстуры ({})", images.len()),
                                |ui| {
                                    if ui
                                        .button("Подключить внешний .txd…")
                                        .on_hover_text("Наложить словарь для предпросмотра")
                                        .clicked()
                                    {
                                        load_external_txd = true;
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    egui::ScrollArea::vertical()
                                        .max_height(240.0)
                                        .show(ui, |ui| {
                                            for (name, handle) in images {
                                                ui.label(format!(
                                                    "{}   {} × {}",
                                                    name,
                                                    handle.size()[0],
                                                    handle.size()[1]
                                                ));
                                            }
                                        });
                                },
                            );

                            if !variants.is_empty() {
                                let prefix = if player_skin_mode {
                                    "Вариант"
                                } else {
                                    "Окраска"
                                };
                                let current = if chosen_variant == 0 {
                                    "Основная"
                                } else {
                                    &variants[chosen_variant - 1].name
                                };
                                let short = if current.chars().count() > 10 {
                                    format!("{}…", current.chars().take(9).collect::<String>())
                                } else {
                                    current.to_string()
                                };
                                theme::panel_menu(ui, format!("{prefix}: {short}"), |ui| {
                                    variant_options(ui, variants, &mut chosen_variant);
                                });
                            }

                            if vinyl_capable {
                                let vinyl_name = vinyl_selected
                                    .and_then(|i| vinyls.get(i))
                                    .map_or("Нет", |v| v.name.as_str());
                                let short = if vinyl_name.chars().count() > 10 {
                                    format!("{}…", vinyl_name.chars().take(9).collect::<String>())
                                } else {
                                    vinyl_name.to_string()
                                };
                                theme::panel_menu(ui, format!("Винил: {short}"), |ui| {
                                    ui.set_min_width(240.0);
                                    ui.horizontal(|ui| {
                                        ui.add_sized(
                                            [180.0, theme::CONTROL_H],
                                            egui::TextEdit::singleline(vinyl_filter)
                                                .id_salt("vinyl_search")
                                                .hint_text("Найти винил…"),
                                        );
                                        if theme::icon_button(
                                            ui,
                                            theme::Icon::Close,
                                            "Очистить",
                                            !vinyl_filter.is_empty(),
                                        )
                                        .clicked()
                                        {
                                            vinyl_filter.clear();
                                        }
                                    });
                                    ui.separator();
                                    if ui
                                        .selectable_label(vinyl_selected.is_none(), "Без винила")
                                        .clicked()
                                    {
                                        pick_vinyl = Some(None);
                                        ui.close_menu();
                                    }
                                    let query = vinyl_filter.to_lowercase();
                                    let filtered: Vec<(usize, String)> = vinyls
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, v)| {
                                            query.is_empty()
                                                || v.name.to_lowercase().contains(&query)
                                        })
                                        .map(|(i, v)| (i, v.name.clone()))
                                        .collect();
                                    egui::ScrollArea::vertical()
                                        .max_height(260.0)
                                        .show(ui, |ui| {
                                            if filtered.is_empty() {
                                                ui.weak(if vinyls.is_empty() {
                                                    "Винилы не найдены в папке игры."
                                                } else {
                                                    "Нет совпадений."
                                                });
                                            } else {
                                                for (idx, name) in filtered {
                                                    if ui
                                                        .selectable_label(
                                                            vinyl_selected == Some(idx),
                                                            &name,
                                                        )
                                                        .clicked()
                                                    {
                                                        pick_vinyl = Some(Some(idx));
                                                        ui.close_menu();
                                                    }
                                                }
                                            }
                                        });
                                    ui.separator();
                                    if ui
                                        .button(format!("Папка винилов… ({})", vinyls.len()))
                                        .clicked()
                                    {
                                        pick_dir = true;
                                        ui.close_menu();
                                    }
                                });
                            }

                            if loading_textures {
                                theme::vsep(ui);
                                ui.add(egui::Spinner::new().size(12.0));
                                ui.label(
                                    egui::RichText::new("Текстуры…")
                                        .size(11.0)
                                        .color(theme::TEXT_MUTED),
                                );
                            }
                        },
                    );
                    if let Some(response) = response {
                        response.context_menu(|ui| {
                            ui.strong("Варианты текстур");
                            variant_options(ui, &self.variants, &mut chosen_variant);
                        });
                    }
                    if load_external_txd {
                        self.load_txd(ctx);
                    }
                    if pick_dir
                        && let Some(dir) = rfd::FileDialog::new()
                            .set_directory(&self.folder)
                            .pick_folder()
                    {
                        self.vinyl_dir = Some(dir);
                        self.reload_vinyls();
                    }
                    if let Some(choice) = pick_vinyl {
                        self.select_vinyl(choice, ctx);
                    }
                } else if !self.images.is_empty() {
                    egui::ScrollArea::both().show(ui, |ui| {
                        for (name, image) in &self.images {
                            ui.label(format!(
                                "{name} · {} × {}",
                                image.size()[0],
                                image.size()[1]
                            ));
                            ui.add(egui::Image::new(image).max_width(ui.available_width()));
                            ui.add_space(12.0);
                        }
                    });
                } else if !self.model_error.is_empty() {
                    ui.colored_label(
                        theme::STATUS_REMOVED,
                        "Не удалось построить предпросмотр модели",
                    );
                    ui.label(&self.model_error);
                    ui.weak("Исходные байты доступны во вкладке «Сведения».");
                } else if self.loading_model {
                    let avail = ui.available_rect_before_wrap();
                    let center_rect =
                        egui::Rect::from_center_size(avail.center(), Vec2::new(260.0, 56.0));
                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(center_rect), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add(egui::Spinner::new().size(20.0));
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Загрузка модели…")
                                    .size(13.0)
                                    .color(theme::TEXT_SECONDARY),
                            );
                        });
                    });
                } else if !has_selected_entry && self.fxp_editor.is_some() {
                    let fxp = self.fxp_editor.as_mut().expect("checked");
                    fxp.show(ui, ctx);
                } else {
                    ui.monospace(if self.preview.is_empty() {
                        "Выберите DFF или TXD в списке ресурсов."
                    } else {
                        &self.preview
                    });
                }
                if chosen_variant != self.variant_selected {
                    self.select_variant(chosen_variant, ctx);
                }
            });

        if close_standalone {
            self.close_standalone_txd();
        } else if trigger_save {
            self.save_current(ctx);
        } else if let Some(target) = jump_to {
            self.selection.clear();
            self.selection.insert(target);
            self.scroll_selection = true;
            self.select(target, ctx);
        }
    }
}

fn variant_options(ui: &mut egui::Ui, variants: &[crate::preview::Variant], selected: &mut usize) {
    if ui.selectable_value(selected, 0, "Основная").clicked() {
        ui.close_menu();
    }
    for (index, variant) in variants.iter().enumerate() {
        if ui
            .selectable_value(selected, index + 1, &variant.name)
            .clicked()
        {
            ui.close_menu();
        }
    }
}

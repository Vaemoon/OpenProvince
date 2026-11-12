use super::{Command, menu};
use crate::{Browser, Edit, EntryRef, theme};
use eframe::egui::{self, Align, Color32, Margin, Sense, Stroke, Vec2};
use std::path::PathBuf;

impl Browser {
    pub(super) fn top_chrome_ui(
        &mut self,
        ctx: &egui::Context,
        command: &mut Option<Command>,
        menu_jump_to: &mut Option<EntryRef>,
    ) {
        let has_archives = !self.archives.is_empty();
        let selected = !self.selection.is_empty();
        let paired_current = self.selected.and_then(|sel| self.find_paired_asset(sel));
        let standalone_dirty = self.txd_editor.as_ref().is_some_and(|e| e.disk_dirty);
        let standalone_open = self.standalone_txd.is_some()
            && self
                .txd_editor
                .as_ref()
                .is_some_and(|e| e.disk_path.is_some());
        let mut close_archive_idx = None;
        let mut close_standalone = false;

        egui::TopBottomPanel::top("manager_chrome")
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_CHROME)
                    .inner_margin(Margin::symmetric(10, 5)),
            )
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
                    ui.menu_button("Файл", |ui| {
                        menu(ui, "Открыть…    Ctrl+O", true, Command::Open, command);
                        menu(
                            ui,
                            "Добавить во вкладки…",
                            true,
                            Command::AddArchive,
                            command,
                        );
                        menu(ui, "Открыть папку…", true, Command::Folder, command);
                        menu(ui, "Собрать IMG из папки…", true, Command::Pack, command);
                        ui.separator();
                        menu(
                            ui,
                            "Сохранить    Ctrl+S",
                            (has_archives && !self.edits.is_empty()) || standalone_dirty,
                            Command::Save,
                            command,
                        );
                        menu(
                            ui,
                            "Сохранить как…",
                            has_archives || standalone_open,
                            Command::SaveAs,
                            command,
                        );
                        ui.separator();
                        menu(
                            ui,
                            "Закрыть вкладку    Ctrl+W",
                            has_archives || standalone_open,
                            Command::CloseActive,
                            command,
                        );
                    });
                    ui.menu_button("Правка", |ui| {
                        menu(
                            ui,
                            "Добавить файлы…    Insert",
                            has_archives,
                            Command::Add,
                            command,
                        );
                        menu(
                            ui,
                            "Заменить файл…",
                            has_archives && self.selection.len() == 1,
                            Command::Replace,
                            command,
                        );
                        menu(
                            ui,
                            "Удалить    Delete",
                            has_archives && selected,
                            Command::Remove,
                            command,
                        );
                        ui.separator();
                        if ui.button("Найти в списке…    Ctrl+F").clicked() {
                            self.focus_search = true;
                            ui.close_menu();
                        }
                        menu(ui, "Выделить всё    Ctrl+A", true, Command::SelectAll, command);
                        ui.separator();
                        menu(
                            ui,
                            "Сбросить правки",
                            !self.edits.is_empty(),
                            Command::Discard,
                            command,
                        );
                    });
                    ui.menu_button("Вид", |ui| {
                        menu(
                            ui,
                            "Обновить предпросмотр    F3",
                            self.selected.is_some(),
                            Command::Preview,
                            command,
                        );
                        ui.separator();
                        if ui.radio(!self.show_details, "Просмотр").clicked() {
                            self.show_details = false;
                            ui.close_menu();
                        }
                        if ui.radio(self.show_details, "Сведения и Hex").clicked() {
                            self.show_details = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        for (mode, label) in [
                            (0u8, "Фильтр: Все ресурсы"),
                            (1, "Фильтр: Модели (DFF)"),
                            (2, "Фильтр: Текстуры (TXD)"),
                            (3, "Фильтр: Прочие"),
                        ] {
                            if ui.radio(self.type_filter == mode, label).clicked() {
                                self.type_filter = mode;
                                self.row_cache = None;
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                has_archives && self.archives.len() > 1,
                                egui::Checkbox::new(&mut self.all_tabs, "Искать по всем вкладкам"),
                            )
                            .changed()
                        {
                            self.row_cache = None;
                        }
                        if ui
                            .add_enabled(
                                has_archives && self.all_tabs && self.archives.len() > 1,
                                egui::Checkbox::new(
                                    &mut self.group_by_archive,
                                    "Группировать по архивам",
                                ),
                            )
                            .changed()
                        {
                            self.row_cache = None;
                        }
                        if let Some((paired_target, paired_name)) = &paired_current {
                            ui.separator();
                            if ui.button(format!("Перейти к {paired_name}")).clicked() {
                                *menu_jump_to = Some(*paired_target);
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("Архив", |ui| {
                        menu(
                            ui,
                            "Извлечь выбранное…    Ctrl+E",
                            has_archives && selected,
                            Command::Extract,
                            command,
                        );
                        menu(
                            ui,
                            if self.all_tabs && self.archives.len() > 1 {
                                "Извлечь из всех вкладок…"
                            } else {
                                "Извлечь активный архив…"
                            },
                            has_archives,
                            Command::ExtractAll,
                            command,
                        );
                        ui.separator();
                        menu(ui, "Проверить целостность", has_archives, Command::Verify, command);
                        if ui
                            .add_enabled(
                                !self.edits.is_empty(),
                                egui::Button::new("Очередь правок…"),
                            )
                            .clicked()
                        {
                            self.changes_open = !self.changes_open;
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Справка", |ui| {
                        menu(
                            ui,
                            "Горячие клавиши и управление    F1",
                            true,
                            Command::Info,
                            command,
                        );
                        if ui.button("О программе OpenProvince…").clicked() {
                            self.about_open = true;
                            ui.close_menu();
                        }
                    });
                });

                ui.add_space(3.0);
                theme::hline(ui);
                ui.add_space(4.0);
                if has_archives {
                    egui::ScrollArea::horizontal()
                        .id_salt("open_archives_strip")
                        .max_height(28.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                for (ai, archive) in self.archives.iter().enumerate() {
                                    let name = archive
                                        .path()
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy();
                                    let active = ai == self.active_archive && !standalone_open;
                                    let dirty =
                                        self.edited_archive == Some(ai) && !self.edits.is_empty();
                                    let tab_fill = if active {
                                        theme::BG_PANEL
                                    } else {
                                        Color32::TRANSPARENT
                                    };
                                    let resp_frame = egui::Frame::new()
                                        .fill(tab_fill)
                                        .inner_margin(Margin::symmetric(8, 4))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 5.0;
                                                let body = ui
                                                    .horizontal(|ui| {
                                                        ui.spacing_mut().item_spacing.x = 5.0;
                                                        ui.add(
                                                            theme::icon_sized(
                                                                theme::Icon::Archive,
                                                                13.0,
                                                            )
                                                            .tint(if active {
                                                                theme::TEXT_PRIMARY
                                                            } else {
                                                                theme::TEXT_MUTED
                                                            }),
                                                        );
                                                        let title_text = if dirty {
                                                            format!("{name} *")
                                                        } else {
                                                            name.as_ref().to_owned()
                                                        };
                                                        let text = egui::RichText::new(title_text)
                                                            .size(12.5)
                                                            .color(if dirty && !active {
                                                                theme::STATUS_MODIFIED
                                                            } else if active {
                                                                theme::TEXT_PRIMARY
                                                            } else {
                                                                theme::TEXT_SECONDARY
                                                            });
                                                        ui.add(egui::Label::new(text));
                                                        ui.label(
                                                            egui::RichText::new(
                                                                theme::format_thousands(
                                                                    archive.entries().len() as u64,
                                                                ),
                                                            )
                                                            .size(11.0)
                                                            .color(theme::TEXT_MUTED),
                                                        );
                                                    })
                                                    .response
                                                    .interact(Sense::click())
                                                    .on_hover_text(
                                                        archive.path().display().to_string(),
                                                    );
                                                if body.clicked() && !self.busy {
                                                    self.active_archive = ai;
                                                    self.row_cache = None;
                                                    if standalone_open && !standalone_dirty {
                                                        close_standalone = true;
                                                    }
                                                }
                                                if ui
                                                    .add(
                                                        egui::Button::image(
                                                            theme::icon_sized(
                                                                theme::Icon::Close,
                                                                12.0,
                                                            )
                                                            .tint(theme::TEXT_MUTED),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .on_hover_text(format!("Закрыть {name}"))
                                                    .clicked()
                                                {
                                                    close_archive_idx = Some(ai);
                                                }
                                            });
                                        });
                                    if active {
                                        let r = resp_frame.response.rect;
                                        let accent_color = if dirty {
                                            theme::STATUS_MODIFIED
                                        } else {
                                            theme::ACCENT
                                        };
                                        ui.painter().line_segment(
                                            [r.left_top(), r.right_top()],
                                            Stroke::new(2.0_f32, accent_color),
                                        );
                                    }
                                }
                                if let Some(txd_path) = &self.standalone_txd
                                    && standalone_open
                                {
                                    let txd_name = txd_path
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy();
                                    let title = if standalone_dirty {
                                        format!("{txd_name} *")
                                    } else {
                                        txd_name.as_ref().to_owned()
                                    };
                                    let resp_frame = egui::Frame::new()
                                        .fill(theme::BG_PANEL)
                                        .inner_margin(Margin::symmetric(8, 4))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 5.0;
                                                ui.add(
                                                    theme::icon_sized(theme::Icon::Image, 13.0)
                                                        .tint(theme::TEXT_PRIMARY),
                                                );
                                                ui.label(
                                                    egui::RichText::new(title)
                                                        .size(12.5)
                                                        .color(theme::TEXT_PRIMARY),
                                                )
                                                .on_hover_text(txd_path.display().to_string());
                                                if ui
                                                    .add(
                                                        egui::Button::image(
                                                            theme::icon_sized(
                                                                theme::Icon::Close,
                                                                12.0,
                                                            )
                                                            .tint(theme::TEXT_MUTED),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .on_hover_text(format!("Закрыть {txd_name}"))
                                                    .clicked()
                                                {
                                                    close_standalone = true;
                                                }
                                            });
                                        });
                                    let r = resp_frame.response.rect;
                                    let accent_color = if standalone_dirty {
                                        theme::STATUS_MODIFIED
                                    } else {
                                        theme::ACCENT
                                    };
                                    ui.painter().line_segment(
                                        [r.left_top(), r.right_top()],
                                        Stroke::new(2.0_f32, accent_color),
                                    );
                                }
                                if theme::icon_button(
                                    ui,
                                    theme::Icon::Plus,
                                    "Добавить ещё IMG-архивы",
                                    !self.busy,
                                )
                                .clicked()
                                {
                                    *command = Some(Command::AddArchive);
                                }
                            });
                        });
                } else {
                    ui.horizontal(|ui| {
                        if theme::icon_button(
                            ui,
                            theme::Icon::ArrowUp,
                            "На уровень вверх (Alt+Up)",
                            !self.busy,
                        )
                        .clicked()
                        {
                            *command = Some(Command::Up);
                        }
                        let response = ui.add_sized(
                            [(ui.available_width() - 4.0).max(120.0), theme::CONTROL_H],
                            egui::TextEdit::singleline(&mut self.address)
                                .id_salt("folder_address")
                                .hint_text("Путь к папке, IMG или TXD…"),
                        );
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let path = PathBuf::from(self.address.trim_matches('"'));
                            if path.is_dir() {
                                self.browse(path, ctx);
                            } else {
                                self.open(path, ctx);
                            }
                        }
                    });
                }
            });
        if let Some(idx) = close_archive_idx {
            self.close_archive(idx, ctx);
        }
        if close_standalone {
            self.close_standalone_txd();
        }
    }

    pub(super) fn status_and_dialogs_ui(
        &mut self,
        ctx: &egui::Context,
        command: &mut Option<Command>,
    ) {
        let has_archives = !self.archives.is_empty();
        let standalone_dirty = self.txd_editor.as_ref().is_some_and(|e| e.disk_dirty);
        let total_items: usize = if has_archives {
            if self.all_tabs {
                self.archives.iter().map(|a| a.entries().len()).sum()
            } else {
                self.archives
                    .get(self.active_archive)
                    .map_or(0, |a| a.entries().len())
            }
        } else {
            self.folder_files.len()
        };

        egui::TopBottomPanel::bottom("manager_status")
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_CHROME)
                    .inner_margin(Margin::symmetric(10, 4)),
            )
            .show(ctx, |ui| {
                if let Some((done, total)) = self.progress {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [ui.available_width() - 88.0, 16.0],
                            egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                                .text(format!("{done} / {total}")),
                        );
                        if theme::bordered_button(
                            ui,
                            "Отмена",
                            Vec2::new(76.0, theme::CONTROL_H),
                        )
                        .clicked()
                        {
                            self.cancel
                                .store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    });
                    ui.add_space(4.0);
                }
                ui.horizontal(|ui| {
                    if self.busy {
                        ui.spinner();
                    }
                    ui.label(
                        egui::RichText::new(&self.status)
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if standalone_dirty {
                            if theme::primary_button(
                                ui,
                                theme::Icon::Save,
                                "Сохранить TXD",
                                "Сохранить файл TXD (Ctrl+S)",
                                !self.busy,
                            )
                            .clicked()
                            {
                                *command = Some(Command::Save);
                            }
                            ui.separator();
                        }
                        if !self.edits.is_empty() {
                            if theme::tool_button(
                                ui,
                                theme::Icon::Close,
                                "Сбросить",
                                "Отменить все правки",
                                !self.busy,
                            )
                            .clicked()
                            {
                                *command = Some(Command::Discard);
                            }
                            if theme::primary_button(
                                ui,
                                theme::Icon::Save,
                                "Сохранить",
                                "Сохранить правки (Ctrl+S)",
                                !self.busy,
                            )
                            .clicked()
                            {
                                *command = Some(Command::Save);
                            }
                            let label = if self.changes_open {
                                format!("Правки: {} ▲", self.edits.len())
                            } else {
                                format!("Правки: {} ▼", self.edits.len())
                            };
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(label)
                                            .size(12.0)
                                            .color(theme::STATUS_MODIFIED),
                                    )
                                    .frame(false)
                                    .min_size(Vec2::new(0.0, theme::CONTROL_H)),
                                )
                                .on_hover_text("Список ожидающих правок")
                                .clicked()
                            {
                                self.changes_open = !self.changes_open;
                            }
                            ui.separator();
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "Выделено: {}  ·  Всего: {}",
                                self.selection.len(),
                                theme::format_thousands(total_items as u64)
                            ))
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY),
                        );
                    });
                });
            });

        if !self.edits.is_empty() && self.changes_open {
            let mut open = self.changes_open;
            let mut undo = None;
            egui::Window::new(format!("Очередь правок ({})", self.edits.len()))
                .id(egui::Id::new("pending_changes_window"))
                .open(&mut open)
                .default_width(440.0)
                .max_height(300.0)
                .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -38.0])
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        for (i, edit) in self.edits.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let (tag, color, desc) = match edit {
                                    Edit::Replace { name, .. } => {
                                        ("[зам]", theme::STATUS_MODIFIED, name.clone())
                                    }
                                    Edit::Remove { name } => {
                                        ("[удал]", theme::STATUS_REMOVED, name.clone())
                                    }
                                    Edit::Add { source } => (
                                        "[нов]",
                                        theme::STATUS_ADDED,
                                        source.display().to_string(),
                                    ),
                                };
                                ui.label(egui::RichText::new(tag).size(11.5).color(color));
                                ui.add(egui::Label::new(desc).truncate());
                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add(egui::Button::new("Отменить").frame(false)).clicked()
                                    {
                                        undo = Some(i);
                                    }
                                });
                            });
                        }
                    });
                });
            self.changes_open = open;
            if let Some(i) = undo {
                self.edits.remove(i);
            }
        }
    }

    pub(super) fn dialogs_ui(&mut self, ctx: &egui::Context) {
        if self.help_open {
            let mut open = self.help_open;
            egui::Window::new("Справка и горячие клавиши")
                .open(&mut open)
                .default_width(520.0)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("OpenProvince").size(16.0).strong());
                    ui.weak("Архиватор и инспектор ресурсов GTA San Andreas / MTA Province");
                    ui.separator();
                    ui.strong("Навигация и архивы");
                    ui.label("Ctrl+O — открыть IMG или TXD (можно выбрать несколько)\nCtrl+S — сохранить правки с резервной копией\nCtrl+F — фокус на поиск ресурса (Esc — очистить)\nCtrl+E — извлечь выделенные файлы\nInsert — добавить файлы в архив\nDelete — пометить удаление / удалить текстуру в TXD\nF3 / 2×клик — просмотр ресурса\nAlt+Up — подняться на папку вверх");
                    ui.add_space(6.0);
                    ui.strong("3D-вьюпорт (DFF) и TXD-редактор");
                    ui.label("ЛКМ — вращение модели (или сдвиг текстуры)\nПКМ / СКМ / Shift+ЛКМ — панорамирование 3D-сцены\nКолесо мыши — плавный масштаб\nR / 2×клик — сброс камеры (3D) или 1:1 / Вписать (TXD)\nCtrl+Z / Ctrl+Y — отмена и повтор правок текстур TXD");
                    ui.separator();
                    ui.weak("Совет: перетащите несколько .img в окно для открытия во вкладках, или .png/.dds картинки в открытый TXD для замены текстур.");
                });
            self.help_open = open;
        }
        if self.about_open {
            let mut open = self.about_open;
            egui::Window::new("О программе")
                .open(&mut open)
                .resizable(false)
                .default_width(390.0)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("OpenProvince").size(18.0).strong());
                    ui.label(
                        egui::RichText::new(format!("Версия {}", env!("CARGO_PKG_VERSION")))
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY),
                    );
                    ui.add_space(4.0);
                    ui.label("Архиватор и инспектор ресурсов IMG, DFF и TXD для GTA San Andreas и MTA Province.");
                    ui.add_space(8.0);
                    theme::hline(ui);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        theme::link_button(
                            ui,
                            theme::Icon::GitHub,
                            "GitHub",
                            "https://github.com/Vaemoon/OpenProvince",
                        );
                        theme::link_button(ui, theme::Icon::Telegram, "Telegram", "https://t.me/");
                        theme::link_button(ui, theme::Icon::Vk, "ВКонтакте", "https://vk.com/");
                    });
                });
            self.about_open = open;
        }
    }
}

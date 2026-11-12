//! Интерфейс файлового менеджера, вкладок IMG и инспектора ресурсов.
use crate::{Browser, Edit, EntryRef, Message, theme};
use eframe::egui::{self, Align, Color32, Margin, Sense, Stroke, Vec2};
use egui_extras::{Column, TableBuilder};
use province_archive::Profile;
use std::{path::PathBuf, sync::Arc, thread};

#[allow(dead_code)]
pub(super) fn visuals() -> egui::Visuals {
    theme::visuals()
}

pub struct File {
    pub path: PathBuf,
    pub directory: bool,
    pub size: u64,
}
pub(super) struct Rows {
    key: String,
    rows: Arc<Vec<RowItem>>,
}
#[derive(Clone)]
struct EntryRow {
    target: EntryRef,
    name: String,
    kind: String,
    archive_idx: usize,
    archive_stem: String,
    size: u64,
    directory: bool,
}
#[derive(Clone)]
enum RowItem {
    Section { title: String, count: usize },
    Entry(EntryRow),
}
#[derive(Clone, Copy)]
enum Command {
    Open,
    AddArchive,
    OpenFolderImgs,
    Folder,
    Pack,
    Add,
    Extract,
    ExtractAll,
    Replace,
    Remove,
    Save,
    SaveAs,
    CloseActive,
    Preview,
    Verify,
    Up,
    Discard,
    SelectAll,
    Info,
}

impl Browser {
    pub(super) fn browse(&mut self, path: PathBuf, ctx: &egui::Context) {
        if !self.edits.is_empty() {
            self.status = "Сначала сохраните или отмените изменения архива.".into();
            return;
        }
        self.busy = true;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = (|| -> Result<_, String> {
                let mut files = Vec::new();
                for f in std::fs::read_dir(&path).map_err(|e| e.to_string())? {
                    let f = f.map_err(|e| e.to_string())?;
                    let metadata = f.metadata().map_err(|e| e.to_string())?;
                    files.push(File {
                        path: f.path(),
                        directory: metadata.is_dir(),
                        size: metadata.len(),
                    });
                }
                Ok((path, files))
            })();
            let _ = sender.send(Message::Folder(result));
            ctx.request_repaint();
        });
    }
    fn command(&mut self, command: Command, ctx: &egui::Context) {
        if self.busy || self.txd_editor.as_ref().is_some_and(|e| e.busy) {
            self.status = "Дождитесь завершения текущей операции.".into();
            return;
        }
        match command {
            Command::Open => {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("IMG archives", &["img"])
                    .set_directory(&self.folder)
                    .pick_files()
                {
                    self.open_many(paths, false, ctx);
                }
            }
            Command::AddArchive => {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("IMG archives", &["img"])
                    .set_directory(&self.folder)
                    .pick_files()
                {
                    self.open_many(paths, !self.archives.is_empty(), ctx);
                }
            }
            Command::OpenFolderImgs => {
                if let Ok(entries) = std::fs::read_dir(&self.folder) {
                    let mut paths: Vec<PathBuf> = entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("img"))
                        })
                        .collect();
                    paths.sort();
                    if paths.is_empty() {
                        self.status = "В текущей папке нет IMG архивов.".into();
                    } else {
                        self.open_many(paths, false, ctx);
                    }
                }
            }
            Command::Folder => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_directory(&self.folder)
                    .pick_folder()
                {
                    self.browse(path, ctx);
                }
            }
            Command::Pack => {
                if self.edits.is_empty() {
                    self.pack(ctx)
                } else {
                    self.status = "Сначала сохраните изменения.".into();
                }
            }
            Command::Add => {
                if let Some(paths) = rfd::FileDialog::new().pick_files() {
                    self.add_files(paths);
                }
            }
            Command::Extract => self.export(self.selection.is_empty(), ctx),
            Command::ExtractAll => self.export(true, ctx),
            Command::Replace => {
                if let Some(source) = rfd::FileDialog::new().pick_file()
                    && let Some(target) = self.selected
                    && let Some(entry) = self
                        .archives
                        .get(target.archive)
                        .and_then(|a| a.entries().get(target.index))
                {
                    if self.edits.is_empty() {
                        self.active_archive = target.archive;
                    }
                    if self.active_archive == target.archive {
                        self.stage(Edit::Replace {
                            name: entry.name.clone(),
                            source,
                        });
                    } else {
                        self.status =
                            "Сохраните изменения текущего архива перед правкой другого.".into();
                    }
                }
            }
            Command::Remove => {
                let first_arch = self.selection.iter().next().map(|r| r.archive);
                if let Some(arch) = first_arch {
                    if self.edits.is_empty() {
                        self.active_archive = arch;
                    }
                    if self.active_archive != arch
                        || self.selection.iter().any(|r| r.archive != arch)
                    {
                        self.status =
                            "Удаление выполняется в пределах одного архива за раз.".into();
                        return;
                    }
                    let names: Vec<_> = self
                        .archives
                        .get(arch)
                        .map(|a| {
                            a.entries()
                                .iter()
                                .filter(|e| {
                                    self.selection.contains(&EntryRef {
                                        archive: arch,
                                        index: e.index,
                                    })
                                })
                                .map(|e| e.name.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    for name in names {
                        self.stage(Edit::Remove { name });
                    }
                }
            }
            Command::Save => self.save_current(ctx),
            Command::SaveAs => self.save(ctx),
            Command::CloseActive => {
                if !self.archives.is_empty() {
                    self.close_archive(self.active_archive, ctx);
                }
            }
            Command::Preview => {
                if let Some(target) = self.selected {
                    if !self.archives.is_empty() {
                        self.preview_open = true;
                        self.select(target, ctx);
                    } else if let Some(file) = self.folder_files.get(target.index) {
                        let path = file.path.clone();
                        if file.directory {
                            self.browse(path, ctx)
                        } else if path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("img"))
                        {
                            self.open(path, ctx);
                        } else {
                            self.status =
                                "Откройте IMG архив, чтобы просмотреть его ресурсы.".into();
                        }
                    }
                }
            }
            Command::Verify => {
                let Some(archive) = self.primary_archive().cloned() else {
                    return;
                };
                let sender = self.sender.clone();
                let ctx = ctx.clone();
                self.busy = true;
                self.cancel
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                let cancel = self.cancel.clone();
                thread::spawn(move || {
                    let mut warnings = Vec::new();
                    let total = archive.entries().len();
                    for (i, e) in archive.entries().iter().enumerate() {
                        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                            let _ = sender.send(Message::Status("Проверка отменена.".into()));
                            ctx.request_repaint();
                            return;
                        }
                        match archive.read(e.index, &Profile::inspected_client()) {
                            Ok(bytes) => {
                                if let Some(w) = province_archive::inspect(&e.name, &bytes).warning
                                {
                                    warnings.push(format!("{}: {w}", e.name));
                                }
                            }
                            Err(error) => warnings.push(format!("{}: {error}", e.name)),
                        }
                        if i % 32 == 0 {
                            let _ = sender.send(Message::Progress(i, total));
                            ctx.request_repaint();
                        }
                    }
                    let message = if warnings.is_empty() {
                        format!("Проверено {total} файлов. Ошибок структуры не найдено.")
                    } else {
                        format!(
                            "Проверено {total} файлов; предупреждений: {}. {}",
                            warnings.len(),
                            warnings
                                .iter()
                                .take(4)
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("; ")
                        )
                    };
                    let _ = sender.send(Message::Status(message));
                    ctx.request_repaint();
                });
            }
            Command::Up => {
                let path = if let Some(a) = self.archives.first() {
                    a.path().parent().map(|p| p.to_owned())
                } else {
                    self.folder.parent().map(|p| p.to_owned())
                };
                if let Some(path) = path {
                    self.browse(path, ctx);
                }
            }
            Command::Discard => {
                self.edits.clear();
                self.changes_open = false;
                if self.type_filter == 4 {
                    self.type_filter = 0;
                }
                if let Some(target) = self.selected {
                    self.select(target, ctx);
                }
            }
            Command::SelectAll => {
                if let Some(cache) = &self.row_cache {
                    self.selection = cache
                        .rows
                        .iter()
                        .filter_map(|item| match item {
                            RowItem::Entry(row) => Some(row.target),
                            RowItem::Section { .. } => None,
                        })
                        .collect();
                } else if self.archives.is_empty() {
                    self.selection = (0..self.folder_files.len())
                        .map(|index| EntryRef { archive: 0, index })
                        .collect();
                } else {
                    self.selection = self
                        .archives
                        .iter()
                        .enumerate()
                        .flat_map(|(ai, a)| {
                            (0..a.entries().len()).map(move |ei| EntryRef {
                                archive: ai,
                                index: ei,
                            })
                        })
                        .collect();
                }
            }
            Command::Info => {
                self.help_open = true;
            }
        }
    }
    fn pending_edit_tag(&self, archive_idx: usize, name: &str) -> Option<(&'static str, Color32)> {
        if self.edits.is_empty() || archive_idx != self.active_archive {
            return None;
        }
        self.edits.iter().rev().find_map(|edit| match edit {
            Edit::Replace { name: target, .. } if target.eq_ignore_ascii_case(name) => {
                Some(("ЗАМ", theme::STATUS_MODIFIED))
            }
            Edit::Remove { name: target } if target.eq_ignore_ascii_case(name) => {
                Some(("УДАЛ", theme::STATUS_REMOVED))
            }
            _ => None,
        })
    }
    fn find_paired_asset(&self, target: EntryRef) -> Option<(EntryRef, String)> {
        let current_name = self
            .archives
            .get(target.archive)?
            .entries()
            .get(target.index)?
            .name
            .as_str();
        let (stem, ext) = current_name.rsplit_once('.')?;
        let wanted_ext = if ext.eq_ignore_ascii_case("dff") {
            "txd"
        } else if ext.eq_ignore_ascii_case("txd") {
            "dff"
        } else {
            return None;
        };
        let wanted_name = format!("{stem}.{wanted_ext}");
        let mut candidates = Vec::with_capacity(self.archives.len());
        candidates.push(target.archive);
        for ai in 0..self.archives.len() {
            if ai != target.archive {
                candidates.push(ai);
            }
        }
        for ai in candidates {
            if let Some(entry) = self.archives[ai]
                .entries()
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(&wanted_name))
            {
                return Some((
                    EntryRef {
                        archive: ai,
                        index: entry.index,
                    },
                    entry.name.clone(),
                ));
            }
        }
        None
    }
    fn filtered_rows(&mut self) -> Arc<Vec<RowItem>> {
        let archive_ptrs: Vec<usize> = self
            .archives
            .iter()
            .map(|a| Arc::as_ptr(a) as usize)
            .collect();
        let type_filter = if self.archives.is_empty() {
            0
        } else {
            self.type_filter
        };
        let cache_key = format!(
            "{:?}|{}|{}|{}|{}|{}|{}|{}|{:?}",
            archive_ptrs,
            self.active_archive,
            self.address,
            self.group_by_archive,
            self.filter,
            type_filter,
            self.sort,
            self.descending,
            self.edits
        );
        if let Some(cache) = &self.row_cache
            && cache.key == cache_key
        {
            cache.rows.clone()
        } else {
            let filter = self.filter.to_lowercase();
            let multi = self.archives.len() > 1;
            let mut entries_list: Vec<EntryRow> = if !self.archives.is_empty() {
                let mut list = Vec::new();
                for (ai, arch) in self.archives.iter().enumerate() {
                    let arch_name = self.archive_label(ai);
                    let arch_stem = arch_name
                        .strip_suffix(".img")
                        .or_else(|| arch_name.strip_suffix(".IMG"))
                        .unwrap_or(&arch_name)
                        .to_owned();
                    for e in arch.entries() {
                        if !filter.is_empty() && !e.name.to_lowercase().contains(&filter) {
                            continue;
                        }
                        let kind = e.name.rsplit('.').next().unwrap_or_default().to_uppercase();
                        let matches_type = match type_filter {
                            1 => kind == "DFF",
                            2 => kind == "TXD",
                            3 => kind != "DFF" && kind != "TXD",
                            4 => self.pending_edit_tag(ai, &e.name).is_some(),
                            _ => true,
                        };
                        if !matches_type {
                            continue;
                        }
                        list.push(EntryRow {
                            target: EntryRef {
                                archive: ai,
                                index: e.index,
                            },
                            name: e.name.clone(),
                            kind,
                            archive_idx: ai,
                            archive_stem: arch_stem.clone(),
                            size: e.size,
                            directory: false,
                        });
                    }
                }
                list
            } else {
                self.folder_files
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| {
                        filter.is_empty()
                            || f.path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_lowercase()
                                .contains(&filter)
                    })
                    .map(|(index, f)| EntryRow {
                        target: EntryRef { archive: 0, index },
                        name: f
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        kind: if f.directory {
                            "Папка".into()
                        } else {
                            f.path
                                .extension()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_uppercase()
                        },
                        archive_idx: 0,
                        archive_stem: String::new(),
                        size: f.size,
                        directory: f.directory,
                    })
                    .collect()
            };

            let grouped = !self.archives.is_empty() && multi && self.group_by_archive;
            let sort_col = self.sort;
            let desc = self.descending;
            entries_list.sort_by(|a, b| {
                if grouped {
                    let arch_cmp = if sort_col == 1 && desc {
                        b.archive_idx.cmp(&a.archive_idx)
                    } else {
                        a.archive_idx.cmp(&b.archive_idx)
                    };
                    if arch_cmp != std::cmp::Ordering::Equal {
                        return arch_cmp;
                    }
                }
                b.directory.cmp(&a.directory).then_with(|| {
                    let order = match sort_col {
                        1 => a
                            .archive_idx
                            .cmp(&b.archive_idx)
                            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
                        2 => a
                            .kind
                            .cmp(&b.kind)
                            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
                        3 => a.size.cmp(&b.size),
                        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    };
                    if desc { order.reverse() } else { order }
                })
            });

            let mut built = Vec::with_capacity(entries_list.len() + self.archives.len());
            if grouped {
                let mut start = 0;
                while start < entries_list.len() {
                    let arch_idx = entries_list[start].archive_idx;
                    let mut end = start + 1;
                    while end < entries_list.len() && entries_list[end].archive_idx == arch_idx {
                        end += 1;
                    }
                    built.push(RowItem::Section {
                        title: self.archive_label(arch_idx),
                        count: end - start,
                    });
                    for item in entries_list[start..end].iter().cloned() {
                        built.push(RowItem::Entry(item));
                    }
                    start = end;
                }
            } else {
                for item in entries_list {
                    built.push(RowItem::Entry(item));
                }
            }

            let rows = Arc::new(built);
            self.row_cache = Some(Rows {
                key: cache_key,
                rows: rows.clone(),
            });
            rows
        }
    }
    pub(super) fn manager_ui(&mut self, ctx: &egui::Context) {
        if self.edits.is_empty() {
            self.changes_open = false;
        }
        let mut command = None;
        if !self.busy {
            let keys = [
                (egui::Modifiers::CTRL, egui::Key::O, Command::Open),
                (egui::Modifiers::CTRL, egui::Key::S, Command::Save),
                (egui::Modifiers::CTRL, egui::Key::W, Command::CloseActive),
                (egui::Modifiers::CTRL, egui::Key::E, Command::Extract),
                (egui::Modifiers::CTRL, egui::Key::A, Command::SelectAll),
                (egui::Modifiers::NONE, egui::Key::F1, Command::Info),
                (egui::Modifiers::NONE, egui::Key::F3, Command::Preview),
                (egui::Modifiers::NONE, egui::Key::Enter, Command::Preview),
                (egui::Modifiers::NONE, egui::Key::Insert, Command::Add),
                (egui::Modifiers::NONE, egui::Key::Delete, Command::Remove),
                (egui::Modifiers::ALT, egui::Key::ArrowUp, Command::Up),
            ];
            for (modifiers, key, action) in keys {
                if (!ctx.wants_keyboard_input()
                    || (modifiers == egui::Modifiers::CTRL && key != egui::Key::A))
                    && ctx.input_mut(|i| i.consume_key(modifiers, key))
                {
                    command = Some(action);
                }
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F)) {
                self.focus_search = true;
            }
        }
        if (!self.edits.is_empty() || self.txd_editor.as_ref().is_some_and(|e| e.busy))
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.status =
                "Есть несохранённые изменения: нажмите «Сохранить» или «Отменить изменения»."
                    .into();
        }
        let has_archives = !self.archives.is_empty();
        let selected = !self.selection.is_empty();
        let paired_current = self.selected.and_then(|sel| self.find_paired_asset(sel));
        let mut menu_jump_to = None;
        let mut close_archive_idx = None;
        egui::TopBottomPanel::top("manager_chrome")
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_CHROME)
                    .inner_margin(Margin::symmetric(10, 5)),
            )
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("Файл", |ui| {
                        menu(
                            ui,
                            "Открыть архив IMG…    Ctrl+O",
                            true,
                            Command::Open,
                            &mut command,
                        );
                        menu(
                            ui,
                            "Добавить архивы во вкладки…",
                            true,
                            Command::AddArchive,
                            &mut command,
                        );
                        menu(
                            ui,
                            "Открыть все IMG из папки…",
                            true,
                            Command::OpenFolderImgs,
                            &mut command,
                        );
                        ui.separator();
                        menu(ui, "Обзор папки на диске…", true, Command::Folder, &mut command);
                        menu(
                            ui,
                            "Собрать новый IMG из папки…",
                            true,
                            Command::Pack,
                            &mut command,
                        );
                        ui.separator();
                        menu(
                            ui,
                            "Сохранить изменения    Ctrl+S",
                            has_archives && !self.edits.is_empty(),
                            Command::Save,
                            &mut command,
                        );
                        menu(
                            ui,
                            "Сохранить архив как…",
                            has_archives,
                            Command::SaveAs,
                            &mut command,
                        );
                        ui.separator();
                        menu(
                            ui,
                            "Закрыть активный архив    Ctrl+W",
                            has_archives,
                            Command::CloseActive,
                            &mut command,
                        );
                    });
                    ui.menu_button("Правка", |ui| {
                        menu(
                            ui,
                            "Добавить файлы в архив…    Insert",
                            has_archives,
                            Command::Add,
                            &mut command,
                        );
                        menu(
                            ui,
                            "Заменить выбранный файл…",
                            has_archives && self.selection.len() == 1,
                            Command::Replace,
                            &mut command,
                        );
                        menu(
                            ui,
                            "Удалить выбранное    Delete",
                            has_archives && selected,
                            Command::Remove,
                            &mut command,
                        );
                        ui.separator();
                        if ui.button("Найти в списке…    Ctrl+F").clicked() {
                            self.focus_search = true;
                            ui.close_menu();
                        }
                        menu(
                            ui,
                            "Выделить всё    Ctrl+A",
                            true,
                            Command::SelectAll,
                            &mut command,
                        );
                        ui.separator();
                        menu(
                            ui,
                            "Отменить все несохранённые правки",
                            !self.edits.is_empty(),
                            Command::Discard,
                            &mut command,
                        );
                    });
                    ui.menu_button("Вид", |ui| {
                        menu(
                            ui,
                            "Обновить предпросмотр    F3",
                            self.selected.is_some(),
                            Command::Preview,
                            &mut command,
                        );
                        if ui.radio(!self.show_details, "Режим: Просмотр").clicked() {
                            self.show_details = false;
                            ui.close_menu();
                        }
                        if ui.radio(self.show_details, "Режим: Сведения и Hex").clicked() {
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
                        if ui
                            .add_enabled(
                                has_archives && self.archives.len() > 1,
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
                                menu_jump_to = Some(*paired_target);
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
                            &mut command,
                        );
                        menu(
                            ui,
                            "Извлечь весь архив…",
                            has_archives,
                            Command::ExtractAll,
                            &mut command,
                        );
                        ui.separator();
                        menu(
                            ui,
                            "Проверить целостность архива",
                            has_archives,
                            Command::Verify,
                            &mut command,
                        );
                        if ui
                            .add_enabled(
                                !self.edits.is_empty(),
                                egui::Button::new("Очередь несохранённых правок…"),
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
                            &mut command,
                        );
                    });
                });

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
                                    let active = ai == self.active_archive;
                                    let dirty = active && !self.edits.is_empty();
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
                                                ui.add(
                                                    theme::icon_sized(theme::Icon::Archive, 13.0)
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
                                                    .color(if active {
                                                        theme::TEXT_PRIMARY
                                                    } else {
                                                        theme::TEXT_SECONDARY
                                                    });
                                                let resp = ui
                                                    .add(egui::Label::new(text).sense(Sense::click()))
                                                    .on_hover_text(archive.path().display().to_string());
                                                if resp.clicked() && !self.busy {
                                                    if self.edits.is_empty() {
                                                        self.active_archive = ai;
                                                    } else if !active {
                                                        self.status = "Сначала сохраните (Ctrl+S) или отмените правки активного архива.".into();
                                                    }
                                                }
                                                ui.label(
                                                    egui::RichText::new(theme::format_thousands(
                                                        archive.entries().len() as u64,
                                                    ))
                                                    .size(11.0)
                                                    .color(theme::TEXT_MUTED),
                                                );
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
                                if theme::icon_button(
                                    ui,
                                    theme::Icon::Plus,
                                    "Добавить ещё IMG-архивы",
                                    !self.busy,
                                )
                                .clicked()
                                {
                                    command = Some(Command::AddArchive);
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
                            command = Some(Command::Up);
                        }
                        let open_imgs_w = 132.0;
                        let response = ui.add_sized(
                            [
                                (ui.available_width() - open_imgs_w - 8.0).max(120.0),
                                theme::CONTROL_H,
                            ],
                            egui::TextEdit::singleline(&mut self.address)
                                .id_salt("folder_address")
                                .hint_text("Путь к папке или IMG архиву…"),
                        );
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let path = PathBuf::from(self.address.trim_matches('"'));
                            if path.is_dir() {
                                self.browse(path, ctx);
                            } else {
                                self.open(path, ctx);
                            }
                        }
                        if ui
                            .add_sized(
                                [open_imgs_w, theme::CONTROL_H],
                                egui::Button::new("Открыть все IMG"),
                            )
                            .on_hover_text("Открыть все IMG из текущей папки")
                            .clicked()
                        {
                            command = Some(Command::OpenFolderImgs);
                        }
                    });
                }
            });
        if let Some(idx) = close_archive_idx {
            self.close_archive(idx, ctx);
        }

        let total_items: usize = if has_archives {
            self.archives.iter().map(|a| a.entries().len()).sum()
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
                            [ui.available_width() - 90.0, 16.0],
                            egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                                .text(format!("{done} / {total}")),
                        );
                        if ui.button("Отмена").clicked() {
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
                                command = Some(Command::Discard);
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
                                command = Some(Command::Save);
                            }
                            let label = if self.changes_open {
                                format!("Правки: {} ▲", self.edits.len())
                            } else {
                                format!("Правки: {} ▾", self.edits.len())
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
                                    if ui.add(egui::Button::new("Отменить").frame(false)).clicked() {
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

        let mut clicked = None;
        let mut double = false;
        let mut rendered_rows = Arc::new(Vec::new());
        let multi = self.archives.len() > 1;
        egui::SidePanel::left("resource_files")
            .resizable(true)
            .default_width(540.0)
            .min_width(380.0)
            .max_width(960.0)
            .frame(egui::Frame::new().fill(theme::BG_PANEL).inner_margin(0.0))
            .show(ctx, |ui| {
                let search_resp = search_field(
                    ui,
                    &mut self.filter,
                    &mut self.group_by_archive,
                    has_archives && multi,
                );
                if self.focus_search {
                    search_resp.request_focus();
                    self.focus_search = false;
                }
                if search_resp.has_focus()
                    && !self.filter.is_empty()
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    self.filter.clear();
                }

                let rows = self.filtered_rows();
                let visible_entries = rows
                    .iter()
                    .filter(|r| matches!(r, RowItem::Entry(_)))
                    .count();
                if has_archives {
                    egui::Frame::new()
                        .inner_margin(Margin::symmetric(8, 2))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for (mode, label) in [
                                    (0u8, "Все"),
                                    (1, "DFF"),
                                    (2, "TXD"),
                                    (3, "Прочие"),
                                ] {
                                    if theme::tab_toggle(ui, self.type_filter == mode, label)
                                        .clicked()
                                    {
                                        self.type_filter = mode;
                                        self.row_cache = None;
                                    }
                                }
                                if !self.edits.is_empty()
                                    && theme::tab_toggle(
                                        ui,
                                        self.type_filter == 4,
                                        &format!("Правки ({})", self.edits.len()),
                                    )
                                    .clicked()
                                {
                                    self.type_filter = if self.type_filter == 4 { 0 } else { 4 };
                                    self.row_cache = None;
                                }
                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(theme::format_thousands(
                                            visible_entries as u64,
                                        ))
                                        .size(11.5)
                                        .color(theme::TEXT_MUTED),
                                    );
                                });
                            });
                        });
                    ui.add_space(2.0);
                    theme::hline(ui);
                }

                let rows = self.filtered_rows();
                rendered_rows = rows.clone();
                let mut arrow = None;
                if !ctx.wants_keyboard_input() {
                    if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        arrow = Some(1isize);
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        arrow = Some(-1isize);
                    }
                }
                let selectable_indices: Vec<usize> = rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, item)| matches!(item, RowItem::Entry(_)).then_some(i))
                    .collect();
                if let Some(delta) = arrow
                    && !selectable_indices.is_empty()
                {
                    let cur_pos = self
                        .selected
                        .and_then(|sel| {
                            selectable_indices.iter().position(
                                |&ri| matches!(&rows[ri], RowItem::Entry(e) if e.target == sel),
                            )
                        })
                        .unwrap_or(0);
                    let next_pos = cur_pos
                        .saturating_add_signed(delta)
                        .min(selectable_indices.len() - 1);
                    if let RowItem::Entry(e) = &rows[selectable_indices[next_pos]] {
                        clicked = Some(e.target);
                        self.scroll_selection = true;
                    }
                }
                let scroll = self
                    .scroll_selection
                    .then(|| {
                        self.selected.and_then(|sel| {
                            rows.iter()
                                .position(|r| matches!(r, RowItem::Entry(e) if e.target == sel))
                        })
                    })
                    .flatten();
                self.scroll_selection = false;
                let table = TableBuilder::new(ui).striped(false);
                let table = if let Some(row) = scroll {
                    table.scroll_to_row(row, Some(Align::Center))
                } else {
                    table
                };
                let mut table = table
                    .striped(false)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(Align::Center))
                    .sense(Sense::click())
                    .column(Column::remainder().at_least(150.0).clip(true).resizable(false));
                if multi {
                    table = table.column(Column::initial(112.0).at_least(76.0).clip(true));
                }
                table = table
                    .column(Column::initial(54.0).at_least(44.0).clip(true))
                    .column(Column::initial(84.0).at_least(68.0).clip(true));

                let mut cols: Vec<(&str, usize)> = vec![("Имя", 0)];
                if multi {
                    cols.push(("Архив", 1));
                }
                cols.extend([("Тип", 2), ("Размер", 3)]);

                table
                    .header(theme::HEADER_H, |mut header| {
                        for &(label, col_id) in &cols {
                            header.col(|ui| {
                                let title = if col_id < 10 && self.sort == col_id {
                                    if self.descending {
                                        format!("{label} ▼")
                                    } else {
                                        format!("{label} ▲")
                                    }
                                } else {
                                    label.to_owned()
                                };
                                let btn_fn = |ui: &mut egui::Ui| {
                                    let resp = ui.selectable_label(
                                        self.sort == col_id,
                                        egui::RichText::new(title).size(12.0).color(
                                            if self.sort == col_id {
                                                theme::TEXT_PRIMARY
                                            } else {
                                                theme::TEXT_SECONDARY
                                            },
                                        ),
                                    );
                                    if resp.clicked() && col_id < 10 {
                                        if self.sort == col_id {
                                            self.descending = !self.descending;
                                        } else {
                                            self.sort = col_id;
                                            self.descending = false;
                                        }
                                        self.row_cache = None;
                                    }
                                };
                                if col_id == 3 {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(Align::Center),
                                        |ui| {
                                            ui.add_space(12.0);
                                            btn_fn(ui);
                                        },
                                    );
                                } else {
                                    btn_fn(ui);
                                }
                            });
                        }
                    })
                    .body(|mut body| {
                        if rows.is_empty() {
                            body.row(64.0, |mut row| {
                                row.col(|ui| {
                                    ui.add_space(10.0);
                                    ui.weak(if self.filter.is_empty() && self.type_filter == 0 {
                                        "В этой папке пока нет элементов"
                                    } else {
                                        "Ничего не найдено по текущему фильтру"
                                    });
                                });
                            });
                            return;
                        }
                        body.rows(theme::ROW_H, rows.len(), |mut row| match &rows[row.index()] {
                            RowItem::Section { title, count, .. } => {
                                row.col(|ui| {
                                    theme::section_bar(ui, title, *count);
                                });
                                for _ in 0..if multi { 3 } else { 2 } {
                                    row.col(|_| {});
                                }
                            }
                            RowItem::Entry(item) => {
                                let is_selected = self.selection.contains(&item.target);
                                row.set_selected(is_selected);
                                let edit_tag = self.pending_edit_tag(item.archive_idx, &item.name);
                                row.col(|ui| {
                                    ui.add_space(6.0);
                                    file_icon(ui, item.directory, &item.kind);
                                    let name_color = if let Some((_, color)) = edit_tag {
                                        color
                                    } else if is_selected {
                                        Color32::WHITE
                                    } else {
                                        theme::TEXT_PRIMARY
                                    };
                                    let label_resp = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&item.name).color(name_color),
                                        )
                                        .truncate(),
                                    );
                                    if item.name.len() > 22 {
                                        label_resp.on_hover_text(&item.name);
                                    }
                                    if let Some((tag, color)) = edit_tag {
                                        let suffix = if tag == "ЗАМ" { "[зам]" } else { "[удал]" };
                                        ui.label(egui::RichText::new(suffix).size(11.0).color(color));
                                    }
                                });
                                if multi {
                                    row.col(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&item.archive_stem)
                                                    .size(12.0)
                                                    .color(theme::TEXT_SECONDARY),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(self.archive_label(item.archive_idx));
                                    });
                                }
                                row.col(|ui| {
                                    ui.label(
                                        egui::RichText::new(if item.directory {
                                            "Папка"
                                        } else {
                                            &item.kind
                                        })
                                        .size(11.5)
                                        .color(theme::TEXT_SECONDARY),
                                    );
                                });
                                row.col(|ui| {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(Align::Center),
                                        |ui| {
                                            ui.add_space(12.0);
                                            if !item.directory {
                                                ui.label(
                                                    egui::RichText::new(theme::format_size(
                                                        item.size,
                                                    ))
                                                    .size(12.0)
                                                    .color(theme::TEXT_SECONDARY),
                                                )
                                                .on_hover_text(format!(
                                                    "{} Б",
                                                    theme::format_thousands(item.size)
                                                ));
                                            }
                                        },
                                    );
                                });
                                let response = row.response();
                                if response.clicked() || response.double_clicked() {
                                    clicked = Some(item.target);
                                    double = response.double_clicked();
                                }
                                let paired = self.find_paired_asset(item.target);
                                response.context_menu(|ui| {
                                    if !self.selection.contains(&item.target) {
                                        self.selection.clear();
                                        self.selection.insert(item.target);
                                        self.selected = Some(item.target);
                                    }
                                    menu(
                                        ui,
                                        "Просмотр    F3",
                                        true,
                                        Command::Preview,
                                        &mut command,
                                    );
                                    if let Some((paired_target, paired_name)) = &paired
                                        && ui.button(format!("Перейти к {paired_name}")).clicked()
                                    {
                                        clicked = Some(*paired_target);
                                        self.scroll_selection = true;
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    menu(
                                        ui,
                                        "Извлечь…    Ctrl+E",
                                        has_archives,
                                        Command::Extract,
                                        &mut command,
                                    );
                                    menu(
                                        ui,
                                        "Заменить…",
                                        has_archives && self.selection.len() == 1,
                                        Command::Replace,
                                        &mut command,
                                    );
                                    menu(
                                        ui,
                                        "Удалить    Delete",
                                        has_archives,
                                        Command::Remove,
                                        &mut command,
                                    );
                                });
                            }
                        });
                    });
            });
        let rows = rendered_rows;
        let selectable_indices: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| matches!(r, RowItem::Entry(_)).then_some(i))
            .collect();
        if let Some(target) = menu_jump_to {
            self.selection.clear();
            self.selection.insert(target);
            self.scroll_selection = true;
            self.select(target, ctx);
        } else if let Some(target) = clicked {
            let modifiers = ctx.input(|i| i.modifiers);
            if modifiers.shift {
                let anchor = self
                    .selected
                    .and_then(|sel| {
                        selectable_indices.iter().position(
                            |&ri| matches!(&rows[ri], RowItem::Entry(e) if e.target == sel),
                        )
                    })
                    .unwrap_or(0);
                let position = selectable_indices
                    .iter()
                    .position(|&ri| matches!(&rows[ri], RowItem::Entry(e) if e.target == target))
                    .unwrap_or(0);
                if !modifiers.ctrl {
                    self.selection.clear();
                }
                for &ri in &selectable_indices[anchor.min(position)..=anchor.max(position)] {
                    if let RowItem::Entry(e) = &rows[ri] {
                        self.selection.insert(e.target);
                    }
                }
            } else if modifiers.ctrl {
                if !self.selection.remove(&target) {
                    self.selection.insert(target);
                }
            } else {
                self.selection.clear();
                self.selection.insert(target);
            }
            self.selected = Some(target);
            if double {
                command = Some(Command::Preview);
            } else if self.preview_open && has_archives {
                self.select(target, ctx);
            }
        }
        if let Some(command) = command {
            self.command(command, ctx);
        }
        self.preview_ui(ctx);
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
                    ui.label("Ctrl+O — открыть один или несколько IMG\nCtrl+S — сохранить правки с резервной копией\nCtrl+F — фокус на поиск ресурса (Esc — очистить)\nCtrl+E — извлечь выделенные файлы\nInsert — добавить файлы в архив\nDelete — пометить удаление / удалить текстуру в TXD\nF3 / 2×клик — просмотр ресурса\nAlt+Up — подняться на папку вверх");
                    ui.add_space(6.0);
                    ui.strong("3D-вьюпорт (DFF) и TXD-редактор");
                    ui.label("ЛКМ — вращение модели (или сдвиг текстуры)\nПКМ / СКМ / Shift+ЛКМ — панорамирование 3D-сцены\nКолесо мыши — плавный масштаб\nR / 2×клик — сброс камеры (3D) или 1:1 / Вписать (TXD)\nCtrl+Z / Ctrl+Y — отмена и повтор правок текстур TXD");
                    ui.separator();
                    ui.weak("Совет: перетащите .img файлы в окно для открытия, или .png/.dds картинки прямо в открытый TXD для замены/добавления текстур.");
                });
            self.help_open = open;
        }
    }
    fn preview_ui(&mut self, ctx: &egui::Context) {
        let selected_entry = self.selected.and_then(|sel| {
            self.archives
                .get(sel.archive)
                .and_then(|a| a.entries().get(sel.index))
                .map(|e| (sel, e.name.clone(), e.size, self.archive_label(sel.archive)))
        });
        let paired = self.selected.and_then(|sel| self.find_paired_asset(sel));
        let mut reload_skin = false;
        let mut jump_to = None;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_PANEL)
                    .inner_margin(Margin::same(10)),
            )
            .show(ctx, |ui| {
                let Some((sel, entry_name, entry_size, arch_label)) = selected_entry else {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(if self.archives.is_empty() {
                                    "Откройте IMG-архив или перетащите файлы в окно"
                                } else {
                                    "Выберите ресурс в списке слева"
                                })
                                .color(theme::TEXT_SECONDARY),
                            );
                            if self.archives.is_empty() {
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    if theme::tool_button(
                                        ui,
                                        theme::Icon::FolderOpen,
                                        "Открыть IMG…",
                                        "Ctrl+O",
                                        !self.busy,
                                    )
                                    .clicked()
                                    {
                                        self.command(Command::Open, ctx);
                                    }
                                    if theme::tool_button(
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
                                });
                            }
                        });
                    });
                    return;
                };

                let mut chosen_variant = self.variant_selected;
                let edit_tag = self.pending_edit_tag(sel.archive, &entry_name);

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(egui::RichText::new(&entry_name).size(14.0).strong());
                    if let Some((tag, color)) = edit_tag {
                        let text = if tag == "ЗАМ" { "изменён" } else { "удаляется" };
                        ui.label(egui::RichText::new(text).size(11.5).color(color));
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "· {} · {}",
                            arch_label,
                            theme::format_size(entry_size)
                        ))
                        .size(12.0)
                        .color(theme::TEXT_SECONDARY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if theme::tab_toggle(ui, self.show_details, "Сведения").clicked() {
                            self.show_details = true;
                        }
                        if theme::tab_toggle(ui, !self.show_details, "Просмотр").clicked() {
                            self.show_details = false;
                        }
                        if let Some((paired_target, paired_name)) = &paired {
                            ui.separator();
                            if theme::tool_button(
                                ui,
                                theme::Icon::ScanEye,
                                &format!("Открыть {paired_name}"),
                                "Перейти к парному ресурсу",
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

                if !self.show_details && self.model.is_some() {
                    if !self.remap_targets.is_empty() {
                        self.ensure_vinyls();
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        theme::panel_menu(
                            ui,
                            format!("Текстуры ({})", self.images.len()),
                            |ui| {
                                if ui
                                    .button("Подключить внешний .txd…")
                                    .on_hover_text("Наложить словарь для предпросмотра")
                                    .clicked()
                                {
                                    self.load_txd(ctx);
                                    ui.close_menu();
                                }
                                ui.separator();
                                egui::ScrollArea::vertical()
                                    .max_height(240.0)
                                    .show(ui, |ui| {
                                        for (name, handle) in &self.images {
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

                        if !self.variants.is_empty() {
                            let prefix = if self.player_skin_mode {
                                "Вариант"
                            } else {
                                "Окраска"
                            };
                            let current = if chosen_variant == 0 {
                                "Основная"
                            } else {
                                &self.variants[chosen_variant - 1].name
                            };
                            theme::panel_menu(ui, format!("{prefix}: {current}"), |ui| {
                                variant_options(ui, &self.variants, &mut chosen_variant);
                            });
                        }

                        if !self.remap_targets.is_empty() {
                            let vinyl_name = self
                                .vinyl_selected
                                .and_then(|i| self.vinyls.get(i))
                                .map_or("Нет", |v| v.name.as_str());
                            let mut pick_vinyl: Option<Option<usize>> = None;
                            let mut pick_dir = false;
                            theme::panel_menu(ui, format!("Винил: {vinyl_name}"), |ui| {
                                ui.set_min_width(240.0);
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [180.0, theme::CONTROL_H],
                                        egui::TextEdit::singleline(&mut self.vinyl_filter)
                                            .id_salt("vinyl_search")
                                            .hint_text("Найти винил…"),
                                    );
                                    if theme::icon_button(
                                        ui,
                                        theme::Icon::Close,
                                        "Очистить",
                                        !self.vinyl_filter.is_empty(),
                                    )
                                    .clicked()
                                    {
                                        self.vinyl_filter.clear();
                                    }
                                });
                                ui.separator();
                                if ui
                                    .selectable_label(self.vinyl_selected.is_none(), "Без винила")
                                    .clicked()
                                {
                                    pick_vinyl = Some(None);
                                    ui.close_menu();
                                }
                                let query = self.vinyl_filter.to_lowercase();
                                let filtered: Vec<(usize, String)> = self
                                    .vinyls
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, v)| {
                                        query.is_empty() || v.name.to_lowercase().contains(&query)
                                    })
                                    .map(|(i, v)| (i, v.name.clone()))
                                    .collect();
                                egui::ScrollArea::vertical()
                                    .max_height(260.0)
                                    .show(ui, |ui| {
                                        if filtered.is_empty() {
                                            ui.weak(if self.vinyls.is_empty() {
                                                "Винилы не найдены в папке игры."
                                            } else {
                                                "Нет совпадений."
                                            });
                                        } else {
                                            for (idx, name) in filtered {
                                                if ui
                                                    .selectable_label(
                                                        self.vinyl_selected == Some(idx),
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
                                    .button(format!(
                                        "Папка винилов… ({})",
                                        self.vinyls.len()
                                    ))
                                    .clicked()
                                {
                                    pick_dir = true;
                                    ui.close_menu();
                                }
                            });
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
                        }

                        if self.player_skin_mode
                            && ui
                                .checkbox(&mut self.assemble_player_skin, "На скине CJ")
                                .changed()
                        {
                            reload_skin = true;
                        }
                    });
                    ui.add_space(4.0);
                    theme::hline(ui);
                    ui.add_space(4.0);
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
                        && (self.edits.is_empty() || self.active_archive == editor.target.archive);
                    editor.show(ui, can_edit);
                } else if let Some(model) = &mut self.model {
                    let vinyl_handle = self.vinyl_texture.clone();
                    if let Some(response) = model.show(
                        ui,
                        &self.images,
                        self.variant_selected != 0,
                        vinyl_handle.as_ref(),
                    ) {
                        response.context_menu(|ui| {
                            ui.strong("Варианты текстур");
                            variant_options(ui, &self.variants, &mut chosen_variant);
                        });
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
        if let Some(target) = jump_to {
            self.selection.clear();
            self.selection.insert(target);
            self.scroll_selection = true;
            self.select(target, ctx);
        } else if reload_skin {
            self.reload_player_preview(ctx);
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

fn search_field(
    ui: &mut egui::Ui,
    filter: &mut String,
    grouped: &mut bool,
    multiple: bool,
) -> egui::Response {
    egui::Frame::new()
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let clear_width = theme::CONTROL_H + 6.0;
                let grouping_width = if multiple { 50.0 } else { 0.0 };
                let response = ui.add_sized(
                    [
                        (ui.available_width() - clear_width - grouping_width - 8.0).max(80.0),
                        theme::CONTROL_H,
                    ],
                    egui::TextEdit::singleline(filter)
                        .id_salt("archive_search")
                        .hint_text("Поиск ресурса… (Ctrl+F)"),
                );
                if theme::icon_button(
                    ui,
                    theme::Icon::Close,
                    "Очистить поиск (Esc)",
                    !filter.is_empty(),
                )
                .clicked()
                {
                    filter.clear();
                    response.request_focus();
                    ui.ctx().request_repaint();
                }
                if response.changed() {
                    ui.ctx().request_repaint();
                }
                if multiple {
                    ui.menu_button("Вид", |ui| {
                        ui.checkbox(grouped, "Группировать по архивам");
                    });
                }
                response
            })
            .inner
        })
        .inner
}
fn menu(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    action: Command,
    target: &mut Option<Command>,
) {
    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
        *target = Some(action);
        ui.close_menu();
    }
}

fn file_icon(ui: &mut egui::Ui, directory: bool, kind: &str) {
    let icon_kind = if directory {
        theme::Icon::Folder
    } else {
        match kind {
            "DFF" => theme::Icon::Box,
            "TXD" => theme::Icon::Image,
            "IMG" => theme::Icon::Archive,
            _ => theme::Icon::File,
        }
    };
    let tint = if directory {
        theme::STATUS_MODIFIED
    } else {
        theme::TEXT_SECONDARY
    };
    ui.add(theme::icon(icon_kind).tint(tint));
}

#[cfg(test)]
mod search_tests {
    use super::*;
    #[test]
    fn search_keeps_id_and_focus_when_text_becomes_empty_or_nonempty() {
        let ctx = egui::Context::default();
        let mut filter = String::new();
        let mut grouped = true;
        let mut id = None;
        for text in ["", "s", "su", "", "x"] {
            filter.clear();
            filter.push_str(text);
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = search_field(ui, &mut filter, &mut grouped, true);
                    if let Some(id) = id {
                        assert_eq!(response.id, id);
                    } else {
                        id = Some(response.id);
                        response.request_focus();
                    }
                    assert_eq!(ctx.memory(|m| m.focused()), id);
                    assert!(
                        response.rect.height() < 48.0,
                        "search must not stretch vertically"
                    );
                });
            });
        }
    }
    #[test]
    fn typing_and_backspace_filter_results_without_losing_focus() {
        let ctx = egui::Context::default();
        let mut browser = Browser::new();
        browser.folder_files = vec![
            File {
                path: PathBuf::from("supergt.dff"),
                directory: false,
                size: 1,
            },
            File {
                path: PathBuf::from("skin.txd"),
                directory: false,
                size: 1,
            },
        ];
        for (step, events, expected, matches) in [
            (0, vec![], "", 2),
            (1, vec![egui::Event::Text("s".into())], "s", 2),
            (2, vec![egui::Event::Text("u".into())], "su", 1),
            (
                3,
                vec![egui::Event::Key {
                    key: egui::Key::Backspace,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                "s",
                2,
            ),
            (
                4,
                vec![egui::Event::Key {
                    key: egui::Key::Backspace,
                    physical_key: None,
                    pressed: true,
                    repeat: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                "",
                2,
            ),
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = search_field(
                            ui,
                            &mut browser.filter,
                            &mut browser.group_by_archive,
                            false,
                        );
                        if step == 0 {
                            response.request_focus();
                        }
                        assert_eq!(ctx.memory(|m| m.focused()), Some(response.id));
                        assert_eq!(browser.filter, expected);
                        assert_eq!(browser.filtered_rows().len(), matches);
                    });
                },
            );
        }
    }
    #[test]
    fn multi_archive_search_and_save_preserve_other_tabs_and_selection() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("openprovince-workspace-test-{stamp}"));
        let profile = Profile::inspected_client();
        let mut browser = Browser::new();
        for (i, name) in ["car.txd", "skin.txd"].into_iter().enumerate() {
            let input = root.join(format!("input-{i}"));
            std::fs::create_dir_all(&input).expect("fixture");
            std::fs::write(input.join(name), b"data").expect("entry");
            let path = root.join(format!("archive-{i}.img"));
            province_archive::pack_directory(&input, &path, &profile).expect("IMG");
            browser.archives.push(Arc::new(
                province_archive::Archive::open(path).expect("archive"),
            ));
        }
        for (query, expected) in [("txd", 2), ("car", 1), ("SKIN", 1), ("absent", 0)] {
            browser.filter = query.into();
            assert_eq!(
                browser
                    .filtered_rows()
                    .iter()
                    .filter(|r| matches!(r, RowItem::Entry(_)))
                    .count(),
                expected
            );
        }
        let selected = EntryRef {
            archive: 1,
            index: 0,
        };
        browser.selected = Some(selected);
        browser.selection.insert(selected);
        browser.active_archive = 1;
        let updated =
            province_archive::Archive::open(browser.archives[1].path()).expect("saved header");
        browser.apply_saved_archive(1, updated);
        assert_eq!(browser.archives.len(), 2);
        assert_eq!(browser.selected, Some(selected));
        assert!(browser.selection.contains(&selected));
        assert_eq!(browser.active_archive, 1);
        std::fs::remove_dir_all(root).expect("fixture cleanup");
    }
    #[test]
    fn archive_search_rebuilds_filtered_rows_for_each_query() {
        let mut browser = Browser::new();
        browser.folder_files = vec![
            File {
                path: PathBuf::from("SuperGT.dff"),
                directory: false,
                size: 1,
            },
            File {
                path: PathBuf::from("skin.txd"),
                directory: false,
                size: 2,
            },
        ];
        for (query, expected) in [("s", 2), ("super", 1), ("SUPER", 1), ("none", 0), ("", 2)] {
            browser.filter = query.into();
            let rows = browser.filtered_rows();
            assert_eq!(
                rows.iter()
                    .filter(|r| matches!(r, RowItem::Entry(_)))
                    .count(),
                expected
            );
        }
    }
    #[test]
    fn type_filter_and_paired_asset_resolve_dff_txd() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("openprovince-ui-paired-{stamp}"));
        let profile = Profile::inspected_client();
        let input = root.join("input");
        std::fs::create_dir_all(&input).expect("fixture");
        for name in ["infernus.dff", "infernus.txd", "readme.txt"] {
            std::fs::write(input.join(name), b"data").expect("entry");
        }
        let img_path = root.join("vehicles.img");
        province_archive::pack_directory(&input, &img_path, &profile).expect("IMG");

        let mut browser = Browser::new();
        browser
            .archives
            .push(Arc::new(province_archive::Archive::open(img_path).expect("archive")));

        for (mode, expected) in [(0, 3), (1, 1), (2, 1), (3, 1)] {
            browser.type_filter = mode;
            let count = browser
                .filtered_rows()
                .iter()
                .filter(|r| matches!(r, RowItem::Entry(_)))
                .count();
            assert_eq!(count, expected, "type_filter={mode}");
        }

        let entries = browser.archives[0].entries();
        let dff_idx = entries.iter().position(|e| e.name == "infernus.dff").unwrap();
        let txd_idx = entries.iter().position(|e| e.name == "infernus.txd").unwrap();
        let paired = browser.find_paired_asset(EntryRef {
            archive: 0,
            index: dff_idx,
        });
        assert_eq!(
            paired,
            Some((
                EntryRef {
                    archive: 0,
                    index: txd_idx,
                },
                "infernus.txd".to_owned()
            ))
        );
        std::fs::remove_dir_all(root).expect("fixture cleanup");
    }
}

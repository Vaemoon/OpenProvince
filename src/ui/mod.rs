//! Интерфейс файлового менеджера, вкладок IMG и инспектора ресурсов.
mod browser_panel;
mod chrome;
mod viewport;

use crate::{Browser, Edit, EntryRef, Message, theme};
use eframe::egui::{self, Color32, Margin};
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
pub(super) struct EntryRow {
    pub(super) target: EntryRef,
    pub(super) name: String,
    pub(super) kind: String,
    pub(super) archive_idx: usize,
    pub(super) archive_stem: String,
    pub(super) size: u64,
    pub(super) directory: bool,
}

#[derive(Clone)]
pub(super) enum RowItem {
    Section { title: String, count: usize },
    Entry(EntryRow),
}

#[derive(Clone, Copy)]
pub(super) enum Command {
    Open,
    AddArchive,
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
        if self.txd_editor.as_ref().is_some_and(|e| e.disk_dirty) {
            self.status = "Сначала сохраните или закройте изменённый TXD.".into();
            return;
        }
        self.standalone_txd = None;
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

    pub(super) fn close_standalone_txd(&mut self) {
        if self.txd_editor.as_ref().is_some_and(|e| e.disk_dirty) {
            self.status = "Нельзя закрыть TXD с несохранёнными правками (Ctrl+S).".into();
            return;
        }
        self.standalone_txd = None;
        if self.txd_editor.as_ref().is_some_and(|e| e.disk_path.is_some()) {
            self.txd_editor = None;
            self.preview.clear();
        }
        if let Some(sel) = self.selected
            && !self.archives.is_empty()
        {
            self.status = format!("Выбрана вкладка: {}", self.archive_label(sel.archive));
        }
    }

    pub(super) fn command(&mut self, command: Command, ctx: &egui::Context) {
        if self.busy || self.txd_editor.as_ref().is_some_and(|e| e.busy) {
            self.status = "Дождитесь завершения текущей операции.".into();
            return;
        }
        match command {
            Command::Open => {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("IMG и TXD ресурсы", &["img", "txd"])
                    .add_filter("IMG архивы", &["img"])
                    .add_filter("RenderWare TXD", &["txd"])
                    .set_directory(&self.folder)
                    .pick_files()
                {
                    let imgs: Vec<PathBuf> = paths
                        .iter()
                        .filter(|p| !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txd")))
                        .cloned()
                        .collect();
                    if !imgs.is_empty() {
                        self.open_many(imgs, false, ctx);
                    } else if let Some(txd) = paths
                        .into_iter()
                        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txd")))
                    {
                        self.open_txd_file(txd, ctx);
                    }
                }
            }
            Command::AddArchive => {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("IMG архивы", &["img"])
                    .set_directory(&self.folder)
                    .pick_files()
                {
                    self.open_many(paths, !self.archives.is_empty(), ctx);
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
                        self.edited_archive = Some(target.archive);
                    }
                    if self.edited_archive == Some(target.archive) {
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
                        self.edited_archive = Some(arch);
                    }
                    if self.edited_archive != Some(arch)
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
                if self.standalone_txd.is_some()
                    && (self.archives.is_empty()
                        || self
                            .txd_editor
                            .as_ref()
                            .is_some_and(|e| e.disk_path.is_some()))
                {
                    self.close_standalone_txd();
                } else if !self.archives.is_empty() {
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
                            .is_some_and(|e| {
                                e.eq_ignore_ascii_case("img") || e.eq_ignore_ascii_case("txd")
                            })
                        {
                            self.open(path, ctx);
                        } else {
                            self.status =
                                "Откройте IMG-архив или TXD-словарь, чтобы просмотреть ресурсы."
                                    .into();
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
                self.edited_archive = None;
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
                } else if let Some(a) = self.archives.get(self.active_archive) {
                    let ai = self.active_archive;
                    self.selection = (0..a.entries().len())
                        .map(|ei| EntryRef {
                            archive: ai,
                            index: ei,
                        })
                        .collect();
                }
            }
            Command::Info => {
                self.help_open = true;
            }
        }
    }

    pub(super) fn pending_edit_tag(
        &self,
        archive_idx: usize,
        name: &str,
    ) -> Option<(&'static str, Color32)> {
        if self.edits.is_empty() || self.edited_archive != Some(archive_idx) {
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

    pub(super) fn find_paired_asset(&self, target: EntryRef) -> Option<(EntryRef, String)> {
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

    pub(super) fn filtered_rows(&mut self) -> Arc<Vec<RowItem>> {
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
            "{:?}|{}|{}|{:?}|{}|{}|{}|{}|{}|{}|{:?}",
            archive_ptrs,
            self.active_archive,
            self.all_tabs,
            self.edited_archive,
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
            let multi = self.all_tabs && self.archives.len() > 1;
            let mut entries_list: Vec<EntryRow> = if !self.archives.is_empty() {
                let mut list = Vec::new();
                let range = if self.all_tabs {
                    0..self.archives.len()
                } else if self.active_archive < self.archives.len() {
                    self.active_archive..self.active_archive + 1
                } else {
                    0..0
                };
                for ai in range {
                    let arch = &self.archives[ai];
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
        let standalone_dirty = self.txd_editor.as_ref().is_some_and(|e| e.disk_dirty);
        if (!self.edits.is_empty()
            || standalone_dirty
            || self.txd_editor.as_ref().is_some_and(|e| e.busy))
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.status =
                "Есть несохранённые изменения: нажмите «Сохранить» или «Отменить изменения»."
                    .into();
        }
        let mut menu_jump_to = None;
        self.top_chrome_ui(ctx, &mut command, &mut menu_jump_to);
        self.status_and_dialogs_ui(ctx, &mut command);
        self.browser_panel_ui(ctx, &mut command, menu_jump_to);
        if let Some(command) = command {
            self.command(command, ctx);
        }
        self.preview_ui(ctx);
        self.dialogs_ui(ctx);
    }
}

pub(super) fn search_field(
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
                let grouping_width = if multiple { 66.0 } else { 0.0 };
                let response = ui.add_sized(
                    [
                        (ui.available_width() - clear_width - grouping_width - 8.0).max(80.0),
                        theme::CONTROL_H,
                    ],
                    egui::TextEdit::singleline(filter)
                        .id_salt("archive_search")
                        .hint_text(if multiple {
                            "Поиск по всем вкладкам… (Ctrl+F)"
                        } else {
                            "Поиск ресурса… (Ctrl+F)"
                        }),
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
                    theme::panel_menu(ui, "Вид", |ui| {
                        ui.checkbox(grouped, "Группировать по архивам");
                    });
                }
                response
            })
            .inner
        })
        .inner
}

pub(super) fn menu(
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

pub(super) fn file_icon(ui: &mut egui::Ui, directory: bool, kind: &str) {
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
    use eframe::egui::Vec2;

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
                    assert!(response.rect.height() < 48.0);
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
        browser.active_archive = 0;
        assert_eq!(
            browser
                .filtered_rows()
                .iter()
                .filter(|r| matches!(r, RowItem::Entry(_)))
                .count(),
            1
        );
        browser.active_archive = 1;
        browser.filter = "car".into();
        assert_eq!(
            browser
                .filtered_rows()
                .iter()
                .filter(|r| matches!(r, RowItem::Entry(_)))
                .count(),
            0
        );
        browser.all_tabs = true;
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

    #[test]
    fn standalone_txd_and_panel_controls_render_cleanly() {
        let mut minimal_txd = Vec::new();
        let ver = 0x1803_FFFFu32.to_le_bytes();
        minimal_txd.extend_from_slice(&0x16u32.to_le_bytes());
        minimal_txd.extend_from_slice(&28u32.to_le_bytes());
        minimal_txd.extend_from_slice(&ver);
        minimal_txd.extend_from_slice(&0x01u32.to_le_bytes());
        minimal_txd.extend_from_slice(&4u32.to_le_bytes());
        minimal_txd.extend_from_slice(&ver);
        minimal_txd.extend_from_slice(&0u32.to_le_bytes());
        minimal_txd.extend_from_slice(&0x03u32.to_le_bytes());
        minimal_txd.extend_from_slice(&0u32.to_le_bytes());
        minimal_txd.extend_from_slice(&ver);

        let doc = crate::txd_document::Document::parse(&minimal_txd).expect("minimal TXD");
        let path = PathBuf::from("standalone_test.txd");
        let mut browser = Browser::new();
        browser.standalone_txd = Some(path.clone());
        browser.txd_editor = Some(crate::texture_editor::Editor::new_standalone(doc, path));

        let ctx = egui::Context::default();
        theme::apply_style(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            browser.manager_ui(ctx);
            egui::Window::new("control_probe").show(ctx, |ui| {
                let mut active = true;
                let r1 = theme::tab_toggle(ui, active, "Все вкладки");
                let r2 = theme::bordered_button(
                    ui,
                    "Открыть все IMG",
                    Vec2::new(120.0, theme::CONTROL_H),
                );
                assert!((r1.rect.height() - theme::CONTROL_H).abs() <= 1.0);
                assert!((r2.rect.height() - theme::CONTROL_H).abs() <= 1.0);
                theme::panel_menu(ui, "Канал: RGBA", |ui| {
                    let _ = ui.checkbox(&mut active, "RGBA");
                });
            });
        });
        assert!(browser.standalone_txd.is_some());
        browser.close_standalone_txd();
        assert!(browser.standalone_txd.is_none());
        assert!(browser.txd_editor.is_none());
    }
}

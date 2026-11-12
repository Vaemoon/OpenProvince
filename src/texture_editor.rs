//! Встроенный редактор TXD (декодирование и запись в фоновом потоке).
use crate::{
    EntryRef, theme, txd,
    txd_document::{Document, Entry},
};
use eframe::egui::{self, Align, Color32, Stroke, Vec2};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::mpsc,
    thread,
};

enum Message {
    Changed(Result<(Document, Option<PathBuf>), String>),
    Image(u64, usize, bool, Option<txd::Texture>),
    Saved(Result<String, String>),
    DiskSaved(Result<PathBuf, String>),
}

pub struct Editor {
    pub target: EntryRef,
    pub disk_path: Option<PathBuf>,
    pub disk_dirty: bool,
    pub name: String,
    document: Document,
    entries: Vec<Entry>,
    selected: usize,
    filter: String,
    rename: String,
    renaming: bool,
    mipmaps: bool,
    revision: u64,
    undo: Vec<Document>,
    redo: Vec<Document>,
    history_action: u8,
    pending_history: Option<Document>,
    thumbnails: BTreeMap<usize, egui::TextureHandle>,
    requested: BTreeSet<(usize, bool)>,
    image: Option<egui::TextureHandle>,
    channel: u8,
    backdrop: u8,
    fit: bool,
    zoom: f32,
    pan: Vec2,
    pub busy: bool,
    status: String,
    sender: mpsc::Sender<Message>,
    receiver: mpsc::Receiver<Message>,
}

fn load_image(path: &std::path::Path) -> Result<image::RgbaImage, String> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let (width, height) = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err("Изображение должно быть не больше 4096 × 4096".into());
    }
    reader
        .decode()
        .map(|i| i.into_rgba8())
        .map_err(|e| e.to_string())
}
fn stage_document(document: &Document) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = std::env::temp_dir().join("OpenProvince");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join(format!("{}-{stamp}.txd", std::process::id()));
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .and_then(|mut f| f.write_all(&document.bytes()))
        .map_err(|e| e.to_string())?;
    Ok(path)
}

impl Editor {
    pub fn mark_saved(&mut self) {
        self.status = "Изменения сохранены в IMG.".into();
    }
    pub fn new(document: Document, target: EntryRef, name: String) -> Self {
        Self::create(document, target, None, name)
    }
    pub fn new_standalone(document: Document, path: PathBuf) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("texture.txd")
            .to_owned();
        Self::create(
            document,
            EntryRef {
                archive: usize::MAX,
                index: 0,
            },
            Some(path),
            name,
        )
    }
    fn create(
        document: Document,
        target: EntryRef,
        disk_path: Option<PathBuf>,
        name: String,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let entries = document.entries();
        let rename = entries.first().map_or(String::new(), |e| e.name.clone());
        Self {
            target,
            disk_path,
            disk_dirty: false,
            name,
            document,
            entries,
            selected: 0,
            filter: String::new(),
            rename,
            renaming: false,
            mipmaps: true,
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            history_action: 0,
            pending_history: None,
            thumbnails: BTreeMap::new(),
            requested: BTreeSet::new(),
            image: None,
            channel: 0,
            backdrop: 0,
            fit: true,
            zoom: 1.0,
            pan: Vec2::ZERO,
            busy: false,
            status: String::new(),
            sender,
            receiver,
        }
    }
    pub fn save_to_disk(&mut self, custom_path: Option<PathBuf>, ctx: &egui::Context) {
        let Some(path) = custom_path.or_else(|| self.disk_path.clone()) else {
            return;
        };
        if self.busy {
            return;
        }
        let bytes = self.document.bytes();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        self.busy = true;
        thread::spawn(move || {
            let result = crate::storage::write_file(&path, &bytes).map(|()| path);
            let _ = sender.send(Message::DiskSaved(result));
            ctx.request_repaint();
        });
    }
    /// Возвращает временный .txd для постановки в очередь правок IMG.
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        let mut staged = None;
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                Message::Changed(result) => {
                    self.busy = false;
                    match result {
                        Ok((document, path)) => {
                            match self.history_action {
                                1 => {
                                    self.undo.pop();
                                    self.redo.push(self.document.clone());
                                }
                                2 => {
                                    self.redo.pop();
                                    self.undo.push(self.document.clone());
                                }
                                _ => {
                                    self.undo.push(self.document.clone());
                                    self.redo.clear();
                                }
                            }
                            if self.undo.len() > 8 {
                                self.undo.remove(0);
                            }
                            if self.redo.len() > 8 {
                                self.redo.remove(0);
                            }
                            self.document = document;
                            self.entries = self.document.entries();
                            self.selected = self.selected.min(self.entries.len().saturating_sub(1));
                            self.rename = self
                                .entries
                                .get(self.selected)
                                .map_or(String::new(), |e| e.name.clone());
                            self.revision += 1;
                            self.image = None;
                            self.thumbnails.clear();
                            self.requested.clear();
                            if self.disk_path.is_some() {
                                self.disk_dirty = true;
                                self.status = "Правки TXD готовы. Сохраните файл: Ctrl+S.".into();
                            } else {
                                self.status = "Правки TXD подготовлены. Сохраните IMG: Ctrl+S.".into();
                                staged = path;
                            }
                        }
                        Err(error) => self.status = error,
                    }
                    self.pending_history = None;
                    self.history_action = 0;
                }
                Message::Image(revision, index, thumbnail, image) => {
                    if revision != self.revision {
                        continue;
                    }
                    self.requested.remove(&(index, thumbnail));
                    if let Some(image) = image {
                        let handle = ctx.load_texture(
                            format!("txd-{revision}-{index}-{thumbnail}"),
                            egui::ColorImage::from_rgba_unmultiplied(
                                [image.width, image.height],
                                &image.rgba,
                            ),
                            egui::TextureOptions::LINEAR,
                        );
                        if thumbnail {
                            if self.thumbnails.len() >= 64
                                && let Some(key) = self.thumbnails.keys().next().copied()
                            {
                                self.thumbnails.remove(&key);
                            }
                            self.thumbnails.insert(index, handle);
                        } else if index == self.selected {
                            self.image = Some(handle);
                        }
                    } else {
                        self.requested.insert((index, thumbnail));
                        if !thumbnail {
                            self.status =
                            "Формат этой текстуры пока нельзя отобразить; исходный блок сохранён."
                                .into();
                        }
                    }
                }
                Message::Saved(result) => {
                    self.busy = false;
                    self.status = result.unwrap_or_else(|e| e);
                }
                Message::DiskSaved(result) => {
                    self.busy = false;
                    match result {
                        Ok(path) => {
                            self.disk_dirty = false;
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                self.name = name.to_owned();
                            }
                            self.status = format!("Сохранено: {}", path.display());
                            self.disk_path = Some(path);
                        }
                        Err(error) => self.status = error,
                    }
                }
            }
        }
        staged
    }
    fn request_image(&mut self, index: usize, thumbnail: bool, ctx: &egui::Context) {
        if !self.requested.insert((index, thumbnail)) {
            return;
        }
        let document = self.document.clone();
        let revision = self.revision;
        let channel = self.channel;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let texture = document.preview(index).map(|mut texture| {
                if thumbnail {
                    if let Some(image) = image::RgbaImage::from_raw(
                        texture.width as u32,
                        texture.height as u32,
                        texture.rgba,
                    ) {
                        let image = image::imageops::thumbnail(&image, 40, 40);
                        texture.width = image.width() as usize;
                        texture.height = image.height() as usize;
                        texture.rgba = image.into_raw();
                    } else {
                        texture.rgba = Vec::new();
                    }
                } else {
                    for pixel in texture.rgba.as_chunks_mut::<4>().0 {
                        match channel {
                            1 => pixel[3] = 255,
                            2 => {
                                *pixel = [pixel[3], pixel[3], pixel[3], 255];
                            }
                            _ => {}
                        }
                    }
                }
                texture
            });
            let _ = sender.send(Message::Image(revision, index, thumbnail, texture));
            ctx.request_repaint();
        });
    }
    fn edit(
        &mut self,
        ctx: &egui::Context,
        operation: impl FnOnce(&mut Document) -> Result<(), String> + Send + 'static,
    ) {
        self.busy = true;
        let mut document = self
            .pending_history
            .take()
            .unwrap_or_else(|| self.document.clone());
        let needs_stage = self.disk_path.is_none();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = operation(&mut document).and_then(|()| {
                if needs_stage {
                    stage_document(&document).map(|path| (document, Some(path)))
                } else {
                    Ok((document, None))
                }
            });
            let _ = sender.send(Message::Changed(result));
            ctx.request_repaint();
        });
    }
    pub fn import_paths(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        if self.busy || paths.is_empty() {
            return;
        }
        let mipmaps = self.mipmaps;
        self.edit(ctx, move |document| {
            for path in paths {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("Нет имени файла")?;
                let existing = document
                    .entries()
                    .iter()
                    .position(|e| e.name.eq_ignore_ascii_case(name));
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("dds"))
                {
                    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                    let index = match existing {
                        Some(index) => index,
                        None => document.add(name, &image::RgbaImage::new(1, 1), false)?,
                    };
                    document.replace_dds(index, &bytes)?;
                } else {
                    let image = load_image(&path)?;
                    if let Some(index) = existing {
                        document.replace(index, &image, mipmaps)?;
                    } else {
                        document.add(name, &image, mipmaps)?;
                    }
                }
            }
            Ok(())
        });
    }
    fn import(&mut self, replace: bool, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Изображения", &["png", "tga", "bmp", "dds", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        let index = self.selected;
        let mipmaps = self.mipmaps;
        self.edit(ctx, move |document| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Нет имени файла")?;
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dds"))
            {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let index = if replace {
                    index
                } else {
                    document.add(name, &image::RgbaImage::new(1, 1), false)?
                };
                return document.replace_dds(index, &bytes);
            }
            let image = load_image(&path)?;
            if replace {
                document.replace(index, &image, mipmaps)
            } else {
                document.add(name, &image, mipmaps).map(|_| ())
            }
        });
    }
    fn export(&mut self, all: bool, dictionary: bool, ctx: &egui::Context) {
        let path = if all {
            rfd::FileDialog::new().pick_folder()
        } else if dictionary {
            rfd::FileDialog::new()
                .set_file_name(&self.name)
                .add_filter("Словарь TXD", &["txd"])
                .save_file()
        } else {
            let name = self
                .entries
                .get(self.selected)
                .map_or("texture", |e| &e.name);
            rfd::FileDialog::new()
                .set_file_name(format!("{name}.png"))
                .add_filter("PNG с прозрачностью", &["png"])
                .add_filter("DDS (исходное сжатие DXT)", &["dds"])
                .save_file()
        };
        let Some(path) = path else {
            return;
        };
        let document = self.document.clone();
        let index = self.selected;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        self.busy = true;
        thread::spawn(move || {
            let result = (|| -> Result<String, String> {
                if dictionary {
                    crate::storage::write_file(&path, &document.bytes())?;
                    return Ok(
                        "Словарь TXD сохранён отдельно. Для сохранения IMG нажмите Ctrl+S.".into(),
                    );
                }
                let indices: Vec<_> = if all {
                    (0..document.len()).collect()
                } else {
                    vec![index]
                };
                let entries = document.entries();
                let mut count = 0;
                for i in indices {
                    let texture = document
                        .preview(i)
                        .ok_or_else(|| format!("Не поддерживается: {}", entries[i].name))?;
                    let destination = if all {
                        path.join(format!(
                            "{i:03}-{}.png",
                            entries[i]
                                .name
                                .chars()
                                .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                                    c
                                } else {
                                    '_'
                                })
                                .collect::<String>()
                        ))
                    } else {
                        path.clone()
                    };
                    if !all
                        && destination
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("dds"))
                    {
                        crate::storage::write_file(&destination, &document.dds(i)?)?;
                        count += 1;
                        continue;
                    }
                    let image = image::RgbaImage::from_raw(
                        texture.width as u32,
                        texture.height as u32,
                        texture.rgba,
                    )
                    .ok_or("Размер изображения")?;
                    let mut bytes = std::io::Cursor::new(Vec::new());
                    image
                        .write_to(&mut bytes, image::ImageFormat::Png)
                        .map_err(|e| e.to_string())?;
                    if all && destination.exists() {
                        return Err(format!("Файл уже существует: {}", destination.display()));
                    }
                    crate::storage::write_file(&destination, bytes.get_ref())?;
                    count += 1;
                }
                Ok(format!("Экспортировано текстур: {count}"))
            })();
            let _ = sender.send(Message::Saved(result));
            ctx.request_repaint();
        });
    }
    pub fn show(&mut self, ui: &mut egui::Ui, can_edit: bool) {
        let ctx = ui.ctx().clone();
        let ready = !self.busy;
        let editable = can_edit && ready;
        let editable_texture =
            editable && self.entries.get(self.selected).is_some_and(|e| e.supported);
        let has_selected = !self.entries.is_empty();
        let editing_text = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| egui::text_edit::TextEditState::load(&ctx, id).is_some());
        let redo_key = editable
            && !editing_text
            && ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers {
                        ctrl: true,
                        shift: true,
                        ..Default::default()
                    },
                    egui::Key::Z,
                ) || i.consume_key(egui::Modifiers::CTRL, egui::Key::Y)
            });
        let undo_key = editable
            && !editing_text
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z));
        let delete_key = editable
            && has_selected
            && !editing_text
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete));

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 5.0);
            if theme::tool_button(
                ui,
                theme::Icon::Plus,
                "Добавить",
                "Добавить текстуру (PNG, DDS, TGA, BMP)",
                editable,
            )
            .clicked()
            {
                self.import(false, &ctx);
            }
            if theme::tool_button(
                ui,
                theme::Icon::Replace,
                "Заменить",
                "Заменить выбранную текстуру",
                editable_texture,
            )
            .clicked()
            {
                self.import(true, &ctx);
            }
            if theme::tool_button(
                ui,
                theme::Icon::Trash,
                "Удалить",
                "Удалить выбранную текстуру (Del)",
                editable && has_selected,
            )
            .clicked()
                || delete_key
            {
                let index = self.selected;
                self.edit(&ctx, move |d| d.remove(index));
            }

            ui.separator();

            if theme::icon_button(
                ui,
                theme::Icon::Undo,
                "Отменить правку TXD (Ctrl+Z)",
                editable && !self.undo.is_empty(),
            )
            .clicked()
            {
                self.history_action = 1;
                self.pending_history = self.undo.last().cloned();
                self.edit(&ctx, |_| Ok(()));
            }
            if theme::icon_button(
                ui,
                theme::Icon::Redo,
                "Повторить правку TXD (Ctrl+Y)",
                editable && !self.redo.is_empty(),
            )
            .clicked()
            {
                self.history_action = 2;
                self.pending_history = self.redo.last().cloned();
                self.edit(&ctx, |_| Ok(()));
            }

            ui.separator();

            theme::panel_menu(ui, "Экспорт", |ui| {
                if ui
                    .add_enabled(
                        ready && has_selected,
                        egui::Button::new("Текущую (PNG / DDS)…"),
                    )
                    .clicked()
                {
                    self.export(false, false, &ctx);
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        ready && has_selected,
                        egui::Button::new("Все текстуры в папку…"),
                    )
                    .clicked()
                {
                    self.export(true, false, &ctx);
                    ui.close_menu();
                }
                ui.separator();
                if ui
                    .add_enabled(ready, egui::Button::new("Словарь .txd отдельно…"))
                    .clicked()
                {
                    self.export(false, true, &ctx);
                    ui.close_menu();
                }
            });

            ui.separator();
            ui.checkbox(&mut self.mipmaps, "Mip-уровни")
                .on_hover_text("Создавать mip-уровни при импорте PNG/TGA/BMP");
        });
        ui.add_space(4.0);
        theme::hline(ui);

        if !self.busy && undo_key && !self.undo.is_empty() {
            self.history_action = 1;
            self.pending_history = self.undo.last().cloned();
            self.edit(&ctx, |_| Ok(()));
        } else if !self.busy && redo_key && !self.redo.is_empty() {
            self.history_action = 2;
            self.pending_history = self.redo.last().cloned();
            self.edit(&ctx, |_| Ok(()));
        }
        if !can_edit {
            ui.add_space(4.0);
            ui.colored_label(
                theme::STATUS_MODIFIED,
                "Редактирование заблокировано: сохраните или отмените правки другого IMG.",
            );
        }
        ui.add_space(6.0);
        let available = ui.available_size();
        if available.x >= 460.0 {
            let list_w = (available.x * 0.34).clamp(184.0, 232.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(list_w, available.y),
                    egui::Layout::top_down(Align::Min),
                    |ui| {
                        self.texture_list(ui, &ctx, available.y, editable);
                    },
                );
                ui.separator();
                let width = ui.available_width();
                ui.allocate_ui_with_layout(
                    Vec2::new(width, available.y),
                    egui::Layout::top_down(Align::Min),
                    |ui| {
                        self.viewer(ui, &ctx, editable, editable_texture);
                    },
                );
            });
        } else {
            self.texture_list(ui, &ctx, (available.y * 0.30).clamp(112.0, 190.0), editable);
            ui.add_space(6.0);
            self.viewer(ui, &ctx, editable, editable_texture);
        }
    }
    fn select_index(&mut self, index: usize) {
        if index != self.selected {
            self.selected = index;
            self.image = None;
            self.revision += 1;
            self.requested.clear();
            self.pan = Vec2::ZERO;
            self.renaming = false;
            self.rename = self
                .entries
                .get(index)
                .map_or(String::new(), |e| e.name.clone());
        }
    }
    fn texture_list(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        height: f32,
        editable: bool,
    ) {
        let mut selected = self.selected;
        let mut pending_action: Option<u8> = None;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Текстуры")
                    .size(12.0)
                    .color(theme::TEXT_SECONDARY),
            );
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{}", self.entries.len()))
                        .size(11.5)
                        .color(theme::TEXT_MUTED),
                );
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let clear_w = theme::CONTROL_H + 4.0;
            ui.add_sized(
                [(ui.available_width() - clear_w).max(60.0), theme::CONTROL_H],
                egui::TextEdit::singleline(&mut self.filter)
                    .id_salt("txd_filter")
                    .hint_text("Фильтр…"),
            );
            if theme::icon_button(
                ui,
                theme::Icon::Close,
                "Очистить",
                !self.filter.is_empty(),
            )
            .clicked()
            {
                self.filter.clear();
            }
        });
        ui.add_space(4.0);
        let query = self.filter.to_lowercase();
        let visible: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.name.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();

        if !ctx.wants_keyboard_input() && !visible.is_empty() {
            let cur_pos = visible.iter().position(|&i| i == selected);
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                let next = cur_pos.map_or(0, |p| (p + 1).min(visible.len() - 1));
                selected = visible[next];
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                let next = cur_pos.map_or(0, |p| p.saturating_sub(1));
                selected = visible[next];
            }
        }

        egui::ScrollArea::vertical()
            .id_salt("txd_entries")
            .max_height((height - 64.0).max(64.0))
            .auto_shrink([false, false])
            .show_rows(ui, 46.0, visible.len(), |ui, range| {
                for row in range {
                    let index = visible[row];
                    if !self.thumbnails.contains_key(&index) {
                        self.request_image(index, true, ctx);
                    }
                    let entry = &self.entries[index];
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 46.0),
                        egui::Sense::click(),
                    );
                    let is_selected = selected == index;
                    if is_selected || response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            if is_selected {
                                theme::SELECTION_BG
                            } else {
                                theme::BG_ELEVATED
                            },
                        );
                    }
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(6.0, 5.0))),
                        |ui| {
                            ui.horizontal(|ui| {
                                let (thumb_rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::hover());
                                theme::draw_checkerboard(ui.painter(), thumb_rect, 8.0);
                                if let Some(image) = self.thumbnails.get(&index) {
                                    ui.put(
                                        thumb_rect,
                                        egui::Image::new(image).max_size(Vec2::splat(36.0)),
                                    );
                                }
                                ui.add_space(4.0);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 1.0;
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&entry.name).color(if is_selected {
                                                Color32::WHITE
                                            } else {
                                                theme::TEXT_PRIMARY
                                            }),
                                        )
                                        .truncate(),
                                    );
                                    ui.label(
                                        egui::RichText::new(if entry.supported {
                                            format!("{}×{} · {}", entry.width, entry.height, entry.format)
                                        } else {
                                            format!(
                                                "{}×{} · {} · только чтение",
                                                entry.width, entry.height, entry.format
                                            )
                                        })
                                        .size(11.5)
                                        .color(if is_selected {
                                            Color32::from_rgb(205, 215, 230)
                                        } else {
                                            theme::TEXT_SECONDARY
                                        }),
                                    );
                                });
                            });
                        },
                    );
                    if response.clicked() {
                        selected = index;
                        response.request_focus();
                    }
                    let name_copy = entry.name.clone();
                    let supported = entry.supported;
                    let response = if entry.name.len() > 18 {
                        response.on_hover_text(name_copy)
                    } else {
                        response
                    };
                    response.context_menu(|ui| {
                        selected = index;
                        if ui
                            .add_enabled(
                                editable && supported,
                                egui::Button::new("Заменить текстуру…"),
                            )
                            .clicked()
                        {
                            pending_action = Some(1);
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(!self.busy, egui::Button::new("Экспорт в PNG / DDS…"))
                            .clicked()
                        {
                            pending_action = Some(2);
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(editable, egui::Button::new("Переименовать"))
                            .clicked()
                        {
                            pending_action = Some(3);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(editable, egui::Button::new("Удалить"))
                            .clicked()
                        {
                            pending_action = Some(4);
                            ui.close_menu();
                        }
                    });
                }
            });
        self.select_index(selected);
        match pending_action {
            Some(1) => self.import(true, ctx),
            Some(2) => self.export(false, false, ctx),
            Some(3) => {
                self.renaming = true;
            }
            Some(4) => {
                let index = self.selected;
                self.edit(ctx, move |d| d.remove(index));
            }
            _ => {}
        }
    }
    fn viewer(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        editable: bool,
        editable_texture: bool,
    ) {
        if let Some((entry_name, w, h, fmt, mipmaps, supported)) =
            self.entries.get(self.selected).map(|e| {
                (
                    e.name.clone(),
                    e.width,
                    e.height,
                    e.format.clone(),
                    e.mipmaps,
                    e.supported,
                )
            })
        {
            let meta_text = format!("{w} × {h} · {fmt} · Mip: {mipmaps}");
            let meta_galley = ui.painter().layout_no_wrap(
                meta_text.clone(),
                egui::FontId::proportional(12.0),
                theme::TEXT_SECONDARY,
            );
            let total_w = ui.available_width();
            let left_w = (total_w - meta_galley.size().x - 12.0).max(80.0);
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(left_w, theme::CONTROL_H),
                    egui::Layout::left_to_right(Align::Center),
                    |ui| {
                        if self.renaming && editable {
                            let response = ui.add_sized(
                                [(left_w - 92.0).max(60.0), theme::CONTROL_H],
                                egui::TextEdit::singleline(&mut self.rename)
                                    .id_salt("txd_rename"),
                            );
                            if ui
                                .add(
                                    egui::Button::new("OK")
                                        .frame(false)
                                        .min_size(Vec2::new(0.0, theme::CONTROL_H)),
                                )
                                .clicked()
                                || (response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                            {
                                let index = self.selected;
                                let name = self.rename.clone();
                                self.renaming = false;
                                self.edit(ctx, move |d| d.rename(index, &name));
                            }
                            if ui
                                .add(
                                    egui::Button::new("Отмена")
                                        .frame(false)
                                        .min_size(Vec2::new(0.0, theme::CONTROL_H)),
                                )
                                .clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                self.rename = entry_name.clone();
                                self.renaming = false;
                            }
                        } else {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&entry_name).size(13.5).strong(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&entry_name);
                            if editable
                                && theme::icon_button(
                                    ui,
                                    theme::Icon::Pencil,
                                    "Переименовать текстуру",
                                    true,
                                )
                                .clicked()
                            {
                                self.rename = entry_name.clone();
                                self.renaming = true;
                            }
                            if !supported {
                                ui.label(
                                    egui::RichText::new("только чтение")
                                        .size(11.5)
                                        .color(theme::STATUS_MODIFIED),
                                );
                            }
                        }
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(meta_text)
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY),
                    );
                });
            });
            ui.add_space(4.0);
        }

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 5.0);
            let previous = self.channel;
            let channel_label = match self.channel {
                1 => "Канал: RGB",
                2 => "Канал: Альфа",
                _ => "Канал: RGBA",
            };
            theme::panel_menu(ui, channel_label, |ui| {
                for (ch, label) in [(0, "RGBA"), (1, "RGB (без альфы)"), (2, "Альфа-маска")] {
                    if ui.radio_value(&mut self.channel, ch, label).clicked() {
                        ui.close_menu();
                    }
                }
            });
            if previous != self.channel {
                self.revision += 1;
                self.image = None;
                self.requested.clear();
            }

            let backdrop_label = match self.backdrop {
                1 => "Фон: Чёрный",
                2 => "Фон: Серый",
                3 => "Фон: Белый",
                _ => "Фон: Сетка",
            };
            theme::panel_menu(ui, backdrop_label, |ui| {
                for (bg, label) in [(0, "Сетка"), (1, "Чёрный"), (2, "Серый"), (3, "Белый")] {
                    if ui.radio_value(&mut self.backdrop, bg, label).clicked() {
                        ui.close_menu();
                    }
                }
            });

            let zoom_label = if self.fit {
                format!("Масштаб: Авто ({:.0}%)", self.zoom * 100.0)
            } else {
                format!("Масштаб: {:.0}%", self.zoom * 100.0)
            };
            theme::panel_menu(ui, zoom_label, |ui| {
                if ui.radio(self.fit, "По размеру окна").clicked() {
                    self.fit = true;
                    self.pan = Vec2::ZERO;
                    ui.close_menu();
                }
                ui.separator();
                for percent in [25, 50, 100, 200, 400, 800] {
                    if ui
                        .selectable_label(
                            !self.fit && (self.zoom * 100.0 - percent as f32).abs() < 0.5,
                            format!("{percent}%"),
                        )
                        .clicked()
                    {
                        self.fit = false;
                        self.zoom = percent as f32 / 100.0;
                        self.pan = Vec2::ZERO;
                        ui.close_menu();
                    }
                }
            });
        });
        ui.add_space(4.0);
        theme::hline(ui);

        if !self.status.is_empty() || self.busy {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if self.busy {
                    ui.spinner();
                }
                if !self.status.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.status)
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY),
                    );
                }
            });
        }

        ui.add_space(4.0);
        if self.image.is_none() && !self.entries.is_empty() {
            self.request_image(self.selected, false, ctx);
        }
        if let Some(image) = &self.image {
            texture_canvas(
                ui,
                image,
                self.backdrop,
                &mut self.fit,
                &mut self.zoom,
                &mut self.pan,
            );
        } else if self.entries.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak("В словаре нет текстур. Перетащите PNG/DDS или нажмите «Добавить».");
            });
        } else if !editable_texture && editable {
            ui.centered_and_justified(|ui| {
                ui.weak("Загрузка текстуры…");
            });
        }
    }
}

fn texture_canvas(
    ui: &mut egui::Ui,
    image: &egui::TextureHandle,
    backdrop: u8,
    fit: &mut bool,
    zoom: &mut f32,
    pan: &mut Vec2,
) {
    let available = ui.available_size().max(Vec2::splat(48.0));
    let original = image.size_vec2().max(Vec2::splat(1.0));
    if *fit {
        *zoom = ((available.x - 16.0) / original.x)
            .min((available.y - 16.0) / original.y)
            .clamp(0.05, 4.0);
        *pan = Vec2::ZERO;
    }

    let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
    ui.painter().rect_filled(rect, 0.0, theme::BG_INPUT);

    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.01 {
            *fit = false;
            *zoom = (*zoom * (scroll * 0.0025).exp()).clamp(0.1, 16.0);
        }
    }
    if response.dragged() {
        *fit = false;
        *pan += ui.input(|i| i.pointer.delta());
    }
    if response.double_clicked() {
        *fit = !*fit;
        if !*fit {
            *zoom = 1.0;
        }
        *pan = Vec2::ZERO;
    }

    let size = (original * *zoom).max(Vec2::splat(1.0));
    let canvas = egui::Rect::from_center_size(rect.center() + *pan, size);

    let scoped = ui.painter().with_clip_rect(rect.shrink(1.0));
    match backdrop {
        1 => {
            scoped.rect_filled(canvas, 0.0, Color32::from_rgb(16, 18, 20));
        }
        2 => {
            scoped.rect_filled(canvas, 0.0, Color32::from_rgb(96, 100, 106));
        }
        3 => {
            scoped.rect_filled(canvas, 0.0, Color32::from_rgb(230, 232, 236));
        }
        _ => theme::draw_checkerboard(&scoped, canvas, 16.0),
    }
    let tex_rect = canvas.intersect(rect.shrink(1.0));
    if tex_rect.is_positive() {
        let uv_min_x = (tex_rect.left() - canvas.left()) / canvas.width().max(1.0);
        let uv_min_y = (tex_rect.top() - canvas.top()) / canvas.height().max(1.0);
        let uv_max_x = (tex_rect.right() - canvas.left()) / canvas.width().max(1.0);
        let uv_max_y = (tex_rect.bottom() - canvas.top()) / canvas.height().max(1.0);
        scoped.image(
            image.id(),
            tex_rect,
            egui::Rect::from_min_max(
                egui::pos2(uv_min_x, uv_min_y),
                egui::pos2(uv_max_x, uv_max_y),
            ),
            Color32::WHITE,
        );
    }
    scoped.rect_stroke(
        canvas,
        0.0,
        Stroke::new(1.0_f32, theme::BORDER_LINE),
        egui::StrokeKind::Outside,
    );
    ui.painter().rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, theme::BORDER_LINE),
        egui::StrokeKind::Inside,
    );
    response.on_hover_text("Колесо — масштаб · Перетаскивание — сдвиг · 2×клик — 1:1 / Вписать");
}

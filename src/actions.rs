use crate::{
    Browser, EntryRef, Message, platform, preview, storage, theme, txd, txd_document, vinyls,
};
use eframe::egui;
use province_archive::{Archive, Edit, Profile, pack_directory};
use std::{path::PathBuf, sync::Arc, thread};

impl Browser {
    pub(crate) fn sync_window_title(&mut self, ctx: &egui::Context) {
        let app = concat!("OpenProvince ", env!("CARGO_PKG_VERSION"));
        let dirty = !self.edits.is_empty()
            || self.txd_editor.as_ref().is_some_and(|e| e.disk_dirty);
        let prefix = if dirty { "(Не сохранено) " } else { "" };
        let title = if let Some(txd_path) = &self.standalone_txd
            && self
                .txd_editor
                .as_ref()
                .is_some_and(|e| e.disk_path.is_some())
        {
            let name = txd_path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_else(|| "texture.txd".into());
            format!("{prefix}{name} — {app}")
        } else if !self.archives.is_empty() {
            let active = self.archive_label(self.active_archive);
            if self.archives.len() > 1 {
                format!("{prefix}{active} (+{}) — {app}", self.archives.len() - 1)
            } else {
                format!("{prefix}{active} — {app}")
            }
        } else if let Some(folder_name) = self.folder.file_name().filter(|n| !n.is_empty()) {
            format!("{} — {app}", folder_name.to_string_lossy())
        } else {
            app.to_owned()
        };
        if title != self.last_window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_window_title = title;
        }
    }

    pub(crate) fn primary_archive(&self) -> Option<&Arc<Archive>> {
        self.archives
            .get(self.active_archive)
            .or_else(|| self.archives.first())
    }

    pub(crate) fn archive_label(&self, idx: usize) -> String {
        self.archives
            .get(idx)
            .and_then(|a| a.path().file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("Archive {idx}"))
    }

    pub(crate) fn select(&mut self, target: EntryRef, ctx: &egui::Context) {
        self.load_entry(target, 0, true, ctx);
    }

    pub(crate) fn load_entry(
        &mut self,
        target: EntryRef,
        variant_index: usize,
        refresh_variants: bool,
        ctx: &egui::Context,
    ) {
        if self.txd_editor.as_ref().is_some_and(|e| e.busy) {
            self.status = "Дождитесь завершения операции с текстурой.".into();
            if let Some(editor) = &self.txd_editor {
                self.selected = Some(editor.target);
            }
            return;
        }
        let Some(archive) = self.archives.get(target.archive).cloned() else {
            return;
        };
        self.standalone_txd = None;
        let is_dff = archive
            .entries()
            .get(target.index)
            .is_some_and(|e| e.name.to_ascii_lowercase().ends_with(".dff"));
        self.player_skin_mode = is_dff && preview::is_player_archive(&archive);
        self.loading_model = is_dff;
        self.loading_textures = is_dff;
        self.selected = Some(target);
        self.txd_editor = None;
        if refresh_variants {
            self.preview = "Загрузка…".into();
            self.variants.clear();
            self.variant_selected = 0;
            self.vinyl_selected = None;
            self.vinyl_texture = None;
            self.vinyl_revision += 1;
        } else {
            self.variant_selected = variant_index;
        }
        self.images.clear();
        self.base_images.clear();
        self.remap_targets.clear();
        self.vinyl_capable = false;
        self.vinyl_on_primary = false;
        self.model = None;
        self.model_error.clear();
        self.generation += 1;
        let generation = self.generation;
        self.preview_epoch
            .store(generation, std::sync::atomic::Ordering::Relaxed);
        let epoch = self.preview_epoch.clone();
        let opened = self.archives.clone();
        let chosen_variant_txd = if variant_index == 0 {
            None
        } else {
            self.variants.get(variant_index - 1).map(|v| v.name.clone())
        };
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        let entry_name = archive
            .entries()
            .get(target.index)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let is_txd = entry_name.to_ascii_lowercase().ends_with(".txd");
        let replacement = if Some(target.archive) == self.edited_archive {
            archive.entries().get(target.index).and_then(|entry| {
                self.edits.iter().find_map(|edit| match edit {
                    Edit::Replace { name, source } if name.eq_ignore_ascii_case(&entry.name) => {
                        Some(source.clone())
                    }
                    _ => None,
                })
            })
        } else {
            None
        };
        thread::spawn(move || {
            let aborted = || epoch.load(std::sync::atomic::Ordering::Relaxed) != generation;
            if aborted() {
                return;
            }
            let result = if let Some(path) = replacement {
                std::fs::read(path).map_err(|e| e.to_string())
            } else {
                archive
                    .read(target.index, &Profile::inspected_client())
                    .map_err(|e| e.to_string())
            };
            if aborted() {
                return;
            }
            let send_preview = || {
                if refresh_variants {
                    let report = result
                        .as_ref()
                        .map(|bytes| platform::format_hex_report(&entry_name, bytes))
                        .map_err(Clone::clone);
                    let _ = sender.send(Message::Preview(generation, report));
                }
            };
            if is_dff {
                if preview::is_player_archive(&archive) {
                    if refresh_variants {
                        let _ = sender.send(Message::Variants(
                            generation,
                            preview::variants_with_archives(&archive, target.index, &opened),
                        ));
                    }
                    match preview::load_model_with_variant(
                        &archive,
                        target.index,
                        &opened,
                        false,
                        chosen_variant_txd.as_deref(),
                    ) {
                        Ok((model, textures)) => {
                            let _ = sender.send(Message::Model(generation, Ok(model)));
                            let _ = sender.send(Message::Images(generation, textures));
                            let _ = sender.send(Message::ImagesDone(generation));
                        }
                        Err(error) => {
                            let _ = sender.send(Message::Model(generation, Err(error)));
                        }
                    }
                    send_preview();
                } else {
                    match result
                        .as_ref()
                        .map_err(Clone::clone)
                        .and_then(|bytes| preview::decode_model_from_bytes(&archive, target.index, bytes))
                    {
                        Ok(model) => {
                            let wanted = preview::model_wanted_textures(&model);
                            let _ = sender.send(Message::Model(generation, Ok(model)));
                            ctx.request_repaint();
                            if aborted() {
                                return;
                            }
                            if refresh_variants {
                                let _ = sender.send(Message::Variants(
                                    generation,
                                    preview::variants_with_archives(&archive, target.index, &opened),
                                ));
                                send_preview();
                                ctx.request_repaint();
                            }
                            preview::load_textures_for_model(
                                &archive,
                                target.index,
                                &opened,
                                &wanted,
                                &aborted,
                                |batch| {
                                    let _ = sender.send(Message::Images(generation, batch));
                                    ctx.request_repaint();
                                },
                            );
                            let _ = sender.send(Message::ImagesDone(generation));
                        }
                        Err(error) => {
                            let _ = sender.send(Message::Model(generation, Err(error.clone())));
                            if refresh_variants {
                                let _ = sender.send(Message::Preview(generation, Err(error)));
                            }
                        }
                    }
                }
            } else if is_txd {
                let document = result
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|bytes| txd_document::Document::parse(bytes));
                let _ = sender.send(Message::Txd(generation, document));
                send_preview();
            } else {
                let images = result
                    .as_ref()
                    .map_or_else(|_| Vec::new(), |bytes| txd::previews(bytes));
                let _ = sender.send(Message::Images(generation, images));
                send_preview();
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn select_variant(&mut self, index: usize, ctx: &egui::Context) {
        if self.player_skin_mode
            && let Some(target) = self.selected
        {
            self.load_entry(target, index, false, ctx);
            return;
        }
        self.variant_selected = index;
        self.variant_revision += 1;
        self.images.clone_from(&self.base_images);
        if index == 0 {
            return;
        }
        let Some(variant) = self.variants.get(index - 1).cloned() else {
            return;
        };
        let targets = self.remap_targets.clone();
        let generation = self.generation;
        let revision = self.variant_revision;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = preview::load_variant(&variant, &targets);
            let _ = sender.send(Message::VariantImages(generation, revision, result));
            ctx.request_repaint();
        });
    }

    pub(crate) fn ensure_vinyls(&mut self) {
        if !self.vinyls_loaded {
            self.reload_vinyls();
        }
    }

    pub(crate) fn reload_vinyls(&mut self) {
        let mut anchors = vec![self.folder.clone()];
        for archive in &self.archives {
            anchors.push(archive.path().to_path_buf());
        }
        self.vinyls = vinyls::discover(&anchors, self.vinyl_dir.as_deref());
        self.vinyls_loaded = true;
        if let Some(sel) = self.vinyl_selected
            && sel >= self.vinyls.len()
        {
            self.vinyl_selected = None;
            self.vinyl_texture = None;
        }
    }

    pub(crate) fn select_vinyl(&mut self, index: Option<usize>, ctx: &egui::Context) {
        self.vinyl_selected = index;
        self.vinyl_revision += 1;
        let revision = self.vinyl_revision;
        let Some(entry) = index.and_then(|i| self.vinyls.get(i).cloned()) else {
            self.vinyl_texture = None;
            return;
        };
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = vinyls::load_texture(&entry);
            let _ = sender.send(Message::Vinyl(revision, result));
            ctx.request_repaint();
        });
    }

    pub(crate) fn merge_images(&mut self, textures: Vec<txd::Texture>, ctx: &egui::Context) {
        for texture in textures {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [texture.width, texture.height],
                &texture.rgba,
            );
            let handle = ctx.load_texture(
                &texture.name,
                image,
                egui::TextureOptions {
                    wrap_mode: egui::TextureWrapMode::Repeat,
                    ..egui::TextureOptions::LINEAR
                },
            );
            self.images
                .retain(|(name, _)| !name.eq_ignore_ascii_case(&texture.name));
            self.images.push((texture.name, handle));
        }
    }

    pub(crate) fn stage(&mut self, edit: Edit) {
        if self.edited_archive.is_none() {
            self.edited_archive = Some(self.active_archive);
        }
        if let Edit::Replace { name, .. } | Edit::Remove { name } = &edit {
            self.edits.retain(|current| match current {
                Edit::Replace { name: current, .. } | Edit::Remove { name: current } => {
                    !current.eq_ignore_ascii_case(name)
                }
                Edit::Add { .. } => true,
            });
        }
        self.edits.push(edit);
    }

    pub(crate) fn add_files(&mut self, paths: Vec<PathBuf>) {
        if !self.edits.is_empty() && self.edited_archive != Some(self.active_archive) {
            self.status = "Сначала сохраните (Ctrl+S) или отмените правки другого архива.".into();
            return;
        }
        let Some(archive) = self.primary_archive().cloned() else {
            return;
        };
        if self.edits.is_empty() {
            self.edited_archive = Some(self.active_archive);
        }
        for source in paths {
            let name = source
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if let Some(entry) = archive
                .entries()
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(name))
            {
                self.stage(Edit::Replace {
                    name: entry.name.clone(),
                    source,
                });
            } else {
                self.edits.retain(|edit| {
                    !matches!(edit, Edit::Add { source: current } if current.file_name() == source.file_name())
                });
                self.stage(Edit::Add { source });
            }
        }
    }

    pub(crate) fn load_txd(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("RenderWare textures", &["txd"])
            .pick_file()
        else {
            return;
        };
        let generation = self.generation;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            if let Ok(bytes) = std::fs::read(&path) {
                let decoded = Profile::inspected_client().decode_file(&path, &bytes);
                let _ = sender.send(Message::Images(generation, txd::previews(&decoded)));
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("txd"))
        {
            self.open_txd_file(path, ctx);
        } else {
            self.open_many(vec![path], false, ctx);
        }
    }

    pub(crate) fn open_txd_file(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.txd_editor.as_ref().is_some_and(|e| e.busy) {
            self.status = "Дождитесь завершения операции с текстурой.".into();
            return;
        }
        if self.txd_editor.as_ref().is_some_and(|e| e.disk_dirty) {
            self.status = "Сначала сохраните (Ctrl+S) или закройте изменённый TXD.".into();
            return;
        }
        self.busy = true;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|raw| {
                    let decoded = Profile::inspected_client().decode_file(&path, &raw);
                    txd_document::Document::parse(&decoded).map(|doc| (doc, decoded))
                });
            let _ = sender.send(Message::StandaloneTxd(path, result));
            ctx.request_repaint();
        });
    }

    pub(crate) fn open_many(&mut self, paths: Vec<PathBuf>, append: bool, ctx: &egui::Context) {
        if self.txd_editor.as_ref().is_some_and(|e| e.busy) {
            self.status = "Дождитесь завершения операции с текстурой.".into();
            return;
        }
        if paths.is_empty() {
            return;
        }
        if !append && !self.edits.is_empty() {
            self.status =
                "Сохраните или отмените несохранённые изменения перед открытием нового набора"
                    .into();
            return;
        }
        let existing: std::collections::BTreeSet<String> = if append {
            self.archives
                .iter()
                .map(|a| a.path().to_string_lossy().to_ascii_lowercase())
                .collect()
        } else {
            Default::default()
        };
        let paths: Vec<PathBuf> = paths
            .into_iter()
            .filter(|p| !existing.contains(&p.to_string_lossy().to_ascii_lowercase()))
            .collect();
        if paths.is_empty() {
            self.status = "Выбранные архивы уже открыты.".into();
            return;
        }
        self.busy = true;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let mut opened = Vec::with_capacity(paths.len());
            for path in paths {
                match Archive::open(&path) {
                    Ok(archive) => opened.push(archive),
                    Err(error) => {
                        let _ = sender.send(Message::Open {
                            result: Err(format!("{}: {error}", path.display())),
                            append,
                        });
                        ctx.request_repaint();
                        return;
                    }
                }
            }
            let _ = sender.send(Message::Open {
                result: Ok(opened),
                append,
            });
            ctx.request_repaint();
        });
    }

    pub(crate) fn close_archive(&mut self, index: usize, ctx: &egui::Context) {
        if self.busy || self.txd_editor.as_ref().is_some_and(|e| e.busy) {
            return;
        }
        if index >= self.archives.len() {
            return;
        }
        if !self.edits.is_empty() && self.edited_archive == Some(index) {
            self.status = "Нельзя закрыть архив с несохранёнными изменениями.".into();
            return;
        }
        if self
            .txd_editor
            .as_ref()
            .is_some_and(|e| e.disk_path.is_none() && e.target.archive == index)
        {
            self.txd_editor = None;
        }
        self.archives.remove(index);
        self.row_cache = None;
        if let Some(edited) = self.edited_archive {
            self.edited_archive = match edited.cmp(&index) {
                std::cmp::Ordering::Greater => Some(edited - 1),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Less => Some(edited),
            };
        }
        if let Some(sel) = self.selected {
            self.selected = match sel.archive.cmp(&index) {
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some(EntryRef {
                    archive: sel.archive - 1,
                    index: sel.index,
                }),
                std::cmp::Ordering::Less => Some(sel),
            };
        }
        self.selection = self
            .selection
            .iter()
            .filter_map(|&item| match item.archive.cmp(&index) {
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some(EntryRef {
                    archive: item.archive - 1,
                    index: item.index,
                }),
                std::cmp::Ordering::Less => Some(item),
            })
            .collect();
        if self.active_archive > index {
            self.active_archive -= 1;
        } else if self.active_archive >= self.archives.len() {
            self.active_archive = 0;
        }
        if self.archives.len() <= 1 {
            self.all_tabs = false;
        }
        if self.archives.is_empty() {
            self.model = None;
            self.images.clear();
            self.base_images.clear();
            self.variants.clear();
            self.preview.clear();
            self.browse(self.folder.clone(), ctx);
        } else {
            self.update_archive_status();
        }
    }

    pub(crate) fn update_archive_status(&mut self) {
        let total: usize = self.archives.iter().map(|a| a.entries().len()).sum();
        if self.archives.len() == 1 {
            let a = &self.archives[0];
            self.address = a.path().display().to_string();
            let name = a
                .path()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            self.status = format!(
                "{name} — {} файлов · {}",
                theme::format_thousands(a.entries().len() as u64),
                theme::format_size(a.file_len())
            );
        } else {
            self.address = self.folder.display().to_string();
            self.status = format!(
                "Открыто архивов: {} — {} файлов",
                self.archives.len(),
                theme::format_thousands(total as u64)
            );
        }
    }

    pub(crate) fn apply_saved_archive(&mut self, index: usize, archive: Archive) {
        let Some(previous) = self.archives.get(index) else {
            return;
        };
        let resolve = |target: EntryRef| -> Option<EntryRef> {
            if target.archive != index {
                return Some(target);
            }
            let name = &previous.entries().get(target.index)?.name;
            archive
                .entries()
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(name))
                .map(|entry| EntryRef {
                    archive: index,
                    index: entry.index,
                })
        };
        self.selected = self.selected.and_then(resolve);
        self.selection = self
            .selection
            .iter()
            .filter_map(|target| resolve(*target))
            .collect();
        if let Some(editor) = &mut self.txd_editor
            && editor.target.archive == index
        {
            if let Some(target) = resolve(editor.target) {
                editor.target = target;
                editor.mark_saved();
            } else {
                self.txd_editor = None;
            }
        }
        self.archives[index] = Arc::new(archive);
        self.edits.clear();
        self.edited_archive = None;
        self.row_cache = None;
        self.update_archive_status();
        self.status = format!("Сохранено: {}", self.archive_label(index));
    }

    pub(crate) fn save(&mut self, ctx: &egui::Context) {
        if let Some(editor) = &mut self.txd_editor
            && editor.disk_path.is_some()
        {
            if let Some(output) = rfd::FileDialog::new()
                .add_filter("RenderWare TXD", &["txd"])
                .set_file_name(&editor.name)
                .save_file()
            {
                editor.save_to_disk(Some(output), ctx);
            }
            return;
        }
        let Some(archive) = self.primary_archive().cloned() else {
            return;
        };
        let Some(output) = rfd::FileDialog::new()
            .add_filter("IMG archives", &["img"])
            .set_file_name("OpenProvince.img")
            .save_file()
        else {
            return;
        };
        self.busy = true;
        let target_idx = self.active_archive;
        let edits = self.edits.clone();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = archive
                .save_as(&output, &edits, &Profile::inspected_client())
                .and_then(|_| Archive::open(output))
                .map_err(|e| e.to_string());
            let _ = sender.send(Message::SavedArchive(target_idx, result));
            ctx.request_repaint();
        });
    }

    pub(crate) fn save_current(&mut self, ctx: &egui::Context) {
        if let Some(editor) = &mut self.txd_editor
            && editor.disk_path.is_some()
        {
            if editor.disk_dirty {
                editor.save_to_disk(None, ctx);
            }
            return;
        }
        if self.edits.is_empty() {
            return;
        }
        let target_idx = self.edited_archive.unwrap_or(self.active_archive);
        let Some(archive) = self.archives.get(target_idx).cloned() else {
            return;
        };
        self.busy = true;
        self.status = "Сохранение архива с резервной копией…".into();
        let edits = self.edits.clone();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = storage::save(&archive, &edits)
                .and_then(|_| Archive::open(archive.path()).map_err(|e| e.to_string()));
            let _ = sender.send(Message::SavedArchive(target_idx, result));
            ctx.request_repaint();
        });
    }

    pub(crate) fn pack(&mut self, ctx: &egui::Context) {
        let Some(input) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let Some(output) = rfd::FileDialog::new()
            .add_filter("IMG archives", &["img"])
            .set_file_name("OpenProvince.img")
            .save_file()
        else {
            return;
        };
        self.busy = true;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = pack_directory(&input, &output, &Profile::inspected_client())
                .and_then(|_| Archive::open(output).map(|a| vec![a]))
                .map_err(|e| e.to_string());
            let _ = sender.send(Message::Open {
                result,
                append: false,
            });
            ctx.request_repaint();
        });
    }

    pub(crate) fn export(&mut self, all: bool, ctx: &egui::Context) {
        if self.archives.is_empty() {
            return;
        }
        let Some(output) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let archives = self.archives.clone();
        let active_archive = self.active_archive;
        let all_tabs = self.all_tabs;
        let selected = self.selection.clone();
        self.busy = true;
        self.cancel
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let cancel = self.cancel.clone();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let mut count = 0;
            let mut warnings = 0;
            let targets: Vec<EntryRef> = if all {
                if all_tabs {
                    archives
                        .iter()
                        .enumerate()
                        .flat_map(|(ai, a)| {
                            (0..a.entries().len()).map(move |ei| EntryRef {
                                archive: ai,
                                index: ei,
                            })
                        })
                        .collect()
                } else if let Some(a) = archives.get(active_archive) {
                    (0..a.entries().len())
                        .map(|ei| EntryRef {
                            archive: active_archive,
                            index: ei,
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                selected.into_iter().collect()
            };
            let total = targets.len();
            for item in targets {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = sender.send(Message::Status(format!(
                        "Экспорт отменён; сохранено файлов: {count}."
                    )));
                    ctx.request_repaint();
                    return;
                }
                let Some(archive) = archives.get(item.archive) else {
                    continue;
                };
                match archive.extract(item.index, &Profile::inspected_client(), &output) {
                    Ok(result) => {
                        count += 1;
                        warnings += usize::from(result.warning.is_some());
                    }
                    Err(error) => {
                        let _ = sender.send(Message::Status(format!(
                            "Остановлено после {count} файлов: {error}"
                        )));
                        ctx.request_repaint();
                        return;
                    }
                }
                if count % 16 == 0 || count == total {
                    let _ = sender.send(Message::Progress(count, total));
                    ctx.request_repaint();
                }
            }
            let _ = sender.send(Message::Status(format!(
                "Извлечено файлов: {count}; предупреждений структуры: {warnings}. {}",
                output.display()
            )));
            ctx.request_repaint();
        });
    }
}

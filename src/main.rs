#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use province_archive::{Archive, Edit, Profile, inspect, pack_directory};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};
mod dff;
mod gpu;
mod preview;
mod storage;
mod texture_editor;
mod theme;
mod thumbs;
mod txd;
mod txd_document;
mod ui;
mod vinyls;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) struct EntryRef {
    pub archive: usize,
    pub index: usize,
}

enum Message {
    Open {
        result: Result<Vec<Archive>, String>,
        append: bool,
    },
    Preview(usize, Result<Vec<u8>, String>),
    Images(usize, Vec<txd::Texture>),
    Model(usize, Result<dff::Model, String>),
    Txd(usize, Result<txd_document::Document, String>),
    SavedArchive(usize, Result<Archive, String>),
    Variants(usize, Vec<preview::Variant>),
    VariantImages(usize, usize, Result<Vec<txd::Texture>, String>),
    Vinyl(usize, Result<txd::Texture, String>),
    StandaloneTxd(PathBuf, Result<(txd_document::Document, Vec<u8>), String>),
    Progress(usize, usize),
    Status(String),
    Folder(Result<(PathBuf, Vec<ui::File>), String>),
}

struct Browser {
    folder: PathBuf,
    folder_files: Vec<ui::File>,
    address: String,
    preview_open: bool,
    help_open: bool,
    about_open: bool,
    archives: Vec<Arc<Archive>>,
    active_archive: usize,
    edited_archive: Option<usize>,
    all_tabs: bool,
    group_by_archive: bool,
    standalone_txd: Option<PathBuf>,
    filter: String,
    selected: Option<EntryRef>,
    preview: String,
    images: Vec<(String, egui::TextureHandle)>,
    base_images: Vec<(String, egui::TextureHandle)>,
    variants: Vec<preview::Variant>,
    variant_selected: usize,
    variant_revision: usize,
    remap_targets: Vec<String>,
    vinyls: Vec<vinyls::VinylEntry>,
    vinyls_loaded: bool,
    vinyl_dir: Option<PathBuf>,
    vinyl_filter: String,
    vinyl_selected: Option<usize>,
    vinyl_revision: usize,
    vinyl_texture: Option<egui::TextureHandle>,
    model: Option<dff::View>,
    model_error: String,
    txd_editor: Option<texture_editor::Editor>,
    player_skin_mode: bool,
    assemble_player_skin: bool,
    show_details: bool,
    generation: usize,
    preview_epoch: Arc<std::sync::atomic::AtomicUsize>,
    preview_lock: Arc<std::sync::Mutex<()>>,
    selection: std::collections::BTreeSet<EntryRef>,

    sort: usize,
    descending: bool,
    col_widths: [f32; 3],
    type_filter: u8,
    focus_search: bool,
    changes_open: bool,
    row_cache: Option<ui::Rows>,
    scroll_selection: bool,
    progress: Option<(usize, usize)>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    status: String,
    busy: bool,
    edits: Vec<Edit>,
    initial_selection: Option<String>,
    initial_variant: Option<String>,
    sender: mpsc::Sender<Message>,
    receiver: mpsc::Receiver<Message>,
}

impl Browser {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            folder: std::env::current_dir().unwrap_or_default(),
            folder_files: Vec::new(),
            address: String::new(),
            preview_open: true,
            help_open: false,
            about_open: false,
            archives: Vec::new(),
            active_archive: 0,
            edited_archive: None,
            all_tabs: false,
            group_by_archive: true,
            standalone_txd: None,
            filter: String::new(),
            selected: None,
            preview: String::new(),
            images: Vec::new(),
            base_images: Vec::new(),
            variants: Vec::new(),
            variant_selected: 0,
            variant_revision: 0,
            remap_targets: Vec::new(),
            vinyls: Vec::new(),
            vinyls_loaded: false,
            vinyl_dir: None,
            vinyl_filter: String::new(),
            vinyl_selected: None,
            vinyl_revision: 0,
            vinyl_texture: None,
            model: None,
            model_error: String::new(),
            txd_editor: None,
            player_skin_mode: false,
            assemble_player_skin: false,
            show_details: false,
            generation: 0,
            preview_epoch: Default::default(),
            preview_lock: Default::default(),
            selection: Default::default(),

            sort: 0,
            descending: false,
            col_widths: [108.0, 56.0, 84.0],
            type_filter: 0,
            focus_search: false,
            changes_open: false,
            row_cache: None,
            scroll_selection: false,
            progress: None,
            cancel: Default::default(),
            status: "Откройте один или несколько архивов Province / GTA IMG".into(),
            busy: false,
            edits: Vec::new(),
            initial_selection: None,
            initial_variant: None,
            sender,
            receiver,
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

    fn select(&mut self, target: EntryRef, ctx: &egui::Context) {
        self.load_entry(target, 0, true, ctx);
    }

    pub(crate) fn reload_player_preview(&mut self, ctx: &egui::Context) {
        if let Some(target) = self.selected {
            self.load_entry(target, self.variant_selected, false, ctx);
        }
    }

    fn load_entry(
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
        self.selected = Some(target);
        self.txd_editor = None;
        if refresh_variants {
            self.preview = "Decoding…".into();
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
        self.model = None;
        self.model_error.clear();
        self.generation += 1;
        let generation = self.generation;
        self.preview_epoch
            .store(generation, std::sync::atomic::Ordering::Relaxed);
        let epoch = self.preview_epoch.clone();
        let lock = self.preview_lock.clone();
        let opened = self.archives.clone();
        let assemble = self.assemble_player_skin;
        let chosen_variant_txd = if variant_index == 0 {
            None
        } else {
            self.variants.get(variant_index - 1).map(|v| v.name.clone())
        };
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        let is_txd = archive
            .entries()
            .get(target.index)
            .is_some_and(|e| e.name.to_ascii_lowercase().ends_with(".txd"));
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
            let Ok(_guard) = lock.lock() else {
                return;
            };
            if epoch.load(std::sync::atomic::Ordering::Relaxed) != generation {
                return;
            }
            let result = if let Some(path) = replacement {
                std::fs::read(path).map_err(|e| e.to_string())
            } else {
                archive
                    .read(target.index, &Profile::inspected_client())
                    .map_err(|e| e.to_string())
            };
            if is_dff {
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
                    assemble,
                    chosen_variant_txd.as_deref(),
                ) {
                    Ok((model, textures)) => {
                        let _ = sender.send(Message::Model(generation, Ok(model)));
                        let _ = sender.send(Message::Images(generation, textures));
                    }
                    Err(error) => {
                        let _ = sender.send(Message::Model(generation, Err(error)));
                    }
                }
            } else if is_txd {
                let document = result
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|bytes| txd_document::Document::parse(bytes));
                let _ = sender.send(Message::Txd(generation, document));
            } else {
                let images = result
                    .as_ref()
                    .map_or_else(|_| Vec::new(), |bytes| txd::previews(bytes));
                let _ = sender.send(Message::Images(generation, images));
            }
            if refresh_variants {
                let _ = sender.send(Message::Preview(generation, result));
            }
            ctx.request_repaint();
        });
    }

    fn select_variant(&mut self, index: usize, ctx: &egui::Context) {
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

    fn merge_images(&mut self, textures: Vec<txd::Texture>, ctx: &egui::Context) {
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

    fn stage(&mut self, edit: Edit) {
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

    fn add_files(&mut self, paths: Vec<PathBuf>) {
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
                self.edits.retain(|edit| !matches!(edit, Edit::Add { source: current } if current.file_name() == source.file_name()));
                self.stage(Edit::Add { source });
            }
        }
    }

    fn load_txd(&mut self, ctx: &egui::Context) {
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

    fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
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

    fn update_archive_status(&mut self) {
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

    fn apply_saved_archive(&mut self, index: usize, archive: Archive) {
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

    fn save(&mut self, ctx: &egui::Context) {
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

    fn save_current(&mut self, ctx: &egui::Context) {
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

    fn pack(&mut self, ctx: &egui::Context) {
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

    fn export(&mut self, all: bool, ctx: &egui::Context) {
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

impl eframe::App for Browser {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Some(editor) = &mut self.txd_editor
            && let Some(source) = editor.poll(ctx)
        {
            let target = editor.target;
            let name = editor.name.clone();
            if self.edits.is_empty() {
                self.edited_archive = Some(target.archive);
            }
            self.stage(Edit::Replace { name, source });
        }
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                Message::SavedArchive(index, result) => {
                    self.busy = false;
                    match result {
                        Ok(archive) => self.apply_saved_archive(index, archive),
                        Err(error) => self.status = error,
                    }
                }
                Message::Txd(generation, result) => {
                    if generation == self.generation {
                        match result {
                            Ok(document) => {
                                if let Some(target) = self.selected {
                                    let name = self
                                        .archives
                                        .get(target.archive)
                                        .and_then(|a| a.entries().get(target.index))
                                        .map_or(String::new(), |e| e.name.clone());
                                    self.txd_editor =
                                        Some(texture_editor::Editor::new(document, target, name));
                                }
                            }
                            Err(error) => self.model_error = error,
                        }
                    }
                }
                Message::Variants(generation, variants) => {
                    if generation == self.generation {
                        self.variants = variants;
                    }
                }
                Message::VariantImages(generation, revision, result) => {
                    if generation == self.generation && revision == self.variant_revision {
                        match result {
                            Ok(textures) => self.merge_images(textures, ctx),
                            Err(error) => self.status = error,
                        }
                    }
                }
                Message::Vinyl(revision, result) => {
                    if revision == self.vinyl_revision {
                        match result {
                            Ok(texture) => {
                                let image = egui::ColorImage::from_rgba_unmultiplied(
                                    [texture.width, texture.height],
                                    &texture.rgba,
                                );
                                self.vinyl_texture = Some(ctx.load_texture(
                                    format!("vinyl-{revision}-{}", texture.name),
                                    image,
                                    egui::TextureOptions::LINEAR,
                                ));
                            }
                            Err(error) => self.status = error,
                        }
                    }
                }
                Message::Images(index, textures) => {
                    if self.generation == index {
                        self.merge_images(textures, ctx);
                        self.base_images.clone_from(&self.images);
                        if let Some(name) = self.initial_variant.take()
                            && let Some(index) = self
                                .variants
                                .iter()
                                .position(|v| v.name.eq_ignore_ascii_case(&name))
                        {
                            self.select_variant(index + 1, ctx);
                        }
                    }
                }
                Message::Open { result, append } => {
                    self.busy = false;
                    match result {
                        Ok(new_archives) => {
                            if let Some(first) = new_archives.first() {
                                self.folder = first
                                    .path()
                                    .parent()
                                    .unwrap_or(std::path::Path::new("."))
                                    .to_owned();
                            }
                            if !append {
                                self.archives.clear();
                                self.active_archive = 0;
                                self.edited_archive = None;
                                self.all_tabs = false;
                                self.standalone_txd = None;
                                self.edits.clear();
                                self.selected = None;
                                self.selection.clear();
                                self.generation += 1;
                                self.preview_epoch
                                    .store(self.generation, std::sync::atomic::Ordering::Relaxed);
                                self.model = None;
                                self.txd_editor = None;
                                self.model_error.clear();
                                self.preview.clear();
                                self.images.clear();
                                self.base_images.clear();
                                self.variants.clear();
                                self.variant_selected = 0;
                            } else if !new_archives.is_empty() {
                                self.active_archive = self.archives.len();
                            }
                            for arch in new_archives {
                                self.archives.push(Arc::new(arch));
                            }
                            self.vinyls_loaded = false;
                            self.row_cache = None;
                            self.update_archive_status();
                            if let Some(name) = self.initial_selection.take()
                                && let Some(found) =
                                    self.archives.iter().enumerate().find_map(|(ai, a)| {
                                        a.entries()
                                            .iter()
                                            .find(|e| e.name.eq_ignore_ascii_case(&name))
                                            .map(|e| EntryRef {
                                                archive: ai,
                                                index: e.index,
                                            })
                                    })
                            {
                                self.select(found, ctx);
                                self.preview_open = true;
                                self.selection.insert(found);
                                self.scroll_selection = true;
                            }
                        }
                        Err(error) => self.status = error,
                    }
                }
                Message::Status(status) => {
                    self.busy = false;
                    self.progress = None;
                    self.status = status;
                }
                Message::Preview(index, result) => {
                    if self.generation == index {
                        self.preview = match result {
                            Err(error) => error,
                            Ok(bytes) => {
                                let name = self
                                    .selected
                                    .and_then(|sel| {
                                        self.archives
                                            .get(sel.archive)
                                            .and_then(|a| a.entries().get(sel.index))
                                    })
                                    .map(|e| e.name.as_str())
                                    .unwrap_or_default();
                                format_hex_report(name, &bytes)
                            }
                        };
                    }
                }
                Message::Model(index, result) => {
                    if index == self.generation {
                        match result {
                            Ok(model) => {
                                self.remap_targets = model
                                    .materials
                                    .iter()
                                    .map(|m| m.texture.clone())
                                    .filter(|name| {
                                        name.starts_with('#') || name.eq_ignore_ascii_case("remap")
                                    })
                                    .collect();
                                self.remap_targets.sort();
                                self.remap_targets.dedup();
                                self.model = Some(dff::View::new(model));
                            }
                            Err(error) => self.model_error = error,
                        }
                    }
                }
                Message::StandaloneTxd(path, result) => {
                    self.busy = false;
                    match result {
                        Ok((document, bytes)) => {
                            let name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("texture.txd")
                                .to_owned();
                            let count = document.len();
                            self.standalone_txd = Some(path.clone());
                            self.model = None;
                            self.model_error.clear();
                            self.images.clear();
                            self.base_images.clear();
                            self.variants.clear();
                            self.remap_targets.clear();
                            self.vinyl_selected = None;
                            self.vinyl_texture = None;
                            self.preview_open = true;
                            self.preview = format_hex_report(&name, &bytes);
                            self.txd_editor =
                                Some(texture_editor::Editor::new_standalone(document, path.clone()));
                            self.status = format!(
                                "{name} — {count} текстур · {}",
                                theme::format_size(bytes.len() as u64)
                            );
                        }
                        Err(error) => self.status = error,
                    }
                }
                Message::Progress(done, total) => self.progress = Some((done, total)),
                Message::Folder(result) => {
                    self.busy = false;
                    match result {
                        Ok((path, files)) => {
                            self.folder = path;
                            self.address = self.folder.display().to_string();
                            self.folder_files = files;
                            self.row_cache = None;
                            self.archives.clear();
                            self.active_archive = 0;
                            self.edited_archive = None;
                            self.all_tabs = false;
                            self.selected = None;
                            self.selection.clear();
                            self.preview_open = true;
                            self.generation += 1;
                            self.preview_epoch
                                .store(self.generation, std::sync::atomic::Ordering::Relaxed);
                            if self.standalone_txd.is_none() {
                                self.txd_editor = None;
                                self.model = None;
                                self.images.clear();
                                self.base_images.clear();
                                self.variants.clear();
                                self.preview.clear();
                                self.model_error.clear();
                                self.status = format!("{} элементов", self.folder_files.len());
                            }
                        }
                        Err(error) => self.status = error,
                    }
                }
            }
        }
        self.manager_ui(ctx);
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() && !self.busy {
            let imgs: Vec<PathBuf> = dropped
                .iter()
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("img")))
                .cloned()
                .collect();
            let txds: Vec<PathBuf> = dropped
                .iter()
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txd")))
                .cloned()
                .collect();
            let images: Vec<PathBuf> = dropped
                .iter()
                .filter(|p| {
                    p.extension().is_some_and(|e| {
                        ["png", "dds", "tga", "bmp", "jpg", "jpeg"]
                            .iter()
                            .any(|ext| e.eq_ignore_ascii_case(ext))
                    })
                })
                .cloned()
                .collect();
            if !imgs.is_empty() {
                self.open_many(imgs, false, ctx);
            } else if !images.is_empty()
                && images.len() == dropped.len()
                && let Some(editor) = &mut self.txd_editor
                && (editor.disk_path.is_some()
                    || self.edits.is_empty()
                    || self.edited_archive == Some(editor.target.archive))
            {
                editor.import_paths(images, ctx);
            } else if !self.archives.is_empty() {
                self.add_files(dropped);
            } else if let Some(txd) = txds.into_iter().next() {
                self.open_txd_file(txd, ctx);
            }
        }
    }
}

fn format_hex_report(name: &str, bytes: &[u8]) -> String {
    let report = inspect(name, bytes);
    let mut text = format!(
        "{}\nFormat: {}\nDecoded bytes: {}\nLogical bytes: {:?}\n{}\n\nHex preview (first 512 bytes)\n",
        name,
        report.format,
        bytes.len(),
        report.logical_size,
        report.warning.unwrap_or_default()
    );
    for (i, row) in bytes[..bytes.len().min(512)].chunks(16).enumerate() {
        text.push_str(&format!("{:08X}  ", i * 16));
        for byte in row {
            text.push_str(&format!("{byte:02X} "));
        }
        text.push_str("  ");
        for byte in row {
            text.push(if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                '.'
            });
        }
        text.push('\n');
    }
    text
}

fn main() -> eframe::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut initial_archives = Vec::new();
    let mut selected = None;
    let mut variant = None;
    let mut active = true;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--no-activate" => active = false,
            "--select" => selected = args.next(),
            "--variant" => variant = args.next(),
            "--export-thumbs" => {
                if let (Some(models), Some(vehs), Some(skins), Some(out)) =
                    (args.next(), args.next(), args.next(), args.next())
                    && let Err(error) = thumbs::export_catalog(
                        &PathBuf::from(models),
                        &PathBuf::from(vehs),
                        &PathBuf::from(skins),
                        &PathBuf::from(out),
                    )
                {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            "--validate-models" => {
                if let (Some(input), Some(output)) = (args.next(), args.next()) {
                    let mut report = Vec::new();
                    let mut queue = vec![PathBuf::from(input)];
                    while let Some(path) = queue.pop() {
                        if path.is_dir() {
                            if let Ok(files) = std::fs::read_dir(path) {
                                queue.extend(files.flatten().map(|f| f.path()));
                            }
                        } else if path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("dff"))
                        {
                            let result = std::fs::read(&path)
                                .map_err(|e| e.to_string())
                                .and_then(|bytes| dff::decode(&bytes));
                            report.push(match result {
                                Ok(model) => serde_json::json!({"path":path,"vertices":model.vertices,"triangles":model.faces.len()}),
                                Err(error) => serde_json::json!({"path":path,"error":error}),
                            });
                        }
                    }
                    if let Err(error) = std::fs::write(
                        output,
                        serde_json::to_vec_pretty(&report).unwrap_or_default(),
                    ) {
                        eprintln!("{error}");
                    }
                }
                return Ok(());
            }
            _ => initial_archives.push(PathBuf::from(argument)),
        }
    }
    eframe::run_native(
        "OpenProvince",
        eframe::NativeOptions {
            depth_buffer: 24,
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 820.0])
                .with_min_inner_size([900.0, 560.0])
                .with_active(active),
            ..Default::default()
        },
        Box::new(move |creation| {
            egui_extras::install_image_loaders(&creation.egui_ctx);
            let mut fonts = egui::FontDefinitions::default();
            if let Some(ui_font) = ["C:/Windows/Fonts/SegUIVar.ttf", "C:/Windows/Fonts/segoeui.ttf"]
                .iter()
                .find_map(|p| std::fs::read(p).ok())
            {
                fonts
                    .font_data
                    .insert("ui".into(), Arc::new(egui::FontData::from_owned(ui_font)));
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "ui".into());
            }
            if let Some(mono_font) = [
                "C:/Windows/Fonts/CascadiaMono.ttf",
                "C:/Windows/Fonts/consola.ttf",
            ]
            .iter()
            .find_map(|p| std::fs::read(p).ok())
            {
                fonts
                    .font_data
                    .insert("mono".into(), Arc::new(egui::FontData::from_owned(mono_font)));
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .insert(0, "mono".into());
            }
            creation.egui_ctx.set_fonts(fonts);
            theme::apply_style(&creation.egui_ctx);
            let mut browser = Browser::new();
            browser.initial_selection = selected;
            browser.initial_variant = variant;
            let imgs: Vec<PathBuf> = initial_archives
                .iter()
                .filter(|p| !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txd")))
                .cloned()
                .collect();
            let txd = initial_archives
                .into_iter()
                .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txd")));
            if !imgs.is_empty() {
                browser.open_many(imgs, false, &creation.egui_ctx);
            } else {
                let start_folder = txd
                    .as_ref()
                    .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
                    .filter(|p| p.is_dir())
                    .unwrap_or_else(|| browser.folder.clone());
                browser.browse(start_folder, &creation.egui_ctx);
            }
            if let Some(txd_path) = txd {
                browser.open_txd_file(txd_path, &creation.egui_ctx);
            }
            Ok(Box::new(browser))
        }),
    )
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod dff;
mod gpu;
mod platform;
mod preview;
mod storage;
mod texture_editor;
mod theme;
mod txd;
mod txd_document;
mod ui;
mod vinyls;

use eframe::egui;
use province_archive::{Archive, Edit};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
};

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
    Preview(usize, Result<String, String>),
    Images(usize, Vec<txd::Texture>),
    ImagesDone(usize),
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

pub(crate) struct Browser {
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
    vinyl_capable: bool,
    vinyl_on_primary: bool,
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
    loading_model: bool,
    loading_textures: bool,
    show_details: bool,
    generation: usize,
    preview_epoch: Arc<std::sync::atomic::AtomicUsize>,
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
    last_window_title: String,
    native_chrome_applied: bool,
    sender: mpsc::Sender<Message>,
    receiver: mpsc::Receiver<Message>,
}

impl Browser {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            folder: platform::initial_folder(),
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
            vinyl_capable: false,
            vinyl_on_primary: false,
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
            loading_model: false,
            loading_textures: false,
            show_details: false,
            generation: 0,
            preview_epoch: Default::default(),
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
            last_window_title: String::new(),
            native_chrome_applied: false,
            sender,
            receiver,
        }
    }
}

impl eframe::App for Browser {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if !self.native_chrome_applied {
            #[cfg(windows)]
            platform::apply_native_titlebar_theme();
            self.native_chrome_applied = true;
        }
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
                Message::ImagesDone(index) => {
                    if self.generation == index {
                        self.loading_textures = false;
                    }
                }
                Message::Open { result, append } => {
                    self.busy = false;
                    match result {
                        Ok(new_archives) => {
                            if let Some(first) = new_archives.first()
                                && let Some(parent) = first.path().parent()
                            {
                                self.folder = parent.to_path_buf();
                                platform::remember_folder(&self.folder);
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
                                self.loading_model = false;
                                self.loading_textures = false;
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
                            Ok(report) => report,
                        };
                    }
                }
                Message::Model(index, result) => {
                    if index == self.generation {
                        self.loading_model = false;
                        match result {
                            Ok(model) => {
                                let mut remap: Vec<String> = model
                                    .materials
                                    .iter()
                                    .map(|m| m.texture.clone())
                                    .filter(|name| dff::is_remap_texture(name))
                                    .collect();
                                remap.sort();
                                remap.dedup();
                                let has_primary = model.materials.iter().any(|m| {
                                    matches!(
                                        [m.color[0], m.color[1], m.color[2]],
                                        [60, 255, 0] | [0, 255, 255]
                                    )
                                });
                                self.vinyl_capable =
                                    !remap.is_empty() || (model.is_vehicle && has_primary);
                                self.vinyl_on_primary = model.is_vehicle && remap.is_empty();
                                self.remap_targets = remap;
                                self.model = Some(dff::View::new(model));
                            }
                            Err(error) => {
                                self.loading_textures = false;
                                self.model_error = error;
                            }
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
                            if let Some(parent) = path.parent()
                                && parent.is_dir()
                            {
                                self.folder = parent.to_path_buf();
                                platform::remember_folder(&self.folder);
                            }
                            self.standalone_txd = Some(path.clone());
                            self.model = None;
                            self.loading_model = false;
                            self.loading_textures = false;
                            self.model_error.clear();
                            self.images.clear();
                            self.base_images.clear();
                            self.variants.clear();
                            self.remap_targets.clear();
                            self.vinyl_capable = false;
                            self.vinyl_on_primary = false;
                            self.vinyl_selected = None;
                            self.vinyl_texture = None;
                            self.preview_open = true;
                            self.preview = platform::format_hex_report(&name, &bytes);
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
                            platform::remember_folder(&self.folder);
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
        self.sync_window_title(ctx);
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
            _ => initial_archives.push(PathBuf::from(argument)),
        }
    }
    eframe::run_native(
        concat!("OpenProvince ", env!("CARGO_PKG_VERSION")),
        eframe::NativeOptions {
            depth_buffer: 24,
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 820.0])
                .with_min_inner_size([900.0, 560.0])
                .with_title(concat!("OpenProvince ", env!("CARGO_PKG_VERSION")))
                .with_active(active),
            ..Default::default()
        },
        Box::new(move |creation| {
            #[cfg(windows)]
            platform::apply_native_titlebar_theme();
            egui_extras::install_image_loaders(&creation.egui_ctx);
            platform::install_system_fonts(&creation.egui_ctx);
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

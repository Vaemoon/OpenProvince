use eframe::egui;
use province_archive::Profile;
use province_mods::{
    ArchiveIndex, BatchPatcher, InstalledMod, ModCategory, ModConflict, ModManifest, ModPackage,
    ModRegistry, PatchSummary, StockCache,
};
use std::path::{Path, PathBuf};

pub struct ModManagerState {
    pub registry: ModRegistry,
    pub stock_cache: StockCache,
    pub index: ArchiveIndex,
    pub _game_root: PathBuf,
    pub models_dir: PathBuf,
    pub mods_dir: PathBuf,
    pub selected_mod_id: Option<String>,
    pub search_query: String,
    pub category_filter: Option<ModCategory>,
    pub banner_message: Option<(String, bool)>,
    pub preview_texture: Option<(String, egui::TextureHandle)>,
    pub conflicts: Vec<ModConflict>,
    pub is_busy: bool,
    pub active_subtab: usize,
    pub catalog_search: String,
    pub catalog_packages: Vec<CatalogPackage>,
    pub catalog_category: Option<ModCategory>,
}

impl ModManagerState {
    pub fn new(current_folder: &Path) -> Self {
        let (game_root, models_dir, mods_dir) = resolve_paths(current_folder);
        let stock_cache_dir = game_root.join("OpenProvince").join("stock_cache");
        let state_file = mods_dir.join("mods_state.json");

        let mut index = ArchiveIndex::new();
        if models_dir.is_dir() {
            index.build_from_models_dir(&models_dir);
        }

        let mut registry = ModRegistry::new(&mods_dir, &state_file);
        let _ = registry.scan_and_load();
        let conflicts = registry.detect_conflicts();

        let stock_cache = StockCache::new(stock_cache_dir);

        let initial_selected = registry.mods().first().map(|m| m.id.clone());

        let catalog_packages = local_packages(&game_root);
        Self {
            registry,
            stock_cache,
            index,
            _game_root: game_root,
            models_dir,
            mods_dir,
            selected_mod_id: initial_selected,
            search_query: String::new(),
            category_filter: None,
            banner_message: None,
            preview_texture: None,
            conflicts,
            is_busy: false,
            active_subtab: 0,
            catalog_search: String::new(),
            catalog_packages,
            catalog_category: None,
        }
    }

    pub fn reload(&mut self) {
        self.catalog_packages = local_packages(&self._game_root);
        if self.models_dir.is_dir() {
            self.index.build_from_models_dir(&self.models_dir);
        }
        let _ = self.registry.scan_and_load();
        self.conflicts = self.registry.detect_conflicts();
        if self.selected_mod_id.is_none() {
            self.selected_mod_id = self.registry.mods().first().map(|m| m.id.clone());
        }
    }

    pub fn toggle_mod(&mut self, id: &str) {
        if let Some(m) = self.registry.find_mod(id) {
            let next = !m.enabled;
            let _ = self.registry.set_mod_enabled(id, next);
            self.conflicts = self.registry.detect_conflicts();
        }
    }

    pub fn install_package(&mut self, path: &Path) -> Result<String, String> {
        let manifest = if path.is_dir() {
            ModPackage::import_from_folder(path, &self.mods_dir, Some(&self.index))
        } else {
            ModPackage::import_from_zip(path, &self.mods_dir, Some(&self.index))
        }
        .map_err(|e| format!("Ошибка импорта: {e}"))?;
        let name = manifest.name.clone();
        let id = manifest.id.clone();
        self.reload();
        let _ = self.registry.set_mod_enabled(&id, true);
        self.selected_mod_id = Some(id);
        self.conflicts = self.registry.detect_conflicts();
        Ok(format!(
            "Модификация «{name}» успешно импортирована и включена!"
        ))
    }

    pub fn apply_active_mods(&mut self) -> Result<PatchSummary, String> {
        let profile = Profile::inspected_client();
        let summary = BatchPatcher::apply_active_mods(
            &self.models_dir,
            self.registry.mods(),
            &self.stock_cache,
            &profile,
        )
        .map_err(|e| format!("Ошибка применения модов: {e}"))?;
        Ok(summary)
    }

    pub fn restore_stock(&mut self) -> Result<PatchSummary, String> {
        let profile = Profile::inspected_client();
        let summary =
            BatchPatcher::restore_all_to_stock(&self.models_dir, &self.stock_cache, &profile)
                .map_err(|e| format!("Ошибка восстановления стоковых архивов: {e}"))?;
        Ok(summary)
    }

    pub fn export_selected_mod(&self, dest_path: &Path) -> Result<(), String> {
        let id = self
            .selected_mod_id
            .as_deref()
            .ok_or_else(|| "Мод не выбран".to_string())?;
        let mod_item = self
            .registry
            .find_mod(id)
            .ok_or_else(|| "Модификация не найдена".to_string())?;
        ModPackage::export_to_opmod(&mod_item.folder, dest_path)
            .map_err(|e| format!("Ошибка экспорта: {e}"))?;
        Ok(())
    }

    pub fn delete_selected_mod(&mut self) -> Result<(), String> {
        let id = self
            .selected_mod_id
            .take()
            .ok_or_else(|| "Мод не выбран".to_string())?;
        let mod_folder = self
            .registry
            .find_mod(&id)
            .map(|m| m.folder.clone())
            .ok_or_else(|| "Модификация не найдена".to_string())?;

        let _ = self.registry.set_mod_enabled(&id, false);
        if mod_folder.is_dir() {
            let _ = std::fs::remove_dir_all(&mod_folder);
        }
        self.reload();
        self.selected_mod_id = self.registry.mods().first().map(|m| m.id.clone());
        Ok(())
    }

    pub fn open_mods_folder(&self) {
        if !self.mods_dir.is_dir() {
            let _ = std::fs::create_dir_all(&self.mods_dir);
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("explorer")
                .arg(&self.mods_dir)
                .spawn();
        }
    }

    pub fn update_preview_texture(&mut self, ctx: &egui::Context, mod_item: &InstalledMod) {
        if let Some((tex_id, _)) = &self.preview_texture {
            if tex_id == &mod_item.id {
                return;
            }
        }
        self.preview_texture = None;
        if let Some(preview_path) = &mod_item.preview {
            if let Ok(img) = image::open(preview_path) {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                let tex = ctx.load_texture(
                    format!("mod_preview_{}", mod_item.id),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                self.preview_texture = Some((mod_item.id.clone(), tex));
            }
        }
    }
}

pub struct CatalogPackage {
    pub(super) path: PathBuf,
    pub(super) manifest: Option<ModManifest>,
    pub(super) error: Option<String>,
}

fn local_packages(game_root: &Path) -> Vec<CatalogPackage> {
    let mut folders = vec![game_root.join("OpenProvince").join("packages")];
    if let Some(parent) = game_root.parent() {
        folders.push(parent.to_path_buf());
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        folders.push(parent.to_path_buf());
    }
    let mut paths = std::collections::BTreeSet::new();
    for folder in folders {
        if let Ok(entries) = std::fs::read_dir(folder) {
            for entry in entries.flatten().take(1000) {
                let path = entry.path();
                if path.is_file()
                    && path.extension().is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("opmod") || ext.eq_ignore_ascii_case("zip")
                    })
                {
                    paths.insert(path);
                }
            }
        }
    }
    paths
        .into_iter()
        .map(|path| {
            let (manifest, error) = match ModPackage::read_manifest(&path) {
                Ok(manifest) => (manifest, None),
                Err(error) => (None, Some(error.to_string())),
            };
            CatalogPackage {
                path,
                manifest,
                error,
            }
        })
        .collect()
}

fn resolve_paths(current_folder: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let mut game_root = None;
    if current_folder.join("models").is_dir() {
        game_root = Some(current_folder.to_path_buf());
    } else if current_folder
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("models"))
    {
        if let Some(parent) = current_folder.parent() {
            game_root = Some(parent.to_path_buf());
        }
    } else if current_folder.join("game").join("models").is_dir() {
        game_root = Some(current_folder.join("game"));
    }

    if game_root.is_none() {
        for candidate in [
            r"Z:\Province Offline\game",
            r"C:\Province Games Offline\game",
            r"C:\Province Games\bin",
            r"D:\Province Games\bin",
        ] {
            let p = PathBuf::from(candidate);
            if p.join("models").is_dir() {
                game_root = Some(p);
                break;
            }
        }
    }

    let root = game_root.unwrap_or_else(|| current_folder.to_path_buf());
    let models_dir = if root.join("models").is_dir() {
        root.join("models")
    } else if root.join("bin").join("models").is_dir() {
        root.join("bin").join("models")
    } else {
        root.join("models")
    };
    let mods_dir = root.join("OpenProvince").join("mods");
    (root, models_dir, mods_dir)
}

use crate::error::Error;
use crate::manifest::ModManifest;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstalledMod {
    pub id: String,
    pub manifest: ModManifest,
    pub folder: PathBuf,
    pub enabled: bool,
    pub preview: Option<PathBuf>,
    pub files: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ModConflict {
    pub archive: String,
    pub file: String,
    pub conflicting_mod_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct RegistryState {
    enabled_mods: HashSet<String>,
    mod_order: Vec<String>,
}

pub struct ModRegistry {
    mods_dir: PathBuf,
    state_file: PathBuf,
    installed: Vec<InstalledMod>,
}

impl ModRegistry {
    pub fn new(mods_dir: impl Into<PathBuf>, state_file: impl Into<PathBuf>) -> Self {
        Self {
            mods_dir: mods_dir.into(),
            state_file: state_file.into(),
            installed: Vec::new(),
        }
    }

    pub fn mods(&self) -> &[InstalledMod] {
        &self.installed
    }

    pub fn mods_mut(&mut self) -> &mut [InstalledMod] {
        &mut self.installed
    }

    pub fn find_mod(&self, id: &str) -> Option<&InstalledMod> {
        self.installed.iter().find(|m| m.id == id)
    }

    pub fn find_mod_mut(&mut self, id: &str) -> Option<&mut InstalledMod> {
        self.installed.iter_mut().find(|m| m.id == id)
    }

    pub fn scan_and_load(&mut self) -> Result<(), Error> {
        let state = self.load_state().unwrap_or_default();

        if !self.mods_dir.is_dir() {
            fs::create_dir_all(&self.mods_dir)?;
        }

        let mut discovered = Vec::new();

        for entry in fs::read_dir(&self.mods_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("mod.json");
                if manifest_path.is_file() {
                    if let Ok(manifest) = ModManifest::load_from_file(&manifest_path) {
                        let mut preview = None;
                        for pname in ["preview.png", "preview.jpg", "preview.jpeg"] {
                            let p = path.join(pname);
                            if p.is_file() {
                                preview = Some(p);
                                break;
                            }
                        }

                        let mut files = Vec::new();
                        let files_dir = path.join("files");
                        if files_dir.is_dir() {
                            if let Ok(file_entries) = fs::read_dir(files_dir) {
                                for f in file_entries.flatten() {
                                    if f.path().is_file() {
                                        if let Some(name) =
                                            f.path().file_name().and_then(|s| s.to_str())
                                        {
                                            files.push(name.to_string());
                                        }
                                    }
                                }
                            }
                        }

                        let enabled = state.enabled_mods.contains(&manifest.id);

                        discovered.push(InstalledMod {
                            id: manifest.id.clone(),
                            manifest,
                            folder: path,
                            enabled,
                            preview,
                            files,
                        });
                    }
                }
            }
        }

        discovered.sort_by(|a, b| {
            let idx_a = state
                .mod_order
                .iter()
                .position(|id| id == &a.id)
                .unwrap_or(usize::MAX);
            let idx_b = state
                .mod_order
                .iter()
                .position(|id| id == &b.id)
                .unwrap_or(usize::MAX);
            idx_a
                .cmp(&idx_b)
                .then_with(|| a.manifest.name.cmp(&b.manifest.name))
        });

        self.installed = discovered;
        Ok(())
    }

    fn load_state(&self) -> Result<RegistryState, Error> {
        if self.state_file.is_file() {
            let data = fs::read_to_string(&self.state_file)?;
            let state: RegistryState = serde_json::from_str(&data)?;
            return Ok(state);
        }
        Ok(RegistryState::default())
    }

    pub fn save_state(&self) -> Result<(), Error> {
        let mut state = RegistryState::default();
        for m in &self.installed {
            if m.enabled {
                state.enabled_mods.insert(m.id.clone());
            }
            state.mod_order.push(m.id.clone());
        }

        if let Some(parent) = self.state_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(&state)?;
        fs::write(&self.state_file, json)?;
        Ok(())
    }

    pub fn set_mod_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(m) = self.find_mod_mut(id) {
            m.enabled = enabled;
            let _ = self.save_state();
            true
        } else {
            false
        }
    }

    pub fn remove_mod(&mut self, id: &str) -> Result<(), Error> {
        if let Some(pos) = self.installed.iter().position(|m| m.id == id) {
            let m = self.installed.remove(pos);
            if m.folder.is_dir() {
                fs::remove_dir_all(&m.folder)?;
            }
            let _ = self.save_state();
        }
        Ok(())
    }

    pub fn detect_conflicts(&self) -> Vec<ModConflict> {
        let mut usages: HashMap<(String, String), Vec<String>> = HashMap::new();

        for m in &self.installed {
            if !m.enabled {
                continue;
            }
            for rep in &m.manifest.replaces {
                let archive = rep
                    .archive
                    .as_deref()
                    .or(m.manifest.target_archive.as_deref())
                    .unwrap_or("province_vehs.img")
                    .to_ascii_lowercase();
                let file = rep.file.to_ascii_lowercase();
                usages
                    .entry((archive, file))
                    .or_default()
                    .push(m.id.clone());
            }
        }

        let mut conflicts = Vec::new();
        for ((archive, file), mod_ids) in usages {
            if mod_ids.len() > 1 {
                conflicts.push(ModConflict {
                    archive,
                    file,
                    conflicting_mod_ids: mod_ids,
                });
            }
        }

        conflicts
    }
}

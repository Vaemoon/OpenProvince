use crate::error::Error;
use crate::registry::InstalledMod;
use crate::stock_cache::StockCache;
use province_archive::{Archive, Edit, Profile};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub struct PatchSummary {
    pub archives_patched: usize,
    pub files_replaced: usize,
    pub backups_stored: usize,
    pub duration_ms: u128,
}

pub struct BatchPatcher;

impl BatchPatcher {
    pub fn apply_active_mods(
        game_models_dir: &Path,
        mods: &[InstalledMod],
        stock_cache: &StockCache,
        profile: &Profile,
    ) -> Result<PatchSummary, Error> {
        let start_time = Instant::now();
        let mut summary = PatchSummary::default();

        let mut archive_replacements: HashMap<String, Vec<(String, PathBuf)>> = HashMap::new();

        for m in mods {
            if !m.enabled {
                continue;
            }
            let files_dir = m.folder.join("files");
            for rep in &m.manifest.replaces {
                let target_archive = rep
                    .archive
                    .as_deref()
                    .or(m.manifest.target_archive.as_deref())
                    .unwrap_or("province_vehs.img");

                let source_file = files_dir.join(&rep.file);
                if source_file.is_file() {
                    let list = archive_replacements
                        .entry(target_archive.to_string())
                        .or_default();

                    if let Some(pos) = list
                        .iter()
                        .position(|(f, _)| f.eq_ignore_ascii_case(&rep.file))
                    {
                        list[pos] = (rep.file.clone(), source_file);
                    } else {
                        list.push((rep.file.clone(), source_file));
                    }
                }
            }
        }

        if archive_replacements.is_empty() {
            return Ok(summary);
        }

        for (archive_name, replacements) in archive_replacements {
            let archive_path = game_models_dir.join(&archive_name);
            if !archive_path.is_file() {
                continue;
            }

            let archive = Archive::open(&archive_path)?;

            for (file_name, _) in &replacements {
                if !stock_cache.has_stock(&archive_name, file_name) {
                    if stock_cache
                        .backup_from_archive(&archive, &archive_name, file_name, profile)
                        .is_ok()
                    {
                        summary.backups_stored += 1;
                    }
                }
            }

            let edits: Vec<Edit> = replacements
                .into_iter()
                .map(|(name, source)| {
                    summary.files_replaced += 1;
                    Edit::Replace { name, source }
                })
                .collect();

            let tmp_path = archive_path.with_extension("patch_tmp");
            archive.save_as(&tmp_path, &edits, profile)?;

            fs::rename(&tmp_path, &archive_path)?;
            summary.archives_patched += 1;
        }

        summary.duration_ms = start_time.elapsed().as_millis();
        Ok(summary)
    }

    pub fn restore_all_to_stock(
        game_models_dir: &Path,
        stock_cache: &StockCache,
        profile: &Profile,
    ) -> Result<PatchSummary, Error> {
        let start_time = Instant::now();
        let mut summary = PatchSummary::default();

        if let Ok(entries) = fs::read_dir(game_models_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("img"))
                {
                    let archive_name = match path.file_name().and_then(|s| s.to_str()) {
                        Some(s) => s.to_string(),
                        None => continue,
                    };

                    let archive_stock_dir = stock_cache.stock_path(&archive_name, "");
                    if !archive_stock_dir.is_dir() {
                        continue;
                    }

                    let mut edits = Vec::new();
                    if let Ok(stock_files) = fs::read_dir(&archive_stock_dir) {
                        for sf in stock_files.flatten() {
                            let spath = sf.path();
                            if spath.is_file() {
                                if let Some(fname) = spath.file_name().and_then(|s| s.to_str()) {
                                    edits.push(Edit::Replace {
                                        name: fname.to_string(),
                                        source: spath,
                                    });
                                    summary.files_replaced += 1;
                                }
                            }
                        }
                    }

                    if !edits.is_empty() {
                        let archive = Archive::open(&path)?;
                        let tmp_path = path.with_extension("restore_tmp");
                        archive.save_as(&tmp_path, &edits, profile)?;
                        fs::rename(&tmp_path, &path)?;
                        summary.archives_patched += 1;
                    }
                }
            }
        }

        summary.duration_ms = start_time.elapsed().as_millis();
        Ok(summary)
    }
}

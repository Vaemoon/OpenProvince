use province_archive::Archive;
use std::collections::HashMap;
use std::path::Path;

#[derive(Default, Clone, Debug)]
pub struct ArchiveIndex {
    entries_to_archive: HashMap<String, String>,
}

impl ArchiveIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build_from_models_dir(&mut self, models_dir: &Path) {
        if !models_dir.is_dir() {
            return;
        }

        let priority_archives = [
            "province_vehs.img",
            "province_gtavehs.img",
            "province_mods.img",
            "province_player.img",
            "province_misc.img",
            "gta3.img",
            "province_gta3.img",
        ];

        for archive_name in priority_archives {
            let archive_path = models_dir.join(archive_name);
            if archive_path.is_file() {
                self.index_archive(&archive_path, archive_name);
            }
        }

        if let Ok(entries) = std::fs::read_dir(models_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("img"))
                {
                    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                        if !priority_archives
                            .iter()
                            .any(|&p| p.eq_ignore_ascii_case(file_name))
                        {
                            self.index_archive(&path, file_name);
                        }
                    }
                }
            }
        }
    }

    fn index_archive(&mut self, archive_path: &Path, archive_name: &str) {
        if let Ok(archive) = Archive::open(archive_path) {
            for entry in archive.entries() {
                let lower_name = entry.name.to_ascii_lowercase();
                self.entries_to_archive
                    .entry(lower_name)
                    .or_insert_with(|| archive_name.to_string());
            }
        }
    }

    pub fn find_archive_for_file(&self, file_name: &str) -> Option<&str> {
        let lower = file_name.to_ascii_lowercase();
        self.entries_to_archive.get(&lower).map(|s| s.as_str())
    }

    pub fn guess_archive_fallback(file_name: &str) -> &'static str {
        let lower = file_name.to_ascii_lowercase();
        if lower.ends_with(".col") {
            "gta3.img"
        } else if lower.starts_with("wheel_")
            || lower.contains("spoiler")
            || lower.contains("nitro")
        {
            "province_mods.img"
        } else if lower.contains("ped")
            || lower.contains("skin")
            || lower.starts_with("c")
            || lower.starts_with("b")
        {
            "province_player.img"
        } else {
            "province_vehs.img"
        }
    }
}

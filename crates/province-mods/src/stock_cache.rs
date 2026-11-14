use crate::error::Error;
use province_archive::{Archive, Profile};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct StockCache {
    base_dir: PathBuf,
}

impl StockCache {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn archive_dir(&self, archive_name: &str) -> PathBuf {
        let stem = Path::new(archive_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(archive_name);
        self.base_dir.join(stem)
    }

    pub fn stock_path(&self, archive_name: &str, file_name: &str) -> PathBuf {
        self.archive_dir(archive_name).join(file_name)
    }

    pub fn has_stock(&self, archive_name: &str, file_name: &str) -> bool {
        self.stock_path(archive_name, file_name).is_file()
    }

    pub fn backup_from_archive(
        &self,
        archive: &Archive,
        archive_name: &str,
        file_name: &str,
        profile: &Profile,
    ) -> Result<PathBuf, Error> {
        let dest_path = self.stock_path(archive_name, file_name);
        if dest_path.is_file() {
            return Ok(dest_path);
        }

        let entry = archive
            .entries()
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(file_name))
            .ok_or_else(|| {
                Error::ArchiveNotFound(format!("File {file_name} not found in {archive_name}"))
            })?;

        let bytes = archive.read(entry.index, profile)?;

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&dest_path, bytes)?;
        Ok(dest_path)
    }
}

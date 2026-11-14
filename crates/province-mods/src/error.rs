use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Archive error: {0}")]
    Archive(#[from] province_archive::Error),

    #[error("Mod manifest not found in package: {0}")]
    ManifestNotFound(String),

    #[error("Invalid mod package: {0}")]
    InvalidPackage(String),

    #[error("Target archive not found: {0}")]
    ArchiveNotFound(String),

    #[error("Stock original file not found in cache for {0} in {1}")]
    StockNotFound(String, String),

    #[error("Mod '{0}' already installed at {1}")]
    AlreadyInstalled(String, PathBuf),
}

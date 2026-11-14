use crate::catalog_matcher::ArchiveIndex;
use crate::error::Error;
use crate::manifest::{ModCategory, ModManifest, ReplacementEntry};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

pub struct ModPackage;

impl ModPackage {
    pub fn read_manifest(path: &Path) -> Result<Option<ModManifest>, Error> {
        let mut archive = ZipArchive::new(BufReader::new(File::open(path)?))?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            if entry
                .name()
                .replace('\\', "/")
                .rsplit('/')
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case("mod.json"))
            {
                if entry.size() > 1024 * 1024 {
                    return Err(Error::InvalidPackage(
                        "Package manifest exceeds 1 MiB".into(),
                    ));
                }
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                return Ok(Some(serde_json::from_slice(&data)?));
            }
        }
        Ok(None)
    }

    pub fn import_from_zip(
        zip_path: &Path,
        mods_dir: &Path,
        index: Option<&ArchiveIndex>,
    ) -> Result<ModManifest, Error> {
        let file = File::open(zip_path)?;
        let mut archive = ZipArchive::new(BufReader::new(file))?;

        let mut manifest_opt: Option<ModManifest> = None;
        let mut preview_bytes: Option<(String, Vec<u8>)> = None;
        let mut mod_files: Vec<(String, Vec<u8>)> = Vec::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            if entry.is_dir() {
                continue;
            }

            let entry_name = entry.name().replace('\\', "/");
            let file_name = Path::new(&entry_name)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            if file_name.is_empty() || file_name.starts_with('.') {
                continue;
            }

            if file_name.eq_ignore_ascii_case("mod.json") {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                if let Ok(m) = serde_json::from_slice::<ModManifest>(&data) {
                    manifest_opt = Some(m);
                }
            } else if file_name.eq_ignore_ascii_case("preview.png")
                || file_name.eq_ignore_ascii_case("preview.jpg")
                || file_name.eq_ignore_ascii_case("preview.jpeg")
            {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                preview_bytes = Some((file_name, data));
            } else {
                let ext = Path::new(&file_name)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();

                if ["dff", "txd", "col", "ide", "dat", "cfg", "wav", "mp3"].contains(&ext.as_str())
                {
                    let mut data = Vec::new();
                    entry.read_to_end(&mut data)?;
                    mod_files.push((file_name, data));
                }
            }
        }

        if mod_files.is_empty() && manifest_opt.is_none() {
            return Err(Error::InvalidPackage(
                "Package does not contain any recognizable game model or texture files".into(),
            ));
        }

        let mut manifest = match manifest_opt {
            Some(m) => m,
            None => {
                let default_id = zip_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("custom_mod")
                    .to_lowercase()
                    .replace(' ', "_");
                let default_name = zip_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Custom Mod")
                    .replace('_', " ");

                let mut replaces = Vec::new();
                for (fname, _) in &mod_files {
                    let target_archive = index
                        .and_then(|idx| idx.find_archive_for_file(fname))
                        .unwrap_or_else(|| ArchiveIndex::guess_archive_fallback(fname))
                        .to_string();

                    replaces.push(ReplacementEntry {
                        archive: Some(target_archive),
                        file: fname.clone(),
                    });
                }

                let category = if replaces
                    .iter()
                    .any(|r| r.archive.as_deref().unwrap_or_default().contains("player"))
                {
                    ModCategory::Skins
                } else {
                    ModCategory::Vehicles
                };

                ModManifest {
                    format_version: Some(1),
                    id: default_id,
                    name: default_name,
                    version: "1.0".into(),
                    author: "Unknown".into(),
                    category,
                    description: "Автоматически импортированный мод".into(),
                    target_archive: replaces.first().and_then(|r| r.archive.clone()),
                    replaces,
                }
            }
        };

        if let Some(idx) = index {
            for rep in &mut manifest.replaces {
                if rep.archive.is_none() {
                    rep.archive = idx.find_archive_for_file(&rep.file).map(|s| s.to_string());
                }
            }
        }

        let target_dir = mods_dir.join(&manifest.id);
        fs::create_dir_all(target_dir.join("files"))?;

        for (fname, data) in mod_files {
            fs::write(target_dir.join("files").join(&fname), data)?;
        }

        if let Some((pname, pdata)) = preview_bytes {
            fs::write(target_dir.join(pname), pdata)?;
        }

        manifest.save_to_file(&target_dir.join("mod.json"))?;

        Ok(manifest)
    }

    pub fn import_from_folder(
        source_dir: &Path,
        mods_dir: &Path,
        index: Option<&ArchiveIndex>,
    ) -> Result<ModManifest, Error> {
        let mut manifest_opt: Option<ModManifest> = None;
        let mut preview_bytes: Option<(String, Vec<u8>)> = None;
        let mut mod_files: Vec<(String, Vec<u8>)> = Vec::new();

        fn collect_files(
            dir: &Path,
            manifest_opt: &mut Option<ModManifest>,
            preview_bytes: &mut Option<(String, Vec<u8>)>,
            mod_files: &mut Vec<(String, Vec<u8>)>,
        ) -> Result<(), Error> {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    collect_files(&path, manifest_opt, preview_bytes, mod_files)?;
                    continue;
                }
                let file_name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                if file_name.is_empty() || file_name.starts_with('.') {
                    continue;
                }
                if file_name.eq_ignore_ascii_case("mod.json") {
                    if let Ok(data) = fs::read(&path) {
                        if let Ok(m) = serde_json::from_slice::<ModManifest>(&data) {
                            *manifest_opt = Some(m);
                        }
                    }
                } else if file_name.eq_ignore_ascii_case("preview.png")
                    || file_name.eq_ignore_ascii_case("preview.jpg")
                    || file_name.eq_ignore_ascii_case("preview.jpeg")
                {
                    if let Ok(data) = fs::read(&path) {
                        *preview_bytes = Some((file_name, data));
                    }
                } else {
                    let ext = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    if [
                        "dff", "txd", "col", "ide", "dat", "cfg", "wav", "mp3", "fxp", "ifp",
                    ]
                    .contains(&ext.as_str())
                    {
                        if let Ok(data) = fs::read(&path) {
                            mod_files.push((file_name, data));
                        }
                    }
                }
            }
            Ok(())
        }

        collect_files(
            source_dir,
            &mut manifest_opt,
            &mut preview_bytes,
            &mut mod_files,
        )?;

        if mod_files.is_empty() && manifest_opt.is_none() {
            return Err(Error::InvalidPackage(
                "Папка не содержит распознаваемых файлов моделей или текстур GTA".into(),
            ));
        }

        let mut manifest = match manifest_opt {
            Some(m) => m,
            None => {
                let default_id = source_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("custom_mod")
                    .to_lowercase()
                    .replace(' ', "_");
                let default_name = source_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Custom Mod")
                    .replace('_', " ");

                let mut replaces = Vec::new();
                for (fname, _) in &mod_files {
                    let target_archive = index
                        .and_then(|idx| idx.find_archive_for_file(fname))
                        .unwrap_or_else(|| ArchiveIndex::guess_archive_fallback(fname))
                        .to_string();

                    replaces.push(ReplacementEntry {
                        archive: Some(target_archive),
                        file: fname.clone(),
                    });
                }

                let category = if replaces
                    .iter()
                    .any(|r| r.archive.as_deref().unwrap_or_default().contains("player"))
                {
                    ModCategory::Skins
                } else {
                    ModCategory::Vehicles
                };

                ModManifest {
                    format_version: Some(1),
                    id: default_id,
                    name: default_name,
                    version: "1.0".into(),
                    author: "Unknown".into(),
                    category,
                    description: "Автоматически импортированный мод".into(),
                    target_archive: replaces.first().and_then(|r| r.archive.clone()),
                    replaces,
                }
            }
        };

        if let Some(idx) = index {
            for rep in &mut manifest.replaces {
                if rep.archive.is_none() {
                    rep.archive = idx.find_archive_for_file(&rep.file).map(|s| s.to_string());
                }
            }
        }

        let target_dir = mods_dir.join(&manifest.id);
        fs::create_dir_all(target_dir.join("files"))?;

        for (fname, data) in mod_files {
            fs::write(target_dir.join("files").join(&fname), data)?;
        }

        if let Some((pname, pdata)) = preview_bytes {
            fs::write(target_dir.join(pname), pdata)?;
        }

        manifest.save_to_file(&target_dir.join("mod.json"))?;

        Ok(manifest)
    }

    pub fn export_to_opmod(mod_dir: &Path, out_path: &Path) -> Result<(), Error> {
        let manifest_path = mod_dir.join("mod.json");
        if !manifest_path.is_file() {
            return Err(Error::ManifestNotFound(
                "mod.json not found in mod folder".into(),
            ));
        }

        let file = File::create(out_path)?;
        let mut zip = ZipWriter::new(BufWriter::new(file));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("mod.json", options)?;
        let manifest_data = fs::read(&manifest_path)?;
        zip.write_all(&manifest_data)?;

        for p_name in ["preview.png", "preview.jpg", "preview.jpeg"] {
            let p_path = mod_dir.join(p_name);
            if p_path.is_file() {
                zip.start_file(p_name, options)?;
                let data = fs::read(&p_path)?;
                zip.write_all(&data)?;
                break;
            }
        }

        let files_dir = mod_dir.join("files");
        if files_dir.is_dir() {
            for entry in fs::read_dir(files_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default();
                    zip.start_file(format!("files/{name}"), options)?;
                    let data = fs::read(&path)?;
                    zip.write_all(&data)?;
                }
            }
        }

        zip.finish()?;
        Ok(())
    }
}

//! Поиск и декодирование винилов из ресурсов MTA Province / OpenProvince.
use crate::txd;
use province_archive::Profile;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub enum VinylSource {
    Txd(PathBuf),
    CutTiles(Box<[PathBuf; 8]>),
    Image(PathBuf),
}

#[derive(Clone)]
pub struct VinylEntry {
    pub name: String,
    pub source: VinylSource,
}

fn push_unique_dir(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if dir.is_dir() && !dirs.iter().any(|d| d == &dir) {
        dirs.push(dir);
    }
}

fn collect_candidate_roots(anchors: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let exe = std::env::current_exe().unwrap_or_default();
    for start in anchors.iter().chain(std::iter::once(&exe)) {
        for anc in start.ancestors().take(6) {
            push_unique_dir(&mut roots, anc.to_path_buf());
        }
    }
    for base in [
        "C:/Province Games",
        "D:/Province Games",
        "Z:/Province Games",
        "Z:/Province Offline/game",
        "C:/Program Files (x86)/Province Games",
        "C:/Program Files/Province Games",
    ] {
        push_unique_dir(&mut roots, PathBuf::from(base));
    }
    roots
}

fn load_openprovince_vinyls(root: &Path, out: &mut BTreeMap<String, VinylEntry>) {
    let op = root.join("OpenProvince");
    let json_path = op.join("vinyls.json");
    if let Ok(bytes) = fs::read(&json_path)
        && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let Some(items) = value.as_array()
    {
        for item in items {
            let Some(name) = item.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(tex) = item.get("texture").and_then(|v| v.as_str()) else {
                continue;
            };
            let txd_path = op.join(tex);
            if txd_path.is_file() {
                let key = name.to_ascii_lowercase();
                out.entry(key).or_insert_with(|| VinylEntry {
                    name: name.to_owned(),
                    source: VinylSource::Txd(txd_path),
                });
            }
        }
    }
    let vinyls_dir = op.join("vinyls");
    if let Ok(entries) = fs::read_dir(vinyls_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("txd"))
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned)
            {
                let key = stem.to_ascii_lowercase();
                out.entry(key).or_insert_with(|| VinylEntry {
                    name: stem,
                    source: VinylSource::Txd(path),
                });
            }
        }
    }
}

fn load_cut_vinyls(vinyl_dir: &Path, out: &mut BTreeMap<String, VinylEntry>) {
    let cut_dir = if vinyl_dir.join("cut").is_dir() {
        vinyl_dir.join("cut")
    } else {
        vinyl_dir.to_path_buf()
    };
    let Ok(files) = fs::read_dir(&cut_dir) else {
        return;
    };
    let mut groups: BTreeMap<String, [Option<PathBuf>; 8]> = BTreeMap::new();
    for file in files.flatten() {
        let path = file.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("dds"))
        {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        let Some((name, num_str)) = stem.rsplit_once('_') else {
            continue;
        };
        let Ok(num) = num_str.parse::<usize>() else {
            continue;
        };
        if !(1..=8).contains(&num) {
            continue;
        }
        groups.entry(name.to_owned()).or_default()[num - 1] = Some(path);
    }
    for (name, parts) in groups {
        if let [
            Some(p0),
            Some(p1),
            Some(p2),
            Some(p3),
            Some(p4),
            Some(p5),
            Some(p6),
            Some(p7),
        ] = parts
        {
            let key = name.to_ascii_lowercase();
            out.entry(key).or_insert_with(|| VinylEntry {
                name,
                source: VinylSource::CutTiles(Box::new([p0, p1, p2, p3, p4, p5, p6, p7])),
            });
        }
    }
}

fn load_loose_vinyls(dir: &Path, out: &mut BTreeMap<String, VinylEntry>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        let key = stem.to_ascii_lowercase();
        if ext.eq_ignore_ascii_case("txd") {
            out.entry(key).or_insert_with(|| VinylEntry {
                name: stem,
                source: VinylSource::Txd(path),
            });
        } else if ["dds", "png", "tga", "bmp"]
            .iter()
            .any(|e| ext.eq_ignore_ascii_case(e))
        {
            out.entry(key).or_insert_with(|| VinylEntry {
                name: stem,
                source: VinylSource::Image(path),
            });
        }
    }
}

pub fn discover(anchors: &[PathBuf], custom_dir: Option<&Path>) -> Vec<VinylEntry> {
    let mut found = BTreeMap::new();
    if let Some(dir) = custom_dir {
        load_cut_vinyls(dir, &mut found);
        load_loose_vinyls(dir, &mut found);
        if dir.join("lod").is_dir() {
            load_loose_vinyls(&dir.join("lod"), &mut found);
        }
    }
    let roots = collect_candidate_roots(anchors);
    for root in &roots {
        load_openprovince_vinyls(root, &mut found);
    }
    for root in &roots {
        for rel in [
            "mods/deathmatch/resources/province_tuningAssets/assets/vinyl",
            "mods/deathmatch/resources/province_components/vinyl",
            "mods/deathmatch/resources/province_components/vinyls",
        ] {
            let dir = root.join(rel);
            if dir.is_dir() {
                load_cut_vinyls(&dir, &mut found);
                if dir.join("lod").is_dir() {
                    load_loose_vinyls(&dir.join("lod"), &mut found);
                } else {
                    load_loose_vinyls(&dir, &mut found);
                }
            }
        }
    }
    let mut list: Vec<_> = found.into_values().collect();
    list.sort_by_key(|a| a.name.to_lowercase());
    list
}

pub fn load_texture(entry: &VinylEntry) -> Result<txd::Texture, String> {
    match &entry.source {
        VinylSource::Txd(path) => {
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            let decoded = Profile::inspected_client().decode_file(path, &bytes);
            txd::previews(&decoded)
                .into_iter()
                .next()
                .ok_or_else(|| format!("Не удалось прочитать текстуру винила: {}", path.display()))
        }
        VinylSource::CutTiles(parts) => {
            let mut atlas = None;
            let mut tile_size = (0u32, 0u32);
            for (i, path) in parts.iter().enumerate() {
                let bytes = fs::read(path).map_err(|e| e.to_string())?;
                let tile =
                    image::load_from_memory_with_format(&bytes, image::ImageFormat::Dds)
                        .map_err(|e| e.to_string())?
                        .into_rgba8();
                if i == 0 {
                    tile_size = tile.dimensions();
                    if tile_size.0 == 0 || tile_size.1 == 0 || tile_size.0 > 1024 || tile_size.1 > 2048
                    {
                        return Err("Некорректный размер тайла винила".into());
                    }
                    atlas = Some(image::RgbaImage::new(tile_size.0 * 4, tile_size.1 * 2));
                }
                if tile.dimensions() != tile_size {
                    return Err("Несовпадающие размеры тайлов винила".into());
                }
                if let Some(dest) = atlas.as_mut() {
                    image::imageops::replace(
                        dest,
                        &tile,
                        (i % 4) as i64 * i64::from(tile_size.0),
                        (i / 4) as i64 * i64::from(tile_size.1),
                    );
                }
            }
            let atlas = atlas.ok_or("Пустой винил")?;
            Ok(txd::Texture {
                name: entry.name.clone(),
                width: atlas.width() as usize,
                height: atlas.height() as usize,
                rgba: atlas.into_raw(),
            })
        }
        VinylSource::Image(path) => {
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            let img = image::load_from_memory(&bytes)
                .map_err(|e| e.to_string())?
                .into_rgba8();
            Ok(txd::Texture {
                name: entry.name.clone(),
                width: img.width() as usize,
                height: img.height() as usize,
                rgba: img.into_raw(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_cut_groups_and_openprovince_vinyls() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("openprovince-vinyl-test-{stamp}"));
        let cut = root.join("custom").join("cut");
        fs::create_dir_all(&cut).unwrap();
        for i in 1..=8 {
            fs::write(cut.join(format!("race_stripe_{i}.dds")), b"dds").unwrap();
        }
        let op = root.join("OpenProvince");
        fs::create_dir_all(op.join("vinyls")).unwrap();
        fs::write(op.join("vinyls/v0001.txd"), b"txd").unwrap();
        fs::write(
            op.join("vinyls.json"),
            r#"[{"id":1,"name":"10years","texture":"vinyls/v0001.txd"}]"#,
        )
        .unwrap();

        let list = discover(std::slice::from_ref(&root), Some(&root.join("custom")));
        assert!(list.iter().any(|v| v.name == "race_stripe"));
        assert!(list.iter().any(|v| v.name == "10years"));
        fs::remove_dir_all(root).unwrap();
    }
}

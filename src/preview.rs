//! Worker-side model/texture resolution across adjacent Province archives and IDE/JSON catalogs.
use crate::{dff, txd};
use province_archive::{Archive, Profile};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Debug)]
pub struct PlayerCloth {
    pub txd: String,
    pub model: String,
    pub slot: u8,
}

#[derive(Default)]
pub struct Catalog {
    pub models: BTreeMap<String, String>,
    pub hashed_models: BTreeMap<u32, String>,
    pub parents: Vec<(String, String)>,
    pub player_clothes: Vec<PlayerCloth>,
}

/// Стандартный хеш имён GTA SA (`CKeyGen::GetUppercaseKey` — JAMCRC по ASCII в верхнем регистре).
fn gta_uppercase_hash(text: &str) -> u32 {
    let mut hash = 0xFFFF_FFFFu32;
    for byte in text.bytes().map(|b| b.to_ascii_uppercase()) {
        hash ^= u32::from(byte);
        for _ in 0..8 {
            hash = if hash & 1 != 0 {
                (hash >> 1) ^ 0xEDB8_8320
            } else {
                hash >> 1
            };
        }
    }
    hash
}

/// Читает определения моделей (`types 1,2,3,5`) и родительских словарей (`type 6`) из `data/map.bin`.
fn parse_map_bin_into(bytes: &[u8], catalog: &mut Catalog) {
    let u32_at = |offset: usize| -> Option<u32> {
        let slice: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
        Some(u32::from_le_bytes(slice))
    };
    if bytes.len() < 8 || u32_at(0) != Some(0x2A09_3EF5) {
        return;
    }
    let count = u32_at(4).unwrap_or(0) as usize;
    let read_str = |offset: usize| -> Option<(String, usize)> {
        let len = u32_at(offset)? as usize;
        if len > 256 {
            return None;
        }
        let s = bytes.get(offset + 4..offset + 4 + len)?;
        Some((
            String::from_utf8_lossy(s).trim().to_ascii_lowercase(),
            offset + 4 + len,
        ))
    };

    let mut pos = 8usize;
    for _ in 0..count {
        let Some(&marker) = bytes.get(pos) else { break };
        if marker == 0xFF {
            pos += 1;
        }
        let Some(&kind) = bytes.get(pos) else { break };
        match kind {
            1 | 2 => {
                let Some(name_hash) = u32_at(pos + 9) else { break };
                let Some((txd, next)) = read_str(pos + 37) else { break };
                if !txd.is_empty() && txd != "null" {
                    catalog.hashed_models.insert(name_hash, txd);
                }
                pos = next;
            }
            3 | 5 => {
                let Some(name_hash) = u32_at(pos + 9) else { break };
                let Some((txd, after_txd)) = read_str(pos + 41) else { break };
                let Some((model_name, next)) = read_str(after_txd) else { break };
                if !txd.is_empty() && txd != "null" {
                    catalog.hashed_models.insert(name_hash, txd.clone());
                    if !model_name.is_empty() {
                        catalog.models.insert(model_name, txd);
                    }
                }
                pos = next;
            }
            6 => {
                let Some((child, after_child)) = read_str(pos + 1) else { break };
                let Some((parent, next)) = read_str(after_child) else { break };
                if !child.is_empty() && !parent.is_empty() {
                    catalog.parents.push((child, parent));
                }
                pos = next;
            }
            _ => break,
        }
    }
}

fn find_game_roots(start: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let exe = std::env::current_exe().unwrap_or_default();
    for candidate in start
        .ancestors()
        .skip(1)
        .take(5)
        .chain(exe.ancestors().skip(1).take(7))
    {
        if (candidate.join("OpenProvince").is_dir()
            || candidate.join("catalog").is_dir()
            || candidate.join("data").is_dir())
            && !roots.iter().any(|r| r == candidate)
        {
            roots.push(candidate.to_path_buf());
        }
    }
    roots
}

fn collect_ide_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ide_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ide"))
        {
            out.push(path);
        }
    }
}

fn parse_ide_into(text: &str, catalog: &mut Catalog) {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Section {
        None,
        Objs,
        Cars,
        Peds,
        Txdp,
        Other,
    }
    let mut section = Section::None;
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower == "end" {
            section = Section::None;
            continue;
        }
        if !line.contains(',') && !line.contains(' ') && !line.contains('\t') {
            section = match lower.as_str() {
                "objs" | "tobj" | "anim" => Section::Objs,
                "cars" => Section::Cars,
                "peds" => Section::Peds,
                "txdp" => Section::Txdp,
                _ => Section::Other,
            };
            continue;
        }
        let fields: Vec<&str> = line
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        match section {
            Section::Txdp if fields.len() >= 2 => {
                catalog.parents.push((
                    fields[0].to_ascii_lowercase(),
                    fields[1].to_ascii_lowercase(),
                ));
            }
            Section::Objs | Section::Cars | Section::Peds if fields.len() >= 3 => {
                let model = fields[1].to_ascii_lowercase();
                let txd = fields[2].to_ascii_lowercase();
                if !model.is_empty() && !txd.is_empty() && txd != "null" {
                    catalog.models.insert(model, txd);
                }
            }
            _ => {}
        }
    }
}

fn parse_shopping_into(text: &str, catalog: &mut Catalog) {
    let mut active = false;
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(sec) = lower.strip_prefix("section ") {
            let sec = sec.trim();
            active = matches!(sec, "clothes" | "haircuts");
            continue;
        }
        if lower == "end" {
            active = false;
            continue;
        }
        if !active {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 4
            && let Ok(slot) = cols[3].parse::<u8>()
        {
            let txd = cols[0].to_ascii_lowercase();
            let model = cols[2].to_ascii_lowercase();
            if model != "-" && txd != "-" {
                catalog
                    .player_clothes
                    .push(PlayerCloth { txd, model, slot });
            }
        }
    }
}

fn load_json_metadata(path: &Path, catalog: &mut Catalog) {
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return;
    };
    let entries = value
        .as_array()
        .or_else(|| value.get("models").and_then(|v| v.as_array()));
    for entry in entries.into_iter().flatten() {
        let Some(name) = entry.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if let Some(texture) = entry
            .get("texture")
            .or_else(|| entry.get("txd"))
            .and_then(|v| v.as_str())
        {
            catalog.models.insert(lower, texture.to_ascii_lowercase());
        }
    }
    if let Some(values) = value.get("texture_parents").and_then(|v| v.as_array()) {
        for parent in values {
            if let (Some(child), Some(parent)) =
                (parent["child"].as_str(), parent["parent"].as_str())
            {
                catalog
                    .parents
                    .push((child.to_ascii_lowercase(), parent.to_ascii_lowercase()));
            }
        }
    }
}

type CatalogCache = Mutex<Vec<(Vec<PathBuf>, Arc<Catalog>)>>;

pub fn catalog_for_path(start: &Path) -> Arc<Catalog> {
    static CACHE: OnceLock<CatalogCache> = OnceLock::new();
    let roots = find_game_roots(start);
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(entries) = cache.lock()
        && let Some((_, existing)) = entries.iter().find(|(r, _)| *r == roots)
    {
        return existing.clone();
    }
    let profile = Profile::inspected_client();
    let mut catalog = Catalog::default();
    let mut ide_files = Vec::new();
    for root in &roots {
        for sub in ["data", "unpacked/data"] {
            collect_ide_files(&root.join(sub), &mut ide_files);
        }
    }
    ide_files.sort();
    for path in ide_files {
        if let Ok(bytes) = std::fs::read(&path) {
            let bytes = profile.decode_file(&path, &bytes);
            let bytes = province_archive::decode_text_records(&bytes).unwrap_or(bytes);
            parse_ide_into(&String::from_utf8_lossy(&bytes), &mut catalog);
        }
    }
    for root in &roots {
        for rel in ["data/map.bin", "unpacked/data/map.bin", "decoded_data/map.bin.bin"] {
            let path = root.join(rel);
            if let Ok(bytes) = std::fs::read(&path) {
                let decoded = profile.decode_file(&path, &bytes);
                parse_map_bin_into(&decoded, &mut catalog);
            }
        }
        for rel in ["data/shopping.dat", "unpacked/data/shopping.dat"] {
            let path = root.join(rel);
            if let Ok(bytes) = std::fs::read(&path) {
                let bytes = profile.decode_file(&path, &bytes);
                let bytes = province_archive::decode_text_records(&bytes).unwrap_or(bytes);
                parse_shopping_into(&String::from_utf8_lossy(&bytes), &mut catalog);
            }
        }
        for file in [
            "catalog/vehicles.json",
            "catalog/skins.json",
            "catalog/province_map.json",
            "map/province_map.json",
            "OpenProvince/vehicles.json",
            "OpenProvince/skins.json",
        ] {
            load_json_metadata(&root.join(file), &mut catalog);
        }
    }
    let catalog = Arc::new(catalog);
    if let Ok(mut entries) = cache.lock() {
        if entries.len() >= 8 {
            entries.remove(0);
        }
        entries.push((roots, catalog.clone()));
    }
    catalog
}

/// An alternative texture dictionary stored beside the model's base dictionary.
#[derive(Clone)]
pub struct Variant {
    pub name: String,
    pub archive: PathBuf,
    pub index: usize,
}

fn variant_number(model: &str, dictionary: &str) -> Option<u32> {
    let suffix = dictionary.strip_prefix(model)?;
    if suffix.is_empty() || !suffix.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

#[cfg(test)]
fn parse_wheel_sizes(text: &str, model: &str) -> Option<[f32; 2]> {
    for line in text.lines() {
        let fields: Vec<_> = line.split('#').next()?.split(',').map(str::trim).collect();
        if fields.len() < 14 || !fields[1].eq_ignore_ascii_case(model) || fields[3] != "car" {
            continue;
        }
        let sizes = [
            fields[12].parse::<f32>().ok()?,
            fields[13].parse::<f32>().ok()?,
        ];
        if sizes
            .iter()
            .all(|size| size.is_finite() && *size > 0.01 && *size < 20.0)
        {
            return Some(sizes);
        }
    }
    None
}

fn gather_archives(archive: &Archive, opened: &[Arc<Archive>]) -> Vec<Arc<Archive>> {
    let mut seen = BTreeSet::new();
    let mut list = Vec::new();
    let mut push_unique = |path: PathBuf, existing: Option<Arc<Archive>>| {
        let key = path.to_string_lossy().to_ascii_lowercase();
        if !seen.insert(key) {
            return;
        }
        if let Some(a) = existing {
            list.push(a);
        } else if let Ok(a) = Archive::open(&path) {
            list.push(Arc::new(a));
        }
    };
    for a in opened {
        push_unique(a.path().to_owned(), Some(a.clone()));
    }
    push_unique(archive.path().to_owned(), None);
    let directory = archive.path().parent().unwrap_or_else(|| Path::new("."));
    if let Ok(files) = std::fs::read_dir(directory) {
        for file in files.flatten().take(128) {
            let path = file.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("img"))
            {
                push_unique(path, None);
            }
        }
    }
    list
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn variants(archive: &Archive, index: usize) -> Vec<Variant> {
    variants_with_archives(archive, index, &[])
}

pub fn is_player_archive(archive: &Archive) -> bool {
    archive
        .path()
        .file_name()
        .is_some_and(|n| n.to_string_lossy().to_ascii_lowercase().contains("player"))
}

pub fn variants_with_archives(
    archive: &Archive,
    index: usize,
    opened: &[Arc<Archive>],
) -> Vec<Variant> {
    let Some(entry) = archive.entries().get(index) else {
        return Vec::new();
    };
    let lower = entry.name.to_ascii_lowercase();
    let Some(model) = lower.strip_suffix(".dff") else {
        return Vec::new();
    };
    if is_player_archive(archive) {
        let catalog = catalog_for_path(archive.path());
        let mut wanted_stems = BTreeSet::<String>::new();
        for cloth in &catalog.player_clothes {
            if cloth.model == model {
                wanted_stems.insert(cloth.txd.clone());
            }
        }
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for e in archive.entries() {
            let name = e.name.to_ascii_lowercase();
            let Some(stem) = name.strip_suffix(".txd") else {
                continue;
            };
            let matches = stem == model
                || wanted_stems.contains(stem)
                || (model.len() >= 3 && stem.starts_with(model));
            if matches && seen.insert(name) {
                result.push(Variant {
                    name: e.name.clone(),
                    archive: archive.path().to_owned(),
                    index: e.index,
                });
            }
        }
        result.sort_by(|a, b| {
            let a_exact = a.name.to_ascii_lowercase().trim_end_matches(".txd") == model;
            let b_exact = b.name.to_ascii_lowercase().trim_end_matches(".txd") == model;
            b_exact.cmp(&a_exact).then_with(|| a.name.cmp(&b.name))
        });
        return result;
    }
    let mut result = Vec::new();
    for candidate in gather_archives(archive, opened) {
        for entry in candidate.entries() {
            let name = entry.name.to_ascii_lowercase();
            if name
                .strip_suffix(".txd")
                .and_then(|n| variant_number(model, n))
                .is_some()
                && !result
                    .iter()
                    .any(|v: &Variant| v.name.eq_ignore_ascii_case(&entry.name))
            {
                result.push(Variant {
                    name: entry.name.clone(),
                    archive: candidate.path().to_owned(),
                    index: entry.index,
                });
            }
        }
    }
    result.sort_by_key(|v| {
        variant_number(model, v.name.to_ascii_lowercase().trim_end_matches(".txd"))
    });
    result
}

pub fn load_variant(variant: &Variant, targets: &[String]) -> Result<Vec<txd::Texture>, String> {
    let archive = Archive::open(&variant.archive).map_err(|e| e.to_string())?;
    let bytes = archive
        .read(variant.index, &Profile::inspected_client())
        .map_err(|e| e.to_string())?;
    let mut textures = txd::previews(&bytes);
    // GTA paintjobs bind the dictionary's first texture to remap/# materials.
    // Retain named entries as well: custom dictionaries may override several materials.
    if let Some(first) = textures.first() {
        let aliases: Vec<_> = targets
            .iter()
            .filter(|name| !textures.iter().any(|t| t.name.eq_ignore_ascii_case(name)))
            .map(|name| txd::Texture {
                name: name.clone(),
                width: first.width,
                height: first.height,
                rgba: first.rgba.clone(),
            })
            .collect();
        textures.extend(aliases);
    }
    if textures.is_empty() {
        return Err("TXD не содержит поддерживаемых текстур".into());
    }
    Ok(textures)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load(archive: &Archive, index: usize) -> Result<(dff::Model, Vec<txd::Texture>), String> {
    load_with_archives(archive, index, &[])
}

pub fn load_with_archives(
    archive: &Archive,
    index: usize,
    opened: &[Arc<Archive>],
) -> Result<(dff::Model, Vec<txd::Texture>), String> {
    load_model_with_variant(archive, index, opened, true, None)
}

fn player_part_slot(catalog: &Catalog, stem: &str) -> u8 {
    if stem == "hands" || stem == "cs_hands" {
        return 99;
    }
    if let Some(found) = catalog.player_clothes.iter().find(|c| c.model == stem) {
        return found.slot;
    }
    if matches!(
        stem,
        "head" | "afro" | "afrobeard" | "afrotash" | "cornrows" | "balaclava"
    ) || stem.contains("hair")
    {
        return 1;
    }
    if stem == "feet" || stem.contains("shoe") || stem.contains("boot") || stem.contains("sneak") {
        return 3;
    }
    if stem == "legs" || stem.contains("jean") || stem.contains("chino") || stem.contains("short") {
        return 2;
    }
    0
}

fn resolve_player_part(
    archive: &Archive,
    catalog: &Catalog,
    profile: &Profile,
    part_stem: &str,
    requested_txd: Option<&str>,
    scope_prefix: &str,
) -> Result<(dff::Model, Vec<txd::Texture>), String> {
    let dff_idx = archive
        .entries()
        .iter()
        .find(|e| {
            e.name
                .to_ascii_lowercase()
                .strip_suffix(".dff")
                .is_some_and(|s| s == part_stem)
        })
        .map(|e| e.index)
        .ok_or_else(|| format!("Не найдена деталь {part_stem}.dff"))?;
    let bytes = archive.read(dff_idx, profile).map_err(|e| e.to_string())?;
    let mut model = dff::decode(&bytes)?;

    let mut candidates = Vec::<String>::new();
    if let Some(req) = requested_txd {
        candidates.push(
            req.trim_end_matches(".txd")
                .trim_end_matches(".TXD")
                .to_ascii_lowercase(),
        );
    }
    let stem_lookup = part_stem.strip_prefix("cs_").unwrap_or(part_stem);
    match stem_lookup {
        "torso" => candidates.extend(["player_torso".into(), "torso".into()]),
        "head" => candidates.extend(["player_face".into(), "head".into()]),
        "hands" => candidates.extend(["player_torso".into(), "torso".into(), "hands".into()]),
        "feet" | "barefeet" => candidates.extend(["foot".into(), "feet".into()]),
        "legs" => candidates.extend(["legs".into()]),
        _ => {}
    }
    candidates.push(part_stem.to_owned());
    if stem_lookup != part_stem {
        candidates.push(stem_lookup.to_owned());
    }
    for cloth in &catalog.player_clothes {
        if cloth.model == part_stem || cloth.model == stem_lookup {
            candidates.push(cloth.txd.clone());
        }
    }
    for e in archive.entries() {
        let lower = e.name.to_ascii_lowercase();
        if let Some(stem) = lower.strip_suffix(".txd")
            && stem_lookup.len() >= 3
            && (stem.starts_with(part_stem) || stem.starts_with(stem_lookup))
        {
            candidates.push(stem.to_owned());
        }
    }

    let mut raw_textures = Vec::new();
    for txd_stem in candidates {
        if let Some(entry) = archive.entries().iter().find(|e| {
            e.name
                .to_ascii_lowercase()
                .strip_suffix(".txd")
                .is_some_and(|s| s == txd_stem)
        }) && let Ok(tbytes) = archive.read(entry.index, profile)
        {
            let decoded = txd::previews(&tbytes);
            if !decoded.is_empty() {
                raw_textures = decoded;
                break;
            }
        }
    }

    if raw_textures.is_empty() {
        return Ok((model, Vec::new()));
    }

    let mut scoped_textures = Vec::with_capacity(raw_textures.len());
    for tex in &raw_textures {
        scoped_textures.push(txd::Texture {
            name: format!("{scope_prefix}::{}", tex.name),
            width: tex.width,
            height: tex.height,
            rgba: tex.rgba.clone(),
        });
    }
    let fallback_name = scoped_textures[0].name.clone();
    for mat in &mut model.materials {
        if mat.texture.is_empty() {
            mat.texture.clone_from(&fallback_name);
        } else if let Some(matched) = raw_textures.iter().find(|t| {
            t.name.eq_ignore_ascii_case(&mat.texture) || txd::name_matches(&mat.texture, &t.name)
        }) {
            mat.texture = format!("{scope_prefix}::{}", matched.name);
        } else {
            mat.texture.clone_from(&fallback_name);
        }
    }
    Ok((model, scoped_textures))
}

pub fn load_model_with_variant(
    archive: &Archive,
    index: usize,
    opened: &[Arc<Archive>],
    assemble_player_skin: bool,
    selected_txd: Option<&str>,
) -> Result<(dff::Model, Vec<txd::Texture>), String> {
    let profile = Profile::inspected_client();
    let name = archive
        .entries()
        .get(index)
        .ok_or("Entry disappeared")?
        .name
        .trim_end_matches(".dff")
        .trim_end_matches(".DFF")
        .to_ascii_lowercase();
    let catalog = catalog_for_path(archive.path());

    if is_player_archive(archive) {
        if !assemble_player_skin || name.starts_with("cs_") {
            return resolve_player_part(archive, &catalog, &profile, &name, selected_txd, "sel");
        }
        let selected_slot = player_part_slot(&catalog, &name);
        let mut parts = Vec::<dff::Model>::new();
        let mut all_textures = Vec::<txd::Texture>::new();
        let base_slots: [(u8, &str); 5] = [
            (1, "head"),
            (0, "torso"),
            (99, "hands"),
            (2, "legs"),
            (3, "feet"),
        ];
        let mut placed_selected = false;
        for (idx, (slot, default_stem)) in base_slots.iter().enumerate() {
            let is_selected_slot =
                *slot == selected_slot || (selected_slot == 17 && matches!(*slot, 0 | 2));
            if is_selected_slot && selected_slot == 17 && *slot != 0 {
                // Special outfit covers torso+legs via the single outfit model
                continue;
            }
            let (stem, req_txd) = if is_selected_slot {
                placed_selected = true;
                (name.as_str(), selected_txd)
            } else {
                (*default_stem, None)
            };
            if let Ok((model, textures)) = resolve_player_part(
                archive,
                &catalog,
                &profile,
                stem,
                req_txd,
                &format!("p{idx}_{stem}"),
            ) {
                parts.push(model);
                all_textures.extend(textures);
            }
        }
        if !placed_selected
            && let Ok((model, textures)) =
                resolve_player_part(archive, &catalog, &profile, &name, selected_txd, "acc")
        {
            parts.push(model);
            all_textures.extend(textures);
        }
        if !parts.is_empty() {
            return Ok((dff::Model::combine(parts), all_textures));
        }
    }

    let bytes = archive.read(index, &profile).map_err(|e| e.to_string())?;
    // GTA SA/MTA only applies vehicles.ide wheel scale when replacing unit wheels dynamically;
    // DFF meshes themselves already carry native wheel dimensions.
    let model = dff::decode(&bytes)?;
    let wanted: BTreeSet<String> = model
        .materials
        .iter()
        .map(|m| m.texture.trim().to_ascii_lowercase())
        .filter(|n| !n.is_empty())
        .collect();
    let mut dictionaries = BTreeSet::from([name.clone()]);
    if let Some(texture) = catalog
        .models
        .get(&name)
        .or_else(|| catalog.hashed_models.get(&gta_uppercase_hash(&name)))
    {
        dictionaries.insert(texture.clone());
    }
    for _ in 0..8 {
        let mut changed = false;
        for (child, parent) in &catalog.parents {
            if dictionaries.contains(child) && dictionaries.insert(parent.clone()) {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut textures = Vec::new();
    let mut loaded = BTreeSet::new();
    let mut pixels = 0usize;
    let mut push_from_txd = |bytes: &[u8]| {
        for texture in txd::matching(bytes, Some(&wanted)) {
            pixels += texture.width * texture.height;
            if pixels <= 67_108_864
                && !textures
                    .iter()
                    .any(|t: &txd::Texture| t.name.eq_ignore_ascii_case(&texture.name))
            {
                textures.push(texture);
            }
        }
    };
    let search_archives = gather_archives(archive, opened);
    let mut ordered_refs: Vec<Arc<Archive>> = Vec::with_capacity(search_archives.len());
    if let Some(primary) = search_archives
        .iter()
        .find(|a| a.path() == archive.path())
        .cloned()
    {
        ordered_refs.push(primary);
    }
    for candidate in search_archives {
        if candidate.path() != archive.path() {
            ordered_refs.push(candidate);
        }
    }
    for candidate in &ordered_refs {
        for entry in candidate.entries() {
            let lower = entry.name.to_ascii_lowercase();
            let Some(stem) = lower.strip_suffix(".txd") else {
                continue;
            };
            if !dictionaries.contains(stem) || !loaded.insert(lower) {
                continue;
            }
            if let Ok(bytes) = candidate.read(entry.index, &profile) {
                push_from_txd(&bytes);
            }
        }
    }
    let directory = archive.path().parent().unwrap_or_else(|| Path::new("."));
    let mut external_txds = vec![
        directory.join("generic/vehicle.txd"),
        directory.join("generic/wheels.txd"),
        directory.join("generic.txd"),
    ];
    for dict in &dictionaries {
        external_txds.push(directory.join("txd").join(format!("{dict}.txd")));
    }
    for path in external_txds {
        if let Ok(bytes) = std::fs::read(&path) {
            let bytes = profile.decode_file(&path, &bytes);
            push_from_txd(&bytes);
        }
    }
    let missing: BTreeSet<String> = wanted
        .iter()
        .filter(|w| !textures.iter().any(|t| txd::name_matches(w, &t.name)))
        .cloned()
        .collect();
    if !missing.is_empty() {
        let base_prefix = name.trim_end_matches(|c: char| c.is_ascii_digit());
        if !base_prefix.is_empty() && !dictionaries.contains(base_prefix) {
            for candidate in &ordered_refs {
                for entry in candidate.entries() {
                    let lower = entry.name.to_ascii_lowercase();
                    let Some(stem) = lower.strip_suffix(".txd") else {
                        continue;
                    };
                    if (stem == base_prefix || stem.eq_ignore_ascii_case("province"))
                        && loaded.insert(lower)
                        && let Ok(bytes) = candidate.read(entry.index, &profile)
                    {
                        for texture in txd::matching(&bytes, Some(&missing)) {
                            if !textures
                                .iter()
                                .any(|t: &txd::Texture| t.name.eq_ignore_ascii_case(&texture.name))
                            {
                                textures.push(texture);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok((model, textures))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires the authorized client archive in OPENPROVINCE_TEST_ARCHIVE"]
    fn inspected_supergt_resolves_base_and_three_native_paintjobs() {
        let path = std::env::var_os("OPENPROVINCE_TEST_ARCHIVE").expect("explicit fixture path");
        let archive = Archive::open(PathBuf::from(path)).expect("authorized fixture archive");
        let index = archive
            .entries()
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case("supergt.dff"))
            .expect("source model")
            .index;
        let (model, textures) = load(&archive, index).expect("complete source model preview");
        assert!(model.faces.len() > 10_000);
        assert!(
            model
                .faces
                .iter()
                .flat_map(|f| f.points)
                .flatten()
                .all(f32::is_finite)
        );
        assert!(!textures.is_empty());
        let targets: Vec<_> = model
            .materials
            .iter()
            .map(|m| m.texture.clone())
            .filter(|n| n.starts_with('#') || n.eq_ignore_ascii_case("remap"))
            .collect();
        assert!(
            !targets.is_empty(),
            "source paintable material must remain available"
        );
        let variants = variants(&archive, index);
        assert_eq!(
            variants.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            ["supergt1.txd", "supergt2.txd", "supergt3.txd"]
        );
        for variant in variants {
            let textures = load_variant(&variant, &targets).expect("native paintjob dictionary");
            for target in &targets {
                assert!(textures.iter().any(|t| t.name.eq_ignore_ascii_case(target)));
            }
        }
    }
    #[test]
    fn numbered_dictionaries_are_related_only_to_exact_model_stem() {
        assert_eq!(variant_number("supergt", "supergt1"), Some(1));
        assert_eq!(variant_number("supergt", "supergt12"), Some(12));
        for name in [
            "supergt",
            "supergt_police",
            "supergt1extra",
            "supergta",
            "superg",
        ] {
            assert_eq!(variant_number("supergt", name), None);
        }
    }
    #[test]
    fn vehicle_dimensions_come_from_matching_ide_row() {
        let ide = "cars\n506, supergt, supergt, car, SUPERGT, SUPERGT, null, executive, 5, 0, 0, -1, 0.81, 0.86, 0\nend";
        assert_eq!(parse_wheel_sizes(ide, "SUPERGT"), Some([0.81, 0.86]));
        assert_eq!(parse_wheel_sizes(ide, "supergt1"), None);
        assert_eq!(
            parse_wheel_sizes(&ide.replace("0.81", "NaN"), "supergt"),
            None
        );
    }
    #[test]
    fn ide_objects_and_shopping_populate_catalog() {
        let ide = "objs\n1200, 120tka1, pgtj, 150, 0\nend\ntxdp\npgtj, province_common\nend\n";
        let shopping = "section Clothes\njeansdenim JEANS jeans 2 respect 0 sexy 0 50\nend\n";
        let mut catalog = Catalog::default();
        parse_ide_into(ide, &mut catalog);
        parse_shopping_into(shopping, &mut catalog);
        assert_eq!(
            catalog.models.get("120tka1").map(String::as_str),
            Some("pgtj")
        );
        assert!(
            catalog
                .parents
                .contains(&("pgtj".into(), "province_common".into()))
        );
        assert_eq!(catalog.player_clothes.len(), 1);
        assert_eq!(catalog.player_clothes[0].model, "jeans");
        assert_eq!(catalog.player_clothes[0].slot, 2);
    }
    #[test]
    fn map_bin_parser_resolves_hashed_models_and_txdp() {
        assert_eq!(gta_uppercase_hash("Mirnnew100"), 0x299a_569c);
        assert_eq!(gta_uppercase_hash("F1"), 0x49db_05b9);

        let mut bin = Vec::new();
        bin.extend_from_slice(&0x2A09_3EF5u32.to_le_bytes());
        bin.extend_from_slice(&2u32.to_le_bytes());

        // Record 0: type=1, hash=gta_uppercase_hash("Mirnnew100"), txd="Mirnnew20"
        bin.push(1);
        bin.extend_from_slice(&9063u32.to_le_bytes());
        bin.extend_from_slice(&0u32.to_le_bytes());
        bin.extend_from_slice(&gta_uppercase_hash("mirnnew100").to_le_bytes());
        bin.extend_from_slice(&[0u8; 24]);
        let txd = b"Mirnnew20";
        bin.extend_from_slice(&(txd.len() as u32).to_le_bytes());
        bin.extend_from_slice(txd);

        // Group marker + Record 1: type=6 (txdp), child="Mirnnew20", parent="province_map"
        bin.push(0xFF);
        bin.push(6);
        let child = b"Mirnnew20";
        let parent = b"province_map";
        bin.extend_from_slice(&(child.len() as u32).to_le_bytes());
        bin.extend_from_slice(child);
        bin.extend_from_slice(&(parent.len() as u32).to_le_bytes());
        bin.extend_from_slice(parent);

        let mut catalog = Catalog::default();
        parse_map_bin_into(&bin, &mut catalog);
        assert_eq!(
            catalog
                .hashed_models
                .get(&gta_uppercase_hash("mirnnew100"))
                .map(String::as_str),
            Some("mirnnew20")
        );
        assert!(
            catalog
                .parents
                .contains(&("mirnnew20".into(), "province_map".into()))
        );
    }
}

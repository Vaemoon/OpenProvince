use province_mods::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_manifest_roundtrip() {
    let mut manifest = ModManifest::new("bmw_m5_cs", "BMW M5 CS");
    manifest.author = "Author".into();
    manifest.category = ModCategory::Vehicles;
    manifest.replaces.push(ReplacementEntry {
        archive: Some("province_vehs.img".into()),
        file: "bmw_m5.dff".into(),
    });

    let dir = tempdir().unwrap();
    let file = dir.path().join("mod.json");
    manifest.save_to_file(&file).unwrap();

    let loaded = ModManifest::load_from_file(&file).unwrap();
    assert_eq!(loaded.id, "bmw_m5_cs");
    assert_eq!(loaded.name, "BMW M5 CS");
    assert_eq!(loaded.replaces.len(), 1);
    assert_eq!(loaded.replaces[0].file, "bmw_m5.dff");
}

#[test]
fn test_conflict_detection() {
    let dir = tempdir().unwrap();
    let mods_dir = dir.path().join("mods");
    let state_file = dir.path().join("mods_state.json");

    let mut reg = ModRegistry::new(&mods_dir, &state_file);

    let mod1_dir = mods_dir.join("mod1");
    fs::create_dir_all(mod1_dir.join("files")).unwrap();
    let mut m1 = ModManifest::new("mod1", "Mod 1");
    m1.replaces.push(ReplacementEntry {
        archive: Some("province_vehs.img".into()),
        file: "supergt.dff".into(),
    });
    m1.save_to_file(&mod1_dir.join("mod.json")).unwrap();

    let mod2_dir = mods_dir.join("mod2");
    fs::create_dir_all(mod2_dir.join("files")).unwrap();
    let mut m2 = ModManifest::new("mod2", "Mod 2");
    m2.replaces.push(ReplacementEntry {
        archive: Some("province_vehs.img".into()),
        file: "supergt.dff".into(),
    });
    m2.save_to_file(&mod2_dir.join("mod.json")).unwrap();

    reg.scan_and_load().unwrap();
    assert_eq!(reg.mods().len(), 2);

    assert!(reg.detect_conflicts().is_empty());

    reg.set_mod_enabled("mod1", true);
    reg.set_mod_enabled("mod2", true);

    let conflicts = reg.detect_conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].file, "supergt.dff");
    assert_eq!(conflicts[0].conflicting_mod_ids.len(), 2);
}

#[test]
fn test_package_export_and_import() {
    let dir = tempdir().unwrap();
    let mod_source_dir = dir.path().join("my_mod");
    fs::create_dir_all(mod_source_dir.join("files")).unwrap();

    let mut manifest = ModManifest::new("audi_rs6", "Audi RS6 C8");
    manifest.author = "TestAuthor".into();
    manifest.replaces.push(ReplacementEntry {
        archive: Some("province_vehs.img".into()),
        file: "stratum.dff".into(),
    });
    manifest.replaces.push(ReplacementEntry {
        archive: Some("province_vehs.img".into()),
        file: "stratum.txd".into(),
    });
    manifest
        .save_to_file(&mod_source_dir.join("mod.json"))
        .unwrap();

    fs::write(mod_source_dir.join("files/stratum.dff"), b"DFF_DATA").unwrap();
    fs::write(mod_source_dir.join("files/stratum.txd"), b"TXD_DATA").unwrap();
    fs::write(mod_source_dir.join("preview.png"), b"PNG_DATA").unwrap();

    let opmod_path = dir.path().join("Audi_RS6.opmod");
    ModPackage::export_to_opmod(&mod_source_dir, &opmod_path).unwrap();
    assert!(opmod_path.is_file());

    let library_dir = dir.path().join("library_mods");
    let imported_manifest = ModPackage::import_from_zip(&opmod_path, &library_dir, None).unwrap();

    assert_eq!(imported_manifest.id, "audi_rs6");
    assert_eq!(imported_manifest.name, "Audi RS6 C8");
    assert_eq!(imported_manifest.replaces.len(), 2);

    let installed_dir = library_dir.join("audi_rs6");
    assert!(installed_dir.join("mod.json").is_file());
    assert!(installed_dir.join("preview.png").is_file());
    assert_eq!(
        fs::read(installed_dir.join("files/stratum.dff")).unwrap(),
        b"DFF_DATA"
    );
    assert_eq!(
        fs::read(installed_dir.join("files/stratum.txd")).unwrap(),
        b"TXD_DATA"
    );
}

#[test]
fn inspect_package_reads_metadata_without_extracting() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source");
    fs::create_dir_all(source.join("files")).unwrap();
    ModManifest::new("inspection", "Inspection")
        .save_to_file(&source.join("mod.json"))
        .unwrap();
    fs::write(source.join("files/model.dff"), b"resource").unwrap();
    let package = dir.path().join("inspection.opmod");
    ModPackage::export_to_opmod(&source, &package).unwrap();
    let before = fs::read_dir(dir.path()).unwrap().count();
    let manifest = ModPackage::read_manifest(&package).unwrap().unwrap();
    assert_eq!(manifest.id, "inspection");
    assert_eq!(before, fs::read_dir(dir.path()).unwrap().count());
}

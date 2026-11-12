//! Транзакционная замена архивов с сохранением резервной копии на том же томе.
use province_archive::{Archive, Edit, Profile};
use std::path::Path;

pub fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = path.with_extension(format!("{stamp}.openprovince.tmp"));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        if path.exists() {
            replace(
                path,
                &temporary,
                &path.with_extension(format!("{stamp}.backup")),
            )
        } else {
            std::fs::rename(&temporary, path).map_err(|e| e.to_string())
        }
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
pub fn save(archive: &Archive, edits: &[Edit]) -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = archive
        .path()
        .with_extension(format!("{stamp}.openprovince.tmp"));
    let backup = archive.path().with_extension(format!("{stamp}.backup.img"));
    archive
        .save_as(&temporary, edits, &Profile::inspected_client())
        .map_err(|e| e.to_string())?;
    if let Err(e) = Archive::open(&temporary) {
        let _ = std::fs::remove_file(&temporary);
        return Err(e.to_string());
    }
    let result = replace(archive.path(), &temporary, &backup);
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
#[cfg(windows)]
fn replace(original: &Path, replacement: &Path, backup: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            original: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *const std::ffi::c_void,
            reserved: *const std::ffi::c_void,
        ) -> i32;
    }
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    // Нуль-терминированные буферы путей живут до конца синхронного вызова.
    if unsafe {
        ReplaceFileW(
            wide(original).as_ptr(),
            wide(replacement).as_ptr(),
            wide(backup).as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn replace(original: &Path, replacement: &Path, backup: &Path) -> Result<(), String> {
    std::fs::copy(original, backup)
        .and_then(|_| std::fs::rename(replacement, original))
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commit_preserves_original_backup_and_reindexes_new_archive() {
        let root = std::env::temp_dir().join(format!("openprovince-commit-{}", std::process::id()));
        std::fs::create_dir_all(root.join("input")).expect("fixture directory");
        std::fs::write(root.join("input/test.txt"), b"original").expect("fixture");
        let path = root.join("test.img");
        province_archive::pack_directory(&root.join("input"), &path, &Profile::inspected_client())
            .expect("fixture IMG");
        let before = std::fs::read(&path).expect("original");
        std::fs::write(root.join("replacement.txt"), b"changed").expect("replacement");
        let archive = Archive::open(&path).expect("index");
        save(
            &archive,
            &[Edit::Replace {
                name: "test.txt".into(),
                source: root.join("replacement.txt"),
            }],
        )
        .expect("transaction");
        let saved = Archive::open(&path).expect("saved IMG");
        assert!(
            saved
                .read(0, &Profile::inspected_client())
                .expect("payload")
                .starts_with(b"changed")
        );
        let backup = std::fs::read_dir(&root)
            .expect("fixture")
            .flatten()
            .find(|f| f.file_name().to_string_lossy().ends_with("backup.img"))
            .expect("backup");
        assert_eq!(std::fs::read(backup.path()).expect("backup bytes"), before);
        std::fs::remove_dir_all(root).expect("clean fixture");
    }
}

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
        let backup = path.with_extension("openprovince.bak");
        replace(path, &temporary, &backup)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn save(archive: &Archive, edits: &[Edit]) -> Result<Archive, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let original = archive.path().to_path_buf();
    let temporary = original.with_extension(format!("{stamp}.openprovince.tmp"));
    let backup = original.with_extension("openprovince.bak");
    let profile = Profile::inspected_client();
    archive
        .save_as(&temporary, edits, &profile)
        .map_err(|e| e.to_string())?;
    let outcome = replace(&original, &temporary, &backup);
    if let Err(error) = outcome {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Archive::open(&original).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn replace(original: &Path, replacement: &Path, backup: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            lpReplacedFileName: *const u16,
            lpReplacementFileName: *const u16,
            lpBackupFileName: *const u16,
            dwReplaceFlags: u32,
            lpExclude: *const std::ffi::c_void,
            lpReserved: *const std::ffi::c_void,
        ) -> i32;
    }
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
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

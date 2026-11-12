use eframe::egui;
use province_archive::inspect;
use std::{path::PathBuf, sync::Arc};

#[cfg(not(test))]
fn last_dir_state_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("OpenProvince").join("last_dir.txt"))
}

fn default_models_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![
        PathBuf::from(r"C:\Province Games\bin\models"),
        PathBuf::from(r"D:\Province Games\bin\models"),
        PathBuf::from(r"C:\Program Files (x86)\Province Games\bin\models"),
        PathBuf::from(r"C:\Games\Province Games\bin\models"),
        PathBuf::from(r"C:\Province Games Offline\game\models"),
        PathBuf::from(r"Z:\Province Offline\game\models"),
    ];
    for drive in b'C'..=b'Z' {
        let letter = char::from(drive);
        candidates.push(PathBuf::from(format!(r"{letter}:\Province Games\bin\models")));
        candidates.push(PathBuf::from(format!(r"{letter}:\Province\bin\models")));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("models"));
        candidates.push(dir.join("bin").join("models"));
        candidates.push(dir.join("game").join("models"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("models"));
        candidates.push(cwd.join("bin").join("models"));
        candidates.push(cwd.join("game").join("models"));
    }
    candidates
}

pub(crate) fn initial_folder() -> PathBuf {
    #[cfg(not(test))]
    if let Some(state_path) = last_dir_state_path()
        && let Ok(saved) = std::fs::read_to_string(state_path)
    {
        let path = PathBuf::from(saved.trim());
        if path.is_dir() {
            return path;
        }
    }
    candidates_dir(&default_models_candidates())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

fn candidates_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.is_dir()).cloned()
}

pub(crate) fn remember_folder(path: &std::path::Path) {
    #[cfg(test)]
    let _ = path;
    #[cfg(not(test))]
    {
        if !path.is_dir() || path.starts_with(std::env::temp_dir()) {
            return;
        }
        if let Some(state_path) = last_dir_state_path() {
            if let Some(parent) = state_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(state_path, path.to_string_lossy().as_bytes());
        }
    }
}

pub(crate) fn install_system_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(ui_font) = ["C:/Windows/Fonts/SegUIVar.ttf", "C:/Windows/Fonts/segoeui.ttf"]
        .iter()
        .find_map(|p| std::fs::read(p).ok())
    {
        fonts
            .font_data
            .insert("ui".into(), Arc::new(egui::FontData::from_owned(ui_font)));
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "ui".into());
    }
    if let Some(mono_font) = [
        "C:/Windows/Fonts/CascadiaMono.ttf",
        "C:/Windows/Fonts/consola.ttf",
    ]
    .iter()
    .find_map(|p| std::fs::read(p).ok())
    {
        fonts
            .font_data
            .insert("mono".into(), Arc::new(egui::FontData::from_owned(mono_font)));
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "mono".into());
    }
    ctx.set_fonts(fonts);
}

pub(crate) fn format_hex_report(name: &str, bytes: &[u8]) -> String {
    let report = inspect(name, bytes);
    let mut text = format!(
        "{}\nFormat: {}\nDecoded bytes: {}\nLogical bytes: {:?}\n{}\n\nHex preview (first 512 bytes)\n",
        name,
        report.format,
        bytes.len(),
        report.logical_size,
        report.warning.unwrap_or_default()
    );
    for (i, row) in bytes[..bytes.len().min(512)].chunks(16).enumerate() {
        text.push_str(&format!("{:08X}  ", i * 16));
        for byte in row {
            text.push_str(&format!("{byte:02X} "));
        }
        text.push_str("  ");
        for byte in row {
            text.push(if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                '.'
            });
        }
        text.push('\n');
    }
    text
}

#[cfg(windows)]
pub(crate) fn apply_native_titlebar_theme() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumThreadWindows(
            dw_thread_id: u32,
            lpfn: unsafe extern "system" fn(isize, isize) -> i32,
            l_param: isize,
        ) -> i32;
    }
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: isize,
            dw_attribute: u32,
            pv_attribute: *const std::ffi::c_void,
            cb_attribute: u32,
        ) -> i32;
    }

    unsafe extern "system" fn paint_window(hwnd: isize, _: isize) -> i32 {
        let dark: i32 = 1;
        let caption: u32 = 0x0020_1E1E; // BG_CHROME rgb(30, 30, 32)
        let border: u32 = 0x003A_3535;
        let text: u32 = 0x00E8_E4E2;
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                20,
                (&raw const dark).cast(),
                std::mem::size_of_val(&dark) as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                35,
                (&raw const caption).cast(),
                std::mem::size_of_val(&caption) as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                34,
                (&raw const border).cast(),
                std::mem::size_of_val(&border) as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                36,
                (&raw const text).cast(),
                std::mem::size_of_val(&text) as u32,
            );
        }
        1
    }

    unsafe {
        let tid = GetCurrentThreadId();
        let _ = EnumThreadWindows(tid, paint_window, 0);
    }
}

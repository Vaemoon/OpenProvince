mod controls;
mod processes;

use crate::Browser;
pub(crate) use controls::{Action, control};
pub(crate) use processes::{Session, handle_stop_request};
use std::path::{Path, PathBuf};

#[cfg(windows)]
fn run_as_admin(exe: &Path, working_dir: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let file_wide: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let dir_wide: Vec<u16> = working_dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let verb_wide: Vec<u16> = std::ffi::OsStr::new("runas")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn ShellExecuteW(
                hwnd: *mut std::ffi::c_void,
                lpOperation: *const u16,
                lpFile: *const u16,
                lpParameters: *const u16,
                lpDirectory: *const u16,
                nShowCmd: i32,
            ) -> isize;
        }

        let ret = ShellExecuteW(
            std::ptr::null_mut(),
            verb_wide.as_ptr(),
            file_wide.as_ptr(),
            std::ptr::null(),
            dir_wide.as_ptr(),
            1,
        );

        if ret > 32 {
            Ok(())
        } else {
            Err(format!("Код ошибки ShellExecute: {ret}"))
        }
    }
}

#[cfg(not(windows))]
fn run_as_admin(exe: &Path, working_dir: &Path) -> Result<(), String> {
    std::process::Command::new(exe)
        .current_dir(working_dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

impl Browser {
    pub(crate) fn find_mta_exe(&self) -> Option<PathBuf> {
        if let Some(custom) = &self.custom_mta_exe {
            if custom.exists() {
                return Some(custom.clone());
            }
        }
        let candidates = [
            PathBuf::from(r"C:\Province Games\Multi Theft Auto.exe"),
            PathBuf::from(r"C:\Province Games\launcher.exe"),
            PathBuf::from(r"C:\Program Files\MTA San Andreas 1.6\Multi Theft Auto.exe"),
            PathBuf::from(r"C:\Program Files (x86)\MTA San Andreas 1.6\Multi Theft Auto.exe"),
            PathBuf::from(r"C:\Province Games\bin\gta_sa.exe"),
        ];
        candidates.into_iter().find(|p| p.exists())
    }

    pub(crate) fn pick_mta_exe(&mut self) {
        if let Some(picked) = rfd::FileDialog::new()
            .set_title("Укажите исполняемый файл Multi Theft Auto.exe")
            .add_filter("Исполняемый файл (*.exe)", &["exe"])
            .pick_file()
        {
            self.status = format!("Указан исполняемый файл: {}", picked.display());
            self.custom_mta_exe = Some(picked);
        }
    }

    pub(crate) fn launch_game_with_mods(&mut self, with_mods: bool) {
        if self.game_session.running() {
            return;
        }
        if with_mods {
            if let Err(err) = self.mod_manager.apply_active_mods() {
                self.status = format!("Запуск отменён: {err}");
                return;
            }
        } else {
            if let Err(err) = self.mod_manager.restore_stock() {
                self.status = format!("Запуск отменён: {err}");
                return;
            }
        }

        let exe = match self.find_mta_exe() {
            Some(e) => e,
            None => {
                if let Some(picked) = rfd::FileDialog::new()
                    .set_title("Укажите Multi Theft Auto.exe")
                    .add_filter("Исполняемый файл (*.exe)", &["exe"])
                    .pick_file()
                {
                    self.custom_mta_exe = Some(picked.clone());
                    picked
                } else {
                    self.status = "Файл Multi Theft Auto.exe не найден.".into();
                    return;
                }
            }
        };

        let working_dir = exe.parent().unwrap_or(std::path::Path::new("."));
        match run_as_admin(&exe, working_dir) {
            Ok(()) => {
                self.game_session.invalidate();
                let count_str = {
                    #[cfg(feature = "mods")]
                    {
                        if with_mods {
                            let count = self
                                .mod_manager
                                .registry
                                .mods()
                                .iter()
                                .filter(|m| m.enabled)
                                .count();
                            if count > 0 {
                                format!("с {count} активн. мод.")
                            } else {
                                "чистая (без модов)".to_string()
                            }
                        } else {
                            "чистая (без модов)".to_string()
                        }
                    }
                    #[cfg(not(feature = "mods"))]
                    {
                        let _ = with_mods;
                        "чистая".to_string()
                    }
                };
                self.status =
                    format!("Multi Theft Auto.exe запущен от имени администратора ({count_str})");
            }
            Err(err) => {
                self.status = format!("Ошибка запуска Multi Theft Auto.exe: {err}");
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn launch_game(&mut self) {
        self.launch_game_with_mods(true);
    }

    pub(crate) fn stop_game(&mut self) {
        match self.game_session.stop() {
            Ok(()) => self.status = "Остановка игры…".into(),
            Err(error) => self.status = format!("Не удалось остановить игру: {error}"),
        }
    }
}

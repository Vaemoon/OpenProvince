use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Process {
    pid: u32,
    path: PathBuf,
}

#[derive(Default)]
pub(crate) struct Session {
    processes: Vec<Process>,
    pending: Option<Receiver<Vec<Process>>>,
    checked: Option<Instant>,
    stop_requested: Option<Instant>,
}

impl Session {
    pub(crate) fn running(&self) -> bool {
        !self.processes.is_empty()
    }
    pub(crate) fn stopping(&self) -> bool {
        self.stop_requested.is_some()
    }
    pub(crate) fn invalidate(&mut self) {
        self.checked = None;
    }
    pub(crate) fn refresh(&mut self, exe: Option<PathBuf>, root: &Path) {
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(processes) => {
                    self.processes = processes;
                    if self.processes.is_empty()
                        || self
                            .stop_requested
                            .is_some_and(|time| time.elapsed() > Duration::from_secs(10))
                    {
                        self.stop_requested = None;
                    }
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.pending.is_some()
            || self
                .checked
                .is_some_and(|time| time.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        let mut paths = vec![root.join("gta_sa.exe")];
        if let Some(exe) = exe {
            if let Some(parent) = exe.parent() {
                paths.push(parent.join("gta_sa.exe"));
                paths.push(parent.join("bin/gta_sa.exe"));
            }
            paths.push(exe);
        }
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.checked = Some(Instant::now());
        std::thread::spawn(move || {
            let _ = sender.send(native::find(&paths));
        });
    }
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        if self.stop_requested.is_some() {
            return Ok(());
        }
        self.stop_requested = Some(Instant::now());
        for process in &self.processes {
            if let Err(error) = native::stop(process) {
                if error.elevate {
                    native::elevated_stop(process)?;
                } else {
                    self.stop_requested = None;
                    return Err(error.message);
                }
            }
        }
        self.invalidate();
        Ok(())
    }
}

pub(crate) fn handle_stop_request() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--stop-game-process") {
        return false;
    }
    if let (Some(pid), Some(path)) = (args.next().and_then(|p| p.parse().ok()), args.next()) {
        let _ = native::stop(&Process {
            pid,
            path: path.into(),
        });
    }
    true
}

#[derive(Debug)]
struct StopError {
    message: String,
    elevate: bool,
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        ffi::c_void,
        mem::{size_of, zeroed},
        os::windows::ffi::OsStrExt,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
            },
        },
        UI::Shell::ShellExecuteW,
    };
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    fn image(handle: HANDLE) -> Option<PathBuf> {
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
            return None;
        }
        Some(PathBuf::from(String::from_utf16_lossy(
            &buffer[..length as usize],
        )))
    }
    fn same_path(left: &Path, right: &Path) -> bool {
        left.to_string_lossy()
            .replace('/', "\\")
            .eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\"))
    }
    pub(super) fn find(paths: &[PathBuf]) -> Vec<Process> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let snapshot = Handle(snapshot);
        let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut valid = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
        let mut result = Vec::new();
        while valid {
            let name = String::from_utf16_lossy(
                &entry.szExeFile[..entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len())],
            );
            if paths.iter().any(|p| {
                p.file_name()
                    .is_some_and(|file| file.to_string_lossy().eq_ignore_ascii_case(&name))
            }) {
                let handle = unsafe {
                    OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID)
                };
                if !handle.is_null() {
                    let handle = Handle(handle);
                    if let Some(path) = image(handle.0)
                        && paths.iter().any(|p| same_path(p, &path))
                    {
                        result.push(Process {
                            pid: entry.th32ProcessID,
                            path,
                        });
                    }
                }
            }
            valid = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
        }
        result
    }
    pub(super) fn stop(process: &Process) -> Result<(), StopError> {
        let failure = || {
            let error = std::io::Error::last_os_error();
            StopError {
                elevate: error.raw_os_error() == Some(5),
                message: error.to_string(),
            }
        };
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                0,
                process.pid,
            )
        };
        if handle.is_null() {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(87 | 1168)) {
                return Ok(());
            }
            return Err(failure());
        }
        let handle = Handle(handle);
        let mut exit_code = 259;
        if unsafe { GetExitCodeProcess(handle.0, &mut exit_code) } != 0 && exit_code != 259 {
            return Ok(());
        }
        if !image(handle.0).is_some_and(|path| same_path(&path, &process.path)) {
            return Err(StopError {
                elevate: false,
                message: "Процесс игры изменился".into(),
            });
        }
        if unsafe { TerminateProcess(handle.0, 0) } == 0 {
            return Err(failure());
        }
        Ok(())
    }
    pub(super) fn elevated_stop(process: &Process) -> Result<(), String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
        let args = format!(
            "--stop-game-process {} \"{}\"",
            process.pid,
            process.path.display()
        );
        let file = wide(executable.as_os_str());
        let parameters = wide(std::ffi::OsStr::new(&args));
        let verb = wide(std::ffi::OsStr::new("runas"));
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut::<c_void>(),
                verb.as_ptr(),
                file.as_ptr(),
                parameters.as_ptr(),
                std::ptr::null(),
                0,
            )
        };
        if result as isize <= 32 {
            return Err("Запрос на остановку не выполнен".into());
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod native {
    use super::*;
    pub(super) fn find(_: &[PathBuf]) -> Vec<Process> {
        Vec::new()
    }
    pub(super) fn stop(_: &Process) -> Result<(), StopError> {
        Err(StopError {
            message: "Остановка поддерживается в Windows".into(),
            elevate: false,
        })
    }
    pub(super) fn elevated_stop(process: &Process) -> Result<(), String> {
        stop(process).map_err(|error| error.message)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    #[test]
    fn detects_and_stops_only_the_verified_executable() {
        let root =
            std::env::temp_dir().join(format!("openprovince-session-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let executable = root.join("game-session-probe.exe");
        std::fs::copy("C:/Windows/System32/cmd.exe", &executable).unwrap();
        let mut child = std::process::Command::new(&executable)
            .args(["/c", "ping -n 20 127.0.0.1 >nul"])
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let processes = native::find(&[executable.clone()]);
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].pid, child.id());
        let incorrect = Process {
            pid: child.id(),
            path: root.join("unrelated.exe"),
        };
        assert!(native::stop(&incorrect).is_err());
        assert!(child.try_wait().unwrap().is_none());
        native::stop(&processes[0]).unwrap();
        child.wait().unwrap();
        native::stop(&processes[0]).unwrap();
        std::fs::remove_file(executable).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

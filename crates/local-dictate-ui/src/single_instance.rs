use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceCommand {
    Show,
    Exit,
}

#[derive(Debug)]
pub enum StartupAction {
    Run(Option<SingleInstance>),
    Exit,
}

#[cfg(target_os = "windows")]
pub use windows::SingleInstance;

#[cfg(unix)]
pub use unix::SingleInstance;

#[cfg(target_os = "windows")]
pub fn prepare_startup() -> StartupAction {
    windows::prepare_startup()
}

#[cfg(unix)]
pub fn prepare_startup() -> StartupAction {
    unix::prepare_startup()
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let max_len = left_parts.len().max(right_parts.len());

    for index in 0..max_len {
        let left = left_parts.get(index).copied().unwrap_or_default();
        let right = right_parts.get(index).copied().unwrap_or_default();

        match left.cmp(&right) {
            Ordering::Equal => {}
            order => return order,
        }
    }

    Ordering::Equal
}

fn version_parts(version: &str) -> Vec<u64> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u64>().unwrap_or_default())
        .collect()
}

#[cfg(unix)]
mod unix {
    use super::{InstanceCommand, StartupAction, compare_versions};
    use directories::ProjectDirs;
    use std::cmp::Ordering;
    use std::fs::{self, File, OpenOptions};
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::{Duration, SystemTime};

    const COMMAND_FILE_NAME: &str = "instance-command.txt";
    const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
    const LOCK_FILE_NAME: &str = "instance.lock";
    const STATE_FILE_NAME: &str = "instance-state.txt";

    #[derive(Debug)]
    pub struct SingleInstance {
        _lock_file: File,
        lock_path: PathBuf,
        state_path: PathBuf,
        command_path: PathBuf,
        last_command_modified: Option<SystemTime>,
    }

    impl SingleInstance {
        pub fn poll_command(&mut self) -> Option<InstanceCommand> {
            let metadata = fs::metadata(&self.command_path).ok()?;
            let modified = metadata.modified().ok();

            if modified.is_some() && modified == self.last_command_modified {
                return None;
            }

            self.last_command_modified = modified;
            let text = fs::read_to_string(&self.command_path).ok()?;
            let _ = fs::remove_file(&self.command_path);
            parse_command(&text)
        }
    }

    impl Drop for SingleInstance {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.state_path);
            let _ = fs::remove_file(&self.lock_path);
        }
    }

    pub fn prepare_startup() -> StartupAction {
        let Ok(paths) = InstancePaths::new() else {
            return StartupAction::Run(None);
        };

        match try_acquire_lock(&paths.lock_path) {
            LockAttempt::Acquired(lock_file) => run_as_primary(lock_file, paths),
            LockAttempt::AlreadyExists => handle_existing_instance(paths),
            LockAttempt::Failed => StartupAction::Run(None),
        }
    }

    fn run_as_primary(lock_file: File, paths: InstancePaths) -> StartupAction {
        let _ = fs::remove_file(&paths.command_path);
        let _ = write_state(&paths.state_path);

        StartupAction::Run(Some(SingleInstance {
            _lock_file: lock_file,
            lock_path: paths.lock_path,
            state_path: paths.state_path,
            command_path: paths.command_path,
            last_command_modified: None,
        }))
    }

    fn handle_existing_instance(paths: InstancePaths) -> StartupAction {
        let state = read_state(&paths.state_path);

        let stale_lock = match state.as_ref().and_then(|state| state.pid) {
            Some(pid) => !process_is_running(pid),
            None => true,
        };

        if stale_lock {
            let _ = fs::remove_file(&paths.lock_path);
            if let LockAttempt::Acquired(lock_file) = try_acquire_lock(&paths.lock_path) {
                return run_as_primary(lock_file, paths);
            }
        }

        let running_version = state
            .as_ref()
            .map(|state| state.version.as_str())
            .unwrap_or_default();

        if !running_version.is_empty()
            && compare_versions(CURRENT_VERSION, running_version) == Ordering::Greater
        {
            let _ = write_command(&paths.command_path, InstanceCommand::Exit);

            if let Some(lock_file) =
                wait_for_primary_to_exit(&paths.lock_path, Duration::from_secs(6))
            {
                return run_as_primary(lock_file, paths);
            }

            if let Some(pid) = state.and_then(|state| state.pid)
                && terminate_existing_process(pid)
                && let Some(lock_file) =
                    wait_for_primary_to_exit(&paths.lock_path, Duration::from_secs(3))
            {
                return run_as_primary(lock_file, paths);
            }
        }

        let _ = write_command(&paths.command_path, InstanceCommand::Show);
        StartupAction::Exit
    }

    fn wait_for_primary_to_exit(lock_path: &Path, timeout: Duration) -> Option<File> {
        let started = std::time::Instant::now();

        while started.elapsed() < timeout {
            match try_acquire_lock(lock_path) {
                LockAttempt::Acquired(lock_file) => return Some(lock_file),
                LockAttempt::AlreadyExists => thread::sleep(Duration::from_millis(100)),
                LockAttempt::Failed => return None,
            }
        }

        None
    }

    enum LockAttempt {
        Acquired(File),
        AlreadyExists,
        Failed,
    }

    fn try_acquire_lock(path: &Path) -> LockAttempt {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(file) => LockAttempt::Acquired(file),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => LockAttempt::AlreadyExists,
            Err(_) => LockAttempt::Failed,
        }
    }

    fn process_is_running(pid: u32) -> bool {
        let pid = pid as libc::pid_t;
        if pid == std::process::id() as libc::pid_t {
            return true;
        }

        unsafe { libc::kill(pid, 0) == 0 }
    }

    fn terminate_existing_process(pid: u32) -> bool {
        let pid = pid as libc::pid_t;
        if pid == std::process::id() as libc::pid_t {
            return false;
        }

        unsafe { libc::kill(pid, libc::SIGTERM) == 0 }
    }

    #[derive(Debug)]
    struct InstancePaths {
        lock_path: PathBuf,
        state_path: PathBuf,
        command_path: PathBuf,
    }

    impl InstancePaths {
        fn new() -> Result<Self, ()> {
            let project_dirs =
                ProjectDirs::from("dev", "Local Dictate", "Local Dictate").ok_or(())?;
            let directory = project_dirs.data_local_dir().join("instance");
            fs::create_dir_all(&directory).map_err(|_| ())?;

            Ok(Self {
                lock_path: directory.join(LOCK_FILE_NAME),
                state_path: directory.join(STATE_FILE_NAME),
                command_path: directory.join(COMMAND_FILE_NAME),
            })
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct InstanceState {
        version: String,
        pid: Option<u32>,
    }

    fn write_state(path: &Path) -> Result<(), std::io::Error> {
        let exe = std::env::current_exe()
            .ok()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let text = format!(
            "version={CURRENT_VERSION}\npid={}\nexe={exe}\n",
            std::process::id()
        );

        fs::write(path, text)
    }

    fn read_state(path: &Path) -> Option<InstanceState> {
        let text = fs::read_to_string(path).ok()?;
        let version = read_key(&text, "version")?.to_string();
        let pid = read_key(&text, "pid").and_then(|pid| pid.parse::<u32>().ok());

        Some(InstanceState { version, pid })
    }

    fn write_command(path: &Path, command: InstanceCommand) -> Result<(), std::io::Error> {
        let command = match command {
            InstanceCommand::Show => "show",
            InstanceCommand::Exit => "exit",
        };
        let text = format!("command={command}\nversion={CURRENT_VERSION}\n");

        fs::write(path, text)
    }

    fn parse_command(text: &str) -> Option<InstanceCommand> {
        match read_key(text, "command")? {
            "show" => Some(InstanceCommand::Show),
            "exit" => Some(InstanceCommand::Exit),
            _ => None,
        }
    }

    fn read_key<'a>(text: &'a str, key: &str) -> Option<&'a str> {
        text.lines().find_map(|line| {
            let (candidate_key, value) = line.split_once('=')?;
            (candidate_key == key).then_some(value.trim())
        })
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::{InstanceCommand, StartupAction, compare_versions};
    use directories::ProjectDirs;
    use std::cmp::Ordering;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::ptr;
    use std::thread;
    use std::time::{Duration, SystemTime};
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, SetLastError,
    };
    use windows_sys::Win32::System::Threading::{
        CreateMutexW, OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, ReleaseMutex,
        TerminateProcess, WaitForSingleObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    const APP_TITLE: &str = "Local Dictate Settings";
    const COMMAND_FILE_NAME: &str = "instance-command.txt";
    const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
    const MUTEX_NAME: &str = "Local\\dev.local-dictate.single-instance";
    const STATE_FILE_NAME: &str = "instance-state.txt";

    #[derive(Debug)]
    pub struct SingleInstance {
        mutex: HANDLE,
        state_path: PathBuf,
        command_path: PathBuf,
        last_command_modified: Option<SystemTime>,
    }

    impl SingleInstance {
        pub fn poll_command(&mut self) -> Option<InstanceCommand> {
            let metadata = fs::metadata(&self.command_path).ok()?;
            let modified = metadata.modified().ok();

            if modified.is_some() && modified == self.last_command_modified {
                return None;
            }

            self.last_command_modified = modified;
            let text = fs::read_to_string(&self.command_path).ok()?;
            let _ = fs::remove_file(&self.command_path);
            parse_command(&text)
        }
    }

    impl Drop for SingleInstance {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.state_path);

            unsafe {
                ReleaseMutex(self.mutex);
                CloseHandle(self.mutex);
            }
        }
    }

    pub fn prepare_startup() -> StartupAction {
        let Ok(paths) = InstancePaths::new() else {
            return StartupAction::Run(None);
        };

        match try_acquire_mutex() {
            MutexAttempt::Acquired(mutex) => run_as_primary(mutex, paths),
            MutexAttempt::AlreadyExists => handle_existing_instance(paths),
            MutexAttempt::Failed => StartupAction::Run(None),
        }
    }

    fn run_as_primary(mutex: HANDLE, paths: InstancePaths) -> StartupAction {
        let _ = fs::remove_file(&paths.command_path);
        let _ = write_state(&paths.state_path);

        StartupAction::Run(Some(SingleInstance {
            mutex,
            state_path: paths.state_path,
            command_path: paths.command_path,
            last_command_modified: None,
        }))
    }

    fn handle_existing_instance(paths: InstancePaths) -> StartupAction {
        let state = read_state(&paths.state_path);
        let running_version = state
            .as_ref()
            .map(|state| state.version.as_str())
            .unwrap_or_default();

        if !running_version.is_empty()
            && compare_versions(CURRENT_VERSION, running_version) == Ordering::Greater
        {
            let _ = write_command(&paths.command_path, InstanceCommand::Exit);

            if let Some(mutex) = wait_for_primary_to_exit(Duration::from_secs(6)) {
                return run_as_primary(mutex, paths);
            }

            if let Some(pid) = state.and_then(|state| state.pid)
                && terminate_existing_process(pid)
                && let Some(mutex) = wait_for_primary_to_exit(Duration::from_secs(3))
            {
                return run_as_primary(mutex, paths);
            }
        }

        let _ = write_command(&paths.command_path, InstanceCommand::Show);
        show_existing_window();
        StartupAction::Exit
    }

    fn wait_for_primary_to_exit(timeout: Duration) -> Option<HANDLE> {
        let started = std::time::Instant::now();

        while started.elapsed() < timeout {
            match try_acquire_mutex() {
                MutexAttempt::Acquired(mutex) => return Some(mutex),
                MutexAttempt::AlreadyExists => thread::sleep(Duration::from_millis(100)),
                MutexAttempt::Failed => return None,
            }
        }

        None
    }

    enum MutexAttempt {
        Acquired(HANDLE),
        AlreadyExists,
        Failed,
    }

    fn try_acquire_mutex() -> MutexAttempt {
        let name = wide_null(MUTEX_NAME);

        unsafe {
            SetLastError(0);
            let mutex = CreateMutexW(ptr::null(), 1, name.as_ptr());

            if mutex.is_null() {
                return MutexAttempt::Failed;
            }

            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(mutex);
                return MutexAttempt::AlreadyExists;
            }

            MutexAttempt::Acquired(mutex)
        }
    }

    fn terminate_existing_process(pid: u32) -> bool {
        if pid == std::process::id() {
            return false;
        }

        let process = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid) };

        if process.is_null() {
            return false;
        }

        let terminated = unsafe { TerminateProcess(process, 0) != 0 };

        if terminated {
            unsafe {
                WaitForSingleObject(process, 3000);
            }
        }

        unsafe {
            CloseHandle(process);
        }

        terminated
    }

    fn show_existing_window() {
        let title = wide_null(APP_TITLE);
        let window: HWND = unsafe { FindWindowW(ptr::null(), title.as_ptr()) };

        if window.is_null() {
            return;
        }

        unsafe {
            ShowWindow(window, SW_SHOW);
            ShowWindow(window, SW_RESTORE);
            SetForegroundWindow(window);
        }
    }

    #[derive(Debug)]
    struct InstancePaths {
        state_path: PathBuf,
        command_path: PathBuf,
    }

    impl InstancePaths {
        fn new() -> Result<Self, ()> {
            let project_dirs =
                ProjectDirs::from("dev", "Local Dictate", "Local Dictate").ok_or(())?;
            let directory = project_dirs.data_local_dir().join("instance");
            fs::create_dir_all(&directory).map_err(|_| ())?;

            Ok(Self {
                state_path: directory.join(STATE_FILE_NAME),
                command_path: directory.join(COMMAND_FILE_NAME),
            })
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct InstanceState {
        version: String,
        pid: Option<u32>,
    }

    fn write_state(path: &Path) -> Result<(), std::io::Error> {
        let exe = std::env::current_exe()
            .ok()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let text = format!(
            "version={CURRENT_VERSION}\npid={}\nexe={exe}\n",
            std::process::id()
        );

        fs::write(path, text)
    }

    fn read_state(path: &Path) -> Option<InstanceState> {
        let text = fs::read_to_string(path).ok()?;
        let version = read_key(&text, "version")?.to_string();
        let pid = read_key(&text, "pid").and_then(|pid| pid.parse::<u32>().ok());

        Some(InstanceState { version, pid })
    }

    fn write_command(path: &Path, command: InstanceCommand) -> Result<(), std::io::Error> {
        let command = match command {
            InstanceCommand::Show => "show",
            InstanceCommand::Exit => "exit",
        };
        let text = format!("command={command}\nversion={CURRENT_VERSION}\n");

        fs::write(path, text)
    }

    fn parse_command(text: &str) -> Option<InstanceCommand> {
        match read_key(text, "command")? {
            "show" => Some(InstanceCommand::Show),
            "exit" => Some(InstanceCommand::Exit),
            _ => None,
        }
    }

    fn read_key<'a>(text: &'a str, key: &str) -> Option<&'a str> {
        text.lines().find_map(|line| {
            let (candidate_key, value) = line.split_once('=')?;
            (candidate_key == key).then_some(value.trim())
        })
    }

    fn wide_null(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::compare_versions;
    use std::cmp::Ordering;

    #[test]
    fn compares_numeric_versions() {
        assert_eq!(compare_versions("0.1.4", "0.1.3"), Ordering::Greater);
        assert_eq!(compare_versions("0.1.3", "0.1.3"), Ordering::Equal);
        assert_eq!(compare_versions("0.1.2", "0.1.3"), Ordering::Less);
    }

    #[test]
    fn missing_patch_versions_compare_as_zero() {
        assert_eq!(compare_versions("1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.2.1", "1.2"), Ordering::Greater);
    }
}

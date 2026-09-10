use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InstallOutcome {
    pub(crate) reboot_required: bool,
}

#[derive(Clone, Copy)]
enum InstallerKind {
    Msi,
    Exe,
}

impl InstallerKind {
    fn name(self) -> &'static str {
        match self {
            Self::Msi => "MSI",
            Self::Exe => "EXE",
        }
    }

    fn outcome(self, exit_code: u32) -> Result<InstallOutcome, String> {
        match (self, exit_code) {
            (_, 0) => Ok(InstallOutcome {
                reboot_required: false,
            }),
            // Windows Installer reports both codes as successful installations.
            // /norestart prevents a requested restart; preserve the result for the UI.
            (Self::Msi, 3010 | 1641) => Ok(InstallOutcome {
                reboot_required: true,
            }),
            (Self::Msi, 1602) => Err("MSI installation was canceled (exit code 1602)".into()),
            _ => Err(format!(
                "{} installation failed (exit code {exit_code})",
                self.name()
            )),
        }
    }
}

trait InstallerProcess {
    fn wait(&self) -> Result<(), String>;
    fn exit_code(&self) -> Result<u32, String>;
}

fn wait_for_installer(
    process: &impl InstallerProcess,
    kind: InstallerKind,
) -> Result<InstallOutcome, String> {
    process.wait()?;
    kind.outcome(process.exit_code()?)
}

fn msi_parameters(path: &Path) -> Result<std::ffi::OsString, String> {
    // Windows file names cannot contain quotes or NUL. Reject them explicitly so
    // the path can never escape its argument, and retain the original Unicode.
    if path
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .any(|byte| matches!(byte, 0 | b'"'))
    {
        return Err("The installer path contains an invalid character".into());
    }
    let mut parameters = std::ffi::OsString::from("/i \"");
    parameters.push(path);
    parameters.push("\" /passive /norestart");
    Ok(parameters)
}

fn launch_error(code: u32) -> String {
    if code == 1223 {
        return "Administrator permission was canceled; installation did not start (Windows error 1223)".into();
    }
    format!(
        "Failed to launch installer with administrator permission (Windows error {code}): {}",
        std::io::Error::from_raw_os_error(code as i32)
    )
}

/// Run on a blocking worker: this returns only after Windows Installer exits.
pub(crate) fn install_msi(path: &Path) -> Result<InstallOutcome, String> {
    #[cfg(target_os = "windows")]
    {
        if !path.is_file() {
            return Err(format!("Installer file not found: {}", path.display()));
        }
        let parameters = msi_parameters(path)?;
        let process = windows::launch_elevated(&windows::msiexec_path()?, &parameters)?;
        wait_for_installer(&process, InstallerKind::Msi)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Err("MSI installation is only supported on Windows".into())
    }
}

/// The supported EXE installer (VLC/NSIS) accepts /S and reports success with 0.
pub(crate) fn install_exe_silent(path: &Path) -> Result<InstallOutcome, String> {
    #[cfg(target_os = "windows")]
    {
        if !path.is_file() {
            return Err(format!("Installer file not found: {}", path.display()));
        }
        let process = windows::launch_elevated(path.as_os_str(), std::ffi::OsStr::new("/S"))?;
        wait_for_installer(&process, InstallerKind::Exe)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Err("EXE installation is only supported on Windows".into())
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::{launch_error, InstallerProcess};
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::ptr::null;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, HANDLE, RPC_E_CHANGED_MODE, WAIT_FAILED, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
        SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    struct ComApartment(bool);

    impl ComApartment {
        fn initialize() -> Result<Self, String> {
            let result = unsafe {
                CoInitializeEx(
                    null(),
                    (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
                )
            };
            if result >= 0 {
                Ok(Self(true))
            } else if result == RPC_E_CHANGED_MODE {
                // A reused worker already has COM initialized in another mode.
                // Do not uninitialize the apartment owned by its original caller.
                Ok(Self(false))
            } else {
                Err(format!(
                    "Failed to initialize installer launcher (HRESULT 0x{:08X})",
                    result as u32
                ))
            }
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub(super) struct ProcessHandle(HANDLE);

    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    impl InstallerProcess for ProcessHandle {
        fn wait(&self) -> Result<(), String> {
            match unsafe { WaitForSingleObject(self.0, INFINITE) } {
                WAIT_OBJECT_0 => Ok(()),
                WAIT_FAILED => Err(format!(
                    "Failed to wait for installer completion: {}",
                    std::io::Error::last_os_error()
                )),
                status => Err(format!(
                    "Installer completion could not be confirmed (wait status {status})"
                )),
            }
        }

        fn exit_code(&self) -> Result<u32, String> {
            let mut exit_code = 0;
            if unsafe { GetExitCodeProcess(self.0, &mut exit_code) } == 0 {
                return Err(format!(
                    "Failed to read installer exit code: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(exit_code)
        }
    }

    fn wide(value: &OsStr) -> Result<Vec<u16>, String> {
        let mut result: Vec<u16> = value.encode_wide().collect();
        if result.contains(&0) {
            return Err("The installer command contains an invalid NUL character".into());
        }
        result.push(0);
        Ok(result)
    }

    pub(super) fn msiexec_path() -> Result<OsString, String> {
        let mut buffer = vec![0u16; 260];
        loop {
            let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
            if length == 0 {
                return Err(format!(
                    "Failed to locate Windows Installer: {}",
                    std::io::Error::last_os_error()
                ));
            }
            if (length as usize) < buffer.len() {
                let mut path = OsString::from_wide(&buffer[..length as usize]);
                path.push("\\msiexec.exe");
                return Ok(path);
            }
            buffer.resize(length as usize + 1, 0);
        }
    }

    pub(super) fn launch_elevated(
        file: &OsStr,
        parameters: &OsStr,
    ) -> Result<ProcessHandle, String> {
        let _com = ComApartment::initialize()?;
        let operation = wide(OsStr::new("runas"))?;
        let file = wide(file)?;
        let parameters = wide(parameters)?;
        let mut execute = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            // NOASYNC is required on the blocking worker, which has no message loop.
            // NO_UI suppresses duplicate error dialogs but preserves the UAC prompt.
            fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: operation.as_ptr(),
            lpFile: file.as_ptr(),
            lpParameters: parameters.as_ptr(),
            nShow: SW_SHOWNORMAL,
            ..Default::default()
        };
        if unsafe { ShellExecuteExW(&mut execute) } == 0 {
            return Err(launch_error(unsafe { GetLastError() }));
        }
        if execute.hProcess.is_null() {
            return Err(
                "Installer started without a process handle; completion could not be confirmed"
                    .into(),
            );
        }
        // The COM guard is released before the caller waits. Only the process
        // handle remains alive until completion (or an error), and Drop closes it.
        Ok(ProcessHandle(execute.hProcess))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::installer_process::{wait_for_installer, InstallerKind};
        use std::io::Write;
        use std::os::windows::io::AsRawHandle;
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        use std::sync::mpsc;
        use std::time::Duration;
        use windows_sys::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, CREATE_NO_WINDOW};

        #[test]
        fn native_process_waits_for_child_exit_and_reads_its_result() {
            let system_installer = std::path::PathBuf::from(msiexec_path().unwrap());
            let command = system_installer.parent().unwrap().join("cmd.exe");
            // /D disables any user-configured AutoRun commands. This isolated,
            // hidden child only reads our pipe and executes the supplied exit.
            let mut child = Command::new(command)
                .args(["/D", "/Q"])
                .creation_flags(CREATE_NO_WINDOW)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            let (waiting_tx, waiting_rx) = mpsc::channel();
            let (result_tx, result_rx) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let mut handle = std::ptr::null_mut();
                let duplicated = unsafe {
                    DuplicateHandle(
                        GetCurrentProcess(),
                        child.as_raw_handle(),
                        GetCurrentProcess(),
                        &mut handle,
                        0,
                        0,
                        DUPLICATE_SAME_ACCESS,
                    )
                };
                assert_ne!(duplicated, 0, "{}", std::io::Error::last_os_error());
                let process = ProcessHandle(handle);
                waiting_tx.send(()).unwrap();
                let outcome = wait_for_installer(&process, InstallerKind::Msi);
                assert_eq!(child.wait().unwrap().code(), Some(3010));
                result_tx.send(outcome).unwrap();
            });
            waiting_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(result_rx.try_recv(), Err(mpsc::TryRecvError::Empty));
            input.write_all(b"exit 3010\r\n").unwrap();
            // Closing the pipe also lets the child exit if a future assertion fails.
            drop(input);
            assert!(
                result_rx
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap()
                    .unwrap()
                    .reboot_required
            );
            worker.join().unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::mpsc;

    #[test]
    fn msi_success_codes_preserve_restart_requirement() {
        assert_eq!(
            InstallerKind::Msi.outcome(0).unwrap(),
            InstallOutcome {
                reboot_required: false
            }
        );
        for code in [3010, 1641] {
            assert!(InstallerKind::Msi.outcome(code).unwrap().reboot_required);
        }
    }

    #[test]
    fn installer_failures_and_cancellation_keep_exit_codes() {
        assert!(InstallerKind::Msi
            .outcome(1602)
            .unwrap_err()
            .contains("canceled"));
        for code in [1602, 1603, 1618, 259, 1] {
            assert!(InstallerKind::Msi
                .outcome(code)
                .unwrap_err()
                .contains(&format!("exit code {code}")));
        }
        assert!(!InstallerKind::Exe.outcome(0).unwrap().reboot_required);
        for code in [1, 2, 1602, 1641, 3010] {
            assert!(InstallerKind::Exe
                .outcome(code)
                .unwrap_err()
                .contains(&format!("exit code {code}")));
        }
        assert!(launch_error(1223).contains("canceled"));
        assert!(launch_error(1223).contains("1223"));
        assert!(launch_error(5).contains("Windows error 5"));
    }

    #[test]
    fn msi_arguments_preserve_spaces_unicode_and_prevent_restart() {
        let path = Path::new(r"C:\Users\測試使用者\App Data\arduino installer.msi");
        assert_eq!(
            msi_parameters(path).unwrap(),
            std::ffi::OsString::from(
                "/i \"C:\\Users\\測試使用者\\App Data\\arduino installer.msi\" /passive /norestart"
            )
        );
        assert!(msi_parameters(Path::new("bad\"path.msi")).is_err());
        assert!(msi_parameters(Path::new("bad\0path.msi")).is_err());
    }

    struct FakeProcess {
        wait_result: Result<(), String>,
        exit_result: Result<u32, String>,
        exit_read: Cell<bool>,
    }

    impl InstallerProcess for FakeProcess {
        fn wait(&self) -> Result<(), String> {
            self.wait_result.clone()
        }

        fn exit_code(&self) -> Result<u32, String> {
            self.exit_read.set(true);
            self.exit_result.clone()
        }
    }

    #[test]
    fn wait_and_exit_read_errors_never_report_installation_success() {
        let mut process = FakeProcess {
            wait_result: Err("wait failed".into()),
            exit_result: Ok(0),
            exit_read: Cell::new(false),
        };
        assert_eq!(
            wait_for_installer(&process, InstallerKind::Msi),
            Err("wait failed".into())
        );
        assert!(!process.exit_read.get());

        process.wait_result = Ok(());
        process.exit_result = Err("exit code unavailable".into());
        assert_eq!(
            wait_for_installer(&process, InstallerKind::Msi),
            Err("exit code unavailable".into())
        );
        assert!(process.exit_read.get());
    }

    #[test]
    fn completion_is_not_reported_until_installer_exits() {
        struct BlockingProcess {
            waiting: mpsc::Sender<()>,
            completed: mpsc::Receiver<()>,
            has_exited: Cell<bool>,
        }
        impl InstallerProcess for BlockingProcess {
            fn wait(&self) -> Result<(), String> {
                self.waiting.send(()).unwrap();
                self.completed.recv().unwrap();
                self.has_exited.set(true);
                Ok(())
            }

            fn exit_code(&self) -> Result<u32, String> {
                assert!(self.has_exited.get());
                Ok(0)
            }
        }

        let (waiting_tx, waiting_rx) = mpsc::channel();
        let (completed_tx, completed_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let process = BlockingProcess {
                waiting: waiting_tx,
                completed: completed_rx,
                has_exited: Cell::new(false),
            };
            result_tx
                .send(wait_for_installer(&process, InstallerKind::Msi))
                .unwrap();
        });
        waiting_rx.recv().unwrap();
        assert_eq!(result_rx.try_recv(), Err(mpsc::TryRecvError::Empty));
        completed_tx.send(()).unwrap();
        assert_eq!(
            result_rx.recv().unwrap(),
            Ok(InstallOutcome {
                reboot_required: false
            })
        );
        worker.join().unwrap();
    }
}

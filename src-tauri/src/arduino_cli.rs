use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ArduinoCliPathState {
    NotInstalled,
    NotOnPath,
    OnPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ArduinoCliStatus {
    pub status: ArduinoCliPathState,
    pub cli_path: Option<String>,
}

pub(crate) fn get_status() -> Result<ArduinoCliStatus, String> {
    #[cfg(windows)]
    return windows_environment::get_status();
    #[cfg(not(windows))]
    Ok(ArduinoCliStatus {
        status: ArduinoCliPathState::NotInstalled,
        cli_path: None,
    })
}

pub(crate) fn add_to_user_path() -> Result<ArduinoCliStatus, String> {
    #[cfg(windows)]
    return windows_environment::add_to_user_path();
    #[cfg(not(windows))]
    Err("Adding Arduino CLI to the user PATH is supported only on Windows.".into())
}

#[cfg(windows)]
mod windows_environment {
    use super::{ArduinoCliPathState, ArduinoCliStatus};
    use std::{
        path::{Path, PathBuf},
        ptr::{null, null_mut},
        sync::Mutex,
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS},
        System::{
            Environment::ExpandEnvironmentStringsW,
            Registry::{
                RegCloseKey, RegCreateKeyExW, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW,
                RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE,
                KEY_READ, KEY_SET_VALUE, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_EXPAND_SZ,
                REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ,
            },
        },
        UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_SETTINGCHANGE,
        },
    };

    const CLI_RELATIVE_PATH: &str = "resources/app/lib/backend/resources/arduino-cli.exe";
    const USER_ENVIRONMENT_KEY: &str = "Environment";
    const MACHINE_ENVIRONMENT_KEY: &str =
        "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment";
    const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
    const APP_PATH_KEY: &str =
        "Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\Arduino IDE.exe";
    const MAX_REGISTRY_BYTES: u32 = 1024 * 1024;
    const MAX_ENVIRONMENT_VALUE_UNITS: usize = 32_766;
    static PATH_OPERATION: Mutex<()> = Mutex::new(());

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct RegistryString {
        text: String,
        kind: u32,
    }

    struct RegistryKey(HKEY);

    impl Drop for RegistryKey {
        fn drop(&mut self) {
            // Only owned handles returned by RegOpenKeyExW/RegCreateKeyExW reach this type.
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    fn registry_error(operation: &str, code: u32) -> String {
        format!(
            "{operation}: {} (Windows error {code})",
            std::io::Error::from_raw_os_error(code as i32)
        )
    }

    fn open_key(
        root: HKEY,
        path: &str,
        access: REG_SAM_FLAGS,
    ) -> Result<Option<RegistryKey>, String> {
        let path = wide(path);
        let mut handle = null_mut();
        let result = unsafe { RegOpenKeyExW(root, path.as_ptr(), 0, access, &mut handle) };
        match result {
            ERROR_SUCCESS => Ok(Some(RegistryKey(handle))),
            ERROR_FILE_NOT_FOUND => Ok(None),
            code => Err(registry_error("Cannot open Windows registry key", code)),
        }
    }

    fn open_user_environment_for_write() -> Result<RegistryKey, String> {
        if let Some(key) = open_key(
            HKEY_CURRENT_USER,
            USER_ENVIRONMENT_KEY,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
        )? {
            return Ok(key);
        }
        let path = wide(USER_ENVIRONMENT_KEY);
        let mut handle = null_mut();
        let result = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                null(),
                &mut handle,
                null_mut(),
            )
        };
        if result != ERROR_SUCCESS {
            return Err(registry_error(
                "Cannot create the user environment registry key",
                result,
            ));
        }
        Ok(RegistryKey(handle))
    }

    fn decode_registry_string(data: &[u8], kind: u32) -> Result<RegistryString, String> {
        if kind != REG_SZ && kind != REG_EXPAND_SZ {
            return Err("Registry value has an unsupported type; it was not changed.".into());
        }
        if !data.len().is_multiple_of(2) {
            return Err("Registry value has invalid UTF-16 data; it was not changed.".into());
        }
        let mut units: Vec<u16> = data
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        while units.last() == Some(&0) {
            units.pop();
        }
        if units.contains(&0) {
            return Err("Registry value contains an embedded NUL; it was not changed.".into());
        }
        let text = String::from_utf16(&units)
            .map_err(|_| "Registry value has invalid UTF-16 data; it was not changed.")?;
        Ok(RegistryString { text, kind })
    }

    fn read_string(key: &RegistryKey, name: &str) -> Result<Option<RegistryString>, String> {
        let name = wide(name);
        // Another application can resize a value between the size query and the read.
        for _ in 0..4 {
            let mut size = 0;
            let mut kind = 0;
            let result = unsafe {
                RegQueryValueExW(
                    key.0,
                    name.as_ptr(),
                    null(),
                    &mut kind,
                    null_mut(),
                    &mut size,
                )
            };
            if result == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if result != ERROR_SUCCESS {
                return Err(registry_error("Cannot read Windows registry value", result));
            }
            if size > MAX_REGISTRY_BYTES {
                return Err(
                    "Registry value exceeds the supported size; it was not changed.".into(),
                );
            }
            let mut bytes = vec![0u8; size.max(2) as usize];
            let mut received = bytes.len() as u32;
            let result = unsafe {
                RegQueryValueExW(
                    key.0,
                    name.as_ptr(),
                    null(),
                    &mut kind,
                    bytes.as_mut_ptr(),
                    &mut received,
                )
            };
            if result == ERROR_MORE_DATA {
                continue;
            }
            if result == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if result != ERROR_SUCCESS {
                return Err(registry_error("Cannot read Windows registry value", result));
            }
            bytes.truncate(received as usize);
            return decode_registry_string(&bytes, kind).map(Some);
        }
        Err("Windows registry value kept changing. Please try again.".into())
    }

    fn read_path(root: HKEY, key_path: &str) -> Result<Option<RegistryString>, String> {
        match open_key(root, key_path, KEY_QUERY_VALUE)? {
            Some(key) => read_string(&key, "Path"),
            None => Ok(None),
        }
    }

    fn expand_environment(value: &str) -> Result<String, String> {
        let input = wide(value);
        for _ in 0..3 {
            let required = unsafe { ExpandEnvironmentStringsW(input.as_ptr(), null_mut(), 0) };
            if required == 0 || required > MAX_REGISTRY_BYTES / 2 {
                return Err("Cannot expand a Windows environment variable.".into());
            }
            let mut output = vec![0u16; required as usize];
            let written =
                unsafe { ExpandEnvironmentStringsW(input.as_ptr(), output.as_mut_ptr(), required) };
            if written > required {
                continue;
            }
            if written == 0 {
                return Err("Cannot expand a Windows environment variable.".into());
            }
            return String::from_utf16(&output[..written as usize - 1])
                .map_err(|_| "Expanded environment variable contains invalid UTF-16.".into());
        }
        Err("Windows environment variables kept changing. Please try again.".into())
    }

    fn normalize_directory(value: &str) -> String {
        value
            .trim()
            .trim_matches('"')
            .trim()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    }

    fn path_contains_with(
        raw: Option<&RegistryString>,
        directory: &str,
        expand: impl Fn(&str) -> Result<String, String>,
    ) -> Result<bool, String> {
        let Some(raw) = raw else {
            return Ok(false);
        };
        let target = normalize_directory(directory);
        let value = if raw.kind == REG_EXPAND_SZ {
            expand(&raw.text)?
        } else {
            raw.text.clone()
        };
        Ok(value
            .split(';')
            .any(|entry| !entry.trim().is_empty() && normalize_directory(entry) == target))
    }

    fn on_persisted_path(
        user: Option<&RegistryString>,
        machine: Option<&RegistryString>,
        directory: &str,
    ) -> Result<bool, String> {
        Ok(path_contains_with(user, directory, expand_environment)?
            || path_contains_with(machine, directory, expand_environment)?)
    }

    fn append_directory(
        existing: Option<&RegistryString>,
        directory: &str,
    ) -> Result<RegistryString, String> {
        if directory.is_empty()
            || directory.contains([';', '"', '\0', '\r', '\n', '%'])
            || !Path::new(directory).is_absolute()
        {
            return Err("The Arduino CLI directory cannot be safely added to PATH.".into());
        }
        let mut value = existing.cloned().unwrap_or(RegistryString {
            text: String::new(),
            kind: REG_EXPAND_SZ,
        });
        if !value.text.is_empty() && !value.text.ends_with(';') {
            value.text.push(';');
        }
        value.text.push_str(directory);
        if value.text.encode_utf16().count() > MAX_ENVIRONMENT_VALUE_UNITS {
            return Err("The user PATH would exceed the Windows limit; it was not changed.".into());
        }
        Ok(value)
    }

    fn installed_cli_in(root: &Path, is_file: impl Fn(&Path) -> bool) -> Option<PathBuf> {
        let cli = root.join(CLI_RELATIVE_PATH);
        (root.is_absolute() && is_file(&root.join("Arduino IDE.exe")) && is_file(&cli))
            .then_some(cli)
    }

    fn is_arduino_ide_name(name: &str) -> bool {
        let name = name.trim().to_ascii_lowercase();
        name == "arduino ide"
            || name
                .strip_prefix("arduino ide ")
                .and_then(|version| version.chars().next())
                .is_some_and(|first| first.is_ascii_digit())
    }

    fn path_from_registry_command(value: &str) -> Option<PathBuf> {
        let value = value.trim();
        let path = if let Some(quoted) = value.strip_prefix('"') {
            quoted.split_once('"')?.0
        } else {
            let end = value.to_ascii_lowercase().find(".exe")? + 4;
            &value[..end]
        };
        let path = PathBuf::from(path);
        path.is_absolute().then_some(path)
    }

    fn registry_text(key: &RegistryKey, name: &str) -> Result<Option<String>, String> {
        read_string(key, name)?
            .map(|value| {
                if value.kind == REG_EXPAND_SZ {
                    expand_environment(&value.text)
                } else {
                    Ok(value.text)
                }
            })
            .transpose()
    }

    fn registry_install_roots() -> Result<Vec<PathBuf>, String> {
        let mut roots = Vec::new();
        for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
                if let Some(key) = open_key(hive, APP_PATH_KEY, KEY_QUERY_VALUE | view)? {
                    if let Some(executable) = registry_text(&key, "")?
                        .and_then(|value| path_from_registry_command(&value))
                    {
                        if let Some(parent) = executable.parent() {
                            roots.push(parent.to_path_buf());
                        }
                    }
                }
                let Some(uninstall) = open_key(hive, UNINSTALL_KEY, KEY_READ | view)? else {
                    continue;
                };
                let mut index = 0;
                loop {
                    // Registry key names have a documented maximum length of 255 characters.
                    let mut name = [0u16; 256];
                    let mut length = name.len() as u32;
                    let result = unsafe {
                        RegEnumKeyExW(
                            uninstall.0,
                            index,
                            name.as_mut_ptr(),
                            &mut length,
                            null(),
                            null_mut(),
                            null_mut(),
                            null_mut(),
                        )
                    };
                    if result == ERROR_NO_MORE_ITEMS {
                        break;
                    }
                    if result != ERROR_SUCCESS {
                        return Err(registry_error(
                            "Cannot enumerate installed applications",
                            result,
                        ));
                    }
                    index += 1;
                    let name = String::from_utf16(&name[..length as usize])
                        .map_err(|_| "An installed application registry key is invalid UTF-16.")?;
                    let Ok(Some(entry)) = open_key(uninstall.0, &name, KEY_QUERY_VALUE | view)
                    else {
                        // Unrelated applications may restrict access to their uninstall entry.
                        continue;
                    };
                    let Ok(Some(display_name)) = registry_text(&entry, "DisplayName") else {
                        continue;
                    };
                    if !is_arduino_ide_name(&display_name) {
                        continue;
                    }
                    if let Some(location) = registry_text(&entry, "InstallLocation")? {
                        roots.push(PathBuf::from(location.trim().trim_matches('"')));
                    }
                    for value_name in ["DisplayIcon", "UninstallString"] {
                        if let Some(executable) = registry_text(&entry, value_name)?
                            .and_then(|value| path_from_registry_command(&value))
                        {
                            if let Some(parent) = executable.parent() {
                                roots.push(parent.to_path_buf());
                            }
                        }
                    }
                }
            }
        }
        Ok(roots)
    }

    fn installed_clis() -> Result<Vec<PathBuf>, String> {
        let mut roots = Vec::new();
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(local_app_data).join("Programs/Arduino IDE"));
        }
        for variable in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(program_files) = std::env::var_os(variable) {
                roots.push(PathBuf::from(program_files).join("Arduino IDE"));
            }
        }
        let mut clis: Vec<PathBuf> = roots
            .iter()
            .filter_map(|root| installed_cli_in(root, Path::is_file))
            .collect();
        match registry_install_roots() {
            Ok(registry_roots) => clis.extend(
                registry_roots
                    .iter()
                    .filter_map(|root| installed_cli_in(root, Path::is_file)),
            ),
            Err(error) if !clis.is_empty() => {
                eprintln!("Arduino CLI: using a validated standard installation; {error}");
            }
            Err(error) => return Err(error),
        }
        let mut seen = std::collections::HashSet::new();
        clis.retain(|cli| seen.insert(normalize_directory(&cli.to_string_lossy())));
        Ok(clis)
    }

    fn status_from_installations(
        clis: &[PathBuf],
        user: Option<&RegistryString>,
        machine: Option<&RegistryString>,
    ) -> Result<ArduinoCliStatus, String> {
        let Some(first) = clis.first() else {
            return Ok(ArduinoCliStatus {
                status: ArduinoCliPathState::NotInstalled,
                cli_path: None,
            });
        };
        for cli in clis {
            let directory = cli
                .parent()
                .ok_or("Cannot locate the Arduino CLI directory.")?;
            if on_persisted_path(user, machine, &directory.to_string_lossy())? {
                return Ok(ArduinoCliStatus {
                    status: ArduinoCliPathState::OnPath,
                    cli_path: Some(cli.to_string_lossy().into_owned()),
                });
            }
        }
        Ok(ArduinoCliStatus {
            status: ArduinoCliPathState::NotOnPath,
            cli_path: Some(first.to_string_lossy().into_owned()),
        })
    }

    fn notify_environment_change() {
        let environment = wide("Environment");
        let mut result = 0;
        // A stale terminal still needs restarting. A hung listener must not block setup forever.
        let sent = unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                environment.as_ptr() as isize,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                100,
                &mut result,
            )
        };
        if sent == 0 {
            // PATH is already persisted. A broadcast timeout must not report the write as failed.
            eprintln!("Arduino CLI PATH was saved; some Windows applications did not acknowledge the environment change.");
        }
    }

    pub(super) fn get_status() -> Result<ArduinoCliStatus, String> {
        let _guard = PATH_OPERATION
            .lock()
            .map_err(|_| "Arduino CLI PATH operation is unavailable. Please restart the Plugin.")?;
        let clis = installed_clis()?;
        if clis.is_empty() {
            return status_from_installations(&clis, None, None);
        }
        let user = read_path(HKEY_CURRENT_USER, USER_ENVIRONMENT_KEY)?;
        let machine = read_path(HKEY_LOCAL_MACHINE, MACHINE_ENVIRONMENT_KEY)?;
        status_from_installations(&clis, user.as_ref(), machine.as_ref())
    }

    pub(super) fn add_to_user_path() -> Result<ArduinoCliStatus, String> {
        let _guard = PATH_OPERATION
            .lock()
            .map_err(|_| "Arduino CLI PATH operation is unavailable. Please restart the Plugin.")?;
        let clis = installed_clis()?;
        if clis.is_empty() {
            return Err("Arduino IDE with its bundled Arduino CLI is not installed.".into());
        }
        let user = read_path(HKEY_CURRENT_USER, USER_ENVIRONMENT_KEY)?;
        let machine = read_path(HKEY_LOCAL_MACHINE, MACHINE_ENVIRONMENT_KEY)?;
        let current = status_from_installations(&clis, user.as_ref(), machine.as_ref())?;
        if current.status == ArduinoCliPathState::OnPath {
            return Ok(current);
        }
        let cli = &clis[0];
        let directory = cli
            .parent()
            .ok_or("Cannot locate the Arduino CLI directory.")?
            .to_str()
            .ok_or("The Arduino CLI directory contains invalid UTF-16.")?;
        let user_key = open_user_environment_for_write()?;
        for _ in 0..3 {
            // Always append to the latest persisted user value, never to the inherited process PATH.
            let latest_user = read_string(&user_key, "Path")?;
            let latest_machine = read_path(HKEY_LOCAL_MACHINE, MACHINE_ENVIRONMENT_KEY)?;
            let status =
                status_from_installations(&clis, latest_user.as_ref(), latest_machine.as_ref())?;
            if status.status == ArduinoCliPathState::OnPath {
                return Ok(status);
            }
            let updated = append_directory(latest_user.as_ref(), directory)?;
            // Avoid overwriting a concurrent edit observed while preparing this update.
            if read_string(&user_key, "Path")? != latest_user {
                continue;
            }
            if !cli.is_file() {
                return Err(
                    "Arduino CLI is no longer installed. Please install Arduino IDE again.".into(),
                );
            }
            let name = wide("Path");
            let data = wide(&updated.text);
            let result = unsafe {
                RegSetValueExW(
                    user_key.0,
                    name.as_ptr(),
                    0,
                    updated.kind,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            };
            if result != ERROR_SUCCESS {
                return Err(registry_error("Cannot update the user PATH", result));
            }
            let saved = read_string(&user_key, "Path")?;
            if !on_persisted_path(saved.as_ref(), latest_machine.as_ref(), directory)? {
                return Err(
                    "Arduino CLI PATH could not be verified after saving. Please try again.".into(),
                );
            }
            notify_environment_change();
            return status_from_installations(&clis, saved.as_ref(), latest_machine.as_ref());
        }
        Err("The user PATH was changed by another application. Please try again.".into())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn raw(text: &str, kind: u32) -> RegistryString {
            RegistryString {
                text: text.into(),
                kind,
            }
        }

        #[test]
        fn installation_requires_both_ide_and_bundled_cli_files() {
            let root = Path::new("C:/Fixture/Arduino IDE");
            let ide = root.join("Arduino IDE.exe");
            let cli = root.join(CLI_RELATIVE_PATH);
            assert!(installed_cli_in(root, |_| false).is_none());
            assert!(installed_cli_in(root, |file| file == ide).is_none());
            assert!(installed_cli_in(root, |file| file == cli).is_none());
            assert_eq!(
                installed_cli_in(root, |file| file == ide || file == cli),
                Some(cli)
            );
            assert!(installed_cli_in(Path::new("relative/Arduino IDE"), |_| true).is_none());
        }

        #[test]
        fn custom_install_registry_metadata_is_restricted_to_arduino_ide() {
            assert!(is_arduino_ide_name("Arduino IDE"));
            assert!(is_arduino_ide_name("Arduino IDE 2.3.8"));
            assert!(!is_arduino_ide_name("Arduino CLI"));
            assert!(!is_arduino_ide_name("Arduino IDE Helper"));
            assert_eq!(
                path_from_registry_command("\"D:\\Custom Apps\\Arduino IDE\\Arduino IDE.exe\",0"),
                Some(PathBuf::from("D:/Custom Apps/Arduino IDE/Arduino IDE.exe"))
            );
            assert_eq!(
                path_from_registry_command(
                    "D:\\Custom Apps\\Arduino IDE\\Uninstall Arduino IDE.exe /S"
                ),
                Some(PathBuf::from(
                    "D:/Custom Apps/Arduino IDE/Uninstall Arduino IDE.exe"
                ))
            );
            assert!(path_from_registry_command("Arduino IDE.exe").is_none());
        }

        #[test]
        fn matches_case_quotes_slashes_and_trailing_separators_without_prefix_matches() {
            let path = raw("C:\\Other;  \"C:/Arduino CLI/\" ;C:\\Last", REG_SZ);
            assert!(path_contains_with(Some(&path), "c:\\arduino cli", |s| Ok(s.into())).unwrap());
            assert!(!path_contains_with(Some(&path), "c:\\arduino", |s| Ok(s.into())).unwrap());
            assert!(!path_contains_with(None, "C:\\Arduino CLI", |s| Ok(s.into())).unwrap());
        }

        #[test]
        fn expands_only_for_comparison_and_keeps_unknown_variables_intact() {
            let path = raw("%KNOWN%\\CLI;%UNKNOWN%\\bin", REG_EXPAND_SZ);
            assert!(
                path_contains_with(Some(&path), "C:\\Installed\\CLI", |value| {
                    Ok(value.replace("%KNOWN%", "C:\\Installed"))
                })
                .unwrap()
            );
            assert_eq!(path.text, "%KNOWN%\\CLI;%UNKNOWN%\\bin");
            let literal = raw("%KNOWN%\\CLI", REG_SZ);
            assert!(
                !path_contains_with(Some(&literal), "C:\\Installed\\CLI", |_| {
                    panic!("REG_SZ must remain literal")
                })
                .unwrap()
            );
        }

        #[test]
        fn all_three_states_use_persisted_user_or_machine_paths() {
            let cli = PathBuf::from("C:/Arduino IDE/resources/arduino-cli.exe");
            assert_eq!(
                status_from_installations(&[], None, None).unwrap(),
                ArduinoCliStatus {
                    status: ArduinoCliPathState::NotInstalled,
                    cli_path: None,
                }
            );
            assert_eq!(
                status_from_installations(std::slice::from_ref(&cli), None, None)
                    .unwrap()
                    .status,
                ArduinoCliPathState::NotOnPath
            );
            let path = raw("C:\\Arduino IDE\\resources\\", REG_SZ);
            for (user, machine) in [(Some(&path), None), (None, Some(&path))] {
                assert_eq!(
                    status_from_installations(std::slice::from_ref(&cli), user, machine)
                        .unwrap()
                        .status,
                    ArduinoCliPathState::OnPath
                );
            }
        }

        #[test]
        fn already_configured_secondary_installation_is_selected() {
            let clis = [
                PathBuf::from("C:/Arduino IDE/resources/arduino-cli.exe"),
                PathBuf::from("D:/Arduino IDE/resources/arduino-cli.exe"),
            ];
            let path = raw("D:\\Arduino IDE\\resources", REG_SZ);
            let status = status_from_installations(&clis, Some(&path), None).unwrap();
            assert_eq!(status.status, ArduinoCliPathState::OnPath);
            assert_eq!(status.cli_path.as_deref(), clis[1].to_str());
        }

        #[test]
        fn append_preserves_exact_existing_text_and_registry_type() {
            for kind in [REG_SZ, REG_EXPAND_SZ] {
                let existing = raw("  %UNKNOWN%\\工具 ;\"C:\\Other Path\";;", kind);
                let result = append_directory(Some(&existing), "D:\\Arduino IDE\\CLI").unwrap();
                assert_eq!(result.kind, kind);
                assert_eq!(
                    result.text,
                    "  %UNKNOWN%\\工具 ;\"C:\\Other Path\";;D:\\Arduino IDE\\CLI"
                );
            }
            assert_eq!(
                append_directory(Some(&raw("C:\\Tools", REG_SZ)), "D:\\CLI")
                    .unwrap()
                    .text,
                "C:\\Tools;D:\\CLI"
            );
            assert_eq!(append_directory(None, "D:\\CLI").unwrap().text, "D:\\CLI");
        }

        #[test]
        fn long_paths_are_preserved_without_setx_truncation() {
            let existing = raw(&format!("C:\\{}", "x".repeat(5000)), REG_EXPAND_SZ);
            let result = append_directory(Some(&existing), "D:\\CLI").unwrap();
            assert!(result.text.starts_with(&existing.text));
            assert!(result.text.ends_with(";D:\\CLI"));
            let too_long = raw(&"x".repeat(MAX_ENVIRONMENT_VALUE_UNITS), REG_SZ);
            assert!(append_directory(Some(&too_long), "D:\\CLI").is_err());
        }

        #[test]
        fn unsafe_path_entries_and_malformed_registry_values_are_rejected() {
            for directory in ["", "relative", "C:\\a;b", "C:\\a\"b", "C:\\%a%", "C:\\a\nb"] {
                assert!(append_directory(None, directory).is_err(), "{directory}");
            }
            assert!(decode_registry_string(b"A", REG_SZ).is_err());
            assert!(decode_registry_string(&[0, 0xD8], REG_SZ).is_err());
            assert!(decode_registry_string(&[b'A', 0, 0, 0, b'B', 0], REG_SZ).is_err());
            assert!(decode_registry_string(&[], 4).is_err());
            let without_terminator = [b'C', 0, b':', 0, b'\\', 0];
            assert_eq!(
                decode_registry_string(&without_terminator, REG_SZ).unwrap(),
                raw("C:\\", REG_SZ)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_status_serializes_the_frontend_contract() {
        for (status, expected) in [
            (ArduinoCliPathState::NotInstalled, "not_installed"),
            (ArduinoCliPathState::NotOnPath, "not_on_path"),
            (ArduinoCliPathState::OnPath, "on_path"),
        ] {
            let value = serde_json::to_value(ArduinoCliStatus {
                status,
                cli_path: None,
            })
            .unwrap();
            assert_eq!(value["status"], expected);
            assert!(value["cli_path"].is_null());
        }
    }
}

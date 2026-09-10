mod annotation_orientation;
mod arduino_cli;
mod image_conversion;
mod image_safety;
mod installer_process;

use annotation_orientation::{
    normalize_annotation_orientations, AnnotationOrientationProgress, AnnotationOrientationSummary,
};
use arduino_cli::ArduinoCliStatus;
use image_conversion::{
    convert_images_in_folder, ImageConversionProgress as CoreImageConversionProgress,
    ImageConversionSummary as CoreImageConversionSummary,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, MutexGuard, TryLockError},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{ipc::Channel, AppHandle, Emitter, Manager};
use thiserror::Error;

const AUTHOR: &str = "breeze0305";
const CONTACT: &str = "breeze0305";
const DEFAULT_LANGUAGE: &str = "zh_TW";
const DEFAULT_UVCD_FORMAT: &str = "MJPG";
const DEFAULT_PREFERENCE_VERSION: &str = "beta";
const NATIVE_DIALOG_STATE_EVENT: &str = "native-dialog-state";
const ARDUINO_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/arduino/arduino-ide/releases/latest";
const INSTALLER_CACHE_DIRECTORY: &str = "installer-cache/v1";
const CACHE_METADATA_MAX_BYTES: u64 = 64 * 1024;
const GITHUB_METADATA_MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_MODEL_CONVERTER_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RECOVERED_ANNOTATION_CLASSES: usize = 10_000;
const SUPPORTED_UVCD_FORMATS: &[&str] = &["YUY2", "NV12", "MJPG", "H264", "H265"];
const SUPPORTED_PREFERENCE_VERSIONS: &[&str] = &["release", "beta"];
const INSTALLED_WEIGHT_RELATIVE_PATHS: [&str; 2] = [
    "libraries/NeuralNetwork/examples/RTSPImageClassification/img_class_cnn.nb",
    "libraries/NeuralNetwork/examples/ObjectDetectionLoop/yolov7_tiny.nb",
];
const ALLOWED_EXTERNAL_URL_HOSTS: &[&str] = &[
    "github.com",
    "raw.githubusercontent.com",
    "downloads.arduino.cc",
    "get.videolan.org",
    "mirror.twds.com.tw",
    "modelconverter.ntnu-aiot.com",
    "www.youtube.com",
];
const ENDPOINT_MANIFEST_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/endpoint_manifest.json"
));
const VERSION_TEXT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../version.txt"));

#[derive(Debug, Error)]
enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    capture_interval: u64,
    language: String,
    uvcd_format: String,
    preference_version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capture_interval: 1,
            language: DEFAULT_LANGUAGE.to_string(),
            uvcd_format: DEFAULT_UVCD_FORMAT.to_string(),
            preference_version: DEFAULT_PREFERENCE_VERSION.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct Dashboard {
    metadata: Metadata,
    settings: Settings,
    realtek_folder: Option<String>,
    output_folder: String,
    internet_connected: bool,
}

#[derive(Clone, Debug, Serialize)]
struct Metadata {
    author: &'static str,
    contact: &'static str,
    version: &'static str,
    repository: String,
    arduino_ide_url: String,
    vlc_url: String,
    realtek_package_url: String,
    model_converter_url: String,
    model_converter_api_base: String,
    supported_languages: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
struct ActionResult {
    ok: bool,
    message: String,
    path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct InstalledWeightCleanupResult {
    deleted: usize,
    missing: usize,
    folder: String,
}

#[derive(Clone, Debug, Serialize)]
struct DownloadResult {
    file_name: String,
    path: String,
    bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
struct DownloadProgress {
    key: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
struct InstallationResult {
    path: String,
    reboot_required: bool,
    arduino_cli: Option<ArduinoCliStatus>,
    path_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct InstallerProgress {
    key: &'static str,
    phase: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct VersionCheck {
    local: String,
    remote: String,
    is_latest: bool,
    is_beta: bool,
    repository: String,
}

#[derive(Clone, Debug, Serialize)]
struct AnnotationWorkspace {
    image_folder: String,
    labels_folder: String,
    images: Vec<AnnotationImage>,
    classes: Vec<String>,
    annotations: HashMap<String, Vec<AnnotationBox>>,
    invalid_class_ids: Vec<usize>,
}

#[derive(Clone, Debug, Serialize)]
struct AnnotationExifProgress {
    phase: &'static str,
    processed: usize,
    total: usize,
    corrected: usize,
    failed: usize,
    current_file: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct AnnotationPreparationSummary {
    total: usize,
    corrected: usize,
    failed: usize,
    failed_files: Vec<String>,
}

impl From<AnnotationOrientationSummary> for AnnotationPreparationSummary {
    fn from(summary: AnnotationOrientationSummary) -> Self {
        Self {
            total: summary.total,
            corrected: summary.corrected,
            failed: summary.failed,
            failed_files: summary.failed_files,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct AnnotationLoadResult {
    workspace: AnnotationWorkspace,
    summary: AnnotationPreparationSummary,
}

#[derive(Clone, Debug, Serialize)]
struct ImageConversionProgress {
    phase: &'static str,
    processed: usize,
    total: usize,
    converted: usize,
    normalized: usize,
    skipped: usize,
    failed: usize,
    current_file: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct ImageConversionSummary {
    total: usize,
    converted: usize,
    normalized: usize,
    skipped: usize,
    failed: usize,
    failed_files: Vec<String>,
}

impl From<CoreImageConversionSummary> for ImageConversionSummary {
    fn from(summary: CoreImageConversionSummary) -> Self {
        Self {
            total: summary.total,
            converted: summary.converted,
            normalized: summary.normalized,
            skipped: summary.skipped,
            failed: summary.failed,
            failed_files: summary.failed_files,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct AnnotationImage {
    name: String,
    path: String,
    annotation_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AnnotationBox {
    class_id: usize,
    x_center: f64,
    y_center: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, Serialize)]
struct AnnotationImageData {
    mime: String,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
struct AnnotationSaveResult {
    path: String,
    count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct UvcdResult {
    changed: bool,
    message: String,
    path: Option<String>,
    format: String,
}

#[derive(Clone, Debug, Serialize)]
struct SettingsResetResult {
    dashboard: Dashboard,
    uvcd: UvcdResult,
}

#[derive(Clone, Debug, Deserialize)]
struct EndpointManifest {
    repository: String,
    version_check: UrlSet,
    downloads: DownloadManifest,
    realtek_packages: RealtekPackageManifest,
    model_converter: ModelConverterManifest,
    internet_check_urls: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct DownloadManifest {
    arduino_ide: VerifiedUrlSet,
    arduino_ide_msi: VerifiedUrlSet,
    vlc: VerifiedUrlSet,
}

#[derive(Clone, Debug, Deserialize)]
struct RealtekPackageManifest {
    beta: UrlSet,
    release: UrlSet,
}

#[derive(Clone, Debug, Deserialize)]
struct ModelConverterManifest {
    site_url: String,
    api_base: String,
}

#[derive(Clone, Debug, Deserialize)]
struct UrlSet {
    urls: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct VerifiedUrlSet {
    urls: Vec<String>,
    sha256: String,
    size: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstallerCacheKey {
    ArduinoIdeExe,
    ArduinoIdeMsi,
    VlcExe,
}

impl InstallerCacheKey {
    fn payload_file_name(self, sha256: &str) -> String {
        match self {
            Self::ArduinoIdeExe => format!("arduino-ide-windows-64bit-{sha256}.exe"),
            Self::ArduinoIdeMsi => format!("arduino-ide-windows-64bit-{sha256}.msi"),
            Self::VlcExe => format!("vlc-windows-32bit-{sha256}.exe"),
        }
    }

    fn metadata_file_name(self) -> &'static str {
        match self {
            Self::ArduinoIdeExe => "arduino-ide-windows-64bit-exe.json",
            Self::ArduinoIdeMsi => "arduino-ide-windows-64bit-msi.json",
            Self::VlcExe => "vlc-windows-32bit.json",
        }
    }

    fn progress_key(self) -> &'static str {
        match self {
            Self::ArduinoIdeExe | Self::ArduinoIdeMsi => "arduino",
            Self::VlcExe => "vlc",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct InstallerMetadata {
    file_name: String,
    urls: Vec<String>,
    sha256: String,
    size: u64,
}

#[derive(Clone, Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

#[derive(Clone, Debug)]
struct InstallerCachePaths {
    root: PathBuf,
    metadata: PathBuf,
}

impl InstallerCachePaths {
    fn payload(&self, key: InstallerCacheKey, sha256: &str) -> PathBuf {
        self.root.join(key.payload_file_name(sha256))
    }
}

#[derive(Clone, Debug)]
struct CachedInstaller {
    path: PathBuf,
    metadata: InstallerMetadata,
}

enum ArduinoInstallerResolution {
    Latest(InstallerMetadata),
    CachedCandidate(InstallerMetadata),
    Fallback(InstallerMetadata),
}

impl ArduinoInstallerResolution {
    fn metadata(&self) -> &InstallerMetadata {
        match self {
            Self::Latest(metadata) | Self::CachedCandidate(metadata) | Self::Fallback(metadata) => {
                metadata
            }
        }
    }
}

struct AppState {
    settings: Mutex<Settings>,
    output_folder: Mutex<Option<PathBuf>>,
    capture_lock: Mutex<()>,
    native_dialog_lock: Mutex<()>,
    image_processing_lock: Arc<Mutex<()>>,
    arduino_installer_lock: Mutex<()>,
    vlc_installer_lock: Mutex<()>,
}

fn lock_native_dialog(state: &AppState) -> Result<MutexGuard<'_, ()>, AppError> {
    match state.native_dialog_lock.try_lock() {
        Ok(guard) => Ok(guard),
        Err(TryLockError::WouldBlock) => Err(AppError::Message(
            "A native file dialog is already open".into(),
        )),
        Err(TryLockError::Poisoned(error)) => Ok(error.into_inner()),
    }
}

fn lock_installer_operation(state: &AppState, key: InstallerCacheKey) -> MutexGuard<'_, ()> {
    let lock = match key {
        InstallerCacheKey::ArduinoIdeExe | InstallerCacheKey::ArduinoIdeMsi => {
            &state.arduino_installer_lock
        }
        InstallerCacheKey::VlcExe => &state.vlc_installer_lock,
    };

    match lock.lock() {
        Ok(guard) => guard,
        Err(error) => error.into_inner(),
    }
}

struct NativeDialogStateGuard<'a> {
    window: &'a tauri::WebviewWindow,
}

impl<'a> NativeDialogStateGuard<'a> {
    fn new(window: &'a tauri::WebviewWindow) -> Self {
        let _ = window.emit(NATIVE_DIALOG_STATE_EVENT, true);
        Self { window }
    }
}

impl Drop for NativeDialogStateGuard<'_> {
    fn drop(&mut self) {
        let _ = self.window.emit(NATIVE_DIALOG_STATE_EVENT, false);
    }
}

fn with_native_dialog<T>(
    window: &tauri::WebviewWindow,
    state: &AppState,
    show: impl FnOnce() -> T,
) -> Result<T, AppError> {
    let _dialog_guard = lock_native_dialog(state)?;
    let _state_guard = NativeDialogStateGuard::new(window);
    Ok(show())
}

struct EmbeddedResource {
    path: &'static str,
    bytes: &'static [u8],
}

static EMBEDDED_RESOURCES: &[EmbeddedResource] = &[
    EmbeddedResource {
        path: "CH341SER.EXE",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resource/CH341SER.EXE"
        )),
    },
    EmbeddedResource {
        path: "gesture_recognition/hand_code.txt",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resource/gesture_recognition/hand_code.txt"
        )),
    },
    EmbeddedResource {
        path: "gesture_recognition/yolov7_tiny.nb",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resource/gesture_recognition/yolov7_tiny.nb"
        )),
    },
    EmbeddedResource {
        path: "object_detection_box/code.txt",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resource/object_detection_box/code.txt"
        )),
    },
    EmbeddedResource {
        path: "object_detection_box/yolov7_tiny.nb",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resource/object_detection_box/yolov7_tiny.nb"
        )),
    },
    EmbeddedResource {
        path: "image_classification_japan/img_class_cnn.nb",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resource/image_classification_japan/img_class_cnn.nb"
        )),
    },
    EmbeddedResource {
        path: "image_classification_taiwan/img_class_cnn.nb",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resource/image_classification_taiwan/img_class_cnn.nb"
        )),
    },
    EmbeddedResource {
        path: "image_classification_singapore/img_class_cnn.nb",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resource/image_classification_singapore/img_class_cnn.nb"
        )),
    },
];

pub fn run() {
    let settings = load_settings().unwrap_or_default();

    tauri::Builder::default()
        .manage(AppState {
            settings: Mutex::new(settings),
            output_folder: Mutex::new(None),
            capture_lock: Mutex::new(()),
            native_dialog_lock: Mutex::new(()),
            image_processing_lock: Arc::new(Mutex::new(())),
            arduino_installer_lock: Mutex::new(()),
            vlc_installer_lock: Mutex::new(()),
        })
        .setup(|app| {
            install_camera_permission_handler(app);
            let handle = app.handle().clone();
            start_uvcd_worker(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            set_language,
            set_uvcd_format,
            set_preference_version,
            reset_settings,
            clear_installed_weights,
            open_realtek_folder,
            open_output_folder,
            select_output_folder,
            open_url,
            save_driver_as,
            save_hand_resources_as,
            save_object_detection_box_resources_as,
            save_image_model_japan_as,
            save_image_model_taiwan_as,
            save_image_model_singapore_as,
            download_arduino_ide_as,
            download_and_install_arduino_ide,
            get_arduino_cli_status,
            add_arduino_cli_to_path,
            download_vlc_as,
            download_and_install_vlc,
            read_model_converter_file,
            download_model_conversion_as,
            check_internet,
            check_version,
            save_capture_image,
            select_annotation_folder,
            load_annotation_folder,
            select_image_conversion_folder,
            convert_image_folder,
            read_annotation_image,
            save_annotation_classes,
            save_annotation_file,
            save_annotation_workspace
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AMB82 desktop application");
}

#[cfg(windows)]
fn install_camera_permission_handler(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        eprintln!("main webview window not found; camera permission handler not installed");
        return;
    };

    if let Err(error) = window.with_webview(|webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2Profile4, ICoreWebView2_13, COREWEBVIEW2_PERMISSION_KIND,
            COREWEBVIEW2_PERMISSION_KIND_CAMERA, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
        };
        use webview2_com::{PermissionRequestedEventHandler, SetPermissionStateCompletedHandler};
        use windows::core::{Interface, HSTRING};

        let result = (|| -> webview2_com::Result<()> {
            let controller = webview.controller();
            let webview = unsafe { controller.CoreWebView2()? };
            allow_camera_permission_for_app_origins(&webview)?;
            let mut token = Default::default();
            unsafe {
                webview.add_PermissionRequested(
                    &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };

                        let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                        args.PermissionKind(&mut kind)?;
                        if kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA {
                            args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                        }

                        Ok(())
                    })),
                    &mut token,
                )?;
            }

            Ok(())
        })();

        fn allow_camera_permission_for_app_origins(
            webview: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
        ) -> webview2_com::Result<()> {
            const APP_ORIGINS: &[&str] = &[
                "http://tauri.localhost",
                "https://tauri.localhost",
                "http://localhost:1420",
                "http://127.0.0.1:1420",
            ];

            let webview = webview.cast::<ICoreWebView2_13>()?;
            let profile = unsafe { webview.Profile()? };
            let profile = profile.cast::<ICoreWebView2Profile4>()?;
            for origin in APP_ORIGINS {
                let origin = HSTRING::from(*origin);
                unsafe {
                    profile.SetPermissionState(
                        COREWEBVIEW2_PERMISSION_KIND_CAMERA,
                        &origin,
                        COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                        &SetPermissionStateCompletedHandler::create(Box::new(|_| Ok(()))),
                    )?;
                }
            }

            Ok(())
        }

        if let Err(error) = result {
            eprintln!("failed to install camera permission handler: {error}");
        }
    }) {
        eprintln!("failed to access webview for camera permission handler: {error}");
    }
}

#[cfg(not(windows))]
fn install_camera_permission_handler(_app: &tauri::App) {}

#[tauri::command]
fn get_dashboard(state: tauri::State<AppState>) -> Result<Dashboard, AppError> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| AppError::Message("Failed to read settings".into()))?
        .clone();

    Ok(Dashboard {
        metadata: metadata(&settings.preference_version)?,
        settings,
        realtek_folder: find_realtek_folder().map(display_path),
        output_folder: display_path(output_dir(&state)?),
        internet_connected: has_internet(),
    })
}

#[tauri::command]
fn set_language(language: String, state: tauri::State<AppState>) -> Result<Settings, AppError> {
    let supported = ["zh_TW", "en_US", "ja_JP"];
    if !supported.contains(&language.as_str()) {
        return Err(AppError::Message("Unsupported language".into()));
    }

    let mut settings = state
        .settings
        .lock()
        .map_err(|_| AppError::Message("Failed to update settings".into()))?;
    settings.language = language;
    let next = settings.clone();
    save_settings(&next)?;
    Ok(next)
}

#[tauri::command]
fn set_uvcd_format(format: String, state: tauri::State<AppState>) -> Result<UvcdResult, AppError> {
    let format = normalize_uvcd_format(&format)?;
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| AppError::Message("Failed to update settings".into()))?;
        settings.uvcd_format = format.clone();
        save_settings(&settings)?;
    }

    match repair_uvcd(&format) {
        Ok(result) => Ok(result),
        Err(error) => Ok(UvcdResult {
            changed: false,
            message: format!("UVC setting saved; {error}"),
            path: None,
            format,
        }),
    }
}

#[tauri::command]
fn set_preference_version(
    version: String,
    state: tauri::State<AppState>,
) -> Result<Dashboard, AppError> {
    let version = normalize_preference_version(&version)?;
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| AppError::Message("Failed to update settings".into()))?;
        settings.preference_version = version;
        save_settings(&settings)?;
    }

    get_dashboard(state)
}

#[tauri::command]
fn reset_settings(state: tauri::State<AppState>) -> Result<SettingsResetResult, AppError> {
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| AppError::Message("Failed to reset settings".into()))?;
        settings.uvcd_format = DEFAULT_UVCD_FORMAT.to_string();
        settings.preference_version = DEFAULT_PREFERENCE_VERSION.to_string();
        save_settings(&settings)?;
    }

    let uvcd = match repair_uvcd(DEFAULT_UVCD_FORMAT) {
        Ok(result) => result,
        Err(error) => UvcdResult {
            changed: false,
            message: format!("Settings reset; {error}"),
            path: None,
            format: DEFAULT_UVCD_FORMAT.to_string(),
        },
    };

    Ok(SettingsResetResult {
        dashboard: get_dashboard(state)?,
        uvcd,
    })
}

#[tauri::command]
fn clear_installed_weights() -> Result<InstalledWeightCleanupResult, AppError> {
    let folder = find_realtek_folder()
        .ok_or_else(|| AppError::Message("Realtek AmebaPro2 folder was not found".into()))?;
    clear_installed_weights_from(&folder)
}

#[tauri::command]
fn open_realtek_folder() -> Result<ActionResult, AppError> {
    let folder = find_realtek_folder()
        .ok_or_else(|| AppError::Message("Realtek AmebaPro2 folder was not found".into()))?;
    open_in_explorer(&folder)?;
    Ok(ActionResult {
        ok: true,
        message: "Realtek folder opened".into(),
        path: Some(display_path(folder)),
    })
}

#[tauri::command]
fn open_output_folder(state: tauri::State<AppState>) -> Result<ActionResult, AppError> {
    let folder = output_dir(&state)?;
    fs::create_dir_all(&folder)?;
    open_in_explorer(&folder)?;
    Ok(ActionResult {
        ok: true,
        message: "Output folder opened".into(),
        path: Some(display_path(folder)),
    })
}

#[tauri::command]
fn select_output_folder(
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<ActionResult, AppError> {
    let Some(parent_folder) = pick_folder_dialog(&window, &state, "Select output folder location")?
    else {
        return Err(AppError::Message("Folder selection was canceled".into()));
    };

    let folder = parent_folder.join("output");
    fs::create_dir_all(&folder)?;
    *state
        .output_folder
        .lock()
        .map_err(|_| AppError::Message("Failed to update output folder".into()))? =
        Some(folder.clone());

    Ok(ActionResult {
        ok: true,
        message: "Output folder location selected".into(),
        path: Some(display_path(folder)),
    })
}

#[tauri::command]
fn open_url(url: String) -> Result<ActionResult, AppError> {
    if !is_allowed_external_url(&url) {
        return Err(AppError::Message(
            "Only approved HTTPS URLs can be opened".into(),
        ));
    }

    open_in_browser(&url)?;
    Ok(ActionResult {
        ok: true,
        message: "URL opened".into(),
        path: Some(url),
    })
}

fn is_allowed_external_url(url: &str) -> bool {
    let Some(host) = external_url_host(url) else {
        return false;
    };

    ALLOWED_EXTERNAL_URL_HOSTS
        .iter()
        .any(|allowed_host| host.eq_ignore_ascii_case(allowed_host))
}

fn external_url_host(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https") {
        return None;
    }

    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.contains('@') || authority.starts_with('[') {
        return None;
    }

    let host = if let Some((host, port)) = authority.rsplit_once(':') {
        if host.is_empty() || port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }

        host
    } else {
        authority
    };

    if host.is_empty() || host.ends_with('.') || host.contains(':') {
        return None;
    }

    Some(host)
}

#[tauri::command]
fn save_driver_as(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<ActionResult, AppError> {
    save_one_resource_as(
        &app,
        &window,
        &state,
        "CH341SER.EXE",
        "CH341SER.EXE",
        "Save CH340/CH341 installer",
    )
}

#[tauri::command]
fn save_hand_resources_as(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<ActionResult, AppError> {
    save_resource_set_as(
        &app,
        &window,
        &state,
        &[
            (
                "gesture_recognition/hand_code.txt",
                "hand_code.txt",
                "Save hand tracking code",
            ),
            (
                "gesture_recognition/yolov7_tiny.nb",
                "yolov7_tiny.nb",
                "Save hand tracking weight",
            ),
        ],
    )
}

#[tauri::command]
fn save_object_detection_box_resources_as(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<ActionResult, AppError> {
    save_resource_set_as(
        &app,
        &window,
        &state,
        &[
            (
                "object_detection_box/code.txt",
                "code.txt",
                "Save AMB box tracking code",
            ),
            (
                "object_detection_box/yolov7_tiny.nb",
                "yolov7_tiny.nb",
                "Save AMB box tracking weight",
            ),
        ],
    )
}

#[tauri::command]
fn save_image_model_japan_as(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<ActionResult, AppError> {
    save_one_resource_as(
        &app,
        &window,
        &state,
        "image_classification_japan/img_class_cnn.nb",
        "img_class_cnn.nb",
        "Save image classification weight",
    )
}

#[tauri::command]
fn save_image_model_taiwan_as(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<ActionResult, AppError> {
    save_one_resource_as(
        &app,
        &window,
        &state,
        "image_classification_taiwan/img_class_cnn.nb",
        "img_class_cnn.nb",
        "Save image classification weight",
    )
}

#[tauri::command]
fn save_image_model_singapore_as(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<ActionResult, AppError> {
    save_one_resource_as(
        &app,
        &window,
        &state,
        "image_classification_singapore/img_class_cnn.nb",
        "img_class_cnn.nb",
        "Save image classification weight",
    )
}

#[tauri::command]
fn download_arduino_ide_as(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<DownloadResult, AppError> {
    let key = InstallerCacheKey::ArduinoIdeExe;
    let _installer_guard = lock_installer_operation(&state, key);
    let manifest = endpoint_manifest()?;
    let fallback = installer_metadata(&manifest.downloads.arduino_ide, key)?;
    let resolution = resolve_arduino_installer(&app, key, &fallback);
    let target = save_dialog(
        &window,
        &state,
        &resolution.metadata().file_name,
        "Save Arduino IDE installer",
    )?
    .ok_or_else(|| AppError::Message("Save was canceled".into()))?;
    let cached = obtain_arduino_installer(&app, key, resolution, &fallback)?;
    copy_cached_installer_to(&app, key, &cached, &target)
}

#[tauri::command]
async fn download_and_install_arduino_ide(app: AppHandle) -> Result<InstallationResult, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let key = InstallerCacheKey::ArduinoIdeMsi;
        let _installer_guard = lock_installer_operation(&state, key);
        let manifest = endpoint_manifest()?;
        let fallback = installer_metadata(&manifest.downloads.arduino_ide_msi, key)?;
        let resolution = resolve_arduino_installer(&app, key, &fallback);
        let cached = obtain_arduino_installer(&app, key, resolution, &fallback)?;
        emit_installer_progress(&app, key, "installing");
        let outcome = installer_process::install_msi(&cached.path).map_err(AppError::Message)?;
        emit_installer_progress(&app, key, "configuring_path");
        let (arduino_cli, path_error) = match arduino_cli::add_to_user_path() {
            Ok(status) => (Some(status), None),
            Err(error) => (arduino_cli::get_status().ok(), Some(error)),
        };
        Ok(InstallationResult {
            path: display_path(&cached.path),
            reboot_required: outcome.reboot_required,
            arduino_cli,
            path_error,
        })
    })
    .await
    .map_err(|error| AppError::Message(format!("Arduino installation task failed: {error}")))?
}

#[tauri::command]
async fn get_arduino_cli_status() -> Result<ArduinoCliStatus, AppError> {
    tauri::async_runtime::spawn_blocking(arduino_cli::get_status)
        .await
        .map_err(|error| AppError::Message(format!("Arduino CLI status task failed: {error}")))?
        .map_err(AppError::Message)
}

#[tauri::command]
async fn add_arduino_cli_to_path(app: AppHandle) -> Result<ArduinoCliStatus, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _installer_guard = lock_installer_operation(&state, InstallerCacheKey::ArduinoIdeMsi);
        arduino_cli::add_to_user_path().map_err(AppError::Message)
    })
    .await
    .map_err(|error| AppError::Message(format!("Arduino CLI PATH task failed: {error}")))?
}

fn emit_installer_progress(app: &AppHandle, key: InstallerCacheKey, phase: &'static str) {
    let _ = app.emit(
        "installer-progress",
        InstallerProgress {
            key: key.progress_key(),
            phase,
        },
    );
}

#[tauri::command]
fn download_vlc_as(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<DownloadResult, AppError> {
    let key = InstallerCacheKey::VlcExe;
    let _installer_guard = lock_installer_operation(&state, key);
    let manifest = endpoint_manifest()?;
    let metadata = installer_metadata(&manifest.downloads.vlc, key)?;
    let target = save_dialog(&window, &state, &metadata.file_name, "Save VLC installer")?
        .ok_or_else(|| AppError::Message("Save was canceled".into()))?;
    let cached = ensure_cached_installer(&app, key, &metadata)?;
    copy_cached_installer_to(&app, key, &cached, &target)
}

#[tauri::command]
async fn download_and_install_vlc(app: AppHandle) -> Result<InstallationResult, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let key = InstallerCacheKey::VlcExe;
        let _installer_guard = lock_installer_operation(&state, key);
        let manifest = endpoint_manifest()?;
        let metadata = installer_metadata(&manifest.downloads.vlc, key)?;
        let cached = ensure_cached_installer(&app, key, &metadata)?;
        emit_installer_progress(&app, key, "installing");
        let outcome =
            installer_process::install_exe_silent(&cached.path).map_err(AppError::Message)?;
        Ok(InstallationResult {
            path: display_path(&cached.path),
            reboot_required: outcome.reboot_required,
            arduino_cli: None,
            path_error: None,
        })
    })
    .await
    .map_err(|error| AppError::Message(format!("VLC installation task failed: {error}")))?
}

#[tauri::command]
fn download_model_conversion_as(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
    url: String,
    file_name: String,
) -> Result<DownloadResult, AppError> {
    let manifest = endpoint_manifest()?;
    let allowed_prefix = format!("{}/conversions/", manifest.model_converter.api_base);
    if !url.starts_with(&allowed_prefix) || !url.ends_with("/download") {
        return Err(AppError::Message(
            "Only model converter download URLs are allowed".into(),
        ));
    }

    let default_name = safe_file_name(&file_name);
    let target = save_dialog(&window, &state, &default_name, "Save converted model")?
        .ok_or_else(|| AppError::Message("Save was canceled".into()))?;
    download_to_path(&app, "converter", &url, &target)
}

#[tauri::command]
async fn read_model_converter_file(
    path: String,
    max_bytes: u64,
) -> Result<tauri::ipc::Response, AppError> {
    let path = PathBuf::from(path);
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        read_model_converter_file_from(&path, max_bytes)
    })
    .await
    .map_err(|error| AppError::Message(format!("Model file read task failed: {error}")))??;

    Ok(tauri::ipc::Response::new(bytes))
}

fn read_model_converter_file_from(path: &Path, max_bytes: u64) -> Result<Vec<u8>, AppError> {
    if max_bytes == 0 || max_bytes > MAX_MODEL_CONVERTER_FILE_BYTES {
        return Err(AppError::Message("Invalid model file size limit".into()));
    }

    let mut file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(AppError::Message(
            "The dropped path is not a regular file".into(),
        ));
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(extension.as_deref(), Some("pt" | "h5")) {
        return Err(AppError::Message(
            "Only .pt and .h5 model files can be dropped".into(),
        ));
    }

    if metadata.len() > max_bytes {
        return Err(AppError::Message(format!(
            "Model file exceeds the {} MB limit",
            max_bytes / (1024 * 1024)
        )));
    }

    let mut bytes = Vec::new();
    (&mut file).take(max_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(AppError::Message(format!(
            "Model file exceeds the {} MB limit",
            max_bytes / (1024 * 1024)
        )));
    }
    if bytes.len() as u64 != metadata.len() {
        return Err(AppError::Message(
            "Model file changed while it was being read; please try again".into(),
        ));
    }

    Ok(bytes)
}

#[tauri::command]
fn check_internet() -> bool {
    has_internet()
}

#[tauri::command]
fn check_version() -> Result<VersionCheck, AppError> {
    if !has_internet() {
        return Err(AppError::Message(
            "Internet connection is not available".into(),
        ));
    }

    let manifest = endpoint_manifest()?;
    let remote = get_text_with_fallback(&manifest.version_check.urls)?
        .trim()
        .to_string();

    let local = app_version();
    let ordering = compare_version_numbers(local, &remote).unwrap_or_else(|| {
        if remote == local {
            Ordering::Equal
        } else {
            Ordering::Less
        }
    });

    Ok(VersionCheck {
        local: local.to_string(),
        is_latest: ordering == Ordering::Equal,
        is_beta: ordering == Ordering::Greater,
        remote,
        repository: manifest.repository,
    })
}

#[tauri::command]
fn save_capture_image(
    bytes: Vec<u8>,
    state: tauri::State<AppState>,
) -> Result<ActionResult, AppError> {
    let _capture_guard = state
        .capture_lock
        .lock()
        .map_err(|_| AppError::Message("Failed to lock camera capture storage".into()))?;
    let folder = output_dir(&state)?;
    fs::create_dir_all(&folder)?;
    let file_path = write_next_image(&folder, &bytes)?;

    Ok(ActionResult {
        ok: true,
        message: "Image saved".into(),
        path: Some(display_path(file_path)),
    })
}

#[tauri::command]
fn select_annotation_folder(
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<Option<String>, AppError> {
    let Some(folder) = pick_folder_dialog(&window, &state, "Select image folder")? else {
        return Ok(None);
    };

    Ok(Some(display_path(folder)))
}

#[tauri::command]
async fn load_annotation_folder(
    path: String,
    on_progress: Channel<AnnotationExifProgress>,
    state: tauri::State<'_, AppState>,
) -> Result<AnnotationLoadResult, AppError> {
    let image_processing_lock = Arc::clone(&state.image_processing_lock);
    tauri::async_runtime::spawn_blocking(move || {
        let _load_guard = image_processing_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        prepare_annotation_folder(&path, |progress| {
            let _ = on_progress.send(progress);
        })
    })
    .await
    .map_err(|error| AppError::Message(format!("Annotation preparation task failed: {error}")))?
}

#[tauri::command]
fn select_image_conversion_folder(
    window: tauri::WebviewWindow,
    state: tauri::State<AppState>,
) -> Result<Option<String>, AppError> {
    let Some(folder) = pick_folder_dialog(&window, &state, "Select image conversion folder")?
    else {
        return Ok(None);
    };
    Ok(Some(display_path(folder)))
}

#[tauri::command]
async fn convert_image_folder(
    path: String,
    on_progress: Channel<ImageConversionProgress>,
    state: tauri::State<'_, AppState>,
) -> Result<ImageConversionSummary, AppError> {
    let image_processing_lock = Arc::clone(&state.image_processing_lock);
    tauri::async_runtime::spawn_blocking(move || {
        let _processing_guard = image_processing_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        prepare_image_conversion(&path, |progress| {
            let _ = on_progress.send(progress);
        })
    })
    .await
    .map_err(|error| AppError::Message(format!("Image conversion task failed: {error}")))?
}

fn prepare_image_conversion(
    path: &str,
    mut on_progress: impl FnMut(ImageConversionProgress),
) -> Result<ImageConversionSummary, AppError> {
    let folder = PathBuf::from(path);
    if !folder.is_dir() {
        return Err(AppError::Message(format!(
            "Please select an image folder: {}",
            display_path(folder)
        )));
    }

    on_progress(ImageConversionProgress {
        phase: "discovering",
        processed: 0,
        total: 0,
        converted: 0,
        normalized: 0,
        skipped: 0,
        failed: 0,
        current_file: None,
    });

    let mut last_progress_at = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    let summary = convert_images_in_folder(&folder, |progress| {
        let is_boundary = progress.processed == 0 || progress.processed == progress.total;
        if is_boundary || last_progress_at.elapsed() >= Duration::from_millis(50) {
            last_progress_at = Instant::now();
            on_progress(image_conversion_progress("converting", &progress));
        }
    })?;

    on_progress(ImageConversionProgress {
        phase: "complete",
        processed: summary.total,
        total: summary.total,
        converted: summary.converted,
        normalized: summary.normalized,
        skipped: summary.skipped,
        failed: summary.failed,
        current_file: None,
    });
    Ok(summary.into())
}

fn image_conversion_progress(
    phase: &'static str,
    progress: &CoreImageConversionProgress,
) -> ImageConversionProgress {
    ImageConversionProgress {
        phase,
        processed: progress.processed,
        total: progress.total,
        converted: progress.converted,
        normalized: progress.normalized,
        skipped: progress.skipped,
        failed: progress.failed,
        current_file: progress.current_file.clone(),
    }
}

fn prepare_annotation_folder(
    path: &str,
    mut on_progress: impl FnMut(AnnotationExifProgress),
) -> Result<AnnotationLoadResult, AppError> {
    let path = PathBuf::from(path);
    let folder = if path.is_file() {
        path.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::Message("Image folder does not exist".into()))?
    } else {
        path
    };

    on_progress(AnnotationExifProgress {
        phase: "discovering",
        processed: 0,
        total: 0,
        corrected: 0,
        failed: 0,
        current_file: None,
    });

    let mut last_progress_at = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    let summary = normalize_annotation_orientations(&folder, |progress| {
        let is_boundary = progress.processed == 0 || progress.processed == progress.total;
        if is_boundary || last_progress_at.elapsed() >= Duration::from_millis(50) {
            last_progress_at = Instant::now();
            on_progress(annotation_exif_progress("normalizing", &progress));
        }
    })?;

    on_progress(AnnotationExifProgress {
        phase: "loading",
        processed: summary.total,
        total: summary.total,
        corrected: summary.corrected,
        failed: summary.failed,
        current_file: None,
    });
    let workspace = load_annotation_workspace(&folder)?;
    on_progress(AnnotationExifProgress {
        phase: "complete",
        processed: summary.total,
        total: summary.total,
        corrected: summary.corrected,
        failed: summary.failed,
        current_file: None,
    });

    Ok(AnnotationLoadResult {
        workspace,
        summary: summary.into(),
    })
}

fn annotation_exif_progress(
    phase: &'static str,
    progress: &AnnotationOrientationProgress,
) -> AnnotationExifProgress {
    AnnotationExifProgress {
        phase,
        processed: progress.processed,
        total: progress.total,
        corrected: progress.corrected,
        failed: progress.failed,
        current_file: progress.current_file.clone(),
    }
}

#[tauri::command]
fn read_annotation_image(path: String) -> Result<AnnotationImageData, AppError> {
    let path = Path::new(&path);
    if !path.is_file() || !is_supported_image(path) {
        return Err(AppError::Message("Unsupported image file".into()));
    }

    Ok(AnnotationImageData {
        mime: image_mime(path).to_string(),
        bytes: fs::read(path)?,
    })
}

#[tauri::command]
fn save_annotation_classes(
    labels_folder: String,
    classes: Vec<String>,
) -> Result<AnnotationSaveResult, AppError> {
    validate_class_names(&classes)?;
    let labels_folder = PathBuf::from(labels_folder);
    fs::create_dir_all(&labels_folder)?;
    let path = labels_folder.join("classes.txt");
    write_classes_file(&path, &classes)?;

    Ok(AnnotationSaveResult {
        path: display_path(path),
        count: classes.len(),
    })
}

#[tauri::command]
fn save_annotation_file(
    labels_folder: String,
    image_file_name: String,
    annotations: Vec<AnnotationBox>,
) -> Result<AnnotationSaveResult, AppError> {
    let labels_folder = PathBuf::from(labels_folder);
    fs::create_dir_all(&labels_folder)?;
    let path = label_path_for_image(&labels_folder, &image_file_name)?;
    write_annotation_file(&path, &annotations)?;

    Ok(AnnotationSaveResult {
        path: display_path(path),
        count: annotations.len(),
    })
}

#[tauri::command]
fn save_annotation_workspace(
    labels_folder: String,
    classes: Vec<String>,
    annotations: HashMap<String, Vec<AnnotationBox>>,
) -> Result<AnnotationSaveResult, AppError> {
    validate_class_names(&classes)?;
    let labels_folder = PathBuf::from(labels_folder);
    fs::create_dir_all(&labels_folder)?;
    write_classes_file(&labels_folder.join("classes.txt"), &classes)?;

    let mut count = 0_usize;
    for (image_file_name, boxes) in annotations {
        let path = label_path_for_image(&labels_folder, &image_file_name)?;
        write_annotation_file(&path, &boxes)?;
        count += boxes.len();
    }

    Ok(AnnotationSaveResult {
        path: display_path(labels_folder),
        count,
    })
}

fn metadata(preference_version: &str) -> Result<Metadata, AppError> {
    let manifest = endpoint_manifest()?;
    Ok(Metadata {
        author: AUTHOR,
        contact: CONTACT,
        version: app_version(),
        repository: manifest.repository,
        arduino_ide_url: first_url(&manifest.downloads.arduino_ide.urls)?.to_string(),
        vlc_url: first_url(&manifest.downloads.vlc.urls)?.to_string(),
        realtek_package_url: preference_url(preference_version)?,
        model_converter_url: manifest.model_converter.site_url,
        model_converter_api_base: manifest.model_converter.api_base,
        supported_languages: vec!["zh_TW", "en_US", "ja_JP"],
    })
}

fn load_annotation_workspace(folder: &Path) -> Result<AnnotationWorkspace, AppError> {
    if !folder.is_dir() {
        return Err(AppError::Message(format!(
            "Image folder does not exist: {}",
            display_path(folder)
        )));
    }

    let labels_folder = annotation_labels_folder(folder)?;
    fs::create_dir_all(&labels_folder)?;
    let classes_path = labels_folder.join("classes.txt");
    let classes_from_file = read_classes_file(&classes_path)?;
    let should_recover_classes = classes_from_file.is_none();
    let mut classes = classes_from_file.unwrap_or_default();

    let mut annotations = HashMap::new();
    let mut images = Vec::new();

    let mut image_paths: Vec<PathBuf> = fs::read_dir(folder)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_supported_image(path))
        .collect();
    image_paths.sort_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    });

    for image_path in image_paths {
        let Some(file_name) = image_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        let label_path = label_path_for_image(&labels_folder, &file_name)?;
        let boxes = read_annotation_file(&label_path)?;

        images.push(AnnotationImage {
            name: file_name.clone(),
            path: display_path(&image_path),
            annotation_count: boxes.len(),
        });
        annotations.insert(file_name, boxes);
    }

    if should_recover_classes {
        let recovered_classes = recover_annotation_class_names(annotations.values().flatten())?;
        classes = create_recovered_classes_file(&classes_path, recovered_classes)?;
    }

    let invalid_class_ids = annotations
        .values()
        .flatten()
        .filter_map(|item| (item.class_id >= classes.len()).then_some(item.class_id))
        .collect::<BTreeSet<_>>();

    Ok(AnnotationWorkspace {
        image_folder: display_path(folder),
        labels_folder: display_path(labels_folder),
        images,
        classes,
        annotations,
        invalid_class_ids: invalid_class_ids.into_iter().collect(),
    })
}

fn annotation_labels_folder(folder: &Path) -> Result<PathBuf, AppError> {
    let parent = folder.parent().unwrap_or_else(|| Path::new("."));
    let folder_name = folder
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("images");
    Ok(parent.join(format!("{folder_name}_labels")))
}

fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "bmp"
            )
        })
        .unwrap_or(false)
}

fn image_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("bmp") => "image/bmp",
        _ => "image/jpeg",
    }
}

fn read_classes_file(path: &Path) -> Result<Option<Vec<String>>, AppError> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };

    Ok(Some(
        content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
    ))
}

fn recover_annotation_class_names<'a>(
    boxes: impl Iterator<Item = &'a AnnotationBox>,
) -> Result<Vec<String>, AppError> {
    let Some(max_class_id) = boxes.map(|item| item.class_id).max() else {
        return Ok(Vec::new());
    };
    let class_count = max_class_id.checked_add(1).ok_or_else(|| {
        AppError::Message("Cannot recover classes.txt: class id is too large".into())
    })?;

    if class_count > MAX_RECOVERED_ANNOTATION_CLASSES {
        return Err(AppError::Message(format!(
            "Cannot recover classes.txt: class id {max_class_id} requires more than {MAX_RECOVERED_ANNOTATION_CLASSES} classes"
        )));
    }

    let mut classes = Vec::new();
    classes
        .try_reserve_exact(class_count)
        .map_err(|error| AppError::Message(format!("Cannot recover classes.txt: {error}")))?;
    classes.extend((1..=class_count).map(|index| format!("object{index}")));
    Ok(classes)
}

fn classes_file_content(classes: &[String]) -> String {
    if classes.is_empty() {
        String::new()
    } else {
        format!("{}\n", classes.join("\n"))
    }
}

fn create_recovered_classes_file(
    path: &Path,
    recovered_classes: Vec<String>,
) -> Result<Vec<String>, AppError> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            if let Err(error) = file.write_all(classes_file_content(&recovered_classes).as_bytes())
            {
                drop(file);
                let _ = fs::remove_file(path);
                return Err(error.into());
            }
            Ok(recovered_classes)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read_classes_file(path)?
            .ok_or_else(|| {
                AppError::Message(
                    "classes.txt changed while annotation classes were recovered".into(),
                )
            }),
        Err(error) => Err(error.into()),
    }
}

fn write_classes_file(path: &Path, classes: &[String]) -> Result<(), AppError> {
    fs::write(path, classes_file_content(classes))?;
    Ok(())
}

fn read_annotation_file(path: &Path) -> Result<Vec<AnnotationBox>, AppError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut boxes = Vec::new();
    for (line_index, line) in fs::read_to_string(path)?.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(AppError::Message(format!(
                "Invalid YOLO annotation at {}:{}",
                display_path(path),
                line_index + 1
            )));
        }

        let class_id = parts[0].parse::<usize>().map_err(|_| {
            AppError::Message(format!(
                "Invalid class id at {}:{}",
                display_path(path),
                line_index + 1
            ))
        })?;
        let values = [
            parse_normalized(parts[1], path, line_index + 1)?,
            parse_normalized(parts[2], path, line_index + 1)?,
            parse_normalized(parts[3], path, line_index + 1)?,
            parse_normalized(parts[4], path, line_index + 1)?,
        ];
        boxes.push(AnnotationBox {
            class_id,
            x_center: values[0],
            y_center: values[1],
            width: values[2],
            height: values[3],
        });
    }

    Ok(boxes)
}

fn parse_normalized(value: &str, path: &Path, line: usize) -> Result<f64, AppError> {
    let number = value.parse::<f64>().map_err(|_| {
        AppError::Message(format!(
            "Invalid annotation number at {}:{}",
            display_path(path),
            line
        ))
    })?;

    if (0.0..=1.0).contains(&number) {
        Ok(number)
    } else {
        Err(AppError::Message(format!(
            "Annotation value must be normalized at {}:{}",
            display_path(path),
            line
        )))
    }
}

fn write_annotation_file(path: &Path, boxes: &[AnnotationBox]) -> Result<(), AppError> {
    let mut content = String::new();
    for item in boxes {
        content.push_str(&format!(
            "{} {:.6} {:.6} {:.6} {:.6}\n",
            item.class_id,
            clamp_normalized(item.x_center),
            clamp_normalized(item.y_center),
            clamp_normalized(item.width),
            clamp_normalized(item.height)
        ));
    }
    fs::write(path, content)?;
    Ok(())
}

fn clamp_normalized(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn label_path_for_image(labels_folder: &Path, image_file_name: &str) -> Result<PathBuf, AppError> {
    let file_name = Path::new(image_file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Message("Invalid image file name".into()))?;
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Message("Invalid image file name".into()))?;

    Ok(labels_folder.join(format!("{stem}.txt")))
}

fn validate_class_names(classes: &[String]) -> Result<(), AppError> {
    let re = Regex::new(r"^[A-Za-z0-9]+$").map_err(|error| AppError::Message(error.to_string()))?;
    for name in classes {
        if !re.is_match(name) {
            return Err(AppError::Message(
                "Class names can only contain English letters and numbers".into(),
            ));
        }
    }
    Ok(())
}

fn start_uvcd_worker(app: AppHandle) {
    thread::spawn(move || loop {
        let format = current_uvcd_format(&app).unwrap_or_else(|_| DEFAULT_UVCD_FORMAT.to_string());
        match repair_uvcd(&format) {
            Ok(result) if result.changed => break,
            Ok(_) => break,
            Err(_) => {
                let _ = app.emit("uvcd-status", "UVCD repair pending");
                thread::sleep(Duration::from_secs(300));
            }
        }
    });
}

fn save_one_resource_as(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    state: &AppState,
    source: &str,
    default_name: &str,
    title: &str,
) -> Result<ActionResult, AppError> {
    let target = save_dialog(window, state, default_name, title)?
        .ok_or_else(|| AppError::Message("Save was canceled".into()))?;
    copy_resource_to(app, source, &target)?;
    Ok(ActionResult {
        ok: true,
        message: "File saved".into(),
        path: Some(display_path(target)),
    })
}

fn save_resource_set_as(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    state: &AppState,
    files: &[(&str, &str, &str)],
) -> Result<ActionResult, AppError> {
    let mut saved = Vec::new();

    for (source, default_name, title) in files {
        let Some(target) = save_dialog(window, state, default_name, title)? else {
            if saved.is_empty() {
                return Err(AppError::Message("Save was canceled".into()));
            }
            break;
        };
        copy_resource_to(app, source, &target)?;
        saved.push(display_path(target));
    }

    Ok(ActionResult {
        ok: true,
        message: format!("Saved {} file(s)", saved.len()),
        path: Some(saved.join("\n")),
    })
}

fn copy_resource_to(app: &AppHandle, source: &str, target: &Path) -> Result<(), AppError> {
    if let Some(source_path) = external_resource_path(app, source) {
        fs::copy(source_path, target)?;
        return Ok(());
    }

    let Some(bytes) = embedded_resource_bytes(source) else {
        return Err(AppError::Message(format!(
            "Required resource not found: {}",
            source
        )));
    };
    fs::write(target, bytes)?;
    Ok(())
}

fn installer_metadata(
    source: &VerifiedUrlSet,
    key: InstallerCacheKey,
) -> Result<InstallerMetadata, AppError> {
    let metadata = InstallerMetadata {
        file_name: file_name_from_url(first_url(&source.urls)?)?.to_string(),
        urls: source.urls.clone(),
        sha256: source.sha256.clone(),
        size: source.size,
    };
    validate_installer_metadata(key, &metadata)?;
    Ok(metadata)
}

fn validate_installer_metadata(
    key: InstallerCacheKey,
    metadata: &InstallerMetadata,
) -> Result<(), AppError> {
    if metadata.size == 0 {
        return Err(AppError::Message(
            "Installer metadata includes an invalid zero-byte size".into(),
        ));
    }
    if !is_valid_sha256(&metadata.sha256) {
        return Err(AppError::Message(
            "Installer metadata includes an invalid SHA-256 digest".into(),
        ));
    }
    if metadata.urls.is_empty()
        || metadata
            .urls
            .iter()
            .any(|url| !is_allowed_external_url(url))
    {
        return Err(AppError::Message(
            "Installer metadata includes an invalid download URL".into(),
        ));
    }

    match key {
        InstallerCacheKey::ArduinoIdeExe | InstallerCacheKey::ArduinoIdeMsi => {
            validate_arduino_metadata_url(key, metadata)
        }
        InstallerCacheKey::VlcExe => {
            if metadata.file_name != "vlc-3.0.23-win32.exe" {
                return Err(AppError::Message(
                    "VLC installer metadata includes an unexpected file name".into(),
                ));
            }
            Ok(())
        }
    }
}

fn validate_arduino_metadata_url(
    key: InstallerCacheKey,
    metadata: &InstallerMetadata,
) -> Result<(), AppError> {
    let suffix = match key {
        InstallerCacheKey::ArduinoIdeExe => "_Windows_64bit.exe",
        InstallerCacheKey::ArduinoIdeMsi => "_Windows_64bit.msi",
        InstallerCacheKey::VlcExe => {
            return Err(AppError::Message(
                "VLC cannot use Arduino installer metadata".into(),
            ))
        }
    };
    let Some(tag) = metadata
        .file_name
        .strip_prefix("arduino-ide_")
        .and_then(|name| name.strip_suffix(suffix))
        .filter(|tag| is_safe_release_tag(tag))
    else {
        return Err(AppError::Message(
            "Arduino installer metadata includes an unexpected file name".into(),
        ));
    };
    let expected_url = format!(
        "https://github.com/arduino/arduino-ide/releases/download/{tag}/{}",
        metadata.file_name
    );
    if metadata.urls != [expected_url] {
        return Err(AppError::Message(
            "Arduino installer metadata includes an unexpected download URL".into(),
        ));
    }
    Ok(())
}

fn is_safe_release_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn is_valid_sha256(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn github_latest_arduino_metadata(key: InstallerCacheKey) -> Result<InstallerMetadata, AppError> {
    let response = github_api_agent()
        .get(ARDUINO_LATEST_RELEASE_API)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(http_error)?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(GITHUB_METADATA_MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > GITHUB_METADATA_MAX_BYTES {
        return Err(AppError::Message(
            "GitHub release metadata response is too large".into(),
        ));
    }
    let release: GithubRelease = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Message(format!("GitHub release metadata error: {error}")))?;
    select_github_arduino_asset(&release, key)
}

fn select_github_arduino_asset(
    release: &GithubRelease,
    key: InstallerCacheKey,
) -> Result<InstallerMetadata, AppError> {
    if !is_safe_release_tag(&release.tag_name) {
        return Err(AppError::Message(
            "GitHub latest release includes an invalid Arduino tag".into(),
        ));
    }
    let extension = match key {
        InstallerCacheKey::ArduinoIdeExe => "exe",
        InstallerCacheKey::ArduinoIdeMsi => "msi",
        InstallerCacheKey::VlcExe => {
            return Err(AppError::Message(
                "VLC cannot use an Arduino GitHub release asset".into(),
            ))
        }
    };
    let expected_name = format!("arduino-ide_{}_Windows_64bit.{extension}", release.tag_name);
    let expected_url = format!(
        "https://github.com/arduino/arduino-ide/releases/download/{}/{}",
        release.tag_name, expected_name
    );
    let mut matching_assets = release
        .assets
        .iter()
        .filter(|asset| asset.name == expected_name);
    let asset = matching_assets.next().ok_or_else(|| {
        AppError::Message(format!(
            "GitHub latest release is missing the exact asset {expected_name}"
        ))
    })?;
    if matching_assets.next().is_some() {
        return Err(AppError::Message(format!(
            "GitHub latest release includes duplicate assets named {expected_name}"
        )));
    }
    if asset.browser_download_url != expected_url {
        return Err(AppError::Message(
            "GitHub Arduino asset includes an unexpected download URL".into(),
        ));
    }
    let digest = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|digest| is_valid_sha256(digest))
        .ok_or_else(|| {
            AppError::Message("GitHub Arduino asset is missing a valid SHA-256 digest".into())
        })?;
    let metadata = InstallerMetadata {
        file_name: expected_name,
        urls: vec![expected_url],
        sha256: digest.to_string(),
        size: asset.size,
    };
    validate_installer_metadata(key, &metadata)?;
    Ok(metadata)
}

fn github_api_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(&app_user_agent())
        .timeout_connect(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()
}

fn resolve_arduino_installer(
    app: &AppHandle,
    key: InstallerCacheKey,
    fallback: &InstallerMetadata,
) -> ArduinoInstallerResolution {
    match github_latest_arduino_metadata(key) {
        Ok(metadata) => ArduinoInstallerResolution::Latest(metadata),
        Err(error) => {
            eprintln!(
                "Arduino latest release lookup failed; using verified cache or fallback: {error}"
            );
            match read_cached_installer_metadata(app, key) {
                Ok(Some(metadata)) => ArduinoInstallerResolution::CachedCandidate(metadata),
                Ok(None) => ArduinoInstallerResolution::Fallback(fallback.clone()),
                Err(cache_error) => {
                    eprintln!("Arduino cached metadata is unavailable: {cache_error}");
                    ArduinoInstallerResolution::Fallback(fallback.clone())
                }
            }
        }
    }
}

fn obtain_arduino_installer(
    app: &AppHandle,
    key: InstallerCacheKey,
    resolution: ArduinoInstallerResolution,
    fallback: &InstallerMetadata,
) -> Result<CachedInstaller, AppError> {
    match resolution {
        ArduinoInstallerResolution::Latest(metadata)
        | ArduinoInstallerResolution::Fallback(metadata) => {
            ensure_cached_installer(app, key, &metadata)
        }
        ArduinoInstallerResolution::CachedCandidate(metadata) => {
            if let Some(cached) = verified_cached_installer(app, key, &metadata)? {
                return Ok(cached);
            }
            ensure_cached_installer(app, key, fallback)
        }
    }
}

fn installer_cache_paths(
    app: &AppHandle,
    key: InstallerCacheKey,
) -> Result<InstallerCachePaths, AppError> {
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| AppError::Message(format!("Installer cache path error: {error}")))?
        .join(relative_path(INSTALLER_CACHE_DIRECTORY));
    fs::create_dir_all(&cache_root)?;
    Ok(installer_cache_paths_from_root(&cache_root, key))
}

fn installer_cache_paths_from_root(
    cache_root: &Path,
    key: InstallerCacheKey,
) -> InstallerCachePaths {
    InstallerCachePaths {
        root: cache_root.to_path_buf(),
        metadata: cache_root.join(key.metadata_file_name()),
    }
}

fn read_cached_installer_metadata(
    app: &AppHandle,
    key: InstallerCacheKey,
) -> Result<Option<InstallerMetadata>, AppError> {
    let paths = installer_cache_paths(app, key)?;
    let file_metadata = match fs::metadata(&paths.metadata) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if file_metadata.len() > CACHE_METADATA_MAX_BYTES {
        return Ok(None);
    }
    let metadata: InstallerMetadata = match serde_json::from_slice(&fs::read(&paths.metadata)?) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(None),
    };
    if validate_installer_metadata(key, &metadata).is_err() {
        return Ok(None);
    }
    Ok(Some(metadata))
}

fn write_cached_installer_metadata(
    paths: &InstallerCachePaths,
    metadata: &InstallerMetadata,
) -> Result<(), AppError> {
    let bytes = serde_json::to_vec(metadata)
        .map_err(|error| AppError::Message(format!("Installer cache metadata error: {error}")))?;
    write_bytes_atomically(&paths.metadata, &bytes)
}

fn write_bytes_atomically(target: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let (temporary_path, mut file) = create_temporary_part_file(target)?;
    let write_result = file
        .write_all(bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error.into());
    }
    if let Err(error) = replace_downloaded_file(&temporary_path, target) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(())
}

fn verified_cached_installer(
    app: &AppHandle,
    key: InstallerCacheKey,
    metadata: &InstallerMetadata,
) -> Result<Option<CachedInstaller>, AppError> {
    validate_installer_metadata(key, metadata)?;
    let paths = installer_cache_paths(app, key)?;
    let payload = paths.payload(key, &metadata.sha256);
    let Some(_) = verified_file_size(&payload, metadata)? else {
        return Ok(None);
    };
    Ok(Some(CachedInstaller {
        path: payload,
        metadata: metadata.clone(),
    }))
}

fn verified_file_size(path: &Path, metadata: &InstallerMetadata) -> Result<Option<u64>, AppError> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let (bytes, digest) = sha256_reader(file)?;
    if bytes != metadata.size || digest != metadata.sha256 {
        return Ok(None);
    }
    Ok(Some(bytes))
}

fn sha256_reader(mut reader: impl Read) -> Result<(u64, String), AppError> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes += read as u64;
    }
    Ok((bytes, format!("{:x}", hasher.finalize())))
}

fn ensure_cached_installer(
    app: &AppHandle,
    key: InstallerCacheKey,
    metadata: &InstallerMetadata,
) -> Result<CachedInstaller, AppError> {
    validate_installer_metadata(key, metadata)?;
    let paths = installer_cache_paths(app, key)?;
    let previous_metadata = read_cached_installer_metadata(app, key)?;
    if let Some(cached) = verified_cached_installer(app, key, metadata)? {
        commit_installer_cache_pointer(&paths, key, previous_metadata.as_ref(), metadata, false)?;
        return Ok(cached);
    }

    let payload = paths.payload(key, &metadata.sha256);
    download_verified_to_path_with_fallback(app, key.progress_key(), metadata, &payload)?;
    commit_installer_cache_pointer(&paths, key, previous_metadata.as_ref(), metadata, true)?;
    Ok(CachedInstaller {
        path: payload,
        metadata: metadata.clone(),
    })
}

fn commit_installer_cache_pointer(
    paths: &InstallerCachePaths,
    key: InstallerCacheKey,
    previous: Option<&InstallerMetadata>,
    current: &InstallerMetadata,
    new_payload_created: bool,
) -> Result<(), AppError> {
    if previous == Some(current) {
        return Ok(());
    }
    commit_installer_cache_pointer_with(paths, key, previous, current, new_payload_created, || {
        write_cached_installer_metadata(paths, current)
    })
}

fn commit_installer_cache_pointer_with(
    paths: &InstallerCachePaths,
    key: InstallerCacheKey,
    previous: Option<&InstallerMetadata>,
    current: &InstallerMetadata,
    new_payload_created: bool,
    write_pointer: impl FnOnce() -> Result<(), AppError>,
) -> Result<(), AppError> {
    let current_payload = paths.payload(key, &current.sha256);
    let previous_payload = previous.map(|metadata| paths.payload(key, &metadata.sha256));

    if let Err(error) = write_pointer() {
        if new_payload_created && previous_payload.as_ref() != Some(&current_payload) {
            let _ = fs::remove_file(&current_payload);
        }
        return Err(error);
    }

    if let Some(previous_payload) = previous_payload {
        if previous_payload != current_payload {
            let _ = fs::remove_file(previous_payload);
        }
    }
    Ok(())
}

fn copy_cached_installer_to(
    app: &AppHandle,
    key: InstallerCacheKey,
    cached: &CachedInstaller,
    target: &Path,
) -> Result<DownloadResult, AppError> {
    let bytes = copy_verified_file_atomically(&cached.path, target, &cached.metadata)?;
    emit_download_progress(app, key.progress_key(), bytes, Some(bytes));
    Ok(download_result(target, bytes))
}

fn copy_verified_file_atomically(
    source: &Path,
    target: &Path,
    metadata: &InstallerMetadata,
) -> Result<u64, AppError> {
    write_stream_atomically(
        fs::File::open(source)?,
        target,
        None,
        Some(metadata),
        |_| {},
    )
}

fn download_result(target: &Path, bytes: u64) -> DownloadResult {
    DownloadResult {
        file_name: target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("download")
            .into(),
        path: display_path(target),
        bytes,
    }
}

fn download_verified_to_path_with_fallback(
    app: &AppHandle,
    key: &'static str,
    metadata: &InstallerMetadata,
    target: &Path,
) -> Result<DownloadResult, AppError> {
    let mut errors = Vec::new();

    for url in &metadata.urls {
        match download_to_path_checked(app, key, url, target, Some(metadata)) {
            Ok(result) => return Ok(result),
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }

    Err(AppError::Message(format!(
        "All download URLs failed: {}",
        errors.join("; ")
    )))
}

fn download_to_path(
    app: &AppHandle,
    key: &'static str,
    url: &str,
    target: &Path,
) -> Result<DownloadResult, AppError> {
    let result = download_to_path_checked(app, key, url, target, None)?;
    emit_download_progress(app, key, result.bytes, Some(result.bytes));
    Ok(result)
}

fn download_to_path_checked(
    app: &AppHandle,
    key: &'static str,
    url: &str,
    target: &Path,
    verification: Option<&InstallerMetadata>,
) -> Result<DownloadResult, AppError> {
    let response = http_agent()
        .get(url)
        .set("Accept-Encoding", "identity")
        .call()
        .map_err(http_error)?;
    let total = validated_content_length(response.header("content-length"), verification)?;
    let mut last_emit = 0_u64;

    emit_download_progress(app, key, 0, total);
    let bytes = write_stream_atomically(
        response.into_reader(),
        target,
        total,
        verification,
        |bytes| {
            if bytes.saturating_sub(last_emit) >= 256 * 1024
                && total.is_none_or(|expected| bytes < expected)
            {
                emit_download_progress(app, key, bytes, total);
                last_emit = bytes;
            }
        },
    )?;

    Ok(download_result(target, bytes))
}

fn write_stream_atomically(
    mut reader: impl Read,
    target: &Path,
    content_length: Option<u64>,
    verification: Option<&InstallerMetadata>,
    mut on_chunk: impl FnMut(u64),
) -> Result<u64, AppError> {
    let (temporary_path, mut file) = create_temporary_part_file(target)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    let mut hasher = Sha256::new();
    let transfer_result = (|| -> Result<(), AppError> {
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            let next_bytes = checked_download_size(bytes, read, verification)?;
            file.write_all(&buffer[..read])?;
            hasher.update(&buffer[..read]);
            bytes = next_bytes;
            on_chunk(bytes);
        }
        file.flush()?;
        file.sync_all()?;
        validate_download_length(bytes, content_length)?;
        if let Some(metadata) = verification {
            validate_verified_download(bytes, &format!("{:x}", hasher.finalize()), metadata)?;
        }
        Ok(())
    })();
    drop(file);

    if let Err(error) = transfer_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    if let Err(error) = replace_downloaded_file(&temporary_path, target) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(bytes)
}

fn validated_content_length(
    content_length: Option<&str>,
    verification: Option<&InstallerMetadata>,
) -> Result<Option<u64>, AppError> {
    let Some(value) = content_length else {
        return Ok(None);
    };
    let total = match value.parse::<u64>() {
        Ok(total) => total,
        Err(_) if verification.is_some() => {
            return Err(AppError::Message(
                "Verified installer response includes an invalid Content-Length".into(),
            ))
        }
        Err(_) => return Ok(None),
    };
    if let Some(metadata) = verification {
        if total != metadata.size {
            return Err(AppError::Message(format!(
                "Verified installer Content-Length mismatch: expected {} bytes, received {total} bytes",
                metadata.size
            )));
        }
    }
    Ok(Some(total))
}

fn checked_download_size(
    bytes: u64,
    read: usize,
    verification: Option<&InstallerMetadata>,
) -> Result<u64, AppError> {
    let next_bytes = bytes
        .checked_add(read as u64)
        .ok_or_else(|| AppError::Message("Downloaded byte count overflowed".into()))?;
    if let Some(metadata) = verification {
        if next_bytes > metadata.size {
            return Err(AppError::Message(format!(
                "Verified installer exceeded its expected size of {} bytes",
                metadata.size
            )));
        }
    }
    Ok(next_bytes)
}

fn validate_verified_download(
    bytes: u64,
    digest: &str,
    metadata: &InstallerMetadata,
) -> Result<(), AppError> {
    if bytes != metadata.size {
        return Err(AppError::Message(format!(
            "Downloaded installer size mismatch: expected {} bytes, received {bytes} bytes",
            metadata.size
        )));
    }
    if digest != metadata.sha256 {
        return Err(AppError::Message(format!(
            "Downloaded installer SHA-256 mismatch: expected {}, received {digest}",
            metadata.sha256
        )));
    }
    Ok(())
}

fn validate_download_length(bytes: u64, total: Option<u64>) -> Result<(), AppError> {
    if let Some(expected) = total {
        if bytes != expected {
            return Err(AppError::Message(format!(
                "Downloaded file is incomplete: expected {expected} bytes, received {bytes} bytes"
            )));
        }
    }
    Ok(())
}

fn create_temporary_part_file(target: &Path) -> Result<(PathBuf, fs::File), AppError> {
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    for attempt in 0..100_u32 {
        let path = parent.join(format!(
            ".{file_name}.{}.{}.{attempt}.part",
            std::process::id(),
            nonce
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Err(AppError::Message(
        "Failed to create a unique temporary part file".into(),
    ))
}

#[cfg(windows)]
fn replace_downloaded_file(source: &Path, target: &Path) -> Result<(), AppError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if result == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_downloaded_file(source: &Path, target: &Path) -> Result<(), AppError> {
    fs::rename(source, target)?;
    Ok(())
}

fn emit_download_progress(app: &AppHandle, key: &'static str, downloaded: u64, total: Option<u64>) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            key,
            downloaded,
            total,
        },
    );
}

fn save_dialog(
    window: &tauri::WebviewWindow,
    state: &AppState,
    default_name: &str,
    title: &str,
) -> Result<Option<PathBuf>, AppError> {
    with_native_dialog(window, state, || {
        rfd::FileDialog::new()
            .set_parent(window)
            .set_title(title)
            .set_file_name(default_name)
            .save_file()
    })
}

fn pick_folder_dialog(
    window: &tauri::WebviewWindow,
    state: &AppState,
    title: &str,
) -> Result<Option<PathBuf>, AppError> {
    with_native_dialog(window, state, || {
        rfd::FileDialog::new()
            .set_parent(window)
            .set_title(title)
            .pick_folder()
    })
}

fn file_name_from_url(url: &str) -> Result<&str, AppError> {
    url.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| AppError::Message("Download URL does not include a file name".into()))
}

fn safe_file_name(file_name: &str) -> String {
    Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("converted_model.nb")
        .to_string()
}

fn app_version() -> &'static str {
    VERSION_TEXT.trim()
}

fn app_user_agent() -> String {
    format!("AMB82-Mini-Computer-Plugin/{}", app_version())
}

fn endpoint_manifest() -> Result<EndpointManifest, AppError> {
    let manifest: EndpointManifest = serde_json::from_str(ENDPOINT_MANIFEST_JSON)
        .map_err(|error| AppError::Message(format!("Endpoint manifest error: {error}")))?;
    installer_metadata(
        &manifest.downloads.arduino_ide,
        InstallerCacheKey::ArduinoIdeExe,
    )?;
    installer_metadata(
        &manifest.downloads.arduino_ide_msi,
        InstallerCacheKey::ArduinoIdeMsi,
    )?;
    installer_metadata(&manifest.downloads.vlc, InstallerCacheKey::VlcExe)?;
    Ok(manifest)
}

fn first_url(urls: &[String]) -> Result<&str, AppError> {
    urls.first()
        .map(String::as_str)
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| AppError::Message("Endpoint manifest does not include a URL".into()))
}

fn get_text_with_fallback(urls: &[String]) -> Result<String, AppError> {
    let mut errors = Vec::new();

    for url in urls {
        match http_agent()
            .get(url)
            .call()
            .map_err(http_error)
            .and_then(|response| {
                response
                    .into_string()
                    .map_err(|error| AppError::Message(format!("HTTP read error: {error}")))
            }) {
            Ok(text) => return Ok(text),
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }

    Err(AppError::Message(format!(
        "All endpoint URLs failed: {}",
        errors.join("; ")
    )))
}

fn http_agent() -> ureq::Agent {
    let user_agent = app_user_agent();
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(120))
        .timeout_connect(Duration::from_secs(20))
        .user_agent(&user_agent)
        .build()
}

fn internet_agent() -> ureq::Agent {
    let user_agent = app_user_agent();
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(3))
        .timeout_connect(Duration::from_secs(2))
        .user_agent(&user_agent)
        .build()
}

fn has_internet() -> bool {
    let Ok(manifest) = endpoint_manifest() else {
        return false;
    };

    manifest
        .internet_check_urls
        .iter()
        .any(|url| internet_agent().get(url).call().is_ok())
}

fn http_error(error: ureq::Error) -> AppError {
    AppError::Message(format!("HTTP error: {error}"))
}

fn current_uvcd_format(app: &AppHandle) -> Result<String, AppError> {
    let state = app.state::<AppState>();
    let settings = state
        .settings
        .lock()
        .map_err(|_| AppError::Message("Failed to read settings".into()))?;
    normalize_uvcd_format(&settings.uvcd_format)
}

fn normalize_uvcd_format(format: &str) -> Result<String, AppError> {
    let normalized = format.trim().to_ascii_uppercase();

    if SUPPORTED_UVCD_FORMATS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(AppError::Message(format!(
            "Unsupported UVC device format: {format}"
        )))
    }
}

fn normalize_preference_version(version: &str) -> Result<String, AppError> {
    let normalized = version.trim().to_ascii_lowercase();

    if SUPPORTED_PREFERENCE_VERSIONS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(AppError::Message(format!(
            "Unsupported preference version: {version}"
        )))
    }
}

fn preference_url(version: &str) -> Result<String, AppError> {
    let manifest = endpoint_manifest()?;
    let urls = if version == "release" {
        &manifest.realtek_packages.release.urls
    } else {
        &manifest.realtek_packages.beta.urls
    };

    Ok(first_url(urls)?.to_string())
}

fn compare_version_numbers(local: &str, remote: &str) -> Option<Ordering> {
    let local = version_numbers(local)?;
    let remote = version_numbers(remote)?;

    for (local_part, remote_part) in local.iter().zip(remote.iter()) {
        match local_part.cmp(remote_part) {
            Ordering::Equal => continue,
            ordering => return Some(ordering),
        }
    }

    Some(Ordering::Equal)
}

fn version_numbers(version: &str) -> Option<[u64; 3]> {
    let normalized = version.trim().trim_start_matches(['v', 'V']);
    let mut numbers = [0_u64; 3];
    let mut parts = normalized.split('.');

    for number in &mut numbers {
        let part = parts.next()?;
        let digits: String = part
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect();
        if digits.is_empty() {
            return None;
        }
        *number = digits.parse().ok()?;
    }

    Some(numbers)
}

fn load_settings() -> Result<Settings, AppError> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(Settings::default());
    }

    let content = fs::read_to_string(path)?;
    let mut settings: Settings = serde_json::from_str(&content)
        .map_err(|error| AppError::Message(format!("Settings file error: {error}")))?;
    settings.uvcd_format = normalize_uvcd_format(&settings.uvcd_format)?;
    settings.preference_version = normalize_preference_version(&settings.preference_version)?;
    Ok(settings)
}

fn save_settings(settings: &Settings) -> Result<(), AppError> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(settings)
        .map_err(|error| AppError::Message(format!("Settings file error: {error}")))?;
    fs::write(path, content)?;
    Ok(())
}

fn settings_path() -> Result<PathBuf, AppError> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    Ok(base
        .join("AMB82 Mini Computer Plugin")
        .join("settings.json"))
}

fn find_realtek_folder() -> Option<PathBuf> {
    let user_profile = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    let root = user_profile
        .join("AppData")
        .join("Local")
        .join("Arduino15")
        .join("packages")
        .join("realtek")
        .join("hardware")
        .join("AmebaPro2");

    if !root.exists() {
        return None;
    }

    let mut versions: Vec<PathBuf> = fs::read_dir(&root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    versions.sort();
    versions.pop().or(Some(root))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstalledWeightDeleteOutcome {
    Deleted,
    Missing,
}

fn clear_installed_weights_from(
    version_folder: &Path,
) -> Result<InstalledWeightCleanupResult, AppError> {
    let canonical_version_folder = validate_installed_weight_root(version_folder)?;
    let mut deleted = 0;
    let mut missing = 0;
    let mut failures = Vec::new();

    for relative_path in INSTALLED_WEIGHT_RELATIVE_PATHS {
        match delete_installed_weight_file(
            version_folder,
            &canonical_version_folder,
            Path::new(relative_path),
        ) {
            Ok(InstalledWeightDeleteOutcome::Deleted) => deleted += 1,
            Ok(InstalledWeightDeleteOutcome::Missing) => missing += 1,
            Err(error) => failures.push(format!("{relative_path}: {error}")),
        }
    }

    if !failures.is_empty() {
        return Err(AppError::Message(format!(
            "Failed to clear all installed weights under {} (deleted: {deleted}, missing: {missing}, failed: {}): {}",
            display_path(version_folder),
            failures.len(),
            failures.join("; ")
        )));
    }

    Ok(InstalledWeightCleanupResult {
        deleted,
        missing,
        folder: display_path(version_folder),
    })
}

fn validate_installed_weight_root(version_folder: &Path) -> Result<PathBuf, AppError> {
    let version_metadata = fs::symlink_metadata(version_folder).map_err(|error| {
        AppError::Message(format!(
            "Failed to inspect the AmebaPro2 version folder {}: {error}",
            display_path(version_folder)
        ))
    })?;
    if image_safety::metadata_is_reparse_point(&version_metadata) || !version_metadata.is_dir() {
        return Err(AppError::Message(format!(
            "The AmebaPro2 version folder is not a regular directory: {}",
            display_path(version_folder)
        )));
    }

    let canonical_version_folder = fs::canonicalize(version_folder).map_err(|error| {
        AppError::Message(format!(
            "Failed to resolve the AmebaPro2 version folder {}: {error}",
            display_path(version_folder)
        ))
    })?;
    Ok(canonical_version_folder)
}

fn delete_installed_weight_file(
    version_folder: &Path,
    canonical_version_folder: &Path,
    relative_path: &Path,
) -> Result<InstalledWeightDeleteOutcome, AppError> {
    let mut current_path = version_folder.to_path_buf();
    let mut components = relative_path.components().peekable();

    while let Some(component) = components.next() {
        let std::path::Component::Normal(name) = component else {
            return Err(AppError::Message(format!(
                "Installed weight path is not a safe relative path: {}",
                display_path(relative_path)
            )));
        };
        current_path.push(name);
        let is_file = components.peek().is_none();
        let metadata = match fs::symlink_metadata(&current_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(InstalledWeightDeleteOutcome::Missing);
            }
            Err(error) => {
                return Err(AppError::Message(format!(
                    "Failed to inspect {}: {error}",
                    display_path(&current_path)
                )));
            }
        };

        if image_safety::metadata_is_reparse_point(&metadata) {
            return Err(AppError::Message(format!(
                "Refusing to follow a symbolic link or reparse point: {}",
                display_path(&current_path)
            )));
        }
        if is_file {
            if !metadata.is_file() {
                return Err(AppError::Message(format!(
                    "Installed weight path is not a regular file: {}",
                    display_path(&current_path)
                )));
            }
        } else if !metadata.is_dir() {
            return Err(AppError::Message(format!(
                "Installed weight parent is not a regular directory: {}",
                display_path(&current_path)
            )));
        }
    }

    let mut target_guard = match image_safety::open_locked_file_for_delete(&current_path) {
        Ok(guard) => guard,
        Err(AppError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(InstalledWeightDeleteOutcome::Missing);
        }
        Err(error) => return Err(error),
    };
    let resolved_target = target_guard.resolved_path().map_err(|error| {
        AppError::Message(format!(
            "Failed to resolve the opened installed weight {}: {error}",
            display_path(&current_path)
        ))
    })?;
    ensure_canonical_path_within(canonical_version_folder, &resolved_target, &current_path)?;

    target_guard.mark_delete().map_err(|error| {
        AppError::Message(format!(
            "Failed to delete {}: {error}",
            display_path(&current_path)
        ))
    })?;
    drop(target_guard);
    Ok(InstalledWeightDeleteOutcome::Deleted)
}

fn ensure_canonical_path_within(
    canonical_root: &Path,
    canonical_path: &Path,
    original_path: &Path,
) -> Result<(), AppError> {
    if !canonical_path.starts_with(canonical_root) {
        return Err(AppError::Message(format!(
            "Resolved path escapes the AmebaPro2 version folder: {}",
            display_path(original_path)
        )));
    }
    Ok(())
}

fn repair_uvcd(format: &str) -> Result<UvcdResult, AppError> {
    let format = normalize_uvcd_format(format)?;
    let version_folder = find_realtek_folder()
        .ok_or_else(|| AppError::Message("Realtek AmebaPro2 folder was not found".into()))?;
    let target = version_folder
        .join("libraries")
        .join("USB")
        .join("src")
        .join("UVCD_pram.h");

    if !target.exists() {
        return Err(AppError::Message(format!(
            "UVCD_pram.h was not found under {}",
            display_path(version_folder)
        )));
    }

    let original = fs::read_to_string(&target)?;
    let repaired = repair_uvcd_content(&original, &format)?;
    let changed = repaired != original;

    if changed {
        fs::write(&target, repaired)?;
    }

    Ok(UvcdResult {
        changed,
        message: if changed {
            format!("UVCD_pram.h repaired for {format}")
        } else {
            format!("UVCD_pram.h already matches {format}")
        },
        path: Some(display_path(target)),
        format,
    })
}

fn repair_uvcd_content(original: &str, format: &str) -> Result<String, AppError> {
    let format = normalize_uvcd_format(format)?;
    let enabled_define = format!("UVCD_{format}");
    let re = Regex::new(r"(?m)^(#define\s+)(UVCD_[A-Za-z0-9_]+)(\s+)\d+")
        .map_err(|error| AppError::Message(error.to_string()))?;
    Ok(re
        .replace_all(original, |captures: &regex::Captures<'_>| {
            if &captures[2] == enabled_define.as_str() {
                format!("{}{}{}1", &captures[1], &captures[2], &captures[3])
            } else {
                format!("{}{}{}0", &captures[1], &captures[2], &captures[3])
            }
        })
        .to_string())
}

fn output_dir(state: &tauri::State<AppState>) -> Result<PathBuf, AppError> {
    if let Some(folder) = state
        .output_folder
        .lock()
        .map_err(|_| AppError::Message("Failed to read output folder".into()))?
        .clone()
    {
        return Ok(folder);
    }

    Ok(std::env::current_dir()?.join("output"))
}

fn next_image_path(folder: &Path) -> Result<PathBuf, AppError> {
    let re = Regex::new(r"^image_(\d{5})\.jpg$")
        .map_err(|error| AppError::Message(error.to_string()))?;
    let mut max_id = 0_u32;

    if folder.exists() {
        for entry in fs::read_dir(folder)?.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let Some(captures) = re.captures(&name) else {
                continue;
            };
            let Ok(id) = captures[1].parse::<u32>() else {
                continue;
            };
            max_id = max_id.max(id);
        }
    }

    Ok(folder.join(format!("image_{:05}.jpg", max_id + 1)))
}

fn write_next_image(folder: &Path, bytes: &[u8]) -> Result<PathBuf, AppError> {
    for _ in 0..100 {
        let file_path = next_image_path(folder)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file_path)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes).and_then(|_| file.flush()) {
                    drop(file);
                    let _ = fs::remove_file(&file_path);
                    return Err(error.into());
                }
                return Ok(file_path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Err(AppError::Message(
        "Failed to reserve a unique camera capture file name".into(),
    ))
}

fn embedded_resource_bytes(source: &str) -> Option<&'static [u8]> {
    EMBEDDED_RESOURCES
        .iter()
        .find(|resource| resource.path == source)
        .map(|resource| resource.bytes)
}

fn external_resource_path(app: &AppHandle, source: &str) -> Option<PathBuf> {
    for root in resource_roots(app) {
        let path = root.join(relative_path(source));
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn resource_roots(app: &AppHandle) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(path) = std::env::current_dir()
        .ok()
        .map(|cwd| cwd.join("resource"))
        .filter(|path| path.exists())
    {
        roots.push(path);
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled_resource = resource_dir.join("resource");
        if bundled_resource.exists() {
            roots.push(bundled_resource);
        }
        if resource_dir.exists() {
            roots.push(resource_dir);
        }
    }

    roots
}

fn relative_path(path: &str) -> PathBuf {
    path.split('/').collect()
}

fn open_in_explorer(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        return Err(AppError::Message(format!(
            "Path does not exist: {}",
            display_path(path)
        )));
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(path).spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }

    Ok(())
}

fn open_in_browser(url: &str) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn()?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(url).spawn()?;
    }

    Ok(())
}

fn display_path(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("amb82-{name}-{}-{nonce}", std::process::id()))
    }

    fn installed_weight_test_folder(name: &str) -> PathBuf {
        let folder = test_directory(name);
        fs::create_dir_all(folder.join("libraries")).unwrap();
        folder
    }

    fn installed_weight_path(folder: &Path, index: usize) -> PathBuf {
        folder.join(Path::new(INSTALLED_WEIGHT_RELATIVE_PATHS[index]))
    }

    fn metadata_for_bytes(file_name: &str, bytes: &[u8]) -> InstallerMetadata {
        let (_, sha256) = sha256_reader(std::io::Cursor::new(bytes)).unwrap();
        InstallerMetadata {
            file_name: file_name.to_string(),
            urls: Vec::new(),
            sha256,
            size: bytes.len() as u64,
        }
    }

    fn github_asset(
        name: &str,
        size: u64,
        digest: &str,
        browser_download_url: &str,
    ) -> GithubReleaseAsset {
        GithubReleaseAsset {
            name: name.to_string(),
            size,
            digest: Some(digest.to_string()),
            browser_download_url: browser_download_url.to_string(),
        }
    }

    #[test]
    fn dropped_model_file_reader_accepts_supported_files_case_insensitively() {
        let folder = test_directory("model-drop-supported");
        fs::create_dir_all(&folder).unwrap();
        let pt_path = folder.join("detector.PT");
        let h5_path = folder.join("classifier.h5");
        fs::write(&pt_path, b"pytorch model").unwrap();
        fs::write(&h5_path, b"keras model").unwrap();

        assert_eq!(
            read_model_converter_file_from(&pt_path, 1024).unwrap(),
            b"pytorch model"
        );
        assert_eq!(
            read_model_converter_file_from(&h5_path, 1024).unwrap(),
            b"keras model"
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn dropped_model_file_reader_rejects_unsupported_or_oversized_files() {
        let folder = test_directory("model-drop-invalid");
        fs::create_dir_all(&folder).unwrap();
        let unsupported_path = folder.join("model.txt");
        let oversized_path = folder.join("model.pt");
        fs::write(&unsupported_path, b"model").unwrap();
        fs::write(&oversized_path, b"12345").unwrap();

        assert!(read_model_converter_file_from(&unsupported_path, 1024).is_err());
        assert!(read_model_converter_file_from(&oversized_path, 4).is_err());
        assert!(read_model_converter_file_from(&oversized_path, 0).is_err());
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn dropped_model_file_reader_rejects_a_directory() {
        let folder = test_directory("model-drop-directory");
        let directory_path = folder.join("model.pt");
        fs::create_dir_all(&directory_path).unwrap();

        assert!(read_model_converter_file_from(&directory_path, 1024).is_err());
        fs::remove_dir_all(folder).unwrap();
    }

    fn annotation_test_folders(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = test_directory(name);
        let images = root.join("images");
        let labels = root.join("images_labels");
        fs::create_dir_all(&images).unwrap();
        fs::create_dir_all(&labels).unwrap();
        (root, images, labels)
    }

    #[test]
    fn annotation_preparation_reports_every_phase_before_returning_workspace() {
        let (root, images, _) = annotation_test_folders("annotation-preparation-progress");
        let mut phases = Vec::new();

        let result = prepare_annotation_folder(&display_path(&images), |progress| {
            phases.push(progress.phase);
        })
        .unwrap();

        assert_eq!(
            phases,
            ["discovering", "normalizing", "loading", "complete"]
        );
        assert_eq!(result.summary.total, 0);
        assert_eq!(result.summary.corrected, 0);
        assert_eq!(result.summary.failed, 0);
        assert!(result.workspace.images.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_classes_are_recovered_and_persisted_from_sparse_ids() {
        let (root, images, labels) = annotation_test_folders("annotation-class-recovery");
        fs::write(images.join("a.jpg"), b"").unwrap();
        fs::write(images.join("b.png"), b"").unwrap();
        fs::write(
            labels.join("a.txt"),
            "0 0.5 0.5 0.2 0.2\n3 0.4 0.4 0.1 0.1\n",
        )
        .unwrap();
        fs::write(labels.join("b.txt"), "255 0.5 0.5 0.3 0.3\n").unwrap();

        let workspace = load_annotation_workspace(&images).unwrap();
        let expected_classes = (1..=256)
            .map(|index| format!("object{index}"))
            .collect::<Vec<_>>();

        assert_eq!(workspace.classes, expected_classes);
        assert!(workspace.invalid_class_ids.is_empty());
        assert_eq!(workspace.annotations["a.jpg"].len(), 2);
        assert_eq!(workspace.annotations["b.png"].len(), 1);
        assert_eq!(
            fs::read_to_string(labels.join("classes.txt")).unwrap(),
            format!("{}\n", expected_classes.join("\n"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_annotation_does_not_create_recovered_classes_file() {
        let (root, images, labels) = annotation_test_folders("annotation-invalid-recovery");
        fs::write(images.join("a.jpg"), b"").unwrap();
        fs::write(labels.join("a.txt"), "invalid annotation\n").unwrap();

        let result = load_annotation_workspace(&images);

        assert!(matches!(
            result,
            Err(AppError::Message(message)) if message.contains("Invalid YOLO annotation")
        ));
        assert!(!labels.join("classes.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_annotation_workspace_creates_empty_classes_file() {
        let (root, images, labels) = annotation_test_folders("annotation-empty-recovery");

        let workspace = load_annotation_workspace(&images).unwrap();

        assert!(workspace.images.is_empty());
        assert!(workspace.classes.is_empty());
        assert!(workspace.annotations.is_empty());
        assert!(workspace.invalid_class_ids.is_empty());
        assert_eq!(fs::read_to_string(labels.join("classes.txt")).unwrap(), "");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_classes_file_is_not_overwritten_during_workspace_load() {
        let (root, images, labels) = annotation_test_folders("annotation-existing-classes");
        fs::write(images.join("a.jpg"), b"").unwrap();
        fs::write(labels.join("a.txt"), "2 0.5 0.5 0.2 0.2\n").unwrap();
        fs::write(labels.join("classes.txt"), "dog\n").unwrap();

        let workspace = load_annotation_workspace(&images).unwrap();

        assert_eq!(workspace.classes, ["dog"]);
        assert_eq!(workspace.invalid_class_ids, [2]);
        assert_eq!(
            fs::read_to_string(labels.join("classes.txt")).unwrap(),
            "dog\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovered_classes_do_not_replace_a_concurrently_created_file() {
        let root = test_directory("annotation-concurrent-classes");
        fs::create_dir_all(&root).unwrap();
        let classes_path = root.join("classes.txt");
        fs::write(&classes_path, "realClass\n").unwrap();

        let classes = create_recovered_classes_file(
            &classes_path,
            vec!["object1".to_string(), "object2".to_string()],
        )
        .unwrap();

        assert_eq!(classes, ["realClass"]);
        assert_eq!(fs::read_to_string(&classes_path).unwrap(), "realClass\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn excessive_class_id_does_not_create_recovered_classes_file() {
        let (root, images, labels) = annotation_test_folders("annotation-class-limit");
        fs::write(images.join("a.jpg"), b"").unwrap();
        fs::write(
            labels.join("a.txt"),
            format!("{MAX_RECOVERED_ANNOTATION_CLASSES} 0.5 0.5 0.2 0.2\n"),
        )
        .unwrap();

        let result = load_annotation_workspace(&images);
        let overflow_box = [AnnotationBox {
            class_id: usize::MAX,
            x_center: 0.5,
            y_center: 0.5,
            width: 0.2,
            height: 0.2,
        }];

        assert!(matches!(
            result,
            Err(AppError::Message(message)) if message.contains("requires more than")
        ));
        assert!(recover_annotation_class_names(overflow_box.iter()).is_err());
        assert!(!labels.join("classes.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn installed_weight_cleanup_deletes_both_fixed_files() {
        let folder = installed_weight_test_folder("weight-cleanup-both");
        for index in 0..INSTALLED_WEIGHT_RELATIVE_PATHS.len() {
            let path = installed_weight_path(&folder, index);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("weight-{index}")).unwrap();
        }
        let unrelated_weight = folder
            .join("libraries/NeuralNetwork/examples/ObjectDetectionLoop")
            .join("keep-this-model.nb");
        fs::write(&unrelated_weight, b"unrelated").unwrap();

        let result = clear_installed_weights_from(&folder).unwrap();

        assert_eq!(
            result,
            InstalledWeightCleanupResult {
                deleted: 2,
                missing: 0,
                folder: display_path(&folder),
            }
        );
        assert!(!installed_weight_path(&folder, 0).exists());
        assert!(!installed_weight_path(&folder, 1).exists());
        assert_eq!(fs::read(&unrelated_weight).unwrap(), b"unrelated");
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn installed_weight_cleanup_is_idempotent_when_files_are_missing() {
        let folder = installed_weight_test_folder("weight-cleanup-idempotent");
        let first = installed_weight_path(&folder, 0);
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::write(&first, b"weight").unwrap();

        let first_result = clear_installed_weights_from(&folder).unwrap();
        assert_eq!(first_result.deleted, 1);
        assert_eq!(first_result.missing, 1);

        let second_result = clear_installed_weights_from(&folder).unwrap();
        assert_eq!(second_result.deleted, 0);
        assert_eq!(second_result.missing, 2);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn installed_weight_cleanup_treats_missing_libraries_as_already_clear() {
        let folder = test_directory("weight-cleanup-no-libraries");
        fs::create_dir_all(&folder).unwrap();

        let result = clear_installed_weights_from(&folder).unwrap();

        assert_eq!(result.deleted, 0);
        assert_eq!(result.missing, 2);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn installed_weight_cleanup_attempts_both_targets_and_reports_partial_failure() {
        let folder = installed_weight_test_folder("weight-cleanup-partial");
        let invalid_target = installed_weight_path(&folder, 0);
        fs::create_dir_all(&invalid_target).unwrap();
        let valid_target = installed_weight_path(&folder, 1);
        fs::create_dir_all(valid_target.parent().unwrap()).unwrap();
        fs::write(&valid_target, b"weight").unwrap();

        let error = clear_installed_weights_from(&folder).unwrap_err();
        let message = error.to_string();

        assert!(message.contains("deleted: 1"));
        assert!(message.contains("missing: 0"));
        assert!(message.contains("failed: 1"));
        assert!(message.contains(INSTALLED_WEIGHT_RELATIVE_PATHS[0]));
        assert!(invalid_target.is_dir());
        assert!(!valid_target.exists());
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn canonical_weight_path_must_remain_inside_the_version_folder() {
        let folder = test_directory("weight-cleanup-containment");
        let inside = folder.join("libraries").join("model.nb");
        let outside = folder
            .parent()
            .unwrap()
            .join("amb82-weight-cleanup-outside")
            .join("model.nb");

        assert!(ensure_canonical_path_within(&folder, &inside, &inside).is_ok());
        assert!(ensure_canonical_path_within(&folder, &outside, &outside).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn installed_weight_cleanup_rejects_a_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let folder = installed_weight_test_folder("weight-cleanup-symlink");
        let outside = test_directory("weight-cleanup-symlink-outside");
        let outside_neural_network = outside.join("NeuralNetwork");
        let first_outside = outside_neural_network
            .join("examples")
            .join("RTSPImageClassification")
            .join("img_class_cnn.nb");
        let second_outside = outside_neural_network
            .join("examples")
            .join("ObjectDetectionLoop")
            .join("yolov7_tiny.nb");
        fs::create_dir_all(first_outside.parent().unwrap()).unwrap();
        fs::create_dir_all(second_outside.parent().unwrap()).unwrap();
        fs::write(&first_outside, b"first").unwrap();
        fs::write(&second_outside, b"second").unwrap();
        symlink(
            &outside_neural_network,
            folder.join("libraries/NeuralNetwork"),
        )
        .unwrap();

        let error = clear_installed_weights_from(&folder).unwrap_err();

        assert!(error.to_string().contains("symbolic link or reparse point"));
        assert_eq!(fs::read(&first_outside).unwrap(), b"first");
        assert_eq!(fs::read(&second_outside).unwrap(), b"second");
        fs::remove_file(folder.join("libraries/NeuralNetwork")).unwrap();
        fs::remove_dir_all(folder).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn repair_uvcd_content_enables_mjpg_and_disables_other_uvcd_formats() {
        let original = "\
#define VIDEO_FHD_WIDTH_UVCD  1920
#define UVCD_YUY2 1
#define UVCD_NV12 1
#define UVCD_MJPG 1
#define UVCD_H264 1
#define UVCD_H265 1
";

        let repaired = repair_uvcd_content(original, "MJPG").expect("uvcd content should repair");

        assert!(repaired.contains("#define VIDEO_FHD_WIDTH_UVCD  1920"));
        assert!(repaired.contains("#define UVCD_YUY2 0"));
        assert!(repaired.contains("#define UVCD_NV12 0"));
        assert!(repaired.contains("#define UVCD_MJPG 1"));
        assert!(repaired.contains("#define UVCD_H264 0"));
        assert!(repaired.contains("#define UVCD_H265 0"));
    }

    #[test]
    fn repair_uvcd_content_enables_selected_format() {
        let original = "\
#define UVCD_YUY2 1
#define UVCD_NV12 1
#define UVCD_MJPG 1
#define UVCD_H264 0
#define UVCD_H265 0
";

        let repaired = repair_uvcd_content(original, "H264").expect("uvcd content should repair");

        assert!(repaired.contains("#define UVCD_YUY2 0"));
        assert!(repaired.contains("#define UVCD_NV12 0"));
        assert!(repaired.contains("#define UVCD_MJPG 0"));
        assert!(repaired.contains("#define UVCD_H264 1"));
        assert!(repaired.contains("#define UVCD_H265 0"));
    }

    #[test]
    fn normalize_uvcd_format_accepts_yuy2() {
        assert_eq!(normalize_uvcd_format("YUY2").unwrap(), "YUY2");
    }

    #[test]
    fn preference_url_uses_beta_by_default_and_release_when_selected() {
        assert_eq!(
            preference_url(DEFAULT_PREFERENCE_VERSION).unwrap(),
            "https://github.com/Ameba-AIoT/ameba-arduino-pro2/raw/dev/Arduino_package/package_realtek_amebapro2_early_index.json"
        );
        assert_eq!(
            preference_url("release").unwrap(),
            "https://github.com/ambiot/ambpro2_arduino/raw/main/Arduino_package/package_realtek_amebapro2_index.json"
        );
    }

    #[test]
    fn endpoint_manifest_pins_verified_installer_fallbacks() {
        let manifest = endpoint_manifest().expect("endpoint manifest should parse");

        assert_eq!(
            manifest.downloads.arduino_ide.urls,
            ["https://github.com/arduino/arduino-ide/releases/download/2.3.10/arduino-ide_2.3.10_Windows_64bit.exe"]
        );
        assert_eq!(
            manifest.downloads.arduino_ide.sha256,
            "a8f3df0ac57c6b74aa1b1d22e5f202dc5ddb46663579d4e5108a69cc99b6f823"
        );
        assert_eq!(manifest.downloads.arduino_ide.size, 158_074_632);
        assert_eq!(
            manifest.downloads.arduino_ide_msi.urls,
            ["https://github.com/arduino/arduino-ide/releases/download/2.3.10/arduino-ide_2.3.10_Windows_64bit.msi"]
        );
        assert_eq!(
            manifest.downloads.arduino_ide_msi.sha256,
            "aebbd1efeac5cfb02a6cad0d93af8221054fc983a5b3c5ce6da8a6bfb9425165"
        );
        assert_eq!(manifest.downloads.arduino_ide_msi.size, 167_145_472);
        assert_eq!(
            manifest.downloads.vlc.sha256,
            "ecc17f097ee0801f04faabb5ef9992ff00ea4c98c8fa005f6508ee74b41b6a53"
        );
        assert_eq!(manifest.downloads.vlc.size, 44_568_024);
    }

    #[test]
    fn installer_cache_keys_separate_arduino_formats_and_share_vlc() {
        let root = Path::new("cache");
        let arduino_exe = installer_cache_paths_from_root(root, InstallerCacheKey::ArduinoIdeExe);
        let arduino_msi = installer_cache_paths_from_root(root, InstallerCacheKey::ArduinoIdeMsi);
        let vlc_manual = installer_cache_paths_from_root(root, InstallerCacheKey::VlcExe);
        let vlc_auto = installer_cache_paths_from_root(root, InstallerCacheKey::VlcExe);
        let first_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let second_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let arduino_exe_payload = arduino_exe.payload(InstallerCacheKey::ArduinoIdeExe, first_sha);
        let arduino_msi_payload = arduino_msi.payload(InstallerCacheKey::ArduinoIdeMsi, first_sha);
        let vlc_manual_payload = vlc_manual.payload(InstallerCacheKey::VlcExe, first_sha);
        let vlc_auto_payload = vlc_auto.payload(InstallerCacheKey::VlcExe, first_sha);

        assert_ne!(arduino_exe_payload, arduino_msi_payload);
        assert_ne!(arduino_exe.metadata, arduino_msi.metadata);
        assert_eq!(vlc_manual_payload, vlc_auto_payload);
        assert_eq!(vlc_manual.metadata, vlc_auto.metadata);
        assert_ne!(
            arduino_exe.payload(InstallerCacheKey::ArduinoIdeExe, first_sha),
            arduino_exe.payload(InstallerCacheKey::ArduinoIdeExe, second_sha)
        );
        assert_eq!(
            arduino_exe.metadata,
            installer_cache_paths_from_root(root, InstallerCacheKey::ArduinoIdeExe).metadata
        );
        assert!(arduino_exe_payload
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(first_sha));
        assert_eq!(
            InstallerCacheKey::ArduinoIdeExe.progress_key(),
            InstallerCacheKey::ArduinoIdeMsi.progress_key()
        );
        assert_eq!(InstallerCacheKey::VlcExe.progress_key(), "vlc");
    }

    #[test]
    fn sha256_reader_matches_known_vector() {
        let (bytes, digest) = sha256_reader(std::io::Cursor::new(b"abc")).unwrap();

        assert_eq!(bytes, 3);
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn github_asset_selection_requires_exact_arduino_name_digest_size_and_url() {
        let tag = "2.3.10";
        let exe_name = "arduino-ide_2.3.10_Windows_64bit.exe";
        let msi_name = "arduino-ide_2.3.10_Windows_64bit.msi";
        let exe_url =
            format!("https://github.com/arduino/arduino-ide/releases/download/{tag}/{exe_name}");
        let msi_url =
            format!("https://github.com/arduino/arduino-ide/releases/download/{tag}/{msi_name}");
        let release = GithubRelease {
            tag_name: tag.to_string(),
            assets: vec![
                github_asset(
                    exe_name,
                    158_074_632,
                    "sha256:a8f3df0ac57c6b74aa1b1d22e5f202dc5ddb46663579d4e5108a69cc99b6f823",
                    &exe_url,
                ),
                github_asset(
                    msi_name,
                    167_145_472,
                    "sha256:aebbd1efeac5cfb02a6cad0d93af8221054fc983a5b3c5ce6da8a6bfb9425165",
                    &msi_url,
                ),
            ],
        };

        let exe = select_github_arduino_asset(&release, InstallerCacheKey::ArduinoIdeExe).unwrap();
        let msi = select_github_arduino_asset(&release, InstallerCacheKey::ArduinoIdeMsi).unwrap();

        assert_eq!(exe.file_name, exe_name);
        assert_eq!(exe.urls, [exe_url]);
        assert_eq!(exe.size, 158_074_632);
        assert_eq!(
            exe.sha256,
            "a8f3df0ac57c6b74aa1b1d22e5f202dc5ddb46663579d4e5108a69cc99b6f823"
        );
        assert_eq!(msi.file_name, msi_name);
        assert_eq!(msi.urls, [msi_url]);
        assert_eq!(msi.size, 167_145_472);
    }

    #[test]
    fn github_asset_selection_rejects_near_matches_and_untrusted_metadata() {
        let exact_name = "arduino-ide_2.3.10_Windows_64bit.exe";
        let exact_url =
            format!("https://github.com/arduino/arduino-ide/releases/download/2.3.10/{exact_name}");
        let digest = "sha256:a8f3df0ac57c6b74aa1b1d22e5f202dc5ddb46663579d4e5108a69cc99b6f823";
        let near_match = GithubRelease {
            tag_name: "2.3.10".into(),
            assets: vec![github_asset(
                "arduino-ide_2.3.10_Windows_64bit_portable.exe",
                158_074_632,
                digest,
                &exact_url,
            )],
        };
        assert!(
            select_github_arduino_asset(&near_match, InstallerCacheKey::ArduinoIdeExe).is_err()
        );

        let wrong_url = GithubRelease {
            tag_name: "2.3.10".into(),
            assets: vec![github_asset(
                exact_name,
                158_074_632,
                digest,
                "https://example.com/arduino.exe",
            )],
        };
        assert!(select_github_arduino_asset(&wrong_url, InstallerCacheKey::ArduinoIdeExe).is_err());

        let missing_digest = GithubRelease {
            tag_name: "2.3.10".into(),
            assets: vec![GithubReleaseAsset {
                name: exact_name.into(),
                size: 158_074_632,
                digest: None,
                browser_download_url: exact_url,
            }],
        };
        assert!(
            select_github_arduino_asset(&missing_digest, InstallerCacheKey::ArduinoIdeExe).is_err()
        );
    }

    #[test]
    fn external_url_allow_list_accepts_expected_https_hosts() {
        let urls = [
            "https://github.com/breeze0305/Realtek_AMB82mini_plugin",
            "https://raw.githubusercontent.com/breeze0305/Realtek_AMB82mini_plugin/main/version.txt",
            "https://downloads.arduino.cc/arduino-ide/arduino-ide_latest_Windows_64bit.exe",
            "https://get.videolan.org/vlc/3.0.23/win32/vlc-3.0.23-win32.exe",
            "https://mirror.twds.com.tw/videolan/vlc/3.0.23/win32/vlc-3.0.23-win32.exe",
            "https://modelconverter.ntnu-aiot.com/api/v1/conversions/123/download",
            "https://www.youtube.com/watch?v=UpYyOiEFA0k",
        ];

        for url in urls {
            assert!(is_allowed_external_url(url), "{url} should be allowed");
        }
    }

    #[test]
    fn external_url_allow_list_rejects_http() {
        assert!(!is_allowed_external_url("http://github.com/breeze0305"));
        assert!(!is_allowed_external_url(
            "http://www.youtube.com/watch?v=UpYyOiEFA0k"
        ));
    }

    #[test]
    fn external_url_allow_list_rejects_similar_malicious_hosts() {
        let urls = [
            "https://github.com.evil.com/breeze0305",
            "https://raw.githubusercontent.com.evil.com/version.txt",
            "https://downloads.arduino.cc.evil.com/arduino.exe",
            "https://github.com@evil.com/breeze0305",
            "https://www.youtube.com.evil.com/watch?v=UpYyOiEFA0k",
            "https://www.youtube.com@evil.com/watch?v=UpYyOiEFA0k",
        ];

        for url in urls {
            assert!(!is_allowed_external_url(url), "{url} should be rejected");
        }
    }

    #[test]
    fn external_url_allow_list_rejects_invalid_urls() {
        let urls = [
            "",
            "not a url",
            "https://",
            "https:///path",
            "https://github.com.",
            "https://github.com:bad/path",
            "https://[::1]/",
        ];

        for url in urls {
            assert!(!is_allowed_external_url(url), "{url} should be rejected");
        }
    }

    #[test]
    fn compare_version_numbers_checks_major_minor_then_patch() {
        assert_eq!(
            compare_version_numbers("3.9.1", "3.8.0"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_version_numbers("3.9.1", "3.10.0"),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_version_numbers("4.0.0", "3.9.9"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_version_numbers("3.9.1", "3.9.1"),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn safe_file_name_strips_parent_paths() {
        assert_eq!(safe_file_name("../model.nb"), "model.nb");
        assert_eq!(safe_file_name(""), "converted_model.nb");
    }

    #[test]
    fn native_dialog_lock_rejects_overlapping_dialogs() {
        let state = AppState {
            settings: Mutex::new(Settings::default()),
            output_folder: Mutex::new(None),
            capture_lock: Mutex::new(()),
            native_dialog_lock: Mutex::new(()),
            image_processing_lock: Arc::new(Mutex::new(())),
            arduino_installer_lock: Mutex::new(()),
            vlc_installer_lock: Mutex::new(()),
        };

        let first_dialog = lock_native_dialog(&state).unwrap();
        let second_dialog = lock_native_dialog(&state);
        assert!(matches!(
            second_dialog,
            Err(AppError::Message(message)) if message == "A native file dialog is already open"
        ));

        drop(first_dialog);
        assert!(lock_native_dialog(&state).is_ok());
    }

    #[test]
    fn download_length_validation_rejects_incomplete_files() {
        assert!(validate_download_length(512, Some(512)).is_ok());
        assert!(validate_download_length(512, None).is_ok());
        assert!(validate_download_length(511, Some(512)).is_err());
        assert!(validate_download_length(513, Some(512)).is_err());
    }

    #[test]
    fn verified_content_length_is_rejected_before_streaming_when_it_differs() {
        let metadata = metadata_for_bytes("installer.exe", b"12345");

        assert_eq!(
            validated_content_length(Some("5"), Some(&metadata)).unwrap(),
            Some(5)
        );
        assert_eq!(
            validated_content_length(None, Some(&metadata)).unwrap(),
            None
        );
        assert!(validated_content_length(Some("4"), Some(&metadata)).is_err());
        assert!(validated_content_length(Some("6"), Some(&metadata)).is_err());
        assert!(validated_content_length(Some("invalid"), Some(&metadata)).is_err());
    }

    #[test]
    fn verified_stream_aborts_on_first_excess_bytes_and_removes_part() {
        let folder = test_directory("installer-stream-limit");
        fs::create_dir_all(&folder).unwrap();
        let target = folder.join("installer.exe");
        let metadata = metadata_for_bytes("installer.exe", b"12345");
        fs::write(&target, b"preserve old installer").unwrap();

        let result = write_stream_atomically(
            std::io::Cursor::new(b"123456"),
            &target,
            None,
            Some(&metadata),
            |_| panic!("oversized chunk must not be committed"),
        );

        assert!(result.is_err());
        assert_eq!(fs::read(&target).unwrap(), b"preserve old installer");
        assert_eq!(
            fs::read_dir(&folder)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".part"))
                .count(),
            0
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn completed_download_replaces_the_target_file() {
        let folder = test_directory("download-replace");
        fs::create_dir_all(&folder).unwrap();
        let source = folder.join("payload.part");
        let target = folder.join("payload.bin");
        fs::write(&source, b"new payload").unwrap();
        fs::write(&target, b"old payload").unwrap();

        replace_downloaded_file(&source, &target).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new payload");
        assert!(!source.exists());
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn verified_file_detects_cached_installer_tampering() {
        let folder = test_directory("installer-tamper");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("installer.exe");
        let metadata = metadata_for_bytes("installer.exe", b"trusted installer");
        fs::write(&path, b"trusted installer").unwrap();

        assert_eq!(
            verified_file_size(&path, &metadata).unwrap(),
            Some(metadata.size)
        );

        fs::write(&path, b"altered installer").unwrap();
        assert_eq!(verified_file_size(&path, &metadata).unwrap(), None);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn verified_atomic_copy_replaces_target_only_after_hash_and_size_match() {
        let folder = test_directory("installer-copy");
        fs::create_dir_all(&folder).unwrap();
        let source = folder.join("cache.exe");
        let target = folder.join("saved.exe");
        let trusted = b"trusted installer payload";
        let metadata = metadata_for_bytes("saved.exe", trusted);
        fs::write(&source, trusted).unwrap();
        fs::write(&target, b"old target").unwrap();

        let copied = copy_verified_file_atomically(&source, &target, &metadata).unwrap();

        assert_eq!(copied, trusted.len() as u64);
        assert_eq!(fs::read(&target).unwrap(), trusted);

        fs::write(&source, b"tampered installer payload").unwrap();
        fs::write(&target, b"preserve this target").unwrap();
        assert!(copy_verified_file_atomically(&source, &target, &metadata).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"preserve this target");
        assert_eq!(
            fs::read_dir(&folder)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".part"))
                .count(),
            0
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn failed_cache_pointer_commit_preserves_old_cache_and_removes_new_payload() {
        let folder = test_directory("installer-pointer-failure");
        fs::create_dir_all(&folder).unwrap();
        let key = InstallerCacheKey::ArduinoIdeExe;
        let paths = installer_cache_paths_from_root(&folder, key);
        let previous = metadata_for_bytes("installer.exe", b"old trusted payload");
        let current = metadata_for_bytes("installer.exe", b"new trusted payload");
        let previous_payload = paths.payload(key, &previous.sha256);
        let current_payload = paths.payload(key, &current.sha256);
        let previous_pointer = serde_json::to_vec(&previous).unwrap();
        fs::write(&previous_payload, b"old trusted payload").unwrap();
        fs::write(&current_payload, b"new trusted payload").unwrap();
        fs::write(&paths.metadata, &previous_pointer).unwrap();

        let result = commit_installer_cache_pointer_with(
            &paths,
            key,
            Some(&previous),
            &current,
            true,
            || Err(AppError::Message("simulated pointer failure".into())),
        );

        assert!(result.is_err());
        assert_eq!(fs::read(&previous_payload).unwrap(), b"old trusted payload");
        assert_eq!(fs::read(&paths.metadata).unwrap(), previous_pointer);
        assert!(!current_payload.exists());
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn successful_cache_pointer_commit_cleans_previous_payload_after_switch() {
        let folder = test_directory("installer-pointer-success");
        fs::create_dir_all(&folder).unwrap();
        let key = InstallerCacheKey::ArduinoIdeExe;
        let paths = installer_cache_paths_from_root(&folder, key);
        let previous = metadata_for_bytes("installer.exe", b"old trusted payload");
        let current = metadata_for_bytes("installer.exe", b"new trusted payload");
        let previous_payload = paths.payload(key, &previous.sha256);
        let current_payload = paths.payload(key, &current.sha256);
        fs::write(&previous_payload, b"old trusted payload").unwrap();
        fs::write(&current_payload, b"new trusted payload").unwrap();
        fs::write(&paths.metadata, serde_json::to_vec(&previous).unwrap()).unwrap();

        commit_installer_cache_pointer_with(&paths, key, Some(&previous), &current, true, || {
            write_cached_installer_metadata(&paths, &current)
        })
        .unwrap();

        assert!(!previous_payload.exists());
        assert_eq!(fs::read(&current_payload).unwrap(), b"new trusted payload");
        assert_eq!(
            serde_json::from_slice::<InstallerMetadata>(&fs::read(&paths.metadata).unwrap())
                .unwrap(),
            current
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn camera_capture_uses_the_next_available_file_without_overwriting() {
        let folder = test_directory("capture-file");
        fs::create_dir_all(&folder).unwrap();
        let first = folder.join("image_00001.jpg");
        fs::write(&first, b"existing image").unwrap();

        let second = write_next_image(&folder, b"new image").unwrap();

        assert_eq!(second.file_name().unwrap(), "image_00002.jpg");
        assert_eq!(fs::read(first).unwrap(), b"existing image");
        assert_eq!(fs::read(second).unwrap(), b"new image");
        fs::remove_dir_all(folder).unwrap();
    }
}

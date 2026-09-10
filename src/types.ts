export type Language = "zh_TW" | "en_US" | "ja_JP";
export type ResourceCategory = "installers" | "weights";
export type View = "home" | ResourceCategory | "camera" | "settings" | "converter" | "annotator" | "image-converter";
export type UvcdFormat = "YUY2" | "NV12" | "MJPG" | "H264" | "H265";
export type PreferenceVersion = "release" | "beta";
export type ModelType = "yolo" | "classification";

export type Metadata = {
  author: string;
  contact: string;
  version: string;
  repository: string;
  arduino_ide_url: string;
  vlc_url: string;
  realtek_package_url: string;
  model_converter_url: string;
  model_converter_api_base: string;
  supported_languages: Language[];
};

export type AppSettings = {
  capture_interval: number;
  language: Language;
  uvcd_format: UvcdFormat;
  preference_version: PreferenceVersion;
};

export type Dashboard = {
  metadata: Metadata;
  settings: AppSettings;
  realtek_folder: string | null;
  output_folder: string;
  internet_connected: boolean;
};

export type ActionResult = {
  ok: boolean;
  message: string;
  path?: string | null;
};

export type DownloadResult = {
  file_name: string;
  path: string;
  bytes: number;
};

export type DownloadKey = "arduino" | "vlc" | "converter";

export type DownloadProgress = {
  key: DownloadKey;
  downloaded: number;
  total?: number | null;
};

export type ArduinoCliStatus = {
  status: "not_installed" | "not_on_path" | "on_path";
  cli_path: string | null;
};

export type InstallationResult = {
  path: string;
  reboot_required: boolean;
  arduino_cli: ArduinoCliStatus | null;
  path_error: string | null;
};

export type InstallerProgress = {
  key: "arduino" | "vlc";
  phase: "installing" | "configuring_path";
};

export type VersionCheck = {
  local: string;
  remote: string;
  is_latest: boolean;
  is_beta: boolean;
  repository: string;
};

export type UvcdResult = {
  changed: boolean;
  message: string;
  path?: string | null;
  format: UvcdFormat;
};

export type SettingsResetResult = {
  dashboard: Dashboard;
  uvcd: UvcdResult;
};

export type WeightCleanupResult = {
  deleted: number;
  missing: number;
  folder: string;
};

export type ConverterModel = {
  type: ModelType;
  label: string;
  input_extensions: string[];
  download_name: string;
};

export type ConverterModelsResponse = {
  models: ConverterModel[];
  max_file_size_mb: number;
};

export type ConversionCreateResponse = {
  task_id: string;
  status: ConversionStatus;
  status_url: string;
  download_url: string;
  expires_in_seconds?: number;
};

export type ConversionStatus = "queued" | "running" | "success" | "failed" | "expired";

export type ConversionStatusResponse = {
  task_id: string;
  status: ConversionStatus;
  model_type: ModelType;
  original_filename: string;
  download_name: string;
  download_url?: string;
  error?: {
    code: string;
    message: string;
  };
};

export type CompletedConversion = {
  downloadUrl: string;
  fileName: string;
};

export type AnnotationBox = {
  class_id: number;
  x_center: number;
  y_center: number;
  width: number;
  height: number;
};

export type AnnotationImage = {
  name: string;
  path: string;
  annotation_count: number;
};

export type AnnotationWorkspace = {
  image_folder: string;
  labels_folder: string;
  images: AnnotationImage[];
  classes: string[];
  annotations: Record<string, AnnotationBox[]>;
  invalid_class_ids: number[];
};

export type AnnotationLoadProgress = {
  phase: "discovering" | "normalizing" | "loading" | "complete";
  processed: number;
  total: number;
  corrected: number;
  failed: number;
  current_file: string | null;
};

export type AnnotationLoadSummary = {
  total: number;
  corrected: number;
  failed: number;
  failed_files: string[];
};

export type AnnotationLoadResult = {
  workspace: AnnotationWorkspace;
  summary: AnnotationLoadSummary;
};

export type AnnotationImageData = {
  mime: string;
  bytes: number[];
};

export type AnnotationSaveResult = {
  path: string;
  count: number;
};

export type ImageConversionProgress = {
  phase: "discovering" | "converting" | "complete";
  processed: number;
  total: number;
  converted: number;
  normalized: number;
  skipped: number;
  failed: number;
  current_file: string | null;
};

export type ImageConversionSummary = {
  total: number;
  converted: number;
  normalized: number;
  skipped: number;
  failed: number;
  failed_files: string[];
};

export type RunningAction =
  | "driver"
  | "hand"
  | "box"
  | "japan"
  | "taiwan"
  | "singapore"
  | "arduino"
  | "arduinoCliPath"
  | "vlc"
  | "folder"
  | "settings"
  | "weightCleanup"
  | "version"
  | "output"
  | "converter"
  | null;

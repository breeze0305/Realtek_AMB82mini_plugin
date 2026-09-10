import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";

import {
  AUTO_UPDATE_CHECK_STORAGE_KEY,
  converterModelDefaults,
  INSTALLER_TOAST_DISPLAY_MS,
  RELEASES_URL,
  TOAST_DISPLAY_MS,
  TOAST_FADE_MS,
  VERSION_CHECK_STORAGE_KEY,
  uvcdFormatOptions,
} from "./appConfig";
import { cameraGuideSteps, PREFERENCE_COPY_MESSAGE, translations } from "./i18n";
import { createOperationGate } from "./operationGate";
import { createSerialTaskScheduler, type SerialTaskScheduler } from "./serialTaskScheduler";
import {
  converterApiUrl,
  fileMatchesExtensions,
  fileNameFromPath,
  fileNameMatchesExtensions,
  readApiJson,
  savedPhotoText,
  wait,
} from "./converterUtils";
import { AppHeader } from "./components/AppHeader";
import { AnnotationView } from "./components/AnnotationView";
import { CameraView } from "./components/CameraView";
import { ConverterView } from "./components/ConverterView";
import { HomeView } from "./components/HomeView";
import { ImageConversionView } from "./components/ImageConversionView";
import { LinkPanel } from "./components/LinkPanel";
import { NetworkStatus } from "./components/NetworkStatus";
import { ResourceLibraryView } from "./components/ResourceLibraryView";
import { SettingsView } from "./components/SettingsView";
import { createHomeCardGroups } from "./homeCards";
import { useArduinoCliStatus } from "./useArduinoCliStatus";
import type {
  ActionResult,
  AppSettings,
  CompletedConversion,
  ConversionCreateResponse,
  ConversionStatusResponse,
  ConverterModel,
  ConverterModelsResponse,
  Dashboard,
  DownloadKey,
  DownloadProgress,
  DownloadResult,
  InstallerProgress,
  Language,
  ModelType,
  PreferenceVersion,
  ResourceCategory,
  RunningAction,
  SettingsResetResult,
  UvcdFormat,
  UvcdResult,
  VersionCheck,
  View,
  WeightCleanupResult,
} from "./types";

function isStoredVersionCheck(value: unknown): value is VersionCheck {
  if (!value || typeof value !== "object") return false;
  const data = value as Partial<VersionCheck>;
  return (
    typeof data.local === "string" &&
    typeof data.remote === "string" &&
    typeof data.is_latest === "boolean" &&
    typeof data.is_beta === "boolean" &&
    typeof data.repository === "string"
  );
}

function readStoredVersionCheck(currentVersion: string) {
  try {
    const raw = window.localStorage.getItem(VERSION_CHECK_STORAGE_KEY);
    if (!raw) return null;

    const data = JSON.parse(raw) as unknown;
    if (isStoredVersionCheck(data) && data.local === currentVersion) {
      return data;
    }

    window.localStorage.removeItem(VERSION_CHECK_STORAGE_KEY);
  } catch {
    window.localStorage.removeItem(VERSION_CHECK_STORAGE_KEY);
  }

  return null;
}

function writeStoredVersionCheck(result: VersionCheck) {
  try {
    window.localStorage.setItem(VERSION_CHECK_STORAGE_KEY, JSON.stringify(result));
  } catch {
    // The in-memory state still updates if local storage is unavailable.
  }
}

function readStoredAutoCheckUpdates() {
  try {
    const raw = window.localStorage.getItem(AUTO_UPDATE_CHECK_STORAGE_KEY);
    return raw === null ? true : raw === "true";
  } catch {
    return true;
  }
}

function writeStoredAutoCheckUpdates(enabled: boolean) {
  try {
    window.localStorage.setItem(AUTO_UPDATE_CHECK_STORAGE_KEY, String(enabled));
  } catch {
    // The setting remains active for the current session if local storage is unavailable.
  }
}

function App() {
  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [view, setView] = useState<View>("home");
  const [running, setRunning] = useState<RunningAction>(null);
  const [isNativeDialogOpen, setIsNativeDialogOpen] = useState(false);
  const [status, setStatus] = useState("");
  const [installerFeedback, setInstallerFeedback] = useState("");
  const [isFeedbackLeaving, setIsFeedbackLeaving] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<Partial<Record<DownloadKey, number>>>({});
  const [installerProgress, setInstallerProgress] = useState<
    Partial<Record<"arduino" | "vlc", InstallerProgress["phase"]>>
  >({});
  const arduinoCli = useArduinoCliStatus(view === "installers", running === "arduino" || running === "arduinoCliPath");
  const [internetConnected, setInternetConnected] = useState(false);
  const [cameras, setCameras] = useState<MediaDeviceInfo[]>([]);
  const [selectedCamera, setSelectedCamera] = useState("");
  const [isPreviewing, setIsPreviewing] = useState(false);
  const [isCapturing, setIsCapturing] = useState(false);
  const [isCameraBusy, setIsCameraBusy] = useState(false);
  const [cameraOperationGate] = useState(() => createOperationGate(setIsCameraBusy));
  const [isLanguageMenuOpen, setIsLanguageMenuOpen] = useState(false);
  const [openActionMenu, setOpenActionMenu] = useState<string | null>(null);
  const [lastSaved, setLastSaved] = useState("");
  const [converterModels, setConverterModels] = useState<Record<ModelType, ConverterModel>>(converterModelDefaults);
  const [converterMaxFileSizeMb, setConverterMaxFileSizeMb] = useState(120);
  const [converterType, setConverterType] = useState<ModelType>("yolo");
  const [converterFile, setConverterFile] = useState<File | null>(null);
  const [converterTask, setConverterTask] = useState<ConversionStatusResponse | null>(null);
  const [completedConversion, setCompletedConversion] = useState<CompletedConversion | null>(null);
  const [converterStatus, setConverterStatus] = useState("");
  const [isConverterBusy, setIsConverterBusy] = useState(false);
  const [isConverterFileLoading, setIsConverterFileLoading] = useState(false);
  const [versionCheck, setVersionCheck] = useState<VersionCheck | null>(null);
  const [autoCheckUpdates, setAutoCheckUpdates] = useState(readStoredAutoCheckUpdates);
  const languageMenuRef = useRef<HTMLDivElement | null>(null);
  const converterInputRef = useRef<HTMLInputElement | null>(null);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const cameraSessionIdRef = useRef(0);
  const captureSchedulerRef = useRef<SerialTaskScheduler | null>(null);
  const nativeDialogOpenRef = useRef(false);
  const autoCheckStartedRef = useRef(false);
  const converterAbortRef = useRef<AbortController | null>(null);
  const converterRunIdRef = useRef(0);
  const converterFileLoadIdRef = useRef(0);

  const language = dashboard?.settings.language ?? "zh_TW";
  const t = translations[language];
  const selectedUvcdFormat = dashboard?.settings.uvcd_format ?? "MJPG";
  const selectedPreferenceVersion = dashboard?.settings.preference_version ?? "beta";
  const selectedConverterModel = converterModels[converterType];
  const converterExtensions = selectedConverterModel.input_extensions.join(", ");
  const modelConverterUrl = dashboard?.metadata.model_converter_url ?? "";
  const modelConverterApiBase = dashboard?.metadata.model_converter_api_base ?? "";

  useEffect(() => {
    void refreshDashboard();
    const timer = window.setInterval(() => void refreshInternet(), 30000);
    let disposed = false;
    let unlistenDownloadProgress: (() => void) | undefined;
    let unlistenInstallerProgress: (() => void) | undefined;
    let unlistenNativeDialogState: (() => void) | undefined;

    void listen<DownloadProgress>("download-progress", (event) => {
      const { key, downloaded, total } = event.payload;
      const progress = total && total > 0 ? Math.min(downloaded / total, 1) : 0.08;
      setDownloadProgress((current) => ({ ...current, [key]: progress }));
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
      } else {
        unlistenDownloadProgress = nextUnlisten;
      }
    });

    void listen<InstallerProgress>("installer-progress", (event) => {
      const { key, phase } = event.payload;
      setInstallerProgress((current) => ({ ...current, [key]: phase }));
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
      } else {
        unlistenInstallerProgress = nextUnlisten;
      }
    });

    void listen<boolean>("native-dialog-state", (event) => {
      nativeDialogOpenRef.current = event.payload;
      setIsNativeDialogOpen(event.payload);
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
      } else {
        unlistenNativeDialogState = nextUnlisten;
      }
    });

    return () => {
      disposed = true;
      unlistenDownloadProgress?.();
      unlistenInstallerProgress?.();
      unlistenNativeDialogState?.();
      window.clearInterval(timer);
      cancelModelConversionRequest({ resetUi: false });
      stopCamera();
    };
    // This effect owns process-lifetime subscriptions and teardown and must only run once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!status) return;
    setIsFeedbackLeaving(false);
    const displayMs =
      status === installerFeedback
        ? INSTALLER_TOAST_DISPLAY_MS
        : status === PREFERENCE_COPY_MESSAGE
          ? TOAST_DISPLAY_MS * 2
          : TOAST_DISPLAY_MS;
    const leaveTimer = window.setTimeout(() => setIsFeedbackLeaving(true), displayMs);
    const clearTimer = window.setTimeout(() => setStatus(""), displayMs + TOAST_FADE_MS);
    return () => {
      window.clearTimeout(leaveTimer);
      window.clearTimeout(clearTimer);
    };
  }, [status, installerFeedback]);

  useEffect(() => {
    if (view === "camera") {
      void scanCameras();
    }
    if (view === "converter" && modelConverterApiBase) {
      void loadConverterModels();
    }
    if (view !== "converter") {
      cancelModelConversionRequest();
      cancelConverterFileLoad();
    }
    return () => {
      if (view === "camera") {
        stopCamera();
      }
    };
    // View and endpoint changes are the only events that should trigger these transitions.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view, modelConverterApiBase]);

  async function refreshDashboard() {
    const data = await invoke<Dashboard>("get_dashboard");
    setDashboard(data);
    setInternetConnected(data.internet_connected);
    setVersionCheck(readStoredVersionCheck(data.metadata.version));
    setStatus("");
    if (autoCheckUpdates && !autoCheckStartedRef.current) {
      autoCheckStartedRef.current = true;
      void checkVersionOnStartup(data.settings.language);
    }
  }

  async function refreshInternet() {
    const online = await invoke<boolean>("check_internet");
    setInternetConnected(online);
  }

  async function changeLanguage(language: Language) {
    const next = await invoke<AppSettings>("set_language", { language });
    setDashboard((current) => (current ? { ...current, settings: next } : current));
    setIsLanguageMenuOpen(false);
    setStatus("");
  }

  async function changeUvcdFormat(format: UvcdFormat) {
    try {
      setRunning("settings");
      const result = await invoke<UvcdResult>("set_uvcd_format", { format });
      setDashboard((current) =>
        current
          ? {
              ...current,
              settings: {
                ...current.settings,
                uvcd_format: result.format,
              },
            }
          : current,
      );
      const label = uvcdFormatOptions.find((item) => item.value === result.format)?.label ?? result.format;
      setStatus(result.path ? `${t.uvcdSaved}: ${label}` : result.message);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setRunning(null);
    }
  }

  async function changePreferenceVersion(version: PreferenceVersion) {
    if (version === selectedPreferenceVersion) return;

    try {
      setRunning("settings");
      const next = await invoke<Dashboard>("set_preference_version", { version });
      setDashboard(next);
      setInternetConnected(next.internet_connected);
      setStatus(`${t.preferenceSaved}: ${version === "beta" ? t.betaVersion : t.releaseVersion}`);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setRunning(null);
    }
  }

  async function resetSettings() {
    try {
      setRunning("settings");
      const result = await invoke<SettingsResetResult>("reset_settings");
      setDashboard(result.dashboard);
      setInternetConnected(result.dashboard.internet_connected);
      changeAutoCheckUpdates(true);
      setStatus(result.uvcd.path ? t.settingsReset : `${t.settingsReset}: ${result.uvcd.message}`);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setRunning(null);
    }
  }

  async function clearWeightRecords() {
    try {
      setRunning("weightCleanup");
      const result = await invoke<WeightCleanupResult>("clear_installed_weights");
      setStatus(
        result.deleted > 0
          ? t.weightRecordsCleared.replace("{count}", String(result.deleted))
          : t.weightRecordsAlreadyClear,
      );
    } catch (error) {
      setStatus(String(error));
    } finally {
      setRunning(null);
    }
  }

  async function copyText(text?: string, message = t.copied) {
    if (!text) return;
    await navigator.clipboard.writeText(text);
    setStatus(message);
  }

  async function openUrl(url?: string) {
    if (!url) return;
    try {
      await invoke<ActionResult>("open_url", { url });
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function runAction<T>(key: Exclude<RunningAction, null>, command: string, next: (result: T) => string) {
    const isInstallerAction =
      command === "download_and_install_arduino_ide" ||
      command === "download_and_install_vlc" ||
      command === "add_arduino_cli_to_path";
    function showResult(message: string) {
      setInstallerFeedback(isInstallerAction ? message : "");
      setStatus(message);
    }
    try {
      setOpenActionMenu(null);
      setRunning(key);
      if (isDownloadKey(key)) {
        setDownloadProgress((current) => ({ ...current, [key]: 0.02 }));
      }
      const result = await invoke<T>(command);
      showResult(next(result));
      if (command === "check_version") {
        await refreshInternet();
      }
    } catch (error) {
      showResult(String(error));
    } finally {
      setRunning(null);
      if (key === "arduino" || key === "vlc") {
        setInstallerProgress((current) => {
          const nextProgress = { ...current };
          delete nextProgress[key];
          return nextProgress;
        });
      }
      if (isDownloadKey(key)) {
        window.setTimeout(() => {
          setDownloadProgress((current) => {
            const nextProgress = { ...current };
            delete nextProgress[key];
            return nextProgress;
          });
        }, 500);
      }
    }
  }

  async function selectOutputFolder() {
    if (nativeDialogOpenRef.current || cameraOperationGate.isBusy() || captureSchedulerRef.current?.isActive()) {
      return;
    }

    try {
      const result = await invoke<ActionResult>("select_output_folder");
      setDashboard((current) =>
        current ? { ...current, output_folder: result.path ?? current.output_folder } : current,
      );
      setStatus(result.path ?? result.message);
    } catch (error) {
      setStatus(String(error));
    }
  }

  function isDownloadKey(key: RunningAction): key is DownloadKey {
    return key === "arduino" || key === "vlc" || key === "converter";
  }

  function openCameraView() {
    cancelModelConversionRequest();
    stopCamera();
    setCameras([]);
    setSelectedCamera("");
    setLastSaved("");
    setView("camera");
  }

  function refreshCameras() {
    if (nativeDialogOpenRef.current || cameraOperationGate.isBusy() || captureSchedulerRef.current?.isActive()) {
      return;
    }
    void scanCameras();
  }

  function openResourcesView(category: ResourceCategory) {
    cancelModelConversionRequest();
    stopCamera();
    setOpenActionMenu(null);
    setView(category);
  }

  function openSettingsView() {
    cancelModelConversionRequest();
    stopCamera();
    setOpenActionMenu(null);
    setView("settings");
  }

  function openConverterView() {
    stopCamera();
    setConverterStatus("");
    setConverterTask(null);
    setView("converter");
  }

  function openAnnotationView() {
    cancelModelConversionRequest();
    stopCamera();
    setView("annotator");
  }

  function openImageConverterView() {
    cancelModelConversionRequest();
    stopCamera();
    setView("image-converter");
  }

  async function loadConverterModels() {
    try {
      if (!modelConverterApiBase) {
        throw new Error("Model converter endpoint is not configured");
      }
      setConverterStatus((current) => current || t.loadingModels);
      const response = await fetch(`${modelConverterApiBase}/models`);
      const data = await readApiJson<ConverterModelsResponse>(response);
      const nextModels = data.models.reduce<Record<ModelType, ConverterModel>>(
        (models, model) => {
          if (model.type === "yolo" || model.type === "classification") {
            models[model.type] = model;
          }
          return models;
        },
        { ...converterModelDefaults },
      );
      setConverterModels(nextModels);
      setConverterMaxFileSizeMb(data.max_file_size_mb || 120);
      setConverterStatus("");
    } catch (error) {
      setConverterModels(converterModelDefaults);
      setConverterStatus(String(error));
    }
  }

  function rememberVersionCheck(result: VersionCheck) {
    writeStoredVersionCheck(result);
    setVersionCheck(result);
  }

  function changeAutoCheckUpdates(enabled: boolean) {
    writeStoredAutoCheckUpdates(enabled);
    setAutoCheckUpdates(enabled);
  }

  function clearConverterProgress() {
    setDownloadProgress((current) => {
      const nextProgress = { ...current };
      delete nextProgress.converter;
      return nextProgress;
    });
  }

  function cancelModelConversionRequest({ resetUi = true } = {}) {
    if (converterAbortRef.current) {
      converterAbortRef.current.abort();
      converterAbortRef.current = null;
      converterRunIdRef.current += 1;
    }

    if (resetUi) {
      setIsConverterBusy(false);
      clearConverterProgress();
    }
  }

  function cancelConverterFileLoad() {
    converterFileLoadIdRef.current += 1;
    setIsConverterFileLoading(false);
  }

  function isAbortError(error: unknown) {
    return error instanceof DOMException && error.name === "AbortError";
  }

  async function checkVersionOnStartup(language: Language) {
    try {
      const result = await invoke<VersionCheck>("check_version");
      rememberVersionCheck(result);
      if (!result.is_latest && !result.is_beta) {
        setStatus(`${translations[language].update}: ${result.remote}`);
      }
      await refreshInternet();
    } catch {
      // Startup checks should not interrupt users when the network is unavailable.
    }
  }

  function selectConverterType(type: ModelType) {
    if (type === converterType) return;
    cancelModelConversionRequest();
    cancelConverterFileLoad();
    setConverterType(type);
    setConverterFile(null);
    setConverterTask(null);
    setCompletedConversion(null);
    setConverterStatus("");
    if (converterInputRef.current) {
      converterInputRef.current.value = "";
    }
  }

  function chooseConverterFile(file?: File | null) {
    if (!file) return;
    cancelConverterFileLoad();
    applyConverterFile(file, converterModels[converterType]);
  }

  function applyConverterFile(file: File, model: ConverterModel) {
    cancelModelConversionRequest();
    setConverterTask(null);
    setCompletedConversion(null);
    if (!fileMatchesExtensions(file, model.input_extensions)) {
      setConverterFile(null);
      setConverterStatus(t.invalidFileType);
      return;
    }
    if (file.size > converterMaxFileSizeMb * 1024 * 1024) {
      setConverterFile(null);
      setConverterStatus(`File exceeds ${converterMaxFileSizeMb} MB`);
      return;
    }
    setConverterFile(file);
    setConverterStatus("");
  }

  async function chooseDroppedConverterFile(path: string) {
    const model = converterModels[converterType];
    const fileName = fileNameFromPath(path);
    if (!fileNameMatchesExtensions(fileName, model.input_extensions)) {
      cancelConverterFileLoad();
      cancelModelConversionRequest();
      setConverterFile(null);
      setConverterTask(null);
      setCompletedConversion(null);
      setConverterStatus(t.invalidFileType);
      return;
    }

    cancelModelConversionRequest();
    setConverterFile(null);
    setConverterTask(null);
    setCompletedConversion(null);
    setConverterStatus(t.loadingDroppedFile);
    const operationId = converterFileLoadIdRef.current + 1;
    converterFileLoadIdRef.current = operationId;
    setIsConverterFileLoading(true);

    try {
      const maxBytes = Math.floor(converterMaxFileSizeMb * 1024 * 1024);
      const bytes = await invoke<ArrayBuffer>("read_model_converter_file", { path, maxBytes });
      if (converterFileLoadIdRef.current !== operationId) return;
      applyConverterFile(new File([bytes], fileName, { type: "application/octet-stream" }), model);
    } catch (error) {
      if (converterFileLoadIdRef.current === operationId) {
        setConverterFile(null);
        setConverterStatus(String(error));
      }
    } finally {
      if (converterFileLoadIdRef.current === operationId) {
        setIsConverterFileLoading(false);
      }
    }
  }

  async function startModelConversion() {
    if (!converterFile) {
      setConverterStatus(t.noFileSelected);
      return;
    }

    const model = converterModels[converterType];
    if (!fileMatchesExtensions(converterFile, model.input_extensions)) {
      setConverterStatus(t.invalidFileType);
      return;
    }

    cancelModelConversionRequest({ resetUi: false });
    const controller = new AbortController();
    const signal = controller.signal;
    const runId = converterRunIdRef.current + 1;
    converterRunIdRef.current = runId;
    converterAbortRef.current = controller;
    const isCurrentRun = () =>
      converterAbortRef.current === controller && converterRunIdRef.current === runId && !signal.aborted;

    try {
      setIsConverterBusy(true);
      setDownloadProgress((current) => ({ ...current, converter: 0 }));
      setConverterStatus(t.uploadQueued);
      setConverterTask(null);
      setCompletedConversion(null);

      const form = new FormData();
      form.append("model_type", model.type);
      form.append("file", converterFile);
      if (!modelConverterApiBase) {
        throw new Error("Model converter endpoint is not configured");
      }
      const createResponse = await fetch(`${modelConverterApiBase}/conversions`, {
        method: "POST",
        body: form,
        signal,
      });
      const task = await readApiJson<ConversionCreateResponse>(createResponse);
      if (!isCurrentRun()) return;
      setConverterStatus(t.uploadQueued);

      let statusData: ConversionStatusResponse | null = null;
      for (let attempt = 0; attempt < 180; attempt += 1) {
        const statusResponse = await fetch(converterApiUrl(modelConverterApiBase, task.status_url), { signal });
        statusData = await readApiJson<ConversionStatusResponse>(statusResponse);
        if (!isCurrentRun()) return;
        setConverterTask(statusData);

        if (statusData.status === "success") break;
        if (statusData.status === "failed" || statusData.status === "expired") {
          throw new Error(statusData.error?.message || "Conversion failed");
        }

        setConverterStatus(statusData.status === "queued" ? t.uploadQueued : t.conversionRunning);
        await wait(2000, signal);
        if (!isCurrentRun()) return;
      }

      if (!isCurrentRun()) return;
      if (!statusData || statusData.status !== "success") {
        throw new Error("Conversion timed out");
      }

      setConverterStatus(t.conversionSuccess);
      const downloadUrl = converterApiUrl(modelConverterApiBase, statusData.download_url ?? task.download_url);
      setCompletedConversion({
        downloadUrl,
        fileName: statusData.download_name || model.download_name,
      });
    } catch (error) {
      if (signal.aborted || isAbortError(error) || !isCurrentRun()) return;
      setConverterStatus(String(error));
      setStatus(String(error));
    } finally {
      if (converterAbortRef.current === controller && converterRunIdRef.current === runId) {
        converterAbortRef.current = null;
        setIsConverterBusy(false);
        clearConverterProgress();
      }
    }
  }

  async function downloadCompletedConversion() {
    if (!completedConversion) return;

    try {
      setIsConverterBusy(true);
      setDownloadProgress((current) => ({ ...current, converter: 0.02 }));
      const result = await invoke<DownloadResult>("download_model_conversion_as", {
        url: completedConversion.downloadUrl,
        fileName: completedConversion.fileName,
      });
      setConverterStatus(`${t.conversionSaved}: ${result.path}`);
      setStatus(`${t.conversionSaved}: ${result.path}`);
    } catch (error) {
      setConverterStatus(String(error));
      setStatus(String(error));
    } finally {
      setIsConverterBusy(false);
      setDownloadProgress((current) => {
        const nextProgress = { ...current };
        delete nextProgress.converter;
        return nextProgress;
      });
    }
  }

  async function scanCameras() {
    if (nativeDialogOpenRef.current) return;
    const cameraSessionId = beginCameraSession();
    const finishCameraOperation = cameraOperationGate.begin();

    try {
      stopCaptureTimer();
      stopPreviewStream();
      const permissionStream = await navigator.mediaDevices.getUserMedia({ video: true });
      stopMediaStream(permissionStream);
      if (!isCurrentCameraSession(cameraSessionId) || nativeDialogOpenRef.current) return;
      const devices = await navigator.mediaDevices.enumerateDevices();
      if (!isCurrentCameraSession(cameraSessionId) || nativeDialogOpenRef.current) return;
      const videoDevices = devices.filter((device) => device.kind === "videoinput");
      const nextCamera =
        videoDevices.find((device) => device.deviceId === selectedCamera)?.deviceId || videoDevices[0]?.deviceId || "";
      setCameras(videoDevices);
      setSelectedCamera(nextCamera);
      setStatus(videoDevices.length ? `${videoDevices.length} camera(s)` : t.noCamera);
      if (nextCamera) await startPreview(nextCamera, cameraSessionId);
    } catch (error) {
      if (isCurrentCameraSession(cameraSessionId)) {
        setStatus(String(error));
      }
    } finally {
      finishCameraOperation();
    }
  }

  async function startPreview(deviceId = selectedCamera, existingCameraSessionId?: number) {
    if (nativeDialogOpenRef.current) return false;
    const cameraSessionId = existingCameraSessionId ?? beginCameraSession();
    if (!isCurrentCameraSession(cameraSessionId)) return false;

    stopCaptureTimer();
    stopPreviewStream();
    if (!deviceId) {
      setStatus(t.noCamera);
      return false;
    }

    const finishCameraOperation = cameraOperationGate.begin();
    let stream: MediaStream | null = null;
    let video: HTMLVideoElement | null = null;

    try {
      stream = await navigator.mediaDevices.getUserMedia({
        video: {
          deviceId: { exact: deviceId },
          width: { ideal: 1280 },
          height: { ideal: 720 },
        },
        audio: false,
      });
      if (!isCurrentCameraSession(cameraSessionId) || nativeDialogOpenRef.current) {
        stopMediaStream(stream);
        return false;
      }

      streamRef.current = stream;
      video = videoRef.current;
      if (video) {
        video.srcObject = stream;
        await video.play();
      }
      if (!isCurrentCameraSession(cameraSessionId) || nativeDialogOpenRef.current) {
        discardPreviewStream(stream, video);
        return false;
      }

      setIsPreviewing(true);
      setStatus(t.preview);
      return true;
    } catch (error) {
      if (stream) {
        discardPreviewStream(stream, video);
      }
      if (!isCurrentCameraSession(cameraSessionId)) return false;
      throw error;
    } finally {
      finishCameraOperation();
    }
  }

  async function startCapture() {
    if (nativeDialogOpenRef.current) return;
    const finishCameraOperation = cameraOperationGate.begin();

    try {
      if (!isPreviewing) {
        const started = await startPreview();
        if (!started) return;
      }
      if (nativeDialogOpenRef.current) return;
      stopCaptureTimer();
      const interval = Math.max(1, dashboard?.settings.capture_interval ?? 1) * 1000;
      const scheduler = createSerialTaskScheduler(captureFrame, interval, (error) => {
        if (captureSchedulerRef.current !== scheduler) return;
        captureSchedulerRef.current = null;
        setIsCapturing(false);
        setStatus(String(error));
      });
      captureSchedulerRef.current = scheduler;
      setIsCapturing(true);
      scheduler.start();
    } catch (error) {
      setStatus(String(error));
    } finally {
      finishCameraOperation();
    }
  }

  function stopCaptureTimer() {
    captureSchedulerRef.current?.stop();
    captureSchedulerRef.current = null;
    setIsCapturing(false);
  }

  function stopPreviewStream() {
    if (streamRef.current) {
      stopMediaStream(streamRef.current);
    }
    streamRef.current = null;
    if (videoRef.current) videoRef.current.srcObject = null;
    setIsPreviewing(false);
  }

  function stopCamera() {
    cameraSessionIdRef.current += 1;
    stopCaptureTimer();
    stopPreviewStream();
  }

  function beginCameraSession() {
    cameraSessionIdRef.current += 1;
    return cameraSessionIdRef.current;
  }

  function isCurrentCameraSession(cameraSessionId: number) {
    return cameraSessionIdRef.current === cameraSessionId;
  }

  function stopMediaStream(stream: MediaStream) {
    stream.getTracks().forEach((track) => track.stop());
  }

  function discardPreviewStream(stream: MediaStream, video: HTMLVideoElement | null) {
    stopMediaStream(stream);
    if (streamRef.current === stream) {
      streamRef.current = null;
    }
    if (video?.srcObject === stream) {
      video.srcObject = null;
    }
  }

  async function selectCamera(deviceId: string) {
    if (nativeDialogOpenRef.current) return;
    setSelectedCamera(deviceId);
    if (!deviceId) {
      stopCamera();
      return;
    }

    try {
      await startPreview(deviceId);
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function captureFrame() {
    if (nativeDialogOpenRef.current) return;
    const finishCameraOperation = cameraOperationGate.begin();

    try {
      const video = videoRef.current;
      if (!video || video.videoWidth === 0 || video.videoHeight === 0) return;

      const canvas = document.createElement("canvas");
      canvas.width = video.videoWidth;
      canvas.height = video.videoHeight;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      ctx.drawImage(video, 0, 0, canvas.width, canvas.height);

      const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/jpeg", 0.92));
      if (!blob) return;

      const buffer = await blob.arrayBuffer();
      const bytes = Array.from(new Uint8Array(buffer));
      const result = await invoke<ActionResult>("save_capture_image", { bytes });
      const path = result.path ?? "";
      const savedText = savedPhotoText(language, path, result.message);
      setLastSaved(savedText);
      setStatus(savedText);
    } finally {
      finishCameraOperation();
    }
  }

  const { installerCards, mainCards, resourceEntryCards, weightCards } = createHomeCardGroups({
    arduinoCliStatus: arduinoCli.status,
    arduinoCliStatusError: arduinoCli.error,
    onArduinoCliStatusChanged: arduinoCli.updateStatus,
    installerProgress,
    dashboard,
    internetConnected,
    language,
    onOpenAnnotator: () => void openAnnotationView(),
    onOpenCamera: () => void openCameraView(),
    onOpenConverter: () => void openConverterView(),
    onOpenImageConverter: () => void openImageConverterView(),
    onOpenResourceCategory: (category) => void openResourcesView(category),
    onOpenVersionUpdate: () => void openUrl(RELEASES_URL),
    onVersionChecked: rememberVersionCheck,
    runAction,
    t,
    versionCheck,
  });
  return (
    <main
      {...(isNativeDialogOpen ? { inert: "" } : {})}
      aria-busy={isNativeDialogOpen}
      className={`appShell ${view === "settings" ? "settingsShell" : ""} ${view === "converter" ? "converterShell" : ""} ${
        view === "annotator" || view === "image-converter" ? "annotationShell" : ""
      }`}
    >
      {view !== "annotator" && view !== "image-converter" && (
        <AppHeader
          dashboard={dashboard}
          isLanguageMenuOpen={isLanguageMenuOpen}
          language={language}
          languageMenuRef={languageMenuRef}
          onBackHome={() => {
            stopCamera();
            setOpenActionMenu(null);
            setView("home");
          }}
          onChangeLanguage={(nextLanguage) => void changeLanguage(nextLanguage)}
          onCloseLanguageMenu={() => setIsLanguageMenuOpen(false)}
          onOpenSettings={() => void openSettingsView()}
          onSettingsBack={() => {
            setOpenActionMenu(null);
            setView("home");
          }}
          onToggleLanguageMenu={() => setIsLanguageMenuOpen((current) => !current)}
          t={t}
          view={view}
        />
      )}

      {(view === "home" || view === "camera") && (
        <LinkPanel
          onCopyText={(text, message) => void copyText(text, message)}
          onOpenUrl={(url) => void openUrl(url)}
          preferenceCopyMessage={PREFERENCE_COPY_MESSAGE}
          realtekPackageUrl={dashboard?.metadata.realtek_package_url}
          repository={dashboard?.metadata.repository}
          t={t}
        />
      )}

      {status && (
        <div
          className={`feedbackToast ${isFeedbackLeaving ? "leaving" : ""}`}
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          {status}
        </div>
      )}

      {view === "home" && (
        <HomeView
          downloadProgress={downloadProgress}
          isDownloadKey={isDownloadKey}
          mainCards={mainCards}
          openActionMenu={openActionMenu}
          resourceEntryCards={resourceEntryCards}
          running={running}
          setOpenActionMenu={setOpenActionMenu}
          t={t}
        />
      )}

      {(view === "installers" || view === "weights") && (
        <ResourceLibraryView
          cards={view === "installers" ? installerCards : weightCards}
          category={view}
          onOpenUrl={(url) => void openUrl(url)}
          downloadProgress={downloadProgress}
          isDownloadKey={isDownloadKey}
          openActionMenu={openActionMenu}
          running={running}
          setOpenActionMenu={setOpenActionMenu}
          t={t}
        />
      )}

      {view === "settings" && (
        <SettingsView
          autoCheckUpdates={autoCheckUpdates}
          onChangeAutoCheckUpdates={changeAutoCheckUpdates}
          onChangePreferenceVersion={(version) => void changePreferenceVersion(version)}
          onChangeUvcdFormat={(format) => void changeUvcdFormat(format)}
          onClearWeightRecords={() => void clearWeightRecords()}
          onResetSettings={() => void resetSettings()}
          running={running}
          selectedPreferenceVersion={selectedPreferenceVersion}
          selectedUvcdFormat={selectedUvcdFormat}
          t={t}
        />
      )}

      {view === "converter" && (
        <ConverterView
          completedConversion={completedConversion}
          converterExtensions={converterExtensions}
          converterFile={converterFile}
          converterInputRef={converterInputRef}
          converterProgress={downloadProgress.converter}
          converterStatus={converterStatus}
          converterTask={converterTask}
          converterType={converterType}
          internetConnected={internetConnected}
          isConverterBusy={isConverterBusy || isConverterFileLoading}
          modelConverterUrl={modelConverterUrl}
          onChooseDroppedPath={(path) => void chooseDroppedConverterFile(path)}
          onChooseFile={chooseConverterFile}
          onDownloadCompletedConversion={() => void downloadCompletedConversion()}
          onOpenUrl={(url) => void openUrl(url)}
          onSelectType={selectConverterType}
          onStartModelConversion={() => void startModelConversion()}
          selectedConverterModel={selectedConverterModel}
          t={t}
        />
      )}

      {view === "annotator" && (
        <AnnotationView
          onBackHome={() => {
            setView("home");
          }}
          onStatus={setStatus}
        />
      )}

      {view === "image-converter" && (
        <ImageConversionView
          language={language}
          onBackHome={() => {
            setView("home");
          }}
          onStatus={setStatus}
          t={t}
        />
      )}

      {view === "camera" && (
        <CameraView
          cameraGuideSteps={cameraGuideSteps[language]}
          cameras={cameras}
          isCameraBusy={isCameraBusy}
          isCapturing={isCapturing}
          isChoosingOutputFolder={isNativeDialogOpen}
          isPreviewing={isPreviewing}
          lastSaved={lastSaved}
          onOpenOutputFolder={() =>
            void runAction<ActionResult>("output", "open_output_folder", (result) => result.path ?? result.message)
          }
          onRefreshCameras={refreshCameras}
          onSelectCamera={(deviceId) => void selectCamera(deviceId)}
          onSelectOutputFolder={() => void selectOutputFolder()}
          onStartCapture={() => void startCapture()}
          onStopCaptureTimer={stopCaptureTimer}
          outputFolder={dashboard?.output_folder ?? ""}
          selectedCamera={selectedCamera}
          t={t}
          videoRef={videoRef}
        />
      )}

      <NetworkStatus internetConnected={internetConnected} t={t} />
    </main>
  );
}

export default App;

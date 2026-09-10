import type { ConverterModel, ModelType, UvcdFormat } from "./types";

export const TOAST_DISPLAY_MS = 1500;
export const INSTALLER_TOAST_DISPLAY_MS = 10000;
export const TOAST_FADE_MS = 240;
export const RELEASES_URL = "https://github.com/breeze0305/Realtek_AMB82mini_plugin/releases";
export const AUTO_UPDATE_CHECK_STORAGE_KEY = "amb82-mini-auto-update-check";
export const VERSION_CHECK_STORAGE_KEY = "amb82-mini-version-check";
export const uvcdFormatOptions: Array<{ value: UvcdFormat; label: string }> = [
  { value: "YUY2", label: "YUY2" },
  { value: "NV12", label: "NV12" },
  { value: "MJPG", label: "MJPG" },
  { value: "H264", label: "H264" },
  { value: "H265", label: "H265" },
];

export const converterModelDefaults: Record<ModelType, ConverterModel> = {
  yolo: {
    type: "yolo",
    label: "Object Detection",
    input_extensions: [".pt"],
    download_name: "yolov7_tiny.nb",
  },
  classification: {
    type: "classification",
    label: "Classification",
    input_extensions: [".h5"],
    download_name: "img_class_cnn.nb",
  },
};

export const converterModelOrder: ModelType[] = ["yolo", "classification"];

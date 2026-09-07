# Realtek AMB82-mini Computer Plugin 開發交接文件

這份文件是未來理解與修改本專案的主要入口。讀完後應該能知道：這個程式有哪些功能、前後端怎麼分工、常見功能要改哪裡、版本號如何由 `version.txt` 統一管理，以及 commit / push 的工作習慣。

目前軟體版本：`3.17.1`

> 注意：`dev_readme.md` 目前會納入 git 追蹤。若交接內容或維護流程有變更，應和相關程式碼一起 commit。

## 專案定位

本專案是 Realtek AMB82-mini 開發用的 Windows 桌面工具，技術上是 Tauri 2 + React + TypeScript + Rust。它把原本偏 CLI / 手動操作的流程包成桌面 UI，協助使用者取得驅動與範例資源、下載常用工具、開啟 Realtek Arduino package、使用 AMB UVC 相機截圖，以及設定 AMB UVC device 輸出格式。

目前不使用 RTSP 預覽，也不使用 Windows Media Foundation worker。RTSP / Media Foundation 方案已廢除並回到 WebView `getUserMedia` 的 UVC 相機流程。

## 技術棧

- Frontend：React 18、TypeScript、Vite
- Desktop shell：Tauri 2
- Backend：Rust
- UI icon：`lucide-react`
- Windows bundle：Tauri NSIS

## Current frontend architecture (3.17.1)

This section is the authoritative source map for the current frontend. Some older notes below may still mention the pre-refactor shape where most UI lived in `src/App.tsx`; when in doubt, follow this section.

- `src/App.tsx`
  - App controller only: dashboard/view state, running action state, Tauri `invoke` calls, camera flow, converter flow, version-check state, and wiring child views together.
- `src/types.ts`
  - Shared frontend types for dashboard/settings/download/converter/version-check/view/action data, including annotation preparation and image-conversion progress/summary payloads.
- `src/i18n.ts`
  - `translations`, `languageNames`, `installActionLabels`, `cameraGuideSteps`, and `PREFERENCE_COPY_MESSAGE`.
- `src/appConfig.ts`
  - Frontend constants: toast timing, releases URL, localStorage keys, UVC options, and converter model defaults.
- `src/converterUtils.ts`
  - Pure helpers: UVC option label, saved-photo text, converter API URL normalization, file-extension checks, `wait`, and API JSON parsing.
- `src/homeCards.ts`
  - Home and resource-library card composition. Resource definitions have an explicit installer/weight category so new files can be added without changing page JSX.
- `src/resourceGuides.ts`
  - `getResourceGuide(resourceId, t)` supplies each code/model-weight resource's guide sections, text, code examples, images, captions, links, and placeholder status. All five resources have complete guides. The gesture (01) and AMB box (02) guides each contain eight sections, six bundled images (five screenshots and one wiring diagram), and an assembly video link that opens in the default browser. They share the general instructions and five images, while each uses its own class settings screenshot: gesture has `itemList[5]` with IDs 0–4 named `gesture1`–`gesture5`; box has `itemList[1]` with ID 0 named `box`, all enabled with value 1. Robot car instructions use `hand_code.txt` for gesture and `code.txt` for box. Classification guides (03–05) share five operating sections and five images for `RTSPImageClassification`, `CUSTOMIZED_IMGCLASS`, `ClassificationClassList.h`, class IDs 0–2 named `box`/`money`/`mouse` in `imgclassItemList`, and placement of `img_class_cnn.nb`. Only their summaries differ: `money` means Japanese coins, Taiwanese banknotes, or Singaporean banknotes; `box` and `mouse` always mean an AMB box and a computer mouse. Unknown resource IDs still receive placeholder content. Localized copy lives in `src/i18n.ts`.
- `src/components/`
  - `AppHeader.tsx`: app title, back button, language menu, settings entry.
  - `CardGrid.tsx`: shared numbered card grid, download progress, running state, and split-action rendering.
  - `LinkPanel.tsx`: GitHub repository and AMB Preference link panel.
  - `HomeView.tsx`: main-menu heading, two numbered resource entries, divider, and six primary function cards.
  - `ResourceLibraryView.tsx`: category-specific secondary page. Code/model weights use a single-column card list on the left and the selected resource's guide on the right; installers retain the shared card grid.
  - `SettingsView.tsx`: settings page UI, including auto update check, Preference version, UVC format, installed-weight cleanup, and reset.
  - `CameraView.tsx`: camera page UI.
  - `ConverterView.tsx`: model converter page UI.
  - `AnnotationView.tsx`: object detection labeling UI, including folder selection/drop, EXIF preparation progress, class management, image navigation, box drawing/moving/resizing, and current-image reset.
  - `ImageConversionView.tsx`: recursive image-conversion folder selection/drop UI, progress display, completion summary, and return-to-home flow.
  - `NetworkStatus.tsx`: global online/offline status indicator fixed to the lower-left corner of every view.
- `src/styles.css`
  - Shared styling for shell, header, home cards, settings, camera, converter, annotation workspace, toast, and network status.

Version-check behavior:

- Rust `check_version` still lives in `src-tauri/src/lib.rs`.
- Remote version URLs live in `src-tauri/endpoint_manifest.json`.
- The home version card is built in `src/homeCards.ts`.
- If a newer remote version is found, the card label changes from check to update and opens `RELEASES_URL`.
- Version-check results persist in browser `localStorage` under `VERSION_CHECK_STORAGE_KEY`.
- The settings page has an auto update check switch. It defaults to enabled and persists under `AUTO_UPDATE_CHECK_STORAGE_KEY`.
- When auto update check is enabled, startup automatically calls `check_version`; if a newer version exists, the app shows a toast and keeps the button in update mode.

Object detection annotation behavior:

- The home annotation card is built in `src/homeCards.ts` and opens `view === "annotator"`.
- `src/components/AnnotationView.tsx` owns the annotation workspace UI and local interaction state.
- Tauri drag/drop is enabled in `src-tauri/tauri.conf.json` so a folder path can be received by `getCurrentWebview().onDragDropEvent`.
- Rust annotation commands live in `src-tauri/src/lib.rs`: `select_annotation_folder` only returns the selected path, while asynchronous `load_annotation_folder` scans and normalizes image orientation before loading the workspace; image reading and annotation saving continue through `read_annotation_image`, `save_annotation_classes`, `save_annotation_file`, and `save_annotation_workspace`.
- EXIF orientation normalization is isolated in `src-tauri/src/annotation_orientation.rs`. The blocking image work runs outside the async executor, and a per-invocation Tauri channel reports `discovering`, `normalizing`, `loading`, and `complete` progress phases.
- The backend creates `{image-folder-name}_labels` beside the selected image folder, reads/writes `classes.txt`, and stores one YOLO `.txt` file per image.
- If `classes.txt` is missing, the backend derives the required class count from the highest loaded YOLO class ID, creates `object1` through `objectN`, and writes the recovered file before returning the workspace. Existing class files are never replaced by this recovery path.
- Selecting or dropping a folder always completes the orientation scan before entering the annotation workspace. The progress view stays visible while the folder is being enumerated, normalized, and loaded.
- Only images with EXIF Orientation `2` through `8` are decoded and rewritten. The orientation transform is applied to the pixels so the visible direction remains unchanged, then the output no longer depends on the EXIF orientation value. Images without a relevant orientation are left byte-for-byte untouched.
- Each rewritten image is produced through a same-directory temporary file and atomically replaces the original only after a successful encode. A failed image retains its original file, is recorded in the summary, and does not stop the remaining folder scan.
- JPEG rewrites retain the edited EXIF, ICC profile, JFIF density, and comments; PNG rewrites retain the edited EXIF, ICC profile, and an explicit allowlist of safe ancillary metadata. APNG files that need orientation work are reported as failures and remain byte-for-byte unchanged so animation frames are never discarded.
- The frontend reads image bytes through `read_annotation_image`, converts them to a Blob URL, and avoids `assetProtocol` permissions.
- Label rows use YOLO normalized values: `<class_id> <x_center> <y_center> <width> <height>`.
- Bounding-box, draft, and resize-handle strokes compensate for CSS zoom so their visible thickness stays constant.
- The drawing cursor shows horizontal and vertical dashed guides to the image edges; guides hide outside the image and during pan or box editing.
- Box edits autosave silently; save errors still surface through the shared toast.

Image conversion behavior:

- The home image-conversion card is built in `src/homeCards.ts` and opens `view === "image-converter"`.
- `src/components/ImageConversionView.tsx` selects or accepts a dropped folder, receives throttled progress through a Tauri `Channel`, prevents re-entry while work is active, and returns home with a completion summary.
- Rust commands `select_image_conversion_folder` and `convert_image_folder` live in `src-tauri/src/lib.rs`; the blocking work is isolated in `src-tauri/src/image_conversion.rs`.
- The selected directory is scanned recursively without following symbolic links, Windows junctions, or any other reparse point. A selected root that is itself a reparse point is rejected. Supported extensions are `jpg`, `jpeg`, `png`, `bmp`, `webp`, `heic`, and `heif`; other files are ignored.
- JPEG/PNG reuse `annotation_orientation.rs` and are only rewritten when EXIF Orientation `2` through `8` needs to be baked into pixels. BMP, static WebP, HEIC, and HEVC (`hvc1`) HEIF become same-directory `.jpg` files.
- Animated WebP is rejected and preserved because flattening it would discard frames. Alpha from static WebP/HEIC is composited over white before JPEG encoding.
- HEIC/HEIF decoding uses pinned pure-Rust `hpvcd 0.3.2`, so the release does not depend on the optional Windows Store HEIF/HEVC codecs or native DLLs. The converter strictly parses `pitm` and `ipma` to inspect only transforms associated with the primary item; transforms belonging only to tiles or auxiliary items do not cause rejection. One primary-associated `imir` or `irot` is supported, while combined or duplicate primary transforms are rejected because this decoder version does not compose them correctly. The parsed transform payload and the orientation reported by the decoder must agree. The decoder-baked container transform is authoritative; EXIF Orientation is only applied as a pixel fallback when the container orientation is `Normal`, and its tag is then removed.
- Before HEIF EXIF reaches the shared sanitizer, `canonicalize_heif_exif` accepts either raw little-/big-endian TIFF or exactly one common `Exif\0\0` identifier followed by a valid TIFF payload. The identifier is removed only after validation; the code does not scan arbitrary offsets. Missing TIFF headers, empty or repeated wrappers, garbage prefixes, and other ambiguous metadata fail closed so the source remains unchanged.
- `src-tauri/src/image_safety.rs` opens each source with read/delete access while sharing read access only. The same Windows handle remains alive through verification, no-overwrite rename, exact-source deletion, and rollback, so another process cannot write, rename, delete, or atomically replace that source during the transaction.
- Conversion first writes a same-directory temporary JPEG through a retained read/write/delete handle, flushes it, reads it back, and fully verifies its dimensions/edited EXIF/ICC. It then renames that exact temporary file without overwriting an existing target and commits deletion against the exact source handle last. JPG/PNG normalization similarly moves the exact source to a unique recovery name before publishing the verified replacement. A collision, active writer, failed verification, or failed deletion preserves or restores the original; rollback never deletes an unknown file merely because it occupies the expected path.
- Every processed source is limited to 512 MiB, 16,384 pixels on either side, and 64 megapixels before a full pixel decode.
- Annotation preparation and image conversion share `AppState.image_processing_lock`, preventing both mutation workflows from running concurrently.


## 重要檔案

- `src/App.tsx`
  - 前端 controller、頁面狀態、Tauri command 呼叫與各 View wiring。
  - 設定頁。
  - AMB 相機預覽與截圖流程。
  - Tauri command 呼叫。

- `src/styles.css`
  - 可調整視窗的 UI 與 responsive 樣式。
  - 主選單卡片、語言 dropdown、設定頁、相機頁、toast、網路狀態燈。

- `src-tauri/src/lib.rs`
  - Tauri command 實作。
  - metadata / dashboard。
  - 檔案另存、下載、版本檢查。
  - 輸出資料夾管理。
  - 相機截圖儲存。
  - UVC device 格式設定持久化。
  - `UVCD_pram.h` 背景覆寫。
  - AMB Preference release / beta 版本連結切換。
  - 內嵌資源讀取。
- `src-tauri/src/image_conversion.rs`
  - 遞迴掃描、BMP/WebP/HEIC/HEIF 解碼、HEIF EXIF 正規化、方向套用、JPEG 安全寫入與驗證。
- `src-tauri/src/image_safety.rs`
  - 圖片大小／尺寸上限、Windows 禁止同時寫入的來源 handle，以及 reparse point／junction 防護。

- `src-tauri/src/main.rs`
  - Tauri 程式入口。
  - release 使用 Windows GUI subsystem，不顯示黑色 terminal。

- `src-tauri/tauri.conf.json`
  - 視窗大小、bundle 設定、產品名稱、Tauri app version。
  - 主視窗預設為 `1180 × 760`，最小為 `1120 × 640`；最小寬度會保持首頁卡片雙欄排列，並容納標註工作區的正常三欄版面。

- `src-tauri/endpoint_manifest.json`
  - 外部端點集中設定。
  - 包含 GitHub repository、版本檢查 URL、Arduino 已知版本 fallback、VLC 下載 URL 與固定 SHA-256、Realtek package URL、模型轉換服務 URL、網路檢查 URL。
  - 若外部服務改版、下載來源失效、需要新增 mirror，優先改這裡。

- `src-tauri/Cargo.toml`
  - Rust crate 設定與 Tauri 版本鎖定。

- `package.json`
  - Node scripts、前端依賴、npm package version。

- `resource/`
  - 建置時被 Rust `include_bytes!` 內嵌進 exe 的原始資源。
  - 目前包含 CH341SER、手勢追蹤程式碼/權重、影像分類權重。

- `readme.md`
  - 使用者面向 README。

- `THIRD_PARTY_NOTICES.txt`
  - 隨 bundle 提供的第三方元件授權；新增會進入正式執行檔的原生依賴時應同步維護。

- `version.txt`
  - 遠端版本檢查會讀 GitHub main branch 上的這個檔案。

## 目前功能總覽

### 主畫面

主畫面與資源分類由 `src/homeCards.ts` 的 `createHomeCardGroups` 組成。

首頁最前方保留兩張獨立的資源入口：

- `01` 安裝檔：CH340/CH341、Arduino IDE、VLC
- `02` 程式碼與權重：手勢追蹤、AMB 盒子追蹤、日本／台灣／新加坡影像分類權重

兩張資源入口之後以「主要功能」分隔線區隔，再排列六張一般功能卡：

- AMB 相機畫面擷取
- 模型量化轉換
- 物件偵測標記
- 圖片轉檔
- 開啟 AmebaPro2 資料夾
- 版本檢查

兩張入口的整張卡片都可點擊，不顯示額外的「開啟」按鈕，並分別開啟自己的二級頁面；頁面內不提供跨分類頁籤。首頁不直接展開個別下載卡。新增既有 command 的資源時，在 `src/homeCards.ts` 的 resource definition 加入項目並指定 `installers` 或 `weights`；若是新的 command，仍需同步新增 `RunningAction` 與 Rust 後端實作，網路下載還要補 `DownloadKey` 與進度事件。

「程式碼與權重」二級頁左側單欄列出原有五張資源卡，右側顯示所選資源的文字、圖片與圖片說明，進入時預設第一項。左右欄依內容自然伸展，共用 `appShell` 的單一整頁垂直捲動條；卡片增多或說明變長時，兩欄一起捲動，不各自捲動。點擊卡片或透過鍵盤操作只切換說明；卡片內的「取得」按鈕維持原本的另存流程，不因選取說明而觸發。

01 手勢與 02 AMB 盒子卡皆提供八個段落的正式教學，每項顯示六張圖片（五張操作截圖與一張自走車接線圖）。共同步驟包含在 `ObjectDetectionLoop` 將 `DEFAULT_YOLOV4TINY` 改為 `CUSTOMIZED_YOLOV7TINY`，再切換至 `ObjectClassList.h`。01 的 `itemList[5]` 使用類別 ID 0～4、名稱 `gesture1`～`gesture5`；02 的 `itemList[1]` 僅使用類別 ID 0、名稱 `box`，各項啟用值皆為 1，類別設定截圖分別對應各項資源。設定類別後，透過主選單的「開啟AmebaPro2資料夾」進入 `libraries → NeuralNetwork → examples → ObjectDetectionLoop`，將取得的 `yolov7_tiny.nb` 放在與 `ObjectDetectionLoop.ino`、`ObjectClassList.h` 相同的資料夾。自走車用途以 01 的 `hand_code.txt` 或 02 的 `code.txt` 完整取代 `ObjectDetectionLoop.ino`，最後依序附上共用的自走車接線圖與組裝影片。影片 URL 為 `https://www.youtube.com/watch?v=UpYyOiEFA0k`，經既有 `open_url` 命令由預設瀏覽器開啟，觀看影片需要網路連線。

03 日本、04 台灣與 05 新加坡影像分類卡皆提供五個段落的正式教學，每項顯示相同的五張操作截圖。操作順序為在 Arduino IDE 開啟 `File → AmebaNN → RTSPImageClassification`，將約第 90 行 `imgclass.modelSelect` 的 `DEFAULT_IMGCLASS` 改為 `CUSTOMIZED_IMGCLASS`；切換至 `ClassificationClassList.h`；將 `imgclassItemList` 的類別 ID 0、1、2 分別設為 `box`、`money`、`mouse`，啟用值皆為 1；透過本工具開啟 AmebaPro2 資料夾；進入 `libraries → NeuralNetwork → examples → RTSPImageClassification`，將所選的 `img_class_cnn.nb` 放在與 `RTSPImageClassification.ino`、`ClassificationClassList.h` 相同的資料夾。三項資源只在摘要說明訓練內容差異：`box` 均為 AMB 盒子，`mouse` 均為滑鼠，`money` 則依 03／04／05 分別為日本硬幣／台灣紙鈔／新加坡紙鈔。其餘操作與圖片共用，避免後續維護時出現不一致。「安裝檔」頁維持原卡片排列與下載／安裝功能。

設定入口不是主選單卡片。設定按鈕位於右上角語言選單旁邊。

### 語言切換

- 前端文字在 `src/i18n.ts` 的 `translations` 物件。
- 支援 `zh_TW`、`en_US`、`ja_JP`。
- 語言狀態會透過 Rust `set_language` 存進 `%LOCALAPPDATA%\AMB82 Mini Computer Plugin\settings.json`。

新增 UI 文字時，要同時補三種語言，否則 TypeScript 型別會報錯或 UI 文字不完整。

### 檔案取得

前端卡片對應後端 command：

| UI 項目 | 前端 command | 來源 | 預設檔名 |
| --- | --- | --- | --- |
| CH340/CH341 安裝檔 | `save_driver_as` | `resource/CH341SER.EXE` | `CH341SER.EXE` |
| 手勢-自走車追蹤程式碼/權重 | `save_hand_resources_as` | `resource/gesture_recognition/*` | `hand_code.txt`, `yolov7_tiny.nb` |
| AMB盒子-自走車追蹤程式碼/權重 | `save_object_detection_box_resources_as` | `resource/object_detection_box/*` | `code.txt`, `yolov7_tiny.nb` |
| 日本影像分類權重 | `save_image_model_japan_as` | `resource/image_classification_japan/img_class_cnn.nb` | `img_class_cnn.nb` |
| 台灣影像分類權重 | `save_image_model_taiwan_as` | `resource/image_classification_taiwan/img_class_cnn.nb` | `img_class_cnn.nb` |
| 新加坡影像分類權重 | `save_image_model_singapore_as` | `resource/image_classification_singapore/img_class_cnn.nb` | `img_class_cnn.nb` |
| Arduino IDE | `download_arduino_ide_as` | Arduino 官方 release metadata / app 私有快取 | 官方 asset 檔名 |
| Arduino IDE 自動安裝 | `download_and_install_arduino_ide` | Arduino 官方 release metadata / app 私有快取 | 官方 MSI asset 檔名 |
| VLC | `download_vlc_as` | VLC 固定版本 URL / app 私有快取 | `vlc-3.0.23-win32.exe` |
| VLC 自動安裝 | `download_and_install_vlc` | VLC 固定版本 URL / app 私有快取 | `vlc-3.0.23-win32.exe` |

注意：

- CH340/CH341 與「程式碼與權重」頁面內的資源使用內嵌檔案，不需要外網。
- Arduino / VLC 第一次取得時需要外網；已有通過 SHA-256 驗證的快取後，離線仍可另存或安裝。
- Arduino / VLC 的卡片都有 split button，主按鈕下載，旁邊選單自動安裝。
- 安裝檔快取位於 Tauri `app_cache_dir()/installer-cache/v1`。每次使用前都會重新計算 SHA-256；驗證失敗的快取不會被另存或執行，有網路時會重新下載。
- Arduino 透過官方 GitHub Releases metadata 解析最新版 EXE / MSI 與官方 digest；metadata 暫時無法取得時，可繼續使用已驗證的快取或 manifest 內的已知版本 fallback。
- VLC 使用 manifest 內固定版本的可信 SHA-256；手動下載與自動安裝共用同一份快取。自動安裝從已驗證的快取執行 `/S` 靜默安裝。
- SHA-256 驗證用來偵測下載或快取 payload 損壞、遭修改；Arduino 離線時的動態版本信任值保存在同一個使用者可寫的 metadata sidecar，因此無法防範同一使用者程序同時替換 payload 與 sidecar，也不代表能防範程式本身或信任資料遭修改。
- 下載進度由 Rust emit `download-progress` event，前端顯示卡片覆蓋式進度。
- 內嵌資源優先邏輯：若 exe 同目錄附近有外部 `resource/` 覆寫檔，會優先使用外部檔；找不到才用 binary 內嵌 bytes。

### AMB 相機畫面擷取

相機頁在 `src/App.tsx`，使用 WebView 的 `navigator.mediaDevices`：

- 進入相機頁後自動要求 camera permission。
- 掃描 `videoinput` 裝置。
- 按「重新偵測鏡頭」會重新掃描裝置並啟動可用鏡頭，使用者不必離開相機頁。
- 自動選第一個 camera 並開始預覽。
- 使用者可用下拉選單切換 camera。
- 按「開始截圖」後，前端把 `<video>` 畫到 canvas，再轉 JPEG bytes。
- JPEG bytes 呼叫 Rust `save_capture_image`。

截圖輸出：

- 預設輸出到程式工作目錄底下的 `output/`。
- 檔名格式：`image_00001.jpg`、`image_00002.jpg`。
- 若資料夾已有圖片，會接續最大序號。
- 使用者可在本次執行期間用「選擇資料夾」改 output 位置。
- 資料夾選擇器以主視窗為 owner-modal；開啟期間固定在 Plugin 上方並鎖住主視窗，取消或完成後恢復操作。
- output 位置目前只存在 runtime state，重開程式會回到預設。

相機頁底部有 UVC 相機設定教學，文字在 `cameraGuideSteps`。

### 設定頁、Preference version、UVC device 格式與權重清除

設定入口在右上角語言選單旁的齒輪按鈕。設定頁目前包含 AMB Preference 版本切換、UVC device 屬性設定、清除權重紀錄、簡易警告文字，以及恢復預設設定按鈕。

Preference version：

- UI 是 release / beta 的雙切開關。
- 預設是 `beta`，開關顯示為 beta 開啟。
- 選 `beta` 時，主畫面常駐顯示的 AMB Preference link 使用 `REALTEK_PACKAGE_BETA_URL`。
- 選 `release` 時，主畫面常駐顯示的 AMB Preference link 使用 `REALTEK_PACKAGE_RELEASE_URL`。
- 設定會存進 `settings.json` 的 `preference_version`。
- 如果未來要換連結，改 `src-tauri/src/lib.rs` 的 `REALTEK_PACKAGE_BETA_URL` / `REALTEK_PACKAGE_RELEASE_URL`。

Reset：

- UI 文字提醒使用者：如果不知道這些設定用途，請不要變更。
- Reset 按鈕會把設定頁參數恢復成 `preference_version = beta`、`uvcd_format = MJPG`。
- Reset 會寫回 `settings.json`，並嘗試把 `UVCD_pram.h` 也修回 MJPG。
- Reset 不會變更語言設定。

清除權重紀錄：

- 前端呼叫 Rust command `clear_installed_weights`。
- command 會重用「開啟 AmebaPro2 資料夾」卡片的資料夾偵測方式，從目前偵測到的 AmebaPro2 版本資料夾開始處理。
- 只會刪除下列兩個固定相對路徑：

  ```text
  libraries\NeuralNetwork\examples\RTSPImageClassification\img_class_cnn.nb
  libraries\NeuralNetwork\examples\ObjectDetectionLoop\yolov7_tiny.nb
  ```

- 任一目標檔案不存在時會視為已清除，不算錯誤；兩個檔案都不存在時也會正常完成。
- command 不會遞迴搜尋權重，也不會刪除固定路徑以外的其他 `.nb` 檔案。
- 找不到 AmebaPro2 資料夾或遇到權限／I/O 錯誤時，前端會顯示錯誤訊息；錯誤處理不會擴大刪除範圍。
- 兩個目標會各自嘗試刪除；部分失敗時錯誤訊息會列出 `deleted` / `missing` / `failed`，已成功刪除的檔案不會 rollback。
- 此功能不會修改 `settings.json`，也不屬於 Reset 設定流程。

可選格式：

- `YUY2`
- `NV12`
- `MJPG`，預設，UI 顯示 `MJPG (預設/default/既定)`
- `H264`
- `H265`

設定儲存位置：

```text
%LOCALAPPDATA%\AMB82 Mini Computer Plugin\settings.json
```

設定資料由 Rust `Settings` struct 管理，目前包含：

- `capture_interval`
- `language`
- `uvcd_format`
- `preference_version`

選擇格式後：

1. 前端呼叫 `set_uvcd_format`。
2. Rust 驗證格式是否合法。
3. Rust 寫入 `settings.json`。
4. Rust 立即嘗試修正 Realtek Arduino package 裡的 `UVCD_pram.h`。

### UVCD_pram.h 背景覆寫

程式啟動時，`src-tauri/src/lib.rs` 的 `run()` 會讀取 settings，並在 Tauri `.setup()` 裡啟動 `start_uvcd_worker`。

目標檔案位置：

```text
%USERPROFILE%\AppData\Local\Arduino15\packages\realtek\hardware\AmebaPro2\<版本>\libraries\USB\src\UVCD_pram.h
```

版本資料夾尋找方式：

- 進入 `AmebaPro2` 目錄。
- 讀取底下所有版本資料夾。
- 排序後取最後一個。

覆寫規則：

- 逐行匹配 `#define UVCD_* 數字`。
- 使用者選定格式對應的 define 設為 `1`。
- 其他 `UVCD_*` define 設為 `0`。
- 不會修改 `VIDEO_FHD_WIDTH_UVCD` 這類不是 `UVCD_` 開頭的 define。

範例，選 `MJPG`：

```c
#define UVCD_YUY2 0
#define UVCD_NV12 0
#define UVCD_MJPG 1
#define UVCD_H264 0
#define UVCD_H265 0
```

如果 Realtek Arduino package 還沒安裝或找不到 `UVCD_pram.h`：

- 啟動背景 worker 會每 300 秒重試。
- 設定頁選擇格式時仍會先儲存設定，再回報找不到檔案的訊息。

使用者改完格式後，仍需要重新燒錄 AmebaUSB / UVC_device，AMB 裝置才會用新格式重新列舉。

### 版本檢查

版本檢查卡呼叫 Rust `check_version`：

- 本機版本：Rust 以 `include_str!` 讀取 repo 根目錄的 `version.txt`。
- 遠端版本：`src-tauri/endpoint_manifest.json` 的 `version_check.urls` 指到 GitHub main branch 的 `version.txt`，並可依序嘗試 fallback URL。
- 版本比較會依序比較 major、minor、patch。
- 若遠端版本大於本機版本，顯示 `偵測到新版本: <遠端版本>`。
- 若本機版本大於遠端版本，視為開發者 / beta 版本，顯示 `當前為beta版本`。
- 若版本一致，顯示目前為最新版。

### Toast 與狀態回饋

- 前端 `status` state 控制 toast。
- Toast 顯示 1.5 秒後淡出。
- 下載進度不是 toast，而是卡片覆蓋進度條。
- 主畫面 `AMB Preference` 連結點擊後會複製目前選定的 Realtek package URL，並顯示專用提示：
  `已複製，請到Arduino IDE左上角 File -> Preferences -> Additional boards manager URLs 貼上 建議版本選擇 4.0.9`
- `AMB Preference` 專用提示會顯示一般 toast 兩倍時間；相關常數在 `src/App.tsx` 的 `TOAST_DISPLAY_MS`、`TOAST_FADE_MS`、`PREFERENCE_COPY_MESSAGE`。

## 修改指南

### 新增主選單卡片

1. 如果是前端純操作，在 `src/homeCards.ts` 的 `createHomeCardGroups` 新增項目；一般功能放進 `mainCards`，資源入口放進 `resourceEntryCards`，個別檔案則加入對應的 `installers` 或 `weights` 分類。
2. 如果需要 Rust 功能：
   - 在 `src-tauri/src/lib.rs` 新增 `#[tauri::command]` function。
   - 加到 `tauri::generate_handler![...]`。
   - 前端用 `invoke<ReturnType>("command_name", args)` 呼叫。
3. 補 `translations` 三語文字。
4. 補 CSS，盡量沿用現有 card / button class。
5. 跑 `npm.cmd run build` 和必要的 Rust 測試。

### 新增可下載或可另存資源

內嵌資源流程：

1. 把檔案放到 `resource/`。
2. 在 `src-tauri/src/lib.rs` 的 `EMBEDDED_RESOURCES` 加一筆 `include_bytes!`。
3. 新增或重用 `save_one_resource_as` / `save_resource_set_as`。
4. 在前端新增卡片與 command 對應；程式碼／權重資源也應在 `src/resourceGuides.ts` 補上對應說明。

### 修改程式碼與權重說明

- 在 `src/resourceGuides.ts` 的 `getResourceGuide(resourceId, t)` 維護各資源的段落、程式碼修改前後範例、圖片與佔位狀態，並補上替代文字與圖片說明。
- 手勢教學圖片放在 `src/assets/resource-guides/gesture/`：五張操作截圖為 `model-selection.png`、`object-class-list-tab.png`、`gesture-class-list.png`、`open-amebapro2-folder.png`、`weight-folder-location.png`，自走車接線圖為 `car-wiring-diagram.png`。盒子教學的第三張圖片使用 `src/assets/resource-guides/box/box-class-list.png`，其餘五張共用手勢教學的圖片。01 與 02 各顯示六張圖片。組裝影片保留為 YouTube 外部連結，由預設瀏覽器開啟，需網路連線。
- 03～05 共用 `src/assets/resource-guides/classification/` 的四張圖片：`model-selection.png`、`classification-class-list-tab.png`、`classification-class-list.png`、`weight-folder-location.png`；開啟 AmebaPro2 資料夾的步驟共用 `gesture/open-amebapro2-folder.png`，因此每項影像分類教學顯示五張。所有教學共十一個圖片檔案，皆透過 Vite import 隨前端打包進 exe，可離線顯示，不依賴使用者原本的 Screenshots 或 Downloads 目錄。
- 03～05 的操作段落與圖片維持共用，只在各資源摘要中描述訓練內容差異。類別設定的實際變數名稱是 `imgclassItemList`，截圖宣告為 `imgclassItemList[6]`，文字說明只要求將 ID 0～2 設為 `box`／`money`／`mouse` 並啟用，不要求修改陣列大小。
- 目前五項資源皆有正式教學。未定義的資源 ID 仍保留佔位備援；新增資源時應補上正式內容與標記，並同步更新 `src/i18n.ts` 的繁中／英文／日文文字。
- 顯示與選取操作集中在 `src/components/ResourceLibraryView.tsx`；修改時維持卡片選取與「取得」按鈕各自的操作效果。

### 新增網路下載資源

網路下載流程：

1. 在 `src-tauri/endpoint_manifest.json` 新增或更新來源、預期檔名、長度與可信 SHA-256；若使用官方 release API，也要限制可接受的 asset 名稱與來源。
2. 安裝檔先下載到 Tauri `app_cache_dir()/installer-cache/v1` 的暫存檔，下載時同步計算 SHA-256 並檢查長度；全部驗證完成後才以原子替換方式寫入正式快取。
3. 每次快取命中都重新計算 SHA-256。快取無效時拒絕另存或執行，並在可連線時下載通過驗證的新檔後原子替換；若下載失敗，原快取仍會保留但不會被使用。
4. 使用者選擇「取得」時，從已驗證快取複製到目標端暫存檔，完成驗證後再替換目標檔；自動安裝則直接執行已驗證快取。
5. 有效快取可離線使用；只有 cache miss、版本更新或快取驗證失敗時才需要重新下載大型安裝檔。
6. 若要進度條，給固定 download key，並在前端 `DownloadKey` 加上該 key。
7. 若要自動安裝，新增對應的 `download_and_install_*` command，並在前端 `menuActions` 接 split button。

### 修改相機功能

前端相機流程集中在 `src/App.tsx`：

- `openCameraView`
- `scanCameras`
- `startPreview`
- `startCapture`
- `captureFrame`
- `selectCamera`
- `stopCamera`

後端儲存集中在 `src-tauri/src/lib.rs`：

- `save_capture_image`
- `output_dir`
- `next_image_path`
- `select_output_folder`
- `open_output_folder`

### 修改 UVC 格式設定

前端：

- `UvcdFormat` type
- `uvcdFormatOptions`
- `uvcdOptionLabel`
- `changeUvcdFormat`
- 設定頁 JSX

後端：

- `DEFAULT_UVCD_FORMAT`
- `SUPPORTED_UVCD_FORMATS`
- `Settings.uvcd_format`
- `set_uvcd_format`
- `normalize_uvcd_format`
- `repair_uvcd`
- `repair_uvcd_content`

測試：

- `repair_uvcd_content_enables_mjpg_and_disables_other_uvcd_formats`
- `repair_uvcd_content_enables_selected_format`
- `normalize_uvcd_format_accepts_yuy2`

### 修改 AMB Preference 版本切換

前端：

- `PreferenceVersion` type
- `selectedPreferenceVersion`
- `changePreferenceVersion`
- `resetSettings`
- 設定頁 `.segmentedToggle` JSX
- 主畫面 AMB Preference link 顯示 `dashboard.metadata.realtek_package_url`

後端：

- `src-tauri/endpoint_manifest.json`
  - `realtek_packages.beta.urls`
  - `realtek_packages.release.urls`
  - `version_check.urls`
  - `downloads.*.urls`
  - `model_converter.site_url`
  - `model_converter.api_base`
  - `internet_check_urls`
- `DEFAULT_PREFERENCE_VERSION`
- `SUPPORTED_PREFERENCE_VERSIONS`
- `Settings.preference_version`
- `set_preference_version`
- `reset_settings`
- `normalize_preference_version`
- `preference_url`
- `metadata(preference_version)`

測試：

- `preference_url_uses_beta_by_default_and_release_when_selected`

### 修改 UI 比例或樣式

主要改 `src/styles.css`。

常用區塊：

- app shell / header：`.appShell`, `.appHeader`, `.headerActions`
- 語言與設定按鈕：`.languageSelect*`, `.settingsIconButton`
- 主選單：`.menuGrid`, `.menuCard`, `.primaryBtn`
- 設定頁：`.settingsShell`, `.settingsPageHeader`, `.settingsSection`, `.settingsRow`, `.settingsField`, `.segmentedToggle`
- 相機頁：`.cameraSection`, `.videoFrame`, `.cameraControls`, `.cameraGuide`
- toast：`.feedbackToast`

UI 原則：

- 桌面工具要保持操作密度，不做 landing page。
- 避免卡片套卡片。
- 設定頁保持簡單，不顯示全局資訊。
- 用 lucide icon，不手刻 SVG icon。
- 固定尺寸控制，避免文字或 hover 造成 layout shift。

## 版本號更新清單

目前版本是 `3.17.1`。未來更新版本時，只手動修改 repo 根目錄的 `version.txt`。

`npm run sync-version` 會把 `version.txt` 同步到：

- `package.json`
- `package-lock.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/tauri.conf.json`
- `readme.md`
- `dev_readme.md`

Rust 執行時的本機版本與 HTTP user agent 會直接從 `version.txt` 讀取，不再於 `src-tauri/src/lib.rs` 維護一份版本常數。

`npm run dev` 與 `npm run build` 會各自透過 `predev` / `prebuild` 自動執行一次 `npm run sync-version`。Tauri 的 `beforeDevCommand` 與 `beforeBuildCommand` 會呼叫這兩個 script，因此不再另外使用 `pretauri` 重複同步。同步腳本只有在內容變更時才寫入檔案；`npm run check:version` 可在不寫檔的情況下檢查版本是否一致。

版本更新建議流程：

```powershell
npm.cmd run build
cargo test --manifest-path src-tauri\Cargo.toml
npm.cmd run tauri build
```

確認產物：

```text
src-tauri/target/release/amb82-mini-computer-plugin.exe
src-tauri/target/release/bundle/nsis/AMB82 Mini Computer Plugin_<version>_x64-setup.exe
```

常見踩坑：

- 如果 release exe 正在執行，Windows 會鎖住檔案，`npm.cmd run tauri build` 可能在 linker 階段出現 `LNK1104`。先關閉 AMB82 程式再 build。

## 開發與建置指令

安裝 Node 依賴：

```powershell
npm install
```

如果 PowerShell execution policy 擋住 `npm.ps1`，改用：

```powershell
npm.cmd install
```

前端 build：

```powershell
npm.cmd run build
```

前端 lint、格式檢查與測試：

```powershell
npm.cmd run lint
npm.cmd run format:check
npm.cmd run test
```

一次執行前端完整檢查：

```powershell
npm.cmd run check
```

GitHub Actions 會在 push 與 pull request 時於 Windows 執行前端完整檢查、Rust format、Clippy 與 Rust tests。

啟動前端 dev server：

```powershell
npm.cmd run dev
```

啟動 Tauri dev：

```powershell
npm.cmd run tauri dev
```

Rust check：

```powershell
cargo check --manifest-path src-tauri\Cargo.toml
```

Rust test：

```powershell
cargo test --manifest-path src-tauri\Cargo.toml
```

產生 Windows release exe 與 NSIS installer：

```powershell
npm.cmd run tauri build
```

## 測試建議

基本測試：

1. 啟動 exe，不出現黑色 terminal。
2. 視窗可以調整大小與最大化；縮小到 `1120 × 640` 後不能再縮小，首頁卡片仍維持雙欄排列；外網狀態固定浮動在所有頁面的左下角，不會隨內容捲動。
3. 首頁的 `01` 安裝檔與 `02` 程式碼／權重為兩張可整卡點擊的獨立入口，沒有額外「開啟」按鈕，序號不與圖示方塊重疊，並與一般功能卡之間有分隔線；兩個二級頁分別顯示 3 張與 5 張資源卡，且沒有跨分類頁籤。
4. 語言切換正常。
5. 右上設定按鈕可進入設定頁。
6. 設定頁切換 `YUY2`、`NV12`、`MJPG`、`H264`、`H265` 後，`settings.json` 有保存。
7. 設定頁切換格式後，`UVCD_pram.h` 被覆寫為選定格式 `1`、其他格式 `0`。
8. 設定頁按下「清除權重紀錄」時，只刪除兩個固定路徑的權重；檔案不存在視為已清除，且相鄰或其他目錄中的 `.nb` 檔案保持不變。
9. 「程式碼與權重」頁在一般大小與 `1120 × 640` 下維持左側單欄資源卡、右側說明，預設顯示第一項；增加左側卡片或加長右側說明後，確認兩欄依內容自然伸展，僅由 `appShell` 顯示單一整頁垂直捲動條，在任一欄捲動時兩欄都一起移動。依序點擊五張卡片與使用鍵盤選取，都會切換對應正式說明，且不開啟另存視窗；01 與 02 應各顯示八段教學、程式碼修改前後範例、權重放置路徑與六張可離線查看的圖片（五張操作截圖與一張接線圖）。01 類別設定須為 `itemList[5]`、`gesture1`～`gesture5`，02 須為 `itemList[1]`、ID 0 的單一 `box`，且使用各自的類別設定截圖；自走車替換程式碼檔名分別為 `hand_code.txt` 與 `code.txt`。替換說明後依序顯示接線圖與組裝影片，影片連結須由預設瀏覽器開啟指定 YouTube 網址。03～05 應各顯示五段相同操作與五張相同圖片，包括 `RTSPImageClassification`、`CUSTOMIZED_IMGCLASS`、`ClassificationClassList.h`、`imgclassItemList` 的 `box`／`money`／`mouse` 類別設定及 `img_class_cnn.nb` 放置位置；三項摘要須分別說明日本硬幣、台灣紙鈔、新加坡紙鈔，且不顯示自走車段落或影片。按各卡片「取得」時，原本的檔案另存功能仍正常；切換語言後所選說明也同步更新。安裝檔頁維持原有排列與操作。
10. 無外網且沒有快取時，Arduino / VLC 會清楚回報無法取得；內嵌資源仍可取得。
11. 有外網時首次取得 Arduino / VLC 會顯示下載進度，完成後可正常另存或自動安裝。
12. 中斷外網後再次取得或安裝同一版本，會通過 SHA-256 驗證並重用快取，不重新下載。
13. 修改快取檔案後再次操作，程式會拒用該檔案；恢復網路後可重新下載並修復快取。
14. 相機頁可掃描 camera、預覽、截圖；進入頁面後再插入鏡頭時，可按「重新偵測鏡頭」直接更新清單並啟動預覽。
15. 輸出圖片序號會接續既有最大編號。
16. 相機頁開啟「選擇資料夾」時，對話框保持在主視窗上方，主視窗不可操作；取消或完成後恢復操作。
17. 圖片轉檔可遞迴處理巢狀資料夾，BMP、靜態 WebP 與真實 HEIC 會在原目錄產生同名 JPG，成功後來源檔消失。
18. 使用帶有常見 `Exif\0\0` 包裝與 Orientation 的 Apple HEIC 驗證：輸出 JPG 的尺寸與顯示方向正確、Orientation tag 已移除，且成功後只刪除測試用來源複本；破損或雙重包裝的 EXIF 應保留來源並回報失敗。
19. 含 EXIF Orientation 的 WebP/JPG/PNG 轉換後顯示方向不變，輸出不再含 Orientation tag；透明區域成為白色。
20. 同名 JPG 已存在、圖片正被其他程式寫入、動畫 WebP、破損、主要圖片組合／重複關聯 `imir`、`irot`，或非 `hvc1` HEIF 時，原檔保持不變，完成摘要正確顯示失敗數量；主要圖片單獨關聯 `imir` 或 `irot` 可正常處理，僅屬於 aux／tile 的 transform 不會誤判。
21. 大量檔案處理時進度條持續更新；完成後自動回首頁，繁中／英文／日文摘要與第一個失敗檔案顯示正常。

## 已知限制

- 單檔 exe 仍需要 Windows WebView2 Runtime。
- 輸出資料夾目前只保存在 runtime state，重開程式會回到預設 `./output`。
- 拍照間隔目前沒有 UI 可調整。
- Arduino IDE / VLC 下載尚未做取消下載功能。
- UVCD 設定改完後，使用者仍需要重新燒錄 AMB82 mini 的 AmebaUSB / UVC_device。
- 目前 HEIF/HEIC 轉檔支援 HEVC `hvc1` 圖片與主要圖片單獨關聯的 `imir`／`irot`；AV1、VVC、JPEG-in-HEIF，以及主要圖片組合或重複關聯的鏡像／旋轉 transform 會保留原檔並回報失敗。
- 動畫 WebP 不會轉成 JPG，以免靜默丟失其他動畫影格。

## Tauri 版本鎖定

目前不要隨意升級 Tauri 相關套件。

原因：

- 之前使用較寬鬆版本時，Cargo 會拉到較新的 `tauri-utils`、`tauri-runtime` 等內部 crate。
- 這曾造成 `tauri-build 2.0.6` 與新版本內部 API 不相容，build 失敗。

目前 Rust 端鎖定：

- Rust toolchain `1.97.0`（根目錄 `rust-toolchain.toml`）
- `Cargo.toml` 的最低支援版本 `rust-version = 1.97`
- `tauri = 2.0.6`
- `tauri-build = 2.0.6`
- `tauri-codegen = 2.0.5`
- `tauri-macros = 2.0.5`
- `tauri-runtime = 2.1.1`
- `tauri-runtime-wry = 2.1.2`
- `tauri-utils = 2.2.0`

Node 端目前：

- Node.js `^20.19.0` 或 `>=22.12.0`（CI 使用 Node.js 24）
- `@tauri-apps/api = 2.0.3`
- `@tauri-apps/cli = 2.0.4`
- `vite = 8.x`
- `vitest = 4.x`

如果未來要升級到新版 Tauri，請一次升級整組 Tauri crates / npm packages，並重新跑完整 build。

## Commit 習慣

原則：

- 專案所有修改都必須建立 Git commit，方便後續維修。
- 每次 commit 聚焦一個主題，例如：
  - `Add configurable UVC device format`
  - `Refine settings page layout`
  - `Document Arduino auto install and UVC warning`
- 不把 build 產物 commit 進 git。
  - `dist/`
  - `src-tauri/target/`
  - `.exe`
- `dev_readme.md` 是交接文件；若內容跟本次變更相關，應一起 commit。
- commit 前先看：

```powershell
git status --short
git diff --check
git diff --stat
```

建議驗證：

- 純前端 / UI 改動：跑 `npm.cmd run check`。
- Rust command / UVCD / 檔案流程改動：跑 `cargo test --manifest-path src-tauri\Cargo.toml`。
- 要給使用者測 exe：跑 `npm.cmd run tauri build`。

commit 指令：

```powershell
git add <files>
git commit -m "Short imperative summary"
```

Push 習慣：

- 只有使用者明確要求 push 時，Codex 才執行 `git push`。
- 一般修改完成後建立本機 commit，不主動 push。

## 發佈前檢查

1. 版本號清單全部同步。
2. `npm.cmd run check` 成功。
3. `cargo fmt --manifest-path src-tauri\Cargo.toml --all -- --check` 成功。
4. `cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets --locked -- -D warnings` 成功。
5. `cargo test --manifest-path src-tauri\Cargo.toml --locked` 成功。
6. `npm.cmd run tauri build` 成功。
7. 確認 release exe 沒被舊 process 鎖住。
8. 確認 `UVCD_pram.h` 覆寫規則符合目前設定。
9. 確認 `readme.md` 的版本與警告文字仍正確。

<div align="center">

<a href="#readme">
  <img src="src-tauri/icons/icon.png" alt="Realtek AMB82-mini Computer Plugin" width="160">
</a>

</div>

# Realtek AMB82-mini Computer Plugin

Realtek AMB82-mini Computer Plugin 是一款 Windows 桌面工具，用來協助 Realtek AMB82-mini 開發者快速取得開發資源、設定 AMB UVC 格式、擷取相機畫面、進行物件偵測標註、批次轉換圖片、開啟模型轉換工具，並檢查軟體版本。

本專案由舊版 Python CLI 工具重構而來，保留原本的核心功能，但改以 Tauri + React 桌面介面重新設計。新版不再要求使用者安裝 Python、OpenCV 或其他開發環境，並把文字選單流程改成更直覺、可直接發佈的 Windows 應用程式。舊版程式碼仍保留在 `legacy/v2` 分支中。

## 主要功能

- 取得 CH340/CH341 驅動安裝檔。
- 取得手勢辨識、物件偵測、影像分類等範例程式碼與模型權重。
- 下載 Arduino IDE 與 VLC 安裝檔。
- 可選擇自動安裝 Arduino IDE / VLC。
- 安裝 Arduino IDE 後，可將內附的 Arduino CLI 加入目前使用者的 PATH；自動安裝會在完成後一併設定。
- 開啟本機 Realtek AmebaPro2 Arduino 套件資料夾。
- 複製 AMB Preference package URL。
- 切換 AMB Preference release / beta 版本來源。
- 設定 AMB UVC device 輸出格式：`YUY2`、`NV12`、`MJPG`、`H264`、`H265`。
- 從設定頁清除 AmebaPro2 範例中已安裝的影像分類與物件偵測權重。
- 使用 AMB82-mini UVC 相機進行即時預覽與定時擷取。
- 使用物件偵測標註工具建立 YOLO 格式資料集。
- 遞迴轉換 `BMP`、`WebP`、`HEIC` 與採 HEVC 編碼的 `HEIF` 圖片，相容常見 Apple／iPhone HEIC 的 EXIF 包裝資料，並將方向資訊寫入像素。
- 透過內建介面上傳模型、追蹤量化轉換進度並下載 `.nb` 模型，也可開啟外部轉換網站。
- 檢查 GitHub 上的最新版本。
- 在所有頁面左下角固定顯示並定期更新外網連線狀態；離線時會停用模型轉換與版本檢查，Arduino IDE／VLC 仍可重用已驗證快取，沒有有效快取時才需要網路下載。
- 支援繁體中文、英文、日文介面。

## 使用方式

> [!IMPORTANT]
> **下載最新版請到 GitHub Releases：** [Realtek AMB82mini Plugin Releases](https://github.com/breeze0305/Realtek_AMB82mini_plugin/releases)
>
> 進入頁面後，請點開最新版本底下的 `Assets`，下載 `amb82-mini-computer-plugin.exe` 或安裝檔。

下載 release 版本後，直接執行：

```text
amb82-mini-computer-plugin.exe
```

> [!WARNING]
> 此工具會依照設定頁選擇修改使用者的 AMB UVC device 格式，預設為 `MJPG`。每次打開工具時，都會嘗試修正 Realtek AmebaPro2 套件中的 `UVCD_pram.h`。

## 功能說明

### 開發資源

程式內嵌並提供下列離線資源：

- `CH341SER.EXE`
- `gesture_recognition/hand_code.txt`
- `gesture_recognition/yolov7_tiny.nb`
- `object_detection_box/code.txt`
- `object_detection_box/yolov7_tiny.nb`
- `image_classification_japan/img_class_cnn.nb`
- `image_classification_taiwan/img_class_cnn.nb`
- `image_classification_singapore/img_class_cnn.nb`

檔案取得功能會開啟 Windows 存檔視窗，使用者可自行指定儲存位置。

首頁提供兩個可整張卡片點擊的獨立入口：`01`「安裝檔」與 `02`「程式碼與權重」。兩張入口卡與一般功能卡之間以分隔線區隔，進入後只會顯示該類別的資源；未來新增資源也會依類型放入對應頁面。

「程式碼與權重」頁面將原有資源卡片排成左側單欄，右側顯示目前所選資源的使用說明，進入時預設選取第一項。左右欄依內容自然伸展，共用單一整頁垂直捲動條；卡片增多或說明變長時，兩欄會一起捲動。點擊卡片或使用鍵盤選取，可切換各自的文字與圖片說明；「取得」按鈕仍會開啟原本的檔案另存流程。

01 手勢與 02 AMB 盒子資源皆提供八個段落的正式教學：在 `ObjectDetectionLoop` 選用 `CUSTOMIZED_YOLOV7TINY`，再切換至 `ObjectClassList.h` 設定辨識類別。01 使用 `itemList[5]`，類別 ID 0～4 依序對應 `gesture1`～`gesture5`；02 使用 `itemList[1]`，僅保留類別 ID 0、名稱為 `box` 的項目，各項啟用值均設為 1，並附上各自的類別設定截圖。接著透過主選單的「開啟AmebaPro2資料夾」進入 `libraries → NeuralNetwork → examples → ObjectDetectionLoop`，將取得的 `yolov7_tiny.nb` 放在與 `ObjectDetectionLoop.ino`、`ObjectClassList.h` 相同的資料夾。如需搭配自走車，01 使用 `hand_code.txt`、02 使用 `code.txt` 的全部內容取代 `ObjectDetectionLoop.ino`。兩項教學共用其他操作截圖、自走車接線圖與[組裝影片連結](https://www.youtube.com/watch?v=UpYyOiEFA0k)；每項皆顯示六張隨程式打包的圖片（五張操作截圖與一張接線圖），可離線查看。影片連結會由預設瀏覽器開啟，觀看時需要網路連線。

03 日本、04 台灣與 05 新加坡影像分類資源共用五個操作步驟與五張截圖，使用相同的 `box`、`money`、`mouse` 類別名稱。`box` 均代表 AMB 盒子，`mouse` 均代表滑鼠；`money` 的訓練內容依權重分別為日本硬幣、台灣紙鈔與新加坡紙鈔。請在 Arduino IDE 開啟 `File → AmebaNN → RTSPImageClassification`，將約第 90 行 `imgclass.modelSelect` 的 `DEFAULT_IMGCLASS` 改為 `CUSTOMIZED_IMGCLASS`，再切換至 `ClassificationClassList.h`，將 `imgclassItemList` 中的類別 ID 0、1、2 依序設為 `box`、`money`、`mouse`，啟用值均設為 1。接著透過本工具開啟 AmebaPro2 資料夾，依序進入 `libraries → NeuralNetwork → examples → RTSPImageClassification`，將所選資源的 `img_class_cnn.nb` 放在與 `RTSPImageClassification.ino`、`ClassificationClassList.h` 相同的資料夾。三項教學的操作與截圖完全相同，僅訓練內容說明不同，圖片皆隨程式打包，可離線查看。「安裝檔」頁面維持原有排列與操作方式。

### Arduino IDE 與 VLC

Arduino IDE 與 VLC 支援兩種方式：

- 下載安裝檔到使用者指定位置。
- 自動下載並啟動安裝流程。

Arduino IDE 或 VLC 第一次下載完成後，安裝檔會保留在應用程式的私有快取中。之後再次取得或安裝同一版本時，程式會先驗證快取檔案的 SHA-256；驗證通過便直接重用，不需重複下載，且可在離線時使用。若快取已損壞或遭修改，程式不會使用該檔案，並會在可連線時重新下載。

Arduino IDE 會透過官方 release metadata 解析最新版安裝檔及其 SHA-256；VLC 使用專案設定的固定版本與可信 SHA-256。CH340/CH341 驅動仍隨程式內嵌，不需網路下載。

下載來源、VLC 固定雜湊與 Arduino fallback 資訊定義在 `src-tauri/endpoint_manifest.json`。

自動安裝會等到安裝程式結束並確認成功後才顯示完成；安裝期間卡片會顯示「安裝中」，取消或失敗會顯示錯誤。若安裝程式要求重新啟動電腦，完成訊息也會提醒。

「安裝檔」頁另有「將 Arduino CLI 加入 PATH」卡片：

- 尚未安裝 Arduino IDE 或找不到內附 CLI：卡片為灰色，無法按下。
- 已安裝但尚未加入 PATH：可按下「加入 PATH」，離線也能操作。
- 已加入 PATH：顯示完成狀態，無需重複設定。

透過本工具自動安裝 Arduino IDE，安裝成功後會自動將 CLI 資料夾加入目前使用者的 PATH，並保留原有內容。手動安裝後可回到此頁，狀態會自動更新。若安裝成功但 PATH 設定失敗，畫面會說明原因，可透過獨立卡片重試。設定完成後，請關閉並重新開啟終端機，再執行 `arduino-cli version`；已開啟的終端機不會自動取得新 PATH。

### AMB Preference 與 UVC 格式

設定頁可開啟或關閉「自動檢查更新」，預設為開啟。此選項儲存在 WebView local storage，不屬於下方的 `settings.json`。

設定頁可切換 AMB Preference 來源：

- `Release version`
- `Beta version`

設定頁也可切換 UVC device 格式：

- `YUY2`
- `NV12`
- `MJPG`
- `H264`
- `H265`

「重置設定」會將 Preference 恢復為 `Beta version`、UVC 恢復為 `MJPG`，並重新開啟自動檢查更新；不會改變介面語言。

設定頁也提供「清除權重紀錄」功能。按下後會立即執行，不會再顯示確認視窗，也沒有 Undo；程式只會刪除目前偵測到的 AmebaPro2 版本資料夾內這兩個固定相對路徑：

```text
libraries\NeuralNetwork\examples\RTSPImageClassification\img_class_cnn.nb
libraries\NeuralNetwork\examples\ObjectDetectionLoop\yolov7_tiny.nb
```

若其中一個或兩個檔案原本就不存在，程式會視為已清除，不會回報失敗。操作不會搜尋其他位置，也不會刪除其他 `.nb` 檔案；找不到 AmebaPro2 資料夾或遇到檔案權限／I/O 錯誤時會顯示錯誤訊息。兩個檔案會分別嘗試刪除；若只有其中一個失敗，訊息會標明部分成功與失敗數量，已刪除的檔案不會自動復原。

Preference 與 UVC 設定會儲存在：

```text
%LOCALAPPDATA%\AMB82 Mini Computer Plugin\settings.json
```

### AMB 相機擷取

相機頁提供：

- 自動掃描本機可用鏡頭。
- 進入頁面後可重新偵測新接上的鏡頭，不需要離開相機頁。
- 下拉選單切換鏡頭。
- 即時預覽畫面。
- 定時擷取 JPEG 圖片。
- 選擇截圖輸出資料夾。
- AMB82-mini UVC 相機設定教學。

相機擷取預設會在程式執行位置建立或使用：

```text
output/
```

也可以在相機頁按「選擇資料夾」指定 `output/` 要建立在哪個位置。自訂輸出位置只在本次程式執行期間有效；重新啟動後會恢復使用程式執行位置下的預設 `output/`。截圖檔名會自動接續最大編號，避免覆蓋既有圖片：

```text
image_00001.jpg
image_00002.jpg
image_00003.jpg
```

### 物件偵測標註

物件偵測標註工具可用來建立 YOLO 格式資料集：

> [!WARNING]
> 載入資料夾時，含有需要處理的 EXIF Orientation 圖片會在完成寫入與驗證後就地取代原圖；JPEG 會以品質 95 重新編碼，可能再次產生有損壓縮。若需要保留完全相同的原始檔案，請先備份資料夾。

- 選擇或拖曳圖片資料夾。
- 支援 `jpg`、`jpeg`、`png`、`bmp` 圖片。
- 進入標註頁面前會逐張檢查圖片方向，並以進度條顯示目前處理進度，適合載入包含大量圖片的資料夾。
- 圖片具有 EXIF Orientation `2`～`8` 時，會把相同的顯示方向寫入圖片像素並移除對方向標籤的依賴；沒有方向標籤或不需旋轉的圖片會直接略過。
- 單張圖片處理失敗時會保留原始檔案並繼續檢查其他圖片，完成後顯示失敗數量。
- 動畫 PNG（APNG）不會以單張圖片流程改寫；若它含有需要處理的方向標籤，程式會保留原檔並將它列入失敗摘要，避免遺失動畫幀。
- 建立、重新命名、刪除 class；class 名稱只能包含英文字母與數字，不支援空白、底線或中文。
- `classes.txt` 遺失時，會依現有標註自動建立 `object1`、`object2` 等暫用名稱並補回檔案。
- 在圖片上繪製 bounding box。
- 移動、縮放、刪除、重設目前圖片的標註框。
- 縮放圖片時，標註框維持固定的畫面線寬。
- 十字游標提供延伸至圖片邊緣的水平與垂直輔助虛線。
- 使用 `A` / `D` 切換上一張 / 下一張圖片。
- 右側圖片清單只會渲染當前可見範圍附近的項目，降低大型資料集的畫面負擔。
- 自動儲存標註結果。

標註輸出會建立在圖片資料夾旁邊：

```text
<image-folder-name>_labels/
```

每張圖片會有一個同名 `.txt` 標註檔，`classes.txt` 則存放 class 名稱。標註列使用 YOLO normalized 格式：

```text
<class_id> <x_center> <y_center> <width> <height>
```

### 圖片轉檔

圖片轉檔工具可選擇或拖入一個資料夾，並遞迴處理其中所有子資料夾：

> [!WARNING]
> `BMP`、`WebP`、`HEIC`、`HEIF` 會在新 JPG 完成寫入與驗證後移除原格式來源。若需要保留原始格式，請先備份資料夾。同名 JPG 已存在或處理失敗時，程式不會覆蓋檔案，來源也會保留。

- `BMP`、靜態 `WebP`、`HEIC`，以及採 HEVC（`hvc1`）編碼的 `HEIF` 會轉成同目錄、同檔名的 `.jpg`；單獨的容器鏡像或旋轉屬性也支援。
- 已相容常見 Apple／iPhone HEIC 所使用的 EXIF 包裝資料；程式會先驗證並正規化 EXIF，再套用圖片方向並移除 Orientation 標籤。EXIF 結構損壞或不受支援時，會保留來源並列入失敗摘要。
- `JPG`、`JPEG`、`PNG` 不會改變格式；只有存在 EXIF Orientation `2`～`8` 時才會把方向寫入像素。
- 轉檔後的圖片維持原本顯示方向，EXIF Orientation 不會再控制圖片旋轉。
- 透明像素會合成在白色背景上，因為 JPEG 不支援透明度。
- 動畫 `WebP` 不會被轉成單一影格；程式會保留原檔並列入失敗摘要。
- 若同目錄已存在同名 `.jpg`，或多個來源會產生同一個 `.jpg`，程式不會覆蓋任何檔案。
- 程式不會跟隨 symbolic link、Windows junction 或其他 reparse point，避免處理到所選資料夾以外的圖片。
- 處理期間會阻止其他程式同時寫入該張來源圖；若圖片正被可寫入方式開啟，該檔會保留並列為失敗。
- 轉檔最多會使用 4 個 workers 同時處理不同圖片，以加速大型資料夾；實際 worker 數不會超過待處理檔案數量。
- 每張新 JPEG 都會先寫入同目錄暫存檔，完成解碼、尺寸、EXIF 與 ICC 驗證後才取代來源；JPG／PNG 也會以鎖定檔案身分的安全交易替換，失敗時復原或保留原圖並繼續處理其他檔案。

處理畫面會顯示總數、目前進度、轉檔／方向修正／略過／失敗數量。全部完成後會自動回到主選單並顯示摘要。

> [!NOTE]
> `HEIF` 是容器格式；目前內建解碼器支援常見 HEVC `hvc1` 圖片，以及主要圖片單獨關聯的 `imir` 鏡像或 `irot` 旋轉。若主要圖片同時關聯 `imir` 與 `irot`，或重複關聯其中一種 transform，程式會安全保留原檔並列為失敗；僅屬於輔助圖片或 tile 的 transform 不受此限制。這是目前固定解碼器版本的組合方向限制。容器沒有方向 transform 時才會使用 EXIF Orientation 作為顯示方向備援，避免重複旋轉。

### 模型量化轉換

模型量化轉換頁面會從 model converter API 載入目前支援的模型類型與檔案限制。使用者可點擊或拖曳模型檔案、啟動遠端轉換、查看任務狀態，完成後將 `.nb` 模型下載到指定位置。頁面右上角也提供外部網站入口：

```text
https://modelconverter.ntnu-aiot.com/
```

## 開發技術

- Tauri 2
- React 18
- TypeScript
- Vite
- Rust 1.97.0（由 `rust-toolchain.toml` 固定）
- lucide-react

開發環境需使用 Node.js `^20.19.0` 或 `>=22.12.0`；CI 使用 Node.js 24。

## 安全性設定

- Tauri CSP 已限制 WebView 可連線來源。
- 外部網址開啟功能只允許預期 HTTPS 網域。
- Model converter 輪詢會在切頁、換檔、重新開始轉換或卸載時取消，避免舊任務覆蓋新狀態。

## 版本

目前版本：`3.18.1`

版本檢查來源：

```text
https://raw.githubusercontent.com/breeze0305/Realtek_AMB82mini_plugin/main/version.txt
```

版本號由 repo 根目錄的 `version.txt` 統一管理。執行 build 前會自動同步到 `package.json`、`Cargo.toml`、`tauri.conf.json` 與相關文件。

## 開發指令

安裝依賴：

```powershell
npm.cmd install
```

前端建置：

```powershell
npm.cmd run build
```

版本一致性檢查、前端 lint、格式檢查、測試與 production build：

```powershell
npm.cmd run check
```

Rust 測試：

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --locked
```

產生 Windows release exe 與 NSIS installer：

```powershell
npm.cmd run tauri build
```

輸出位置：

```text
src-tauri/target/release/amb82-mini-computer-plugin.exe
src-tauri/target/release/bundle/nsis/AMB82 Mini Computer Plugin_<version>_x64-setup.exe
```

## 授權與注意事項

本工具用於輔助 Realtek AMB82-mini 開發流程。Arduino IDE、VLC、Realtek AmebaPro2 套件與相關第三方工具仍依各自官方授權與使用條款為準。隨安裝包提供的 `THIRD_PARTY_NOTICES.txt` 記載新增原生元件的第三方授權。

## 貢獻者

### 主要貢獻者

- NTNU Feng
- Email: benfeng99@gmail.com

### 共同貢獻者

- 賴彥廷
- 范哲瑋
- 陳柏序
- 李易修
- 黃琮善
- 陳品妤
- 余品誼
- 李鍇灝
- 吳祐安
- 王威達
- 吳子安

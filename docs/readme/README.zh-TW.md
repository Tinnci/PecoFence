<p align="center">
  <img src="../assets/hero-zh-TW.png" alt="PecoFence — 把桌面還給桌布，把檔案放在手邊。實際示範桌面上的三個玻璃圍欄。" width="1280">
</p>

https://github.com/user-attachments/assets/6320cf28-a791-4720-9659-b4575df021a0

<p align="center">
  <strong>免費、開源的 Windows 11 桌面整理工具，Stardock Fences 的另一種選擇。</strong><br>
  用玻璃圍欄收好檔案，按一下快速鍵，就能在任何應用程式前取用。
</p>

<p align="center">
  <a href="#開始使用"><strong>下載使用 →</strong></a>
  &nbsp;·&nbsp; <a href="#看看它怎麼用">看看實際操作</a>
  &nbsp;·&nbsp; <a href="../README.md">專案文件</a>
</p>

<p align="center">
  <a href="../../README.md">English</a>
  &nbsp;·&nbsp; <a href="README.zh-CN.md">简体中文</a>
  &nbsp;·&nbsp; <strong>繁體中文</strong>
  &nbsp;·&nbsp; <a href="README.ja.md">日本語</a>
  &nbsp;·&nbsp; <a href="README.ko.md">한국어</a>
  &nbsp;·&nbsp; <a href="README.de.md">Deutsch</a>
  &nbsp;·&nbsp; <a href="README.fr.md">Français</a>
  &nbsp;·&nbsp; <a href="README.es.md">Español</a>
  &nbsp;·&nbsp; <a href="README.pt-BR.md">Português (Brasil)</a>
  &nbsp;·&nbsp; <a href="README.ru.md">Русский</a>
</p>

---

## 給每件事留一個位置

手上的專案、剛存下的截圖、打算晚點再看的資料——各自放進一個圍欄，
照你的習慣擺好。桌面上的東西依然順手，也終於有了秩序。

| **依專案收好** | **把資料夾放在手邊** | **隨時讓出空間** |
| :--- | :--- | :--- |
| 為工作、設計或常用資料各建一個圍欄，拖曳、縮放、吸附對齊。 | 把真實的資料夾變成桌面上的視窗；可以進入子資料夾，內容一有變動就自動同步。 | 在桌面空白處按兩下就隱藏全部圍欄，再按兩下就恢復。要用檔案時叫回來，想看桌布時收起來。 |

## 看看它怎麼用

### 一個視窗切換多種工作情境

把相關的圍欄合併成分頁，點一下就能從 Work 切到 Art。
需要同時看兩組檔案時，把分頁拖出來，就變回兩個獨立的圍欄。

![在 Work 與 Art 分頁之間切換，再把分頁拖出來，拆成獨立圍欄。](../assets/tabs.gif)

### 檔案就在目前應用程式前面

按下 **Ctrl + Alt + 空白鍵**，所有圍欄會浮現在目前的應用程式上方。
拿到需要的檔案後，按 **Esc** 回去繼續工作。

![用快速鍵讓圍欄浮現在應用程式上方，再按 Esc 返回應用程式。](../assets/peek.gif)

<sub>以上為 PecoFence 的實際操作錄影，使用示範檔案與 Fluent 主題。動圖會自動循環播放。</sub>

## 好用之處藏在細節裡

| 體驗 | 能做什麼 |
| :--- | :--- |
| **少一點手動整理** | 依類型、副檔名、名稱、萬用字元、捷徑目標、時間和大小設定規則，新檔案會自動找到自己的位置。 |
| **配得上你的桌布** | Fluent 與 Liquid Glass 兩種風格，支援淺色／深色模式、每個圍欄各自的色調、不透明度與圖示著色。 |
| **熟悉的檔案操作** | 檔案總管右鍵選單、拖放、複製貼上、多選、縮圖，以及圖示／清單／詳細資料三種檢視。 |
| **要用時展開，不用時收合** | 把圍欄收合成一條標題列，游標停留就展開；也可以鎖定已經擺好的位置與大小。 |
| **喜歡的配置留得住** | 儲存配置快照、每日自動備份、匯入匯出設定、交換兩個顯示器上的圍欄。 |
| **原生設定介面** | 設定現由 Reactor 承載的原生 WinUI 3 控制項提供，包含五個任務頁和十三種規則條件表單。headless 測試與包含關閉/重開的合成 WinUI 測試已通過；這不代表功能對等或產品驗收完成。選定發行方案自包含 Windows App SDK Runtime 2.5.1，使用者不必另外安裝，並排除 WebView2（最終套件待核驗）；執行階段會增加部署和資源成本，不宣稱大小或啟動時間有所改善。 |

自動整理只改變檔案所屬的圍欄，檔案原本的位置不會變動。
你自己發起的移動、重新命名和刪除，則和檔案總管一樣直接作用在真實檔案上。

[查看完整功能清單 →](../FEATURES.md)

## 用你熟悉的語言

**繁體中文 · 简体中文 · English · 日本語 · 한국어**  
**Deutsch · Français · Español · Português (Brasil) · Русский**

介面支援的十種語言均已內建，離線可用；檔案名稱與自訂名稱保持不變。原生 Settings 的語言選擇與本地化表單已實作；視覺、無障礙以及完整語言/DPI 驗收仍待完成。

## 開始使用

這是在 [Tinnci/PecoFence](https://github.com/Tinnci/PecoFence) 獨立維護的版本。
本版本尚未設定 Microsoft Store 和 winget 發行管道，也尚未發布任何 Release。
請在本儲存庫的 [Releases](https://github.com/Tinnci/PecoFence/releases) 或
[CI 建置產物](https://github.com/Tinnci/PecoFence/actions)中查看是否有可用建置，或依下方說明從原始碼建置。

1. 有可用的免安裝建置時，從本儲存庫的 Releases 或 CI 建置產物下載 `pecofence-<版本>-x64.zip`。
2. 把壓縮檔**完整解壓縮**到一個資料夾，執行 `pecofence.exe`。
3. 開始整理。需要開啟設定或結束程式時，在系統匣的 PecoFence 圖示上按右鍵。

**Windows 11 x64 · 免安裝版 · 不需帳號 · Apache 2.0 開源授權**

第一次執行時，會依所選語言建立「程式」「資料夾」「檔案與文件」和「桌面」四個圍欄。
結束程式時，Windows 桌面圖示會恢復顯示。

<details>
<summary><strong>系統需求、設定檔位置與使用說明</strong></summary>

- 以 Windows 11 22H2 以上版本為對象，目前主要在 25H2 上完成原生驗證；
  舊版 Windows 與多顯示器硬體組合的完整回歸測試仍在進行中。
- 設定現包含五個 Reactor/WinUI 3 原生任務頁。headless 測試與包含關閉/重開的合成 WinUI 測試已通過；
  這不等於功能對等或發行驗收。鍵盤、UI Automation/Narrator、高對比、文字縮放、全部十種語言在
  100%/150%/200% DPI 以及乾淨系統檢查仍是必要的發行驗收工作。發行套件、匯入和處理程序檢查還需確認
  自包含 Windows App SDK Runtime 及 WebView2 元件缺失。Full Windows CI 的 EXE 上限為
  6.5 MiB（6,815,744 位元組）；最終發行測量待補，不宣稱啟動或記憶體有所改善。
- 為了復原桌面圖示，請將 `pecofence-watchdog.exe` 與主程式放在同一個資料夾。
- 設定儲存在 `%APPDATA%\PecoFence\workspace.v2.json`。
  以 `--portable` 啟動，可改為儲存在程式旁的 `config` 資料夾。
- 新格式不讀取或自動遷移舊設定。原檔案保留；請明確新增工作區或匯入支援的文件。詳見[升級說明](../UPGRADING.md)。
- 玻璃效果取樣的是靜態桌布，不會折射其他應用程式的視窗或動態桌布。
- Windows 內建的對話方塊與第三方的檔案總管選單項目仍使用系統語言。
- 免安裝版尚未進行程式碼簽署。首次執行若出現 Windows SmartScreen 提示，僅在信任建置來源時點選**其他資訊 → 仍要執行**。

[免安裝版說明](../PORTABLE.md) · [多語言說明](../LOCALIZATION.md)

</details>

## 歡迎一起讓它更好

改進一句翻譯、修好一次拖曳、讓某個日常操作更順手，都非常歡迎。

[參與貢獻](../../CONTRIBUTING.md) · [改善翻譯](../LOCALIZATION.md) · [開發文件](../DEVELOPMENT.md)

<details>
<summary><strong>從原始碼建置</strong></summary>

安裝 Rust stable、Visual Studio Build Tools 的 C++ 工作負載與 Windows SDK。
Windows SDK 是開發工具，與發行套件自包含的 Windows App SDK Runtime 不同。
公開建置只使用本儲存庫與公開 registry，包含本機 `crates/spm-contracts`（0.1.0），
不需要私有 token 或 SPM checkout。選用的 `Tinnci/spm` 後端維持私有，
只有即時 SPM 資料需要它；編譯或使用一般桌面圍欄不需要，也不隨 PecoFence 散布。
詳見 [SPM 邊界](../SPM_BOUNDARY.md)。執行：

```powershell
cargo build --locked --release
```

產生免安裝版發行套件：

```powershell
./scripts/make-portable.ps1
```

Rust 應用程式位於 `crates/`，翻譯在 `locales/`，驗證與打包指令碼在 `scripts/`。設定採用
Reactor 承載的原生 WinUI 3，而非瀏覽器介面。`extras/` 下的宣傳影片專案不參與應用程式的建置。

[發行指南](../RELEASING.md) · [原始碼結構](../DEVELOPMENT.md#architecture) · [驗證門檻](../VERIFICATION_GATES.md)

</details>

---

**讓每次回到桌面，都更舒服一點。**  
[Apache 2.0 授權條款](../../LICENSE) · [第三方授權聲明](../../third_party/README.md)

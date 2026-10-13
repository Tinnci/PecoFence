# PecoFence 设置页与 SPM 面板右键菜单重设计

日期：2026-09-28。状态：设计方案，尚未实现。

## 当前实现与可观察问题

- 设置窗口由 `crates/app/src/settings_host.rs` 在打开时创建 Win32 窗口和 WebView2 控件；`ui/settings.html`、`ui/i18n.js` 编译时内嵌。页面用原生 HTML/CSS/JavaScript，通过 `window.chrome.webview.postMessage` 发送 JSON；宿主经 `Command::SettingsMessage`、`app/settings.rs` 分发，并向页面推送完整 `state`。
- 设置导航为常规、栅栏、整理规则、布局与备份、关于。栅栏设置只呈现通用外观与文件/门户属性；`fence_options_json` 不返回 `FenceContentSpec::Panel` 的提供者、实例 ID 或配置。界面没有创建 SPM 面板和绑定项目/交付范围的入口。
- 栅栏右键菜单由 `app/menus.rs` 构造 `PopupMenu`，经 User32 `TrackPopupMenuEx` 显示。标题右键触发 `Command::FenceMenu`。当前菜单对面板仍呈现文件栅栏的视图、排序、新建文件、粘贴、图标尺寸等命令；这些命令不表达 SPM 任务。
- `栅栏选项…` 会打开设置窗口，推送 `showFence` 定位到栅栏页。SPM 内容由 `PanelSpec { provider, instance_id, config_version, config }` 保存；`plugin-spm` 仅解析 `project_id`、`delivery_scope_id` 并订阅 SPM v2 读模型。面板上的刷新、复制简报、展开、跳转来源等操作已经由插件处理，但尚未进入右键菜单。
- 当前 v2 RPC 没有项目/范围目录查询。初版绑定界面必须允许输入已知 ID 并验证订阅结果；项目选择器需要后续独立扩充 daemon 契约，不能假设现有 IPC 已能列出项目。

本次视觉检查只在 WSL 浏览器中打开了原始 `ui/settings.html`。页面没有 Windows 宿主推送的真实配置状态，原生菜单和 WebView2 行为仍需 Windows 实机验证。

## 用户任务与信息架构

优先让用户完成四件事：创建项目面板、绑定项目和交付范围、看清连接/数据可信度、在面板上执行常用动作。

```text
设置
├─ 常规
├─ 栅栏                     文件与门户栅栏的外观、行为
├─ 项目面板                 已绑定面板列表、添加、项目/范围绑定、状态与动作
├─ 整理规则
├─ 布局与备份
└─ 关于

项目面板 / 选中面板
├─ 项目与交付范围            当前为手工输入 ID；后续接项目目录
├─ 连接与数据                daemon 连接、最近更新、revision、范围覆盖、来源错误
├─ 快捷操作                  刷新、复制简报、打开关联来源
└─ 窗口外观与行为            指向已有栅栏设置，不复制一套配置
```

入口：设置导航的“项目面板”；托盘的“新建项目面板…”；SPM 面板右键“面板设置…”直接定位该实例。普通文件栅栏继续使用“栅栏选项…”。创建流程先采集项目/范围 ID，校验格式，再建立版本化 `PanelSpec` 和虚拟栅栏，显示“正在连接”；成功收到匹配的快照后显示数据状态。订阅失败时保留配置和面板占位，给出可重试操作。

## 右键菜单

以被点击对象的内容类型构造菜单。容器命令（展开/卷起、重命名、锁定、标签页、删除）仍由宿主处理；内容命令由面板提供者声明并执行。菜单只放常用任务，诊断详情放设置页。

```text
SPM 面板标题/空白处
  刷新数据
  复制当前简报                 无快照或能力不可用时禁用；原因见设置页状态
  ──────────
  展开/卷起
  标签页 …                    有适用目标时出现
  锁定位置和大小
  ──────────
  面板设置…                   直达选中实例
  删除面板

SPM 事项行
  打开来源…                   只对有可解析 SourceRef 的行显示
  复制事项编号
  ──────────
  面板设置…
```

“复制简报”继续走 daemon `BuildBriefing`，使用快照 revision，不能在菜单层拼接业务文本；“打开来源”继续走 daemon `ResolveNavigation`。不可把“刷新数据”解释为绕开来源新鲜度或覆盖率门槛。右键菜单不展示通用文件菜单中的“新建文本文档/图标排序/粘贴”。

## 技术选择与职责边界

| 层 | 选择 | 理由 |
| --- | --- | --- |
| 设置 | 沿用按需创建的 WebView2 和原生 HTML/CSS/JavaScript | 现有窗口、主题、桥接协议可复用；第一阶段无需引入 React、Node 打包链或第二套 UI 框架。把 939 行单页按导航、通用栅栏、项目面板、消息客户端拆成源文件，再由宿主嵌入页面；未来若使用浏览器 ES 模块，先解决本地页面 URL/资源映射。 |
| 右键菜单 | 沿用 Win32 `HMENU`/`TrackPopupMenuEx` | 与桌面窗口、键盘导航和 Explorer 文件右键菜单相容；菜单内容按提供者能力生成。 |
| 面板 | 沿用 Rust 插件 API 与 Direct2D/Composition | 面板和桌面容器保持相同渲染、缩放和生命周期；WebView2 仅服务设置窗口。 |
| 数据 | 沿用 SPM v2 named pipe/版本化契约 | daemon 对项目状态、简报和来源解析保持权威；设置页只绑定配置并展示只读状态。 |
| 配置 | `PanelSpec` 继续存项目/范围 ID | 通用 core 只保存提供者描述符；SPM 负责类型校验。不要在 `Settings` 再存一份项目/范围。 |

遵循单一职责与依赖倒置：宿主定义 `PanelSettingsDescriptor`（标题、可编辑字段、状态摘要）和 `PanelMenuAction`（稳定 action ID、标签、可用性）；提供者填充描述符，宿主负责安全展示、消息路由和持久化。第一阶段只注册内建 SPM 提供者，避免过早设计动态插件 ABI。通用设置消息应改为明确的 `createPanel`、`updatePanelBinding`、`invokePanelAction`；每个消息带实例 ID 和请求 ID，宿主按当前实例及 `config_version` 校验。不要使用当前的 `patchSettings` 整对象提交面板配置。

更新绑定时保留旧配置直到新配置验证通过，再替换 `PanelSpec` 并重建订阅；如果激活失败，保留原配置或明确显示待修复状态，不静默回退为文件栅栏。对状态页区分“IPC 已连接”和“数据可用于门槛判断”，分别显示快照时间、覆盖率、来源错误与 revision。

## 实施顺序与验收

1. 加入内容类型判定，使面板右键菜单不显示文件命令；增加面板动作路由和“面板设置…”深链。
2. 增加项目面板设置页和“创建/编辑绑定”消息，持久化版本化配置；输入无效、daemon 离线及未知项目范围均有可恢复反馈。
3. 将设置页巨型脚本拆为小模块，补上消息 schema、request ID 和过期响应保护；实现状态摘要而非复制整个业务模型。
4. 如需可搜索项目选择器，再为 SPM v2 增加目录查询契约，不能由 PecoFence 猜测本地数据库或配置文件。

WSL 可验收：Rust 单测/交叉编译、设置页静态布局与可访问性、菜单模型纯逻辑。Windows 实机可验收：WebView2 消息回路、原生菜单位置与键盘/屏幕阅读器行为、面板创建/重新绑定/离线重试、DPI 与多显示器、真实 named pipe 连接和 SPM 动作结果。

参考：[WebView2 消息与安全](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security)、[WebView2 Win32 开发](https://learn.microsoft.com/en-us/microsoft-edge/webview2/get-started/win32)、[Win32 可访问性实践](https://learn.microsoft.com/en-us/windows/win32/winauto/accessibility-best-practices)。

## 技术选型补充：`windows-reactor`（2026-09-28）

微软 `windows-rs` 已发布 `windows-reactor` 0.100.0：Rust 的声明式 WinUI 3 控件库，与本仓使用的 `windows-window`、`windows-canvas` 等同属 0.100 系列。它适合项目绑定表单、导航和状态列表，值得为“项目面板设置”做独立原型。它不改变本方案的用户任务、配置边界和菜单命令模型。

当前发布版的 `App::run*` 自己启动 WinUI 应用及消息循环；`MenuExt` 把 WinUI 菜单挂到 Reactor 的 `View`。本仓 `FenceWindow` 已有独立 Win32 消息循环和 `WS_EX_NOACTIVATE` 桌面窗口，不能把 Reactor 菜单当作现有 `PopupMenu` 的直接替换。WinUI Composition 与本仓当前的 `Windows.UI.Composition` 类型也不是同一套对象；加入 `windows-reactor` 本身不启用本仓 `windows-composition/reactor` feature，因此不应把这一点误写成必然的 Cargo feature 冲突。

原型验收应先验证：独立 WinUI 设置窗口与现有进程的生命周期/消息循环如何共存，Windows App Runtime 部署体积、启动延迟、DPI/输入法/高对比度、以及配置编辑到 Rust 宿主的可靠消息回路。原型通过后再决定是否替换 WebView2 设置页；桌面 Fence 的原生容器和文件右键菜单先保持原实现。

资料：[windows-reactor 0.100.0](https://docs.rs/windows-reactor/0.100.0/windows_reactor/)、[windows-rs Reactor 指南](https://github.com/microsoft/windows-rs/blob/master/docs/crates/windows-reactor.md)、[windows-rs 2026 年 8 月发布说明](https://github.com/microsoft/windows-rs/issues/4867)。

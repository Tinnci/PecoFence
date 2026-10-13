# Fence 渲染、效果图与小组件架构

日期：2026-09-28。根据当前源码及本轮改动整理；WSL 可以编译 Windows target，但还没有 Windows 桌面/GPU 实机帧时间。

## 当前有哪些管线

| 管线 | 输入 → 处理 → 输出 | 主要实现 |
| --- | --- | --- |
| 文件图标 | Shell 路径 → STA worker 提取 BGRA → CPU IconCache → GPU BitmapCache → Direct2D cell | `platform/shell.rs`, `app/icons.rs`, `render/bitmaps.rs`, `render/fence_chrome.rs` |
| 名称/标签 | Shell display name → ItemView 名称 → 按字号、宽度、行数拟合并缓存 → DirectWrite/Direct2D 文本 | `platform/shell.rs`, `app/fence_window/render.rs`, `render/fence_chrome.rs` |
| 文件内容 | 文件目录与规则 → `ItemView`/布局/可见项 → Direct2D content surface | `app/state.rs`, `app/fence_window/render.rs` |
| Fence 外观 | 壁纸快照 → monitor 映射/背景 → 圆角背景、标题、边框 → chrome surface | `app/visuals.rs`, `render/backdrop.rs`, `render/fence_chrome.rs` |
| GPU Glass | 共享壁纸 GPU 图像 + CPU 生成的低分辨率位移场 → Direct2D 内建模糊、位移、色矩阵、合成效果图 → chrome surface | `render/gpu_glass.rs`, `render/liquid_glass.rs` |
| 小组件/SPM | provider 状态 → `PreparedFrame`/命中树/语义树 → Canvas/Direct2D content surface | `plugin-api`, `plugin-spm`, `plugin-widgets`, `app/fence_window/plugin_panel.rs` |
| 合成和动画 | chrome/content SpriteVisual → Windows Composition → HWND；位置/透明度动画只改 visual 属性 | `render/stack.rs`, `render/panel.rs`, `render/motion.rs` |

仓库目前没有自编写的 HLSL/编译好的 `.cso` shader。`gpu_glass.rs` 创建的是 Direct2D **内建效果图**，其中包括 GaussianBlur、DisplacementMap、ColorMatrix、Saturation、ArithmeticComposite；位移场和高光数据由 Rust CPU 算好并上传。非 Liquid Glass 的 Acrylic 走 CPU 壁纸映射、三次 box blur 近似 Gaussian、色调处理，再把 crop 作为位图绘到 Direct2D。纯色材质不需要壁纸采样。DWM 负责桌面合成，当前没有把 Fence 的 Acrylic 背景交给 DWM system backdrop 生成。

## 本轮小组件边界

```text
配置 PanelSpec(provider, instance_id, config_version, JSON)
  → PanelManager 静态注册并管理实例/生命周期
  → ClockPlugin / CalendarPlugin 本地日期和状态
  → PreparedFrame 同时提供 paint、hit tree、semantic tree
  → Direct2dCanvas → content CompositionDrawingSurface
  → 与 FenceChrome 独立合成
```

所有需要定时变化的第一方 provider 可以实现 `PanelInstance::next_wake_after`，宿主只用一个控制 HWND 的 Win32 timer，按最早截止时间唤醒对应实例，接收 `PanelEvent::Wake`，合并 `PanelUpdate::Invalidate`。数字时钟对齐秒/分钟，月历按本地午夜并每小时兜底检查；`WM_TIMECHANGE` 和 `WM_SETTINGCHANGE` 触发即时重读。删除 Fence 后停止实例和其定时器。时钟/月历没有进程边界或 IPC；SPM 的命名管道仍只承载它自己的 read model 与操作。

新组件由托盘和 Fence 右键菜单创建为单独的 `FenceContentSpec::Panel`。外框复用 FenceChrome，内容使用同一个 Canvas API。配置结构已支持 12 小时制、显示秒、周日起始，但设置页面尚未提供这些编辑项；当前可通过配置 JSON 指定。日期网格具备点击和语义节点，宿主尚未把语义树接到 UI Automation，也没有通用键盘焦点，因此完整键盘/读屏交互仍待补。

## 已做的安全优化与下一步

本轮让 `Direct2dCanvas` 在**一次 paint 内**复用同字体角色、字号和字重的 DirectWrite `TextFormat`。月历 29–31 个日期格不再为每格重复创建相同格式。缓存随画帧销毁，不跨设备重建保留无效 COM 资源。

下一步按 Windows 实机分析选择：

1. 对文件列表做可见区准备，避免每次滚动扫描全部 `N` 项；绘制已有可见区剔除，但准备仍有 `O(N)` 工作。
2. 为静态文字和图标设置有预算的布局/位图缓存；当前图标缓存没有明确容量上限，组件 Canvas 的图像资源尚未接入宿主。
3. `Panel::draw_ex` 目前总是整张 surface `BeginDraw`，局部更新需扩展封装的 update rect、偏移和首次完整初始化；先量测 hover 与时钟更新的面积/耗时，再决定是否实现。
4. GPU Glass 已复用壁纸纹理和效果图，移动主要更新 transform；若仍有 GPU 瓶颈，应分别量测效果图、上传、chrome 重绘与 composition 开销，再考虑降低位移图精度或合并效果节点。不要仅凭“shader 多”判断瓶颈。

参考：[Direct2D 内建效果](https://learn.microsoft.com/en-us/windows/win32/direct2d/built-in-effects)、[BeginDraw 更新矩形](https://learn.microsoft.com/en-us/windows/win32/api/windows.ui.composition.interop/nf-windows-ui-composition-interop-icompositiondrawingsurfaceinterop-begindraw)、[Direct2D 性能指南](https://learn.microsoft.com/en-us/windows/win32/direct2d/improving-direct2d-performance)、[SetTimer](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-settimer)。

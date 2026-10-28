# 桌面栅栏渲染分层拆解

日期：2026-10-17。性质：依据当前源码的结构分析与改进提案；尚未实施重构，尚无本轮 Windows 实机视觉/性能结果。

## 结论

保留有价值的桌面宿主与业务能力，分别改进材质、内容清晰度、交互和帧调度。是否引入 WinUI 3 应由内容层原型验证决定，不应把“换渲染技术”当作所有问题的统一解法。本提案不包含生成插画。

## 一、当前结构

```text
业务/文件系统/设置投影
           ↓
应用状态、ItemView、布局与交互状态
           ↓
FenceWindow（HWND，桌面锚定与不激活行为）
└─ Composition root
   ├─ chrome panel：背景材质、标题、标签、边框等
   └─ content panel：图标/文字/列表/选择等
阴影：另有 ShadowWindow，生命周期与栅栏协调
```

并非所有层都缺失：当前已有 core/app/platform/render 划分，chrome/content 分面，GPU glass 与壁纸上传缓存，compositor animation 和按需 client frame clock。主要问题是视觉职责仍集中在若干绘制和窗口状态模块，分层应逐步梳理，避免一次重写。

## 二、七层职责与改进边界

| 层 | 职责 | 当前主要入口 | 改进方向 | WinUI 3 的位置 |
|---|---|---|---|---|
| L0 业务与数据 | 文件、规则、标签内容、持久化、命令与回执 | crates/core、app/state.rs、app/commands.rs | 保持唯一业务状态；视图只提交意图 | 原生控件也不能直接修改业务配置 |
| L1 桌面与窗口宿主 | Explorer 锚定、HWND、层级、不抢焦点、Peek、多屏、DPI、恢复 | app/anchor.rs、platform/desktop.rs、platform/window.rs、app/peek.rs | 明确生命周期、几何、激活与输入契约 | WinUI 内容托管在此层之上，不能自动替代这些行为 |
| L2 呈现状态、布局与命中 | 可见条目、几何、截断、选择、hover、滚动、拖放目标 | app/layout.rs、fence_window/state.rs、hit.rs、items.rs、selection.rs、dnd.rs | 同一布局结果服务绘制与命中；分离语义状态和视觉进度 | 两个内容原型应消费相同条目与业务意图；布局可由各自实现拥有 |
| L3 图形设备与合成 | GPU 设备、surface、visual tree、裁剪、变换、设备丢失恢复 | render/stack.rs、panel.rs、motion.rs、app/fence_window/render.rs | 清楚区分栅栏几何变化与内容像素变化；细分失效 | 现有 Windows.UI.Composition 与 WinUI 的 Microsoft.UI.Composition 对象不可直接混用 |
| L4 材质与外观 | 壁纸来源、模糊、色调、折射、边缘、圆角、阴影、对比度 | render/backdrop.rs、gpu_glass.rs、liquid_glass.rs、theme.rs、app/shadow.rs | 分离背景采样、光学效果、chrome token、可读性策略 | XAML 控件层不会自动赋予真实实时背景折射 |
| L5 内容绘制/控件 | 字体、图标、缩略图、列表/网格、标签、滚动条、焦点视觉 | render/fence_chrome.rs、text.rs、bitmaps.rs、app/icons.rs、fence_window/render.rs | 测量字体和像素对齐、按 DPI 请求图标、可见区准备、复用文本布局 | 最适合验证 WinUI 的 TextBlock/ListView 等原生控件；仍需行为对齐 |
| L6 动效与反馈策略 | 状态之间如何过渡、何时显示反馈、减少动画、高对比、可见性 | render/motion.rs、platform/frameclock.rs、fence_window/frame.rs、app/motion.rs | 统一触发规则与时间；合成变换和逐帧重绘分别管理 | 控件原生状态动画与自绘动画各用所属体系，避免叠加重复反馈 |

辅助技术、DPI、主题、性能和生命周期是跨层约束，不是最后补一个“无障碍层”即可解决的问题。

## 三、视觉树如何渐进演进

建议目标（不是当前已经实现）：

```text
Fence HWND / root
├─ material：壁纸/玻璃背景
├─ chrome：标题、标签、边缘
├─ content：文件图标与名称，或将来验证的 WinUI 内容宿主
└─ feedback：hover、选择、焦点、拖放提示
```

不要求第一步就变成四个 surface，也不为每个文件创建 HWND。先建立逻辑失效边界，测量后再决定独立 Visual/surface 的粒度。大量小对象也有管理成本。

- 材质不因 hover 一次就重新准备壁纸。
- hover 只更新相应反馈区域；几何整体改变、滚动和布局重排采用独立策略。
- surface 脏区更新需要支持 BeginDraw 更新矩形、返回偏移、裁剪与首次初始化，不能只在 CPU 端少算几个项目就宣称已完成局部重绘。
- 阴影当前拥有独立窗口；先保持宿主契约正确，再评估是否有必要更换阴影机制。

## 四、按感知问题定位

| 用户感知 | 首先检查 | 不应直接推断 |
|---|---|---|
| 玻璃灰、脏、塑料感强 | 背景来源/分辨率、叠色与透明度、模糊和边缘幅度、圆角阴影一致性 | Direct2D 无法产生精致材质 |
| 文字/图标发虚 | 最终 DPI、像素坐标、位图分辨率、重复缩放、透明表面文字渲染策略、动画结束后的像素归位 | 换成 XAML 就能消除所有模糊 |
| 滚动/hover 卡顿 | UI 线程准备、绘制提交、全 surface 更新、缓存命中、是否落到 WARP、实际帧间隔 | 显示器刷新率或 GPU 必然不足 |
| 像“自己画的按钮” | 状态完整性、间距/字号、焦点、按下与选中层级、主题适配 | 再增加动画时长就会更现代 |
| 多屏拖动/收起时跳变 | 坐标空间、DPI 切换、宿主几何、裁剪与合成时序 | 仅调 easing 就能修复 |

当前玻璃读取静态壁纸；实时桌面内容是另一项背景采集能力及性能/隐私边界。其需求须单独确定。文件名应处于清晰内容层，不参与背景折射。

## 五、三个可独立推进的工作包

### A. 视觉基线与材质

在同一台 Windows 机器、壁纸、窗口尺寸和 DPI 下，记录深浅主题、普通/液态材质、桌面/Peek、静止/拖动/收起状态。确认真实运行是否出现 GPU fallback。先调对比度、轮廓、阴影与光学幅度，再评估是否需要改变材质来源。

### B. 内容质量与更新范围

核对 100%/150%/200% DPI 下的文字与图标；按可见区准备项目、缓存文本布局、限制图标请求与缓存；分解 material/chrome/content/feedback 失效。已有性能核对见 RENDER_COMPLEXITY_AUDIT_2026-09-28.md，其历史观察需要对照当前代码复核。

### C. WinUI 内容层验证

另建实验入口，仅覆盖一个普通文件栅栏，使用相同测试数据，保留现有桌面宿主。先验证 DesktopWindowXamlSource 的焦点、非激活行为、尺寸/DPI、剪裁、背景与层级，再加入选择、Shell 拖放、键盘和上下文菜单。托管 API 的可用性不等于本仓库 Reactor 已支持嵌入式宿主；适配成本需独立核对。

两套实现只作为有范围、有期限的对比实验，不在正式产品中无期限维护两套业务状态。比较质量、功能覆盖、启动/内存/空闲资源和改动规模，再作选择。

## 六、动效如何加在正确层

- 平移、淡入、裁剪、缩放：优先使用所属合成体系的动画，避免不必要的逐帧内容重绘。
- hover/selection：先使用覆盖层或明确的局部更新；文字不跟随背景变形。
- 列表变化：以稳定 ItemId 保留选择与位置连续性，批量变化控制动画数量。
- 几何操作：命中布局与最终视觉必须一致，动画可被新的操作打断。
- 保存反馈：从真实业务回执触发；加载、错误、离线、无内容不能共用一个视觉状态。
- 尊重系统动画设置；不可见、收起、关闭时不维持装饰性帧循环。

## 七、落地顺序与验收

1. 基线记录与分段计时，不先换宿主或持久化模型。
2. 在现有渲染器完成一个普通文件栅栏的材质、清晰度与反馈改进。
3. 在独立实验入口实现 WinUI 内容层，使用同一组测试场景对比。
4. 根据 Windows 真实结果选择继续自绘、采用 WinUI 内容层或分场景组合。

验收至少覆盖：静止、滚动、框选、拖放、标签切换、Peek、窗口收起/展开、多屏 DPI 切换、系统减少动画、高对比、键盘与辅助技术、Explorer 重启及图形设备恢复。

现有文档中的滚动绘制提交 p95 ≤ 8ms、静止平均 CPU ≤ 0.5% 是待验证目标，不是本轮实测成绩。Debian 下的静态检查和 headless tests 不足以证明 Windows 图形质量。

## 外部参考

- [Win32 使用 Visual Layer](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/using-the-visual-layer-with-win32)
- [WinUI DesktopWindowXamlSource](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.hosting.desktopwindowxamlsource)
- [WinUI 3](https://learn.microsoft.com/en-us/windows/apps/winui/winui3/)
- [Mica 材质](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)

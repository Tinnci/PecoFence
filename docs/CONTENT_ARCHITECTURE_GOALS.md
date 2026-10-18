# 桌面内容架构目标与验收

日期：2026-10-17。本阶段保留 Direct2D/Composition，先修复抽象与提交边界。

## 分层目标

| 层 | 职责 | 本阶段结果 |
| --- | --- | --- |
| 业务 | 保存内容、处理动作、持久化和 SPM 协议 | 保留原 provider 与业务测试 |
| 布局适配器 | 按模式和文字比例产生 DIP 节点 | WidgetFrame 成为只在准备期存在的构建器；SPM 通过 prepare_scene 转换 |
| 场景 | 冻结节点及版本，派生绘制、命中和语义信息 | 新增 plugin-api::scene::Scene，所有内置插件共用 |
| 呈现规则 | 字体角色、色彩、裁剪、反馈及像素对齐 | 复用 design::Component、ContentScale、PixelGrid 与实际 ThemeSnapshot |
| 内容宿主 | 输入手势、焦点、失效、候选帧提交 | 使用已提交帧处理输入；反馈复用该帧，不重新准备业务 |
| Windows 渲染 | DIP 到像素、设备、surface 与 Composition visual | 插件和文件内容使用可复用的双 surface，成功完成绘制和 EndDraw 后才切换 |

文件网格/列表保留专用绘制适配器，新增 FileScene 作为已发布几何快照，共享 plugin-api::file_geometry；点击、框选、拖动预览和重命名读取该快照。它与插件 Scene 目前仍是两种场景适配器，尚未统一成完整的文本/图像场景树；chrome 仍为专用路径。

## 本阶段完成条件

- 内置便笺、待办、时钟、日历、SPM 的绘制/命中/语义由同一组不可变节点派生。
- 准备期拒绝非有限或负尺寸、重复 ID、无效样式、悬空父节点和循环父子关系。
- WidgetFrame 构建器不能作为 PreparedFrame 交给宿主，必须消费构建器并成功生成 Scene。
- 插件绘制或反馈绘制失败时，不发布被清空或只绘制了一部分的候选 surface，不替换已提交命中树。
- 保留 DPI 与文字缩放的职责分离、实际主题、既有业务协议及原生文本编辑能力。
- SPM 的数据来源变化推进共享布局版本；过期来源或布局的动作被拒绝，即使新旧命中矩形完全相同。
- 平台无关回归、Windows 目标检查和原生测试验证代码边界；单独记录实际显示器验收范围。

## 提交约定与代价

```text
业务状态 → 布局节点 → 验证/冻结 Scene → 绘制未发布 surface
                                          ↓ 成功完成 EndDraw
                              切换 visual brush → 更新宿主 committed frame
```

以上在同一个 UI 线程执行；它保证宿主输入树与成功发布的候选内容对应，不保证 GPU 已在显示器上扫描完成。回调失败仍会结束 D2D 绘制，但该 surface 不发布。设备丢失继续沿用原恢复路径。

双 surface 为采用 draw_candidate/draw_candidate_ex 的插件和文件内容惰性分配，暂存中的 outgoing/spare 内容 Panel 也可能保留双缓冲。第二块 BGRA surface 的原始像素容量约为 `物理宽 × 物理高 × 4` 字节，尚未计入驱动/图集开销；每个候选还会创建 surface brush。之后可针对性能数据缓存 brush 或拆分反馈层，当前不声称已做脏区优化。

窗口 resize 仍先调整已有 surface；设备重建仍替换设备资源。这两个路径不属于“失败时保留旧像素”的保证范围。Provider 准备过程也不是业务事务：失败后业务/布局版本可能已推进，旧帧动作可能被 provider 拒绝；不会回滚已发送业务请求。

Scene 当前表达矩形文本组件，父子关系用于语义结构，不隐含父节点裁剪或布局继承。图标、位图、文件拖放、虚拟化和原生编辑器仍由现有专用路径负责。避免为了形式统一把这些行为压进一个万能节点。

## 后续收敛顺序

1. 文件几何快照与稳定 ID 已接入绘制、命中、框选、拖动预览和编辑位置。下一步补齐共享场景的图像原语及文件语义适配，保持 Shell 拖放和选择规则；不能把独立 FileScene 称作所有内容已经使用同一个 Scene。
2. WidgetFrame 已改用具名 WidgetElement 配置（几何、文字、样式、动作、语义角色）；后续继续提炼标题、动作、行、空状态等角色组件，并按可用尺寸设计布局。
3. 插件已区分 Clean/Feedback/Content/Layout，隐藏更新保留最强类别，反馈提交不会清除内容/布局工作；后续根据帧耗时与 surface 内存数据决定反馈分层和脏区更新。
4. 检查真实 UIA 接入、键盘入口和高对比主题，不能把语义节点存在等同于辅助技术已可用。
5. 完成 Windows 实际显示器矩阵：100/125/150/200% DPI，独立 100/150/200/225% 文字尺寸，跨屏、长中英文、最小窗口、深浅/高对比、Peek、拖放和焦点操作。

本阶段源码改进不替换用户当前安装或配置。实际视觉与交互验收仍是下一阶段工作。


## 第四轮：文件已发布几何

- 排序/插入动画原来按瞬时位置绘制、按最终位置命中。FileScene 在绘制前采样一次运动值，绘制和输入共用冻结矩形；重叠项按绘制顺序命中，展开标签作为最上层目标。
- 命中结果携带 ItemId，当前列表若已重排则重新按 ID 定位；消失文件不会被新的同索引文件替代。
- 文件文字与图标的矩形由平台无关 FileItemGeometry 计算，实际绘制、重命名和标签点击复用该几何。
- 点击只查当前可见节点；框选保留全部节点以支持跨滚动范围的选择。已移除项的退场图像不进入输入快照。
- 文件候选画面绘制及 EndDraw 成功后才发布 FileScene。错误时保留前台 surface 和此前提交的几何。
- Panel 的 surface_epoch 在创建、resize 或 recreate 后变化，候选发布本身不改变它。即使尺寸改回原值，旧文件几何也不能复用；插件也在资源代次、DPI 或设备改变时清理旧输入帧和反馈。
- 隐藏/未激活插件跳过绘制并清理交互；这个分支不再把“未绘制”当成“设备丢失”。

仍需验收：实际 OLE 拖动、跨屏/折叠动画、重命名编辑器和长文件名展开的桌面交互。后续轮次继续收敛字体、语义和失效类别；文件 UIA 桥、反馈分层和桌面实机验收仍需完成。


## 第五轮：文件内容排版

- ContentTypography 将列表正文、固定表头、分组标题和空状态字体作为一个整体构建，成功后才替换旧资源；100%–225% 文字比例与显示 DPI 分开。
- RowMetrics 和 GridMetrics 从实际 DirectWrite 行高计算文字带，保留最小尺寸和 4 DIP 对齐；分组高度进入 Bands 的排布、命中与滚动计算。
- 设置广播同时更新系统图标标题字体和文件内容字体，任一变化都使拟合标签失效、取消旧布局动画并重算自动高度。
- 固定表头高度和列位置保存到 FileScene，输入读取已发布几何；列文本按可用宽度拟合，并在实际内容绘制时裁剪。
- Windows 原生绘制覆盖 100/125/150/200% DPI 与 100/150/225% 文字比例组合，包含表头、分组、窄列和空状态。它验证 D2D 调用/提交，不等同于屏幕图像验收。

## 第六轮：语义和失效契约

- SemanticNode 的 ID 变为有默认类型的泛型：插件保留 u64，文件保留完整 ItemId，避免截断 UUID。字段与角色契约共用，不修改业务身份。
- FileScene 从冻结几何和完整文件名称生成语义快照。点击读取同一语义矩形，展开名称同步扩展语义边界；显示时省略的文字不会损失完整名称。诊断 dump 报告已提交语义节点数量。
- 上述快照是 Windows UIA 适配的数据基础，当前没有实现 WM_GETOBJECT/UIA provider，不声称屏幕阅读器已能读取它。
- FrameInvalidation 保留反馈、内容、布局工作的优先级；隐藏或折叠更新不会把布局要求降为普通重绘。宿主先处理待办布局，再提交内容；失败保留未完成工作。
- WidgetElement 用具名字段替代 24 处多位置参数构建调用，保留业务布局、颜色、动作和语义角色；准备期仍只缩放一次并生成不可变 Scene。
- 新增 Windows 宿主集成测试，实际绘制部分内容后注入错误，验证输入仍绑定旧提交；反馈不重新准备 provider，也不能清除待布局状态；换 surface 后旧输入失效；隐藏跳过绘制不报告设备丢失。

仍在目标内的差异：文件和插件尚有不同的场景适配器，图像/材质与 Shell 专用交互还需进一步收敛；标题/chrome 的完整缩放与高对比策略、UIA 对接、反馈资源复用及实际桌面交互验收尚未完成。


## 第七轮：标题与标签页缩放

- 字体资源包扩展并更名为 FenceTypography，将标题的三档字号、居中标签页和折叠计数纳入同一次更新；字体格式复用并以轻量 COM clone 提供给测量与绘制。
- 标题高度由三档标题中最大的实测行高加留白得到，保留 36 DIP 下限和 4 DIP 对齐。缩小文字会重新计算高度，不把放大后的旧值当作新下限。
- 应用创建、主题刷新和系统文字设置刷新均使用同一计算值；窗口内容偏移、折叠高度、标题按钮及标签命中继续使用该 Theme.title_height。
- 文字变化清除标签自然宽度缓存和旧位置动画，重建 infotip；正在折叠/展开时按目标状态结算高度。HWND 尺寸变更在释放 view 借用后执行，避免 WM_SIZE 同步重入。
- 折叠计数使用缩放后的字体并拟合保留宽度；图标/箭头仍保持原 DIP 尺寸，由显示 DPI 转换，避免与文字比例重复放大。
- Windows 原生 D2D 测试扩展到缩放后的标题、装饰按钮和折叠计数；标题/标签页字体度量、缩放恢复基线、折叠高度与中途动画结算有独立回归。

此轮未接入标题栏已提交几何快照；标签页命中仍按实时动画计算，chrome 绘制仍使用现有路径。这是后续与文件/插件提交模型收敛的明确差异。高对比、UIA 和实际桌面跨屏/编辑验收也仍未完成。


## 第八轮：标题栏提交快照与原生交互验证

ChromeScene 将标题、标签页、返回和折叠按钮的绘制几何冻结到成功提交的 surface；命中、拖动抓取位置和 tooltip 读取同一快照。标签使用稳定 ContentId，保留完整名称，过期存储、活动内容、DPI、设备代次或尺寸不再复用旧输入。标题绘制也采用候选 surface，仅绘制成功才发布。

Windows 原生测试共 104 项通过（render 44、窗口/宿主 43、布局 17）；Linux 平台无关测试 44 项通过，Windows 应用严格 Clippy 通过。隔离配置的原生 GUI 脚本验证标签合并、拖动排序、恢复排序、取消拖动和折叠/展开，6 次快照均无残留 capture/tab_drag；展开语义节点 16、折叠 4，无 ERROR/PANIC/device-loss。运行的是本地 NTFS 上的开发构建，显示 DPI 为 96；不代表物理鼠标、跨显示器、视觉像素或完整 OLE 验收。UNC 夹具监视与未启动 SPM daemon 的警告不纳入本次验收。

候选双缓冲覆盖插件、文件正文和标题栏；每个 surface 的额外显存约为宽×高×4 字节。设备重建/resize 期间不保证保留旧像素，提交快照也不代表显示器扫描输出已完成。高对比、Windows UIA provider、图像/材质场景收敛与真实跨屏输入仍待完成。


## 第九轮：插件输入取消与可交互生命周期

确认并修复：Esc 原来流入文件键盘处理器，未直接清除插件按下快照；重绘若保留布局身份和命中矩形，后续松键仍可能触发动作。插件输入现在有统一的指针取消、全部输入取消和单次松键解析入口。Esc、WM_CANCELMODE、失去焦点、捕获变化各自明确取消范围；原生捕获释放放在 view 借用之外。系统取消/捕获变化保留键盘焦点，Esc/失焦清除焦点；标题拖动的 Esc 继续交给标题处理器。

折叠、折叠/展开动画、淡出、退役、隐藏或未激活内容不接受插件正文输入。成功提交的正文帧可以保留供动画使用，取消时通过反馈重绘去除旧焦点/按下墨迹，不为清除反馈重新准备业务数据。

原生宿主回归增加同一提交帧上的正常松键、重复松键、指针取消后松键、完整取消后松键和重复取消断言，检查保留焦点的范围、帧身份及 prepare 调用次数。此回归验证宿主状态和真实 Composition 反馈绘制；不代表完整原生消息路由、物理键鼠或跨屏操作已验收。

本轮验证：Windows 窗口/宿主原生测试 43 项通过（含扩展的取消回归）；Windows 应用 all-targets Clippy（-D warnings）通过；Windows 开发版构建及 Windows App SDK Runtime 2.5.1 暂存成功。高对比、UIA 与完整桌面输入/跨屏验收仍为未完成项。


## 第十轮：资源失效与原生捕获释放

资源绑定变化或隐藏绘制原来直接丢弃 pressed，因而丢失该手势的 ReleaseCapture 责任。现在取消会保存独立的待释放标记，重复绑定/取消不覆盖该标记；取出后只执行一次。WM_CAPTURECHANGED 确认 OS 已移交所有权，会清除旧请求。

窗口过程入口在派发新手势前处理旧请求，出口使用 RAII 边界覆盖提前返回和原生 resize/DPI/重绘中的取消。取出标记时持有 view 借用，实际 ReleaseCapture 则在借用释放后执行，允许同步 WM_CAPTURECHANGED 回调。请求只对仍拥有捕获的 HWND 生效。

控制窗口命令也可能在 fence 窗口过程之外重绘，因此重绘路径即使返回错误，也会为待释放请求投递专用唤醒消息。该消息只处理待办效果，不模拟 WM_CANCELMODE，已清理的旧唤醒不会主动取消后来开始的新手势。

Windows 窗口/宿主测试 44 项通过。扩展回归检查取消效果仅取出一次、资源变化后重复绑定不会丢失请求；新增隐藏 HWND 原生测试实际调用 SetCapture/GetCapture/ReleaseCapture，验证所属窗口、重复释放和同步通知。未将该测试等同于真实跨屏拖动、完整宿主消息路由或全生命周期无竞态证明。

本轮 Windows 应用 all-targets Clippy（-D warnings）、开发版构建和固定 Windows App SDK 运行时暂存均通过；高对比、UIA 和真实跨屏交互验收继续保留。


## 第十一轮：捕获效果归属窗口

复查卸载和标签切换后确认，组件内的待释放 bool 生命周期过短：release_panel_view 或 set_content 替换对象会连同待办效果一起销毁。窗口现在持有共享的待释放 Cell，插件只持有引用；PluginPanelContent::drop 登记取消，窗口仍能在组件已为空时完成原生释放。

窗口过程入口先绑定仍存在的插件资源，再处理窗口级待办；没有插件时也必须先释放旧捕获，才交给文件或标题处理器。with_view 和 try_apply_panel_update 增加借用结束后的效果处理边界，覆盖控制窗口发起的更新、组件替换和卸载，不依赖将来的一次鼠标移动。保留专用异步唤醒作为其他重绘入口的处理路径。

原生宿主回归扩展为：成功提交并按下控件后销毁整个 PluginPanelContent，窗口持有的待释放请求仍然存在且只消费一次。此测试证明组件销毁不会丢弃标记；真实标签切换消息链、物理输入与跨屏验收仍需独立验证。

本轮 Windows 原生窗口/宿主测试 44 项、应用 all-targets 严格 Clippy、开发版构建及运行时暂存通过。高对比、UIA 和真实桌面输入/跨屏验收仍未完成。


## 第十二轮：系统高对比主题基础通路

确认缺陷：ThemeSnapshot 虽有 high_contrast 字段，宿主此前始终保留默认 false；实际颜色只来自深浅主题和玻璃背景。平台层现在只读查询 SPI_GETHIGHCONTRAST 与 GetSysColor 的 COLOR_WINDOW/COLOR_WINDOWTEXT 配对，不修改系统设置。实现依据：[高对比参数](https://learn.microsoft.com/en-us/windows/win32/winauto/high-contrast-parameter)、[HIGHCONTRASTW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-highcontrastw)。

Theme 增加明确的高对比状态，以不透明系统背景和前景替换文字、边框、焦点与选择轮廓；选择不填色，以保持 Window/WindowText 配对，不猜测反色文字。普通 accent、玻璃样式和壁纸亮度不能覆盖该配色。WM_SYSCOLORCHANGE 纳入主题刷新；在高对比保持开启但系统前景/背景变化时仍会刷新，而不是仅比较 mode。

宿主快照同步高对比标记与系统文字色，状态角色同样使用可读前景，业务状态继续由文本表达。共享控件反馈在高对比时画不透明轮廓，避免把普通半透明反馈覆盖到文字上。标题绘制选择 Solid 背景，并忽略单栅栏透明度、tint、自定义标题文字色和标签颜色；高对比显示标题，不使用标题悬停隐去策略。文件正文和插件使用同一 Theme/ThemeSnapshot；字体、DPI 和文字比例约定不变。

验证：Windows render 47 项、主题宿主 5 项、窗口/宿主 44 项通过；Linux 平台无关测试 45 项通过。新增回归覆盖深/浅/自定义系统配对、主题 epoch 与模式恢复、轮廓反馈；实际 D2D 绘制在 96/144 DPI 下使用被独占借用的 GPU material，确保高对比路径不会访问玻璃采样，并带低透明度/自定义 tint 参数提交。

边界：尚未切换用户真实 Windows 高对比设置或做屏幕像素验收。原生菜单、所有按钮的悬停/焦点可见性、完整跨屏/文字比例矩阵仍需逐项验证完善；基础通路通过不代表完整高对比可访问性验收。UIA provider 仍未接入。

本轮 Windows all-targets 严格 Clippy、开发版构建和固定 Windows App SDK 运行时暂存通过。上述实机与 UIA 未完成项继续保留在整体目标中。


## 第十三轮：高对比控件状态与场景边框

进一步确认两处缺陷：文件选中悬停的 HOT_SELECTED_LIFT 会重新给透明 selection_fill 增加 alpha，改变系统背景；标题返回/折叠按钮的 hover/pressed 则因普通填色已透明而失去状态反馈。文件状态在高对比时改为仅描边，不再经过普通混色分支；标题、标签、返回和折叠按钮使用共同的内对齐轮廓。折叠按钮在高对比静止状态也保持显示。

平台无关 design::contrast_outline 统一控件轮廓的像素对齐、粗细和尺寸下限，原生 chrome 与插件悬停/按下/键盘焦点共用它。回归覆盖 100/125/150/200% 显示比例下描边外缘落在整数设备像素上，不增加输入缩放。

NodeStyle/Component 增加可选 stroke，场景准备时验证颜色和正有限宽度，公共 painter 在原节点内部画边框并保持原裁剪和文字矩形。日历在高对比时用持久边框标记选中日，当天仍保留字体强调；不为装饰新增语义或命中节点。回归使用独立文字比例，验证原有 32 个命中目标及选中节点的绘制、命中、语义矩形一致。

新增命令记录回归验证边框位于原矩形内部、文字使用原始矩形且裁剪平衡；非法颜色、零/负/非有限边框宽度被拒绝。Windows D2D 回归扩展到返回/折叠按下、文件 hover/selected/pressed/focused 状态，并继续验证高对比不访问 GPU 材质。

这些结果仍不能代替屏幕像素/真实输入验收；原生菜单、UIA provider 和真实跨屏矩阵继续属于整体目标的未完成项。

本轮验证：Linux 平台无关测试 48 项，Windows render 48 项、主题宿主 5 项、窗口/宿主 44 项通过；Linux 库及 Windows 应用 all-targets 严格 Clippy、Windows 开发版构建与运行时暂存通过。


## 第十四轮：菜单主题边界与原生交互验收

菜单使用 WinUI MenuFlyout，并未自行画深浅/高对比菜单颜色。依据 [Windows 对比主题说明](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/high-contrast-themes)，框架默认参与高对比调整。本轮显式约定菜单请求策略：检测到系统高对比时请求 WindowTheme::System，普通模式继续按保存的菜单深浅偏好请求 Dark/Light；每次打开时读取系统状态。标题可见性目标也纳入 high_contrast，与 renderer 的强制显示策略一致。

新增菜单策略优先级回归。实际隔离 WinUI 菜单测试最初在 UIA SetFocus 调用失败，另一次菜单提前关闭并返回 0，原因未确认；未据此认定是应用渲染或高对比缺陷。诊断确认菜单可获得前台、首项可聚焦且已持有焦点；测试脚本改为前台归属校验下的 Home/Enter 和 Esc，去掉冗余的 UIA SetFocus。没有降低前台进程检查或向其他应用发送键盘输入。

修改后的仓库脚本完整运行通过：深色勾选项、禁用项、浅色菜单、子菜单展开/执行、键盘选择、Esc 取消及重复打开；返回命令依次为 11、22、0，测试进程正常退出。通过 UIA 检查 ToggleState 和 IsEnabled；查看了本地深浅菜单截图，文字、勾选、禁用和分隔线可见。测试只操作隔离进程，临时运行时由脚本清理，未修改用户配置/系统高对比设置。

本轮菜单普通单元测试 2 项及显式执行的原生 live 测试 1 项通过；窗口/宿主回归 44 项通过。严格 Clippy、Windows 测试目标与开发版构建通过。菜单验证发生在当前普通系统配色下；真正开启/自定义 Windows 高对比后的 UIA/像素验收仍未完成，不能把菜单 UIA 通过外推为桌面 D2D 内容已有 UIA provider。


## 第十五轮：延迟提示的身份、几何与定时器边界（2026-10-18）

文件 infotip 原来保留索引；改为 ContentId + 完整 ItemId，门户向上提示也绑定 ContentId。现有 set_items 会主动隐藏提示，此轮不将该正常路径描述为必现串位；修复的是延迟消费仍依赖索引、缺少最终已提交几何校验的残余契约。文件标题取已发布 FileScene 的完整语义名称，当前业务列表只用于按 ID 查找附加文件属性，不再让相同索引的新项替代旧项。

显示前重新查询系统指针与所属 HWND，再用当前有效 ChromeScene/FileScene 命中；隐藏、退役、淡出、被其他窗口覆盖或失去指针的目标不显示提示。折叠/展开动画中的文件正文和滚动条热区不能成为文件提示目标。指针已移动到其他目标或离开停留盒时，重新建立等待，不复用旧等待时间。

等待以单个 TipWait（指针锚点 + deadline）表达，显示状态以自动隐藏截止时间表达，替代分离的 armed/shown bool。KillTimer 不能证明已排队的 WM_TIMER 消失：显示和自动隐藏两条路径都核对当前截止时间，旧消息不能提前展示或关闭新气泡。原生弹窗同步重入时，try_borrow_mut 失败保留待办并以 16 ms 重试，不先消耗唯一唤醒。

回归增加：FileScene 按完整身份查找语义，跨重排仍读取同一文件，索引复用和 UUID 高位不同不能误配；旧显示/隐藏定时器不能提前完成新截止时间。Windows 原生窗口/宿主测试 47 项全部通过；Windows 应用 all-targets 严格 Clippy 通过。测试日志位于忽略目录 .cache/tooltip-boundary-native-tests-20261018.txt。

这些回归验证稳定身份、截止时间和既有原生绘制/捕获边界，不等于实际 hover 消息序列、弹窗重入、鼠标驻留或截图验收。Windows 开发构建与固定运行时暂存继续使用仓库脚本验证；没有替换当前用户安装。UIA provider、文件/插件场景进一步收敛以及真实高对比/跨屏/文字比例矩阵仍未完成，整体目标保持进行中。


## 第十六轮：窗口/正文交互生命周期与拖放最终提交（2026-10-18）

确认原有边界不一致：插件有滚动/淡出/退役输入检查，文件命中与多数原生命令却各自判断；隐藏入口只隐藏提示，没有统一取消正在按下的文件、表头、滚动条或标签拖动。interaction::InputLifecycle 现在区分窗口可交互和正文可交互：隐藏、淡出、退役禁止窗口动作；折叠/展开期间停止正文输入，标题仍能反向切换。插件、文件/表头命中、文件键盘入口、滚轮、提示和 OLE 入口共用该判定。取消/失焦、尺寸与生命周期消息继续通过，避免输入门禁挡住释放和完成通知。

捕获效果从 plugin_panel 移入窗口交互模块，窗口持有 input_capture_release。隐藏时统一取消文件/表头/滚动条/插件输入和自定义标签/窗口拖动，折叠只取消正文手势；取消不会清除文件选择或业务内容。隐藏入口与窗口/宿主操作在 view 借用结束后执行 ReleaseCapture 和 DragCancel，且只释放仍由本 HWND 持有的捕获，不抢走另一窗口的新手势。

OLE 的 IDataObject 读取与右拖菜单可能运行嵌套消息。Drop 先保存命令候选和稳定目标身份，回调结束后重新核对窗口存活/可交互、当前内容、目标标签、文件夹完整 ID/路径及插入顺序。只有同一目标仍有效才把命令入队并返回原效果；目标失效或没有候选命令时返回 None，不能在没有动作时向来源报告 Move。插入索引绑定当时的完整 ItemId 顺序，不能把旧 slot 应用到重排后的列表。这里保证的是 UI 线程入队边界，不代表已完成磁盘事务或跨进程 Shell 操作。

验证：Windows 原生窗口/宿主测试 50 项通过（含窗口/正文生命周期矩阵、消息门禁、拖放最终入队/效果一致性及捕获所有权）；Windows all-targets 严格 Clippy、开发构建、固定 Windows App SDK Runtime 2.5.1 暂存、fmt 与 diff 检查通过。

新增 scripts/test-input-lifecycle.ps1 和独立夹具。脚本先验证配置，再从本地 NTFS 运行隔离实例，不操作用户配置、真实桌面文件或系统高对比设置。真实取得 HWND 捕获后分别隐藏标签拖动、折叠文件按下、隐藏文件按下；快照验证捕获与手势清除、隐藏输入拒绝、折叠时标题可用/正文不可用、恢复后正文可用，并核对持久化标签顺序未保存取消的拖动。报告 .cache/input-lifecycle-audit-20261018/report.json 为 passed=true，进程退出 0。日志显示 dpi=96；折叠检查点此时已完成高度变化，不将其外推为每一动画帧已验收。

夹具调试中修正了 UTF-8 BOM、工作区规则/Inbox 契约、稳定 HWND 匹配以及按活动布局检查持久化；这些启动/断言错误不计为产品回归通过证据。最终脚本完整执行通过。隔离 SPM endpoint 未启动的连接警告不属于本轮服务验收；日志级别 ERROR/PANIC 均未出现。临时运行时在退出后清理。

边界：此轮没有执行真实 Shell/OLE 数据对象重入或右拖文件操作，也没有物理键鼠、跨屏、视觉像素、高对比与完整文字比例矩阵验收。用户当前安装未替换。UIA provider、场景图像/材质抽象和上述实机验收仍在整体目标内，不能将这轮交互收敛视为整体完成。

## 第十七轮：已提交场景的只读 UIA 桥接（2026-10-18）

新增 platform::accessibility，将不可变 Snapshot / Node / Bounds 发布给原生 IRawElementProviderSimple、Fragment、FragmentRoot。COM 回调只读 Arc 快照，不借用 FenceViewState，也不等待 UI 消息循环；UI 线程适配器从当前有效的 ChromeScene、FileScene 和插件已提交帧构建数据。窗口消息/帧边界与成功绘制后发布，隐藏和正文折叠状态限制导出范围，设备/DPI/surface 不匹配的插件帧不导出。几何先按 DIP viewport 裁切，再按显示 DPI 转物理屏幕坐标，不重复应用文字比例。完整 ContentId / ItemId 与插件实例、激活和挂载代次组成节点身份。

COM ABI 显式保留 S_OK/null 的导航终点和不支持的 pattern，VARIANT / SAFEARRAY 使用生成的 Windows ABI 声明。修复删除节点或关闭窗口后仍可能成功返回部分接口结果的缺口：查询按当前快照检查身份，失败前初始化输出；非法快照拒绝发布并清空旧几何，包含重复 ID、丢失父节点、环和非有限/溢出边界。WM_DESTROY 通知 UIA 释放本 HWND 的 provider 映射；关闭后的 publisher 不再发布。

Windows 原生测试：platform accessibility 5 项、fence_window 52 项通过。覆盖实际 COM 接口的中文 BSTR、SAFEARRAY 完整 ID、支持接口范围、S_OK/null、删除/关闭后的持有引用、无效快照，以及通过 AgileReference 从 STA 转到 MTA 后读取并观察发布失效。应用适配器覆盖负屏幕坐标、100/125/150/200% 显示比例与 UUID/代次身份。测试输出位于 .cache/uia-bridge-audit-20261018/。Windows platform + app all-targets 严格 Clippy 通过。

这是只读语义桥的增量，尚未证明外部 UIA 客户端能稳定遍历完整窗口树；未实现 Invoke/Selection/Value 等操作模式、真实焦点和结构/属性变化事件，也未完成屏幕阅读器验收。跨 apartment 测试不能替代跨进程客户端；比例计算单元测试不能替代真实跨屏。标题/文件/插件的交互、辅助技术和呈现继续需要按整体目标收敛，不能将本轮视为架构目标完成。

本轮 Windows 开发构建及固定运行时暂存、fmt/diff 检查通过；接入 UIA 后重跑隔离原生输入生命周期脚本，.cache/uia-bridge-lifecycle-20261018/report.json 为 passed=true、进程退出 0，隐藏/折叠/恢复与捕获取消未出现回归。没有替换当前用户安装。

## 第十八轮：外部 Windows UIA 客户端与折叠生命周期（2026-10-18）

新增 scripts/test-accessibility.ps1，复用自有配置夹具和固定运行时，从本地 NTFS 启动独立实例，Windows PowerShell 的 UIAutomationClient 在另一个进程中按 PID 枚举 HWND，经 AutomationElement.FromHandle 接入真实 WM_GETOBJECT/provider 路径。只读取测试进程自己的窗口，保留窗口清单、完整 UIA 树、日志和报告；失败同样保存诊断，退出后清理临时实例与运行时。未使用全局键鼠输入，未修改用户安装、用户工作区或系统显示设置。

原生脚本增加仅 debug 且 PECOFENCE_UI_TEST_WINDOWS 开启时生效的 wait-file 屏障，每次以 timer 返回消息循环，等待外部客户端完成查询后再折叠/展开。这样不依赖固定长 sleep 猜测客户端耗时；实例还有 --exit-after 上限，脚本 finally 只清理自己的进程。

最终报告 .cache/uia-client-lifecycle-final-20261018/report.json：passed=true、退出 0，读取 12 个可见文件节点（包含 Fixture-00-中文.txt）。确认自定义 FrameworkId、完整运行时 ID、非空且裁切在根窗口内的物理屏幕区域、文件→列表→根的父链、重复查询身份稳定；折叠后新查询不再找到正文文件，持有文件引用的显式 GetCurrentPropertyValue 抛出 ElementNotAvailableException；展开后同一文件恢复原运行时身份和非空几何。树记录 tree.json，失效结果 held-reference-result.json。日志显示显示 DPI 为 96。

最初 readiness 只读取 stdout，漏掉 stderr 的 tracing 标记，已改为读取两路日志。首次折叠失效断言采用 PowerShell 链式 .Current.Name，未可靠暴露查询异常；改用显式 GetCurrentPropertyValue 并保存异常类型，最终记录为 ElementNotAvailableException。前述夹具/客户端断言失败不作为产品缺陷或验收通过证据。

Windows 开发构建及固定运行时暂存、应用 all-targets 严格 Clippy、fmt/diff 检查通过。第十七轮 57 项原生回归的源码主体未改动，本轮新验证的是实际外部客户端和新增测试屏障，不把旧测试结果写成重新执行。

剩余边界：此次只验证文件门户和标题的只读树，没有证明插件控件的外部 UIA 全路径、Invoke/Selection/Value、焦点、结构/属性通知或屏幕阅读器体验。没有真实多显示器、其他 DPI/文字比例、高对比、物理键鼠、Shell/OLE 或像素验收。文件/插件图像与材质场景收敛和上述能力仍在整体目标内。

## 第十九轮：待办业务身份与场景身份对齐（2026-10-18）

检查插件外部语义前确认了具体缺陷：WidgetFrame 默认以节点位置分配 ID，TodoConfig 的任务也只有 title/done。清除前面的任务会让后续任务占用之前的节点 ID；即使布局版本能拒绝旧动作，UIA 持有引用仍可能被重新解释为另一任务。不能用布局版本全面替换运行时 ID 来掩盖，因为同一存活任务在布局变化后应保留身份。

TodoConfig v1 增加 nextId、TodoTask 增加 id，保留原有任务顺序、标题和完成状态。旧配置缺少字段时按顺序分配唯一身份；分配结果随下一次正常业务持久化写回，不在读取时改用户文件。拒绝重复和超范围 ID，分配器耗尽明确报错。清除/编辑保留存活任务 ID；撤销只恢复任务，不能回退分配器，避免以后新任务复用已经删除的身份。新增字段是 v1 JSON 的加法扩展，没有修改原有 action 编号或现有业务命令接口。

待办行、勾选框和编辑标题使用任务 ID 派生、互不相同的 u64 场景 ID；高位与固定控件命名空间分离，行成为两个控件的语义父节点。页脚操作、页码和空状态也使用固定身份，不受行数与可选按钮出现顺序影响。绘制、命中和语义仍来自同一个 Scene；索引式 action 的变化继续由既有布局版本保护，不将稳定场景 ID 当作业务动作提交凭证。

回归：Linux 插件 14 项、Windows 原生插件 14 项和窗口/宿主 52 项通过。新测试覆盖旧 JSON 迁移与保存重读、同名任务、重复/溢出拒绝、清除/撤销/模拟重启后的 ID 不复用，以及清除/分页/页脚变化下任务控件身份和父子关系稳定。Linux 插件及 Windows app all-targets 严格 Clippy、Windows 开发构建/固定运行时暂存、fmt/diff 检查通过。

外部 Windows UIA 脚本扩展为自有便笺和两条待办任务。先读中文便笺正文、任务标题和行父节点，再通过原有调试业务命令清除第一条已完成任务：第二条任务的运行时 ID 不变，原持有引用仍读取原任务名称，已清除任务不在新树中；撤销后两条任务和存活身份恢复。文件门户折叠失效和展开恢复同时继续通过。报告 .cache/uia-plugin-identity-20261018/report.json 为 passed=true，进程退出 0；plugins.json 记录插件身份和名称。日志为 96 DPI。此轮没有通过 UIA 发起动作，不能据此声称 Invoke/Toggle/Selection 模式已实现。

剩余：其他插件的动态节点身份需要继续审查，UIA 操作模式/焦点/事件尚未完成；文字比例/跨屏、高对比、真实输入和像素验收、图像/材质场景抽象仍在整体目标内。没有替换用户当前 Windows 安装或修改用户待办数据。

## 第二十轮：本地组件显式身份与日历日期语义（2026-10-18）

确认另外两处身份复用：日历不同月份的同号日期原来共享节点 ID，改变周起始日时星期身份随列位置移动；便笺撤销按钮出现/消失会改变页脚提示 ID，甚至让旧提示位置成为撤销控件。将 WidgetElement 的 id/parent 改为必须显式提供，删除 WidgetFrame 按 nodes.len 自动分配以及事后改身份的 setter，待办也迁到同一声明式入口。Scene 的重复/悬空身份验证保持不变。

日期 ID 保存完整有符号公历天数位，命名空间与固定控件分离；同一日期在文字比例、周起始日和布局变化下不变，不同月份的同号日期不同。星期 ID 绑定实际星期，导航控件固定身份。日期可见文字仍为日号，语义名称改为完整 YYYY-MM-DD。便笺正文、撤销和页脚使用固定身份；正文文本归属具名的“编辑便笺 / Edit note”按钮。时钟在紧凑/展开、日期显示与否的布局之间显式保留时间/日期身份。UIA 对 heading 映射 Text、columnheader 映射 HeaderItem，常量与本机 Windows SDK UIAutomationClient.h 核对。

新增回归证明跨月后 ControlInteraction 不会把焦点留在下个月同号日期上；星期重排及文字缩放保持同一日期/星期身份；时钟可选日期布局和便笺可选撤销页脚身份稳定，正文父子关系正确。Linux 插件 17 项通过；Windows 原生插件 17、UIA/COM 6、窗口/宿主 52，共 75 项通过。日志位于 .cache/widget-identity-20261018/。Linux 插件及 Windows 应用 all-targets 严格 Clippy、Windows 开发构建/固定运行时暂存、fmt/diff 检查通过。

外部 UIA 脚本增加便笺正文的具名按钮父节点与撤销后的持有引用检查：正文文字改变仍是同一个节点；撤销按钮消失后，页脚提示的持有引用、名称和运行时 ID 保持正确。待办清除/撤销和文件折叠/展开继续通过。.cache/uia-widget-identity-20261018/report.json 为 passed=true、退出 0。该脚本实际操作自身测试实例的业务命令，UIA 仍只查询，没有实现或验收 UIA Invoke/Toggle。

边界：日历/时钟变化目前由平台无关与 Windows 原生插件测试证明，尚无外部客户端跨月、真实键鼠或像素验收；SPM 动态来源的身份还需继续审查。UIA 操作、焦点与事件、图像/材质场景抽象、真实 DPI/文字比例/高对比/跨屏矩阵仍未完成，整体目标保持进行中。当前用户安装与配置未替换。

## 第二十一轮：SPM 记录身份、选择与来源边界（2026-10-18）

确认 SPM 的动态列表仍以 100 + index 作为节点 ID，selected_item 也保存索引。新快照若仅重排事项，语义引用和详情可能被重新解释为另一条业务记录。将选择改为完整 RecordId，在准备布局时解析当前索引；接受新快照时，已删除事项清除选择，不跳到占用旧索引的新事项。

PanelView 增加动态记录绑定，明确完整 RecordId 与 Row/Detail 部件。布局局部行 ID 使用单独范围，与固定按钮/详情 ID 分开。SpmPanel 的 SceneIdentities 在准备 Scene 前把完整业务键映射为宿主实例内单调分配的 u64 ID：同一记录在重排、Workspace/Compact 和宽窄详情间保留对应部件身份；不同模式固定控件隔离，pane 根保持一致。使用完整键的 HashMap 比较，不把哈希值或截断字符串作为节点 ID。移除来源中已不存在的记录映射，保留当前快照中暂时不可见的记录；分配器不复用 ID、耗尽时报错。场景绘制/命中/语义继续从同一份准备结果派生。

重复 preview RecordId 在 SnapshotCursor 接受版本前拒绝，因此非法快照不能污染现有快照或消耗合法的新修订号。原 action 编号和 daemon 协议不变；prepared_source_revision / layout_revision 的旧动作门禁继续有效，稳定节点身份不能绕过来源版本检查。

验证：Linux SPM 15 项、Windows 原生 SPM 15 项及窗口/宿主 52 项通过。新增回归以真实 PanelInstance event/prepare_frame 和模拟服务能力验证两条记录重排、详情保持、跨模式/断点身份稳定、存活焦点保留、删除清除选择、映射回收且重新出现不复用旧 ID、重复来源拒绝及相同修订号的合法快照仍能接受。旧来源动作在新帧准备前也不能入 IPC。测试记录 .cache/spm-identity-20261018/。Linux SPM 及 Windows 应用 all-targets 严格 Clippy 通过。

边界：没有连接或写入用户 SPM daemon、数据源或工作区。模拟服务与 PreparedFrame 测试不能外推为真实 SPM 外部 UIA、导航、业务提交或屏幕像素验收。UIA 操作模式/焦点/事件、图像/材质场景收敛和实际 DPI/文字比例/跨屏/高对比矩阵继续在整体目标内；没有替换用户 Windows 安装。

本轮 Windows 开发构建、固定 Windows App SDK Runtime 2.5.1 暂存及 fmt/diff 检查通过。

## 第二十二轮：插件按钮的异步 UIA Invoke（2026-10-18）

UIA 首次接通插件按钮动作。依据 [Microsoft IInvokeProvider.Invoke 说明](https://learn.microsoft.com/dotnet/api/system.windows.automation.provider.iinvokeprovider.invoke)，调用需异步立即返回；桥接线程读取当前不可变节点，把节点身份与 opaque 动作 token 放入上限 64 的队列，只 PostMessage 唤醒窗口。通知失败时在同一队列锁内回滚本次请求，不留下“调用失败但以后执行”的动作。控件禁用、隐藏、已删除、无动作或队列满明确返回相应失败。

UI 线程收到通知后再次从当前有效的已提交场景确认完整节点身份、动作、布局修订和设备代次，并检查窗口/正文可交互与可见几何。挂载/内容身份包含在 NodeId；颜色主题或仅 frame_revision 改变不单独取消请求，与既有指针路径的代次策略对齐。view 被重入借用时保留队列，以独立 16 ms timer 重试，COM 回调不会等待或直接借用 UI 状态。

键盘、指针和 UIA 现在共用 PluginPanelContent::invoke_committed，统一检查活动/暴露状态、已提交布局修订及动作是否仍存在，再经原 PanelHandle.event 和 apply_plugin_update 处理。只有确有对应命中动作的插件 button 才导出 Invoke；没有动作的按钮不启用，checkbox/text/文件/chrome 不伪装成 Invoke。节点身份仍独立于其动作 token，因此同一存活按钮可以在后续布局中保持引用。

核对本机 Windows SDK UIAutomationCoreApi.h 时修正旧常量：UIA_E_NOTSUPPORTED 应为 0x80040204，UIA_E_ELEMENTNOTENABLED 为 0x80040200，二者原来混用。新回归明确核对这两个返回值。

验证：Windows 原生 UIA/COM 9 项、窗口/宿主 54 项，共 63 项通过；包括模式声明、队列保留/通知回滚/上限、隐藏/禁用/删除拒绝、执行前身份与代次检查，以及纯主题更新不取消。记录 .cache/uia-invoke-20261018/。Windows platform/app all-targets 严格 Clippy、开发构建/固定运行时暂存、fmt/diff 检查通过。

外部客户端在隔离实例中通过真实 InvokePattern 调用便笺撤销，等待正文实际改变，再调用按钮消失前持有的旧 pattern，确认 ElementNotAvailableException。原文件/待办/页脚检查一并通过。.cache/uia-invoke-client-20261018/report.json：passed=true、externalInvoke=true、staleInvokeRejected=true、退出 0。该测试只改自有夹具内容，没有向用户实例发动作；未替换安装。

边界：Invoke 的 InvokedEvent 及结构/属性事件通知仍未接入，不能把这轮动作链路描述为完整 Invoke 无障碍契约。焦点、Toggle/Selection/Value、原生编辑器/文件/chrome 操作及屏幕阅读器仍未完成；重入 timer 的实现不能替代实际模态编辑器嵌套消息验收。跨屏/DPI/文字比例/高对比与图像/材质场景工作继续在整体目标内。

## 第二十三轮：Invoke 事件与稳定 COM 根身份（2026-10-18）

插件按钮经指针、键盘或 UIA 的统一入口接受动作后，保存源节点及祖先身份，随后应用更新。通知先入 outbox，在消息边界释放 UI 状态借用及快照锁后调用 UiaRaiseAutomationEvent；失败或过期动作不产生事件。源按钮可能因撤销动作立即消失，因此独立事件 provider 保留该次动作的运行时身份和祖先链，数据与动作查询仍遵循实时可用性检查。

窗口根 provider 通过 Weak 引用保留唯一 COM 身份，FragmentRoot 返回原根，避免每次查询创建不同根对象，也不形成强引用环。核对 Windows SDK 后修正 ProviderOptions：ServerSideProvider | UseComThreading 是 0x22，原 0x21 错用了 ClientSideProvider。回归明确核对标志及根对象 IUnknown 身份。外部订阅在独立 MTA 线程注册和移除，回调使用 C# 队列记录，避免 PowerShell 回调线程限制。

验证：外部 Windows UIA 隔离夹具通过真实 InvokePattern 撤销便笺，收到恰好一次、源运行时 ID 匹配的 InvokedEvent；按钮消失后旧 pattern 被拒绝，未产生额外事件。文件树、中文文本、折叠/展开、待办清除/撤销及页脚身份检查同时通过。.cache/uia-event-server-20261018/report.json 为 passed=true、invokedEvents=1、externalInvoke=true、staleInvokeRejected=true。此前发送接口返回成功但未收到事件的诊断未算作通过。

本轮提交前 Linux 插件 API/组件/SPM 共 51 项，Windows 原生 UIA/COM、渲染、窗口共 113 项通过；严格 Clippy 与构建检查通过。事件成功不代表结构/属性/焦点、Toggle/Selection/Value 或屏幕阅读器契约已经完整；实际 DPI/文字比例/高对比/跨屏、图像/材质场景及真实输入验收继续保留为整体目标。发行时从提交后的干净源码构建，安装校验及配置备份记录留在本机，不上传用户工作区。

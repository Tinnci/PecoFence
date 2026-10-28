# 内容绘制、查询刷新与小组件扩展评估

日期：2026-09-28。当前实现核对与增量设计；Windows 桌面/GPU 行为尚未在本轮实测。

## 当前实现

### 文件 Fence

`app/sync.rs` 将文件系统事件合批、保留改名归属，随后枚举桌面并调用 `State::sync_desktop`；若数据变化，`refresh_all` 重新构造相关视图。Folder Portal 已按受影响目录定向刷新，`refresh_portal` 在项目集合/内容不变时不要求重绘。`FenceViewState::replace_items` 按 ID 继承图标和标签缓存。滚动时内容 surface 按需重绘，绘制阶段剔除离屏项目，但准备阶段仍全量扫描并构建项目数据。

这适合目前“文件图标、列表、详情”三类视图，但目录越大，更新与滚动时 UI 线程处理量越大。继续增加筛选、搜索、卡片、实时状态时，不应让每种内容都从 `App::refresh_all` 和 `FenceViewState::redraw_content` 分支扩张。

### SPM Panel

`PanelSpec` 已持久化 provider ID、实例 ID、配置版本和 provider 自有 JSON。`PanelProvider/PanelInstance`、`FrameInput/PreparedFrame`、Canvas、命中树和语义树定义了较好的扩展边界。`spm_transport` 按 `ProjectQuery` 复用订阅，保存最新快照并把同一 `Arc<[u8]>` 扇出给本地实例；`SnapshotCursor` 拒绝旧 session/revision。`spmd` v2 live feed 用定时/手动触发采集，只在内容变化后提交并发布 revision。

目前落地边界仍偏 SPM 专用：宿主只注册 `SpmPlugin`，`PipeIpc` 只接受 `spm.v2/read-model`，设置和右键菜单尚无从 provider descriptor 自动发现、创建和编辑小组件的流程；真实文字度量和 `HostDesktop::request_mode` 尚未实现为宿主行为，Storage 只保存在内存。`HostRender::invalidate`、`PanelUpdate` 命令/relayout、实例定向刷新和点击事件调度已在本轮接通。`PreparedFrame::semantics` 已定义，但当前宿主只消费 `hit_tree`，尚未连成 UIA 语义提供者。

`PipeState` 已按订阅合并最新快照，操作回执单独排队并施加背压。渲染失败时当前 surface 绘制先清空再执行 `paint`，`paint` 返回错误后不提交新 hit tree，但部分像素可能已更新，需验证并定义恢复策略。

### 扩展适配度

| 新内容 | 现有基础 | 主要缺口 |
| --- | --- | --- |
| 时钟、天气、交付状态卡片等低频信息 | `PanelProvider` + Canvas + 快照/主题、实例定向重绘 | 通用数据源和定时失效；真实文字度量 |
| 可滚动列表、搜索、筛选、任务卡片 | ViewNode/HitNode/语义节点 | 通用滚动/键盘/焦点/UIA；可见区虚拟化；局部布局失效 |
| 图表、动态看板 | Canvas 路径/文本/图像 | 增量数据、资源注册与生命周期、缓存、绘制区域预算 |
| 第三方插件 | provider trait 与能力接口 | 动态发现/加载、隔离、能力授权、版本迁移、失败恢复；当前仅第一方静态注册 |

结论：作为**第一方低频小组件的原型底座**已基本可用；作为对外开放的通用小组件 SDK 尚不充分。文件 Fence 应保留专门的桌面/Shell 模型，小组件复用窗口与渲染宿主，不必把文件模型强行泛化成任意内容模型。

## 建议的实现边界

```text
数据源/daemon（采集、缓存、重试、领域语义）
  → QueryBroker（规范化查询、复用订阅、最新快照、限额、完成事件）
  → WidgetInstance（纯状态更新、ViewModel、动作）
  → LayoutSnapshot（同一份几何生成 paint/hit/UIA）
  → RenderScheduler（按 mount + 原因 + 脏区合并）
  → Direct2D surface / Composition visual
```

1. **先完成宿主事件闭环。** `PanelManager::poll_events` 返回受影响的 `InstanceKey/MountKey` 及 `PanelUpdate`，宿主用实例到 HWND 的映射仅重绘可见内容；统一执行 `Invalidate/SetTitle/RequestMode/relayout`。按钮、主题、IPC 都走同一调度。隐藏标签记录 dirty revision，激活时只绘制最新一次。
2. **把快照与操作结果分流。** 每个规范化 query 只保留最新快照及 revision；同帧多次变化合并一次 UI 唤醒。请求完成/错误按 token 可靠、限量投递，超额时显式报错而非静默丢弃。保留 generation 校验，卸载后丢弃迟到结果。SPM 的采集/重试仍归 daemon，宿主不复制业务调度。
3. **明确刷新语义。** 组件订阅可收到 `Loading → Ready/Empty/Stale/Error` 和 `data_revision`；`Refresh` 只是发起采集，界面需在完成/新 revision 后结束加载状态。按项目/数据源合并同一时间的手动请求，并给定超时、重试与离线状态。freshness 倒计时用下一次文字变化的单次 timer，不复制全量业务快照。
4. **建立可复用的可见区布局。** `prepare_frame` 用不可变数据 revision + viewport + presentation + theme/text/device epoch 缓存视图；绘制、点击和 UIA 均引用同一个 `LayoutSnapshot`。长列表只实例化可见项并预取少量邻近项，离屏语义通过虚拟化接口提供。局部变更先用 mount 级整面重绘确保正确，性能证据成立后再启用脏矩形。
5. **补齐 Widget SDK 的最小能力。** 第一阶段只支持内置、声明式 manifest：尺寸/展示模式、数据 query、刷新策略、配置 schema、动作、资源预算及设置/右键菜单入口。宿主提供真实文字度量、图像资源注册、焦点与键盘、UIA、持久配置和错误占位。外部插件加载与安全隔离单独设计，不与首个小组件一起引入。

微软建议把长耗时工作移出 UI 线程、延迟创建暂不可见内容；虚拟化列表也以可见范围为核心。[保持 UI 线程响应](https://learn.microsoft.com/en-us/windows/apps/develop/performance/keep-ui-thread-responsive)、[ItemsRepeater](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/items-repeater)。Tokio `watch` 只保留最新值，适合查询状态通知，不适合可靠的逐项操作完成事件。[Tokio watch](https://docs.rs/tokio/latest/tokio/sync/watch/index.html)

## 最小验证闭环

先新增一个内置“状态卡片” fixture provider：两个实例订阅同一 query，更新一次只采集/解码一次；只重绘两个相关可见 mount；隐藏 mount 不绘制且恢复后显示最新 revision；关闭一个实例不撤销另一个的订阅；刷新失败、主题/DPI 变化、快速点击时 paint/hit/UIA 一致。随后用 1000+ 行列表验证虚拟化，再考虑更多小组件和开放 SDK。

## IPC 与宿主刷新：本轮实施及下一步

2026-09-28 已实施第一段宿主闭环：`PanelManager` 收集每个实例的 `PanelUpdate` 并合并，`App` 只把结果交给当前显示该实例的窗口；点击与 IPC 更新使用同一个结果处理函数。`RenderService::invalidate` 从空实现改为 mount 级队列，校验当前 mount generation 后唤醒 UI；隐藏窗口的内容更新标为 dirty，显示时补绘最新帧。`SetTitle`、`RequestMode` 和 `relayout` 也在窗口侧执行。当前仍是 mount 整面绘制，尚未消费 `RectDip` 做局部重绘。

**不需要重写为微服务。** 现有边界是 `spmd` 管采集、缓存和业务查询，PecoFence 管窗口、输入和渲染；双方通过 Windows 命名管道上的 v2 envelope 通信。下一轮优先改协议消费和背压语义，而不是更换管道或为每个小组件启动进程：

1. 已修复：快照按 `(scope, generation, subscription)` 保存最新一份，同一会话比较 revision 并拒绝晚到的旧版；查询缓存也拒绝旧版。同一 UI 消费批次只派发最终值；订阅建立后仍从 query 缓存补发最新快照。
2. 已修复：操作回执进入独立的 128 槽可靠队列；队列满时异步等待 UI 消费，保持传输背压，不再淘汰回执。`spm_transport` 的 256 条 pending 请求满时向发起实例返回 `Backpressure`；超时、断线和离线已接受请求返回对应错误。SPM 面板显示操作错误。主窗口在模态循环中借用 App 时，有专用 timer 重新投递插件事件；窗口 view 暂时被借用时，App 保留并合并 `PanelUpdate` 后重试。
3. 把查询状态和操作状态分开：`watch/latest` 适合状态快照，有限队列适合不可合并的操作结果。UI 只订阅所需 query，数据源采集/重试仍在 daemon；新增天气或任务源时先实现 host 内的 source adapter，只有隔离、权限或跨设备需求经测量成立时才添加进程边界。
4. 待补：记录端到端时延、队列深度、合并次数、过期丢弃数及绘制耗时；在 Windows 实机上验证高频快照、模态菜单、断线重连和关窗竞争，再调整队列容量和脏矩形策略。`RectDip` 局部绘制仍未启用。

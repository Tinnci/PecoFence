# 路线图：应用边界与接口重设计

设计入口：[ARCHITECTURE.md](ARCHITECTURE.md) · 决策：[ADR-006](decisions/ADR-006-application-boundaries.md)。

维护者已明确重新设计且不要求后向兼容。此前“不做重构性转向”、兼容读取/双向解码和保留服务内核的约束不再适用。新格式不兼容不等于有权覆盖旧用户文件。

## 基线与完成定义

重设计开始于 `43b6b7d`。此前阶段提供了原生窗口、文件功能、静态 SPM 面板、scope/任务工具和 CI/发布基线；这些是可以复用的实现，不证明新边界已经成立。

历史阶段记录见 Git 历史及 [旧实施规划](NEXT_PHASE_ROADMAP_AND_PLANNING.md)、[P0–P2a 蓝图](SOL_ENGINEERING_EXECUTION_PLAN.md)。此前自动化/实机结果只能证明当时受测组合，不能替代本次生命周期、保存或接口验收。

**每阶段完成条件：**输入 → 校验 → 有界副作用 → 真实结果 → 状态提交 → 呈现全链路可证明；对应旧入口被删除；故障测试与独立核验完成。只添加 trait、只移动文件或只编译通过不算完成。

## A：异步门户观测（第一实施切片）

状态：代码已落入工作区并通过自动化核验；Windows GUI/watcher 人工验收未执行，不宣称完整阶段验收结束。

- 应用协调器拥有门户来源生命周期、独立请求 ID 和合并重读；持续同路径通知不使当前完整观测失效。
- 背景 reader 拥有文件/Shell 查询与自己的 COM apartment。
- 完整结果原子替换；完整空目录确实清空；partial/unavailable 保留同路径最近完整视图并标记健康。
- 导航、删除/重建、配置/布局替换撤销旧结果；旧路径条目不可作为新路径内容操作。
- UI 只消费结果和投影，不枚举、不等待 worker；关闭投递早于 HWND 销毁。
- 空状态与标题区分加载/不可用/真正空目录，覆盖全部十种 UI 语言。内容推导的自动高度与其动画终态不再保存文档。

自动化：空目录、open/遍历/metadata 失败、迟到/重复结果、刷新突发/持续变动、同路径多个门户、停止；Windows 临时目录/Shell reader 测试；工作区测试、Clippy、绑定一致性、release 构建与 portable 打包。人工：真实 watcher/F5/导航、不可读来源提示、tab/导入/布局切换后不串内容。

实现入口：[读取契约](../crates/core/src/portal.rs)、[应用协调器](../crates/app/src/portal_runtime.rs)、[STA 读取适配器](../crates/platform/src/portal_reader.rs)、[状态提交](../crates/app/src/state.rs)、[原生投影接线](../crates/app/src/app/portals.rs)。旧同步门户读取入口已删除；整体重构状态见下面各阶段。

## B：文档与提交状态

状态：B1 规范化模型、载入保护和提交结果已落地；B2 完整持久/观测分离与异步修订提交仍待实现。

已实现：

- schema-2 `Config.layouts` 使用独立 `ContainerId`、`ContentId` 和单一容器/内容图；旧持久 Fence、tab_host 图、别名与自动迁移已删除。
- `Workspace` 组件校验创建、标签选择/重排、合并、拆出、删除与结构逆操作；只读 `FenceSnapshot` 适配原生呈现，不成为另一份可变状态。
- 窗口、几何、外观和自动高度按容器寻址；文件、门户、规则与扩展实例按内容寻址。拆出/挂载不重新创建来源身份。
- `workspace.v2.json` 使用精确格式；错误载入不开启普通工作区、自动保存或 Explorer 图标隐藏。恢复候选只读，明确接受/导入/新建后才能替换。
- 明确替换保留唯一原始字节 archive；主文件提交 receipt 与可选备份退化分开。重置/导入/恢复统一重新应用原生设置。
- 删除/合并后的迟到窗口事件通过可空身份查询拒绝，不再在 `expect("live container")` 中崩溃。

实现：[模型与校验](../crates/core/src/model.rs)、[工作区协调器](../crates/core/src/workspace.rs)、[存储](../crates/core/src/config_store.rs)、[应用状态](../crates/app/src/state.rs)、[组件验收](../crates/core/tests/workspace_model.rs)。

尚未实现：把桌面 metadata 从 `Config.items` 全部移出、文档修订与异步串行提交、合作写入者锁/外部修改 stamp、关闭时重试/明确放弃流程。现有保存仍同步且使用 dirty 标记，不能用 B1 结果宣称 r8/r9 并发提交保证。

## C：类型化 Settings 用例

状态：容器/内容配对、属性校验、集合规则目标、只读恢复 UI 和真实提交 toast 已实现；完整版本化/修订协议待实现，依赖 B2。

- 精确版本、workspace activation/client/request sequence、expected document revision、细粒度 change；旧会话和重复/已淘汰请求不重复执行。
- 删除整 Settings/Rules 替换和 raw JSON Value switch。
- Accepted 与 Committed 分开；草稿不被服务器推送覆盖。
- 规则编辑支持领域已有的多条件；OS 设置通过真实操作终态反馈。

验收：错版本/无效数值/冲突/草稿保留/保存失败/OS 注册失败，以及 WebView 实际流程。

## D：静态 provider 与类型化 SPM 端口

状态：待实现；涉及 SPM wire 的改变需要其 worktree 与配对验证。

- 区分组件契约、领域端口和宿主内部生命周期。
- 删除通用服务定位器/DAG、字符串能力列表、通用字节 IPC、共享 Token 和无效服务。
- watch/operation 分离，一对一完成关联；接纳后有明确本地终态。
- owner 关闭后不回调；每个操作有 deadline，已发送后断连/取消/超时但结果不可证明时为 Indeterminate，不自动重放。
- 复制简报/导航由 daemon 结果到真实 clipboard/navigation 完成；显示业务、连接、freshness 三类状态。

验收：双实例、多个订阅、重复/晚到完成、取消/断连、队列饱和、共享订阅关闭、错 daemon session、无能力与创建回滚。headless 测试不代替真实 daemon 配对。

## E：帧与原生恢复

状态：待实现，可在应用/扩展接口稳定后并行拆分用例。

- paint/hit/semantics/action map 在成功原生提交后发布为同一逻辑帧，失败保留旧帧或禁用输入；拒绝旧 frame/mount。
- 真实字体测量，主题/metrics/geometry/device 失效分开。
- render 设备恢复有 retry/backoff；领域实例不随设备重建。
- Explorer 图标恢复失败保留 ownership marker；watchdog 必须确认成功。
- 壁纸 I/O/CPU 处理离开 UI，按代际提交；日志后置单实例与有上限轮转。

验收：多 DPI、文字缩放/高对比、旧 action、Explorer 重启/Win+D/Peek、设备故障恢复。现有 [P2C 人工清单](P2C-CHECKLIST.md) 是证据入口，空清单不等于通过。

## 不作为本次交付

动态 ABI、插件市场、热卸载 DLL、沙箱、共享内存总线、事件溯源、通用微内核和盲目拆 crate。不重写 SPM 业务判断、不以本地一次终态承诺远端副作用恰好一次，不为体积牺牲诊断和正确性。

## 每次交付需附

具体替换链路与已删除入口；测试命令和实际结果；未验证的 Windows/daemon 行为；来源/配置文件处理政策；若涉及协议，则列双方 SHA 和支持的精确组合。提交/发布与架构验收是不同动作。

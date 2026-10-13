# PecoFence / SPM 热路径复杂度与渲染优化核对

日期：2026-09-28。依据当前源码静态分析；WSL 未运行 Windows GPU、Explorer 桌面或 ETW 采样。以下是量级推导，均非实测耗时。

## 口径

- `F`：宿主 Fence 窗口数；`N`：一个文件 Fence 的项目数；`V`：视口内项目数；`G`：连续日期分组数。
- `K`：一次 SPM 画面的节点数；`B`：一次 SPM JSON 快照字节数；`A`：内容 surface 的像素面积。
- 字符串比较/复制还受名称长度影响，下表把它记为 `L`。`O(A)` 仅表示全 surface 清空/更新的像素量级，实际 GPU 时间需实测。

## 已实现的路径与代价

| 场景 | 当前路径 | 时间复杂度 | 临时/常驻空间与备注 |
| --- | --- | --- | --- |
| 文件目录刷新、排序 | `State::items_of` 收集、排序、namespace 再排序，`App::item_views` 构造视图 | `O(N log N × L)`；`Type` 比较器反复生成小写名称/扩展名 | `O(N)` 临时视图；名称/path/icon key 另有复制 |
| Fence 项目替换 | `FenceViewState::replace_items` 按 ID 合并旧图标/标签、重映射选择、构建动画 | 哈希表预期 `O(N)`，布局位置计算另计 | `O(N)` 临时；保留已有图标/标签是有效缓存 |
| 文件 Fence 每次内容重绘 | 重建分组布局、扫描全部项目确保图标/标签、采样全部 alpha、生成全部 draw key 与 cell/row，然后绘制时按 Y 剔除 | CPU `O(N + N×G)` 上界；常见未分组/少分组近似 `O(N)`；实际绘制 `O(V)` 次项目绘制外加 surface 更新 | 每次 `O(N)` 临时向量/字符串；surface 约随 `A` 增长 |
| 首次显示大目录 | `ensure_icons_and_labels` 为所有项目排图标提取、拟合全部标签；图标由 2 个 worker 异步提取 | UI 提交 `O(N)`，实际 shell 工作约 `O(U)`（`U` 为不同图标 key 数） | 图标请求 channel 无界；CPU 图像和 GPU bitmap 缓存未见容量/LRU 上限，峰值需计量 |
| 分组布局访问 | `group_spans` 扫描 `N`；`Bands::band_index` 线性找组，每个 cell 调用一次 | `O(N + N×G)`；按日期排序时 `G` 通常较小，但其他排序可出现交错分组 | `O(G)` 分组；不是所有场景都可称为平方级 |
| 门户刷新 | 文件夹枚举，加上 `evict_portal_members` 对每个旧 ID 扫描其他门户的 `Vec::contains` | 枚举代价另计；共享检查最坏 `O(R×Q)`，`R` 为旧项目数、`Q` 为其他门户引用总数 | 旧/新项目及名称缓存 `O(N)`；多个重叠大门户时可能成为热点 |
| 宿主窗口同步 | `sync_fence_windows` 用 `Vec::contains` 比对 Fence ID；随后逐宿主重建活动标签视图 | 窗口集合检查 `O(F²)`；项目处理为各活动 Fence 的排序与构建之和 | `O(F + ΣN)`；`F` 通常小，优先级低于内容重绘 |
| SPM 快照接收 | JSON 解析、校验、按 session/revision 拒绝旧快照，构建可见画面 | 解析 `O(B)`；画面 `O(K)` | 快照对象及传输缓冲约 `O(B)`；上游带宽/更新频率决定总负担 |
| SPM 每次内容重绘 | `prepare_frame` 构建可见节点、两次 `layout_snapshot`、克隆一次布局；`paint` 遍历并复制节点文本；Canvas 每条文本新建 TextFormat | `O(K + 文本总长度)`，重复布局增大常数 | `O(K + 文本总长度)`；Workspace 行数按视口高度限制，Compact 最多 3 条，Capsule 常数级 |
| 鼠标命中 | SPM hit tree 逆序线性查找；文件视图从几何位置求 cell | SPM `O(K)`；文件几何取决于组数 | 目前 SPM `K` 小，不值得先建空间索引 |

证据入口：`crates/app/src/state.rs`、`crates/app/src/app/items.rs`、`crates/app/src/fence_window/items.rs`、`crates/app/src/fence_window/render.rs`、`crates/app/src/fence_window/hit.rs`、`crates/app/src/layout.rs`、`crates/app/src/icons.rs`、`crates/plugin-spm/src/lib.rs`。

## 当前渲染管线

```text
文件变化 / 用户输入 ─→ State 更新/排序 ─→ ItemView 替换与缓存继承
                    ─→ frame clock 按需 tick ─→ 全量准备项目几何/状态/键
                    ─→ Direct2D 清空并绘制可见 cell 到 CompositionDrawingSurface
                    ─→ Composition SpriteVisual 合成到桌面 HWND

spmd 快照 ─→ pipe/JSON ─→ SPM snapshot ─→ 可见 PanelView
          ─→ hit/semantic 布局 ─→ Canvas/Direct2D 全 surface 绘制
          ─→ 同一内容 SpriteVisual 合成
```

已经有的有效措施：共享渲染栈；chrome/content 分面；绘制时 Y 剔除；内容无变化不持续 tick；项目替换继承图标和标签；图标提取异步；SPM 列表按视口截断。优化应保留这些性质。

## 优化顺序

1. **先量测，不先换 UI 技术栈。** 在 Windows 上对 100/1000/5000 个文件、交错日期组、8/32 个 Fence、SPM 频繁快照分别记录 UI 线程 p50/p95、`prepare/layout/paint/EndDraw` 分段耗时、每帧分配字节、CPU/GPU 图标缓存占用、队列深度、掉帧和滚动到首个可见结果的时间。已有 `app/motion.rs` 动画帧间隔日志可作定位入口，WPR/WPA 与 GPUView 用于实机归因。
2. **文件视图按可见区准备。** 从滚动位置与行高算可见 index 范围，再加 1–2 屏预取；只为该范围提取图标、拟合标签、生成 draw key 和 cell/row。离场动画的项目另做小集合处理。目标是稳态内容准备从 `O(N)` 变为 `O(V + G)`，滚动表面绘制仍取决于可见项目和更新区域。需覆盖分组、反向排序、插入指示、焦点展开与跨页动画。
3. **限制图标工作与缓存。** 可见优先的有界请求队列、重复 key 合并、滚动后取消未启动的离屏请求；分别给 CPU 图像与 GPU bitmap 设字节预算/LRU，DPI/主题/设备 epoch 作为失效键。否则虚拟化后仍可能因历史浏览目录积累内存。`U × icon_width × icon_height × 4` 是未压缩像素的最低量级估计，实际缓存副本与驱动开销另计。
4. **分层失效和局部更新。** 已分离 chrome/content；下一步把静态背景、项目层、hover/selection/焦点反馈区分失效原因。仅 hover 变化可重画旧/新 item 的矩形并合并脏区；滚动/整体布局变化仍先采用全 surface 更新，避免错误的旧像素。当前 `Panel::draw_ex` 调用无更新矩形的 `begin_draw`，需要在封装层新增可选 rect，并正确处理返回 offset、裁剪和首次完整初始化。微软 `ICompositionDrawingSurfaceInterop::BeginDraw` 支持更新矩形，但局部更新的正确性必须验证。[CompositionDrawingSurface BeginDraw](https://learn.microsoft.com/en-us/windows/win32/api/windows.ui.composition.interop/nf-windows-ui-composition-interop-icompositiondrawingsurfaceinterop-begindraw)
5. **减少 SPM 重复工作。** `prepare_frame` 只建一次 layout，比较 `hits` 后更新其 revision，并避免把同一 `LayoutSnapshot` 深拷贝到多个持有者；针对相同 snapshot revision/viewport/presentation/selection/theme/device 复用 prepared view。将稳定的 `TextFormat` 缓存到主题/字体 epoch，重复绘制文本时评估缓存 `TextLayout`，先测文字创建占比再扩大缓存。微软指出重复文本用 `DrawTextLayout` 可避免每次构建布局。[Direct2D 性能指南](https://learn.microsoft.com/en-us/windows/win32/direct2d/improving-direct2d-performance)
6. **降低低频全量操作成本。** 排序前缓存小写名称、扩展名及自然排序键，并合并 namespace 主键；门户维护引用计数或反向索引，将逐项目全表搜索改为接近 `O(R + Q)`；Fence ID/HWND 比对用 `HashSet` 降至预期 `O(F)`。这些先用同规模基准确认收益，避免为小 `F` 增加复杂状态。

## 验收边界

- 采用仓库已有的性能预算作初始门槛：1120×760 DIP、150% DPI、60 Hz 滚动绘制提交 p95 ≤ 8 ms；静止 60 秒平均 CPU ≤ 0.5%；其余预算见 `docs/NEXT_PHASE_ROADMAP_AND_PLANNING.md` §6.3。目标值不是当前成绩。
- 新旧实现需在相同 Windows 硬件、系统版本、DPI、电源模式和预热状态下对比；同时核对视觉结果、点击命中、UIA 语义、设备丢失恢复和快速滚动后图标最终补齐。
- WSL 可测纯排序/布局/快照构建，无法证明 Windows Direct2D/Composition、Explorer 锚定、shell 缩略图或 GPU 帧时间。[Windows Performance Toolkit](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/)、[DirectX 应用分析](https://learn.microsoft.com/en-us/windows/win32/direct2d/profiling-directx-applications)

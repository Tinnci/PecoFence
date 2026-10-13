# TODO 组件与布局持久化

## 用户操作

从托盘或 Fence 右键菜单创建“待办清单”。面板内容复用现有 Canvas/Direct2D 绘制与 Fence 外框；“添加”打开 Win32 文本输入框，点击任务行切换完成状态，“清除已完成”删除已完成任务。列表按可见高度分页，最多 128 条任务，每条标题最多 160 个 Unicode 字符。当前没有编辑单条标题、到期时间、拖动排序或跨设备同步。

## 数据流与保存位置

```text
Win32 菜单 → new_widget_near → FenceContentSpec::Panel
点击/输入 → PanelEvent → TodoWidget → HostCommand::PersistConfig
  → App 验证 provider/instance/config → 所有同 instance_id 的布局副本
  → AppState::mark_dirty → 800 ms 合并保存 → ConfigStore::save
```

TODO 任务是 `PanelSpec.config.tasks`，而不是临时的 `StorageService`；后者当前只有进程内存实现。设置在 `Config.settings`，窗口在 `Config.layouts[*].fences[*]`。整个配置保存为 `config.json`：普通安装默认 `%APPDATA%\PecoFence\config.json`，portable 默认 `<exe>\config\config.json`，`--config-dir` 可覆盖。保存采用临时文件、同步写入、旧文件 `.bak` 与每日备份。单个 provider 配置写入限制为 64 KiB。

每个 `Layout` 用显示器 device path 集合作指纹。Fence 几何数据 `NormGeometry` 保存显示器、工作区相对位置/大小（DIP）、原工作区尺寸和锚点。恢复时按当前 DPI 与工作区重新计算；靠右/下边的面板保持边缘距离，中央面板按相对中心位置缩放，最后限制在工作区内。因此不是固定像素坐标，分辨率和 DPI 改变后仍可尽量保留排布。

## 现在的放置算法

新 Fence 先尝试指定相邻 Fence 的右/左/下/上方，然后尝试鼠标中心；若冲突，以约 24 DIP 步长从工作区左上到右下寻找空位，最后在鼠标附近夹到工作区内。每个候选矩形与现有 `F` 个 Fence 做相交检查，因此最坏时间为 `O(CF)`，`C` 为候选网格数；额外空间为 `O(1)`（少量相邻候选）。这适合目前最多几十个 Fence 的桌面，且保持用户拖动后的手工位置。

建议分阶段扩展：

1. **现在：保持手工排布 + 碰撞放置。** 新增面板使用同一算法；提供“自动整理当前布局”时才重排旧面板，避免启动或添加任务时挪动用户已有位置。
2. **可选固定区域。** 借鉴 [PowerToys FancyZones](https://learn.microsoft.com/en-us/windows/powertoys/fancyzones) 的区域概念，为工作/日历/状态面板保存用户可编辑的区域 ID。先匹配区域，再在区域内按稳定顺序放置；显示器变化时回退到现有锚点恢复。
3. **更密集的面板。** 若 Fence 数显著增长，用空间网格或 R-tree 索引相交候选，降低每次碰撞检查扫描全部 Fence 的成本；需要基准数据后再引入索引与维护复杂度。
4. **长列表。** 当前 TODO 分页只构建一页可见行；若未来需要滚轮与千条任务，应在宿主补滚轮、键盘焦点和可见区索引。可参考 [WinUI ItemsRepeater 的虚拟化思路](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/items-repeater)，无需为此将现有原生渲染整体改成 WinUI。

本地验证：`scripts/test-todo-widget.ps1` 用独立 Windows 实例创建面板、添加/勾选任务、检查 JSON，再重启检查任务与几何数据。

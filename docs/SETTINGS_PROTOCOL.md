# Settings 协议 v1：当前实现

设置页面是应用客户端，不是持久文档的所有者。页面的完整 `view` 只是只读投影；修改只能通过闭合的类型化命令进入用例。

实现入口：

- [Rust 契约、修订时钟与会话去重](../crates/core/src/settings_protocol.rs)
- [应用接纳与用例](../crates/app/src/app/settings.rs)
- [内容/容器配对校验](../crates/app/src/app/fence_options.rs)
- [无 DOM 的客户端与草稿队列](../ui/settings-client.js)
- [生产页面与规则编辑器](../ui/settings.html)

## 身份与接纳

`ready` 带精确 `protocol: 1` 和每次页面创建生成的 UUID `page`。宿主分配 `client` UUID；同一 page 重发 ready 不重置序号。新 page 撤销旧 client 的提交资格。

`DocumentStamp { workspace, revision }` 中 workspace 是本次工作区激活的 UUID，不是文件路径，也不是持久化的文档 ID。载入、新建、导入和恢复接受产生新身份；revision 只在这一身份内比较。页面刷新改变 client，但不改变当前 workspace。

```json
{
  "type": "request",
  "protocol": 1,
  "client": "e4d64f64-379e-4536-a598-d58a0df563c6",
  "sequence": 1,
  "base": {
    "workspace": "80c6d765-4032-4ebf-a46a-aafcd214ccac",
    "revision": 8
  },
  "command": {
    "kind": "setSetting",
    "change": { "property": "peekEnabled", "value": false }
  }
}
```

请求必须满足当前 client、workspace、精确 base revision 和下一个 sequence；不能用另一工作区恰好相同的 revision 绕过校验。每次内存文档变更推进修订，来源健康和门户条目观测不推进修订。无效目标/值在用例边界拒绝；内容编辑和窗口属性编辑都验证 `contentId + containerId` 当前归属配对。

序号从 1 开始，最大为 JavaScript 安全整数。接纳决定消耗序号，包括冲突、只读、校验失败和用户取消；错误 client、错版本、跳号或已过期请求不推进接纳水位。宿主保留最近 32 个请求及决定，相同请求重发返回原 receipt；同序号不同内容拒绝。淘汰的序号拒绝，绝不重新执行。重启/新页面之后没有跨会话恰好一次保证。

输入最多 64 KiB，未知字段、旧 `patchSettings`/`setRules`/`setFence` 协议不接受。没有自动兼容适配器。

## 命令

| kind | 所有者与行为 |
|---|---|
| `setSetting` | 单个设置属性；没有整个 Settings 替换 |
| `setContent` | 标题、文件视图、门户选项；验证当前内容/容器配对 |
| `setContainer` | 外观、锁定、自动高度、Quick Hide、停靠；共享给同一容器的标签 |
| `rule` | 创建、编辑、启停、重排、删除、自动归类开关、默认目标；没有整个 RuleSet 替换 |
| `action` | 快照、模板、文件选择、备份恢复、图标操作和明确工作区替换 |

规则创建/编辑提供 1–32 个 AND 条件。页面支持添加/移除条件和编辑已有规则，含用户桌面、公共桌面、系统 namespace 来源；已有条件和规则身份不会因编辑丢失。集合目标不能是门户或 panel。命令与文件导入使用同一规则验证；时间字段精确使用 `fromMin`/`toMin`。通配符匹配使用有界动态规划，不再递归展开星号分支。

只读恢复状态只开放查看、导出、打开配置目录及明确确认的导入/新建/接受恢复/备份恢复。文件选择取消返回 cancelled 决定，而非保存成功。

## 回复与保存

- `snapshot`：protocol/page/client/stamp/独立视图 sequence/view；推送序号防止旧投影覆盖新投影。
- `receipt`：client/sequence/base/current/rejected/cancelled；`rejected: null` 且未取消只表示内存接纳成功。
- `persistence`：page/client/stamp/committedRevision/issue；只有主文件提交成功才推进 committedRevision。可选备份失败可以同时报告已提交的主文件修订和退化原因。
- `protocolError`：不可解码或不支持的协议；客户端停止继续提交，需重新打开页面。

接纳与保存是不同证据。旧修订的提交不能清掉新修订，旧工作区的提交不能确认新工作区。当前存储仍同步，正常保存计时器/退出路径尚未升级为异步提交、合作写入锁或关闭失败的 retry/discard UX；不能用当前 clock 单测宣称这些 B2 功能已实现。

## 草稿

客户端最多保留 64 个排队/进行中/失败/待投影确认的修改，按顺序只发送一个未决请求。仅在自己的修改获得接纳时推进后续排队修改的 base，绝不因外部 snapshot 自动重放冲突修改。

规则表单在开始编辑时捕获 base，宿主推送不覆盖未提交字段。冲突保留草稿，用户明确选择“重试未提交的修改”才使用最新修订提交新请求；“放弃”只丢客户端草稿。页面/工作区替换不把旧草稿套到新工作区。晚到保存通知不能把新修改显示为已保存。

## 证据与边界

```powershell
cargo test --locked -p pecofence-core settings_protocol
node --test scripts/test-settings-client.cjs
pwsh -NoProfile -File scripts/test-settings-browser.ps1
```

浏览器测试使用生产 HTML/client 与本地 mock；Rust 测试单独验证真实编解码、接纳水位/重放、修订提交和规则校验。完整构建入口也执行两组 JavaScript 测试。

这些不证明真实 WebView2、Run-key、热键和 Explorer 图标的 OS 效果。当前 OS 错误仍通过已有集成路径反馈，独立期望/实际/最近失败状态与异步操作终态属于 C2，见 [ROADMAP](ROADMAP.md)。

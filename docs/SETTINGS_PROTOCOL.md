# Settings 类型化应用边界

Settings 窗口是应用客户端，不是持久文档的所有者。Reactor 承载的原生
WinUI 3 窗口已有五个任务页和十三种规则条件表单。UI 通过 owned `SettingsView` 只读投影和
类型化 `Request`/`Receipt`、`DocumentStamp` 边界调用用例，不使用 HTML、
JavaScript、浏览器进程或 JSON 页面消息桥。

实现入口：

- [命令、修订时钟与会话接纳](../crates/core/src/settings_protocol.rs)
- [只读投影类型](../crates/app/src/settings_ui.rs)
- [应用接纳、恢复与设置用例](../crates/app/src/app/settings.rs)
- [内容/容器配对校验](../crates/app/src/app/fence_options.rs)

文件名 `settings_protocol.rs` 保留，但 Request/Receipt 是进程内 Rust 类型；
序列化实现只用于配置值及明确的测试输入，不用于生产 UI 传输。

## 身份与接纳

每次窗口创建产生独立 source UUID，应用为这一激活打开 `SettingsSession`
并分配 client UUID。关闭窗口撤销 client。排队事件带 source，而不是可复用 HWND；
旧窗口事件不能修改另一次激活。

`DocumentStamp { workspace, revision }` 中 workspace 是本次工作区激活的 UUID，
不是文件路径。新建、导入和接受恢复产生新身份；revision 只在这一身份内比较。
重新打开设置窗口更换 client，不更换 workspace。

请求必须满足当前 client、workspace、精确 base revision 和下一个 sequence。
另一工作区恰好相同的 revision 不能绕过校验。每次文档变更推进修订；
来源健康和门户条目观测不推进修订。内容和窗口属性编辑同时验证
`content_id + container_id` 当前归属，不能把迁移前的编辑应用到新窗口。

接纳决定消耗序号，包括冲突、只读、校验失败和用户取消。错误 client、版本、
跳号或过期请求不推进水位。应用保留最近 32 个请求及决定；同序号同内容返回原
receipt，同序号不同内容拒绝。淘汰的序号不能重新执行。没有跨重启恰好一次保证。

## 命令与风险

| 命令 | 所有者与行为 |
| --- | --- |
| `SetSetting` | 单个全局设置；控件不能替换整个 Settings |
| `SetContent` | 标题、文件视图、门户选项；验证内容/容器配对 |
| `SetContainer` | 外观、锁定、自动高度、Quick Hide、停靠；影响同一窗口的标签 |
| `Rule` | 创建、编辑、启停、重排、删除、自动归类、默认目标 |
| `Action` | 快照、模板、原生文件选择、备份恢复、图标操作和明确工作区替换 |

规则支持 1–32 个 AND 条件，覆盖所有持久模型条件。目标只能是文件集合；
门户和业务 panel 不能成为虚拟归类目标。命令与导入使用同一规则验证。
用户通过原生表单操作，不必编写 JSON。

只读恢复保留查看、导出、打开配置目录及明确确认的导入、新建、接受恢复和备份
恢复入口。替换前保留原文件。取消文件选择返回 cancelled，不是保存成功。
恢复/删除等风险操作有原生确认。

`RetrySave` 明确重新派发失败的提交。`CancelClose` 只取消退出等待，不取消
writer 已经执行的副作用。关闭设置窗口也不取消应用拥有的保存。

## 决定、保存与草稿

`Receipt { client, sequence, base, current, rejected, cancelled }` 表示命令决定。
无拒绝、未取消只说明应用已接纳，不说明已经落盘。`SettingsView` 独立提供
`committed_revision`、dirty、saving、closing 和保存问题。

旧修订提交不能清掉较新修改，旧工作区提交不能确认新工作区。主文件成功后才
推进 committed revision；备份失败可同时报告主文件已提交与退化警告。
保存与退出策略见 [PERSISTENCE.md](PERSISTENCE.md)。

原生适配器保留草稿，与最新投影分离。后台更新不能重写正在编辑的字段、
改变所选目标或转移焦点。冲突和校验失败保留草稿；重试采用最新修订必须由
用户明确决定。工作区替换不能自动把旧草稿应用到新工作区。

## 验证与边界

```powershell
cargo test --locked -p pecofence-core settings_protocol
cargo test --locked -p pecofence settings_host
cargo test --locked -p pecofence persistence
```

报告的 headless 原生 Settings 测试和包含关闭/重开的合成 WinUI tour 已通过；
一次本机探测在抽样 tour 中加载 18 个 app-local Windows App SDK DLL，未观察到浏览器模块或子进程。
这些结果不证明全部 N01–N08 工作流已通过，也不证明未受测路径不存在浏览器加载。
完整工作流还需 Windows 实机键盘、UI Automation/Narrator、多 DPI、高对比、文字缩放和
干净环境验收；还需核验 self-contained Windows App SDK Runtime 2.5.1 的确切清单，
且 WebView2 runtime、loader、helper 及 UI 资源不在 imports、动态加载路径、进程树或最终发行包中。
最终构建与 package 测量待补。详细分层见[验证门槛](VERIFICATION_GATES.md)。
自动化不能证明 Run-key、热键或 Explorer 图标副作用全部成功。OS 集成的独立
期望/实际/最近失败状态、跨进程锁和外部编辑检测仍属于后续工作，
不能因 UI 原生化宣称它们已完成。

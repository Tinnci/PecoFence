# B2-1：串行异步保存与退出

此切片实现当前应用自身的单写入者、修订确认与正常退出策略；**不是**跨进程锁、外部编辑 CAS、断电恢复或文件传输 journal 的完整实现。

## 所有权

- [AppState](../crates/app/src/state.rs) 拥有文档、当前修订、已提交修订和带修订的明确替换屏障。只准备 owned `SavePlan`，不在生产保存路径写文件。
- [SaveCoordinator / Writer](../crates/app/src/persistence.rs) 拥有写入 ticket 与有界渠道，独立于 Settings 页面和窗口。worker 仅接收 owned、Send 数据，不接触 HWND、COM 或 UI 回调。
- [原生接线](../crates/app/src/app/persistence.rs) 合并保存意图，轮询结果、更新状态和显示退出选择。
- [ConfigStore](../crates/core/src/config_store.rs) 仍执行精确格式校验、同目录暂存、主文件替换、原件 archive 和备份 receipt；这些写入现在运行在专用线程。

请求/结果 channel 容量均为 1，应用只允许一个 accepted write。在途期间不积累配置副本；完成后只准备最新修订，合并中间修改。提交 r8 时内存变成 r9，只确认 r8，r9 仍为 dirty。准备 snapshot 的克隆仍在 UI；完整 metadata 分离和更轻的不可变文档属于后续工作。

## 完成与替换

ticket 有独立单调 ID、工作区激活、修订和写入模式。迟到/重复/错误 ticket 不能释放当前槽；同修订重试使用新 ticket。

工作区替换不会杀掉已派发写入。旧任务按串行顺序完成并被核算，但旧激活的成功/失败都不能确认或污染新激活。新激活随后执行自己的明确替换，保留磁盘上实际已有的主文件。

替换屏障不是 bool，而是 `DocumentStamp`：较早 replace 的成功不能清掉稍后同激活的恢复/替换意图。普通后续修改在屏障已被覆盖后使用 normal commit，不为每次保存重复 archive。

主文件失败保留 dirty 与替换屏障。失败不会自发形成重试循环；新的保存意图、后续修改或用户明确重试才可重新提交。备份退化可以确认主文件修订，但保留警告。Settings 保存成功 toast 也等待真实结果，页面关闭不取消写入或丢弃其结果。

Settings 组件事件附带独立 source UUID，不用可复用 HWND 识别 owner；关闭窗口撤销 session。选定的 Reactor 接线将由主 STA `Reactor::run_with` 继续拥有 `App`、桌面和托盘生命周期：关闭 Settings 组件窗口只撤销该窗口的 client，不退出整个应用，也不取消应用拥有的保存。旧窗口排队的 request/close 不能影响新窗口。不存在浏览器 ready 消息。

## 正常退出

退出进入 Waiting：取消防抖，停止接纳修改，请求最新 dirty 修订，继续消息泵和后台写入，**不 join**。

- 主文件确认且没有较新 dirty 文档时才退出。
- 保存失败时原生对话框提供取消、重试、继续；默认取消。只有没有在途写入且用户明确选择继续时，才放弃未提交修改并退出。
- 在途写入等待过久时，Settings 或托盘的“取消退出”恢复使用；取消的是退出，不是已经执行的文件写入。
- 取消退出后重新投影窗口，并处理等待期间保留的桌面来源通知。

OS 的 QUERYENDSESSION 只尽力派发保存，不阻塞结束会话；ENDSESSION 可以直接结束。强制结束、崩溃、`panic = abort` 没有未提交修改保存保证。

断连没有 receipt 时保留未确认状态，worker 不自动重建或重放。测试 profile 的 thread panic 可验证断连终态，但 release panic 会终止进程；不能把测试当作 release panic 恢复承诺。

## 自动化证据与未完成项

```powershell
cargo test --locked -p pecofence persistence
cargo test --locked -p pecofence state::tests
cargo test --locked -p pecofence settings_host
```

覆盖单飞/合并、重复与旧 ticket、阻塞写入不阻塞调用方、失败/重试/退出选择、真实临时目录写入与 archive 原始字节、跨激活及同激活替换屏障、备份退化和会话撤销。原生 Settings 的 headless 测试和包含关闭/重开的合成 WinUI tour 报告通过；这类保存/协议和合成窗口证据不等于 N01–N08 功能对等，也不证明键盘/读屏、DPI、高对比度或真实 Windows 退出行为通过。分层验收见[验证门槛](VERIFICATION_GATES.md)。

仍需：真实 Windows 模态对话框/退出/OS 结束验收；合作写入者锁与外部变更检测；完整持久/观测分离；文件传输的逐项结果与 journal；未知磁盘结果的独立核对流程。当前退出保证只针对文档保存，不能据此声称所有已派发 Shell 文件任务都被 drain/journal。

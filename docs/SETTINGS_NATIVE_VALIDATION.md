# Reactor / WinUI 3 设置迁移验证记录

本记录区分已运行的验证和仍待完成的验收。设置宿主已替换为 Reactor
组件，五个任务页使用原生 WinUI 3 控件；这不等于整个迁移已经达到发行标准。

## 已验证

- `cargo test --offline --locked --workspace` 通过；真实窗口测试默认忽略，
  不会在普通单元测试中启动桌面应用。
- `cargo clippy --offline --locked --workspace --all-targets -- -D warnings`
  通过。
- 完整 `build-and-verify.ps1` 链路通过：源码政策、fmt、严格 Clippy、
  生成绑定一致性、工作区及 validator 测试、release 构建、运行时 staging
  和实际 portable ZIP 校验。Reactor 退场及关闭屏障回归测试纳入完整 Windows CI。
- 设置组件测试覆盖请求单飞、有界队列、接纳与保存的区别、工作区替换、
  旧投影、提交期间继续编辑、原始草稿修订、显式重试及内容/容器身份。
- 新增回归测试覆盖显式 Inbox 角色（不按集合顺序推断）、新规则接纳后
  继续编辑保留同一规则身份、草稿替换保护，以及绑定工作区、会话和对话框
  token 的确认。取消或过期对话框不产生授权命令。
- 条件表单测试覆盖全部 13 种持久规则条件；更新已有条件不受 32 条上限阻断。
- 无窗口 Reactor 测试挂载五页与规则编辑器。通知、冲突和主题更新保持
  正在编辑的文本控件身份，避免因条件控件插入而重建文本输入。
- 界面使用原生 `TitleBar` 和自适应 `NavigationView`，标题栏汉堡按钮控制
  左侧导航，避免重复汉堡按钮。实际窗口测试验证 1080/800/580 DIP 宽度下
  的 Expanded/Compact/Minimal 模式、收起/展开和页选择；导航变化不丢弃草稿。
- 独立的真实 WinUI 测试在当前 Windows 环境打开五页及 13 种条件表单，
  在十种语言下自动挂载全部五页。原生 `ContentDialog` 取消、带对话框的
  组件关闭、标题栏关闭与重开、应用退出均通过；三次退场事件均被收集。
- 固定 Reactor 版本存在析构期间 `Hide()` 同步触发 `Closed` 的重入借用
  崩溃，以及窗口关闭后才隐藏对话框导致的失效 XAML root 访问。
  仓库内修补在 root 有效时准备关闭屏障，释放借用后隐藏对话框，等待
  `Closed` 后再关闭窗口或退出应用。root 的终止状态同时阻止排队重绘重新
  打开原对话框或创建新对话框。实际窗口测试保留全部三种关闭路径。
- 运行时验证检查 206 个文件、200 个 PE 图像的大小、哈希、静态及延迟导入。
  已运行原生测试的采样观察记录了 19 个从测试目录加载的运行时 DLL，
  未观察到 WebView 模块或浏览器子进程。Rust 测试程序的系统
  `conhost.exe` 不被当作浏览器进程。
- 运行时来源是固定版本 Windows App SDK Runtime 2.5.1，未运行 Appx
  安装、修改全局运行时或卸载用户已有软件。真实窗口测试的命令队列未
  连接桌面 `App`，不会更改 Explorer 图标、启动项、热键或真实工作区。

## 复现原生测试

在仓库根目录执行；需要 Windows、Rust MSVC 工具链及 uv：

```powershell
uv run --no-project --python ">=3.11" python scripts/test-winappsdk-runtime.py
uv run --no-project --python ">=3.11" python scripts/stage-winappsdk-runtime.py stage --target-dir target --profile debug
cargo test --locked -p pecofence --no-run --message-format=json > target/native-test-build.json
if ($LASTEXITCODE -ne 0) { throw "Native test build failed" }
$testBinary = Get-Content target/native-test-build.json |
    ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq "compiler-artifact" -and $_.target.name -eq "pecofence" -and $_.profile.test -and $_.executable } |
    Select-Object -ExpandProperty executable
./scripts/test-native-settings.ps1 -TestBinary $testBinary
```

证据写入 `target/native-settings-evidence.json` 及对应测试日志。
运行时、构建和证据产物不进入 Git。窗口测试只能单独运行，不能把同一个
进程内多次 WinUI 初始化当作应用关闭/重开的替代。

## 体积测量与发行门禁

自适应导航、完整翻译及最终 Reactor 关闭屏障修补后的正常 release 构建主程序为
**6,225,408 字节（5.94 MiB）**。没有采用对照实验中的 Reactor `s`
优化，也没有移除诊断来满足预算。
用户已批准主程序门禁为 **6.5 MiB（6,815,744 字节）**；
它是 EXE 单项上限，不是自包含运行时或整个安装包的预算。

自包含运行时另有 **59,158,503 字节（约 56.4 MiB）**，不计入 EXE
测量。已构建并验证的 portable ZIP 为 **25,158,775 字节（23.99 MiB）**，
解包内容 **66,150,387 字节（63.09 MiB）**；SHA-256：
`7e27ef6d57d529b12d680351172acf60426cae7c827a96e9056065b1574cb571`。
该包来自补写本次测量记录前的提交前工作树，真实包的安装 dry-run 验证也通过；它是本地
构建/打包证据，不是已发布的 clean-tag 发行产物。正式发行需从目标
干净提交重新构建，不能复用修改来源后的构建收据。

## 仍待完成

- 九种译文已补全，源码文案及占位符检查通过；全部十种语言的人工布局验收仍待完成。
- 打包、安全安装与 CI/lab 集成已落地；25 项打包、14 项运行时、
  24 项合成 lab 检查及本地真实包验证通过；clean-tag 发行产物仍需重新构建。
- 真实桌面 `App` 的菜单、托盘、保存退出和 OS 集成端到端验证。
- 键盘操作、UI Automation、Narrator、高对比、文本缩放和
  100%/150%/200% DPI 验收。
- 未安装 Windows App SDK Runtime / WebView2 的干净环境验证。

当前采样进程观察不证明所有未来功能路径均不创建浏览器进程；原生控件和
组件测试通过也不证明读屏或完整可访问性验收通过。

# Agent CLI 使用与设计约定

PecoFence 本体是 Windows GUI，不把启动参数伪装成查询 API。Agent 的打包/验证入口是 `scripts/package_desktop.py`。

```sh
python scripts/package_desktop.py verify --archive release.zip --sha256 <trusted-sha256> --summary --json
python scripts/package_desktop.py install --archive release.zip --sha256 <trusted-sha256> --destination versions --dry-run --json
```

每个子命令说明本地副作用。`--json` 提供版本化成功/错误信封；错误在 stderr，参数错误退出 2，操作失败退出 1。
不加 `--json` 保留原有输出。`verify --summary` 只返回版本、文件数和校验值。
install 预演仍读取并验证归档，但不创建目录、不解压、不启动打包的程序；capture/record-build 会运行本地构建工具并写记录。
校验值必须来自可信渠道；不能把校验失败当成可自动重试的安装指令。
验证：`python scripts/test-cli-guidance.py` 和 `python scripts/test-package-desktop.py`。

## 工作区查询与整理建议：`pecofence-cli`

只读控制台程序，随 `pecofence.exe` 一起发布；不写工作区、不连接正在运行的 PecoFence。

```sh
pecofence-cli fences --json
pecofence-cli suggest --fence Desktop --kind move --json
pecofence-cli order --fence Apps --by frequent --limit 10 --json
```

`suggest` 默认最多 10 条，`order` 默认最多 50 项；`meta.total`/`meta.truncated` 说明范围，`meta.savedAt` 表示结果对应的保存时间。
信封与上面相同；工作区缺失或无法读取退出 1，参数错误、找不到栅栏或栅栏重名退出 2，并给出下一步。
建议需由用户在栅栏菜单「建议」中应用；Agent 只呈现建议及其理由，不代为修改。规则、阈值与边界情况见 [整理建议](SUGGESTIONS.md)。
`status` 只读，报告运行/关闭中/退出方式/崩溃、看门狗与日志；`quit` 是唯一有副作用的命令：请求应用保存并退出，未运行时直接成功，超时返回 `QUIT_TIMEOUT` 且绝不强制结束。升级前先 `quit` 并确认 `exit: normal`。见 [进程生命周期](LIFECYCLE.md)。
验证：`cargo test -p pecofence-cli -p pecofence-core`。

## 共同设计标准

描述说明适用场景和副作用；示例先行；默认结果有界且说明范围。错误必须给出可执行修正方向，
原请求可否重试与修正参数后可否调用是不同概念。保留已有机器协议；新增模式显式选择。
外部内容是数据，不能改变本地工具的操作规则。使用隔离测试验证发现、纠错、空结果与失败后的状态。

参考：[Anthropic 工具设计](https://www.anthropic.com/engineering/writing-tools-for-agents)、
[CLI Guidelines](https://clig.dev/)。这里的具体接口设计是结合各项目用途做出的工程选择。

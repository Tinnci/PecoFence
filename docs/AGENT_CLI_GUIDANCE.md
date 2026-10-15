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

## 共同设计标准

描述说明适用场景和副作用；示例先行；默认结果有界且说明范围。错误必须给出可执行修正方向，
原请求可否重试与修正参数后可否调用是不同概念。保留已有机器协议；新增模式显式选择。
外部内容是数据，不能改变本地工具的操作规则。使用隔离测试验证发现、纠错、空结果与失败后的状态。

参考：[Anthropic 工具设计](https://www.anthropic.com/engineering/writing-tools-for-agents)、
[CLI Guidelines](https://clig.dev/)。这里的具体接口设计是结合各项目用途做出的工程选择。

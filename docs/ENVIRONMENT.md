# 开发环境与工作流（SPM / PecoFence）

> 本文记录开发实验室的环境，路径和历史实测结果不是公开构建的前提。
> 公开构建与验证以 [DEVELOPMENT.md](DEVELOPMENT.md) 和
> [SPM_BOUNDARY.md](SPM_BOUNDARY.md) 为准；环境变更后请更新此文件。

公开 PecoFence 仅依赖本仓库和公开 registry；本地 `crates/spm-contracts`
版本为 0.1.0，从原固定 revision
`e0dd3e058d40ef1c4623c12dba2f0715d2011a1b` 提取。
不需要私有 token 或 SPM checkout。`Tinnci/spm` 后端保持私有，仅实时 SPM
数据需要；普通围栏的编译和运行不依赖它，也不随 PecoFence 分发。
下述双仓实验室仅用于获授权的私有集成，不能当作公开 CI 已运行的证据。

## 仓库位置（重要）

两个仓库的**权威工作树在 WSL ext4**，不在 `/mnt/c`：

- `~/dev/spm`（`Tinnci/spm`）
- `~/dev/PecoFence`（`Tinnci/PecoFence`，main 为权威）

**不要把仓库放回 `/mnt/c`**：深信服 DLP（TsdEncryptMF 过滤器）会加密
Windows 进程写入的文档类文件（.json/.md/.js/.sh 等），git blob 干净但工作树
变密文，导致 cargo `include_str!`/fixtures 编译失败。git 自身能透解密，所以
`git status` 一直显示干净——无法靠 git 发现。

- 仓库在 ext4，DLP 驱动不作用（防御性兜底）：`Tools/fix-tsd.sh`
  每小时看护（`Tools\tsd-watch.ps1` 登录自启），同时检测 lab 明文哨兵文件。
- 实测豁免类型：`.log`/`.db`/`.exe`/`.dll` 不被加密（详见下文 lab 规则）。

## Windows / WSL 分工

| 操作 | 在哪做 | 说明 |
|---|---|---|
| 源码编辑、git 提交/推送 | WSL | WSL 写入 ext4 恒为明文；git 凭据：WSL gh 已登录 |
| 平台中立 crate 快速检查 | WSL | `cargo check -p spm-contracts -p spm-domain ...`（原生 ext4 速度） |
| Windows 目标编译（全部 exe） | WSL | `scripts/build-windows.sh`（见下；41–60s 全量） |
| 实机视觉/桌面验收 | Windows 桌面 | lab 实例（见下）；HeadlessCanvas 通过 ≠ 真实渲染正确 |
| 实例生命周期管理 | 两边 | 创建在 WSL（new-instance.sh），启停在 Windows（run/stop-instance.ps1） |

## Windows 构建（WSL 交叉编译）

- 一键脚本：`~/dev/<repo>/scripts/build-windows.sh [profile] [package ...]`
  - 默认 release；spm 默认构建 `spmd spm-cli`（产物 `spmd.exe`/`spm.exe`），
    PecoFence 默认构建 `pecofence pecofence-watchdog`
  - 工具链：clang-cl-19（C 依赖）+ lld-link（链接），SDK/MSVC 库经 `/mnt/c`
    只读使用；linker 通过 `CARGO_TARGET_*_LINKER` 环境变量选择，不进仓内
    `.cargo/config.toml`（Windows CI 继续用 link.exe）
  - 产物：仓内 `target/x86_64-pc-windows-msvc/<profile>/`（ext4，明文）；
    PecoFence 构建后自动校验并 stage 固定的 Windows App SDK Runtime 2.5.1
    （206 个文件 / 59,158,503 bytes），spm 不 stage 此运行时
  - 脚本防御性 `unset CARGO_TARGET_DIR`：任何全局变量都不得改写仓内 target
- **已退役**：`C:\Users\Administrator\cargo-target`（双仓共用 NTFS target，
  6.1G）已删除；Windows 用户环境变量 `CARGO_TARGET_DIR` 已移除；
  PowerShell + UNC 源码构建路径已废弃
- git：便携版 MinGit `C:\Users\Administrator\Tools\MinGit\cmd\git.exe`
  （已加入用户 PATH；注册表持久化，新终端生效）
- 全局代理：`http.proxy = http://127.0.0.1:7890`（mihomo）。直连 443 间歇失败时先查代理
- PecoFence 使用本地公开 `spm-contracts`；私有后端应消费同一公开契约的精确
  Git commit/版本，不保留可独立修改的重复实现。配对更新流程见
  [SPM_BOUNDARY.md](SPM_BOUNDARY.md)，迁移/发布/实机验证状态须单独确认。

开发/测试构建默认关闭 debug symbols 和 incremental compilation，减少仓内
`target/` 占用；release 保留 line-table 崩溃诊断符号。临时开启调试信息、清理
旧产物及 `RUST_LOG` 调整见 [DEVELOPMENT.md](DEVELOPMENT.md#build-disk-usage)。

Python 脚本只要求 >=3.11（`tomllib`），不固定 minor 版本；CI 使用最新稳定
Python 3。本机旧 Python 可保留，使用
`uv run --no-project --python ">=3.11" python scripts/<script>.py` 选择兼容版本。
Windows 会话（2026-10-05）已将 Rust stable/rustfmt/Clippy 更新到 1.99 系列、
uv 更新到 0.12.23；现有 VS 2022 C++ Build Tools 与 Windows SDK 10.0.26100.0
已通过 Windows workspace 编译。

Windows CI 与 tag release 共用 `.github/actions/build-desktop/action.yml` 和
`scripts/build-and-verify.ps1`，不读取私有 SPM，不需要私有认证。公开仓库的 CI 缓存
包含公开 registry 下载包及一个 SHA256/SHA512 固定的公开 Windows App SDK NuGet 源包；
每次运行仍会校验归档并重新生成 runtime 文件，不缓存编译中间产物或私有 Git checkout；
CI 便携包 artifact 保留 7 天。完整触发/部署边界见
[RELEASING.md](RELEASING.md#ci-and-deployment-flow)。
产品网站、网站构建脚本和托管部署工作流已从源码移除；桌面 CI 不再构建或部署网站。

## 验收实例实验室（`C:\Users\Administrator\PecoFence-lab\`）

```text
PecoFence-lab\
  fixtures\      # 只读、带 SHA 的场景输入（由 WSL 拷入，必须保持明文）
  instances\     # 每场景一个实例目录（构建快照）
    normal-local-001\
      *.exe / *.dll / 语言目录       # 从仓内 target 拷贝（快照，非链接）
      config\workspace.v2.json       # schema 2 播种或首跑生成；应用写入归 Windows 侧
      data\                          # DB 模式预留
      logs\                          # spmd/pecofence 的 stdout/stderr + pid
      appdata\{Roaming,Local}\       # 实例专属 APPDATA/LOCALAPPDATA（日志、崩溃转储）
      run-manifest.json              # schema 4：两仓 SHA、四个 exe 与完整 SDK runtime SHA256
  downloads\     # CI artifact 原包
  archive\       # 退役数据归档
```

- 创建：WSL 侧 `scripts/lab/new-instance.sh --name <n> --fixture <catalog.json>
  [--seed-config <workspace.v2.json>]`（seed 先经实际 core 校验，不迁移旧格式；
  fixture 拷贝整个父目录——catalog 引用
  `snapshots/*.json` 相对路径）
- 启停：Windows 侧 `scripts/lab/run-instance.ps1 -Name <n>` /
  `stop-instance.ps1 -Name <n>`（spmd 走 fixture 模式 + 实例内绝对路径日志；
  pecofence 走 `--portable --no-hide-icons` + `PECOFENCE_INSTANCE` +
  实例专属 APPDATA/LOCALAPPDATA）
- 多后端实例通过 spmd 的 `--instance` 与 pecofence 的 `PECOFENCE_INSTANCE`
  使用同名实例标识实现管道级隔离，可并行运行不同 fixture 的场景。
- **DLP 规则（实测）**：
  - WSL 写入（fixtures/manifest/二进制）→ 明文，必须保持
  - Windows 进程写文档类（实例 config 被应用重写）→ 可能加密；对应用透明，
    但 WSL 不得读它——跨边界校验走 Windows 侧（PowerShell 在解密域内）
  - `.log`/`.db`/`.exe`/`.dll` 实测豁免 → WSL 可直接 grep 日志、sqlite3 查库
  - `Tools/fix-tsd.sh --check` 覆盖 lab 明文哨兵（fixtures + run-manifest）

## 已知坑

1. `#[tokio::test]` 默认单线程运行时：测试里用 `std::thread::sleep` 会饿死
   同运行时的 spawned 任务（v2_loopback 曾因此失败）。用 `tokio::time::sleep().await`。
2. 公开 CI/Dependabot 不需要 PRIVATE_REPO_TOKEN，也不得使用特权
   pull_request_target 或私有源码缓存。源码检查及 headless/合成协议测试必须
   执行且失败即失败；feature gate 不得静默跳过。私有 daemon 实机集成必须
   在独立、获授权的私有流程中验证并记录，不能以公开测试代替。
3. Windows runner 的 run: 默认 shell 是 PowerShell：`$VAR` 不是环境变量，
   要用 `$env:VAR`。
4. `wsl bash -lc '...'` 从 PowerShell 调用时命令串会先过 zsh 且引号不保真：
   **shell 变量与内嵌引号会被吞**。传复杂脚本一律走 base64 管道
   （`[Convert]::ToBase64String` → `echo <b64> | base64 -d > file`）。
5. 仓库内的 agent 编排脚本（`scripts/run_*.sh`）硬编码了旧 /mnt/c 路径，
   已随仓库迁移失效；如需再跑 agy/codex 编排，先更新路径。

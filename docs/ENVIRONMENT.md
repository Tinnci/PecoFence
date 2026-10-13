# 开发环境与工作流（SPM / PecoFence）

> 本文档是新会话/新代理上手的唯一环境事实来源。任何环境变更后请更新此文件。

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
  - 默认 release；spm 默认构建 `spmd spm-cli spm-widget`（产物 `spmd.exe`/`spm.exe`/`spm-widget.exe`），
    PecoFence 默认构建 `pecofence pecofence-watchdog`
  - 工具链：clang-cl-19（C 依赖）+ lld-link（链接），SDK/MSVC 库经 `/mnt/c`
    只读使用；linker 通过 `CARGO_TARGET_*_LINKER` 环境变量选择，不进仓内
    `.cargo/config.toml`（Windows CI 继续用 link.exe）
  - 产物：仓内 `target/x86_64-pc-windows-msvc/<profile>/`（ext4，明文）
  - spm 发布 ZIP：构建后执行 `python3 scripts/package-windows.py --release-dir target/x86_64-pc-windows-msvc/release`，输出 `dist/spm-<version>-x64.zip` 和校验文件；CI 使用同一脚本。
  - `scripts/build-windows.sh test spmd` 可在 WSL 编译 Windows 测试 EXE；需显式运行所需测试 EXE（脚本本身不执行）。
  - 脚本防御性 `unset CARGO_TARGET_DIR`：任何全局变量都不得改写仓内 target
- **已退役**：`C:\Users\Administrator\cargo-target`（双仓共用 NTFS target，
  6.1G）已删除；Windows 用户环境变量 `CARGO_TARGET_DIR` 已移除；
  PowerShell + UNC 源码构建路径已废弃
- git：便携版 MinGit `C:\Users\Administrator\Tools\MinGit\cmd\git.exe`
  （已加入用户 PATH；注册表持久化，新终端生效）
- 全局代理：`http.proxy = http://127.0.0.1:7890`（mihomo）。直连 443 间歇失败时先查代理
- spm-contracts 依赖固定在 git rev（见根 Cargo.toml）；契约变更后手动 bump rev

## 验收实例实验室（`C:\Users\Administrator\PecoFence-lab\`）

```text
PecoFence-lab\
  fixtures\      # 只读、带 SHA 的场景输入（由 WSL 拷入，必须保持明文）
  instances\     # 每场景一个实例目录（构建快照）
    normal-local-001\
      *.exe + WebView2Loader.dll     # 从仓内 target 拷贝（快照，非链接）
      config\config.json             # 播种或首跑生成；应用会重写 → 归 Windows 侧
      data\                          # DB 模式预留
      logs\                          # spmd/pecofence 的 stdout/stderr + pid
      appdata\{Roaming,Local}\       # 实例专属 APPDATA/LOCALAPPDATA（WebView2 profile、崩溃日志）
      run-manifest.json              # 溯源：两仓 SHA、配对 rev、每个文件 SHA256
  downloads\     # CI artifact 原包
  archive\       # 退役数据归档
```

- 创建：WSL 侧 `scripts/lab/new-instance.sh --name <n> --fixture <catalog.json>
  [--seed-config <config.json>]`（fixture 拷贝整个父目录——catalog 引用
  `snapshots/*.json` 相对路径）。创建时检查本地 `spm-contracts` 与
  PecoFence 锁定提交一致；仅实验性配对可显式加 `--allow-contract-drift`。
- 启停：Windows 侧 `scripts/lab/run-instance.ps1 -Name <n>` /
  `stop-instance.ps1 -Name <n>`（spmd 走 fixture 模式 + 实例内绝对路径日志；
  pecofence 走 `--portable --no-hide-icons` + `PECOFENCE_INSTANCE` +
  实例专属 APPDATA/LOCALAPPDATA）
- 多后端实例通过 spmd 的 `--instance` 与 pecofence 的 `PECOFENCE_INSTANCE`
  使用同名实例标识实现管道级隔离，可并行运行不同 fixture 的场景。
- 启动脚本会拒绝同一实例目录内仍在运行的进程，并在 spmd 或 PecoFence
  启动后立即退出时报告错误；停止脚本只根据 PID 结束可执行文件仍位于该目录内的进程。
- 默认 PecoFence 持有会话内新旧两个单实例互斥体；spmd v2 的首个命名管道实例
  由进程在初始化数据库前占用。相同用户、会话、实例标识下的新旧 v2 版本不能同时
  服务同一端点。命名实例在未传 `--db` 时使用独立的稳定哈希数据库文件名；显式
  `--db` 仍由调用方保证隔离。此前使用旧默认 `spmd-v2.sqlite` 的命名持久实例，
  升级时应显式传原数据库绝对路径；新默认文件名不会自动迁移旧数据。
- 命名 PecoFence 实例的配置仍由启动目录（`--portable`）或 `APPDATA` 决定；
  `PECOFENCE_INSTANCE` 本身只隔离互斥体、日志、WebView2 profile 和 SPM 管道。
  Lab 为每个实例复制可执行文件并设置独立 APPDATA/LOCALAPPDATA，避免配置互相覆盖。
- 便携版双程序更新使用 `scripts/update-portable-pair.ps1`，文档见
  `docs/PORTABLE_UPDATE.md`。新版本使用安装根目录共享 `config/`，先切换并探测
  spmd，再让 PecoFence 正常退出并重建窗口；失败会回滚 SQLite 文件和进程。
  旧的 `replace-running-instance.ps1` 已退役，禁止按进程名全局终止实例。
- **DLP 规则（实测）**：
  - WSL 写入（fixtures/manifest/二进制）→ 明文，必须保持
  - Windows 进程写文档类（实例 config 被应用重写）→ 可能加密；对应用透明，
    但 WSL 不得读它——跨边界校验走 Windows 侧（PowerShell 在解密域内）
  - `.log`/`.db`/`.exe`/`.dll` 实测豁免 → WSL 可直接 grep 日志、sqlite3 查库
  - `Tools/fix-tsd.sh --check` 覆盖 lab 明文哨兵（fixtures + run-manifest）

## 已知坑

1. `#[tokio::test]` 默认单线程运行时：测试里用 `std::thread::sleep` 会饿死
   同运行时的 spawned 任务（v2_loopback 曾因此失败）。用 `tokio::time::sleep().await`。
2. PecoFence CI 认证私有依赖：必须 unset actions/checkout 持久化的
   `http.https://github.com/.extraheader`（GITHUB_TOKEN 无权读 Tinnci/spm），
   再用 PRIVATE_REPO_TOKEN 凭据助手。
3. Windows runner 的 run: 默认 shell 是 PowerShell：`$VAR` 不是环境变量，
   要用 `$env:VAR`。
4. `wsl bash -lc '...'` 从 PowerShell 调用时命令串会先过 zsh 且引号不保真：
   **shell 变量与内嵌引号会被吞**。传复杂脚本一律走 base64 管道
   （`[Convert]::ToBase64String` → `echo <b64> | base64 -d > file`）。
5. 仓库内的 agent 编排脚本（`scripts/run_*.sh`）硬编码了旧 /mnt/c 路径，
   已随仓库迁移失效；如需再跑 agy/codex 编排，先更新路径。

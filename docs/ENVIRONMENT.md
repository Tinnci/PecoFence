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
  - 默认 release；spm 默认构建 `spmd spm-cli`（产物 `spmd.exe`/`spm.exe`），
    PecoFence 默认构建 `pecofence pecofence-watchdog`
  - 工具链：clang-cl-19（C 依赖）+ lld-link（链接），SDK/MSVC 库经 `/mnt/c`
    只读使用；linker 通过 `CARGO_TARGET_*_LINKER` 环境变量选择，不进仓内
    `.cargo/config.toml`（Windows CI 继续用 link.exe）
  - 产物：仓内 `target/x86_64-pc-windows-msvc/<profile>/`（ext4，明文）
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
  `snapshots/*.json` 相对路径）
- 启停：Windows 侧 `scripts/lab/run-instance.ps1 -Name <n>` /
  `stop-instance.ps1 -Name <n>`（spmd 走 fixture 模式 + 实例内绝对路径日志；
  pecofence 走 `--portable --no-hide-icons` + `PECOFENCE_INSTANCE` +
  实例专属 APPDATA/LOCALAPPDATA）
- **多后端场景不能并行**：v2 命名管道按用户 SID+会话计算，双端都没有实例
  覆盖参数（后续开发项）；当前顺序运行，或多个前端共享同一 spmd
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
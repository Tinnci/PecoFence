# 开发环境与工作流（SPM / PecoFence）

> 本文档是新会话/新代理上手的唯一环境事实来源。任何环境变更后请更新此文件。

## 仓库位置（重要）

两个仓库的**权威工作树在 WSL ext4**，不在 `/mnt/c`：

- `~/dev/spm`（`Tinnci/spm`）
- `~/dev/PecoFence`（`Tinnci/PecoFence`，theme-chain 分支含主题链路工作）

**不要把仓库放回 `/mnt/c`**：深信服 DLP（TsdEncryptMF 过滤器）会加密
Windows 进程写入的 .json/.md/.js/.sh 等文件，git blob 干净但工作树变密文，
导致 cargo `include_str!`/fixtures 编译失败。git 自身能透解密，所以
`git status` 一直显示干净——无法靠 git 发现。

- 若在 `/mnt/c` 上做过新 clone/checkout，立即运行：
  `wsl -d Debian -e bash /mnt/c/Users/Administrator/Tools/fix-tsd.sh`
- 每小时的看护任务（`Tools\tsd-watch.ps1`，登录自启）会自动修复。

## Windows / WSL 分工

| 操作 | 在哪做 | 说明 |
|---|---|---|
| 源码编辑、git 提交/推送 | WSL | WSL 写入 /mnt/c 或 ext4 均为明文；git 凭据：WSL gh 已登录 |
| spm 平台中立 crate 快速检查 | WSL | `cargo check -p spm-contracts -p spm-domain ...`（原生 ext4 速度） |
| Windows 目标编译/测试（spmd/widget/pecofence） | PowerShell | 源码走 `\\wsl.localhost\Debian\home\enterp\dev\...` |
| 实机视觉/桌面验收 | Windows 桌面 | HeadlessCanvas 通过 ≠ 真实字体和渲染正确 |

## Windows 构建

- git：便携版 MinGit `C:\Users\Administrator\Tools\MinGit\cmd\git.exe`
  （已加入用户 PATH；注册表持久化，新终端生效）
- 全局代理：`http.proxy = http://127.0.0.1:7890`（mihomo）。直连 443 间歇失败时先查代理
- `CARGO_TARGET_DIR = C:\Users\Administrator\cargo-target`（用户环境变量，已持久化）——
  构建产物留在 NTFS，避免每次全量重编
- spm-contracts 依赖固定在 git rev（见 PecoFence 根 Cargo.toml）；契约变更后
  手动 bump rev

## 已知坑

1. `#[tokio::test]` 默认单线程运行时：测试里用 `std::thread::sleep` 会饿死
   同运行时的 spawned 任务（v2_loopback 曾因此失败）。用 `tokio::time::sleep().await`。
2. PecoFence CI 认证私有依赖：必须 unset actions/checkout 持久化的
   `http.https://github.com/.extraheader`（GITHUB_TOKEN 无权读 Tinnci/spm），
   再用 PRIVATE_REPO_TOKEN 凭据助手。
3. Windows runner 的 run: 默认 shell 是 PowerShell：`$VAR` 不是环境变量，
   要用 `$env:VAR`。
4. 仓库内的 agent 编排脚本（`scripts/run_*.sh`）硬编码了旧 /mnt/c 路径，
   已随仓库迁移失效；如需再跑 agy/codex 编排，先更新路径。

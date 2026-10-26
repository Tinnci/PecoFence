//! Fixed interface strings. Source subjects and queue reasons are displayed verbatim.
use spm_contracts::{CiBuildState, WatchHealth};
#[derive(Clone, Copy)]
pub struct Lang {
    pub zh: bool,
}
impl Lang {
    pub fn new(locale: &str) -> Self {
        Self {
            zh: locale.starts_with("zh"),
        }
    }
    pub fn t(self, en: &'static str) -> &'static str {
        if !self.zh {
            return en;
        }
        match en {
            "CI watch" => "CI 关注",
            "Builds" => "构建",
            "Watchlist" => "关注列表",
            "My builds" => "我的构建",
            "Watched jobs" => "关注的任务",
            "Gerrit branches" => "Gerrit 分支",
            "Add" => "添加",
            "Job path or project:branch" => "任务路径或 项目:分支",
            "Only mine" => "仅我的",
            "Only my triggers" => "仅我触发",
            "Failure alert" => "失败提醒",
            "Include merged" => "含已合入",
            "Remove" => "移除",
            "Refresh" => "刷新",
            "Demo data" => "演示数据",
            "Loading CI…" => "正在加载 CI…",
            "No builds observed" => "暂无构建观测",
            "No watched jobs" => "暂无关注任务",
            "No watched branches" => "暂无关注分支",
            "No matching suggestions" => "暂无匹配建议",
            "Already watched" => "已在关注列表",
            "Local watchlist; Jenkins and Gerrit are read-only." => {
                "关注列表仅保存在本机；只读访问 Jenkins 与 Gerrit。"
            }
            "Conflict" => "⇄ 合并冲突",
            "Merged" => "已合入",
            "Abandoned" => "已放弃",
            "Needs repair" => "需修复",
            "Ready to merge" => "可合入",
            "Awaiting verification" => "等待验证",
            "CI unknown" => "CI 未知",
            "Partial results" => "部分结果",
            "Not observed" => "尚未观测",
            "SPM did not respond" => "SPM 未响应",
            "Open source unavailable" => "无法打开数据源",
            "Refresh failed" => "刷新失败",
            "selected" => "已选",
            "on" => "开",
            "off" => "关",
            _ => en,
        }
    }
    pub fn state(self, state: CiBuildState) -> &'static str {
        match (self.zh, state) {
            (true, CiBuildState::Queued) => "○ 排队中",
            (true, CiBuildState::Running) => "● 构建中",
            (true, CiBuildState::Success) => "✓ 成功",
            (true, CiBuildState::Failure) => "× 失败",
            (true, CiBuildState::Unstable) => "! 不稳定",
            (true, CiBuildState::Aborted) => "○ 中止",
            (true, CiBuildState::NotBuilt) => "— 未构建",
            (false, CiBuildState::Queued) => "○ Queued",
            (false, CiBuildState::Running) => "● Running",
            (false, CiBuildState::Success) => "✓ Success",
            (false, CiBuildState::Failure) => "× Failure",
            (false, CiBuildState::Unstable) => "! Unstable",
            (false, CiBuildState::Aborted) => "○ Aborted",
            (false, CiBuildState::NotBuilt) => "— Not built",
        }
    }
    pub fn health(self, h: WatchHealth) -> &'static str {
        match (self.zh, h) {
            (true, WatchHealth::Ok) => "读取正常",
            (true, WatchHealth::NoPermission) => "! 无读取权限",
            (true, WatchHealth::NotFound) => "! 未找到",
            (true, WatchHealth::Stale) => "! 观测过期",
            (false, WatchHealth::Ok) => "Observed",
            (false, WatchHealth::NoPermission) => "! No permission",
            (false, WatchHealth::NotFound) => "! Not found",
            (false, WatchHealth::Stale) => "! Stale",
        }
    }
    pub fn minutes(self, ms: u64) -> String {
        format!(
            "{} {}",
            ms.div_ceil(60_000),
            if self.zh { "分" } else { "min" }
        )
    }
}
pub fn eta(build: &spm_contracts::CiBuild, lang: Lang, timezone: &str) -> String {
    if build.overdue {
        return format!(
            "! {} {}",
            if lang.zh {
                "超出预计"
            } else {
                "Over estimate"
            },
            lang.minutes(
                build
                    .elapsed_ms
                    .saturating_sub(build.estimated_ms.unwrap_or(0))
            )
        );
    }
    match build.eta_at {
        Some(at) => {
            let remaining = build.remaining_ms.unwrap_or(0);
            format!(
                "{} {} · {} {}",
                if lang.zh { "约" } else { "About" },
                lang.minutes(remaining),
                crate::board::text::short_time(at, timezone),
                if lang.zh { "完成" } else { "finish" }
            )
        }
        None => lang.state(build.state).into(),
    }
}

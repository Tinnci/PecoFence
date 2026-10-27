//! Fixed interface strings. Source subjects and queue reasons are displayed verbatim.
use spm_contracts::{CiBuildState, CiJobRole, CiRatio, CiStageKind, CiStageState, WatchHealth};
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
            "CI/CD" => "CI/CD",
            "Pipeline" => "流水线",
            "Release" => "发布",
            "Watchlist" => "关注",
            "Views" => "视图",
            "Changes" => "变更",
            "Release train" => "发布列车",
            "Release train empty" => "在「关注」中把任务设为日构建或发布，即可在此看到发布列车。",
            "No changes observed" => "暂无变更观测",
            "Scheduled" => "计划",
            "Done" => "完成",
            "In progress" => "进行中",
            "Failed" => "失败",
            "Attention" => "注意",
            "Waiting" => "待定",
            "Needs fix" => "需修复",
            "Needs rebase" => "需变基",
            "Needs attention" => "需处理",
            "Shipped" => "已进版本",
            "Role" => "角色",
            "Next" => "下一个",
            "Other" => "其他",
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
            "Submit" => "提交",
            "Verify" => "验证",
            "Review" => "评审",
            "Merge" => "合入",
            "Daily" => "日构建",
            "Release stage" => "版本",
            "Loading CI…" => "正在加载 CI…",
            "No builds observed" => "暂无构建观测",
            "No watched jobs" => "暂无关注任务",
            "No watched branches" => "暂无关注分支",
            "No matching suggestions" => "暂无匹配建议",
            "Already watched" => "已在关注列表",
            "Local watchlist; Jenkins and Gerrit are read-only." => {
                "关注列表仅保存在本机；只读访问 Jenkins 与 Gerrit。"
            }
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
    pub fn stage(self, kind: CiStageKind) -> &'static str {
        self.t(match kind {
            CiStageKind::Submit => "Submit",
            CiStageKind::Verify => "Verify",
            CiStageKind::Review => "Review",
            CiStageKind::Merge => "Merge",
            CiStageKind::DailyBuild => "Daily",
            CiStageKind::Release => "Release stage",
        })
    }
    pub fn stage_state(self, state: CiStageState) -> &'static str {
        self.t(match state {
            CiStageState::Done => "Done",
            CiStageState::Running => "In progress",
            CiStageState::Failed => "Failed",
            CiStageState::Warn => "Attention",
            CiStageState::Pending => "Waiting",
        })
    }
    pub fn role(self, role: CiJobRole) -> &'static str {
        self.t(match role {
            CiJobRole::Verify => "Verify",
            CiJobRole::Daily => "Daily",
            CiJobRole::Release => "Release",
            CiJobRole::Other => "Other",
        })
    }
    /// Stage text for a running or waiting headline, such as `● 验证中` or `○ 待验证`.
    pub fn running_word(self, stage: &str) -> String {
        if self.zh {
            format!("● {stage}中")
        } else {
            format!("● {stage} running")
        }
    }
    pub fn waiting_word(self, stage: &str) -> String {
        if self.zh {
            format!("○ 待{stage}")
        } else {
            format!("○ Waiting for {stage}")
        }
    }
    pub fn recent(self, ratio: CiRatio) -> String {
        if self.zh {
            format!("{}/{} 次成功", ratio.passed, ratio.total)
        } else {
            format!("{}/{} passed", ratio.passed, ratio.total)
        }
    }
    pub fn pending(self, merges: u32, mine: u32) -> String {
        if self.zh {
            format!("待进入 {merges} 个合入（你的 {mine} 个）")
        } else {
            format!("{merges} merges pending ({mine} mine)")
        }
    }
    pub fn median(self, ms: u64) -> String {
        if self.zh {
            format!("中位 {}", self.minutes(ms))
        } else {
            format!("median {}", self.minutes(ms))
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
                crate::board::text::short_time(at, timezone)
                    .split_once(' ')
                    .map_or_else(|| "?".into(), |(_, time)| time.to_owned()),
                if lang.zh { "完成" } else { "finish" }
            )
        }
        None => lang.state(build.state).into(),
    }
}

pub fn freshness(
    at: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
    lang: Lang,
) -> String {
    let seconds = (now - at).num_seconds().max(0) as u64;
    let (value, zh, en) = if seconds < 60 {
        (seconds, "秒", "s")
    } else if seconds < 3600 {
        (seconds / 60, "分钟", "min")
    } else if seconds < 86400 {
        (seconds / 3600, "小时", "h")
    } else {
        (seconds / 86400, "天", "d")
    };
    if lang.zh {
        format!("{value} {zh}前")
    } else {
        format!("{value} {en} ago")
    }
}

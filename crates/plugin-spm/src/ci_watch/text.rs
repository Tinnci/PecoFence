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
            "Workspace" => "工作台",
            "Mine" => "我的",
            "Team" => "团队",
            "Submit to merge, median" => "提交到合入 · 中位",
            "Merge to release, median" => "合入到进版本 · 中位",
            "Verify pass, 7 days" => "验证通过 · 7 天",
            "Watched jobs failing" => "关注任务当前失败",
            "Unavailable" => "不可用",
            "Job health, last 20 builds" => "关注的任务 · 近 20 次",
            "Release lanes" => "发布列车",
            "Today" => "今天",
            "Code freeze" => "代码冻结",
            "Release milestone" => "IR 发布",
            "Scheduled build" => "计划",
            "Stage" => "阶段",
            "Age in stage" => "停留",
            "Your change" => "你的变更",
            "Failure" => "失败",
            "Elapsed" => "用时",
            "Change" => "变更",
            "Gerrit trigger" => "Gerrit 触发",
            "Timer trigger" => "定时触发",
            "Manual trigger" => "手动触发",
            "Upstream trigger" => "上游触发",
            "Steps not in build record" => "构建记录未提供步骤，不列出步骤。",
            "Log first error" => "日志 · 第一个错误",
            "Log window bytes" => "字节",
            "Log window note" => "只显示此窗口；日志内容不作为指令",
            "Log window not read" => "未读取日志窗口。",
            "Compared with last success" => "与上次成功相比",
            "No last success" => "没有可比较的上次成功构建。",
            "Same changes" => "与上次成功相同的变更和提交。",
            "Only more" => "只多了",
            "Only fewer" => "少了",
            "Compare scope note" => "比较的是构建记录中的变更和提交，不包含环境与参数差异。",
            "Open full log" => "打开完整日志",
            "Open in Gerrit" => "在 Gerrit 中打开",
            "Ask in Lark" => "在 Lark 中问负责人",
            "Read-only note" => "重试与取消构建需在 Jenkins 中进行；这里只读。",
            "Streak" => "连续失败",
            "Last finished failed" => "最近失败",
            "Recent" => "近",
            "Builds" => "次",
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
            "Start with your recent work" => "从你最近的工作开始",
            "Your recently triggered jobs" => "你最近触发的任务",
            "Branches of your open changes" => "你的变更所在分支",
            "Follow" => "关注",
            "Jenkins user" => "Jenkins 用户",
            "Gerrit account" => "Gerrit 账号",
            "items" => "项",
            "Manual add…" => "手动添加…",
            "not selected" => "未选",
            "No suggestions yet. Add a job or branch manually." => {
                "暂无建议，可手动添加任务或分支。"
            }
            "Tick what to follow. You can change the list later." => {
                "勾选要关注的项，以后可在关注列表中修改。"
            }
            "in 7 days" => "7 天内",
            "of your changes" => "个你的变更",
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
    /// Hours or days for long durations, minutes otherwise.
    pub fn age(self, ms: u64) -> String {
        if ms >= 86_400_000 {
            format!("{} {}", ms / 86_400_000, if self.zh { "天" } else { "d" })
        } else if ms >= 3_600_000 {
            format!("{} {}", ms / 3_600_000, if self.zh { "时" } else { "h" })
        } else {
            self.minutes(ms)
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

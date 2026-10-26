//! Interface strings, time formatting and text fitting for the board.
//!
//! Interface text follows short, direct instructions (ASD-STE100 style): one
//! instruction per sentence, active voice, no idioms. Source text (titles, next
//! actions, evidence summaries) comes from the backend and is shown as written.

use chrono::{DateTime, Duration, FixedOffset, Utc};
use pecofence_plugin_api::{FontRole, TextAlign, TextSpec};
use std::cell::RefCell;
use std::collections::HashMap;

/// Interface language. Chinese is used when the host locale is Chinese.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lang {
    pub zh: bool,
}

impl Lang {
    pub fn for_locale(locale: &str) -> Self {
        Self {
            zh: locale.to_ascii_lowercase().starts_with("zh"),
        }
    }

    /// Translate a fixed English interface string.
    pub fn t(self, en: &'static str) -> &'static str {
        if !self.zh {
            return en;
        }
        ZH.iter()
            .find(|(key, _)| *key == en)
            .map_or(en, |(_, zh)| zh)
    }

    /// Translate a template and substitute `{}` placeholders in order.
    pub fn f(self, en: &'static str, values: &[&str]) -> String {
        let mut text = self.t(en).to_owned();
        for value in values {
            if let Some(at) = text.find("{}") {
                text.replace_range(at..at + 2, value);
            }
        }
        text
    }
}

const ZH: &[(&str, &str)] = &[
    ("Board", "看板"),
    ("Week", "周"),
    ("Month", "月"),
    ("Quarter", "季"),
    ("Today", "今天"),
    ("Expected", "预计"),
    ("Open evidence", "查看证据"),
    ("Show on board", "在看板中显示"),
    ("Back to timeline", "返回时间线"),
    ("Days to due", "距到期"),
    ("days", "天"),
    ("No next action", "暂无下一步"),
    ("Evidence unavailable", "证据不可用"),
    ("Loading evidence…", "正在加载证据…"),
    ("Timeline", "时间线"),
    ("To do", "待开始"),
    ("Develop", "开发"),
    ("Verify", "验证"),
    ("Done", "完成"),
    ("Unmapped", "未映射"),
    ("Refresh", "刷新"),
    ("Refreshing…", "正在刷新…"),
    ("Copy summary", "复制会议摘要"),
    ("Demo data", "演示数据"),
    ("Restart demo", "重新开始演示"),
    ("All models", "全部型号"),
    ("Needs attention", "需要关注"),
    ("Find", "查找"),
    ("Clear", "清除"),
    ("Clear filters", "清除筛选"),
    ("Unassigned", "未分配"),
    ("Blocked", "阻塞"),
    ("Test failed", "测试失败"),
    ("Build failed", "构建失败"),
    ("Owner conflict", "负责人冲突"),
    ("Unmapped node", "未映射节点"),
    ("Code ahead of workflow", "代码先于流程"),
    ("Not in Meegle", "未录入飞书项目"),
    ("Source not refreshed", "数据源未刷新"),
    ("Loading the board…", "正在加载看板…"),
    ("The board did not load.", "看板未能加载。"),
    ("Try again", "重试"),
    (
        "No requirements in this delivery scope.",
        "此交付范围内没有需求。",
    ),
    ("No cards match the filters.", "没有卡片符合筛选条件。"),
    (
        "Select a card to see its evidence.",
        "选择一张卡片以查看证据。",
    ),
    ("Back to board", "返回看板"),
    ("Close", "关闭"),
    ("Status", "状态"),
    ("Workflow", "流程"),
    ("Code", "代码"),
    ("Build", "构建"),
    ("Test", "测试"),
    ("Acceptance", "验收"),
    ("Signals", "提示"),
    ("Next action", "下一步"),
    ("People", "人员"),
    ("Evidence", "证据"),
    ("Responsible now", "当前负责"),
    ("Also assigned", "其他分配"),
    ("Earlier statement", "早先指派"),
    ("Conflicting", "冲突指派"),
    ("Developer", "开发者"),
    ("Reviewer", "评审人"),
    ("Submitter", "提交人"),
    ("Verified by", "验证人"),
    ("Accepted by", "验收人"),
    ("Code history", "代码历史"),
    ("Contact", "联系人"),
    ("Loading evidence…", "正在加载证据…"),
    ("Evidence did not load.", "证据未能加载。"),
    ("Open source", "打开来源"),
    ("Copy reference", "复制引用"),
    ("Copy ID", "复制编号"),
    ("Open in {}", "在 {} 中打开"),
    ("not started", "未开始"),
    ("running", "进行中"),
    ("passed", "通过"),
    ("failed", "失败"),
    ("unknown", "未知"),
    ("Not accepted", "未验收"),
    ("merged", "已合入"),
    ("open", "评审中"),
    ("link not confirmed", "关联未确认"),
    ("No linked change", "无关联变更"),
    ("{} of {} merged", "已合入 {}/{}"),
    ("{} open", "{} 个评审中"),
    ("Implementation", "实现"),
    ("Review", "评审"),
    ("Integration build", "集成构建"),
    ("Regression", "回归"),
    ("Not started", "未开始"),
    ("Data as of {}", "数据截至 {}"),
    ("rev {}", "版本 {}"),
    ("{} did not refresh", "{} 未刷新"),
    ("{} is {} old", "{} 已 {} 未更新"),
    ("Totals may be low", "总数可能偏低"),
    ("{} more", "另有 {} 项"),
    (
        "Refresh accepted. Waiting for new data.",
        "已接受刷新请求，等待新数据。",
    ),
    ("A refresh is already running.", "刷新已在进行中。"),
    (
        "Refresh finished. No newer data.",
        "刷新完成，没有更新的数据。",
    ),
    ("Refresh failed: {}", "刷新失败：{}"),
    ("Summary copied (rev {}).", "会议摘要已复制（版本 {}）。"),
    (
        "The summary is for an older revision. Copy it again.",
        "摘要对应旧版本，请重新复制。",
    ),
    ("Copied {}", "已复制 {}"),
    (
        "{} has no configured link. Nothing opened.",
        "{} 未配置链接，未打开任何页面。",
    ),
    ("Opened {}", "已打开 {}"),
    ("{} did not open: {}", "{} 未能打开：{}"),
    (
        "{} sent an invalid link. Nothing opened.",
        "{} 返回的链接无效，未打开任何页面。",
    ),
    ("Revision {}: {}", "版本 {}：{}"),
    ("{} moved {} → {}", "{} 从{}移到{}"),
    ("{} left the board", "{} 已不在看板中"),
    ("no card moved", "没有卡片移动"),
    (
        "Waiting for SPM. It has not sent a board yet.",
        "正在等待 SPM，尚未收到看板。",
    ),
    ("cards", "张卡片"),
    ("blocked", "阻塞"),
    ("Find cards", "查找卡片"),
    ("Number, title or person", "编号、标题或人员"),
    ("Step {} of {}", "第 {}/{} 步"),
    ("since {}", "自 {}"),
    ("by {}", "由 {}"),
    ("{} named {} in {} · {}", "{} 在{}中指派 {} · {}"),
    ("{} field · {}", "{} 字段 · {}"),
    ("authority unknown", "授权未知"),
    ("derived from code history", "由代码历史推断"),
    ("Earlier: {}", "早先：{}"),
    ("copied", "沿用"),
    ("self-review", "自评"),
    ("Evidence for this card", "此卡片的证据"),
    ("observed {}", "采集于 {}"),
    ("Times in {}", "时间为 {}"),
    ("owns change {}", "变更 {} 的所有者"),
    ("wrote {}", "编写 {}"),
    ("uploaded {}", "上传 {}"),
    ("listed on change {}", "列为变更 {} 的评审人"),
    ("merged change {} · {}", "合入变更 {} · {}"),
    ("{} · not an assignment", "{} · 不是指派"),
    ("correction", "更正"),
    ("change owner", "变更所有者"),
    ("patch set author", "补丁集作者"),
    ("patch set uploader", "补丁集上传者"),
    ("listed reviewer", "评审人"),
    ("vote", "投票"),
    ("submitter", "提交人"),
    ("assignment", "指派"),
    ("acceptance", "验收"),
    ("verification", "验证"),
    ("code history", "代码历史"),
    ("contact (not an assignment)", "联系人（不是指派）"),
    ("superseded", "已被取代"),
    ("retracted", "已撤回"),
    ("conflicting", "冲突"),
    ("Lark", "飞书"),
];

/// Fixed offsets for project time zones without daylight saving. Anything else
/// is shown in UTC and says so, instead of guessing an offset.
fn offset(timezone: &str) -> Option<FixedOffset> {
    let hours = match timezone {
        "Asia/Shanghai" | "Asia/Chongqing" | "Asia/Hong_Kong" | "Asia/Taipei"
        | "Asia/Singapore" | "Asia/Macau" => 8 * 3600,
        "Asia/Tokyo" | "Asia/Seoul" => 9 * 3600,
        "Asia/Kolkata" => 5 * 3600 + 1800,
        "UTC" | "Etc/UTC" => 0,
        _ => return None,
    };
    FixedOffset::east_opt(hours)
}

/// Short project-local time, such as `10-27 14:00`.
pub fn short_time(at: DateTime<Utc>, timezone: &str) -> String {
    match offset(timezone) {
        Some(offset) => at.with_timezone(&offset).format("%m-%d %H:%M").to_string(),
        None => format!("{} UTC", at.format("%m-%d %H:%M")),
    }
}

/// Elapsed time in the largest whole unit, such as `28 h`.
pub fn age(from: DateTime<Utc>, now: DateTime<Utc>, lang: Lang) -> String {
    let elapsed = now.signed_duration_since(from).max(Duration::zero());
    let (value, unit) = if elapsed.num_days() >= 2 {
        (elapsed.num_days(), if lang.zh { "天" } else { " d" })
    } else if elapsed.num_hours() >= 1 {
        (elapsed.num_hours(), if lang.zh { "小时" } else { " h" })
    } else {
        (elapsed.num_minutes(), if lang.zh { "分钟" } else { " min" })
    };
    format!("{value}{unit}")
}

/// Text measurement in final DIPs. The host measures with DirectWrite; layout
/// tests use [`Heuristic`].
pub trait Measure {
    fn measure(&self, text: &str, size: f32, weight: u16, font: FontRole, width: f32)
    -> (f32, f32);
}

/// Body face of the board: the paper widgets' Noto Sans SC.
pub const BODY: FontRole = FontRole::PaperBody;

/// Deterministic stand-in for DirectWrite: CJK advances one em, Latin about half.
pub struct Heuristic;

impl Heuristic {
    fn advance(c: char, size: f32) -> f32 {
        match c as u32 {
            0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0xE000..=0xF8FF => size,
            _ if c.is_ascii_uppercase() || c.is_ascii_digit() => size * 0.6,
            _ => size * 0.5,
        }
    }
}

impl Measure for Heuristic {
    fn measure(
        &self,
        text: &str,
        size: f32,
        _weight: u16,
        _font: FontRole,
        width: f32,
    ) -> (f32, f32) {
        let line = size * 1.33;
        let mut lines = 1u32;
        let mut x = 0.0f32;
        let mut widest = 0.0f32;
        // Break at spaces for Latin words and anywhere between CJK characters.
        let mut word = 0.0f32;
        for c in text.chars() {
            let a = Self::advance(c, size);
            let breakable = c == ' ' || a >= size;
            if c == '\n' {
                widest = widest.max(x + word);
                x = 0.0;
                word = 0.0;
                lines += 1;
                continue;
            }
            word += a;
            if breakable {
                if x + word > width && x > 0.0 {
                    widest = widest.max(x);
                    lines += 1;
                    x = word;
                } else {
                    x += word;
                }
                word = 0.0;
            }
        }
        if x + word > width && x > 0.0 {
            widest = widest.max(x);
            lines += 1;
            x = word;
        } else {
            x += word;
        }
        widest = widest.max(x);
        (widest.min(width), lines as f32 * line)
    }
}

/// Fits text to a line budget with an ellipsis and caches results per frame size.
pub struct Fitter<'a> {
    measure: &'a dyn Measure,
    /// Logical-to-final DIP factor (text scale).
    factor: f32,
    font: FontRole,
    cache: &'a RefCell<HashMap<FitKey, (String, u32)>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FitKey {
    text: String,
    size: u32,
    weight: u16,
    font: FontRole,
    width: u32,
    lines: u32,
}

impl<'a> Fitter<'a> {
    pub fn new(
        measure: &'a dyn Measure,
        factor: f32,
        cache: &'a RefCell<HashMap<FitKey, (String, u32)>>,
    ) -> Self {
        Self {
            measure,
            factor,
            font: BODY,
            cache,
        }
    }

    /// The same fitter measuring in another face (the serif titles).
    pub fn with_font(&self, font: FontRole) -> Self {
        Self { font, ..*self }
    }

    pub fn line_height(&self, size: f32) -> f32 {
        let (_, h) = self
            .measure
            .measure("Ag国", size * self.factor, 400, self.font, 10_000.0);
        (h / self.factor).max(size)
    }

    fn lines_of(&self, text: &str, size: f32, weight: u16, width: f32) -> u32 {
        let line = self.line_height(size) * self.factor;
        let (_, h) = self.measure.measure(
            text,
            size * self.factor,
            weight,
            self.font,
            width * self.factor,
        );
        ((h / line).round() as u32).max(1)
    }

    /// Single-line width in logical DIPs.
    pub fn width(&self, text: &str, size: f32, weight: u16) -> f32 {
        let (w, _) = self
            .measure
            .measure(text, size * self.factor, weight, self.font, 10_000.0);
        w / self.factor
    }

    /// The longest prefix (plus `…`) that wraps into `max_lines` at `width`, and
    /// the number of lines it takes.
    pub fn fit(
        &self,
        text: &str,
        size: f32,
        weight: u16,
        width: f32,
        max_lines: u32,
    ) -> (String, u32) {
        let key = FitKey {
            text: text.to_owned(),
            size: (size * self.factor * 10.0) as u32,
            weight,
            font: self.font,
            width: (width * self.factor * 10.0).max(0.0) as u32,
            lines: max_lines,
        };
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let result = self.fit_uncached(text, size, weight, width.max(1.0), max_lines.max(1));
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key, result.clone());
        result
    }

    fn fit_uncached(
        &self,
        text: &str,
        size: f32,
        weight: u16,
        width: f32,
        max_lines: u32,
    ) -> (String, u32) {
        // Line breaks inside source text would defeat the line budget.
        let text = text.replace(['\n', '\r'], " ");
        let lines = self.lines_of(&text, size, weight, width);
        if lines <= max_lines {
            return (text, lines);
        }
        let chars: Vec<char> = text.chars().collect();
        let (mut lo, mut hi) = (0usize, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let candidate: String = chars[..mid].iter().collect::<String>() + "…";
            if self.lines_of(&candidate, size, weight, width) <= max_lines {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let fitted = chars[..lo].iter().collect::<String>().trim_end().to_owned() + "…";
        let lines = self.lines_of(&fitted, size, weight, width).min(max_lines);
        (fitted, lines)
    }
}

/// Host measurement through the render service.
pub struct HostMeasure<'a>(pub &'a dyn Fn(&TextSpec, f32) -> Option<(f32, f32)>);

impl Measure for HostMeasure<'_> {
    fn measure(
        &self,
        text: &str,
        size: f32,
        weight: u16,
        font: FontRole,
        width: f32,
    ) -> (f32, f32) {
        let spec = TextSpec {
            text: text.to_owned(),
            size_dip: size,
            weight,
            font,
            align: TextAlign::TopLeading,
        };
        (self.0)(&spec, width).unwrap_or_else(|| Heuristic.measure(text, size, weight, font, width))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_time_uses_the_fixed_offset_and_names_unknown_zones() {
        let at: DateTime<Utc> = "2026-10-27T06:00:00Z".parse().unwrap();
        assert_eq!(short_time(at, "Asia/Shanghai"), "10-27 14:00");
        assert_eq!(short_time(at, "Europe/Berlin"), "10-27 06:00 UTC");
    }

    #[test]
    fn fitting_keeps_short_text_and_clamps_long_text_with_an_ellipsis() {
        let cache = RefCell::default();
        let fitter = Fitter::new(&Heuristic, 1.0, &cache);
        assert_eq!(
            fitter.fit("Rename Home", 14.0, 400, 200.0, 2),
            ("Rename Home".into(), 1)
        );
        let long = "桌面搜索框在横屏模式与深色主题切换后保持位置一致，并同步到副屏".repeat(2);
        let (fitted, lines) = fitter.fit(&long, 14.0, 400, 200.0, 2);
        assert_eq!(lines, 2);
        assert!(fitted.ends_with('…'));
        assert!(fitted.chars().count() < long.chars().count());
        // Scaling the text does not change the logical line budget.
        let big = Fitter::new(&Heuristic, 2.0, &cache);
        assert_eq!(big.fit(&long, 14.0, 400, 200.0, 2).1, 2);
    }

    #[test]
    fn chinese_strings_cover_templates() {
        let zh = Lang { zh: true };
        assert_eq!(zh.f("{} of {} merged", &["1", "2"]), "已合入 1/2");
        assert_eq!(Lang { zh: false }.t("Done"), "Done");
    }
}

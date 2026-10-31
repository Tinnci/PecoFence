//! Data age and source health derived from a snapshot and an injected clock.
//!
//! A live daemon republishes at least every half TTL even when nothing changed, so a
//! snapshot older than its TTL means collection stopped or the daemon serves retained
//! (historical) data. Views evaluate this on every frame; there is no host timer yet, so a
//! collector that stalls without disconnecting is shown at the next repaint.
use chrono::{DateTime, Utc};
use spm_contracts::{CoverageState, ProjectSnapshot, SourceSystem};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Freshness {
    pub age_secs: i64,
    pub stale: bool,
    /// Sources reporting an error, incomplete coverage or their own expired TTL.
    pub degraded: Vec<SourceSystem>,
}

impl Freshness {
    pub fn healthy(&self) -> bool {
        !self.stale && self.degraded.is_empty()
    }
}

pub fn assess(snapshot: &ProjectSnapshot, now: DateTime<Utc>) -> Freshness {
    let age_secs = (now - snapshot.computed_at).num_seconds().max(0);
    let mut degraded = Vec::new();
    for source in &snapshot.sources {
        let source_age = (now - source.observed_at).num_seconds().max(0);
        let unhealthy = source.error.is_some()
            || source.coverage != CoverageState::Complete
            || source_age > i64::from(source.freshness_ttl_secs);
        if unhealthy && !degraded.contains(&source.source) {
            degraded.push(source.source);
        }
    }
    Freshness {
        age_secs,
        stale: age_secs > i64::from(snapshot.freshness_ttl_secs),
        degraded,
    }
}

pub fn age_label(secs: i64) -> String {
    match secs {
        ..60 => "不到 1 分钟".into(),
        60..3600 => format!("{} 分钟", secs / 60),
        3600..86_400 => format!("{} 小时", secs / 3600),
        _ => format!("{} 天", secs / 86_400),
    }
}

pub fn source_label(source: SourceSystem) -> &'static str {
    match source {
        SourceSystem::Jira => "Jira",
        SourceSystem::Meegle => "Meegle",
        SourceSystem::Gerrit => "Gerrit",
        SourceSystem::Jenkins => "Jenkins",
        SourceSystem::Lark => "Lark",
        SourceSystem::People => "People",
    }
}

/// Workspace status line; unchanged wording while the data is fresh and healthy.
pub fn status_text(snapshot: &ProjectSnapshot, freshness: &Freshness) -> String {
    let time = snapshot.computed_at.to_rfc3339();
    let mut text = if freshness.stale {
        format!(
            "数据已过期：{}前更新（{time}）",
            age_label(freshness.age_secs)
        )
    } else {
        format!("数据时间 {time}")
    };
    if !freshness.degraded.is_empty() {
        let names: Vec<_> = freshness
            .degraded
            .iter()
            .map(|s| source_label(*s))
            .collect();
        text.push_str(&format!(" · 来源异常：{}", names.join("、")));
    }
    text
}

//! Presentation policy shared by the CI/CD entry points. SPM owns observation, ETA and
//! attribution; this module only decides what an entry shows and what it focuses.
use serde::{Deserialize, Serialize};
use spm_contracts::*;

/// A request from an entry point (tray, capsule, board drawer, toast) to the CI/CD fence.
/// `edit` is applied to the watchlist first; `focus` names the row to reveal.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OpenIntent {
    #[serde(default)]
    pub focus: Option<String>,
    #[serde(default)]
    pub edit: Option<UpdateCiWatchlistRequest>,
}

/// The focus token for one build: `build:<job>:<number>`, or `build:<job>:queued`.
pub fn build_key(build: &CiBuild) -> String {
    match build.number {
        Some(number) => format!("build:{}:{number}", build.job),
        None => format!("build:{}:queued", build.job),
    }
}

/// The focus token for a Gerrit change: `change:<number>`.
pub fn change_key(number: u64) -> String {
    format!("change:{number}")
}

/// Whether a row key recorded by the layout answers a focus token. Change rows end in their
/// change number (branch ids contain colons, so only the trailing segment is compared); build
/// focus reveals the release-train row of its job.
pub fn focus_matches(row: &str, target: &str) -> bool {
    if let Some(number) = target.strip_prefix("change:") {
        return row.starts_with("change:")
            && row.rsplit_once(':').is_some_and(|(_, n)| n == number);
    }
    if let Some(rest) = target.strip_prefix("build:") {
        let job = rest.rsplit_once(':').map_or(rest, |(job, _)| job);
        return row
            .strip_prefix("train:")
            .and_then(|r| r.rsplit_once(':'))
            .is_some_and(|(j, _)| j == job);
    }
    false
}

/// Running and failed counts for the tray, the capsule and the accessible names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub running: usize,
    pub failures: usize,
    /// The shortest backend-projected remaining time among running builds.
    pub shortest_remaining_ms: Option<u64>,
    /// The build an entry should focus: the running build with the shortest remaining time, else
    /// the first failure.
    pub focus: Option<String>,
}

impl Summary {
    pub fn from_snapshot(snapshot: &CiWatchSnapshot) -> Self {
        // A build can appear in several places (job last, my builds, change embed); count it once.
        let mut builds = std::collections::BTreeMap::new();
        for build in snapshot
            .my_builds
            .iter()
            .chain(snapshot.jobs.iter().filter_map(|j| j.last.as_ref()))
            .chain(
                snapshot
                    .branches
                    .iter()
                    .flat_map(|b| b.changes.iter().filter_map(|c| c.ci.as_ref())),
            )
        {
            builds.insert((build.job.as_str(), build.number), build);
        }
        let mut summary = Self::default();
        let mut focus_remaining: Option<u64> = None;
        for build in builds.values() {
            match build.state {
                CiBuildState::Running => {
                    summary.running += 1;
                    if let Some(ms) = build.remaining_ms {
                        summary.shortest_remaining_ms =
                            Some(summary.shortest_remaining_ms.map_or(ms, |old| old.min(ms)));
                        if focus_remaining.is_none_or(|old| ms < old) {
                            focus_remaining = Some(ms);
                            summary.focus = Some(build_key(build));
                        }
                    }
                    if summary.focus.is_none() {
                        summary.focus = Some(build_key(build));
                    }
                }
                CiBuildState::Failure => summary.failures += 1,
                _ => {}
            }
        }
        if summary.focus.is_none() {
            summary.focus = builds
                .values()
                .find(|b| b.state == CiBuildState::Failure)
                .map(|b| build_key(b));
        }
        summary
    }

    /// The capsule segment shows only while something runs or has failed.
    pub fn visible(&self) -> bool {
        self.running > 0 || self.failures > 0
    }

    /// Compact text: running count, shortest ETA, failure count. Glyphs and numbers carry the
    /// status, so colour is never the only signal.
    pub fn label(&self, zh: bool) -> String {
        let mut parts = vec![format!("● {}", self.running)];
        if let Some(ms) = self.shortest_remaining_ms {
            let minutes = ms.div_ceil(60_000);
            parts.push(if zh {
                format!("约{minutes}分")
            } else {
                format!("~{minutes} min")
            });
        }
        parts.push(format!("× {}", self.failures));
        parts.join(" · ")
    }

    /// Full accessible name for the capsule segment.
    pub fn accessible_name(&self, zh: bool) -> String {
        let eta = self.shortest_remaining_ms.map(|ms| ms.div_ceil(60_000));
        if zh {
            format!(
                "CI/CD：{} 个构建中，{}；{} 个失败",
                self.running,
                eta.map_or_else(|| "完成时间未知".into(), |m| format!("约 {m} 分钟完成")),
                self.failures
            )
        } else {
            format!(
                "CI/CD: {} running, {}; {} failed",
                self.running,
                eta.map_or_else(
                    || "ETA unknown".into(),
                    |m| format!("about {m} minutes remaining")
                ),
                self.failures
            )
        }
    }
}

/// Which failures should notify. A build notifies once, when a watched job's latest build
/// becomes a failure after an earlier observation. The first snapshot is a baseline: failures
/// that were already there when PecoFence started do not notify, and neither does a job the
/// watch has only just started following while it is already failing.
#[derive(Default)]
pub struct FailureTransitions {
    /// The latest (build number, state) seen per watched job.
    previous: std::collections::BTreeMap<String, (Option<u64>, CiBuildState)>,
    /// Failures already decided, so a build is never announced twice.
    decided: std::collections::BTreeSet<(String, u64)>,
    initialized: bool,
}

impl FailureTransitions {
    /// The builds to announce for this snapshot. `enabled` is the global switch: failures are
    /// still recorded as decided while it is off, so turning it back on does not replay them.
    pub fn observe(&mut self, snapshot: &CiWatchSnapshot, enabled: bool) -> Vec<CiBuild> {
        let mut announce = Vec::new();
        for job in &snapshot.jobs {
            let Some(build) = &job.last else { continue };
            let before = self
                .previous
                .insert(job.job.clone(), (build.number, build.state));
            if build.state != CiBuildState::Failure {
                continue;
            }
            let Some(number) = build.number else { continue };
            let first = self.decided.insert((job.job.clone(), number));
            let transitioned =
                before.is_some_and(|old| old != (build.number, CiBuildState::Failure));
            if first && self.initialized && enabled && job.notify_on_failure && transitioned {
                announce.push(build.clone());
            }
        }
        self.initialized = true;
        announce
    }
}

/// Demo seeds: the same shape SPM sends in live mode, for the deterministic capture.
pub fn demo_seeds() -> CiSuggestions {
    let job = |value: &str, count: u32| CiSuggestion {
        kind: CiSuggestionKind::Job,
        value: value.into(),
        label: value.into(),
        already_watched: false,
        count: Some(count),
    };
    let branch = |value: &str, count: u32| CiSuggestion {
        kind: CiSuggestionKind::Branch,
        value: value.into(),
        label: value.into(),
        already_watched: false,
        count: Some(count),
    };
    CiSuggestions {
        items: vec![
            job("orion/launcher-gerrit-verify", 6),
            job("orion/launcher-daily-userdebug", 3),
            job("orion/carmode-lc3-smoke", 1),
            branch("orion/launcher:master", 2),
            branch("orion/launcher:release/ir-1113", 1),
        ],
        truncated: false,
    }
}

/// The watchlist edit a suggestion stands for, used when an entry adds the item it was given.
pub fn suggestion_edit(s: &CiSuggestion) -> Result<UpdateCiWatchlistRequest, &'static str> {
    match s.kind {
        CiSuggestionKind::Job => Ok(UpdateCiWatchlistRequest::AddJob {
            job: s.value.clone(),
        }),
        CiSuggestionKind::Branch => {
            let (project, branch) = s.value.split_once(':').ok_or("missing project:branch")?;
            Ok(UpdateCiWatchlistRequest::AddBranch {
                project: project.into(),
                branch: branch.into(),
            })
        }
    }
}

/// The focus token for a board source: its change (Gerrit) or its build (Jenkins).
pub fn source_focus(source: &SourceRef) -> Option<String> {
    match source.system {
        SourceSystem::Gerrit => source
            .record_id
            .split('/')
            .next()
            .and_then(|n| n.parse::<u64>().ok())
            .map(change_key),
        SourceSystem::Jenkins => {
            let (job, number) = source.record_id.rsplit_once('/')?;
            Some(format!("build:{job}:{number}"))
        }
        _ => None,
    }
}

/// The suggestion a board source resolves to, from a snapshot alone. Used by the demo backend;
/// the live path asks SPM, which also reads Gerrit for the branch.
pub fn suggestion_for_source(
    snapshot: &CiWatchSnapshot,
    source: &SourceRef,
) -> Option<CiSuggestion> {
    match source.system {
        SourceSystem::Jenkins => {
            let (job, _) = source.record_id.rsplit_once('/')?;
            validate_ci_job(job).ok()?;
            Some(CiSuggestion {
                kind: CiSuggestionKind::Job,
                value: job.into(),
                label: job.into(),
                already_watched: snapshot.jobs.iter().any(|j| j.job == job),
                count: None,
            })
        }
        SourceSystem::Gerrit => {
            let number: u64 = source.record_id.split('/').next()?.parse().ok()?;
            let (project, branch) = snapshot.branches.iter().find_map(|b| {
                b.changes
                    .iter()
                    .any(|c| c.number == number)
                    .then(|| (b.project.clone(), b.branch.clone()))
            })?;
            Some(CiSuggestion {
                kind: CiSuggestionKind::Branch,
                value: format!("{project}:{branch}"),
                label: format!("{project} : {branch}"),
                already_watched: snapshot
                    .branches
                    .iter()
                    .any(|b| b.project == project && b.branch == branch),
                count: None,
            })
        }
        _ => None,
    }
}

/// Whether a board source row is already followed, so the drawer can show 已关注 ✓.
pub fn source_watched(snapshot: &CiWatchSnapshot, source: &SourceRef) -> bool {
    match source.system {
        SourceSystem::Jenkins => snapshot.jobs.iter().any(|j| {
            source.record_id == j.job
                || source
                    .record_id
                    .strip_prefix(&j.job)
                    .is_some_and(|rest| rest.starts_with('#') || rest.starts_with('/'))
        }),
        SourceSystem::Gerrit => source
            .record_id
            .split('/')
            .next()
            .and_then(|n| n.parse::<u64>().ok())
            .is_some_and(|n| {
                snapshot
                    .branches
                    .iter()
                    .any(|b| b.project == source.project && b.changes.iter().any(|c| c.number == n))
            }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ci_watch::demo_snapshot;

    fn build(
        job: &str,
        number: Option<u64>,
        state: CiBuildState,
        remaining: Option<u64>,
    ) -> CiBuild {
        CiBuild {
            job: job.into(),
            number,
            url_origin: "jenkins".into(),
            path: String::new(),
            state,
            started_at: None,
            elapsed_ms: 0,
            estimated_ms: None,
            eta_at: None,
            remaining_ms: remaining,
            overdue: false,
            queue_reason: None,
            node: None,
            trigger: CiTrigger::Other,
            commit_ids: vec![],
        }
    }

    #[test]
    fn capsule_visibility_and_accessible_counts() {
        let summary = Summary::from_snapshot(&demo_snapshot());
        assert!(summary.visible());
        assert!(summary.running > 0 && summary.failures > 0);
        assert!(
            summary.label(false).contains("×"),
            "counts are text, not colour"
        );
        assert!(summary.accessible_name(true).contains("个失败"));
        assert!(!Summary::from_snapshot(&CiWatchSnapshot::default()).visible());
    }

    #[test]
    fn focus_prefers_the_running_build_with_least_time_left() {
        let snapshot = CiWatchSnapshot {
            my_builds: vec![
                build("a", Some(1), CiBuildState::Running, Some(600_000)),
                build("b", Some(7), CiBuildState::Running, Some(120_000)),
                build("c", Some(9), CiBuildState::Failure, None),
            ],
            ..Default::default()
        };
        let summary = Summary::from_snapshot(&snapshot);
        assert_eq!((summary.running, summary.failures), (2, 1));
        assert_eq!(summary.shortest_remaining_ms, Some(120_000));
        assert_eq!(summary.focus.as_deref(), Some("build:b:7"));
    }

    #[test]
    fn focus_falls_back_to_the_first_failure_when_nothing_runs() {
        let snapshot = CiWatchSnapshot {
            my_builds: vec![build("x", Some(4), CiBuildState::Failure, None)],
            ..Default::default()
        };
        assert_eq!(
            Summary::from_snapshot(&snapshot).focus.as_deref(),
            Some("build:x:4")
        );
    }

    #[test]
    fn a_build_seen_in_several_places_is_counted_once() {
        let b = build("dup", Some(3), CiBuildState::Running, Some(1));
        let snapshot = CiWatchSnapshot {
            my_builds: vec![b.clone()],
            jobs: vec![WatchedJob {
                id: "j".into(),
                job: "dup".into(),
                role: CiJobRole::Other,
                only_mine: false,
                notify_on_failure: false,
                last: Some(b),
                history: vec![],
                median_ms: None,
                health: WatchHealth::Ok,
            }],
            ..Default::default()
        };
        assert_eq!(Summary::from_snapshot(&snapshot).running, 1);
    }

    #[test]
    fn focus_tokens_match_the_rows_they_name() {
        assert!(focus_matches(
            "change:branch:p:master:18234",
            "change:18234"
        ));
        assert!(!focus_matches(
            "change:branch:p:master:18235",
            "change:18234"
        ));
        assert!(!focus_matches("train:orion/x:0", "change:18234"));
        assert!(focus_matches(
            "train:orion/launcher-gerrit-verify:1",
            "build:orion/launcher-gerrit-verify:913"
        ));
        assert!(!focus_matches(
            "train:orion/launcher:0",
            "build:orion/launcher-gerrit-verify:913"
        ));
    }

    fn failing_watch(number: u64, state: CiBuildState) -> CiWatchSnapshot {
        CiWatchSnapshot {
            jobs: vec![WatchedJob {
                id: "job:orion/a".into(),
                job: "orion/a".into(),
                role: CiJobRole::Other,
                only_mine: false,
                notify_on_failure: true,
                last: Some(CiBuild {
                    number: Some(number),
                    state,
                    ..build("orion/a", Some(number), state, None)
                }),
                history: vec![],
                median_ms: None,
                health: WatchHealth::Ok,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn startup_failures_are_a_baseline_not_notices() {
        let mut t = FailureTransitions::default();
        assert!(
            t.observe(&failing_watch(900, CiBuildState::Failure), true)
                .is_empty()
        );
        // The same failure on the next poll is still not new.
        assert!(
            t.observe(&failing_watch(900, CiBuildState::Failure), true)
                .is_empty()
        );
    }

    #[test]
    fn a_transition_to_failure_notifies_once_per_build() {
        let mut t = FailureTransitions::default();
        t.observe(&failing_watch(912, CiBuildState::Success), true);
        t.observe(&failing_watch(913, CiBuildState::Running), true);
        let announced = t.observe(&failing_watch(913, CiBuildState::Failure), true);
        assert_eq!(announced.len(), 1);
        assert_eq!(announced[0].number, Some(913));
        assert!(
            t.observe(&failing_watch(913, CiBuildState::Failure), true)
                .is_empty()
        );
        assert!(
            t.observe(&failing_watch(913, CiBuildState::Failure), true)
                .is_empty()
        );
    }

    #[test]
    fn the_global_switch_and_the_per_job_flag_both_gate_notices() {
        let mut t = FailureTransitions::default();
        t.observe(&failing_watch(1, CiBuildState::Success), true);
        assert!(
            t.observe(&failing_watch(2, CiBuildState::Failure), false)
                .is_empty()
        );
        // Turning the switch back on does not replay the failure that happened while it was off.
        t.observe(&failing_watch(3, CiBuildState::Success), true);
        assert!(
            t.observe(&failing_watch(2, CiBuildState::Failure), true)
                .is_empty()
        );

        let mut quiet = failing_watch(4, CiBuildState::Failure);
        quiet.jobs[0].notify_on_failure = false;
        t.observe(&failing_watch(5, CiBuildState::Success), true);
        assert!(t.observe(&quiet, true).is_empty());
    }

    #[test]
    fn a_job_first_seen_while_failing_does_not_notify() {
        let mut t = FailureTransitions::default();
        t.observe(&CiWatchSnapshot::default(), true);
        assert!(
            t.observe(&failing_watch(7, CiBuildState::Failure), true)
                .is_empty()
        );
    }

    #[test]
    fn suggestion_edits_follow_their_kind() {
        let job = CiSuggestion {
            kind: CiSuggestionKind::Job,
            value: "orion/a".into(),
            label: String::new(),
            already_watched: false,
            count: None,
        };
        assert_eq!(
            suggestion_edit(&job),
            Ok(UpdateCiWatchlistRequest::AddJob {
                job: "orion/a".into()
            })
        );
        let branch = CiSuggestion {
            kind: CiSuggestionKind::Branch,
            value: "orion/launcher:release/ir-1113".into(),
            label: String::new(),
            already_watched: false,
            count: None,
        };
        assert_eq!(
            suggestion_edit(&branch),
            Ok(UpdateCiWatchlistRequest::AddBranch {
                project: "orion/launcher".into(),
                branch: "release/ir-1113".into(),
            })
        );
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;

    fn source(system: SourceSystem, record_id: &str) -> SourceRef {
        SourceRef {
            system,
            tenant: "t".into(),
            project: "p".into(),
            record_kind: "k".into(),
            record_id: record_id.into(),
        }
    }

    #[test]
    fn board_sources_focus_their_change_or_build() {
        assert_eq!(
            source_focus(&source(SourceSystem::Gerrit, "4711/1")).as_deref(),
            Some("change:4711")
        );
        assert_eq!(
            source_focus(&source(SourceSystem::Jenkins, "launcher-orion/1182")).as_deref(),
            Some("build:launcher-orion:1182")
        );
        assert_eq!(source_focus(&source(SourceSystem::Lark, "x")), None);
    }

    #[test]
    fn a_jenkins_source_is_followed_by_its_job_and_shown_as_followed() {
        let snapshot = CiWatchSnapshot {
            jobs: vec![WatchedJob {
                id: "job:launcher-orion".into(),
                job: "launcher-orion".into(),
                role: CiJobRole::Other,
                only_mine: false,
                notify_on_failure: false,
                last: None,
                history: vec![],
                median_ms: None,
                health: WatchHealth::Ok,
            }],
            ..Default::default()
        };
        let build = source(SourceSystem::Jenkins, "launcher-orion/1182");
        assert!(source_watched(&snapshot, &build));
        let other = source(SourceSystem::Jenkins, "orion-other/7");
        assert!(!source_watched(&snapshot, &other));
    }
}

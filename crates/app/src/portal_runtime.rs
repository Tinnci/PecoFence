//! Application scheduling and snapshot commit. Deliberately window/COM-free.
//! One global running effect and one latest pending request per live source.

use pecofence_core::ContentId;
use pecofence_core::portal::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug)]
pub struct PortalSnapshot {
    pub request: PortalRead,
    pub entries: Vec<PortalEntry>,
    pub health: PortalHealth,
    pending: Option<u64>,
}

#[derive(Default)]
pub struct PortalRuntime {
    sources: BTreeMap<ContentId, PortalSnapshot>,
    running: Option<PortalRead>,
    generation: u64,
    request_id: u64,
    schedule_order: u64,
    closed: bool,
}

impl PortalRuntime {
    pub fn has_work(&self) -> bool {
        !self.closed
            && (self.running.is_some() || self.sources.values().any(|s| s.pending.is_some()))
    }

    pub fn snapshot(&self, fence: ContentId) -> Option<&PortalSnapshot> {
        self.sources.get(&fence)
    }

    pub fn remove(&mut self, fence: ContentId) {
        self.sources.remove(&fence);
    }

    /// Invalidate activations without forgetting the outstanding effect or reusing tokens.
    pub fn reset(&mut self) {
        self.sources.clear();
    }

    pub fn request(&mut self, fence: ContentId, path: PathBuf) {
        if self.closed {
            return;
        }
        self.schedule_order = self
            .schedule_order
            .checked_add(1)
            .expect("portal scheduling sequence exhausted");
        if let Some(source) = self.sources.get_mut(&fence)
            && source.request.path == path
        {
            // Same-source notifications ask for one follow-up read. They do not invalidate
            // an in-flight complete observation: a busy directory must still make progress.
            source.pending.get_or_insert(self.schedule_order);
            if !matches!(source.health, PortalHealth::Stale(_)) {
                source.health = PortalHealth::Loading;
            }
            return;
        }
        self.generation = self
            .generation
            .checked_add(1)
            .expect("portal generation exhausted");
        let request = PortalRead {
            fence,
            generation: self.generation,
            request_id: 0,
            path,
        };
        self.sources.insert(
            fence,
            PortalSnapshot {
                request,
                entries: Vec::new(),
                health: PortalHealth::Loading,
                pending: Some(self.schedule_order),
            },
        );
    }

    pub fn next_read(&mut self) -> Option<PortalRead> {
        if self.closed || self.running.is_some() {
            return None;
        }
        // First pending time wins; later notifications do not move a source to the back.
        let source = self
            .sources
            .values_mut()
            .filter(|s| s.pending.is_some())
            .min_by_key(|s| s.pending.unwrap())?;
        source.pending = None;
        if !matches!(source.health, PortalHealth::Stale(_)) {
            source.health = PortalHealth::Loading;
        }
        self.request_id = self
            .request_id
            .checked_add(1)
            .expect("portal read sequence exhausted");
        source.request.request_id = self.request_id;
        self.running = Some(source.request.clone());
        self.running.clone()
    }

    /// Correlate before committing. A completion also releases the running slot even when
    /// its activation/path/generation is no longer current. Returns the changed source.
    pub fn complete(&mut self, result: PortalResult) -> Option<ContentId> {
        if self.closed || self.running.as_ref() != Some(&result.request) {
            return None;
        }
        self.running = None;
        let source = self.sources.get_mut(&result.request.fence)?;
        if source.request != result.request {
            return None;
        }
        match result.outcome {
            PortalOutcome::Complete(entries) => {
                source.entries = entries;
                source.health = PortalHealth::Ready;
            }
            PortalOutcome::Partial { failure, .. } | PortalOutcome::Unavailable(failure) => {
                source.health = PortalHealth::Stale(failure);
            }
        }
        Some(result.request.fence)
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.sources.clear();
        self.running = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_core::FenceId;

    fn entry(name: &str) -> PortalEntry {
        PortalEntry {
            path: name.into(),
            display_name: name.into(),
            is_folder: false,
            attributes: 0,
            mtime: 0,
            size: 0,
        }
    }
    fn finish(runtime: &mut PortalRuntime, outcome: PortalOutcome) -> Option<FenceId> {
        let request = runtime.next_read().unwrap();
        runtime.complete(PortalResult { request, outcome })
    }
    #[test]
    fn complete_empty_and_incomplete_retention() {
        let mut r = PortalRuntime::default();
        let id = FenceId::new_v4();
        r.request(id, "a".into());
        finish(&mut r, PortalOutcome::Complete(vec![entry("old")]));
        for outcome in [
            PortalOutcome::Unavailable(ReadFailure::Open("offline".into())),
            PortalOutcome::Partial {
                entries: vec![entry("half")],
                failure: ReadFailure::Metadata("denied".into()),
            },
        ] {
            r.request(id, "a".into());
            finish(&mut r, outcome);
            assert_eq!(r.snapshot(id).unwrap().entries, vec![entry("old")]);
            assert!(matches!(
                r.snapshot(id).unwrap().health,
                PortalHealth::Stale(_)
            ));
        }
        r.request(id, "a".into());
        finish(&mut r, PortalOutcome::Complete(vec![]));
        assert!(r.snapshot(id).unwrap().entries.is_empty());
        assert_eq!(r.snapshot(id).unwrap().health, PortalHealth::Ready);
    }
    #[test]
    fn navigation_reset_and_recreation_reject_late_results() {
        for change in 0..3 {
            let mut r = PortalRuntime::default();
            let id = FenceId::new_v4();
            r.request(id, "a".into());
            finish(&mut r, PortalOutcome::Complete(vec![entry("old")]));
            r.request(id, "a".into());
            let request = r.next_read().unwrap();
            match change {
                0 => r.request(id, "b".into()),
                1 => {
                    r.reset();
                    r.request(id, "a".into());
                }
                _ => {
                    r.remove(id);
                    r.request(id, "a".into());
                }
            }
            assert!(r.snapshot(id).unwrap().entries.is_empty());
            assert_eq!(
                r.complete(PortalResult {
                    request,
                    outcome: PortalOutcome::Complete(vec![entry("late")])
                }),
                None
            );
            finish(&mut r, PortalOutcome::Complete(vec![entry("new")]));
            assert_eq!(r.snapshot(id).unwrap().entries, vec![entry("new")]);
        }
    }
    #[test]
    fn burst_commits_in_flight_observation_and_one_follow_up() {
        let mut r = PortalRuntime::default();
        let id = FenceId::new_v4();
        r.request(id, "a".into());
        let first = r.next_read().unwrap();
        for _ in 0..1000 {
            r.request(id, "a".into());
        }
        assert!(r.next_read().is_none());
        assert_eq!(
            r.complete(PortalResult {
                request: first,
                outcome: PortalOutcome::Complete(vec![entry("interim")])
            }),
            Some(id)
        );
        assert_eq!(r.snapshot(id).unwrap().entries, vec![entry("interim")]);
        finish(&mut r, PortalOutcome::Complete(vec![entry("final")]));
        assert!(r.next_read().is_none());
        assert_eq!(r.snapshot(id).unwrap().entries, vec![entry("final")]);
    }
    #[test]
    fn shared_path_is_independent_and_shutdown_rejects_completion() {
        let mut r = PortalRuntime::default();
        let a = FenceId::new_v4();
        let b = FenceId::new_v4();
        for id in [a, b] {
            r.request(id, "same".into());
            finish(&mut r, PortalOutcome::Complete(vec![entry("kept")]));
        }
        r.remove(a);
        assert_eq!(r.snapshot(b).unwrap().entries, vec![entry("kept")]);
        r.request(b, "same".into());
        let request = r.next_read().unwrap();
        r.close();
        assert_eq!(
            r.complete(PortalResult {
                request,
                outcome: PortalOutcome::Complete(vec![])
            }),
            None
        );
        assert!(r.next_read().is_none());
    }

    #[test]
    fn busy_source_does_not_starve_other_sources() {
        let mut r = PortalRuntime::default();
        let a = FenceId::new_v4();
        let b = FenceId::new_v4();
        r.request(a, "a".into());
        let first = r.next_read().unwrap();
        r.request(b, "b".into());
        for _ in 0..100 {
            r.request(a, "a".into());
        }
        r.complete(PortalResult {
            request: first,
            outcome: PortalOutcome::Complete(vec![]),
        });
        let next = r.next_read().unwrap();
        assert_eq!(next.fence, b);
        r.complete(PortalResult {
            request: next,
            outcome: PortalOutcome::Complete(vec![]),
        });
        assert_eq!(r.next_read().unwrap().fence, a);
    }

    #[test]
    fn duplicate_completion_cannot_release_new_running_slot() {
        let mut r = PortalRuntime::default();
        let id = FenceId::new_v4();
        r.request(id, "a".into());
        let first = r.next_read().unwrap();
        let result = PortalResult {
            request: first,
            outcome: PortalOutcome::Complete(vec![]),
        };
        assert_eq!(r.complete(result.clone()), Some(id));
        r.request(id, "a".into());
        let current = r.next_read().unwrap();
        assert_eq!(r.complete(result), None);
        assert!(r.next_read().is_none());
        assert_eq!(
            r.complete(PortalResult {
                request: current,
                outcome: PortalOutcome::Complete(vec![]),
            }),
            Some(id)
        );
    }

    #[test]
    fn continuously_changing_source_commits_each_completed_observation() {
        let mut r = PortalRuntime::default();
        let id = FenceId::new_v4();
        r.request(id, "a".into());
        let generation = r.snapshot(id).unwrap().request.generation;
        let mut last_read = 0;
        for i in 0..10 {
            let request = r.next_read().unwrap();
            assert_eq!(request.generation, generation);
            assert!(request.request_id > last_read);
            last_read = request.request_id;
            for _ in 0..100 {
                r.request(id, "a".into());
            }
            let entries = vec![entry(&i.to_string())];
            assert_eq!(
                r.complete(PortalResult {
                    request,
                    outcome: PortalOutcome::Complete(entries.clone()),
                }),
                Some(id)
            );
            assert_eq!(r.snapshot(id).unwrap().entries, entries);
        }
    }
}

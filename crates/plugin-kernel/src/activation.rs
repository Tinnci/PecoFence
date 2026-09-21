use pecofence_plugin_api::{Error, MountContext, MountKey, PanelInstance, Result, StopReason};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivationFailure {
    Create(String),
    Mount(String),
    Commit(String),
}

/// Owns a newly-created instance until create and the optional initial mount both commit.
pub struct ActivationTransaction {
    panel: Option<Box<dyn PanelInstance>>,
    initial_mount: Option<MountKey>,
    committed: bool,
}

impl ActivationTransaction {
    pub fn begin() -> Self {
        Self {
            panel: None,
            initial_mount: None,
            committed: false,
        }
    }
    pub fn create(
        &mut self,
        create: impl FnOnce() -> Result<Box<dyn PanelInstance>>,
    ) -> std::result::Result<(), ActivationFailure> {
        self.panel = Some(create().map_err(|error| ActivationFailure::Create(error.to_string()))?);
        Ok(())
    }
    pub fn attach_initial(
        &mut self,
        mount: Option<MountContext>,
    ) -> std::result::Result<(), ActivationFailure> {
        let Some(mount) = mount else {
            return Ok(());
        };
        let key = mount.key;
        self.panel
            .as_mut()
            .ok_or_else(|| ActivationFailure::Mount("panel has not been created".into()))?
            .mount(mount)
            .map_err(|error| ActivationFailure::Mount(error.to_string()))?;
        self.initial_mount = Some(key);
        Ok(())
    }
    pub fn commit(
        mut self,
    ) -> std::result::Result<(Box<dyn PanelInstance>, Option<MountKey>), ActivationFailure> {
        self.committed = true;
        Ok((
            self.panel
                .take()
                .ok_or_else(|| ActivationFailure::Commit("panel has not been created".into()))?,
            self.initial_mount.take(),
        ))
    }
    pub fn rollback(&mut self, _cause: ActivationFailure) {
        if let Some(panel) = self.panel.as_mut() {
            if let Some(mount) = self.initial_mount.take() {
                panel.unmount(mount);
            }
            panel.begin_stop(StopReason::DependencyLost);
        }
        self.panel.take();
    }
}

impl Drop for ActivationTransaction {
    fn drop(&mut self) {
        if !self.committed && self.panel.is_some() {
            self.rollback(ActivationFailure::Commit(
                "activation transaction dropped".into(),
            ));
        }
    }
}

impl From<ActivationFailure> for Error {
    fn from(value: ActivationFailure) -> Self {
        Error::Backend(format!("{value:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalScope;
    use pecofence_plugin_api::*;
    use std::{cell::Cell, rc::Rc};

    struct FailingPanel {
        stopped: Rc<Cell<u32>>,
    }
    impl PanelInstance for FailingPanel {
        fn mount(&mut self, _ctx: MountContext) -> Result<()> {
            Err(Error::Backend("mount failed".into()))
        }
        fn event(&mut self, _event: PanelEvent) -> Result<PanelUpdate> {
            Ok(PanelUpdate::default())
        }
        fn layout(&mut self, _input: LayoutInput) -> Result<LayoutSnapshot> {
            Ok(LayoutSnapshot::default())
        }
        fn paint(&self, _canvas: &mut dyn Canvas, _layout: &LayoutSnapshot) -> Result<()> {
            Ok(())
        }
        fn unmount(&mut self, _key: MountKey) {}
        fn begin_stop(&mut self, _reason: StopReason) {
            self.stopped.set(self.stopped.get() + 1);
        }
    }

    #[test]
    fn mount_failure_rolls_back_created_panel_once() {
        let stopped = Rc::new(Cell::new(0));
        let mut transaction = ActivationTransaction::begin();
        transaction
            .create(|| {
                Ok(Box::new(FailingPanel {
                    stopped: stopped.clone(),
                }))
            })
            .unwrap();
        let local = LocalScope::new(ScopeId(7), 1);
        let key = InstanceKey {
            id: 1,
            activation: 1,
        };
        let failure = transaction
            .attach_initial(Some(MountContext {
                key: MountKey {
                    instance: key,
                    generation: 1,
                },
                scope: LocalScope::handle(&local),
                viewport: RectDip::default(),
                dpi: 96,
            }))
            .unwrap_err();
        transaction.rollback(failure);
        assert_eq!(stopped.get(), 1);
    }
}

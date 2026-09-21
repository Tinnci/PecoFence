use pecofence_plugin_api::{
    Error, InstanceKey, MountKey, PanelInstance, PanelProvider, StopReason,
};
use std::collections::HashMap;
use std::rc::Rc;

pub struct InstanceRecord {
    pub key: InstanceKey,
    pub provider_id: String,
    pub mount: Option<MountKey>,
    pub panel: Box<dyn PanelInstance>,
}

#[derive(Default)]
pub struct PanelManager {
    providers: HashMap<String, Rc<dyn PanelProvider>>,
    instances: HashMap<u128, InstanceRecord>,
}

impl PanelManager {
    pub fn register_provider(&mut self, provider: Rc<dyn PanelProvider>) -> Result<(), Error> {
        let id = provider.descriptor().id.to_string();
        if self.providers.contains_key(&id) {
            return Err(Error::Duplicate);
        }
        self.providers.insert(id, provider);
        Ok(())
    }

    pub fn provider(&self, id: &str) -> Option<Rc<dyn PanelProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn insert(&mut self, record: InstanceRecord) -> Result<(), Error> {
        if self.instances.contains_key(&record.key.id) {
            return Err(Error::Duplicate);
        }
        self.instances.insert(record.key.id, record);
        Ok(())
    }

    pub fn instance_mut(&mut self, id: u128) -> Option<&mut InstanceRecord> {
        self.instances.get_mut(&id)
    }

    pub fn remove(&mut self, id: u128, reason: StopReason) -> Option<InstanceRecord> {
        let mut record = self.instances.remove(&id)?;
        if let Some(mount) = record.mount.take() {
            record.panel.unmount(mount);
        }
        record.panel.begin_stop(reason);
        Some(record)
    }
}

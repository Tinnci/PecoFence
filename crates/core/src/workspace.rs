//! Invariant-checked ownership transitions. Runtime content/source lifetimes are deliberately
//! absent: moving a tab changes references, never the content business instance.

use crate::model::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    InvalidLayout(String),
    MissingContainer(ContainerId),
    MissingContent(ContentId),
    NotOwned {
        container: ContainerId,
        content: ContentId,
    },
    DuplicateIdentity,
    InvalidIndex {
        index: usize,
        len: usize,
    },
    InboxRequired,
    BudgetExceeded,
    StaleDetach,
}
impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WorkspaceError {}

/// The composition root reconciles windows from these outcomes. No source is revoked by a
/// move. Only `deleted_content` authorizes business-instance/source teardown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transition {
    pub created_containers: Vec<ContainerId>,
    pub removed_containers: Vec<ContainerId>,
    pub moved_content: Option<ContentId>,
    pub deleted_content: Option<ContentId>,
}

/// A narrow structural inverse, not a workspace snapshot. Source metadata is retained only
/// when its last tab left and the source window disappeared.
#[derive(Clone, Debug)]
pub struct TabDetach {
    pub content: ContentId,
    pub source: ContainerId,
    pub detached: ContainerId,
    index: usize,
    active_before: ContentId,
    expected_source_tabs: Vec<ContentId>,
    expected_source_active: Option<ContentId>,
    removed_source: Option<Container>,
}

pub struct Workspace<'a> {
    layout: &'a mut Layout,
}
impl<'a> Workspace<'a> {
    pub fn new(layout: &'a mut Layout) -> Result<Self, WorkspaceError> {
        layout.validate().map_err(WorkspaceError::InvalidLayout)?;
        Ok(Self { layout })
    }
    pub fn layout(&self) -> &Layout {
        self.layout
    }

    fn check_add(&self, content: &ContentInstance) -> Result<(), WorkspaceError> {
        validate_content(content).map_err(WorkspaceError::InvalidLayout)?;
        if content.id.0.is_nil()
            || self.layout.contents.iter().any(|c| c.id == content.id)
            || self
                .layout
                .containers
                .iter()
                .any(|c| c.id.0 == content.id.0)
        {
            return Err(WorkspaceError::DuplicateIdentity);
        }
        if self.layout.contents.len() >= MAX_CONTENTS {
            return Err(WorkspaceError::BudgetExceeded);
        }
        let memberships: usize = self.layout.contents.iter().map(|c| c.items().len()).sum();
        if memberships + content.items().len() > MAX_ITEMS {
            return Err(WorkspaceError::BudgetExceeded);
        }
        if content.is_inbox() == self.layout.inbox().is_some() {
            return Err(WorkspaceError::InboxRequired);
        }
        let mut items = std::collections::HashSet::new();
        for item in content.items() {
            if item.item_id.is_nil()
                || !items.insert(item.item_id)
                || self
                    .layout
                    .contents
                    .iter()
                    .any(|c| c.contains_item(item.item_id))
            {
                return Err(WorkspaceError::InvalidLayout(
                    "duplicate collection membership".into(),
                ));
            }
        }
        if let ContentSpec::Panel { panel } = &content.content
            && self.layout.contents.iter().any(|c| {
                matches!(&c.content, ContentSpec::Panel { panel: existing }
                    if existing.instance_id == panel.instance_id)
            })
        {
            return Err(WorkspaceError::DuplicateIdentity);
        }
        Ok(())
    }
    fn fresh_container(
        &self,
        content: ContentId,
        geometry: NormGeometry,
    ) -> Result<Container, WorkspaceError> {
        validate_geometry(&geometry).map_err(WorkspaceError::InvalidLayout)?;
        let mut container = Container::new(content, geometry);
        while self.layout.containers.iter().any(|c| c.id == container.id)
            || self
                .layout
                .contents
                .iter()
                .any(|c| c.id.0 == container.id.0)
            || container.id.0 == content.0
        {
            container.id = ContainerId::new();
        }
        Ok(container)
    }
    pub fn create(
        &mut self,
        content: ContentInstance,
        geometry: NormGeometry,
    ) -> Result<Transition, WorkspaceError> {
        self.check_add(&content)?;
        let container = self.fresh_container(content.id, geometry)?;
        let id = container.id;
        let content_id = content.id;
        self.layout.contents.push(content);
        self.layout.containers.push(container);
        Ok(Transition {
            created_containers: vec![id],
            moved_content: Some(content_id),
            ..Transition::default()
        })
    }
    pub fn add(
        &mut self,
        container: ContainerId,
        content: ContentInstance,
    ) -> Result<Transition, WorkspaceError> {
        self.layout
            .container(container)
            .ok_or(WorkspaceError::MissingContainer(container))?;
        self.check_add(&content)?;
        let id = content.id;
        let target = self.layout.container_mut(container).expect("checked");
        target.tabs.push(id);
        target.active_tab = id;
        self.layout.contents.push(content);
        Ok(Transition {
            moved_content: Some(id),
            ..Transition::default()
        })
    }
    pub fn select(
        &mut self,
        container: ContainerId,
        content: ContentId,
    ) -> Result<Transition, WorkspaceError> {
        let target = self
            .layout
            .container_mut(container)
            .ok_or(WorkspaceError::MissingContainer(container))?;
        if !target.tabs.contains(&content) {
            return Err(WorkspaceError::NotOwned { container, content });
        }
        target.active_tab = content;
        Ok(Transition::default())
    }
    /// Insert into another container at `index`; within the same container this is a reorder
    /// using final strip indices. Out-of-range indices are errors, not silently clamped.
    pub fn attach(
        &mut self,
        content: ContentId,
        container: ContainerId,
        index: usize,
    ) -> Result<Transition, WorkspaceError> {
        let source = self
            .layout
            .owner_of(content)
            .ok_or(WorkspaceError::MissingContent(content))?;
        let target = self
            .layout
            .container(container)
            .ok_or(WorkspaceError::MissingContainer(container))?;
        if source == container {
            return self.reorder(container, content, index);
        }
        if index > target.tabs.len() {
            return Err(WorkspaceError::InvalidIndex {
                index,
                len: target.tabs.len(),
            });
        }
        let removed = self.remove_reference(source, content);
        let target = self.layout.container_mut(container).expect("checked");
        target.tabs.insert(index, content);
        target.active_tab = content;
        Ok(Transition {
            removed_containers: removed.into_iter().collect(),
            moved_content: Some(content),
            ..Transition::default()
        })
    }
    pub fn reorder(
        &mut self,
        container: ContainerId,
        content: ContentId,
        index: usize,
    ) -> Result<Transition, WorkspaceError> {
        let target = self
            .layout
            .container_mut(container)
            .ok_or(WorkspaceError::MissingContainer(container))?;
        let from = target
            .tabs
            .iter()
            .position(|id| *id == content)
            .ok_or(WorkspaceError::NotOwned { container, content })?;
        if index >= target.tabs.len() {
            return Err(WorkspaceError::InvalidIndex {
                index,
                len: target.tabs.len(),
            });
        }
        target.tabs.remove(from);
        target.tabs.insert(index, content);
        Ok(Transition::default())
    }
    pub fn detach(
        &mut self,
        content: ContentId,
        geometry: NormGeometry,
    ) -> Result<Transition, WorkspaceError> {
        self.detach_with_plan(content, geometry)
            .map(|(transition, _)| transition)
    }
    pub fn detach_with_plan(
        &mut self,
        content: ContentId,
        geometry: NormGeometry,
    ) -> Result<(Transition, TabDetach), WorkspaceError> {
        let source = self
            .layout
            .owner_of(content)
            .ok_or(WorkspaceError::MissingContent(content))?;
        let original = self.layout.container(source).expect("validated owner");
        let index = original
            .tabs
            .iter()
            .position(|id| *id == content)
            .expect("owned");
        let active_before = original.active_tab;
        if original.tabs.len() > 1 && self.layout.containers.len() >= MAX_CONTENTS {
            return Err(WorkspaceError::BudgetExceeded);
        }
        let removed_source = (original.tabs.len() == 1).then(|| original.clone());
        let detached = self.fresh_container(content, geometry)?;
        let detached_id = detached.id;
        let removed = self.remove_reference(source, content);
        let remaining = self.layout.container(source);
        let plan = TabDetach {
            content,
            source,
            detached: detached_id,
            index,
            active_before,
            removed_source,
            expected_source_tabs: remaining.map(|c| c.tabs.clone()).unwrap_or_default(),
            expected_source_active: remaining.map(|c| c.active_tab),
        };
        self.layout.containers.push(detached);
        Ok((
            Transition {
                created_containers: vec![detached_id],
                removed_containers: removed.into_iter().collect(),
                moved_content: Some(content),
                ..Transition::default()
            },
            plan,
        ))
    }
    /// Cancels only the ownership change if its exact topology is still current. Content
    /// edits and surviving-window geometry/appearance are never replaced with old values.
    pub fn cancel_detach(&mut self, plan: &TabDetach) -> Result<Transition, WorkspaceError> {
        let detached = self
            .layout
            .container(plan.detached)
            .ok_or(WorkspaceError::StaleDetach)?;
        if detached.tabs != [plan.content]
            || detached.active_tab != plan.content
            || self.layout.owner_of(plan.content) != Some(plan.detached)
        {
            return Err(WorkspaceError::StaleDetach);
        }
        match (self.layout.container(plan.source), &plan.removed_source) {
            (Some(source), None)
                if source.tabs == plan.expected_source_tabs
                    && Some(source.active_tab) == plan.expected_source_active => {}
            (None, Some(_)) => {}
            _ => return Err(WorkspaceError::StaleDetach),
        }
        self.layout.containers.retain(|c| c.id != plan.detached);
        let recreated = if let Some(source) = &plan.removed_source {
            self.layout.containers.push(source.clone());
            vec![plan.source]
        } else {
            let source = self.layout.container_mut(plan.source).expect("checked");
            source.tabs.insert(plan.index, plan.content);
            source.active_tab = plan.active_before;
            Vec::new()
        };
        Ok(Transition {
            created_containers: recreated,
            removed_containers: vec![plan.detached],
            moved_content: Some(plan.content),
            ..Transition::default()
        })
    }
    pub fn delete(&mut self, content: ContentId) -> Result<Transition, WorkspaceError> {
        let instance = self
            .layout
            .content(content)
            .ok_or(WorkspaceError::MissingContent(content))?;
        if instance.is_inbox() && self.layout.contents.len() > 1 {
            return Err(WorkspaceError::InboxRequired);
        }
        let owner = self.layout.owner_of(content).expect("validated owner");
        let removed = self.remove_reference(owner, content);
        self.layout.contents.retain(|c| c.id != content);
        Ok(Transition {
            removed_containers: removed.into_iter().collect(),
            deleted_content: Some(content),
            ..Transition::default()
        })
    }
    fn remove_reference(&mut self, owner: ContainerId, content: ContentId) -> Option<ContainerId> {
        let source = self.layout.container_mut(owner).expect("validated owner");
        let index = source
            .tabs
            .iter()
            .position(|id| *id == content)
            .expect("owned");
        source.tabs.remove(index);
        if source.tabs.is_empty() {
            self.layout.containers.retain(|c| c.id != owner);
            Some(owner)
        } else {
            if source.active_tab == content {
                source.active_tab = source.tabs[index.min(source.tabs.len() - 1)];
            }
            None
        }
    }
}

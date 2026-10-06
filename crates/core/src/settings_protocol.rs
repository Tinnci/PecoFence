//! Settings is an application client, not a serialized-config writer.
//! Wire version, page identity, workspace lifetime and document revision are independent.

use crate::i18n::Language;
use crate::rules::{Class, Cond, Rule, Target, Template};
use crate::{
    Config, ContainerId, ContentId, PeekHotkey, RuleId, Settings, ShowDesktopSetting, Spacing,
    ThemeSetting, ThemeStyle, TitleSize,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use uuid::Uuid;

pub const VERSION: u16 = 1;
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
const REPLAY_WINDOW: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentStamp {
    pub workspace: Uuid,
    pub revision: u64,
}

#[derive(Debug)]
pub struct DocumentClock {
    stamp: DocumentStamp,
    committed: Option<u64>,
}

impl DocumentClock {
    pub fn new(committed: bool) -> Self {
        Self {
            stamp: DocumentStamp {
                workspace: Uuid::new_v4(),
                revision: 0,
            },
            committed: committed.then_some(0),
        }
    }
    pub fn stamp(&self) -> DocumentStamp {
        self.stamp
    }
    pub fn committed(&self) -> Option<u64> {
        self.committed
    }
    pub fn dirty(&self) -> bool {
        self.stamp.revision > self.committed.unwrap_or(0)
    }
    pub fn change(&mut self) {
        self.stamp.revision = self
            .stamp
            .revision
            .checked_add(1)
            .expect("document revision exhausted");
    }
    pub fn replace(&mut self) {
        *self = Self::new(false);
    }
    pub fn commit(&mut self, stamp: DocumentStamp) -> bool {
        if stamp.workspace != self.stamp.workspace || stamp.revision > self.stamp.revision {
            return false;
        }
        self.committed = Some(self.committed.unwrap_or(0).max(stamp.revision));
        true
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "property",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum SettingChange {
    Language(Language),
    Theme(ThemeSetting),
    ThemeStyle(ThemeStyle),
    IconSize(u32),
    Autostart(bool),
    HideRealIcons(bool),
    QuickHideEnabled(bool),
    ShowDesktop(ShowDesktopSetting),
    HoverPeek(bool),
    ClickToExpand(bool),
    TitleOnHover(bool),
    HideInactiveScrollbar(bool),
    SnappingEnabled(bool),
    PeekEnabled(bool),
    PeekDim(bool),
    PeekHotkey(PeekHotkey),
    IconTint(Option<[u8; 3]>),
    IconTintStrength(f32),
    Chameleon(bool),
}

impl SettingChange {
    pub fn apply(&self, settings: &mut Settings) -> Result<(), String> {
        match self {
            Self::Language(v) => settings.language = *v,
            Self::Theme(v) => settings.theme = *v,
            Self::ThemeStyle(v) => settings.theme_style = *v,
            Self::IconSize(v) if matches!(v, 32 | 48 | 64 | 96) => settings.icon_size = *v,
            Self::IconSize(_) => return Err("invalid icon size".into()),
            Self::Autostart(v) => settings.autostart = *v,
            Self::HideRealIcons(v) => settings.hide_real_icons = *v,
            Self::QuickHideEnabled(v) => settings.quick_hide.enabled = *v,
            Self::ShowDesktop(v) => settings.show_desktop = *v,
            Self::HoverPeek(v) => settings.roll_up.hover_peek = *v,
            Self::ClickToExpand(v) => settings.roll_up.click_to_expand = *v,
            Self::TitleOnHover(v) => settings.roll_up.title_on_hover = *v,
            Self::HideInactiveScrollbar(v) => settings.roll_up.hide_inactive_scrollbar = *v,
            Self::SnappingEnabled(v) => settings.snapping.enabled = *v,
            Self::PeekEnabled(v) => settings.peek.enabled = *v,
            Self::PeekDim(v) => settings.peek.dim = *v,
            Self::PeekHotkey(v) => settings.peek.hotkey = *v,
            Self::IconTint(v) => settings.icons.tint_rgb = *v,
            Self::IconTintStrength(v) if v.is_finite() && (0.0..=1.0).contains(v) => {
                settings.icons.tint_strength = *v
            }
            Self::IconTintStrength(_) => return Err("invalid tint strength".into()),
            Self::Chameleon(v) => settings.icons.chameleon = *v,
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "property",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum ContentChange {
    Title(String),
    IconSize(u32),
    Spacing(Spacing),
    PortalNavigate(bool),
    PortalTitleIcon(bool),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TitleColor {
    Theme,
    Tint,
    White,
    Black,
    Custom([u8; 3]),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "property",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum ContainerChange {
    AutoHeight(bool),
    Locked(bool),
    ExcludeFromQuickHide(bool),
    Opacity(Option<f32>),
    Tint(Option<[u8; 3]>),
    TitleColor(TitleColor),
    TitleSize(TitleSize),
    DockTop,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleDraft {
    pub name: String,
    pub enabled: bool,
    pub target: Target,
    pub all_of: Vec<Cond>,
    pub priority_class: Class,
}

impl RuleDraft {
    fn validate(&self) -> Result<(), String> {
        Rule::validate_definition(&self.name, &self.all_of)
    }
    fn rule(&self, id: RuleId, template: Option<String>) -> Rule {
        Rule {
            id,
            name: self.name.clone(),
            enabled: self.enabled,
            target: self.target,
            all_of: self.all_of.clone(),
            priority_class: self.priority_class,
            template,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuleChange {
    Create { id: RuleId, draft: RuleDraft },
    Edit { id: RuleId, draft: RuleDraft },
    SetEnabled { id: RuleId, value: bool },
    Move { id: RuleId, before: Option<RuleId> },
    Delete { id: RuleId },
    KeepUpdated { value: bool },
    DefaultTarget { target: Target },
}

impl RuleChange {
    pub fn apply(&self, config: &mut Config) -> Result<bool, String> {
        let mut candidate = config.clone();
        let rules = &mut candidate.rules;
        match self {
            Self::KeepUpdated { value } => rules.keep_updated = *value,
            Self::DefaultTarget { target } => rules.default_target = *target,
            Self::Create { id, draft } => {
                draft.validate()?;
                if rules.list.iter().any(|r| r.id == *id) {
                    return Err("rule already exists".into());
                }
                rules.list.push(draft.rule(*id, None));
            }
            Self::Edit { id, draft } => {
                draft.validate()?;
                let rule = rules
                    .list
                    .iter_mut()
                    .find(|r| r.id == *id)
                    .ok_or("rule not found")?;
                *rule = draft.rule(*id, rule.template.clone());
            }
            Self::SetEnabled { id, value } => {
                rules
                    .list
                    .iter_mut()
                    .find(|r| r.id == *id)
                    .ok_or("rule not found")?
                    .enabled = *value
            }
            Self::Delete { id } => {
                let index = rules
                    .list
                    .iter()
                    .position(|r| r.id == *id)
                    .ok_or("rule not found")?;
                rules.list.remove(index);
            }
            Self::Move { id, before } => {
                let index = rules
                    .list
                    .iter()
                    .position(|r| r.id == *id)
                    .ok_or("rule not found")?;
                if *before == Some(*id) {
                    return Ok(false);
                }
                if let Some(target) = before
                    && !rules.list.iter().any(|r| r.id == *target)
                {
                    return Err("insertion target not found".into());
                }
                let rule = rules.list.remove(index);
                let index = before
                    .and_then(|target| rules.list.iter().position(|r| r.id == target))
                    .unwrap_or(rules.list.len());
                rules.list.insert(index, rule);
            }
        }
        candidate.validate()?;
        let changed = candidate.rules != config.rules;
        if changed {
            config.rules = candidate.rules;
        }
        Ok(changed)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Action {
    ApplyRules,
    AddTemplate { template: String },
    OpenConfigFolder,
    SaveSnapshot { name: String },
    RestoreSnapshot { id: Uuid },
    DeleteSnapshot { id: Uuid },
    SwapMonitors { first: String, second: String },
    ExportConfig,
    NewWorkspace { confirmed: bool },
    AcceptRecovery { confirmed: bool },
    ImportConfig { confirmed: bool },
    RestoreBackup { path: PathBuf, confirmed: bool },
    RepairIcons,
    HideDesktopIcons,
    RetrySave,
    CancelClose,
}

impl Action {
    pub fn permitted_read_only(&self) -> bool {
        matches!(
            self,
            Self::OpenConfigFolder
                | Self::ExportConfig
                | Self::CancelClose
                | Self::NewWorkspace { .. }
                | Self::AcceptRecovery { .. }
                | Self::ImportConfig { .. }
                | Self::RestoreBackup { .. }
        )
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::AddTemplate { template } if Template::parse(template).is_none() => {
                Err("unknown template".into())
            }
            Self::NewWorkspace { confirmed }
            | Self::AcceptRecovery { confirmed }
            | Self::ImportConfig { confirmed }
            | Self::RestoreBackup { confirmed, .. }
                if !confirmed =>
            {
                Err("explicit confirmation required".into())
            }
            Self::SwapMonitors { first, second }
                if first.is_empty() || second.is_empty() || first == second =>
            {
                Err("choose two different monitors".into())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SettingsCommand {
    SetSetting {
        change: SettingChange,
    },
    SetContent {
        content_id: ContentId,
        container_id: ContainerId,
        change: ContentChange,
    },
    SetContainer {
        content_id: ContentId,
        container_id: ContainerId,
        change: ContainerChange,
    },
    Rule {
        change: RuleChange,
    },
    Action {
        action: Action,
    },
}

impl SettingsCommand {
    pub fn permitted_read_only(&self) -> bool {
        matches!(self, Self::Action { action } if action.permitted_read_only())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol: u16,
    pub client: Uuid,
    pub sequence: u64,
    pub base: DocumentStamp,
    pub command: SettingsCommand,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ClientMessage {
    Ready { protocol: u16, page: Uuid },
    Request(Box<Request>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rejection {
    Protocol,
    Client,
    Workspace,
    Conflict,
    Expired,
    Sequence,
    ReusedSequence,
    ReadOnly,
    Invalid(String),
    Backend(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub client: Uuid,
    pub sequence: u64,
    pub base: DocumentStamp,
    pub current: DocumentStamp,
    pub rejected: Option<Rejection>,
    pub cancelled: bool,
}

#[derive(Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ServerMessage {
    Snapshot {
        protocol: u16,
        page: Uuid,
        client: Uuid,
        stamp: DocumentStamp,
        sequence: u64,
        view: serde_json::Value,
    },
    Receipt(Receipt),
    Persistence {
        page: Uuid,
        client: Uuid,
        stamp: DocumentStamp,
        committed_revision: Option<u64>,
        issue: Option<String>,
    },
    ProtocolError {
        detail: String,
    },
}

pub enum Admission {
    Apply,
    Replay(Receipt),
    Reject(Rejection),
}

#[derive(Default)]
pub struct SettingsSession {
    page: Option<Uuid>,
    client: Option<Uuid>,
    next: u64,
    cache: VecDeque<(Request, Receipt)>,
}

impl SettingsSession {
    pub fn open(&mut self, page: Uuid) -> Uuid {
        if self.page == Some(page)
            && let Some(client) = self.client
        {
            return client;
        }
        let client = Uuid::new_v4();
        self.page = Some(page);
        self.client = Some(client);
        self.next = 1;
        self.cache.clear();
        client
    }
    pub fn client(&self) -> Option<Uuid> {
        self.client
    }
    pub fn close(&mut self) {
        *self = Self::default();
    }
    pub fn page(&self) -> Option<Uuid> {
        self.page
    }
    pub fn admit(&self, request: &Request, current: DocumentStamp, writable: bool) -> Admission {
        if request.protocol != VERSION {
            return Admission::Reject(Rejection::Protocol);
        }
        if Some(request.client) != self.client {
            return Admission::Reject(Rejection::Client);
        }
        if let Some((original, receipt)) = self
            .cache
            .iter()
            .find(|(r, _)| r.sequence == request.sequence)
        {
            return if original == request {
                Admission::Replay(receipt.clone())
            } else {
                Admission::Reject(Rejection::ReusedSequence)
            };
        }
        if request.sequence < self.next {
            return Admission::Reject(Rejection::Expired);
        }
        if request.sequence != self.next || request.sequence > 9_007_199_254_740_991 {
            return Admission::Reject(Rejection::Sequence);
        }
        if request.base.workspace != current.workspace {
            return Admission::Reject(Rejection::Workspace);
        }
        if request.base.revision != current.revision {
            return Admission::Reject(Rejection::Conflict);
        }
        if !writable && !request.command.permitted_read_only() {
            return Admission::Reject(Rejection::ReadOnly);
        }
        Admission::Apply
    }
    /// A decision consumes a sequence even when validation or persistence policy rejects it.
    /// Expired/mismatched identities cannot change the sequence high-water mark.
    pub fn record(
        &mut self,
        request: Request,
        current: DocumentStamp,
        rejected: Option<Rejection>,
        cancelled: bool,
    ) -> Receipt {
        let receipt = Receipt {
            client: request.client,
            sequence: request.sequence,
            base: request.base,
            current,
            rejected,
            cancelled,
        };
        if Some(request.client) == self.client
            && request.sequence == self.next
            && request.protocol == VERSION
        {
            self.next = self
                .next
                .checked_add(1)
                .expect("settings sequence exhausted");
            self.cache.push_back((request, receipt.clone()));
            if self.cache.len() > REPLAY_WINDOW {
                self.cache.pop_front();
            }
        }
        receipt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(client: Uuid, sequence: u64, base: DocumentStamp) -> Request {
        Request {
            protocol: VERSION,
            client,
            sequence,
            base,
            command: SettingsCommand::SetSetting {
                change: SettingChange::Autostart(true),
            },
        }
    }

    #[test]
    fn commits_do_not_clear_newer_edits_or_another_workspace() {
        let mut clock = DocumentClock::new(true);
        clock.change();
        let older = clock.stamp();
        clock.change();
        assert!(clock.commit(older));
        assert!(clock.dirty());
        assert_eq!(clock.committed(), Some(older.revision));
        clock.replace();
        clock.change();
        assert!(!clock.commit(older));
        assert_eq!(clock.committed(), None);
    }

    #[test]
    fn page_identity_and_workspace_lifetime_are_not_revision_aliases() {
        let mut session = SettingsSession::default();
        let page = Uuid::new_v4();
        let client = session.open(page);
        assert_eq!(session.open(page), client);
        let original = DocumentClock::new(true).stamp();
        let another = DocumentClock::new(true).stamp();
        let r = request(client, 1, original);
        assert!(matches!(
            session.admit(&r, another, true),
            Admission::Reject(Rejection::Workspace)
        ));
        assert_ne!(session.open(Uuid::new_v4()), client);
        assert!(matches!(
            session.admit(&r, original, true),
            Admission::Reject(Rejection::Client)
        ));
    }

    #[test]
    fn closing_a_page_revokes_queued_requests_and_cannot_revive_its_client() {
        let mut session = SettingsSession::default();
        let page = Uuid::new_v4();
        let client = session.open(page);
        let stamp = DocumentClock::new(true).stamp();
        let request = request(client, 1, stamp);
        session.close();
        assert!(session.page().is_none());
        assert!(matches!(
            session.admit(&request, stamp, true),
            Admission::Reject(Rejection::Client)
        ));
        assert_ne!(session.open(page), client);
        assert!(Action::CancelClose.permitted_read_only());
        assert!(!Action::RetrySave.permitted_read_only());
    }
    #[test]
    fn accepted_and_rejected_requests_are_replayed_without_execution() {
        let mut session = SettingsSession::default();
        let client = session.open(Uuid::new_v4());
        let mut clock = DocumentClock::new(true);
        let r = request(client, 1, clock.stamp());
        assert!(matches!(
            session.admit(&r, clock.stamp(), true),
            Admission::Apply
        ));
        clock.change();
        let receipt = session.record(r.clone(), clock.stamp(), None, false);
        assert!(
            matches!(session.admit(&r, clock.stamp(), true), Admission::Replay(cached) if cached == receipt)
        );
        let mut different = r;
        different.command = SettingsCommand::SetSetting {
            change: SettingChange::Autostart(false),
        };
        assert!(matches!(
            session.admit(&different, clock.stamp(), true),
            Admission::Reject(Rejection::ReusedSequence)
        ));
        let stale = request(
            client,
            2,
            DocumentStamp {
                revision: 0,
                ..clock.stamp()
            },
        );
        assert!(matches!(
            session.admit(&stale, clock.stamp(), true),
            Admission::Reject(Rejection::Conflict)
        ));
        let receipt = session.record(
            stale.clone(),
            clock.stamp(),
            Some(Rejection::Conflict),
            false,
        );
        assert!(
            matches!(session.admit(&stale, clock.stamp(), true), Admission::Replay(cached) if cached == receipt)
        );
    }

    #[test]
    fn bounded_replay_cache_never_reexecutes_expired_requests() {
        let mut session = SettingsSession::default();
        let client = session.open(Uuid::new_v4());
        let clock = DocumentClock::new(true);
        for sequence in 1..=40 {
            let r = request(client, sequence, clock.stamp());
            assert!(matches!(
                session.admit(&r, clock.stamp(), true),
                Admission::Apply
            ));
            session.record(r, clock.stamp(), None, false);
        }
        assert_eq!(session.cache.len(), REPLAY_WINDOW);
        assert!(matches!(
            session.admit(&request(client, 1, clock.stamp()), clock.stamp(), true),
            Admission::Reject(Rejection::Expired)
        ));
        assert!(matches!(
            session.admit(&request(client, 42, clock.stamp()), clock.stamp(), true),
            Admission::Reject(Rejection::Sequence)
        ));
    }

    #[test]
    fn invalid_wire_values_unknown_fields_and_legacy_commands_are_rejected() {
        assert!(
            serde_json::from_str::<ClientMessage>(r#"{"type":"patchSettings","settings":{}}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<ContainerChange>(r#"{"property":"locked","value":"true"}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<ContentChange>(
                r#"{"property":"spacing","value":"unsupported"}"#
            )
            .is_err()
        );
        let mut session = SettingsSession::default();
        let client = session.open(Uuid::new_v4());
        let r = request(client, 1, DocumentClock::new(true).stamp());
        let value = serde_json::to_value(ClientMessage::Request(Box::new(r.clone()))).unwrap();
        assert_eq!(
            serde_json::from_value::<ClientMessage>(value.clone()).unwrap(),
            ClientMessage::Request(Box::new(r))
        );
        let mut unknown = value;
        unknown["legacy"] = true.into();
        assert!(serde_json::from_value::<ClientMessage>(unknown).is_err());
    }

    #[test]
    fn readonly_commands_are_admitted_only_for_explicit_recovery_actions() {
        let mut session = SettingsSession::default();
        let client = session.open(Uuid::new_v4());
        let stamp = DocumentClock::new(false).stamp();
        let mut r = request(client, 1, stamp);
        assert!(matches!(
            session.admit(&r, stamp, false),
            Admission::Reject(Rejection::ReadOnly)
        ));
        r.command = SettingsCommand::Action {
            action: Action::NewWorkspace { confirmed: false },
        };
        assert!(matches!(session.admit(&r, stamp, false), Admission::Apply));
        assert!(
            Action::NewWorkspace { confirmed: false }
                .validate()
                .is_err()
        );
    }

    #[test]
    fn rule_edits_are_granular_and_failed_edits_are_atomic() {
        let mut config = Config::default();
        let id = Uuid::new_v4();
        let draft = RuleDraft {
            name: "documents".into(),
            enabled: true,
            target: Target::Inbox,
            all_of: vec![Cond::FilesOnly, Cond::Origin(crate::Origin::UserDesktop)],
            priority_class: Class::Custom,
        };
        assert!(
            RuleChange::Create {
                id,
                draft: draft.clone()
            }
            .apply(&mut config)
            .unwrap()
        );
        config.rules.keep_updated = false;
        assert!(
            RuleChange::SetEnabled { id, value: false }
                .apply(&mut config)
                .unwrap()
        );
        assert!(!config.rules.keep_updated);
        let before = config.rules.clone();
        let mut invalid = draft.clone();
        invalid.all_of = vec![Cond::CreatedWeekday(vec![8])];
        assert!(
            RuleChange::Edit { id, draft: invalid }
                .apply(&mut config)
                .is_err()
        );
        assert_eq!(config.rules, before);
        assert!(RuleChange::Edit { id, draft }.apply(&mut config).unwrap());
        assert_eq!(config.rules.list[0].all_of.len(), 2);
    }

    #[test]
    fn browser_condition_shapes_roundtrip_and_unknown_nested_fields_fail() {
        for json in [
            r#"{"cond":"createdTime","value":{"fromMin":1080,"toMin":360}}"#,
            r#"{"cond":"sizeMb","value":{"min":0,"max":12.5}}"#,
            r#"{"cond":"name","value":{"op":"contains","value":"报告"}}"#,
            r#"{"cond":"idleDays","value":{"min":30}}"#,
            r#"{"cond":"filesOnly","value":null}"#,
            r#"{"cond":"origin","value":"namespace"}"#,
        ] {
            let value: serde_json::Value = serde_json::from_str(json).unwrap();
            let condition: Cond = serde_json::from_value(value.clone()).unwrap();
            // Unit variants serialize without their unnecessary null value.
            let serialized = serde_json::to_value(&condition).unwrap();
            assert_eq!(serialized["cond"], value["cond"]);
            assert_eq!(
                serde_json::from_value::<Cond>(serialized.clone()).unwrap(),
                condition
            );
            if value["cond"] == "createdTime" {
                assert_eq!(serialized["value"], value["value"]);
            }
            if value["cond"] == "filesOnly" {
                assert!(serialized.get("value").is_none());
            }
        }
        assert!(
            serde_json::from_str::<Cond>(
                r#"{"cond":"createdTime","value":{"fromMin":0,"toMin":120,"legacy":true}}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<Cond>(
                r#"{"cond":"createdTime","value":{"from_min":0,"to_min":120}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn server_wire_names_distinguish_decision_from_primary_commit() {
        let stamp = DocumentClock::new(true).stamp();
        let client = Uuid::new_v4();
        let receipt = ServerMessage::Receipt(Receipt {
            client,
            sequence: 1,
            base: stamp,
            current: stamp,
            rejected: None,
            cancelled: false,
        });
        let value = serde_json::to_value(receipt).unwrap();
        assert_eq!(value["type"], "receipt");
        assert_eq!(value["sequence"], 1);
        assert!(value["rejected"].is_null());
        assert!(value.get("committedRevision").is_none());
        let value = serde_json::to_value(ServerMessage::Persistence {
            page: Uuid::new_v4(),
            client,
            stamp,
            committed_revision: Some(0),
            issue: Some("backup unavailable".into()),
        })
        .unwrap();
        assert_eq!(value["type"], "persistence");
        assert_eq!(value["committedRevision"], 0);
        assert_eq!(value["issue"], "backup unavailable");
        assert!(value.get("committed_revision").is_none());
    }

    #[test]
    fn imported_rules_obey_the_same_validation_as_client_edits() {
        let mut config = Config::default();
        config.rules.list.push(Rule::new(
            "bad range",
            Target::Inbox,
            vec![Cond::SizeMb {
                min: Some(9.0),
                max: Some(1.0),
            }],
        ));
        assert!(config.validate().is_err());
        config.rules.list[0].all_of = vec![Cond::CreatedTime {
            from_min: 1440,
            to_min: 0,
        }];
        assert!(config.validate().is_err());
        config.rules.list[0].all_of = vec![Cond::Origin(crate::Origin::Namespace)];
        assert!(config.validate().is_ok());
    }

    #[test]
    fn wildcard_work_is_bounded_even_for_many_failed_star_branches() {
        assert!(!crate::rules::glob_match(
            &format!("{}b", "*a".repeat(100)),
            &"a".repeat(500)
        ));
        assert!(crate::rules::glob_match("*报告??.TXT", "季度报告01.txt"));
        assert!(crate::rules::glob_match("", ""));
        assert!(crate::rules::glob_match("***", ""));
        assert!(!crate::rules::glob_match("?", ""));
    }
}

use super::*;

impl SettingsComponent {
    pub(super) fn update_projection(&mut self, client: Uuid, mut view: SettingsView) {
        if self.client == Some(client)
            && self.view.as_ref().is_some_and(|current| {
                current.stamp.workspace == view.stamp.workspace
                    && current.stamp.revision > view.stamp.revision
            })
        {
            return;
        }
        let previous = self.view.as_ref().map(|previous| previous.stamp);
        if self.client.is_some_and(|old| old != client) {
            self.pending.clear();
            self.inflight = None;
            self.accepted.clear();
            self.next_sequence = 0;
            if let Some(draft) = &mut self.rule_draft {
                draft.needs_review = true;
            }
            for draft in self
                .title_drafts
                .values_mut()
                .chain(self.title_color_drafts.values_mut())
            {
                draft.needs_review = true;
            }
            self.notice = Some(Notice {
                text: tr("设置会话已更换；草稿仍保留，请先检查。"),
                error: true,
            });
        }
        self.client = Some(client);
        if let Some(previous) = previous {
            if previous.workspace != view.stamp.workspace {
                self.pending.clear();
                self.accepted.clear();
                if let Some(draft) = &mut self.rule_draft {
                    draft.needs_review = true;
                }
                for draft in self
                    .title_drafts
                    .values_mut()
                    .chain(self.title_color_drafts.values_mut())
                {
                    draft.needs_review = true;
                }
                self.notice = Some(Notice {
                    text: tr("工作区已更换；旧草稿没有自动应用到新工作区。"),
                    error: true,
                });
            } else if previous != view.stamp {
                if let Some(draft) = &mut self.rule_draft
                    && draft.submitted.is_none()
                {
                    draft.needs_review = true;
                }
                for draft in self
                    .title_drafts
                    .values_mut()
                    .chain(self.title_color_drafts.values_mut())
                {
                    if draft.submitted.is_none() {
                        draft.needs_review = true;
                    }
                }
            }
        }
        self.accepted.retain(|accepted| {
            accepted.stamp.workspace != view.stamp.workspace
                || accepted.stamp.revision > view.stamp.revision
        });
        self.title_drafts.retain(|_, draft| {
            !draft.submitted.is_some_and(|stamp| {
                stamp.workspace == view.stamp.workspace && view.stamp.revision >= stamp.revision
            })
        });
        self.title_color_drafts.retain(|_, draft| {
            !draft.submitted.is_some_and(|stamp| {
                stamp.workspace == view.stamp.workspace && view.stamp.revision >= stamp.revision
            })
        });
        if self.rule_draft.as_ref().is_some_and(|draft| {
            draft.submitted.is_some_and(|stamp| {
                stamp.workspace == view.stamp.workspace && view.stamp.revision >= stamp.revision
            })
        }) {
            self.rule_draft = None;
        }
        if let Some((stamp, count)) = self.item_summary
            && stamp.workspace == view.stamp.workspace
            && stamp.revision <= view.stamp.revision
        {
            view.item_count = count;
        }
        let explicitly_requested = self.pending_content.is_some();
        if let Some(requested) = self.pending_content.take() {
            self.selected_content = view
                .contents
                .iter()
                .find(|content| content.content_id == requested)
                .map(Self::key);
        }
        if previous.is_none() && !explicitly_requested && self.selected_content.is_none() {
            self.selected_content = view.contents.first().map(Self::key);
        }
        // Missing targets remain selected by identity until the user chooses
        // another one. Background updates must never redirect an edit or swap.
        if previous.is_none() {
            self.monitor_first = view.monitors.first().map(|monitor| monitor.id.clone());
            self.monitor_second = view.monitors.get(1).map(|monitor| monitor.id.clone());
        }
        if self.confirm_action.as_ref().is_some_and(|confirmation| {
            confirmation.client != client
                || confirmation.base != view.stamp
                || (!view.writable && !confirmation.command.permitted_read_only())
        }) {
            self.confirm_action = None;
            self.notice = Some(Notice {
                text: tr("工作区已变化；请重新确认此操作。"),
                error: true,
            });
        }
        self.view = Some(view);
    }

    pub(super) fn on_decision(&mut self, receipt: Receipt) {
        let Some(inflight) = self.inflight.take() else {
            return;
        };
        if inflight.request.client != receipt.client
            || inflight.request.sequence != receipt.sequence
            || inflight.request.base != receipt.base
        {
            self.inflight = Some(inflight);
            return;
        }
        let workspace = self.view.as_ref().map(|view| view.stamp.workspace);
        if let Some(rejection) = &receipt.rejected {
            self.notice = Some(Notice {
                text: rejection_text(rejection),
                error: true,
            });
            match inflight.kind {
                RequestKind::RuleDraft(token, _) => {
                    if let Some(draft) = &mut self.rule_draft
                        && draft.token == token
                    {
                        draft.needs_review = true;
                    }
                }
                RequestKind::TextDraft(field, key, _) => {
                    if let Some(draft) = self.text_drafts(field).get_mut(&key) {
                        draft.needs_review = true;
                        draft.submitted = None;
                    }
                }
                _ => {}
            }
        } else if receipt.cancelled {
            self.notice = Some(Notice {
                text: tr("操作已取消；没有报告为已保存。"),
                error: false,
            });
        } else {
            self.notice = None;
            if workspace == Some(receipt.current.workspace) {
                self.accepted.push(Accepted {
                    stamp: receipt.current,
                    command: inflight.request.command.clone(),
                });
            }
            match inflight.kind {
                RequestKind::RuleDraft(token, generation) => {
                    if let Some(draft) = &mut self.rule_draft
                        && draft.token == token
                    {
                        if let SettingsCommand::Rule {
                            change: RuleChange::Create { id, draft: created },
                        } = &inflight.request.command
                        {
                            draft.accept_creation(*id, created.priority_class);
                        }
                        draft.mark_submitted(generation, receipt.current);
                    }
                }
                RequestKind::TextDraft(field, key, generation) => {
                    if let Some(draft) = self.text_drafts(field).get_mut(&key) {
                        if draft.generation == generation {
                            draft.submitted = Some(receipt.current);
                            draft.needs_review = false;
                        } else {
                            draft.needs_review = true;
                        }
                    }
                }
                _ => {}
            }
            if inflight.kind == RequestKind::Immediate
                && receipt.current.workspace == receipt.base.workspace
                && workspace == Some(receipt.current.workspace)
            {
                for pending in &mut self.pending {
                    if pending.kind == RequestKind::Immediate
                        && pending.request.base == receipt.base
                        && pending.request.base.workspace == receipt.current.workspace
                    {
                        pending.request.base = receipt.current;
                    }
                }
            }
        }
        self.pump();
    }

    pub(super) fn submit(
        &mut self,
        command: SettingsCommand,
        kind: RequestKind,
        base: Option<DocumentStamp>,
    ) {
        let (Some(client), Some(view)) = (self.client, self.view.as_ref()) else {
            self.notice = Some(Notice {
                text: tr("设置尚未连接到应用工作区。"),
                error: true,
            });
            return;
        };
        if !view.writable && !command.permitted_read_only() {
            self.notice = Some(Notice {
                text: tr("此工作区为只读；仅恢复、查看和导出操作可用。"),
                error: true,
            });
            return;
        }
        if self.pending.len() + usize::from(self.inflight.is_some()) >= MAX_REQUESTS {
            self.notice = Some(Notice {
                text: tr("设置请求队列已满，请等待当前操作完成。"),
                error: true,
            });
            return;
        }
        if self.next_sequence + self.pending.len() as u64 >= MAX_SEQUENCE {
            self.notice = Some(Notice {
                text: tr("设置会话序号已用尽，请关闭并重新打开设置。"),
                error: true,
            });
            return;
        }
        self.pending.push_back(Pending {
            request: Request {
                protocol: VERSION,
                client,
                // Unsent requests do not consume a protocol sequence. A workspace
                // replacement can discard this queue without creating a gap.
                sequence: 0,
                base: base.unwrap_or(view.stamp),
                command,
            },
            kind,
        });
        self.notice = None;
        self.pump();
    }

    pub(super) fn pump(&mut self) {
        if self.inflight.is_none() {
            self.inflight = self.pending.pop_front();
            if let Some(pending) = &mut self.inflight {
                self.next_sequence += 1;
                pending.request.sequence = self.next_sequence;
                self.queue
                    .push(Command::Settings(SettingsCmd::SettingsRequest {
                        source: self.source,
                        request: Box::new(pending.request.clone()),
                    }));
            }
        }
    }

    pub(super) fn ask_confirmation(&mut self, command: SettingsCommand) {
        if self.confirm_action.is_some() {
            return;
        }
        let (Some(client), Some(view)) = (self.client, self.view.as_ref()) else {
            self.notice = Some(Notice {
                text: tr("设置尚未连接到应用工作区。"),
                error: true,
            });
            return;
        };
        self.confirm_action = Some(Confirmation {
            token: Uuid::new_v4(),
            source: self.source,
            client,
            base: view.stamp,
            command,
        });
    }

    pub(super) fn finish_confirmation(&mut self, token: Uuid, result: ContentDialogResult) {
        if !self
            .confirm_action
            .as_ref()
            .is_some_and(|confirmation| confirmation.token == token)
        {
            return;
        }
        let confirmation = self.confirm_action.take().unwrap();
        if result != ContentDialogResult::Primary {
            return;
        }
        if !self.shared.alive.get()
            || confirmation.source != self.source
            || self.client != Some(confirmation.client)
            || !self
                .view
                .as_ref()
                .is_some_and(|view| view.stamp == confirmation.base)
        {
            self.notice = Some(Notice {
                text: tr("工作区已变化；请重新确认此操作。"),
                error: true,
            });
            return;
        }
        let command = match confirmation.command {
            SettingsCommand::Action { action } => SettingsCommand::Action {
                action: mark_confirmed(action),
            },
            other => other,
        };
        self.submit(command, RequestKind::Action, Some(confirmation.base));
    }

    pub(super) fn submit_action(&mut self, action: Action) {
        self.submit(
            SettingsCommand::Action { action },
            RequestKind::Action,
            None,
        );
    }

    pub(super) fn rule_action(&mut self, change: RuleChange) {
        self.submit(
            SettingsCommand::Rule { change },
            RequestKind::Immediate,
            None,
        );
    }
}

pub(super) fn apply_command(view: &mut SettingsView, command: &SettingsCommand) {
    match command {
        SettingsCommand::SetSetting { change } => {
            _ = change.apply(&mut view.settings);
        }
        SettingsCommand::SetContent {
            content_id,
            container_id,
            change,
        } => {
            if let Some(content) = view.contents.iter_mut().find(|content| {
                content.content_id == *content_id && content.container_id == *container_id
            }) {
                match change {
                    ContentChange::Title(value) => content.title.clone_from(value),
                    ContentChange::IconSize(value) => content.icon_size = *value,
                    ContentChange::Spacing(value) => content.spacing = *value,
                    ContentChange::PortalNavigate(value) => {
                        if let Some(portal) = &mut content.portal {
                            portal.navigate = *value;
                        }
                    }
                    ContentChange::PortalTitleIcon(value) => {
                        if let Some(portal) = &mut content.portal {
                            portal.title_icon = *value;
                        }
                    }
                }
            }
        }
        SettingsCommand::SetContainer {
            content_id,
            container_id,
            change,
        } => {
            let belongs = view.contents.iter().any(|content| {
                content.content_id == *content_id && content.container_id == *container_id
            });
            if belongs {
                for content in view
                    .contents
                    .iter_mut()
                    .filter(|content| content.container_id == *container_id)
                {
                    apply_container_change(content, change);
                }
            }
        }
        SettingsCommand::Rule { change } => apply_rule_change(&mut view.rules, change),
        SettingsCommand::Action { .. } => {}
    }
}

fn apply_container_change(content: &mut ContentOptions, change: &ContainerChange) {
    match change {
        ContainerChange::AutoHeight(value) => content.auto_height = *value,
        ContainerChange::Locked(value) => content.locked = *value,
        ContainerChange::ExcludeFromQuickHide(value) => content.exclude_from_quick_hide = *value,
        ContainerChange::Opacity(value) => content.opacity = *value,
        ContainerChange::Tint(value) => content.tint = *value,
        ContainerChange::TitleColor(value) => content.title_color = value.clone(),
        ContainerChange::TitleSize(value) => content.title_size = *value,
        ContainerChange::DockTop => {}
    }
}

fn apply_rule_change(rules: &mut RuleSet, change: &RuleChange) {
    match change {
        RuleChange::KeepUpdated { value } => rules.keep_updated = *value,
        RuleChange::DefaultTarget { target } => rules.default_target = *target,
        RuleChange::Create { id, draft } => {
            if !rules.list.iter().any(|rule| rule.id == *id) {
                rules.list.push(Rule {
                    id: *id,
                    name: draft.name.clone(),
                    enabled: draft.enabled,
                    target: draft.target,
                    all_of: draft.all_of.clone(),
                    priority_class: draft.priority_class,
                    template: None,
                });
            }
        }
        RuleChange::Edit { id, draft } => {
            if let Some(rule) = rules.list.iter_mut().find(|rule| rule.id == *id) {
                let template = rule.template.clone();
                *rule = Rule {
                    id: *id,
                    name: draft.name.clone(),
                    enabled: draft.enabled,
                    target: draft.target,
                    all_of: draft.all_of.clone(),
                    priority_class: draft.priority_class,
                    template,
                };
            }
        }
        RuleChange::SetEnabled { id, value } => {
            if let Some(rule) = rules.list.iter_mut().find(|rule| rule.id == *id) {
                rule.enabled = *value;
            }
        }
        RuleChange::Move { id, before } => {
            if let Some(index) = rules.list.iter().position(|rule| rule.id == *id) {
                let rule = rules.list.remove(index);
                let index = before
                    .and_then(|target| rules.list.iter().position(|rule| rule.id == target))
                    .unwrap_or(rules.list.len());
                rules.list.insert(index, rule);
            }
        }
        RuleChange::Delete { id } => rules.list.retain(|rule| rule.id != *id),
    }
}

fn mark_confirmed(action: Action) -> Action {
    match action {
        Action::RestoreSnapshot { id } => Action::RestoreSnapshot { id },
        Action::DeleteSnapshot { id } => Action::DeleteSnapshot { id },
        Action::RestoreBackup { path, .. } => Action::RestoreBackup {
            path,
            confirmed: true,
        },
        Action::ApplyDefaultPreset { operation, .. } => Action::ApplyDefaultPreset {
            operation,
            confirmed: true,
        },
        Action::NewWorkspace { .. } => Action::NewWorkspace { confirmed: true },
        Action::AcceptRecovery { .. } => Action::AcceptRecovery { confirmed: true },
        Action::ImportConfig { .. } => Action::ImportConfig { confirmed: true },
        other => other,
    }
}

pub(super) fn action_confirmation_text(action: &Action) -> String {
    match action {
        Action::ApplyDefaultPreset { operation, .. } => match operation {
            pecofence_core::default_template::DefaultPresetOperation::RestoreLayout => tr(
                "恢复默认组件的排布；空间不足时合并为标签组。便笺、待办和文件归属保持不变，操作前保存快照。是否继续？",
            ),
            pecofence_core::default_template::DefaultPresetOperation::AddMissingWidgets => tr(
                "只补齐缺失的时钟、日历、便笺和待办。已有组件与内容保持不变，操作前保存快照。是否继续？",
            ),
            pecofence_core::default_template::DefaultPresetOperation::RestoreAppearance => tr(
                "恢复主题、小组件纸张和窗口外观。布局、内容和启动设置保持不变，操作前保存快照。是否继续？",
            ),
        },
        Action::RestoreSnapshot { .. } => tr("恢复快照将替换当前布局，是否继续？"),
        Action::DeleteSnapshot { .. } => tr("删除此快照后无法从设置中恢复。是否继续？"),
        Action::RestoreBackup { .. } => {
            tr("恢复备份会替换当前工作区。现有配置文件会保留。是否继续？")
        }
        Action::ImportConfig { .. } => tr("导入会替换当前工作区。现有配置文件会保留。是否继续？"),
        Action::AcceptRecovery { .. } => tr("接受恢复候选会替换当前工作区。是否继续？"),
        Action::NewWorkspace { .. } => {
            tr("新建工作区会替换当前工作区。现有配置文件会保留。是否继续？")
        }
        _ => tr("请确认此操作。"),
    }
}

pub(super) fn confirmation_label(command: &SettingsCommand) -> &'static str {
    match command {
        SettingsCommand::Action { action } => match action {
            Action::RestoreSnapshot { .. } => "恢复",
            Action::DeleteSnapshot { .. } => "删除",
            Action::RestoreBackup { .. } => "恢复此备份",
            Action::ImportConfig { .. } => "导入配置",
            Action::AcceptRecovery { .. } => "接受恢复候选…",
            Action::NewWorkspace { .. } => "新建工作区",
            _ => "确认并继续",
        },
        SettingsCommand::Rule {
            change: RuleChange::Delete { .. },
        } => "删除",
        _ => "确认并继续",
    }
}

fn rejection_text(rejection: &pecofence_core::settings_protocol::Rejection) -> String {
    use pecofence_core::settings_protocol::Rejection;
    match rejection {
        Rejection::Conflict => tr("工作区已变化；草稿已保留，请检查后明确重试。"),
        Rejection::Workspace => tr("工作区已更换；旧请求没有应用。"),
        Rejection::ReadOnly => tr("此工作区为只读，请先解决加载或恢复问题。"),
        Rejection::Invalid(detail) | Rejection::Backend(detail) => detail.clone(),
        Rejection::Expired | Rejection::Sequence | Rejection::ReusedSequence => {
            tr("设置请求序号已失效；请重新打开设置窗口。")
        }
        _ => tr("设置请求未被接受。"),
    }
}

pub(super) fn parse_rgb(value: &str) -> Option<[u8; 3]> {
    let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some([
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ])
}

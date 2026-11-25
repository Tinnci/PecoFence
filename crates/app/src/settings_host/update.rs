use super::*;

impl SettingsComponent {
    pub(super) fn handle_message(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::HostUpdate(client, view) => self.update_projection(client, *view),
            Message::HostDecision(receipt) => self.on_decision(receipt),
            Message::HostNotice(text, error) => self.notice = Some(Notice { text, error }),
            Message::HostTheme(mode, glass) => {
                self.mode = mode;
                self.liquid_glass = glass;
            }
            Message::LanguageChanged => {}
            Message::ShowContent(id) => {
                self.page = Page::Content;
                if let Some(view) = &self.view {
                    self.selected_content = view
                        .contents
                        .iter()
                        .find(|content| content.content_id == id)
                        .map(Self::key);
                    self.pending_content = None;
                } else {
                    self.pending_content = Some(id);
                }
            }
            Message::Summary(stamp, count) => {
                if self.view.as_ref().is_none_or(|view| {
                    view.stamp.workspace == stamp.workspace && view.stamp.revision <= stamp.revision
                }) {
                    self.item_summary = Some((stamp, count));
                }
            }
            Message::SetIcons(small, big) => {
                self.pending_icons = Some((small, big));
                self.apply_pending_icons();
            }
            Message::NativeHandle(raw) => {
                tracing::info!(source = %self.source, "settings: native controls ready");
                self.shared
                    .hwnd
                    .set(Some(HWND(raw as *mut std::ffi::c_void)));
                self.apply_pending_icons();
            }
            Message::WindowPlacement(placement) => self.placement = Some(placement),
            Message::Activate => {
                _ = context.activate_window();
            }
            Message::Close => {
                _ = context.close_window();
            }
            #[cfg(test)]
            Message::Navigate(page) => {
                self.navigate(page);
            }
            Message::SelectPage(tag) => {
                if let Some(page) = tag.as_deref().and_then(Page::from_tag) {
                    self.navigate(page);
                }
            }
            Message::TogglePane => {
                self.pane_open = !self.pane_open;
                if self.navigation_mode == NavigationViewDisplayMode::Expanded {
                    self.pane_override = Some(self.pane_open);
                } else {
                    self.overlay_open = self.pane_open;
                }
                tracing::debug!(open = self.pane_open, mode = ?self.navigation_mode, "settings: pane toggle requested");
            }
            Message::PaneOpenChanged(open) => {
                if self.navigation_mode == NavigationViewDisplayMode::Expanded {
                    self.pane_open = open && self.pane_override != Some(false);
                } else {
                    // Light dismissal and auto-close of an overlay.
                    self.pane_open = open;
                    self.overlay_open = open;
                }
                tracing::debug!(observed_open = open, open = self.pane_open, requested = ?self.pane_override, mode = ?self.navigation_mode, "settings: native pane state observed");
            }
            Message::NavigationModeChanged(mode) => {
                #[cfg(test)]
                self.shared.navigation_mode.set(mode);
                // Choose adaptive defaults until the first hamburger interaction. After
                // that, display-mode notifications must not override the user's request
                // or write IsPaneOpen in the middle of WinUI's own transition.
                if self.navigation_mode != mode {
                    self.navigation_mode = mode;
                    self.pane_open = if mode == NavigationViewDisplayMode::Expanded {
                        self.overlay_open = false;
                        self.pane_override.unwrap_or(true)
                    } else {
                        self.overlay_open
                    };
                }
                tracing::debug!(
                    open = self.pane_open,
                    ?mode,
                    "settings: navigation display mode observed"
                );
            }
            Message::Setting(change) => self.submit(
                SettingsCommand::SetSetting { change },
                RequestKind::Immediate,
                None,
            ),
            Message::IconTint(rgb) => self.submit(
                SettingsCommand::SetSetting {
                    change: SettingChange::IconTint(rgb),
                },
                RequestKind::Immediate,
                None,
            ),
            Message::SelectContent(index) => {
                self.selected_content =
                    index.and_then(|index| self.view.as_ref()?.contents.get(index).map(Self::key));
            }
            Message::ContentText(value) => self.set_title_draft(value),
            Message::ApplyTitle => self.submit_title(false),
            Message::RetryTitle => self.submit_title(true),
            Message::DiscardTitle => {
                if let Some(key) = self.selected_content {
                    self.title_drafts.remove(&key);
                }
            }
            Message::Content(key, change) => self.submit(
                SettingsCommand::SetContent {
                    content_id: key.content,
                    container_id: key.container,
                    change,
                },
                RequestKind::Immediate,
                None,
            ),
            Message::Container(key, change) => {
                if matches!(
                    &change,
                    ContainerChange::TitleColor(
                        pecofence_core::settings_protocol::TitleColor::Tint
                    )
                ) && self
                    .content(key)
                    .is_some_and(|content| content.tint.is_none())
                {
                    self.notice = Some(Notice {
                        text: tr("选择标题色调前，请先为窗口选择一种色调。"),
                        error: true,
                    });
                } else {
                    self.submit(
                        SettingsCommand::SetContainer {
                            content_id: key.content,
                            container_id: key.container,
                            change,
                        },
                        RequestKind::Immediate,
                        None,
                    );
                }
            }
            Message::Opacity(key, value) => {
                if let Some(value) = value.filter(|value| value.is_finite()) {
                    self.submit(
                        SettingsCommand::SetContainer {
                            content_id: key.content,
                            container_id: key.container,
                            change: ContainerChange::Opacity(Some(value as f32)),
                        },
                        RequestKind::Immediate,
                        None,
                    );
                }
            }
            Message::ContainerToggle(key, field, value) => {
                let command = match field {
                    ContainerField::AutoHeight => SettingsCommand::SetContainer {
                        content_id: key.content,
                        container_id: key.container,
                        change: ContainerChange::AutoHeight(value),
                    },
                    ContainerField::Locked => SettingsCommand::SetContainer {
                        content_id: key.content,
                        container_id: key.container,
                        change: ContainerChange::Locked(value),
                    },
                    ContainerField::ExcludeFromQuickHide => SettingsCommand::SetContainer {
                        content_id: key.content,
                        container_id: key.container,
                        change: ContainerChange::ExcludeFromQuickHide(value),
                    },
                    ContainerField::PortalNavigate => SettingsCommand::SetContent {
                        content_id: key.content,
                        container_id: key.container,
                        change: ContentChange::PortalNavigate(value),
                    },
                    ContainerField::PortalTitleIcon => SettingsCommand::SetContent {
                        content_id: key.content,
                        container_id: key.container,
                        change: ContentChange::PortalTitleIcon(value),
                    },
                };
                self.submit(command, RequestKind::Immediate, None);
            }
            Message::TitleColorText(key, value) => {
                let Some(stamp) = self.view.as_ref().map(|view| view.stamp) else {
                    return;
                };
                let draft = self.title_color_drafts.entry(key).or_insert(TextDraft {
                    value: String::new(),
                    base: stamp,
                    generation: Uuid::new_v4(),
                    needs_review: false,
                    submitted: None,
                });
                draft.value = value;
                draft.generation = Uuid::new_v4();
                draft.submitted = None;
            }
            Message::ApplyTitleColor(key) => self.submit_text(TextField::TitleColor, key, false),
            Message::RetryTitleColor(key) => self.submit_text(TextField::TitleColor, key, true),
            Message::DiscardTitleColor(key) => {
                self.title_color_drafts.remove(&key);
            }
            Message::KeepUpdated(value) => self.submit(
                SettingsCommand::Rule {
                    change: RuleChange::KeepUpdated { value },
                },
                RequestKind::Immediate,
                None,
            ),
            Message::DefaultTarget(index) => {
                if let Some(target) = index.and_then(|index| self.collection_target(index)) {
                    self.submit(
                        SettingsCommand::Rule {
                            change: RuleChange::DefaultTarget { target },
                        },
                        RequestKind::Immediate,
                        None,
                    );
                }
            }
            Message::RuleEnabled(id, value) => {
                self.rule_action(RuleChange::SetEnabled { id, value })
            }
            Message::RuleMove(id, before) => self.rule_action(RuleChange::Move { id, before }),
            Message::RuleDelete(id) => self.ask_confirmation(SettingsCommand::Rule {
                change: RuleChange::Delete { id },
            }),
            Message::RuleEdit(id) => self.open_rule_draft(Some(id)),
            Message::RuleNew => self.open_rule_draft(None),
            Message::RuleName(value) => self.update_rule_draft(|draft| draft.name = value),
            Message::RuleTarget(index) => {
                let target = index.and_then(|index| self.collection_target(index));
                if let Some(target) = target {
                    self.update_rule_draft(|draft| draft.target = Some(target));
                }
            }
            Message::RuleEnabledDraft(value) => {
                self.update_rule_draft(|draft| draft.enabled = value)
            }
            Message::ConditionKind(index) => self.update_rule_draft(|draft| {
                draft.form.kind = rule_editor::ConditionKind::from_index(index);
                draft.form.reset_values();
            }),
            Message::ConditionText(value) => {
                self.update_rule_draft(|draft| draft.form.text = value)
            }
            Message::ConditionOperation(index) => {
                self.update_rule_draft(|draft| draft.form.set_operation_index(index))
            }
            Message::ConditionType(category, checked) => self
                .update_rule_draft(|draft| set_selected(&mut draft.form.types, category, checked)),
            Message::ConditionWeekday(day, checked) => {
                self.update_rule_draft(|draft| set_selected(&mut draft.form.weekdays, day, checked))
            }
            Message::ConditionOrigin(index) => {
                self.update_rule_draft(|draft| draft.form.set_origin_index(index))
            }
            Message::ConditionMinimum(value) => {
                self.update_rule_draft(|draft| draft.form.minimum = value)
            }
            Message::ConditionMaximum(value) => {
                self.update_rule_draft(|draft| draft.form.maximum = value)
            }
            Message::ConditionFrom(value) => {
                self.update_rule_draft(|draft| draft.form.time_from = value)
            }
            Message::ConditionTo(value) => {
                self.update_rule_draft(|draft| draft.form.time_to = value)
            }
            Message::ConditionIdle(value) => {
                self.update_rule_draft(|draft| draft.form.idle_days = value)
            }
            Message::AddCondition => self.add_rule_condition(),
            Message::EditCondition(id) => {
                if let Some(draft) = &mut self.rule_draft {
                    draft.edit_condition(id);
                }
            }
            Message::RemoveCondition(id) => {
                if let Some(draft) = &mut self.rule_draft {
                    draft.conditions.retain(|item| item.id != id);
                    if draft.editing_condition == Some(id) {
                        draft.editing_condition = None;
                        draft.form.reset_values();
                    }
                    draft.touch();
                }
            }
            Message::SaveRule => self.save_rule_draft(),
            Message::CancelRule | Message::DiscardRule => self.rule_draft = None,
            Message::RetryRule => self.retry_rule_draft(),
            Message::SnapshotName(value) => self.snapshot_name = value,
            Message::MonitorFirst(index) => {
                self.monitor_first = index.and_then(|index| {
                    self.view
                        .as_ref()?
                        .monitors
                        .get(index)
                        .map(|monitor| monitor.id.clone())
                });
            }
            Message::MonitorSecond(index) => {
                self.monitor_second = index.and_then(|index| {
                    self.view
                        .as_ref()?
                        .monitors
                        .get(index)
                        .map(|monitor| monitor.id.clone())
                });
            }
            Message::AskAction(action) => self.ask_confirmation(SettingsCommand::Action { action }),
            Message::DialogClosed(token, result) => self.finish_confirmation(token, result),
            #[cfg(test)]
            Message::CancelAction => self.confirm_action = None,
            Message::RunAction(action) => self.submit_action(action),
        }
    }

    pub(super) fn set_title_draft(&mut self, value: String) {
        let Some(key) = self.selected_content else {
            return;
        };
        let Some(stamp) = self.view.as_ref().map(|view| view.stamp) else {
            return;
        };
        let initial = self
            .content(key)
            .map(|content| content.title.clone())
            .unwrap_or_default();
        let draft = self.title_drafts.entry(key).or_insert(TextDraft {
            value: initial,
            base: stamp,
            generation: Uuid::new_v4(),
            needs_review: false,
            submitted: None,
        });
        draft.value = value;
        draft.generation = Uuid::new_v4();
        draft.submitted = None;
    }

    pub(super) fn submit_title(&mut self, retry: bool) {
        let Some(key) = self.selected_content else {
            return;
        };
        self.submit_text(TextField::Title, key, retry);
    }

    pub(super) fn text_drafts(&mut self, field: TextField) -> &mut HashMap<ContentKey, TextDraft> {
        match field {
            TextField::Title => &mut self.title_drafts,
            TextField::TitleColor => &mut self.title_color_drafts,
        }
    }

    pub(super) fn submit_text(&mut self, field: TextField, key: ContentKey, retry: bool) {
        let Some(latest) = self.view.as_ref().map(|view| view.stamp) else {
            return;
        };
        if !self.view.as_ref().is_some_and(|view| {
            view.contents
                .iter()
                .any(|content| Self::key(content) == key)
        }) {
            self.notice = Some(Notice {
                text: tr("该内容已移动或删除；草稿保留，请重新选择目标。"),
                error: true,
            });
            return;
        }
        let Some(draft) = self.text_drafts(field).get_mut(&key) else {
            return;
        };
        if draft.needs_review && !retry {
            self.notice = Some(Notice {
                text: tr("文本草稿基于较旧的修订；请检查后明确重试。"),
                error: true,
            });
            return;
        }
        if retry && !draft.needs_review {
            return;
        }
        if draft.base.workspace != latest.workspace {
            self.notice = Some(Notice {
                text: tr("文本草稿属于另一工作区；草稿保留，但不能自动重试。"),
                error: true,
            });
            return;
        }
        let title = draft.value.trim().to_owned();
        let command = match field {
            TextField::Title => {
                if title.is_empty() {
                    self.notice = Some(Notice {
                        text: tr("名称不能为空。"),
                        error: true,
                    });
                    return;
                }
                SettingsCommand::SetContent {
                    content_id: key.content,
                    container_id: key.container,
                    change: ContentChange::Title(title),
                }
            }
            TextField::TitleColor => {
                let Some(rgb) = parse_rgb(&title) else {
                    self.notice = Some(Notice {
                        text: tr("标题颜色请使用六位十六进制 RGB 值，例如 #4A90E2。"),
                        error: true,
                    });
                    return;
                };
                SettingsCommand::SetContainer {
                    content_id: key.content,
                    container_id: key.container,
                    change: ContainerChange::TitleColor(
                        pecofence_core::settings_protocol::TitleColor::Custom(rgb),
                    ),
                }
            }
        };
        if retry {
            draft.base = latest;
            draft.needs_review = false;
        }
        let generation = draft.generation;
        let base = draft.base;
        self.submit(
            command,
            RequestKind::TextDraft(field, key, generation),
            Some(base),
        );
    }

    pub(super) fn update_rule_draft(&mut self, edit: impl FnOnce(&mut rule_editor::Draft)) {
        if let Some(draft) = &mut self.rule_draft {
            edit(draft);
            draft.touch();
        }
    }

    pub(super) fn add_rule_condition(&mut self) {
        let Some(draft) = &mut self.rule_draft else {
            return;
        };
        match draft.form.to_condition() {
            Ok(condition)
                if draft.conditions.len() < 32
                    || draft
                        .editing_condition
                        .is_some_and(|id| draft.conditions.iter().any(|entry| entry.id == id)) =>
            {
                draft.upsert_condition(condition);
                draft.touch();
                self.notice = None;
            }
            Ok(_) => {
                self.notice = Some(Notice {
                    text: tr("每条规则最多添加 32 个条件。"),
                    error: true,
                })
            }
            Err(error) => {
                self.notice = Some(Notice {
                    text: error,
                    error: true,
                })
            }
        }
    }

    pub(super) fn open_rule_draft(&mut self, id: Option<RuleId>) {
        if self.rule_draft.is_some() {
            self.notice = Some(Notice {
                text: tr("规则草稿尚未提交。"),
                error: true,
            });
            return;
        }
        self.rule_draft = self.view.as_ref().and_then(|view| match id {
            Some(id) => view
                .rules
                .list
                .iter()
                .find(|rule| rule.id == id)
                .map(|rule| rule_editor::Draft::edit(view.stamp, rule)),
            None => Some(rule_editor::Draft::new(view.stamp)),
        });
    }

    pub(super) fn save_rule_draft(&mut self) {
        let Some(draft) = &self.rule_draft else {
            return;
        };
        if self.inflight.iter().chain(self.pending.iter()).any(|pending| {
            matches!(pending.kind, RequestKind::RuleDraft(token, _) if token == draft.token)
        }) {
            self.notice = Some(Notice {
                text: tr("正在应用更改…"),
                error: false,
            });
            return;
        }
        if draft.needs_review {
            self.notice = Some(Notice {
                text: tr("请先检查草稿并明确重试，不能覆盖较新的工作区修订。"),
                error: true,
            });
            return;
        }
        let Some(target) = draft.target else {
            self.notice = Some(Notice {
                text: tr("请选择一个文件集合目标。"),
                error: true,
            });
            return;
        };
        let conditions = draft
            .conditions
            .iter()
            .map(|condition| condition.condition.clone())
            .collect::<Vec<_>>();
        if conditions.is_empty() {
            self.notice = Some(Notice {
                text: tr("请添加至少一个 AND 条件。"),
                error: true,
            });
            return;
        }
        let id = draft.rule_id.unwrap_or(draft.token);
        let rule_draft = RuleCommandDraft {
            name: draft.name.trim().to_owned(),
            enabled: draft.enabled,
            target,
            all_of: conditions,
            priority_class: draft.priority_class(),
        };
        let change = if draft.rule_id.is_some() {
            RuleChange::Edit {
                id,
                draft: rule_draft,
            }
        } else {
            RuleChange::Create {
                id,
                draft: rule_draft,
            }
        };
        let token = draft.token;
        let generation = draft.generation;
        let base = draft.base;
        self.submit(
            SettingsCommand::Rule { change },
            RequestKind::RuleDraft(token, generation),
            Some(base),
        );
    }

    pub(super) fn retry_rule_draft(&mut self) {
        let Some(stamp) = self.view.as_ref().map(|view| view.stamp) else {
            return;
        };
        let Some(draft) = &mut self.rule_draft else {
            return;
        };
        if !draft.needs_review {
            return;
        }
        if draft.base.workspace != stamp.workspace {
            self.notice = Some(Notice {
                text: tr("规则草稿属于另一工作区；请在当前工作区重新创建。"),
                error: true,
            });
            return;
        }
        draft.base = stamp;
        draft.needs_review = false;
        self.save_rule_draft();
    }
}

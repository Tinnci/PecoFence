use super::super::*;
use super::primitives::*;
use pecofence_core::settings_protocol::Action;
use windows_reactor::{
    Border, Button, ComboBox, Grid, GridLength, HorizontalAlignment, StackPanel, TextBlock,
    TextBox, ThemeBrush, TooltipExt, VerticalAlignment, View, ViewContext,
};

pub(super) fn layout(
    component: &SettingsComponent,
    view: &SettingsView,
    context: &mut ViewContext<SettingsComponent>,
) -> View {
    let mut rows = vec![storage_hero(component, view, context)];

    rows.push(section("布局快照"));
    let mut snapshots = vec![
        row(
            "快照名称（可选）",
            buttons(vec![
                TextBox::new(component.snapshot_name.clone())
                    .width(200.0)
                    .placeholder_text(tr("快照名称（可选）"))
                    .automation_name(tr("快照名称（可选）"))
                    .is_enabled(view.writable)
                    .on_text_changed(context.callback(|value: std::rc::Rc<str>| {
                        Message::SnapshotName(value.to_string())
                    }))
                    .into(),
                Button::new()
                    .content(tr("保存当前布局快照"))
                    .style(windows_reactor::ButtonStyle::Accent)
                    .is_enabled(view.writable)
                    .on_click(context.message(Message::RunAction(Action::SaveSnapshot {
                        name: component.snapshot_name.clone(),
                    })))
                    .into(),
            ]),
        )
        .icon(glyph::CAMERA),
    ];
    if view.snapshots.is_empty() {
        snapshots.push(
            Row::new(tr("尚无布局快照。"), None)
                .icon(glyph::HISTORY)
                .description(tr("保存快照后，可以随时回到当前的栅栏排布。")),
        );
    }
    for snapshot in &view.snapshots {
        let id = snapshot.id;
        snapshots.push(
            Row::new(
                snapshot.name.clone(),
                Some(buttons(vec![
                    Button::new()
                        .content(tr("恢复"))
                        .automation_name(pecofence_core::i18n::format(
                            "恢复：{0}",
                            std::slice::from_ref(&snapshot.name),
                        ))
                        .is_enabled(view.writable)
                        .on_click(
                            context.message(Message::AskAction(Action::RestoreSnapshot { id })),
                        )
                        .into(),
                    Button::new()
                        .content(tr("删除"))
                        .style(windows_reactor::ButtonStyle::Subtle)
                        .automation_name(pecofence_core::i18n::format(
                            "删除：{0}",
                            std::slice::from_ref(&snapshot.name),
                        ))
                        .is_enabled(view.writable)
                        .on_click(
                            context.message(Message::AskAction(Action::DeleteSnapshot { id })),
                        )
                        .into(),
                ])),
            )
            .icon(glyph::HISTORY)
            .description(format!(
                "{} · {} {}",
                snapshot.date,
                snapshot.content_count,
                tr("项")
            )),
        );
    }
    rows.push(group(snapshots));

    rows.push(section("配置与恢复"));
    let backup_row = |path: &std::path::PathBuf, context: &mut ViewContext<SettingsComponent>| {
        let name = backup_name(path);
        Row::new(
            name.clone(),
            Some(
                Button::new()
                    .content(tr("恢复"))
                    .automation_name(pecofence_core::i18n::format(
                        "恢复：{0}",
                        std::slice::from_ref(&name),
                    ))
                    .on_click(context.message(Message::AskAction(Action::RestoreBackup {
                        path: path.clone(),
                        confirmed: false,
                    })))
                    .tooltip(path.display().to_string()),
            ),
        )
        .icon(glyph::RECENT)
    };
    let mut files = vec![
        row(
            "配置文件",
            buttons(vec![
                Button::new()
                    .content(tr("导出配置…"))
                    .on_click(context.message(Message::RunAction(Action::ExportConfig)))
                    .into(),
                Button::new()
                    .content(tr("导入配置…"))
                    .on_click(context.message(Message::AskAction(Action::ImportConfig {
                        confirmed: false,
                    })))
                    .into(),
            ]),
        )
        .icon(glyph::EXPORT)
        .description(view.config_path.display().to_string()),
    ];
    for path in view.backups.iter().take(RECENT_BACKUPS) {
        files.push(backup_row(path, context));
    }
    rows.push(group(files));
    let older: Vec<Row> = view
        .backups
        .iter()
        .skip(RECENT_BACKUPS)
        .map(|path| backup_row(path, context))
        .collect();
    if !older.is_empty() {
        rows.push(
            windows_reactor::Expander::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .horizontal_content_alignment(HorizontalAlignment::Stretch)
                .header(
                    StackPanel::new()
                        .orientation(windows_reactor::Orientation::Horizontal)
                        .spacing(16.0)
                        .children((
                            Border::new()
                                .margin(windows_reactor::Thickness {
                                    left: 4.0,
                                    top: 0.0,
                                    right: 0.0,
                                    bottom: 0.0,
                                })
                                .content(icon(glyph::RECENT, 18.0)),
                            TextBlock::new()
                                .text(pecofence_core::i18n::format(
                                    "更早的备份（{0}）",
                                    &[older.len().to_string()],
                                ))
                                .vertical_alignment(VerticalAlignment::Center),
                        )),
                )
                .content(group(older))
                .into(),
        );
    }

    use pecofence_core::default_template::DefaultPresetOperation;
    rows.push(section("默认布局与小组件"));
    rows.push(note(
        "左侧放置时钟、日历、便笺和待办，右侧整理文件；空间不足时使用标签组。恢复前自动保存快照，不清空内容。",
    ));
    rows.push(group(
        [
            (
                "恢复默认排布",
                "恢复",
                glyph::TILES,
                DefaultPresetOperation::RestoreLayout,
            ),
            (
                "补齐默认组件",
                "补齐",
                glyph::ADD,
                DefaultPresetOperation::AddMissingWidgets,
            ),
            (
                "恢复默认外观",
                "恢复",
                glyph::COLOR,
                DefaultPresetOperation::RestoreAppearance,
            ),
        ]
        .into_iter()
        .map(|(label, verb, glyph, operation)| {
            row(
                label,
                Button::new()
                    .content(tr(verb))
                    .automation_name(tr(label))
                    .is_enabled(view.writable)
                    .on_click(
                        context.message(Message::AskAction(Action::ApplyDefaultPreset {
                            operation,
                            confirmed: false,
                        })),
                    )
                    .into(),
            )
            .icon(glyph)
        })
        .collect::<Vec<_>>(),
    ));

    rows.push(section("多显示器"));
    if view.monitors.len() < 2 {
        rows.push(group([Row::new(
            tr("检测到的显示器不足两个，无法交换布局。"),
            None,
        )
        .icon(glyph::MONITOR)]));
    } else {
        let labels = view
            .monitors
            .iter()
            .map(|monitor| monitor.label.clone())
            .collect::<Vec<_>>();
        let first = component
            .monitor_first
            .as_ref()
            .and_then(|id| view.monitors.iter().position(|monitor| &monitor.id == id));
        let second = component
            .monitor_second
            .as_ref()
            .and_then(|id| view.monitors.iter().position(|monitor| &monitor.id == id));
        let monitor_combo =
            |label: &'static str,
             selected: Option<usize>,
             callback: windows_reactor::Callback<Option<usize>>| {
                row(
                    label,
                    ComboBox::new()
                        .width(ROW_CONTROL_WIDTH)
                        .automation_name(tr(label))
                        .items_source(labels.clone())
                        .selected_index(selected)
                        .on_selection_changed(callback)
                        .into(),
                )
                .icon(glyph::MONITOR)
            };
        rows.push(group([
            monitor_combo(
                "第一个显示器",
                first,
                context.callback(Message::MonitorFirst),
            ),
            monitor_combo(
                "第二个显示器",
                second,
                context.callback(Message::MonitorSecond),
            ),
            Row::new(
                String::new(),
                Some(
                    Button::new()
                        .content(tr("交换显示器布局"))
                        .style(windows_reactor::ButtonStyle::Accent)
                        .is_enabled(
                            view.writable && first.is_some() && second.is_some() && first != second,
                        )
                        .on_click(context.message(Message::RunAction(Action::SwapMonitors {
                            first: component.monitor_first.clone().unwrap_or_default(),
                            second: component.monitor_second.clone().unwrap_or_default(),
                        })))
                        .into(),
                ),
            )
            .icon(glyph::SWITCH),
        ]));
    }
    StackPanel::new().spacing(8.0).children(rows).into()
}

/// A daily backup's display name: its date (`2026-10-21.json` → `2026-10-21`).
fn backup_name(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The page's hero: whether the workspace is safely saved, at a glance, with the one action
/// that fits the state (save now, retry, cancel the exit wait, or the recovery choices).
fn storage_hero(
    component: &SettingsComponent,
    view: &SettingsView,
    context: &mut ViewContext<SettingsComponent>,
) -> View {
    let issue = view.load_issue.as_ref().or(view.save_issue.as_ref());
    let busy = view.saving
        || view.closing
        || component.inflight.is_some()
        || !component.pending.is_empty();
    let glyph = if issue.is_some() {
        glyph::ERROR
    } else if busy {
        glyph::SYNC
    } else if view.dirty {
        glyph::RECENT
    } else {
        glyph::CHECK
    };
    let headline = if view.load_issue.is_some() {
        tr("恢复模式：工作区只读")
    } else {
        component.status_text(Some(view))
    };
    let mut lines: Vec<View> = vec![
        TextBlock::new()
            .text(headline)
            .font_size(20.0)
            .font_weight(windows_reactor::FontWeight::SEMI_BOLD)
            .text_wrapping(windows_reactor::TextWrapping::Wrap)
            .into(),
    ];
    if let Some(issue) = issue {
        lines.push(supporting(issue));
    }
    let mut facts = Vec::new();
    if let Some(latest) = view.backups.first() {
        facts.push(pecofence_core::i18n::format(
            "最近备份：{0}",
            &[backup_name(latest)],
        ));
    }
    facts.push(pecofence_core::i18n::format(
        "{0} 个每日备份 · {1} 个布局快照",
        &[
            view.backups.len().to_string(),
            view.snapshots.len().to_string(),
        ],
    ));
    lines.push(supporting(facts.join(" · ")));

    let action = |label: &'static str, message: Message, accent: bool| -> View {
        let button = Button::new()
            .content(tr(label))
            .on_click(context.message(message));
        if accent {
            button.style(windows_reactor::ButtonStyle::Accent).into()
        } else {
            button.into()
        }
    };
    let open_folder = || Message::RunAction(Action::OpenConfigFolder);
    let mut actions: Vec<Vec<View>> = Vec::new();
    if view.load_issue.is_some() {
        actions.push(vec![
            action("打开配置目录", open_folder(), true),
            action(
                "导出可用配置",
                Message::RunAction(Action::ExportConfig),
                false,
            ),
        ]);
        let mut recovery = vec![
            action(
                "导入配置…",
                Message::AskAction(Action::ImportConfig { confirmed: false }),
                false,
            ),
            action(
                "新建工作区…",
                Message::AskAction(Action::NewWorkspace { confirmed: false }),
                false,
            ),
        ];
        if view.recovered_from.is_some() {
            recovery.push(action(
                "接受恢复候选…",
                Message::AskAction(Action::AcceptRecovery { confirmed: false }),
                false,
            ));
        }
        actions.push(recovery);
    } else {
        let mut primary = Vec::new();
        if view.closing {
            primary.push(action(
                "取消退出等待",
                Message::RunAction(Action::CancelClose),
                true,
            ));
        } else if view.save_issue.is_some() || view.dirty {
            let label = if view.save_issue.is_some() {
                "重试保存"
            } else {
                "立即保存"
            };
            let save = action(label, Message::RunAction(Action::RetrySave), true);
            primary.push(save);
        }
        primary.push(action("打开配置目录", open_folder(), false));
        actions.push(primary);
    }
    for set in actions {
        lines.push(
            Border::new()
                .margin(windows_reactor::Thickness {
                    left: 0.0,
                    top: 6.0,
                    right: 0.0,
                    bottom: 0.0,
                })
                .content(buttons(set))
                .into(),
        );
    }
    Border::new()
        .background(if issue.is_some() {
            ThemeBrush::SystemCriticalBackground
        } else {
            ThemeBrush::CardBackground
        })
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(1.0)
        .corner_radius(12.0)
        .padding(20.0)
        .content(
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR])
                .column_spacing(16.0)
                .children((
                    tonal_icon(glyph, issue.is_some()),
                    StackPanel::new()
                        .grid_column(1)
                        .spacing(4.0)
                        .children(lines),
                )),
        )
        .into()
}

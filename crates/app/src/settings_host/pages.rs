use super::{ContentKey, Message, Page, SettingsComponent, SettingsView, parse_rgb, tr};
use crate::settings_ui::TINT_PALETTE;
use pecofence_core::settings_protocol::{
    Action, ContainerChange, ContentChange, SettingChange, TitleColor,
};
use pecofence_core::{Spacing, TitleSize};
use windows_reactor::{
    Button, CheckBox, ComboBox, NumberBox, Slider, StackPanel, TextBlock, TextBox, View,
    ViewContext,
};

pub(super) fn notice(error: bool, message: &str) -> View {
    StackPanel::new()
        .spacing(4.0)
        .children((
            TextBlock::new().text(if error {
                tr("需要检查")
            } else {
                tr("状态")
            }),
            TextBlock::new()
                .text(message)
                .text_wrapping(windows_reactor::TextWrapping::Wrap),
        ))
        .into()
}

pub(super) fn button(callback: windows_reactor::Callback<()>, label: &'static str) -> View {
    Button::new().content(tr(label)).on_click(callback).into()
}

pub(super) fn page(
    component: &SettingsComponent,
    projected: Option<&SettingsView>,
    context: &mut ViewContext<SettingsComponent>,
) -> View {
    let Some(view) = projected else {
        return TextBlock::new().text(tr("正在加载设置工作区…")).into();
    };
    match component.page {
        Page::General => general(view, context),
        Page::Content => content(component, view, context),
        Page::Rules => super::rule_editor::view(component, view, context),
        Page::Layout => layout(component, view, context),
        Page::About => about(component, view, context),
    }
}

fn general(view: &SettingsView, context: &mut ViewContext<SettingsComponent>) -> View {
    let s = &view.settings;
    let enabled = view.writable;
    let mut rows = vec![
        TextBlock::new()
            .text(tr("桌面与启动"))
            .font_size(20.0)
            .into(),
        toggle(
            "随 Windows 启动",
            s.autostart,
            enabled,
            context,
            SettingChange::Autostart,
        ),
        toggle(
            "隐藏 Windows 桌面图标",
            s.hide_real_icons,
            enabled,
            context,
            SettingChange::HideRealIcons,
        ),
        toggle(
            "双击桌面空白处快速隐藏",
            s.quick_hide.enabled,
            enabled,
            context,
            SettingChange::QuickHideEnabled,
        ),
        field_combo(
            "显示桌面（Win+D）时",
            &["保持栅栏可见", "随桌面隐藏"],
            usize::from(s.show_desktop == pecofence_core::ShowDesktopSetting::HideWithDesktop),
            enabled,
            context.callback(|index| {
                Message::Setting(SettingChange::ShowDesktop(if index == Some(1) {
                    pecofence_core::ShowDesktopSetting::HideWithDesktop
                } else {
                    pecofence_core::ShowDesktopSetting::KeepVisible
                }))
            }),
        ),
        TextBlock::new().text(tr("外观")).font_size(20.0).into(),
        field_combo(
            "主题风格",
            &["Fluent", "Liquid Glass"],
            usize::from(s.theme_style == pecofence_core::ThemeStyle::LiquidGlass),
            enabled,
            context.callback(|index| {
                Message::Setting(SettingChange::ThemeStyle(if index == Some(1) {
                    pecofence_core::ThemeStyle::LiquidGlass
                } else {
                    pecofence_core::ThemeStyle::Fluent
                }))
            }),
        ),
        field_combo(
            "颜色模式",
            &["跟随 Windows 模式", "跟随应用模式", "浅色", "深色"],
            match s.theme {
                pecofence_core::ThemeSetting::FollowWindowsMode => 0,
                pecofence_core::ThemeSetting::FollowAppMode => 1,
                pecofence_core::ThemeSetting::Light => 2,
                pecofence_core::ThemeSetting::Dark => 3,
            },
            enabled,
            context.callback(|index: Option<usize>| {
                Message::Setting(SettingChange::Theme(match index.unwrap_or(0) {
                    1 => pecofence_core::ThemeSetting::FollowAppMode,
                    2 => pecofence_core::ThemeSetting::Light,
                    3 => pecofence_core::ThemeSetting::Dark,
                    _ => pecofence_core::ThemeSetting::FollowWindowsMode,
                }))
            }),
        ),
        field_combo(
            "显示语言",
            &[
                "跟随系统",
                "简体中文",
                "繁體中文",
                "English",
                "日本語",
                "한국어",
                "Deutsch",
                "Français",
                "Español",
                "Português (Brasil)",
                "Русский",
            ],
            language_index(s.language),
            enabled,
            context.callback(|index: Option<usize>| {
                Message::Setting(SettingChange::Language(language_at(index.unwrap_or(0))))
            }),
        ),
        field_combo(
            "默认图标大小",
            &["32", "48", "64", "96"],
            match s.icon_size {
                32 => 0,
                48 => 1,
                64 => 2,
                96 => 3,
                _ => 1,
            },
            enabled,
            context.callback(|index: Option<usize>| {
                Message::Setting(SettingChange::IconSize(
                    [32, 48, 64, 96][index.unwrap_or(1).min(3)],
                ))
            }),
        ),
        TextBlock::new().text(tr("图标着色")).into(),
    ];
    let current_tint = s.icons.tint_rgb;
    let mut tint_items = vec![tr("不着色")];
    tint_items.extend(TINT_PALETTE.iter().map(|(name, _)| tr(name)));
    let custom_tint =
        current_tint.filter(|rgb| !TINT_PALETTE.iter().any(|(_, color)| color == rgb));
    if let Some(rgb) = custom_tint {
        tint_items.push(format!(
            "{} #{:02X}{:02X}{:02X}",
            tr("自定义"),
            rgb[0],
            rgb[1],
            rgb[2]
        ));
    }
    let tint_index = current_tint
        .and_then(|rgb| {
            TINT_PALETTE
                .iter()
                .position(|(_, value)| *value == rgb)
                .map(|index| index + 1)
        })
        .or_else(|| custom_tint.map(|_| TINT_PALETTE.len() + 1))
        .unwrap_or(0);
    rows.push(
        ComboBox::new()
            .header(tr("图标着色"))
            .items_source(tint_items)
            .selected_index(Some(tint_index))
            .is_enabled(enabled)
            .on_selection_changed(context.callback(move |index| {
                let rgb = match index {
                    Some(0) | None => None,
                    Some(index) if index <= TINT_PALETTE.len() => {
                        TINT_PALETTE.get(index - 1).map(|(_, color)| *color)
                    }
                    Some(_) => current_tint,
                };
                Message::IconTint(rgb)
            }))
            .into(),
    );
    rows.push(TextBlock::new().text(tr("图标着色强度")).into());
    rows.push(
        Slider::new()
            .minimum(0.0)
            .maximum(1.0)
            .value(f64::from(s.icons.tint_strength))
            .is_enabled(enabled && s.icons.tint_rgb.is_some())
            .on_value_changed(
                context.callback(|value| {
                    Message::Setting(SettingChange::IconTintStrength(value as f32))
                }),
            )
            .into(),
    );
    rows.extend([
        toggle(
            "图标融入背景（Chameleon）",
            s.icons.chameleon,
            enabled,
            context,
            SettingChange::Chameleon,
        ),
        toggle(
            "卷起的栅栏悬停时自动展开",
            s.roll_up.hover_peek,
            enabled,
            context,
            SettingChange::HoverPeek,
        ),
        toggle(
            "卷起的栅栏需点击标题才展开",
            s.roll_up.click_to_expand,
            enabled,
            context,
            SettingChange::ClickToExpand,
        ),
        toggle(
            "鼠标悬停时才显示标题栏",
            s.roll_up.title_on_hover,
            enabled,
            context,
            SettingChange::TitleOnHover,
        ),
        toggle(
            "不活动时隐藏滚动条",
            s.roll_up.hide_inactive_scrollbar,
            enabled,
            context,
            SettingChange::HideInactiveScrollbar,
        ),
        toggle(
            "移动栅栏时吸附对齐",
            s.snapping.enabled,
            enabled,
            context,
            SettingChange::SnappingEnabled,
        ),
        toggle(
            "启用 Peek 浮现栅栏",
            s.peek.enabled,
            enabled,
            context,
            SettingChange::PeekEnabled,
        ),
        field_combo(
            "Peek 快捷键",
            &["Ctrl + Alt + 空格", "Win + 空格", "Win + Shift + 空格"],
            match s.peek.hotkey {
                pecofence_core::PeekHotkey::CtrlAltSpace => 0,
                pecofence_core::PeekHotkey::WinSpace => 1,
                pecofence_core::PeekHotkey::WinShiftSpace => 2,
            },
            enabled,
            context.callback(|index: Option<usize>| {
                Message::Setting(SettingChange::PeekHotkey(match index.unwrap_or(0) {
                    1 => pecofence_core::PeekHotkey::WinSpace,
                    2 => pecofence_core::PeekHotkey::WinShiftSpace,
                    _ => pecofence_core::PeekHotkey::CtrlAltSpace,
                }))
            }),
        ),
        toggle(
            "Peek 时压暗背景",
            s.peek.dim,
            enabled,
            context,
            SettingChange::PeekDim,
        ),
    ]);
    StackPanel::new().spacing(10.0).children(rows).into()
}

fn toggle(
    label: &'static str,
    value: bool,
    enabled: bool,
    context: &mut ViewContext<SettingsComponent>,
    change: fn(bool) -> SettingChange,
) -> View {
    CheckBox::new()
        .content(tr(label))
        .is_checked(value)
        .is_enabled(enabled)
        .on_is_checked_changed(
            context.callback(move |value: Option<bool>| {
                Message::Setting(change(value.unwrap_or(false)))
            }),
        )
        .into()
}

fn field_combo(
    label: &'static str,
    choices: &[&'static str],
    selected: usize,
    enabled: bool,
    callback: windows_reactor::Callback<Option<usize>>,
) -> View {
    ComboBox::new()
        .header(tr(label))
        .items_source(choices.iter().map(|choice| tr(choice)))
        .selected_index(Some(selected))
        .is_enabled(enabled)
        .on_selection_changed(callback)
        .into()
}

fn language_index(language: pecofence_core::i18n::Language) -> usize {
    use pecofence_core::i18n::Language::*;
    match language {
        System => 0,
        SimplifiedChinese => 1,
        TraditionalChinese => 2,
        English => 3,
        Japanese => 4,
        Korean => 5,
        German => 6,
        French => 7,
        Spanish => 8,
        Portuguese => 9,
        Russian => 10,
    }
}

fn language_at(index: usize) -> pecofence_core::i18n::Language {
    use pecofence_core::i18n::Language::*;
    [
        System,
        SimplifiedChinese,
        TraditionalChinese,
        English,
        Japanese,
        Korean,
        German,
        French,
        Spanish,
        Portuguese,
        Russian,
    ]
    .get(index)
    .copied()
    .unwrap_or(System)
}

fn content(
    component: &SettingsComponent,
    view: &SettingsView,
    context: &mut ViewContext<SettingsComponent>,
) -> View {
    let mut rows = vec![TextBlock::new().text(tr("选择内容")).into()];
    if view.contents.is_empty() {
        rows.push(TextBlock::new().text(tr("当前没有可配置的内容。")).into());
        return StackPanel::new().spacing(10.0).children(rows).into();
    }
    let labels = view
        .contents
        .iter()
        .map(|item| format!("{} · {}", item.title, item.window_title))
        .collect::<Vec<_>>();
    let selected_index = component.selected_content.and_then(|selected| {
        view.contents.iter().position(|item| {
            item.content_id == selected.content && item.container_id == selected.container
        })
    });
    rows.push(
        ComboBox::new()
            .header(tr("内容与所属窗口"))
            .items_source(labels)
            .selected_index(selected_index)
            .on_selection_changed(context.callback(Message::SelectContent))
            .into(),
    );
    let Some(key) = component.selected_content else {
        rows.push(
            TextBlock::new()
                .text(tr("请重新选择仍然存在的内容目标。"))
                .into(),
        );
        return StackPanel::new().spacing(10.0).children(rows).into();
    };
    let Some(item) = view
        .contents
        .iter()
        .find(|item| item.content_id == key.content && item.container_id == key.container)
    else {
        rows.push(
            TextBlock::new()
                .text(tr("此内容已移动或删除。请重新选择。"))
                .into(),
        );
        return StackPanel::new().spacing(10.0).children(rows).into();
    };
    let title_draft = component.title_drafts.get(&key);
    rows.push(
        TextBox::new(title_draft.map_or_else(|| item.title.clone(), |draft| draft.value.clone()))
            .header(tr("内容名称"))
            .is_enabled(view.writable)
            .on_text_changed(
                context.callback(|value: std::rc::Rc<str>| Message::ContentText(value.to_string())),
            )
            .into(),
    );
    rows.push(
        Button::new()
            .content(tr("应用名称"))
            .is_enabled(view.writable)
            .on_click(context.message(Message::ApplyTitle))
            .into(),
    );
    let title_review = if title_draft.is_some_and(|draft| draft.needs_review) {
        vec![
            Button::new()
                .content(tr("检查后重试名称"))
                .on_click(context.message(Message::RetryTitle))
                .into(),
            Button::new()
                .content(tr("放弃名称草稿"))
                .on_click(context.message(Message::DiscardTitle))
                .into(),
        ]
    } else {
        vec![]
    };
    // Keep a stable slot so conflict controls do not recreate the inputs below.
    rows.push(StackPanel::new().spacing(8.0).children(title_review).into());
    rows.push(
        TextBlock::new()
            .text(format!("{}: {}", tr("所在窗口"), item.window_title))
            .into(),
    );
    rows.push(
        TextBlock::new()
            .text(format!(
                "{}: {}",
                tr("同一窗口的内容"),
                item.window_contents.join("、")
            ))
            .text_wrapping(windows_reactor::TextWrapping::Wrap)
            .into(),
    );
    rows.push(TextBlock::new().text(tr("内容选项")).font_size(20.0).into());
    let icon_sizes = ["32", "48", "64", "96"];
    rows.push(field_combo(
        "图标大小",
        &icon_sizes,
        match item.icon_size {
            32 => 0,
            48 => 1,
            64 => 2,
            96 => 3,
            _ => 1,
        },
        view.writable && item.is_file_view,
        context.callback({
            let key = ContentKey {
                content: item.content_id,
                container: item.container_id,
            };
            move |index: Option<usize>| {
                Message::Content(
                    key,
                    ContentChange::IconSize([32, 48, 64, 96][index.unwrap_or(1).min(3)]),
                )
            }
        }),
    ));
    rows.push(field_combo(
        "图标间距",
        &["紧凑", "标准", "宽松"],
        match item.spacing {
            Spacing::Compact => 0,
            Spacing::Normal => 1,
            Spacing::Loose => 2,
        },
        view.writable && item.is_file_view,
        context.callback({
            let key = ContentKey {
                content: item.content_id,
                container: item.container_id,
            };
            move |index: Option<usize>| {
                Message::Content(
                    key,
                    ContentChange::Spacing(match index.unwrap_or(1) {
                        0 => Spacing::Compact,
                        2 => Spacing::Loose,
                        _ => Spacing::Normal,
                    }),
                )
            }
        }),
    ));
    let key = ContentKey {
        content: item.content_id,
        container: item.container_id,
    };
    if let Some(portal) = &item.portal {
        rows.push(
            TextBlock::new()
                .text(tr("文件夹门户"))
                .font_size(20.0)
                .into(),
        );
        rows.push(container_toggle(
            "门户内双击打开子文件夹",
            portal.navigate,
            view.writable,
            key,
            super::ContainerField::PortalNavigate,
            context,
        ));
        rows.push(container_toggle(
            "标题栏显示文件夹图标",
            portal.title_icon,
            view.writable,
            key,
            super::ContainerField::PortalTitleIcon,
            context,
        ));
    }
    rows.push(
        TextBlock::new()
            .text(tr("整个窗口（包括所有标签页）"))
            .font_size(20.0)
            .into(),
    );
    rows.push(
        NumberBox::new()
            .header(tr("不透明度"))
            .minimum(0.1)
            .maximum(2.0)
            .value(Some(f64::from(item.opacity.unwrap_or(1.0))))
            .is_enabled(view.writable)
            .on_value_changed(
                context.callback(move |value: Option<f64>| Message::Opacity(key, value)),
            )
            .into(),
    );
    rows.push(
        Button::new()
            .content(tr("恢复默认不透明度"))
            .is_enabled(view.writable)
            .on_click(context.message(Message::Container(key, ContainerChange::Opacity(None))))
            .into(),
    );
    let mut tint_items = vec![tr("无")];
    tint_items.extend(TINT_PALETTE.iter().map(|(name, _)| tr(name)));
    let current_tint = item.tint;
    let tint_index = item
        .tint
        .and_then(|rgb| {
            TINT_PALETTE
                .iter()
                .position(|(_, color)| *color == rgb)
                .map(|index| index + 1)
        })
        .unwrap_or_else(|| {
            if let Some(rgb) = current_tint {
                tint_items.push(format!("{} ({})", tr("自定义"), format_rgb(rgb)));
                tint_items.len() - 1
            } else {
                0
            }
        });
    rows.push(
        ComboBox::new()
            .header(tr("窗口色调"))
            .items_source(tint_items)
            .selected_index(Some(tint_index))
            .is_enabled(view.writable)
            .on_selection_changed(context.callback(move |index: Option<usize>| {
                let tint = match index {
                    Some(0) | None => None,
                    Some(index) if index <= TINT_PALETTE.len() => Some(TINT_PALETTE[index - 1].1),
                    Some(_) => current_tint,
                };
                Message::Container(key, ContainerChange::Tint(tint))
            }))
            .into(),
    );
    let title_color_index = match item.title_color {
        TitleColor::Theme => Some(0),
        TitleColor::Tint => Some(1),
        TitleColor::White => Some(2),
        TitleColor::Black => Some(3),
        TitleColor::Custom(_) => None,
    };
    rows.push(
        ComboBox::new()
            .header(tr("标题颜色"))
            .items_source([tr("跟随主题"), tr("跟随色调"), tr("白色"), tr("黑色")])
            .selected_index(title_color_index)
            .is_enabled(view.writable)
            .on_selection_changed(context.callback(move |index: Option<usize>| {
                Message::Container(
                    key,
                    ContainerChange::TitleColor(match index.unwrap_or(0) {
                        1 => TitleColor::Tint,
                        2 => TitleColor::White,
                        3 => TitleColor::Black,
                        _ => TitleColor::Theme,
                    }),
                )
            }))
            .into(),
    );
    let custom_rgb = component
        .title_color_drafts
        .get(&key)
        .map(|draft| draft.value.clone())
        .unwrap_or_else(|| match item.title_color {
            TitleColor::Custom(rgb) => format_rgb(rgb),
            _ => String::new(),
        });
    rows.push(
        TextBox::new(custom_rgb.clone())
            .header(tr("自定义标题颜色（RRGGBB）"))
            .is_enabled(view.writable)
            .on_text_changed(context.callback(move |value: std::rc::Rc<str>| {
                Message::TitleColorText(key, value.to_string())
            }))
            .into(),
    );
    rows.push(
        Button::new()
            .content(tr("应用自定义标题颜色"))
            .is_enabled(view.writable && parse_rgb(&custom_rgb).is_some())
            .on_click(context.message(Message::ApplyTitleColor(key)))
            .into(),
    );
    let color_review = if component
        .title_color_drafts
        .get(&key)
        .is_some_and(|draft| draft.needs_review)
    {
        vec![
            Button::new()
                .content(tr("检查后重试"))
                .on_click(context.message(Message::RetryTitleColor(key)))
                .into(),
            Button::new()
                .content(tr("取消"))
                .on_click(context.message(Message::DiscardTitleColor(key)))
                .into(),
        ]
    } else {
        vec![]
    };
    rows.push(StackPanel::new().spacing(8.0).children(color_review).into());
    rows.push(field_combo(
        "标题字号",
        &["小", "标准", "大"],
        match item.title_size {
            TitleSize::Small => 0,
            TitleSize::Normal => 1,
            TitleSize::Large => 2,
        },
        view.writable,
        context.callback(move |index: Option<usize>| {
            Message::Container(
                key,
                ContainerChange::TitleSize(match index.unwrap_or(1) {
                    0 => TitleSize::Small,
                    2 => TitleSize::Large,
                    _ => TitleSize::Normal,
                }),
            )
        }),
    ));
    rows.extend([
        container_toggle(
            "自动高度",
            item.auto_height,
            view.writable,
            key,
            super::ContainerField::AutoHeight,
            context,
        ),
        container_toggle(
            "锁定位置和大小",
            item.locked,
            view.writable,
            key,
            super::ContainerField::Locked,
            context,
        ),
        container_toggle(
            "快速隐藏时保持可见",
            item.exclude_from_quick_hide,
            view.writable,
            key,
            super::ContainerField::ExcludeFromQuickHide,
            context,
        ),
        Button::new()
            .content(tr("停靠到屏幕顶部"))
            .is_enabled(view.writable)
            .on_click(context.message(Message::Container(key, ContainerChange::DockTop)))
            .into(),
    ]);
    StackPanel::new().spacing(10.0).children(rows).into()
}

fn container_toggle(
    label: &'static str,
    value: bool,
    enabled: bool,
    key: ContentKey,
    field: super::ContainerField,
    context: &mut ViewContext<SettingsComponent>,
) -> View {
    CheckBox::new()
        .content(tr(label))
        .is_checked(value)
        .is_enabled(enabled)
        .on_is_checked_changed(context.callback(move |value: Option<bool>| {
            Message::ContainerToggle(key, field, value.unwrap_or(false))
        }))
        .into()
}

fn layout(
    component: &SettingsComponent,
    view: &SettingsView,
    context: &mut ViewContext<SettingsComponent>,
) -> View {
    let mut rows = vec![
        TextBlock::new().text(tr("布局快照")).font_size(20.0).into(),
        TextBox::new(component.snapshot_name.clone())
            .header(tr("快照名称（可选）"))
            .is_enabled(view.writable)
            .on_text_changed(
                context
                    .callback(|value: std::rc::Rc<str>| Message::SnapshotName(value.to_string())),
            )
            .into(),
        Button::new()
            .content(tr("保存当前布局快照"))
            .is_enabled(view.writable)
            .on_click(context.message(Message::RunAction(Action::SaveSnapshot {
                name: component.snapshot_name.clone(),
            })))
            .into(),
    ];
    if view.snapshots.is_empty() {
        rows.push(TextBlock::new().text(tr("尚无布局快照。")).into());
    }
    rows.push(
        StackPanel::new()
            .spacing(6.0)
            .keyed_children(view.snapshots.iter().map(|snapshot| {
                let id = snapshot.id;
                windows_reactor::keyed(
                    id.to_string(),
                    StackPanel::new().spacing(4.0).children((
                        TextBlock::new().text(snapshot.name.clone()),
                        TextBlock::new().text(format!(
                            "{} · {} {}",
                            snapshot.date,
                            snapshot.content_count,
                            tr("项")
                        )),
                        Button::new()
                            .content(tr("恢复"))
                            .is_enabled(view.writable)
                            .on_click(
                                context.message(Message::AskAction(Action::RestoreSnapshot { id })),
                            ),
                        Button::new()
                            .content(tr("删除"))
                            .is_enabled(view.writable)
                            .on_click(
                                context.message(Message::AskAction(Action::DeleteSnapshot { id })),
                            ),
                    )),
                )
            }))
            .into(),
    );
    rows.push(TextBlock::new().text(tr("多显示器")).font_size(20.0).into());
    if view.monitors.len() < 2 {
        rows.push(
            TextBlock::new()
                .text(tr("检测到的显示器不足两个，无法交换布局。"))
                .into(),
        );
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
        rows.push(
            ComboBox::new()
                .header(tr("第一个显示器"))
                .items_source(labels.clone())
                .selected_index(first)
                .on_selection_changed(context.callback(Message::MonitorFirst))
                .into(),
        );
        rows.push(
            ComboBox::new()
                .header(tr("第二个显示器"))
                .items_source(labels)
                .selected_index(second)
                .on_selection_changed(context.callback(Message::MonitorSecond))
                .into(),
        );
        rows.push(
            Button::new()
                .content(tr("交换显示器布局"))
                .is_enabled(view.writable && first.is_some() && second.is_some() && first != second)
                .on_click(context.message(Message::RunAction(Action::SwapMonitors {
                    first: component.monitor_first.clone().unwrap_or_default(),
                    second: component.monitor_second.clone().unwrap_or_default(),
                })))
                .into(),
        );
    }
    rows.extend([
        TextBlock::new()
            .text(tr("配置与恢复"))
            .font_size(20.0)
            .into(),
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
        TextBlock::new().text(tr("可用备份")).into(),
    ]);
    for path in &view.backups {
        rows.push(
            TextBlock::new()
                .text(path.display().to_string())
                .text_wrapping(windows_reactor::TextWrapping::Wrap)
                .into(),
        );
        rows.push(
            Button::new()
                .content(tr("恢复此备份"))
                .on_click(context.message(Message::AskAction(Action::RestoreBackup {
                    path: path.clone(),
                    confirmed: false,
                })))
                .into(),
        );
    }
    if let Some(issue) = &view.load_issue {
        rows.push(
            TextBlock::new()
                .text(tr("恢复模式：工作区只读"))
                .font_size(18.0)
                .into(),
        );
        rows.push(
            TextBlock::new()
                .text(issue.clone())
                .text_wrapping(windows_reactor::TextWrapping::Wrap)
                .into(),
        );
        rows.push(
            Button::new()
                .content(tr("打开配置目录"))
                .on_click(context.message(Message::RunAction(Action::OpenConfigFolder)))
                .into(),
        );
        rows.push(
            Button::new()
                .content(tr("导出可用配置"))
                .on_click(context.message(Message::RunAction(Action::ExportConfig)))
                .into(),
        );
        rows.push(
            Button::new()
                .content(tr("导入配置…"))
                .on_click(context.message(Message::AskAction(Action::ImportConfig {
                    confirmed: false,
                })))
                .into(),
        );
        rows.push(
            Button::new()
                .content(tr("新建工作区…"))
                .on_click(context.message(Message::AskAction(Action::NewWorkspace {
                    confirmed: false,
                })))
                .into(),
        );
        if view.recovered_from.is_some() {
            rows.push(
                Button::new()
                    .content(tr("接受恢复候选…"))
                    .on_click(context.message(Message::AskAction(Action::AcceptRecovery {
                        confirmed: false,
                    })))
                    .into(),
            );
        }
    }
    rows.push(
        Button::new()
            .content(tr("重试保存"))
            .is_enabled(view.dirty && !view.saving && view.writable)
            .on_click(context.message(Message::RunAction(Action::RetrySave)))
            .into(),
    );
    if view.closing {
        rows.push(
            Button::new()
                .content(tr("取消退出等待"))
                .on_click(context.message(Message::RunAction(Action::CancelClose)))
                .into(),
        );
    }
    StackPanel::new().spacing(10.0).children(rows).into()
}

fn about(
    _component: &SettingsComponent,
    view: &SettingsView,
    context: &mut ViewContext<SettingsComponent>,
) -> View {
    let icon_hidden = view
        .desktop_icons_hidden
        .unwrap_or(view.settings.hide_real_icons);
    let mut rows = vec![
        TextBlock::new().text("PecoFence").font_size(28.0).into(),
        TextBlock::new()
            .text(format!("{} {}", tr("版本"), view.version))
            .into(),
        TextBlock::new()
            .text(format!(
                "{}: {}",
                tr("配置路径"),
                view.config_path.display()
            ))
            .text_wrapping(windows_reactor::TextWrapping::Wrap)
            .into(),
        Button::new()
            .content(tr("打开配置目录"))
            .on_click(context.message(Message::RunAction(Action::OpenConfigFolder)))
            .into(),
        TextBlock::new()
            .text(format!(
                "{}: {} MB · {} {}",
                tr("内存占用"),
                view.memory_mb
                    .map_or_else(|| "未知".into(), |value| format!("{value:.1}")),
                view.item_count,
                tr("项")
            ))
            .text_wrapping(windows_reactor::TextWrapping::Wrap)
            .into(),
        TextBlock::new()
            .text(if icon_hidden {
                tr("Windows 桌面图标当前处于隐藏状态。")
            } else {
                tr("Windows 桌面图标当前处于显示状态。")
            })
            .into(),
        Button::new()
            .content(tr("修复桌面图标"))
            .on_click(context.message(Message::RunAction(Action::RepairIcons)))
            .into(),
    ];
    if !icon_hidden {
        rows.push(
            Button::new()
                .content(tr("重新隐藏桌面图标"))
                .on_click(context.message(Message::RunAction(Action::HideDesktopIcons)))
                .into(),
        );
    }
    rows.extend([
        TextBlock::new()
            .text(tr("许可与归属信息"))
            .font_size(20.0)
            .into(),
        TextBlock::new()
            .text(tr(
                "PecoFence 是独立发行的桌面整理工具。第三方组件与许可信息随安装包提供。",
            ))
            .text_wrapping(windows_reactor::TextWrapping::Wrap)
            .into(),
    ]);
    StackPanel::new().spacing(10.0).children(rows).into()
}

fn format_rgb(rgb: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

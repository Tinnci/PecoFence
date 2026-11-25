use super::super::*;
use super::primitives::*;
use crate::settings_ui::TINT_PALETTE;
use pecofence_core::settings_protocol::{ContainerChange, ContentChange, TitleColor};
use pecofence_core::{Spacing, TitleSize};
use windows_reactor::{Button, ComboBox, NumberBox, StackPanel, TextBox, View, ViewContext};

pub(super) fn content(
    component: &SettingsComponent,
    view: &SettingsView,
    context: &mut ViewContext<SettingsComponent>,
) -> View {
    let mut rows = vec![section("选择内容")];
    if view.contents.is_empty() {
        rows.push(note("当前没有可配置的内容。"));
        return StackPanel::new().spacing(8.0).children(rows).into();
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
    let selector = ComboBox::new()
        .width(ROW_CONTROL_WIDTH)
        .automation_name(tr("内容与所属窗口"))
        .items_source(labels)
        .selected_index(selected_index)
        .on_selection_changed(context.callback(Message::SelectContent))
        .into();
    let Some(key) = component.selected_content else {
        rows.push(group([row("内容与所属窗口", selector).icon(glyph::LIST)]));
        rows.push(note("请重新选择仍然存在的内容目标。"));
        return StackPanel::new().spacing(8.0).children(rows).into();
    };
    let Some(item) = view
        .contents
        .iter()
        .find(|item| item.content_id == key.content && item.container_id == key.container)
    else {
        rows.push(group([row("内容与所属窗口", selector).icon(glyph::LIST)]));
        rows.push(note("此内容已移动或删除。请重新选择。"));
        return StackPanel::new().spacing(8.0).children(rows).into();
    };
    let title_draft = component.title_drafts.get(&key);
    rows.push(group([
        row("内容与所属窗口", selector)
            .icon(glyph::LIST)
            .description(format!(
                "{}: {} · {}: {}",
                tr("所在窗口"),
                item.window_title,
                tr("同一窗口的内容"),
                item.window_contents.join("、")
            )),
        row(
            "内容名称",
            buttons(vec![
                TextBox::new(
                    title_draft.map_or_else(|| item.title.clone(), |draft| draft.value.clone()),
                )
                .width(200.0)
                .automation_name(tr("内容名称"))
                .is_enabled(view.writable)
                .on_text_changed(
                    context.callback(|value: std::rc::Rc<str>| {
                        Message::ContentText(value.to_string())
                    }),
                )
                .into(),
                Button::new()
                    .content(tr("应用名称"))
                    .is_enabled(view.writable)
                    .on_click(context.message(Message::ApplyTitle))
                    .into(),
            ]),
        )
        .icon(glyph::RENAME),
    ]));
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
    rows.push(buttons(title_review));
    rows.push(section("内容选项"));
    let icon_sizes = ["32", "48", "64", "96"];
    let mut options = vec![
        field_combo(
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
        )
        .icon(glyph::ZOOM),
    ];
    options.push(
        field_combo(
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
        )
        .icon(glyph::FIT),
    );
    rows.push(group(options));
    let key = ContentKey {
        content: item.content_id,
        container: item.container_id,
    };
    if let Some(portal) = &item.portal {
        rows.push(section("文件夹门户"));
        let portal_rows = vec![
            container_toggle(
                "门户内双击打开子文件夹",
                portal.navigate,
                view.writable,
                key,
                super::super::ContainerField::PortalNavigate,
                context,
            )
            .icon(glyph::FOLDER),
            container_toggle(
                "标题栏显示文件夹图标",
                portal.title_icon,
                view.writable,
                key,
                super::super::ContainerField::PortalTitleIcon,
                context,
            )
            .icon(glyph::PICTURE),
        ];
        rows.push(group(portal_rows));
    }
    rows.push(section("整个窗口（包括所有标签页）"));
    let mut window = vec![
        row(
            "不透明度",
            StackPanel::new()
                .orientation(windows_reactor::Orientation::Horizontal)
                .spacing(8.0)
                .children((
                    NumberBox::new()
                        .width(120.0)
                        .automation_name(tr("不透明度"))
                        .minimum(0.1)
                        .maximum(2.0)
                        .value(Some(f64::from(item.opacity.unwrap_or(1.0))))
                        .is_enabled(view.writable)
                        .on_value_changed(
                            context
                                .callback(move |value: Option<f64>| Message::Opacity(key, value)),
                        ),
                    Button::new()
                        .content(tr("恢复默认不透明度"))
                        .is_enabled(view.writable)
                        .on_click(
                            context
                                .message(Message::Container(key, ContainerChange::Opacity(None))),
                        ),
                ))
                .into(),
        )
        .icon(glyph::CONTRAST),
    ];
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
    window.push(
        row(
            "窗口色调",
            ComboBox::new()
                .width(ROW_CONTROL_WIDTH)
                .automation_name(tr("窗口色调"))
                .items_source(tint_items)
                .selected_index(Some(tint_index))
                .is_enabled(view.writable)
                .on_selection_changed(context.callback(move |index: Option<usize>| {
                    let tint = match index {
                        Some(0) | None => None,
                        Some(index) if index <= TINT_PALETTE.len() => {
                            Some(TINT_PALETTE[index - 1].1)
                        }
                        Some(_) => current_tint,
                    };
                    Message::Container(key, ContainerChange::Tint(tint))
                }))
                .into(),
        )
        .icon(glyph::COLOR),
    );
    let title_color_index = match item.title_color {
        TitleColor::Theme => Some(0),
        TitleColor::Tint => Some(1),
        TitleColor::White => Some(2),
        TitleColor::Black => Some(3),
        TitleColor::Custom(_) => None,
    };
    window.push(
        row(
            "标题颜色",
            ComboBox::new()
                .width(ROW_CONTROL_WIDTH)
                .automation_name(tr("标题颜色"))
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
        )
        .icon(glyph::FONT_COLOR),
    );
    let custom_rgb = component
        .title_color_drafts
        .get(&key)
        .map(|draft| draft.value.clone())
        .unwrap_or_else(|| match item.title_color {
            TitleColor::Custom(rgb) => format_rgb(rgb),
            _ => String::new(),
        });
    window.push(
        row(
            "自定义标题颜色（RRGGBB）",
            buttons(vec![
                TextBox::new(custom_rgb.clone())
                    .width(120.0)
                    .placeholder_text("RRGGBB")
                    .automation_name(tr("自定义标题颜色（RRGGBB）"))
                    .is_enabled(view.writable)
                    .on_text_changed(context.callback(move |value: std::rc::Rc<str>| {
                        Message::TitleColorText(key, value.to_string())
                    }))
                    .into(),
                Button::new()
                    .content(tr("应用自定义标题颜色"))
                    .is_enabled(view.writable && parse_rgb(&custom_rgb).is_some())
                    .on_click(context.message(Message::ApplyTitleColor(key)))
                    .into(),
            ]),
        )
        .icon(glyph::EDIT),
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
    let mut frame = vec![
        field_combo(
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
        )
        .icon(glyph::FONT_SIZE),
    ];
    frame.extend([
        container_toggle(
            "自动高度",
            item.auto_height,
            view.writable,
            key,
            super::super::ContainerField::AutoHeight,
            context,
        )
        .icon(glyph::RULER),
        container_toggle(
            "锁定位置和大小",
            item.locked,
            view.writable,
            key,
            super::super::ContainerField::Locked,
            context,
        )
        .icon(glyph::LOCK),
        container_toggle(
            "快速隐藏时保持可见",
            item.exclude_from_quick_hide,
            view.writable,
            key,
            super::super::ContainerField::ExcludeFromQuickHide,
            context,
        )
        .icon(glyph::PIN),
    ]);
    window.extend(frame);
    rows.push(group(window));
    // A stable slot, so review controls appearing do not recreate the inputs above.
    rows.push(buttons(color_review));
    rows.extend([Button::new()
        .content(tr("停靠到屏幕顶部"))
        .is_enabled(view.writable)
        .on_click(context.message(Message::Container(key, ContainerChange::DockTop)))
        .into()]);
    StackPanel::new().spacing(10.0).children(rows).into()
}

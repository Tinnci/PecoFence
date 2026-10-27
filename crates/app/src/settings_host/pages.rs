use super::{ContentKey, Message, Page, SettingsComponent, SettingsView, parse_rgb, tr};
use crate::settings_ui::TINT_PALETTE;
use pecofence_core::settings_protocol::{
    Action, ContainerChange, ContentChange, SettingChange, TitleColor,
};
use pecofence_core::{Spacing, TitleSize};
use windows_reactor::{
    Border, Button, ComboBox, Grid, GridLength, HorizontalAlignment, InfoBar, InfoBarSeverity,
    NumberBox, ProgressRing, Slider, StackPanel, TextBlock, TextBox, ThemeBrush, ToggleSwitch,
    TooltipExt, VerticalAlignment, View, ViewContext,
};

pub(super) fn notice(error: bool, message: &str) -> View {
    InfoBar::new()
        .title(if error {
            tr("需要检查")
        } else {
            tr("状态")
        })
        .message(message)
        .severity(if error {
            InfoBarSeverity::Error
        } else {
            InfoBarSeverity::Informational
        })
        .is_open(true)
        .is_closable(false)
        .into()
}

/// Width of the control column in a settings row (combo boxes, sliders).
const ROW_CONTROL_WIDTH: f64 = 240.0;
/// Outer corners of a settings group, as on Windows 11 cards.
const OUTER_RADIUS: f64 = 8.0;
/// Corners between rows of one group. The small inner radius and 2 px gap make a section
/// read as one connected list (Material 3 Expressive grouped lists) while every row keeps
/// its own card.
const INNER_RADIUS: f64 = 3.0;
const GROUP_GAP: f64 = 2.0;

/// Segoe Fluent Icons glyphs used by the settings rows.
pub(super) mod glyph {
    pub const POWER: &str = "\u{E7E8}";
    pub const HIDE: &str = "\u{ED1A}";
    pub const TOUCH: &str = "\u{E7C9}";
    pub const MONITOR: &str = "\u{E7F4}";
    pub const COLOR: &str = "\u{E790}";
    pub const BRIGHTNESS: &str = "\u{E706}";
    pub const GLOBE: &str = "\u{E774}";
    pub const ZOOM: &str = "\u{E71E}";
    pub const PERSONALIZE: &str = "\u{E771}";
    pub const EQUALIZER: &str = "\u{E9E9}";
    pub const LAYERS: &str = "\u{E81E}";
    pub const CHEVRON_DOWN: &str = "\u{E70D}";
    pub const MOUSE: &str = "\u{E962}";
    pub const VIEW_ALL: &str = "\u{E8A9}";
    pub const SCROLL: &str = "\u{EC8F}";
    pub const MAGNET: &str = "\u{E8F0}";
    pub const VIEW: &str = "\u{E890}";
    pub const KEYBOARD: &str = "\u{E765}";
    pub const QUIET: &str = "\u{E708}";
    pub const PAGE: &str = "\u{E7C3}";
    pub const CHECK: &str = "\u{E73E}";
    pub const ERROR: &str = "\u{E783}";
    pub const SYNC: &str = "\u{E895}";
    pub const RECENT: &str = "\u{E823}";
    pub const HISTORY: &str = "\u{E81C}";
    pub const CAMERA: &str = "\u{E722}";
    pub const EXPORT: &str = "\u{EDE1}";
    pub const ADD: &str = "\u{E710}";
    pub const TILES: &str = "\u{ECA5}";
    pub const SWITCH: &str = "\u{E8AB}";
    pub const FIT: &str = "\u{E9A6}";
    pub const FOLDER: &str = "\u{E8B7}";
    pub const PICTURE: &str = "\u{E8B9}";
    pub const CONTRAST: &str = "\u{E7A1}";
    pub const FONT_COLOR: &str = "\u{E8D3}";
    pub const FONT_SIZE: &str = "\u{E8E9}";
    pub const EDIT: &str = "\u{E70F}";
    pub const RULER: &str = "\u{ED5E}";
    pub const LOCK: &str = "\u{E72E}";
    pub const PIN: &str = "\u{E718}";
    pub const LIST: &str = "\u{E71D}";
    pub const RENAME: &str = "\u{E8AC}";
}

/// A page section heading.
pub(super) fn section(title: &'static str) -> View {
    TextBlock::new()
        .text(tr(title))
        .font_size(16.0)
        .font_weight(windows_reactor::FontWeight::SEMI_BOLD)
        .margin(windows_reactor::Thickness {
            left: 0.0,
            top: 20.0,
            right: 0.0,
            bottom: 2.0,
        })
        .automation_heading_level(windows_reactor::AutomationHeadingLevel::Level2)
        .into()
}

/// A wrapped explanatory line under a section heading.
pub(super) fn note(text: &'static str) -> View {
    supporting(tr(text))
}

/// Secondary text: a description under a label, or a note under a heading.
fn supporting(text: impl AsRef<str>) -> View {
    TextBlock::new()
        .text(text.as_ref())
        .font_size(12.0)
        .opacity(0.72)
        .text_wrapping(windows_reactor::TextWrapping::Wrap)
        .into()
}

/// A Segoe Fluent Icons glyph at `size` DIP (FontIcon is fixed at 20, so it is scaled).
pub(super) fn icon(glyph: &str, size: f64) -> View {
    windows_reactor::Viewbox::new()
        .width(size)
        .height(size)
        .content(windows_reactor::FontIcon::new().glyph(glyph))
        .into()
}

/// An icon in a tinted circle: the emphasized status mark of a hero card.
fn tonal_icon(glyph: &str, critical: bool) -> View {
    Grid::new()
        .width(44.0)
        .height(44.0)
        .vertical_alignment(VerticalAlignment::Top)
        .children((
            windows_reactor::Ellipse::new()
                .fill(if critical {
                    ThemeBrush::SystemCritical
                } else {
                    ThemeBrush::Accent
                })
                .opacity(0.18),
            Border::new()
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .content(icon(glyph, 20.0)),
        ))
        .into()
}

/// One settings row, as in Windows Settings: an optional icon, the label (with an optional
/// description) on the left and its control on the right. Rows go into a [`group`]; a lone
/// row converts into a standalone card.
pub(super) struct Row {
    icon: Option<&'static str>,
    label: String,
    description: Option<String>,
    control: Option<View>,
}

impl Row {
    pub(super) fn new(label: impl Into<String>, control: Option<View>) -> Self {
        Self {
            icon: None,
            label: label.into(),
            description: None,
            control,
        }
    }

    pub(super) fn icon(mut self, glyph: &'static str) -> Self {
        self.icon = Some(glyph);
        self
    }

    pub(super) fn description(mut self, text: impl Into<String>) -> Self {
        self.description = Some(text.into());
        self
    }

    fn body(self) -> View {
        let text: View = match self.description {
            Some(description) => StackPanel::new()
                .spacing(2.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text(self.label)
                        .text_wrapping(windows_reactor::TextWrapping::Wrap),
                    supporting(description),
                ))
                .into(),
            None => TextBlock::new()
                .text(self.label)
                .text_wrapping(windows_reactor::TextWrapping::Wrap)
                .vertical_alignment(VerticalAlignment::Center)
                .into(),
        };
        let mut cells: Vec<View> = Vec::new();
        if let Some(glyph) = self.icon {
            cells.push(
                Border::new()
                    .vertical_alignment(VerticalAlignment::Center)
                    .margin(windows_reactor::Thickness {
                        left: 4.0,
                        top: 0.0,
                        right: 16.0,
                        bottom: 0.0,
                    })
                    .content(icon(glyph, 18.0))
                    .into(),
            );
        }
        let text_column = cells.len() as i32;
        cells.push(
            Border::new()
                .grid_column(text_column)
                .vertical_alignment(VerticalAlignment::Center)
                .margin(windows_reactor::Thickness {
                    right: 24.0,
                    left: 0.0,
                    top: 0.0,
                    bottom: 0.0,
                })
                .content(text)
                .into(),
        );
        if let Some(control) = self.control {
            cells.push(
                Border::new()
                    .grid_column(text_column + 1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(control)
                    .into(),
            );
        }
        let columns: Vec<GridLength> = if self.icon.is_some() {
            vec![GridLength::Auto, GridLength::STAR, GridLength::Auto]
        } else {
            vec![GridLength::STAR, GridLength::Auto]
        };
        Grid::new()
            .columns(columns)
            .min_height(40.0)
            .children(cells)
            .into()
    }
}

impl From<Row> for View {
    fn from(row: Row) -> Self {
        card(row.body())
    }
}

/// A section's rows as one connected list: full corners at its ends, tight ones between.
pub(super) fn group(rows: impl IntoIterator<Item = Row>) -> View {
    let rows: Vec<Row> = rows.into_iter().collect();
    let last = rows.len().saturating_sub(1);
    let children: Vec<View> = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let top = if index == 0 {
                OUTER_RADIUS
            } else {
                INNER_RADIUS
            };
            let bottom = if index == last {
                OUTER_RADIUS
            } else {
                INNER_RADIUS
            };
            surface(
                row.body(),
                windows_reactor::CornerRadius::new(top, top, bottom, bottom),
            )
        })
        .collect();
    StackPanel::new()
        .spacing(GROUP_GAP)
        .children(children)
        .into()
}

fn surface(content: View, corners: windows_reactor::CornerRadius) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(1.0)
        .corner_radius(corners)
        .padding(windows_reactor::Thickness {
            left: 16.0,
            top: 10.0,
            right: 16.0,
            bottom: 10.0,
        })
        .content(content)
        .into()
}

/// Shorthand for a label and its control.
pub(super) fn row(label: &'static str, control: View) -> Row {
    Row::new(tr(label), Some(control))
}

/// An immediate setting: a labelled, accessible native switch in a themed settings row.
pub(super) fn switch(
    label: &'static str,
    value: bool,
    enabled: bool,
    callback: windows_reactor::Callback<bool>,
) -> Row {
    row(
        label,
        ToggleSwitch::new()
            .automation_name(tr(label))
            .is_on(value)
            .is_enabled(enabled)
            .on_toggled(callback)
            .into(),
    )
}

/// A slider setting row; `label` names it for UI Automation.
fn slider_row(
    label: &'static str,
    (min, max, value): (f64, f64, f64),
    enabled: bool,
    callback: windows_reactor::Callback<f64>,
) -> Row {
    row(
        label,
        Slider::new()
            .width(ROW_CONTROL_WIDTH)
            .automation_name(tr(label))
            .minimum(min)
            .maximum(max)
            .value(value)
            .is_enabled(enabled)
            .on_value_changed(callback)
            .into(),
    )
}

pub(super) fn card(content: View) -> View {
    surface(
        content,
        windows_reactor::CornerRadius::new(8.0, 8.0, 8.0, 8.0),
    )
}

/// Buttons that belong together, side by side (Material 3 Expressive button groups).
fn buttons(items: Vec<View>) -> View {
    StackPanel::new()
        .orientation(windows_reactor::Orientation::Horizontal)
        .spacing(8.0)
        .children(items)
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
        return StackPanel::new()
            .spacing(12.0)
            .children((
                ProgressRing::new()
                    .width(24.0)
                    .height(24.0)
                    .is_active(true)
                    .is_indeterminate(true)
                    .automation_name(tr("正在加载设置工作区…"))
                    .horizontal_alignment(HorizontalAlignment::Left),
                TextBlock::new().text(tr("正在加载设置工作区…")),
            ))
            .into();
    };
    match component.page {
        Page::General => general(view, context),
        Page::Widgets => widgets(view, context),
        Page::Content => content(component, view, context),
        Page::Rules => super::rule_editor::view(component, view, context),
        Page::Layout => layout(component, view, context),
        Page::About => about(component, view, context),
    }
}

fn general(view: &SettingsView, context: &mut ViewContext<SettingsComponent>) -> View {
    let s = &view.settings;
    let enabled = view.writable;
    let desktop = vec![
        toggle(
            "随 Windows 启动",
            s.autostart,
            enabled,
            context,
            SettingChange::Autostart,
        )
        .icon(glyph::POWER),
        toggle(
            "隐藏 Windows 桌面图标",
            s.hide_real_icons,
            enabled,
            context,
            SettingChange::HideRealIcons,
        )
        .icon(glyph::HIDE),
        toggle(
            "双击桌面空白处快速隐藏",
            s.quick_hide.enabled,
            enabled,
            context,
            SettingChange::QuickHideEnabled,
        )
        .icon(glyph::TOUCH),
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
        )
        .icon(glyph::MONITOR),
    ];
    let mut appearance = vec![
        field_combo(
            "主题风格",
            &["Fluent", "Liquid Glass", "纸张"],
            match s.theme_style {
                pecofence_core::ThemeStyle::Fluent => 0,
                pecofence_core::ThemeStyle::LiquidGlass => 1,
                pecofence_core::ThemeStyle::Paper => 2,
            },
            enabled,
            context.callback(|index| {
                Message::Setting(SettingChange::ThemeStyle(match index {
                    Some(1) => pecofence_core::ThemeStyle::LiquidGlass,
                    Some(2) => pecofence_core::ThemeStyle::Paper,
                    _ => pecofence_core::ThemeStyle::Fluent,
                }))
            }),
        )
        .icon(glyph::COLOR),
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
        )
        .icon(glyph::BRIGHTNESS),
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
        )
        .icon(glyph::GLOBE),
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
        )
        .icon(glyph::ZOOM),
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
    appearance.push(
        row(
            "图标着色",
            ComboBox::new()
                .width(ROW_CONTROL_WIDTH)
                .automation_name(tr("图标着色"))
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
        )
        .icon(glyph::PERSONALIZE),
    );
    appearance.extend([
        // Strength only means something while a tint colour is chosen.
        slider_row(
            "图标着色强度",
            (0.0, 1.0, f64::from(s.icons.tint_strength)),
            enabled && s.icons.tint_rgb.is_some(),
            context
                .callback(|value| Message::Setting(SettingChange::IconTintStrength(value as f32))),
        )
        .icon(glyph::EQUALIZER),
        toggle(
            "图标融入背景（Chameleon）",
            s.icons.chameleon,
            enabled,
            context,
            SettingChange::Chameleon,
        )
        .icon(glyph::LAYERS),
    ]);
    let behavior = vec![
        // Click-to-expand replaces hover expansion, so the hover switch is moot while it is on.
        toggle(
            "卷起的栅栏需点击标题才展开",
            s.roll_up.click_to_expand,
            enabled,
            context,
            SettingChange::ClickToExpand,
        )
        .icon(glyph::CHEVRON_DOWN),
        toggle(
            "卷起的栅栏悬停时自动展开",
            s.roll_up.hover_peek,
            enabled && !s.roll_up.click_to_expand,
            context,
            SettingChange::HoverPeek,
        )
        .icon(glyph::MOUSE),
        toggle(
            "鼠标悬停时才显示标题栏",
            s.roll_up.title_on_hover,
            enabled,
            context,
            SettingChange::TitleOnHover,
        )
        .icon(glyph::VIEW_ALL),
        toggle(
            "不活动时隐藏滚动条",
            s.roll_up.hide_inactive_scrollbar,
            enabled,
            context,
            SettingChange::HideInactiveScrollbar,
        )
        .icon(glyph::SCROLL),
        toggle(
            "移动栅栏时吸附对齐",
            s.snapping.enabled,
            enabled,
            context,
            SettingChange::SnappingEnabled,
        )
        .icon(glyph::MAGNET),
    ];
    let peek = vec![
        toggle(
            "启用 Peek 浮现栅栏",
            s.peek.enabled,
            enabled,
            context,
            SettingChange::PeekEnabled,
        )
        .icon(glyph::VIEW),
        field_combo(
            "Peek 快捷键",
            &["Ctrl + Alt + 空格", "Win + 空格", "Win + Shift + 空格"],
            match s.peek.hotkey {
                pecofence_core::PeekHotkey::CtrlAltSpace => 0,
                pecofence_core::PeekHotkey::WinSpace => 1,
                pecofence_core::PeekHotkey::WinShiftSpace => 2,
            },
            enabled && s.peek.enabled,
            context.callback(|index: Option<usize>| {
                Message::Setting(SettingChange::PeekHotkey(match index.unwrap_or(0) {
                    1 => pecofence_core::PeekHotkey::WinSpace,
                    2 => pecofence_core::PeekHotkey::WinShiftSpace,
                    _ => pecofence_core::PeekHotkey::CtrlAltSpace,
                }))
            }),
        )
        .icon(glyph::KEYBOARD),
        toggle(
            "Peek 时压暗背景",
            s.peek.dim,
            enabled && s.peek.enabled,
            context,
            SettingChange::PeekDim,
        )
        .icon(glyph::QUIET),
    ];
    StackPanel::new()
        .spacing(8.0)
        .children(vec![
            section("桌面与启动"),
            group(desktop),
            section("外观"),
            group(appearance),
            section("栅栏行为"),
            group(behavior),
            section("Peek 浮现栅栏"),
            note("单击托盘图标或按快捷键，把所有栅栏浮到当前窗口之上；再按一次、按 Esc 或点击空白处返回。"),
            group(peek),
            section("CI/CD"),
            note("开启「失败提醒」的任务构建失败时，发出一条通知。通知遵循 Windows 的专注助手与勿扰设置。"),
            group(vec![toggle(
                "CI 失败通知",
                s.ci_failure_notifications,
                enabled,
                context,
                SettingChange::CiFailureNotifications,
            )]),
            section("CI/CD 快捷键"),
            supporting(view.ci_hotkey_status.as_str()),
            group(vec![
                toggle(
                    "启用 CI/CD 快捷键",
                    s.ci_hotkey_enabled,
                    enabled,
                    context,
                    SettingChange::CiHotkeyEnabled,
                ),
                field_combo(
                    "CI/CD 快捷键",
                    &["Ctrl + Alt + J", "Ctrl + Alt + K", "Ctrl + Alt + L"],
                    match s.ci_hotkey_key {
                        0x4B => 1,
                        0x4C => 2,
                        _ => 0,
                    },
                    enabled,
                    context.callback(|i: Option<usize>| {
                        Message::Setting(SettingChange::CiHotkeyKey(match i.unwrap_or(0) {
                            1 => 0x4B,
                            2 => 0x4C,
                            _ => 0x4A,
                        }))
                    }),
                ),
            ]),
        ])
        .into()
}

fn widgets(view: &SettingsView, context: &mut ViewContext<SettingsComponent>) -> View {
    let s = &view.settings;
    let enabled = view.writable;
    StackPanel::new()
        .spacing(8.0)
        .children(vec![
            section("小组件外观"),
            note("适用于便笺、待办、日历、时钟和图表。"),
            group([
                field_combo(
                    "小组件纸张",
                    &["跟随壁纸", "浅色纸张", "柔和纸张", "深色纸张"],
                    match s.widget_paper_style {
                        pecofence_core::WidgetPaperStyle::Adaptive => 0,
                        pecofence_core::WidgetPaperStyle::Light => 1,
                        pecofence_core::WidgetPaperStyle::Soft => 2,
                        pecofence_core::WidgetPaperStyle::Dark => 3,
                    },
                    enabled,
                    context.callback(|index: Option<usize>| {
                        Message::Setting(SettingChange::WidgetPaperStyle(
                            match index.unwrap_or(0) {
                                1 => pecofence_core::WidgetPaperStyle::Light,
                                2 => pecofence_core::WidgetPaperStyle::Soft,
                                3 => pecofence_core::WidgetPaperStyle::Dark,
                                _ => pecofence_core::WidgetPaperStyle::Adaptive,
                            },
                        ))
                    }),
                )
                .icon(glyph::PAGE),
                slider_row(
                    "纸张亮度",
                    (0.6, 1.0, f64::from(s.widget_paper_brightness)),
                    enabled,
                    context.callback(|value| {
                        Message::Setting(SettingChange::WidgetPaperBrightness(value as f32))
                    }),
                )
                .icon(glyph::BRIGHTNESS),
            ]),
            note("跟随壁纸会根据所在显示器的壁纸亮度自动选择纸色。高对比度模式使用系统颜色。"),
        ])
        .into()
}

fn toggle(
    label: &'static str,
    value: bool,
    enabled: bool,
    context: &mut ViewContext<SettingsComponent>,
    change: fn(bool) -> SettingChange,
) -> Row {
    switch(
        label,
        value,
        enabled,
        context.callback(move |value| Message::Setting(change(value))),
    )
}

fn field_combo(
    label: &'static str,
    choices: &[&'static str],
    selected: usize,
    enabled: bool,
    callback: windows_reactor::Callback<Option<usize>>,
) -> Row {
    row(
        label,
        ComboBox::new()
            .width(ROW_CONTROL_WIDTH)
            .automation_name(tr(label))
            .items_source(choices.iter().map(|choice| tr(choice)))
            .selected_index(Some(selected))
            .is_enabled(enabled)
            .on_selection_changed(callback)
            .into(),
    )
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
                super::ContainerField::PortalNavigate,
                context,
            )
            .icon(glyph::FOLDER),
            container_toggle(
                "标题栏显示文件夹图标",
                portal.title_icon,
                view.writable,
                key,
                super::ContainerField::PortalTitleIcon,
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
            super::ContainerField::AutoHeight,
            context,
        )
        .icon(glyph::RULER),
        container_toggle(
            "锁定位置和大小",
            item.locked,
            view.writable,
            key,
            super::ContainerField::Locked,
            context,
        )
        .icon(glyph::LOCK),
        container_toggle(
            "快速隐藏时保持可见",
            item.exclude_from_quick_hide,
            view.writable,
            key,
            super::ContainerField::ExcludeFromQuickHide,
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

fn container_toggle(
    label: &'static str,
    value: bool,
    enabled: bool,
    key: ContentKey,
    field: super::ContainerField,
    context: &mut ViewContext<SettingsComponent>,
) -> Row {
    switch(
        label,
        value,
        enabled,
        context.callback(move |value| Message::ContainerToggle(key, field, value)),
    )
}

/// Backups listed before the rest fold into 「更早的备份」.
const RECENT_BACKUPS: usize = 3;

fn layout(
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
        section("许可与归属信息"),
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

use super::super::*;
use super::primitives::*;
use crate::settings_ui::TINT_PALETTE;
use pecofence_core::settings_protocol::SettingChange;
use windows_reactor::{ComboBox, StackPanel, View, ViewContext};

pub(super) fn general(view: &SettingsView, context: &mut ViewContext<SettingsComponent>) -> View {
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

pub(super) fn widgets(view: &SettingsView, context: &mut ViewContext<SettingsComponent>) -> View {
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

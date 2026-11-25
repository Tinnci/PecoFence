use super::super::*;
use pecofence_core::settings_protocol::SettingChange;
use windows_reactor::{
    Border, Button, ComboBox, Grid, GridLength, HorizontalAlignment, InfoBar, InfoBarSeverity,
    Slider, StackPanel, TextBlock, ThemeBrush, ToggleSwitch, VerticalAlignment, View, ViewContext,
};

pub(in crate::settings_host) fn notice(error: bool, message: &str) -> View {
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
pub(super) const ROW_CONTROL_WIDTH: f64 = 240.0;
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
pub(in crate::settings_host) fn section(title: &'static str) -> View {
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
pub(in crate::settings_host) fn note(text: &'static str) -> View {
    supporting(tr(text))
}

/// Secondary text: a description under a label, or a note under a heading.
pub(super) fn supporting(text: impl AsRef<str>) -> View {
    TextBlock::new()
        .text(text.as_ref())
        .font_size(12.0)
        .opacity(0.72)
        .text_wrapping(windows_reactor::TextWrapping::Wrap)
        .into()
}

/// A Segoe Fluent Icons glyph at `size` DIP (FontIcon is fixed at 20, so it is scaled).
pub(in crate::settings_host) fn icon(glyph: &str, size: f64) -> View {
    windows_reactor::Viewbox::new()
        .width(size)
        .height(size)
        .content(windows_reactor::FontIcon::new().glyph(glyph))
        .into()
}

/// An icon in a tinted circle: the emphasized status mark of a hero card.
pub(super) fn tonal_icon(glyph: &str, critical: bool) -> View {
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
pub(in crate::settings_host) struct Row {
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
pub(in crate::settings_host) fn group(rows: impl IntoIterator<Item = Row>) -> View {
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
pub(in crate::settings_host) fn row(label: &'static str, control: View) -> Row {
    Row::new(tr(label), Some(control))
}

/// An immediate setting: a labelled, accessible native switch in a themed settings row.
pub(in crate::settings_host) fn switch(
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
pub(super) fn slider_row(
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

pub(in crate::settings_host) fn card(content: View) -> View {
    surface(
        content,
        windows_reactor::CornerRadius::new(8.0, 8.0, 8.0, 8.0),
    )
}

/// Buttons that belong together, side by side (Material 3 Expressive button groups).
pub(super) fn buttons(items: Vec<View>) -> View {
    StackPanel::new()
        .orientation(windows_reactor::Orientation::Horizontal)
        .spacing(8.0)
        .children(items)
        .into()
}

pub(in crate::settings_host) fn button(
    callback: windows_reactor::Callback<()>,
    label: &'static str,
) -> View {
    Button::new().content(tr(label)).on_click(callback).into()
}

pub(super) fn toggle(
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

pub(super) fn field_combo(
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

pub(super) fn container_toggle(
    label: &'static str,
    value: bool,
    enabled: bool,
    key: ContentKey,
    field: crate::settings_host::ContainerField,
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
pub(super) const RECENT_BACKUPS: usize = 3;

pub(super) fn format_rgb(rgb: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

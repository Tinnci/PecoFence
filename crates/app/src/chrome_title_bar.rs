//! A panel's [`PanelChrome`] as a WinUI 3 `TitleBar` (Reactor), for a standalone
//! window such as the SPM workspace. The fence draws the same data in its title
//! strip, so a panel publishes one header and both hosts show it.
//!
//! Mapping: title and subtitle go to `TitleBar.Title`/`Subtitle`; the status line
//! and the commands go to `RightHeader`, as a severity glyph with text and
//! icon-only `AppBarButton`s (Segoe Fluent `FontIcon`, label as tooltip and
//! accessible name). `ChromeSeverity` has the same levels as `InfoBarSeverity`.

use pecofence_plugin_api::{ChromeSeverity, PanelChrome, paper};
use windows_reactor::{
    AppBarButton, Color, FontIcon, Icon, InfoBarSeverity, IntoUnitCallback, Orientation,
    StackPanel, TextBlock, TitleBar, TooltipExt, VerticalAlignment, View, keyed,
};

pub fn info_bar_severity(severity: ChromeSeverity) -> InfoBarSeverity {
    match severity {
        ChromeSeverity::Informational => InfoBarSeverity::Informational,
        ChromeSeverity::Success => InfoBarSeverity::Success,
        ChromeSeverity::Warning => InfoBarSeverity::Warning,
        ChromeSeverity::Error => InfoBarSeverity::Error,
    }
}

fn severity_glyph(severity: ChromeSeverity) -> Option<&'static str> {
    match severity {
        ChromeSeverity::Informational => None,
        ChromeSeverity::Success => Some("\u{E73E}"),
        ChromeSeverity::Warning => Some("\u{E7BA}"),
        ChromeSeverity::Error => Some("\u{E783}"),
    }
}

fn color(c: [f32; 4]) -> Color {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::rgb(byte(c[0]), byte(c[1]), byte(c[2]))
}

/// The window ground behind the title bar and content. WinUI's `TitleBar` is
/// transparent, so a window that hosts a panel paints this on its root `Grid`
/// (with no Mica/Acrylic backdrop) to get one continuous sheet.
pub fn sheet_color(chrome: &PanelChrome) -> Option<Color> {
    chrome.surface.map(color)
}

/// The WinUI title bar for `chrome`. `command(id)` builds the click callback of a
/// command; `dark` picks the status inks for a dark paper sheet.
pub fn title_bar<C: IntoUnitCallback>(
    chrome: &PanelChrome,
    dark: bool,
    command: impl Fn(u64) -> C,
) -> TitleBar {
    let mut right = Vec::new();
    if let Some(status) = &chrome.status {
        let ink = paper::status_ink(status.severity, dark);
        let mut parts = Vec::new();
        if let Some(glyph) = severity_glyph(status.severity) {
            // Reactor's FontIcon takes no brush: the glyph's shape carries the severity,
            // the text beside it carries the colour.
            parts.push(keyed("glyph", FontIcon::new().glyph(glyph)));
        }
        let mut text = TextBlock::new().text(&status.text);
        if let Some(ink) = ink {
            text = text.foreground(color(ink));
        }
        parts.push(keyed("text", text));
        right.push(keyed(
            "status",
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(6.0)
                .vertical_alignment(VerticalAlignment::Center)
                .keyed_children(parts),
        ));
    }
    // The chrome lists commands nearest the window controls first; WinUI lays out left to right.
    for c in chrome.commands.iter().rev() {
        let button = AppBarButton::new()
            .label(&c.label)
            .icon(Icon::font(c.glyph.to_string()))
            .is_enabled(c.enabled)
            .on_click(command(c.id));
        right.push(keyed(format!("command:{}", c.id), button.tooltip(&c.label)));
    }
    let mut bar = TitleBar::new()
        .title(&chrome.title)
        .is_back_button_visible(false);
    if !chrome.subtitle.is_empty() {
        bar = bar.subtitle(&chrome.subtitle);
    }
    if !right.is_empty() {
        bar = bar.right_header(View::from(
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(4.0)
                .keyed_children(right),
        ));
    }
    bar
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecofence_plugin_api::{ChromeCommand, ChromeStatus};
    use windows_reactor::{
        Component, ComponentContext, ComponentHost, PropertyId, PropertyValue, RecordingAdapter,
        ViewContext, component,
    };

    #[derive(Clone, PartialEq)]
    struct Input(PanelChrome);

    struct Bar;
    impl Component for Bar {
        type Input = Input;
        type Message = u64;
        fn create(_: &Input, _: &ComponentContext<Self>) -> Self {
            Self
        }
        fn view(&self, input: &Input, context: &mut ViewContext<Self>) -> View {
            title_bar(&input.0, false, |id| context.message(id)).into()
        }
    }

    fn chrome() -> PanelChrome {
        PanelChrome {
            title: "orion · orion-ir-2026-11".into(),
            subtitle: "Demo data · Step 3 of 3".into(),
            status: Some(ChromeStatus {
                text: "Gerrit did not refresh".into(),
                severity: ChromeSeverity::Warning,
            }),
            commands: vec![
                ChromeCommand {
                    id: 1,
                    label: "Refresh".into(),
                    glyph: '\u{E72C}',
                    enabled: false,
                },
                ChromeCommand {
                    id: 2,
                    label: "Copy summary".into(),
                    glyph: '\u{E8C8}',
                    enabled: true,
                },
            ],
            surface: None,
        }
    }

    fn strings(host: &ComponentHost<RecordingAdapter>, id: PropertyId) -> Vec<String> {
        let graph = host.runtime().graph();
        graph
            .objects()
            .filter_map(|object| graph.properties(object))
            .flat_map(|properties| properties.iter())
            .filter(|p| p.id == id)
            .filter_map(|p| match &p.value {
                PropertyValue::String(s) => Some(s.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn severities_map_one_to_one_onto_info_bar() {
        assert_eq!(
            [
                ChromeSeverity::Informational,
                ChromeSeverity::Success,
                ChromeSeverity::Warning,
                ChromeSeverity::Error,
            ]
            .map(info_bar_severity),
            [
                InfoBarSeverity::Informational,
                InfoBarSeverity::Success,
                InfoBarSeverity::Warning,
                InfoBarSeverity::Error,
            ]
        );
    }

    #[test]
    fn panel_chrome_becomes_a_winui_title_bar() {
        let host = ComponentHost::mount(
            RecordingAdapter::new(),
            [component::<Bar>("bar", Input(chrome()))],
        )
        .unwrap();
        assert_eq!(
            strings(&host, PropertyId::Title),
            ["orion · orion-ir-2026-11"]
        );
        assert_eq!(
            strings(&host, PropertyId::Subtitle),
            ["Demo data · Step 3 of 3"]
        );
        assert!(strings(&host, PropertyId::Text).contains(&"Gerrit did not refresh".to_owned()));
        // Left to right: the command farthest from the window controls comes first.
        assert_eq!(
            strings(&host, PropertyId::Label),
            ["Copy summary", "Refresh"]
        );
        let graph = host.runtime().graph();
        let disabled = graph
            .objects()
            .filter_map(|object| graph.properties(object))
            .filter(|properties| {
                properties.iter().any(|p| {
                    p.id == PropertyId::IsEnabled && matches!(p.value, PropertyValue::Bool(false))
                })
            })
            .count();
        assert_eq!(disabled, 1, "Refresh is disabled while it runs");
        let mut paper = chrome();
        paper.surface = Some(pecofence_plugin_api::paper::GROUND);
        assert_eq!(sheet_color(&paper), Some(Color::rgb(0xF6, 0xF1, 0xE7)));
        assert_eq!(sheet_color(&chrome()), None);
    }
}

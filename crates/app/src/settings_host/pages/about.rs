use super::super::*;
use super::primitives::*;
use windows_reactor::{Button, StackPanel, TextBlock, View, ViewContext};

pub(super) fn about(
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

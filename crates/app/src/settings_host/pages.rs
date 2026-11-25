use super::{Page, SettingsComponent, SettingsView, tr};
use windows_reactor::{
    HorizontalAlignment, ProgressRing, StackPanel, TextBlock, View, ViewContext,
};

mod about;
mod content;
mod general;
mod layout;
mod primitives;

pub(super) use primitives::{button, notice, section, switch};

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
        Page::General => general::general(view, context),
        Page::Widgets => general::widgets(view, context),
        Page::Content => content::content(component, view, context),
        Page::Rules => super::rule_editor::view(component, view, context),
        Page::Layout => layout::layout(component, view, context),
        Page::About => about::about(component, view, context),
    }
}

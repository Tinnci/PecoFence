//! The fence menu's 「建议」 submenu: organizing suggestions from `pecofence_core::suggest`,
//! each applied with the same operation as the menu or drag it shortcuts.

use super::*;
use pecofence_core::suggest::{Action, Suggestion};

/// Entries in one fence's 「建议」 submenu.
pub(super) const MAX_FENCE_SUGGESTIONS: usize = 4;

fn sort_label(sort: SortMode) -> &'static str {
    match sort {
        SortMode::Manual => pecofence_core::i18n::text("手动"),
        SortMode::Name => pecofence_core::i18n::text("按名称"),
        SortMode::Type => pecofence_core::i18n::text("按类型"),
        SortMode::Date => pecofence_core::i18n::text("按修改日期"),
        SortMode::Size => pecofence_core::i18n::text("按大小"),
        SortMode::OpenCount => pecofence_core::i18n::text("按使用频率"),
    }
}

impl App {
    /// Suggestions that change or fill `fence`, most confident first. The inbox also offers
    /// the cleanup fence, since idle downloads usually wait there.
    pub(super) fn fence_suggestions(&self, fence: FenceId) -> Vec<Suggestion> {
        let inbox = self.state.inbox_id() == Some(fence);
        pecofence_core::suggest::suggest(
            &self.state.config,
            self.state.layout,
            pecofence_core::now_unix(),
        )
        .into_iter()
        .filter(|s| s.action.concerns(fence) || (inbox && s.action == Action::AddCleanup))
        .take(MAX_FENCE_SUGGESTIONS)
        .collect()
    }

    pub(super) fn suggestion_label(&self, suggestion: &Suggestion) -> String {
        let title = |id: FenceId| self.state.fence(id).map(|f| f.title).unwrap_or_default();
        match &suggestion.action {
            Action::SetSort { sort, .. } => {
                pecofence_core::i18n::format("排序：{0}", &[sort_label(*sort).to_string()])
            }
            Action::AddRule { ext, target } => pecofence_core::i18n::format(
                "自动将 .{0} 文件放入“{1}”",
                &[ext.clone(), title(*target)],
            ),
            Action::Move { item, to, .. } => pecofence_core::i18n::format(
                "将“{0}”移到“{1}”",
                &[
                    self.state
                        .item(*item)
                        .map(|it| it.display_name.clone())
                        .unwrap_or_default(),
                    title(*to),
                ],
            ),
            Action::AddCleanup => pecofence_core::i18n::text("收集闲置的安装包和压缩包").into(),
        }
    }

    pub(super) fn apply_suggestion(&mut self, suggestion: &Suggestion) {
        match &suggestion.action {
            Action::SetSort { fence, sort } => {
                self.state.set_sort(*fence, *sort);
                self.refresh_fence(*fence);
                self.schedule_save();
            }
            Action::AddRule { ext, target } => {
                let Some(fence) = self.state.fence(*target) else {
                    return;
                };
                let rule = pecofence_core::suggest::rule_for(ext, *target, &fence.title);
                if self.state.add_rule(rule) {
                    let entries = shell::enumerate_desktop();
                    self.state.apply_rules_all(&entries);
                    self.refresh_all();
                    self.schedule_save();
                    self.push_settings_state();
                }
            }
            Action::Move { item, to, .. } => self.move_items(&[*item], *to),
            Action::AddCleanup => self.add_template(pecofence_core::rules::Template::Cleanup),
        }
    }
}

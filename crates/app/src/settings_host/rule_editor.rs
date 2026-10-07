use super::{Message, SettingsComponent, tr};
use pecofence_core::Origin;
use pecofence_core::rules::{Class, Cond, Rule, StrOp, Target, TypeCategory};
use pecofence_core::settings_protocol::{Action, DocumentStamp};
use std::rc::Rc;
use uuid::Uuid;
use windows_reactor::{
    Button, CheckBox, ComboBox, StackPanel, TextBlock, TextBox, View, ViewContext, keyed,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ConditionKind {
    #[default]
    Type,
    Extension,
    ExactName,
    Name,
    Glob,
    ShortcutTarget,
    FoldersOnly,
    FilesOnly,
    SizeMb,
    CreatedTime,
    CreatedWeekday,
    Origin,
    IdleDays,
}

impl ConditionKind {
    const ALL: [Self; 13] = [
        Self::Type,
        Self::Extension,
        Self::ExactName,
        Self::Name,
        Self::Glob,
        Self::ShortcutTarget,
        Self::FoldersOnly,
        Self::FilesOnly,
        Self::SizeMb,
        Self::CreatedTime,
        Self::CreatedWeekday,
        Self::Origin,
        Self::IdleDays,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Type => "文件类型",
            Self::Extension => "扩展名",
            Self::ExactName => "名称恰好匹配",
            Self::Name => "名称条件",
            Self::Glob => "通配符",
            Self::ShortcutTarget => "快捷方式目标",
            Self::FoldersOnly => "仅文件夹",
            Self::FilesOnly => "仅文件",
            Self::SizeMb => "大小范围（MB）",
            Self::CreatedTime => "创建时间段",
            Self::CreatedWeekday => "创建星期",
            Self::Origin => "来源",
            Self::IdleDays => "闲置天数",
        }
    }
    pub(super) fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
    fn index(self) -> usize {
        Self::ALL.iter().position(|item| *item == self).unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
pub(super) struct ConditionEntry {
    pub id: Uuid,
    pub condition: Cond,
}

#[derive(Clone, Debug)]
pub(super) struct ConditionForm {
    pub kind: ConditionKind,
    pub text: String,
    pub operation: StrOp,
    pub types: Vec<TypeCategory>,
    pub weekdays: Vec<u8>,
    pub origin: Origin,
    pub minimum: String,
    pub maximum: String,
    pub time_from: String,
    pub time_to: String,
    pub idle_days: String,
}

impl Default for ConditionForm {
    fn default() -> Self {
        Self {
            kind: ConditionKind::Type,
            text: String::new(),
            operation: StrOp::Contains,
            types: vec![],
            weekdays: vec![],
            origin: Origin::UserDesktop,
            minimum: String::new(),
            maximum: String::new(),
            time_from: "18:00".into(),
            time_to: "06:00".into(),
            idle_days: "30".into(),
        }
    }
}

impl ConditionForm {
    pub(super) fn reset_values(&mut self) {
        let kind = self.kind;
        *self = Self {
            kind,
            ..Self::default()
        };
    }
    pub(super) fn set_operation_index(&mut self, index: usize) {
        self.operation = operation_at(index);
    }
    pub(super) fn set_origin_index(&mut self, index: usize) {
        self.origin = match index {
            1 => Origin::PublicDesktop,
            2 => Origin::Namespace,
            _ => Origin::UserDesktop,
        };
    }
    fn from_condition(condition: &Cond) -> Self {
        let mut form = Self::default();
        match condition {
            Cond::Type(values) => {
                form.kind = ConditionKind::Type;
                form.types = values.clone();
            }
            Cond::Ext(values) => {
                form.kind = ConditionKind::Extension;
                form.text = values.join(", ");
            }
            Cond::ExactName(values) => {
                form.kind = ConditionKind::ExactName;
                form.text = values.join(", ");
            }
            Cond::Name { op, value } => {
                form.kind = ConditionKind::Name;
                form.operation = *op;
                form.text = value.clone();
            }
            Cond::Glob(value) => {
                form.kind = ConditionKind::Glob;
                form.text = value.clone();
            }
            Cond::ShortcutTarget { op, value } => {
                form.kind = ConditionKind::ShortcutTarget;
                form.operation = *op;
                form.text = value.clone();
            }
            Cond::FoldersOnly => form.kind = ConditionKind::FoldersOnly,
            Cond::FilesOnly => form.kind = ConditionKind::FilesOnly,
            Cond::SizeMb { min, max } => {
                form.kind = ConditionKind::SizeMb;
                form.minimum = min.map_or_else(String::new, |v| v.to_string());
                form.maximum = max.map_or_else(String::new, |v| v.to_string());
            }
            Cond::CreatedTime { from_min, to_min } => {
                form.kind = ConditionKind::CreatedTime;
                form.time_from = from_minute(*from_min);
                form.time_to = from_minute(*to_min);
            }
            Cond::CreatedWeekday(values) => {
                form.kind = ConditionKind::CreatedWeekday;
                form.weekdays = values.clone();
            }
            Cond::Origin(origin) => {
                form.kind = ConditionKind::Origin;
                form.origin = *origin;
            }
            Cond::IdleDays { min } => {
                form.kind = ConditionKind::IdleDays;
                form.idle_days = min.to_string();
            }
        }
        form
    }
    pub(super) fn to_condition(&self) -> Result<Cond, String> {
        let split = |value: &str| {
            value
                .split(|ch: char| ch == ',' || ch == '，' || ch.is_whitespace())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        match self.kind {
            ConditionKind::Type if !self.types.is_empty() => Ok(Cond::Type(self.types.clone())),
            ConditionKind::Type => Err(tr("请选择至少一种文件类型。")),
            ConditionKind::Extension => {
                let values = split(&self.text)
                    .into_iter()
                    .map(|value| value.strip_prefix('.').unwrap_or(&value).to_owned())
                    .collect::<Vec<_>>();
                if values.is_empty() {
                    Err(tr("请输入一个或多个扩展名。"))
                } else {
                    Ok(Cond::Ext(values))
                }
            }
            ConditionKind::ExactName => {
                let values = split(&self.text);
                if values.is_empty() {
                    Err(tr("请输入完整文件名。"))
                } else {
                    Ok(Cond::ExactName(values))
                }
            }
            ConditionKind::Name if !self.text.trim().is_empty() => Ok(Cond::Name {
                op: self.operation,
                value: self.text.trim().to_owned(),
            }),
            ConditionKind::Name => Err(tr("请输入名称条件值。")),
            ConditionKind::Glob if !self.text.trim().is_empty() => {
                Ok(Cond::Glob(self.text.trim().into()))
            }
            ConditionKind::Glob => Err(tr("请输入通配符模式。")),
            ConditionKind::ShortcutTarget if !self.text.trim().is_empty() => {
                Ok(Cond::ShortcutTarget {
                    op: self.operation,
                    value: self.text.trim().into(),
                })
            }
            ConditionKind::ShortcutTarget => Err(tr("请输入快捷方式目标条件值。")),
            ConditionKind::FoldersOnly => Ok(Cond::FoldersOnly),
            ConditionKind::FilesOnly => Ok(Cond::FilesOnly),
            ConditionKind::SizeMb => {
                let min = parse_number(&self.minimum)?;
                let max = parse_number(&self.maximum)?;
                if min.is_none() && max.is_none() {
                    return Err(tr("请至少填写一个大小边界。"));
                }
                if min.zip(max).is_some_and(|(min, max)| min > max) {
                    return Err(tr("最大大小不能小于最小大小。"));
                }
                Ok(Cond::SizeMb { min, max })
            }
            ConditionKind::CreatedTime => Ok(Cond::CreatedTime {
                from_min: parse_time(&self.time_from)?,
                to_min: parse_time(&self.time_to)?,
            }),
            ConditionKind::CreatedWeekday if !self.weekdays.is_empty() => {
                Ok(Cond::CreatedWeekday(self.weekdays.clone()))
            }
            ConditionKind::CreatedWeekday => Err(tr("请选择至少一个星期。")),
            ConditionKind::Origin => Ok(Cond::Origin(self.origin)),
            ConditionKind::IdleDays => {
                let min = self
                    .idle_days
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .filter(|v| *v > 0)
                    .ok_or_else(|| tr("闲置天数必须是大于 0 的整数。"))?;
                Ok(Cond::IdleDays { min })
            }
        }
    }
}

pub(super) struct Draft {
    pub token: Uuid,
    pub generation: Uuid,
    pub base: DocumentStamp,
    pub rule_id: Option<Uuid>,
    pub name: String,
    pub enabled: bool,
    pub target: Option<Target>,
    pub conditions: Vec<ConditionEntry>,
    pub form: ConditionForm,
    pub editing_condition: Option<Uuid>,
    pub needs_review: bool,
    pub dirty: bool,
    pub submitted: Option<DocumentStamp>,
    priority_class: Class,
}

impl Draft {
    pub(super) fn new(base: DocumentStamp) -> Self {
        Self {
            token: Uuid::new_v4(),
            generation: Uuid::new_v4(),
            base,
            rule_id: None,
            name: String::new(),
            enabled: true,
            target: None,
            conditions: vec![],
            form: ConditionForm::default(),
            editing_condition: None,
            needs_review: false,
            dirty: true,
            submitted: None,
            priority_class: Class::Custom,
        }
    }
    pub(super) fn edit(base: DocumentStamp, rule: &Rule) -> Self {
        Self {
            token: Uuid::new_v4(),
            generation: Uuid::new_v4(),
            base,
            rule_id: Some(rule.id),
            name: rule.name.clone(),
            enabled: rule.enabled,
            target: Some(rule.target),
            conditions: rule
                .all_of
                .iter()
                .cloned()
                .map(|condition| ConditionEntry {
                    id: Uuid::new_v4(),
                    condition,
                })
                .collect(),
            form: ConditionForm::default(),
            editing_condition: None,
            needs_review: false,
            dirty: false,
            submitted: None,
            priority_class: rule.priority_class,
        }
    }
    pub(super) fn touch(&mut self) {
        self.dirty = true;
        self.submitted = None;
        self.generation = Uuid::new_v4();
    }
    pub(super) fn mark_submitted(&mut self, generation: Uuid, stamp: DocumentStamp) {
        if self.generation == generation {
            self.submitted = Some(stamp);
        } else {
            self.needs_review = true;
        }
    }
    pub(super) fn accept_creation(&mut self, id: Uuid, priority_class: Class) {
        self.rule_id = Some(id);
        self.priority_class = priority_class;
    }
    pub(super) fn priority_class(&self) -> Class {
        if self.rule_id.is_some() {
            return self.priority_class;
        }
        let conditions = self
            .conditions
            .iter()
            .map(|item| item.condition.clone())
            .collect();
        Rule::new(&self.name, self.target.unwrap_or(Target::Inbox), conditions).priority_class
    }
    pub(super) fn upsert_condition(&mut self, condition: Cond) {
        if let Some(id) = self.editing_condition.take()
            && let Some(existing) = self.conditions.iter_mut().find(|item| item.id == id)
        {
            existing.condition = condition;
            return;
        }
        self.conditions.push(ConditionEntry {
            id: Uuid::new_v4(),
            condition,
        });
    }
    pub(super) fn edit_condition(&mut self, id: Uuid) {
        if let Some(item) = self.conditions.iter().find(|item| item.id == id) {
            self.form = ConditionForm::from_condition(&item.condition);
            self.editing_condition = Some(id);
        }
    }
}

pub(super) fn view(
    component: &SettingsComponent,
    projected: &super::SettingsView,
    context: &mut ViewContext<SettingsComponent>,
) -> View {
    let draft = component.rule_draft.as_ref();
    let enabled = projected.writable;
    let targets = projected
        .contents
        .iter()
        .filter(|content| content.is_collection)
        .map(|content| content.title.clone())
        .collect::<Vec<_>>();
    let target_index = draft.and_then(|draft| draft.target).and_then(|target| {
        (0..targets.len()).find(|index| component.collection_target(*index) == Some(target))
    });
    let mut rows = vec![
        TextBlock::new().text(tr("自动整理")).into(),
        CheckBox::new()
            .content(tr("有新项目时自动归类"))
            .is_checked(projected.rules.keep_updated)
            .is_enabled(enabled)
            .on_is_checked_changed(
                context
                    .callback(|value: Option<bool>| Message::KeepUpdated(value.unwrap_or(false))),
            )
            .into(),
        ComboBox::new()
            .header(tr("默认目标"))
            .items_source(targets.clone())
            .selected_index(
                (0..targets.len()).find(|i| {
                    component.collection_target(*i) == Some(projected.rules.default_target)
                }),
            )
            .is_enabled(enabled && !targets.is_empty())
            .on_selection_changed(context.callback(Message::DefaultTarget))
            .into(),
        Button::new()
            .content(tr("立即应用规则"))
            .is_enabled(enabled)
            .on_click(context.message(Message::RunAction(Action::ApplyRules)))
            .into(),
        TextBlock::new().text(tr("快速添加内置模板")).into(),
    ];
    let templates = [
        ("images", "图片"),
        ("music", "音乐"),
        ("video", "视频"),
        ("archives", "压缩包"),
        ("installers", "安装包"),
        ("cleanup", "待清理"),
    ];
    rows.push(
        StackPanel::new()
            .orientation(windows_reactor::Orientation::Horizontal)
            .spacing(6.0)
            .children(
                templates
                    .into_iter()
                    .map(|(key, title)| {
                        Button::new()
                            .content(tr(title))
                            .is_enabled(
                                enabled
                                    && !projected
                                        .rules
                                        .list
                                        .iter()
                                        .any(|rule| rule.template.as_deref() == Some(key)),
                            )
                            .on_click(context.message(Message::RunAction(Action::AddTemplate {
                                template: key.into(),
                            })))
                            .into()
                    })
                    .collect::<Vec<_>>(),
            )
            .into(),
    );
    rows.push(TextBlock::new().text(tr("规则列表")).font_size(20.0).into());
    rows.push(
        StackPanel::new()
            .spacing(8.0)
            .keyed_children(
                projected
                    .rules
                    .list
                    .iter()
                    .enumerate()
                    .map(|(index, rule)| {
                        let up = (index > 0).then(|| projected.rules.list[index - 1].id);
                        let down = projected.rules.list.get(index + 2).map(|rule| rule.id);
                        keyed(
                            rule.id.to_string(),
                            StackPanel::new().spacing(4.0).children((
                                TextBlock::new().text(rule.name.clone()),
                                TextBlock::new()
                                    .text(
                                        rule.all_of
                                            .iter()
                                            .map(condition_label)
                                            .collect::<Vec<_>>()
                                            .join(" AND "),
                                    )
                                    .text_wrapping(windows_reactor::TextWrapping::Wrap),
                                CheckBox::new()
                                    .content(tr("启用"))
                                    .is_checked(rule.enabled)
                                    .is_enabled(enabled)
                                    .on_is_checked_changed(context.callback({
                                        let id = rule.id;
                                        move |value: Option<bool>| {
                                            Message::RuleEnabled(id, value.unwrap_or(false))
                                        }
                                    })),
                                StackPanel::new()
                                    .orientation(windows_reactor::Orientation::Horizontal)
                                    .spacing(4.0)
                                    .children((
                                        Button::new()
                                            .content(tr("编辑"))
                                            .is_enabled(enabled && component.rule_draft.is_none())
                                            .on_click(context.message(Message::RuleEdit(rule.id))),
                                        Button::new()
                                            .content(tr("上移"))
                                            .is_enabled(enabled && up.is_some())
                                            .on_click(
                                                context.message(Message::RuleMove(rule.id, up)),
                                            ),
                                        Button::new()
                                            .content(tr("下移"))
                                            .is_enabled(
                                                enabled && index + 1 < projected.rules.list.len(),
                                            )
                                            .on_click(
                                                context.message(Message::RuleMove(rule.id, down)),
                                            ),
                                        Button::new()
                                            .content(tr("删除"))
                                            .is_enabled(enabled)
                                            .on_click(
                                                context.message(Message::RuleDelete(rule.id)),
                                            ),
                                    )),
                            )),
                        )
                    }),
            )
            .into(),
    );
    rows.push(
        Button::new()
            .content(tr("新建规则"))
            .is_enabled(enabled && component.rule_draft.is_none())
            .on_click(context.message(Message::RuleNew))
            .into(),
    );
    if let Some(draft) = draft {
        rows.extend(editor_view(draft, &targets, target_index, enabled, context));
    }
    StackPanel::new().spacing(12.0).children(rows).into()
}

fn editor_view(
    draft: &Draft,
    targets: &[String],
    target_index: Option<usize>,
    enabled: bool,
    context: &mut ViewContext<SettingsComponent>,
) -> Vec<View> {
    let mut rows = vec![
        TextBlock::new()
            .text(if draft.rule_id.is_some() {
                tr("编辑规则")
            } else {
                tr("新建规则")
            })
            .font_size(20.0)
            .into(),
        TextBox::new(draft.name.clone())
            .header(tr("规则名称"))
            .is_enabled(enabled)
            .on_text_changed(
                context.callback(|value: Rc<str>| Message::RuleName(value.to_string())),
            )
            .into(),
        ComboBox::new()
            .header(tr("目标文件集合"))
            .items_source(targets.iter().cloned())
            .selected_index(target_index)
            .is_enabled(enabled)
            .on_selection_changed(context.callback(Message::RuleTarget))
            .into(),
        CheckBox::new()
            .content(tr("启用此规则"))
            .is_checked(draft.enabled)
            .is_enabled(enabled)
            .on_is_checked_changed(
                context.callback(|value: Option<bool>| {
                    Message::RuleEnabledDraft(value.unwrap_or(false))
                }),
            )
            .into(),
        ComboBox::new()
            .header(tr("条件类型"))
            .items_source(ConditionKind::ALL.iter().map(|kind| tr(kind.label())))
            .selected_index(Some(draft.form.kind.index()))
            .is_enabled(enabled)
            .on_selection_changed(
                context.callback(|index: Option<usize>| Message::ConditionKind(index.unwrap_or(0))),
            )
            .into(),
    ];
    rows.extend(form_fields(&draft.form, enabled, context));
    rows.push(
        Button::new()
            .content(if draft.editing_condition.is_some() {
                tr("更新条件")
            } else {
                tr("添加 AND 条件")
            })
            .is_enabled(enabled)
            .on_click(context.message(Message::AddCondition))
            .into(),
    );
    rows.push(
        TextBlock::new()
            .text(tr("所有条件都必须满足；每条规则支持 1–32 个 AND 条件。"))
            .into(),
    );
    rows.push(
        StackPanel::new()
            .spacing(6.0)
            .keyed_children(draft.conditions.iter().map(|entry| {
                keyed(
                    entry.id.to_string(),
                    StackPanel::new()
                        .orientation(windows_reactor::Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            TextBlock::new()
                                .text(condition_label(&entry.condition))
                                .text_wrapping(windows_reactor::TextWrapping::Wrap),
                            Button::new()
                                .content(tr("编辑条件"))
                                .is_enabled(enabled)
                                .on_click(context.message(Message::EditCondition(entry.id))),
                            Button::new()
                                .content(tr("删除条件"))
                                .is_enabled(enabled)
                                .on_click(context.message(Message::RemoveCondition(entry.id))),
                        )),
                )
            }))
            .into(),
    );
    rows.push(
        StackPanel::new()
            .orientation(windows_reactor::Orientation::Horizontal)
            .spacing(8.0)
            .children((
                Button::new()
                    .content(if draft.rule_id.is_some() {
                        tr("保存规则")
                    } else {
                        tr("添加规则")
                    })
                    .is_enabled(enabled && !draft.needs_review)
                    .on_click(context.message(Message::SaveRule)),
                Button::new()
                    .content(tr("取消编辑"))
                    .on_click(context.message(Message::CancelRule)),
            ))
            .into(),
    );
    rows
}

fn form_fields(
    form: &ConditionForm,
    enabled: bool,
    context: &mut ViewContext<SettingsComponent>,
) -> Vec<View> {
    match form.kind {
        ConditionKind::Type => type_checkboxes(&form.types, enabled, context),
        ConditionKind::CreatedWeekday => weekday_checkboxes(&form.weekdays, enabled, context),
        ConditionKind::Name | ConditionKind::ShortcutTarget => vec![
            ComboBox::new()
                .header(tr("匹配方式"))
                .items_source([
                    tr("包含"),
                    tr("不包含"),
                    tr("开头是"),
                    tr("结尾是"),
                    tr("等于"),
                ])
                .selected_index(Some(operation_index(form.operation)))
                .is_enabled(enabled)
                .on_selection_changed(context.callback(|index: Option<usize>| {
                    Message::ConditionOperation(index.unwrap_or(0))
                }))
                .into(),
            text_field(
                &form.text,
                "条件值",
                enabled,
                context.callback(|v: Rc<str>| Message::ConditionText(v.to_string())),
            ),
        ],
        ConditionKind::Extension | ConditionKind::ExactName | ConditionKind::Glob => {
            vec![text_field(
                &form.text,
                "值（多个项目用逗号分隔）",
                enabled,
                context.callback(|v: Rc<str>| Message::ConditionText(v.to_string())),
            )]
        }
        ConditionKind::FoldersOnly | ConditionKind::FilesOnly => {
            vec![TextBlock::new().text(tr("此条件不需要其他输入。")).into()]
        }
        ConditionKind::SizeMb => vec![
            text_field(
                &form.minimum,
                "最小 MB（可留空）",
                enabled,
                context.callback(|v: Rc<str>| Message::ConditionMinimum(v.to_string())),
            ),
            text_field(
                &form.maximum,
                "最大 MB（可留空）",
                enabled,
                context.callback(|v: Rc<str>| Message::ConditionMaximum(v.to_string())),
            ),
        ],
        ConditionKind::CreatedTime => vec![
            text_field(
                &form.time_from,
                "开始时间（HH:MM）",
                enabled,
                context.callback(|v: Rc<str>| Message::ConditionFrom(v.to_string())),
            ),
            text_field(
                &form.time_to,
                "结束时间（HH:MM）",
                enabled,
                context.callback(|v: Rc<str>| Message::ConditionTo(v.to_string())),
            ),
            TextBlock::new()
                .text(tr("结束时间早于开始时间表示跨越午夜。"))
                .into(),
        ],
        ConditionKind::Origin => {
            vec![
                ComboBox::new()
                    .header(tr("来源"))
                    .items_source([tr("用户桌面"), tr("公共桌面"), tr("系统项目")])
                    .selected_index(Some(match form.origin {
                        Origin::UserDesktop => 0,
                        Origin::PublicDesktop => 1,
                        Origin::Namespace => 2,
                    }))
                    .is_enabled(enabled)
                    .on_selection_changed(context.callback(|index: Option<usize>| {
                        Message::ConditionOrigin(index.unwrap_or(0))
                    }))
                    .into(),
            ]
        }
        ConditionKind::IdleDays => vec![text_field(
            &form.idle_days,
            "至少闲置天数",
            enabled,
            context.callback(|v: Rc<str>| Message::ConditionIdle(v.to_string())),
        )],
    }
}

fn type_checkboxes(
    selected: &[TypeCategory],
    enabled: bool,
    context: &mut ViewContext<SettingsComponent>,
) -> Vec<View> {
    type_categories()
        .iter()
        .map(|(category, label)| {
            let category = *category;
            CheckBox::new()
                .content(tr(label))
                .is_checked(selected.contains(&category))
                .is_enabled(enabled)
                .on_is_checked_changed(context.callback(move |value: Option<bool>| {
                    Message::ConditionType(category, value.unwrap_or(false))
                }))
                .into()
        })
        .collect()
}

fn weekday_checkboxes(
    selected: &[u8],
    enabled: bool,
    context: &mut ViewContext<SettingsComponent>,
) -> Vec<View> {
    ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        .into_iter()
        .enumerate()
        .map(|(index, label)| {
            let day = index as u8;
            CheckBox::new()
                .content(tr(label))
                .is_checked(selected.contains(&day))
                .is_enabled(enabled)
                .on_is_checked_changed(context.callback(move |value: Option<bool>| {
                    Message::ConditionWeekday(day, value.unwrap_or(false))
                }))
                .into()
        })
        .collect()
}

fn text_field(
    value: &str,
    label: &'static str,
    enabled: bool,
    callback: windows_reactor::Callback<Rc<str>>,
) -> View {
    TextBox::new(value)
        .header(tr(label))
        .is_enabled(enabled)
        .on_text_changed(callback)
        .into()
}

fn type_categories() -> [(TypeCategory, &'static str); 9] {
    [
        (TypeCategory::Programs, "程序"),
        (TypeCategory::Shortcuts, "快捷方式"),
        (TypeCategory::Folders, "文件夹"),
        (TypeCategory::Documents, "文档"),
        (TypeCategory::Images, "图片"),
        (TypeCategory::Music, "音乐"),
        (TypeCategory::Video, "视频"),
        (TypeCategory::Archives, "压缩包"),
        (TypeCategory::Installers, "安装包"),
    ]
}

fn condition_label(condition: &Cond) -> String {
    match condition {
        Cond::Type(values) => format!(
            "{}{}",
            tr("类型："),
            values
                .iter()
                .map(|value| type_name(*value))
                .collect::<Vec<_>>()
                .join("、")
        ),
        Cond::Ext(values) => format!("{}{}", tr("扩展名："), values.join(", ")),
        Cond::ExactName(values) => format!("{}{}", tr("文件名："), values.join(", ")),
        Cond::Name { op, value } => format!("{} {} “{}”", tr("名称"), operation_label(*op), value),
        Cond::Glob(value) => format!("{}{}", tr("通配符："), value),
        Cond::ShortcutTarget { op, value } => format!(
            "{} {} “{}”",
            tr("快捷方式目标"),
            operation_label(*op),
            value
        ),
        Cond::FoldersOnly => tr("仅文件夹"),
        Cond::FilesOnly => tr("仅文件"),
        Cond::SizeMb { min, max } => format!(
            "{} {}{}",
            tr("大小 MB"),
            min.map_or_else(|| "≥ —".into(), |v| format!("≥ {v}")),
            max.map_or_else(String::new, |v| format!("，≤ {v}"))
        ),
        Cond::CreatedTime { from_min, to_min } => format!(
            "{} {}–{}",
            tr("创建时间"),
            from_minute(*from_min),
            from_minute(*to_min)
        ),
        Cond::CreatedWeekday(days) => format!(
            "{} {}",
            tr("创建星期"),
            days.iter()
                .map(
                    |day| tr(["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
                        .get(*day as usize)
                        .copied()
                        .unwrap_or("?"))
                )
                .collect::<Vec<_>>()
                .join("、")
        ),
        Cond::Origin(origin) => format!("{}{}", tr("来源："), origin_label(*origin)),
        Cond::IdleDays { min } => format!("{} {} {}", tr("闲置至少"), min, tr("天")),
    }
}

fn type_name(value: TypeCategory) -> String {
    tr(type_categories()
        .iter()
        .find(|(category, _)| *category == value)
        .map(|(_, name)| *name)
        .unwrap_or("?"))
}
fn origin_label(value: Origin) -> String {
    tr(match value {
        Origin::UserDesktop => "用户桌面",
        Origin::PublicDesktop => "公共桌面",
        Origin::Namespace => "系统项目",
    })
}
fn operation_label(value: StrOp) -> String {
    tr(match value {
        StrOp::Contains => "包含",
        StrOp::NotContains => "不包含",
        StrOp::StartsWith => "开头是",
        StrOp::EndsWith => "结尾是",
        StrOp::Is => "等于",
    })
}
fn operation_at(index: usize) -> StrOp {
    match index {
        1 => StrOp::NotContains,
        2 => StrOp::StartsWith,
        3 => StrOp::EndsWith,
        4 => StrOp::Is,
        _ => StrOp::Contains,
    }
}
fn operation_index(value: StrOp) -> usize {
    match value {
        StrOp::Contains => 0,
        StrOp::NotContains => 1,
        StrOp::StartsWith => 2,
        StrOp::EndsWith => 3,
        StrOp::Is => 4,
    }
}
fn parse_number(value: &str) -> Result<Option<f64>, String> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite() && *number >= 0.0)
        .map(Some)
        .ok_or_else(|| tr("大小必须是大于或等于 0 的有效数字。"))
}
fn parse_time(value: &str) -> Result<u16, String> {
    let (hour, minute) = value
        .split_once(':')
        .ok_or_else(|| tr("时间格式应为 HH:MM。"))?;
    let hour = hour
        .parse::<u16>()
        .ok()
        .filter(|v| *v < 24)
        .ok_or_else(|| tr("小时必须在 00 至 23 之间。"))?;
    let minute = minute
        .parse::<u16>()
        .ok()
        .filter(|v| *v < 60)
        .ok_or_else(|| tr("分钟必须在 00 至 59 之间。"))?;
    Ok(hour * 60 + minute)
}
fn from_minute(value: u16) -> String {
    format!("{:02}:{:02}", value / 60, value % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn condition_forms_roundtrip_every_condition_variant() {
        let values = vec![
            Cond::Type(vec![TypeCategory::Images, TypeCategory::Folders]),
            Cond::Ext(vec!["png".into(), "jpg".into()]),
            Cond::ExactName(vec!["todo.txt".into(), "notes.md".into()]),
            Cond::Name {
                op: StrOp::NotContains,
                value: "backup".into(),
            },
            Cond::Glob("*.png".into()),
            Cond::ShortcutTarget {
                op: StrOp::StartsWith,
                value: "steam".into(),
            },
            Cond::FoldersOnly,
            Cond::FilesOnly,
            Cond::SizeMb {
                min: Some(1.5),
                max: Some(99.0),
            },
            Cond::CreatedTime {
                from_min: 1080,
                to_min: 360,
            },
            Cond::CreatedWeekday(vec![0, 2, 6]),
            Cond::Origin(Origin::Namespace),
            Cond::IdleDays { min: 30 },
        ];
        for condition in values {
            assert_eq!(
                ConditionForm::from_condition(&condition)
                    .to_condition()
                    .unwrap(),
                condition
            );
        }
    }
    #[test]
    fn ranges_times_and_empty_conditions_are_validated_before_submit() {
        let mut form = ConditionForm {
            kind: ConditionKind::SizeMb,
            ..ConditionForm::default()
        };
        form.minimum = "5".into();
        form.maximum = "1".into();
        assert!(form.to_condition().is_err());
        form.kind = ConditionKind::CreatedTime;
        form.time_from = "24:00".into();
        assert!(form.to_condition().is_err());
        form.kind = ConditionKind::Type;
        form.types.clear();
        assert!(form.to_condition().is_err());
    }
    #[test]
    fn drafts_keep_document_identity() {
        let stamp = DocumentStamp {
            workspace: Uuid::new_v4(),
            revision: 9,
        };
        let draft = Draft::new(stamp);
        assert_eq!(draft.base, stamp);
        assert!(draft.dirty && !draft.needs_review);
    }
}

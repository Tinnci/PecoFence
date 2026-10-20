//! Versioned default roles and pure placement policy. Business data is never a template input.
use crate::{
    ContentSpec, NormGeometry, WidgetSpan,
    geometry::{self, PxRect, WorkArea},
};
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DefaultRole {
    #[serde(rename = "default.programs")]
    Programs,
    #[serde(rename = "default.folders")]
    Folders,
    #[serde(rename = "default.documents")]
    Documents,
    #[serde(rename = "default.inbox")]
    Inbox,
    #[serde(rename = "default.clock")]
    Clock,
    #[serde(rename = "default.calendar")]
    Calendar,
    #[serde(rename = "default.note")]
    Note,
    #[serde(rename = "default.todo")]
    Todo,
}
impl DefaultRole {
    pub const ALL: [Self; 8] = [
        Self::Programs,
        Self::Folders,
        Self::Documents,
        Self::Inbox,
        Self::Clock,
        Self::Calendar,
        Self::Note,
        Self::Todo,
    ];
    pub fn provider(self) -> Option<&'static str> {
        match self {
            Self::Clock => Some("pecofence.clock"),
            Self::Calendar => Some("pecofence.calendar"),
            Self::Note => Some("pecofence.note"),
            Self::Todo => Some("pecofence.todo"),
            _ => None,
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Programs => "程序",
            Self::Folders => "文件夹",
            Self::Documents => "文件与文档",
            Self::Inbox => "桌面",
            Self::Clock => "时钟",
            Self::Calendar => "日历",
            Self::Note => "便笺",
            Self::Todo => "待办清单",
        }
    }
    pub fn accepts(self, content: &ContentSpec) -> bool {
        match content {
            ContentSpec::FileCollection { inbox, .. } => {
                self.provider().is_none() && (*inbox == (self == Self::Inbox))
            }
            ContentSpec::Panel { panel } => self.provider() == Some(panel.provider.as_str()),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultPresetOperation {
    RestoreLayout,
    AddMissingWidgets,
    RestoreAppearance,
}

#[derive(Clone, Debug)]
pub struct TemplateSize {
    pub role: DefaultRole,
    pub w: f32,
    pub h: f32,
    pub span: Option<WidgetSpan>,
}
#[derive(Clone, Debug)]
pub struct Placement {
    pub roles: Vec<DefaultRole>,
    pub geometry: NormGeometry,
    pub span: Option<WidgetSpan>,
}

/// Eight separate windows when they fit. Smaller work areas use tab groups; all
/// roles remain available and every generated rectangle stays inside the work area.
pub fn plan(work: &WorkArea, sizes: &[TemplateSize], file_width: f32) -> Vec<Placement> {
    let w = work.width_dip().max(64.0);
    let h = work.height_dip().max(36.0);
    let gap = 16.0f32.min(w / 16.0).min(h / 16.0);
    let available_w = w - gap * 2.0;
    let available_h = h - gap * 2.0;
    let size = |role| {
        sizes
            .iter()
            .find(|s| s.role == role)
            .expect("complete template metrics")
    };
    let mut result = Vec::new();
    let mut add = |roles: Vec<DefaultRole>, x: f32, y: f32, width: f32, height: f32| {
        let width = width
            .min(available_w)
            .max(geometry::MIN_W_DIP.min(available_w));
        let height = height
            .min(available_h)
            .max(geometry::MIN_H_DIP.min(available_h));
        let x = x.clamp(gap, w - gap - width);
        let y = y.clamp(gap, h - gap - height);
        let geometry = geometry::normalize(
            PxRect {
                left: work.left + (x * work.scale()).round() as i32,
                top: work.top + (y * work.scale()).round() as i32,
                right: work.left + ((x + width) * work.scale()).round() as i32,
                bottom: work.top + ((y + height) * work.scale()).round() as i32,
            },
            work,
        );
        let span = if roles.len() == 1 {
            size(roles[0]).span
        } else {
            None
        };
        result.push(Placement {
            roles,
            geometry,
            span,
        });
    };
    use DefaultRole::*;
    let left_w = size(Clock).w.max(size(Note).w);
    let middle_w = size(Calendar).w.max(size(Todo).w);
    let top_h = size(Clock).h.max(size(Calendar).h);
    let bottom_h = size(Note).h.max(size(Todo).h);
    let fw = file_width.min(available_w);
    if left_w + middle_w + fw + gap * 4.0 <= w && top_h + bottom_h + gap * 3.0 <= h {
        for (role, x, y) in [
            (Clock, gap, gap),
            (Calendar, gap * 2.0 + left_w, gap),
            (Note, gap, gap * 2.0 + top_h),
            (Todo, gap * 2.0 + left_w, gap * 2.0 + top_h),
        ] {
            add(vec![role], x, y, size(role).w, size(role).h);
        }
        let fh = (available_h - gap * 3.0) / 4.0;
        for (i, role) in [Programs, Folders, Documents, Inbox]
            .into_iter()
            .enumerate()
        {
            add(
                vec![role],
                w - gap - fw,
                gap + i as f32 * (fh + gap),
                fw,
                fh,
            );
        }
    } else {
        let ww = [Clock, Calendar, Note, Todo]
            .into_iter()
            .map(|r| size(r).w)
            .fold(0.0, f32::max);
        let wh = [Clock, Calendar, Note, Todo]
            .into_iter()
            .map(|r| size(r).h)
            .fold(0.0, f32::max)
            + 36.0;
        if ww + fw + gap * 3.0 <= w {
            add(vec![Clock, Calendar, Note, Todo], gap, gap, ww, wh);
            add(
                vec![Programs, Folders, Documents, Inbox],
                w - gap - fw,
                gap,
                fw,
                available_h,
            );
        } else {
            add(
                vec![
                    Clock, Calendar, Note, Todo, Programs, Folders, Documents, Inbox,
                ],
                gap,
                gap,
                available_w,
                available_h,
            );
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    fn sizes(factor: f32) -> Vec<TemplateSize> {
        DefaultRole::ALL
            .into_iter()
            .map(|role| TemplateSize {
                role,
                w: if role.provider().is_some() {
                    400.0 * factor
                } else {
                    240.0
                },
                h: if role.provider().is_some() {
                    384.0 * factor
                } else {
                    160.0
                },
                span: None,
            })
            .collect()
    }
    #[test]
    fn every_role_is_available_and_windows_stay_disjoint_inside_each_scaled_work_area() {
        for (width, height) in [(1920, 1040), (1280, 720), (768, 1024), (640, 480)] {
            for dpi in [96, 120, 144, 192] {
                for factor in [1.0, 1.5, 2.25] {
                    let work = WorkArea {
                        device_path: "monitor".into(),
                        left: -2400,
                        top: 32,
                        right: -2400 + width * dpi / 96,
                        bottom: 32 + height * dpi / 96,
                        dpi: dpi as u32,
                        mon_left: -2400,
                        mon_top: 0,
                        mon_right: 0,
                        mon_bottom: 1200,
                    };
                    let placement = plan(&work, &sizes(factor), 240.0);
                    let roles: Vec<_> = placement
                        .iter()
                        .flat_map(|p| p.roles.iter().copied())
                        .collect();
                    assert_eq!(roles.len(), 8);
                    assert_eq!(roles.into_iter().collect::<HashSet<_>>().len(), 8);
                    for (index, p) in placement.iter().enumerate() {
                        let rect = geometry::denormalize(&p.geometry, &work);
                        assert!(
                            rect.left >= work.left
                                && rect.top >= work.top
                                && rect.right <= work.right
                                && rect.bottom <= work.bottom
                        );
                        for other in &placement[..index] {
                            let b = geometry::denormalize(&other.geometry, &work);
                            assert!(
                                rect.right <= b.left
                                    || b.right <= rect.left
                                    || rect.bottom <= b.top
                                    || b.bottom <= rect.top
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn legacy_content_can_load_without_a_role_and_mismatched_or_duplicate_roles_are_rejected() {
        let mut inbox = crate::ContentInstance::collection("Inbox", true);
        let value = serde_json::to_value(&inbox).unwrap();
        assert!(value.get("defaultRole").is_none());
        inbox = serde_json::from_value(value).unwrap();
        inbox.default_role = Some(DefaultRole::Note);
        assert!(crate::model::validate_content(&inbox).is_err());
        inbox.default_role = Some(DefaultRole::Inbox);
        assert!(crate::model::validate_content(&inbox).is_ok());
        let mut programs = crate::ContentInstance::collection("Programs", false);
        programs.default_role = Some(DefaultRole::Programs);
        let mut duplicate = crate::ContentInstance::collection("Other", false);
        duplicate.default_role = Some(DefaultRole::Programs);
        let geometry = crate::NormGeometry {
            monitor: "test".into(),
            x: 0.0,
            y: 0.0,
            w: 240.0,
            h: 200.0,
            work_w: 1920.0,
            work_h: 1040.0,
            anchor: crate::Anchor::LeftTop,
        };
        let layout = crate::Layout {
            fingerprint: Vec::new(),
            containers: vec![
                crate::Container::new(inbox.id, geometry.clone()),
                crate::Container::new(programs.id, geometry.clone()),
                crate::Container::new(duplicate.id, geometry),
            ],
            contents: vec![inbox, programs, duplicate],
        };
        assert_eq!(layout.validate().unwrap_err(), "duplicate default role");
    }
}

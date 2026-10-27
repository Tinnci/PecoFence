//! CI/CD entry points. The tray, the capsule, the board drawer, the failure toast and the
//! hotkey all land here, so each one opens the same fence in the same way.
//!
//! SPM owns the data. A hidden monitor instance of the CI provider publishes snapshots to this
//! module; the fence and the board only present them.
use super::*;
use pecofence_plugin_spm::{
    board,
    ci_watch::{
        self,
        entry::{self, OpenIntent, Summary},
    },
};

/// The registered id of the CI/CD hotkey (distinct from Peek's id on the same window).
pub(super) const HOTKEY_CI: i32 = 2;
/// Ctrl+Alt, not repeating while held.
const CI_HOTKEY_MODS: u32 = pecofence_platform::hotkey::mods::CONTROL
    | pecofence_platform::hotkey::mods::ALT
    | pecofence_platform::hotkey::mods::NOREPEAT;

/// What the CI/CD hotkey should do after a settings change: release the held combination, then
/// register the wanted key if there is one. Nothing is done when the held key already matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HotkeyPlan {
    pub release: bool,
    pub register: Option<u32>,
}

pub(super) fn hotkey_plan(wanted: Option<u32>, held: Option<u32>) -> HotkeyPlan {
    if wanted == held {
        return HotkeyPlan {
            release: false,
            register: None,
        };
    }
    HotkeyPlan {
        release: held.is_some(),
        register: wanted,
    }
}

/// The settings page's view of the hotkey: off, registered, or the combination that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HotkeyState {
    Off,
    Registered(char),
    /// Another program or Windows holds the combination; the user should pick another letter.
    Conflict(char),
}

pub(super) fn hotkey_state(enabled: bool, held: Option<u32>, key: u32) -> HotkeyState {
    let letter = char::from_u32(key).unwrap_or('J');
    match (enabled, held) {
        (false, _) => HotkeyState::Off,
        (true, Some(_)) => HotkeyState::Registered(letter),
        (true, None) => HotkeyState::Conflict(letter),
    }
}

/// The fixed instance id of the hidden monitor (no fence, never drawn).
pub(super) const MONITOR_ID: u128 = 0x7e3c_5a1f_0000_4000_8000_6a1c_0000_0001;

/// What an entry does with the CI fence, decided from its current state alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CiRoute {
    /// No CI fence exists yet: create one near the cursor.
    Create,
    /// The fence was closed with its title-bar Close button: reopen it where it was.
    Reopen,
    /// The fence exists and is open: bring it to the front and show its tab.
    Activate,
    /// The hotkey toggles a fence that is already in front: hide it.
    Close,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CiFence {
    pub exists: bool,
    pub closed: bool,
    pub rolled: bool,
    /// The CI content is the active tab of its fence.
    pub active: bool,
}

/// Create vs reuse vs reopen. A rolled-up fence is activated (it unrolls), not closed, so the
/// hotkey always shows something visible when it finds a rolled fence.
pub(super) fn route(fence: CiFence, toggle: bool) -> CiRoute {
    if !fence.exists {
        CiRoute::Create
    } else if toggle && !fence.closed && !fence.rolled && fence.active {
        CiRoute::Close
    } else if fence.closed {
        CiRoute::Reopen
    } else {
        CiRoute::Activate
    }
}

/// Whether the UI is Simplified Chinese, for the few strings a view composes itself.
pub(super) fn zh_ui() -> bool {
    matches!(
        pecofence_core::i18n::language(),
        pecofence_core::i18n::Language::SimplifiedChinese
    )
}

impl App {
    /// Opens the hidden monitor. It keeps the tray counts and the capsule current even when no
    /// CI fence is open.
    pub(super) fn start_ci_monitor(&mut self) {
        let (version, mut config) = ci_watch::live_config();
        config["monitor"] = true.into();
        let spec = pecofence_core::PanelSpec {
            provider: ci_watch::PROVIDER_ID.into(),
            instance_id: uuid::Uuid::from_u128(MONITOR_ID),
            config_version: version,
            config,
        };
        if let Err(error) = self.panel_manager.borrow_mut().open_panel(&spec) {
            tracing::warn!(%error, "CI monitor unavailable");
        }
    }

    /// Tray label: the words, the running and failed counts (as glyphs and numbers, so colour is
    /// not the only carrier), and the shortcut when the hotkey is registered.
    pub(super) fn ci_tray_label(&self) -> String {
        let summary = Summary::from_snapshot(&self.ci_snapshot);
        let counts = if summary.visible() {
            format!("\t{}", summary.label(zh_ui()))
        } else {
            String::new()
        };
        let shortcut = self
            .ci_hotkey
            .map(|vk| format!("\tCtrl+Alt+{}", char::from_u32(vk).unwrap_or('J')))
            .unwrap_or_default();
        format!("CI/CD{counts}{shortcut}")
    }

    /// Registers the CI/CD hotkey when it is enabled and not yet registered under the chosen key.
    /// A combination another program holds stays unregistered; the settings page says so.
    pub(super) fn sync_ci_hotkey(&mut self) {
        let settings = &self.state.config.settings;
        let wanted = settings.ci_hotkey_enabled.then_some(settings.ci_hotkey_key);
        let plan = hotkey_plan(wanted, self.ci_hotkey);
        if !plan.release && plan.register.is_none() {
            return;
        }
        let hwnd = self.control.hwnd();
        if plan.release {
            hotkey::unregister(hwnd, HOTKEY_CI);
            self.ci_hotkey = None;
        }
        if let Some(vk) = plan.register
            && hotkey::register(hwnd, HOTKEY_CI, CI_HOTKEY_MODS, vk)
        {
            self.ci_hotkey = Some(vk);
        }
    }

    /// The settings page's status line: off, registered, or failed (with the reason in words).
    pub(super) fn ci_hotkey_status(&self) -> String {
        let settings = &self.state.config.settings;
        match hotkey_state(
            settings.ci_hotkey_enabled,
            self.ci_hotkey,
            settings.ci_hotkey_key,
        ) {
            HotkeyState::Off => pecofence_core::i18n::text("CI/CD 快捷键已关闭").into(),
            HotkeyState::Registered(key) => {
                pecofence_core::i18n::format("已注册 {0}", &[format!("Ctrl+Alt+{key}")])
            }
            HotkeyState::Conflict(key) => pecofence_core::i18n::format(
                "注册失败：{0} 已被系统或其他程序占用。请在下方换一个字母。",
                &[format!("Ctrl+Alt+{key}")],
            ),
        }
    }

    /// Hands a context to every panel of one provider (or one instance). The updates merge into
    /// the pending set and are applied by the same pass that delivered them.
    pub(super) fn deliver_ci_context(
        &mut self,
        provider: &str,
        id: Option<u128>,
        topic: &str,
        json: &str,
    ) {
        let updates = self
            .panel_manager
            .borrow()
            .context(provider, id, topic, json);
        for (key, update) in updates {
            self.pending_panel_updates
                .entry(key)
                .or_default()
                .merge(update);
        }
    }

    /// First-party events raised by panels (monitor snapshots, open requests).
    pub(super) fn ci_app_event(&mut self, topic: &str, json: &str) {
        match topic {
            "ci.snapshot" => {
                let Ok(snapshot) = serde_json::from_str::<spm_contracts::CiWatchSnapshot>(json)
                else {
                    return;
                };
                if snapshot.validate().is_err() {
                    return;
                }
                let failures = self.ci_failures.observe(
                    &snapshot,
                    self.state.config.settings.ci_failure_notifications,
                );
                self.ci_snapshot = snapshot;
                self.deliver_ci_context(board::PROVIDER_ID, None, topic, json);
                for build in failures {
                    self.notify_ci_failure(&build);
                }
            }
            "ci.open" => {
                let Ok(intent) = serde_json::from_str::<OpenIntent>(json) else {
                    return;
                };
                self.open_ci_watch(intent, false);
            }
            _ => {}
        }
    }

    /// The first CI fence in the workspace, with its host container.
    fn ci_fence(&self) -> Option<(ContentId, pecofence_core::PanelSpec)> {
        self.state.fences().iter().find_map(|f| match &f.content {
            pecofence_core::FenceContentSpec::Panel { panel }
                if panel.provider == ci_watch::PROVIDER_ID =>
            {
                Some((f.id, panel.clone()))
            }
            _ => None,
        })
    }

    /// Opens the CI/CD fence as the route says, then hands it the intent (edit and focus).
    pub(super) fn open_ci_watch(&mut self, intent: OpenIntent, toggle: bool) {
        if self.ci_fence().is_none() {
            let cursor = pecofence_platform::window::cursor_pos();
            let (x, y) = (cursor.x, cursor.y);
            self.new_spm_panel_near(true, x, y, None);
        }
        let Some((content, spec)) = self.ci_fence() else {
            return;
        };
        let Some(host) = self.state.host_of(content) else {
            return;
        };
        let container = self.state.container(host);
        let fence = CiFence {
            exists: true,
            closed: container.is_some_and(|c| c.closed),
            rolled: self.fences.get(&host).is_some_and(|w| w.is_rolled()),
            active: container.is_some_and(|c| c.active_tab == content),
        };
        if route(fence, toggle) == CiRoute::Close {
            self.close_fence(host);
            return;
        }
        if fence.closed {
            self.reopen_fence(host);
        }
        self.switch_tab(host, content);
        if self.fences.get(&host).is_some_and(|w| w.is_rolled()) {
            self.toggle_roll(host);
        }
        if let Some(w) = self.fences.get(&host) {
            window::bring_to_front(w.hwnd());
            window::set_focus(w.hwnd());
        }
        let Ok(handle) = self.panel_manager.borrow_mut().open_panel(&spec) else {
            return;
        };
        let json = serde_json::to_string(&intent).unwrap_or_default();
        self.deliver_ci_context(
            ci_watch::PROVIDER_ID,
            Some(handle.key().id),
            "ci.open",
            &json,
        );
    }

    /// One balloon per failed build. Windows decides whether it shows (focus assist, quiet hours);
    /// PecoFence only decides whether a failure is news.
    fn notify_ci_failure(&mut self, build: &spm_contracts::CiBuild) {
        let number = build.number.map(|n| format!(" #{n}")).unwrap_or_default();
        let text = pecofence_core::i18n::format(
            "{0}{1} 构建失败。点击在 CI/CD 中查看。",
            &[build.job.clone(), number],
        );
        if let Some(tray) = &self.tray {
            tray.show_info("PecoFence · CI/CD", &text, true);
        }
        self.ci_last_failure = Some(entry::build_key(build));
    }

    /// A click on the failure balloon opens the CI/CD fence at the build it announced.
    pub(super) fn ci_failure_clicked(&mut self) {
        let Some(focus) = self.ci_last_failure.take() else {
            return;
        };
        self.open_ci_watch_now(
            OpenIntent {
                focus: Some(focus),
                edit: None,
            },
            false,
        );
    }

    /// Entry points that are not already inside a panel-event pass (tray, hotkey) call this so
    /// the context updates they produced are applied now.
    pub(super) fn open_ci_watch_now(&mut self, intent: OpenIntent, toggle: bool) {
        self.open_ci_watch(intent, toggle);
        self.process_panel_events();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN_ACTIVE: CiFence = CiFence {
        exists: true,
        closed: false,
        rolled: false,
        active: true,
    };

    #[test]
    fn without_a_fence_every_entry_creates_one() {
        assert_eq!(route(CiFence::default(), false), CiRoute::Create);
        assert_eq!(route(CiFence::default(), true), CiRoute::Create);
    }

    #[test]
    fn an_open_fence_is_reused_and_brought_forward() {
        let behind = CiFence {
            active: false,
            ..OPEN_ACTIVE
        };
        assert_eq!(route(OPEN_ACTIVE, false), CiRoute::Activate);
        assert_eq!(route(behind, false), CiRoute::Activate);
    }

    #[test]
    fn a_closed_fence_is_reopened_not_duplicated() {
        let closed = CiFence {
            closed: true,
            ..OPEN_ACTIVE
        };
        assert_eq!(route(closed, false), CiRoute::Reopen);
        assert_eq!(route(closed, true), CiRoute::Reopen);
    }

    #[test]
    fn a_rolled_fence_unrolls_instead_of_hiding() {
        let rolled = CiFence {
            rolled: true,
            ..OPEN_ACTIVE
        };
        assert_eq!(route(rolled, true), CiRoute::Activate);
    }

    #[test]
    fn the_hotkey_hides_only_the_fence_already_in_front() {
        assert_eq!(route(OPEN_ACTIVE, true), CiRoute::Close);
        let behind = CiFence {
            active: false,
            ..OPEN_ACTIVE
        };
        assert_eq!(route(behind, true), CiRoute::Activate);
    }

    #[test]
    fn a_held_combination_is_released_before_a_new_one_is_registered() {
        assert_eq!(
            hotkey_plan(None, None),
            HotkeyPlan {
                release: false,
                register: None
            }
        );
        assert_eq!(
            hotkey_plan(Some(0x4A), None),
            HotkeyPlan {
                release: false,
                register: Some(0x4A)
            }
        );
        assert_eq!(
            hotkey_plan(Some(0x4B), Some(0x4A)),
            HotkeyPlan {
                release: true,
                register: Some(0x4B)
            }
        );
        assert_eq!(
            hotkey_plan(None, Some(0x4A)),
            HotkeyPlan {
                release: true,
                register: None
            }
        );
        assert_eq!(
            hotkey_plan(Some(0x4A), Some(0x4A)),
            HotkeyPlan {
                release: false,
                register: None
            },
            "an unchanged hotkey is not re-registered on every settings apply"
        );
    }

    #[test]
    fn a_taken_combination_is_reported_as_a_conflict_not_silently_dropped() {
        assert_eq!(hotkey_state(false, None, 0x4A), HotkeyState::Off);
        assert_eq!(hotkey_state(false, Some(0x4A), 0x4A), HotkeyState::Off);
        assert_eq!(
            hotkey_state(true, Some(0x4A), 0x4A),
            HotkeyState::Registered('J')
        );
        assert_eq!(hotkey_state(true, None, 0x4A), HotkeyState::Conflict('J'));
        assert_eq!(hotkey_state(true, None, 0x4C), HotkeyState::Conflict('L'));
    }
}

//! Small, stable command contract shared by the Explorer COM DLL and PecoFence.

pub const CONTROL_CLASS: &str = "PecoFence.Control";
pub const CONTROL_TITLE_MAIN: &str = "PecoFence.Control.main";
/// WM_APP + 15; keep in sync with the control window's message range.
pub const WM_APP_SHELL_COMMAND: u32 = 0x800f;
/// WM_APP + 16; carries signed 32-bit screen coordinates in LPARAM.
pub const WM_APP_SHELL_COMMAND_AT: u32 = 0x8010;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ShellAction {
    NewFence = 1,
    NewTodo = 2,
    OpenSettings = 3,
    NewNote = 4,
}

impl ShellAction {
    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Self::NewFence),
            2 => Some(Self::NewTodo),
            3 => Some(Self::OpenSettings),
            4 => Some(Self::NewNote),
            _ => None,
        }
    }

    pub const fn cli_name(self) -> &'static str {
        match self {
            Self::NewFence => "new-fence",
            Self::NewTodo => "new-todo",
            Self::OpenSettings => "settings",
            Self::NewNote => "new-note",
        }
    }

    pub fn from_cli_name(name: &str) -> Option<Self> {
        match name {
            "new-fence" => Some(Self::NewFence),
            "new-todo" => Some(Self::NewTodo),
            "settings" => Some(Self::OpenSettings),
            "new-note" => Some(Self::NewNote),
            _ => None,
        }
    }
}

/// A fixed Explorer action and the pointer position captured when it was invoked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellCommand {
    pub action: ShellAction,
    pub x: i32,
    pub y: i32,
}

impl ShellCommand {
    pub const fn new(action: ShellAction, x: i32, y: i32) -> Self {
        Self { action, x, y }
    }

    /// Lossless on the x64 Windows target, including monitors left/above the primary.
    pub const fn point_lparam(self) -> isize {
        (((self.y as u32 as u64) << 32) | self.x as u32 as u64) as isize
    }

    pub const fn from_wire(action_id: usize, lparam: isize) -> Option<Self> {
        if action_id > u32::MAX as usize {
            return None;
        }
        match ShellAction::from_id(action_id as u32) {
            Some(action) => Some(Self::new(
                action,
                lparam as u32 as i32,
                ((lparam as u64) >> 32) as u32 as i32,
            )),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_and_wire_values_round_trip() {
        for id in 1..=4 {
            let action = ShellAction::from_id(id).unwrap();
            assert_eq!(ShellAction::from_cli_name(action.cli_name()), Some(action));
        }
        assert_eq!(ShellAction::from_id(0), None);
        assert_eq!(ShellAction::from_id(5), None);
        assert_eq!(ShellAction::from_cli_name("quit"), None);
    }

    #[test]
    fn screen_position_round_trips_full_signed_range() {
        for (x, y) in [(0, 0), (-1920, 1440), (7680, -2160), (i32::MIN, i32::MAX)] {
            let command = ShellCommand::new(ShellAction::NewTodo, x, y);
            assert_eq!(
                ShellCommand::from_wire(command.action as usize, command.point_lparam()),
                Some(command)
            );
        }
        assert_eq!(ShellCommand::from_wire(0, 0), None);
        assert_eq!(ShellCommand::from_wire(usize::MAX, 0), None);
    }
}

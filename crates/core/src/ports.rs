//! Platform capabilities consumed by command handlers. Native window owners are
//! opaque identities; core code never dereferences them or imports Win32 types.
use std::path::{Path, PathBuf};

pub type PortResult<T> = Result<T, String>;

pub trait ShellPort: Send + Sync {
    fn open(&self, path: &Path, verb: Option<&str>, owner: Option<isize>) -> PortResult<()>;
    fn create_shortcut(&self, target: &Path, directory: &Path) -> PortResult<PathBuf>;
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()>;
    /// True means every operation completed; false preserves user cancellation.
    fn transfer(
        &self,
        paths: &[PathBuf],
        destination: &Path,
        owner: Option<isize>,
        copy: bool,
        rename_on_collision: bool,
    ) -> PortResult<bool>;
}

pub trait ClipboardPort: Send + Sync {
    fn sequence(&self) -> u32;
    fn files(&self) -> Option<(Vec<PathBuf>, bool)>;
    fn has_files(&self) -> bool;
    fn clear(&self);
    fn write_text(&self, text: &str) -> PortResult<()>;
}

/// Event delivery is deferred until the UI state borrow has ended.
pub trait AccessibilityNotifications {
    fn queue_invoked(&self, id: &[i32]) -> bool;
    fn flush_events(&self);
}

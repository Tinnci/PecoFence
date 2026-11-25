//! Native adapters selected by the application composition root.
use pecofence_core::ports::{ClipboardPort, PortResult, ShellPort};
use std::path::{Path, PathBuf};

pub struct WindowsShell;
fn owner(value: Option<isize>) -> Option<crate::HWND> {
    value.map(|value| crate::HWND(value as *mut core::ffi::c_void))
}
impl ShellPort for WindowsShell {
    fn open(&self, path: &Path, verb: Option<&str>, window: Option<isize>) -> PortResult<()> {
        crate::shell::shell_execute(path, verb, owner(window)).map_err(|e| e.to_string())
    }
    fn create_shortcut(&self, target: &Path, directory: &Path) -> PortResult<PathBuf> {
        crate::shell::create_shortcut(target, directory).map_err(|e| e.to_string())
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        crate::shell::rename_no_replace(from, to)
    }
    fn transfer(
        &self,
        paths: &[PathBuf],
        destination: &Path,
        window: Option<isize>,
        copy: bool,
        rename_on_collision: bool,
    ) -> PortResult<bool> {
        crate::shell::transfer_to_folder(
            paths,
            destination,
            owner(window),
            copy,
            rename_on_collision,
        )
        .map_err(|e| e.to_string())
    }
}
pub struct WindowsClipboard;
impl ClipboardPort for WindowsClipboard {
    fn sequence(&self) -> u32 {
        crate::clipboard::sequence()
    }
    fn files(&self) -> Option<(Vec<PathBuf>, bool)> {
        crate::clipboard::file_list()
    }
    fn has_files(&self) -> bool {
        crate::clipboard::has_file_list()
    }
    fn clear(&self) {
        crate::clipboard::clear()
    }
    fn write_text(&self, text: &str) -> PortResult<()> {
        crate::clipboard::set_text(text).map_err(|e| e.to_string())
    }
}
impl pecofence_core::ports::AccessibilityNotifications for crate::accessibility::Publisher {
    fn queue_invoked(&self, id: &[i32]) -> bool {
        self.queue_invoked(&id.to_vec())
    }
    fn flush_events(&self) {
        self.flush_events();
    }
}

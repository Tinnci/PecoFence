//! Windows 11 Explorer context-menu COM server. Menu construction never calls the app.

#![cfg(windows)]
#![allow(non_snake_case)]

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    clippy::upper_case_acronyms,
    clippy::missing_transmute_annotations
)]
mod bindings;

use bindings::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_NOTIMPL, E_POINTER, ECF_HASSUBCOMMANDS,
    ECS_ENABLED, IBindCtx, IClassFactory, IClassFactory_Impl, IEnumExplorerCommand,
    IEnumExplorerCommand_Impl, IExplorerCommand, IExplorerCommand_Impl, IShellItemArray, S_FALSE,
};
use pecofence_shell_contract::{
    CONTROL_CLASS, CONTROL_TITLE_MAIN, ShellAction, ShellCommand, WM_APP_SHELL_COMMAND,
    WM_APP_SHELL_COMMAND_AT,
};
use std::cell::Cell;
use std::ffi::c_void;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows_core::{
    BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, PWSTR, Ref, Result, implement,
};

/// Also declared in the MSIX manifest.
const COMMAND_CLSID: GUID = GUID::from_u128(0xeb39d6c6_6c5e_4e59_a8bb_fd88c429f78f);
const FENCE_GUID: GUID = GUID::from_u128(0xeb39d6c6_6c5e_4e59_a8bb_fd88c429f780);
const TODO_GUID: GUID = GUID::from_u128(0xeb39d6c6_6c5e_4e59_a8bb_fd88c429f781);
const SETTINGS_GUID: GUID = GUID::from_u128(0xeb39d6c6_6c5e_4e59_a8bb_fd88c429f782);
const E_OUTOFMEMORY: HRESULT = HRESULT(0x8007000e_u32 as i32);
const CHILD_ACTIONS: [ShellAction; 3] = [
    ShellAction::NewFence,
    ShellAction::NewTodo,
    ShellAction::OpenSettings,
];

static MODULE: AtomicIsize = AtomicIsize::new(0);
static OBJECTS: AtomicU32 = AtomicU32::new(0);
static SERVER_LOCKS: AtomicU32 = AtomicU32::new(0);

struct LiveObject;

impl LiveObject {
    fn new() -> Self {
        OBJECTS.fetch_add(1, Ordering::Relaxed);
        Self
    }
}

impl Drop for LiveObject {
    fn drop(&mut self) {
        OBJECTS.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Root,
    Action(ShellAction),
}

#[implement(IExplorerCommand)]
struct ExplorerCommand {
    _live: LiveObject,
    kind: Kind,
}

impl ExplorerCommand {
    fn new(kind: Kind) -> Self {
        Self {
            _live: LiveObject::new(),
            kind,
        }
    }
}

// Keep this tiny and allocation-free on Explorer's menu-construction path. Titles mirror
// the three existing locale keys in locales/*.json; Chinese simplified is the source locale.
fn localized_title(kind: Kind, language: u16) -> &'static str {
    if matches!(kind, Kind::Root) {
        return "PecoFence";
    }
    let titles = match language {
        0x0404 | 0x0c04 | 0x1404 => ["新增圍欄", "新增待辦清單", "設定…"],
        lang if lang & 0x03ff == 0x04 => ["新建栅栏", "新建待办清单", "设置…"],
        lang if lang & 0x03ff == 0x07 => ["Neuer Bereich", "Neue Aufgabenliste", "Einstellungen…"],
        lang if lang & 0x03ff == 0x0a => ["Nuevo grupo", "Nueva lista de tareas", "Configuración…"],
        lang if lang & 0x03ff == 0x0c => {
            ["Nouveau groupe", "Nouvelle liste de tâches", "Paramètres…"]
        }
        lang if lang & 0x03ff == 0x11 => ["フェンスを作成", "新しいToDoリスト", "設定…"],
        lang if lang & 0x03ff == 0x12 => ["새 펜스", "새 할 일 목록", "설정…"],
        lang if lang & 0x03ff == 0x16 => ["Novo grupo", "Nova lista de tarefas", "Configurações…"],
        lang if lang & 0x03ff == 0x19 => ["Создать область", "Новый список задач", "Настройки…"],
        _ => ["New fence", "New to-do list", "Settings…"],
    };
    match kind {
        Kind::Action(ShellAction::NewFence) => titles[0],
        Kind::Action(ShellAction::NewTodo) => titles[1],
        Kind::Action(ShellAction::OpenSettings) => titles[2],
        Kind::Root => unreachable!(),
    }
}

impl IExplorerCommand_Impl for ExplorerCommand_Impl {
    fn GetTitle(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        // SAFETY: GetUserDefaultUILanguage is a parameterless Win32 query.
        taskmem_string(localized_title(self.kind, unsafe {
            GetUserDefaultUILanguage()
        }))
    }

    fn GetIcon(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetToolTip(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(match self.kind {
            Kind::Root => COMMAND_CLSID,
            Kind::Action(ShellAction::NewFence) => FENCE_GUID,
            Kind::Action(ShellAction::NewTodo) => TODO_GUID,
            Kind::Action(ShellAction::OpenSettings) => SETTINGS_GUID,
        })
    }

    fn GetState(&self, _: Ref<IShellItemArray>, _: BOOL) -> Result<u32> {
        Ok(ECS_ENABLED)
    }

    fn Invoke(&self, _: Ref<IShellItemArray>, _: Ref<IBindCtx>) -> Result<()> {
        match self.kind {
            Kind::Root => Err(E_NOTIMPL.into()),
            Kind::Action(action) => invoke_action(action),
        }
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(if matches!(self.kind, Kind::Root) {
            ECF_HASSUBCOMMANDS
        } else {
            0
        })
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        if !matches!(self.kind, Kind::Root) {
            return Err(E_NOTIMPL.into());
        }
        Ok(ExplorerCommands::new(0).into())
    }
}

#[implement(IEnumExplorerCommand)]
struct ExplorerCommands {
    _live: LiveObject,
    next: Cell<usize>,
}

impl ExplorerCommands {
    fn new(next: usize) -> Self {
        Self {
            _live: LiveObject::new(),
            next: Cell::new(next),
        }
    }
}

impl IEnumExplorerCommand_Impl for ExplorerCommands_Impl {
    fn Next(
        &self,
        celt: u32,
        puicommand: *mut Option<IExplorerCommand>,
        pceltfetched: *mut u32,
    ) -> Result<()> {
        if (celt > 0 && puicommand.is_null()) || (celt > 1 && pceltfetched.is_null()) {
            return Err(E_POINTER.into());
        }
        let mut count = 0u32;
        while count < celt && self.next.get() < CHILD_ACTIONS.len() {
            let action = CHILD_ACTIONS[self.next.get()];
            let command: IExplorerCommand = ExplorerCommand::new(Kind::Action(action)).into();
            // SAFETY: Explorer allocated `celt` writable slots.
            unsafe { puicommand.add(count as usize).write(Some(command)) };
            self.next.set(self.next.get() + 1);
            count += 1;
        }
        if !pceltfetched.is_null() {
            // SAFETY: COM supplied a writable fetched-count pointer.
            unsafe { pceltfetched.write(count) };
        }
        if count == celt {
            Ok(())
        } else {
            Err(S_FALSE.into())
        }
    }

    fn Skip(&self, celt: u32) -> Result<()> {
        let next = self.next.get().saturating_add(celt as usize);
        self.next.set(next.min(CHILD_ACTIONS.len()));
        if next <= CHILD_ACTIONS.len() {
            Ok(())
        } else {
            Err(S_FALSE.into())
        }
    }

    fn Reset(&self) -> Result<()> {
        self.next.set(0);
        Ok(())
    }

    fn Clone(&self) -> Result<IEnumExplorerCommand> {
        Ok(ExplorerCommands::new(self.next.get()).into())
    }
}

#[implement(IClassFactory)]
struct CommandFactory {
    _live: LiveObject,
}

impl IClassFactory_Impl for CommandFactory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        if riid.is_null() || ppvobject.is_null() {
            return Err(E_POINTER.into());
        }
        let command: IExplorerCommand = ExplorerCommand::new(Kind::Root).into();
        // SAFETY: COM supplied valid output and IID pointers, checked above.
        unsafe { command.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            SERVER_LOCKS.fetch_add(1, Ordering::Relaxed);
        } else {
            let _ =
                SERVER_LOCKS.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1));
        }
        Ok(())
    }
}

fn taskmem_string(value: &str) -> Result<PWSTR> {
    let utf16: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: task allocator returns an independent buffer transferred to Explorer.
    let ptr = unsafe { bindings::CoTaskMemAlloc(utf16.len() * size_of::<u16>()) }.cast::<u16>();
    if ptr.is_null() {
        return Err(E_OUTOFMEMORY.into());
    }
    // SAFETY: the allocated buffer holds the complete UTF-16 string and terminator.
    unsafe { ptr.copy_from_nonoverlapping(utf16.as_ptr(), utf16.len()) };
    Ok(PWSTR(ptr))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delivery {
    Delivered,
    NoInstance,
    Uncertain,
}

fn send_to_running(command: ShellCommand) -> Delivery {
    let class = wide(CONTROL_CLASS);
    let title = wide(CONTROL_TITLE_MAIN);
    let mut accepted = 0usize;
    // SAFETY: user32 receives NUL-terminated strings and only fixed integer message data.
    unsafe {
        let hwnd = FindWindowW(PCWSTR(class.as_ptr()), PCWSTR(title.as_ptr()));
        if hwnd.is_null() {
            return Delivery::NoInstance;
        }
        if SendMessageTimeoutW(
            hwnd,
            WM_APP_SHELL_COMMAND_AT,
            command.action as usize,
            command.point_lparam(),
            0x0002,
            500,
            &mut accepted,
        ) == 0
        {
            return Delivery::Uncertain;
        }
        if accepted == 1 {
            return Delivery::Delivered;
        }
        // Allow an older main app to process commands during a staged package update.
        if SendMessageTimeoutW(
            hwnd,
            WM_APP_SHELL_COMMAND,
            command.action as usize,
            0,
            0x0002,
            500,
            &mut accepted,
        ) != 0
            && accepted == 1
        {
            Delivery::Delivered
        } else {
            Delivery::Uncertain
        }
    }
}

fn module_directory() -> Option<PathBuf> {
    let module = MODULE.load(Ordering::Relaxed);
    if module == 0 {
        return None;
    }
    let mut path = vec![0u16; 512];
    loop {
        // SAFETY: `module` is the HINSTANCE saved by DllMain; the buffer is writable.
        let len = unsafe {
            GetModuleFileNameW(module as *mut c_void, path.as_mut_ptr(), path.len() as u32)
        };
        if len == 0 {
            return None;
        }
        if (len as usize) < path.len() - 1 {
            return PathBuf::from(std::ffi::OsString::from_wide(&path[..len as usize]))
                .parent()
                .map(PathBuf::from);
        }
        if path.len() >= 32_768 {
            return None;
        }
        path.resize(path.len() * 2, 0);
    }
}

fn invoke_action(action: ShellAction) -> Result<()> {
    let mut point = ScreenPoint { x: 0, y: 0 };
    // SAFETY: user32 writes one POINT into our stack value.
    if !unsafe { GetCursorPos(&mut point) }.as_bool() {
        return Err(windows_core::Error::from_hresult(HRESULT(
            0x80004005_u32 as i32,
        )));
    }
    let command = ShellCommand::new(action, point.x, point.y);
    match send_to_running(command) {
        Delivery::Delivered => return Ok(()),
        Delivery::Uncertain => {
            // A timeout does not prove non-delivery. Spawning a second process could create
            // the fence twice when the existing app catches up.
            return Err(windows_core::Error::from_hresult(HRESULT(
                0x80004005_u32 as i32,
            )));
        }
        Delivery::NoInstance => {}
    }
    let executable = module_directory()
        .ok_or_else(|| windows_core::Error::from_hresult(E_NOTIMPL))?
        .join("pecofence.exe");
    std::process::Command::new(executable)
        .arg("--shell-command")
        .arg(action.cli_name())
        .arg("--shell-x")
        .arg(command.x.to_string())
        .arg("--shell-y")
        .arg(command.y.to_string())
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            windows_core::Error::new(HRESULT(0x80004005_u32 as i32), error.to_string())
        })
}

#[repr(C)]
struct ScreenPoint {
    x: i32,
    y: i32,
}

windows_core::link!("user32.dll" "system" fn FindWindowW(class: PCWSTR, title: PCWSTR) -> *mut c_void);
windows_core::link!("user32.dll" "system" fn SendMessageTimeoutW(hwnd: *mut c_void, message: u32, wparam: usize, lparam: isize, flags: u32, timeout: u32, result: *mut usize) -> isize);
windows_core::link!("user32.dll" "system" fn GetCursorPos(point: *mut ScreenPoint) -> BOOL);
windows_core::link!("kernel32.dll" "system" fn GetModuleFileNameW(module: *mut c_void, path: *mut u16, capacity: u32) -> u32);
windows_core::link!("kernel32.dll" "system" fn GetUserDefaultUILanguage() -> u16);

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(module: *mut c_void, reason: u32, _: *mut c_void) -> BOOL {
    if reason == 1 {
        MODULE.store(module as isize, Ordering::Relaxed);
    }
    BOOL(1)
}

#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OBJECTS.load(Ordering::Relaxed) == 0 && SERVER_LOCKS.load(Ordering::Relaxed) == 0 {
        HRESULT(0)
    } else {
        S_FALSE
    }
}

#[unsafe(no_mangle)]
/// Returns the Explorer command factory for the manifest CLSID.
///
/// # Safety
///
/// COM must pass valid GUID pointers and a writable output pointer. Null pointers are
/// rejected; other invalid addresses cannot be checked here.
pub unsafe extern "system" fn DllGetClassObject(
    clsid: *const GUID,
    iid: *const GUID,
    object: *mut *mut c_void,
) -> HRESULT {
    if clsid.is_null() || iid.is_null() || object.is_null() {
        return E_POINTER;
    }
    // SAFETY: COM passed pointers to GUIDs and a writable output slot, checked above.
    unsafe {
        object.write(std::ptr::null_mut());
        if *clsid != COMMAND_CLSID {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = CommandFactory {
            _live: LiveObject::new(),
        }
        .into();
        factory.query(iid, object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn com_factory_exposes_three_ordered_commands_and_releases_objects() {
        let mut raw = std::ptr::null_mut();
        assert_eq!(
            // SAFETY: all pointers refer to live values and `raw` is writable.
            unsafe { DllGetClassObject(&COMMAND_CLSID, &IClassFactory::IID, &mut raw) },
            HRESULT(0)
        );
        assert!(!raw.is_null());
        assert_eq!(DllCanUnloadNow(), S_FALSE);
        // SAFETY: DllGetClassObject returned an owned IClassFactory pointer.
        let factory = unsafe { IClassFactory::from_raw(raw) };
        let root: IExplorerCommand = unsafe { factory.CreateInstance(None::<&IUnknown>) }.unwrap();
        assert_eq!(unsafe { root.GetFlags() }.unwrap(), ECF_HASSUBCOMMANDS);
        assert_eq!(unsafe { root.GetCanonicalName() }.unwrap(), COMMAND_CLSID);
        let commands = unsafe { root.EnumSubCommands() }.unwrap();
        let mut children: [Option<IExplorerCommand>; 3] = std::array::from_fn(|_| None);
        let mut fetched = 0;
        assert_eq!(
            unsafe { commands.Next(3, children.as_mut_ptr(), Some(&mut fetched)) },
            HRESULT(0)
        );
        assert_eq!(fetched, 3);
        for (child, expected) in children.iter().zip([FENCE_GUID, TODO_GUID, SETTINGS_GUID]) {
            let child = child.as_ref().unwrap();
            assert_eq!(unsafe { child.GetCanonicalName() }.unwrap(), expected);
            assert_eq!(unsafe { child.GetFlags() }.unwrap(), 0);
            assert_eq!(
                unsafe { child.GetState(None::<&IShellItemArray>, false) }.unwrap(),
                ECS_ENABLED
            );
        }
        let mut extra = None;
        assert_eq!(
            unsafe { commands.Next(1, &mut extra, Some(&mut fetched)) },
            S_FALSE
        );
        assert_eq!(fetched, 0);
        drop(children);
        drop(commands);
        drop(root);
        drop(factory);
        assert_eq!(DllCanUnloadNow(), HRESULT(0));
    }

    #[test]
    fn titles_follow_the_supported_ui_languages() {
        assert_eq!(
            localized_title(Kind::Action(ShellAction::NewFence), 0x0804),
            "新建栅栏"
        );
        assert_eq!(
            localized_title(Kind::Action(ShellAction::NewTodo), 0x0404),
            "新增待辦清單"
        );
        assert_eq!(
            localized_title(Kind::Action(ShellAction::OpenSettings), 0x0409),
            "Settings…"
        );
    }
}

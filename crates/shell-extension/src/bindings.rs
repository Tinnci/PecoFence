#[inline]
pub unsafe fn CoTaskMemAlloc(cb: usize) -> *mut core::ffi::c_void {
    windows_core::link!("ole32.dll" "system" fn CoTaskMemAlloc(cb : usize) -> *mut core::ffi::c_void);
    unsafe { CoTaskMemAlloc(cb) }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BIND_OPTS {
    pub cbStruct: u32,
    pub grfFlags: u32,
    pub grfMode: u32,
    pub dwTickCountDeadline: u32,
}
pub const CLASS_E_CLASSNOTAVAILABLE: windows_core::HRESULT =
    windows_core::HRESULT(0x80040111_u32 as _);
pub const CLASS_E_NOAGGREGATION: windows_core::HRESULT = windows_core::HRESULT(0x80040110_u32 as _);
pub const ECF_HASSUBCOMMANDS: EXPCMDFLAGS = 1;
pub const ECS_ENABLED: EXPCMDSTATE = 0;
pub type EXPCMDFLAGS = u32;
pub type EXPCMDSTATE = u32;
pub const E_NOTIMPL: windows_core::HRESULT = windows_core::HRESULT(0x80004001_u32 as _);
pub const E_POINTER: windows_core::HRESULT = windows_core::HRESULT(0x80004003_u32 as _);
pub type GETPROPERTYSTOREFLAGS = u32;
windows_core::imp::define_interface!(
    IBindCtx,
    IBindCtx_Vtbl,
    0x0000000e_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IBindCtx, windows_core::IUnknown);
impl IBindCtx {
    pub(crate) unsafe fn RegisterObjectBound<P0>(&self, punk: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::IUnknown>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).RegisterObjectBound)(
                windows_core::Interface::as_raw(self),
                punk.param().abi(),
            )
        }
    }
    pub(crate) unsafe fn RevokeObjectBound<P0>(&self, punk: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::IUnknown>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).RevokeObjectBound)(
                windows_core::Interface::as_raw(self),
                punk.param().abi(),
            )
        }
    }
    pub(crate) unsafe fn ReleaseBoundObjects(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).ReleaseBoundObjects)(
                windows_core::Interface::as_raw(self),
            )
        }
    }
    pub(crate) unsafe fn SetBindOptions(
        &self,
        pbindopts: *const BIND_OPTS,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetBindOptions)(
                windows_core::Interface::as_raw(self),
                pbindopts,
            )
        }
    }
    pub(crate) unsafe fn GetBindOptions(&self, pbindopts: *mut BIND_OPTS) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetBindOptions)(
                windows_core::Interface::as_raw(self),
                pbindopts as _,
            )
        }
    }
    pub(crate) unsafe fn GetRunningObjectTable(&self) -> windows_core::Result<IRunningObjectTable> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetRunningObjectTable)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub(crate) unsafe fn RegisterObjectParam<P0, P1>(
        &self,
        pszkey: P0,
        punk: P1,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
        P1: windows_core::Param<windows_core::IUnknown>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).RegisterObjectParam)(
                windows_core::Interface::as_raw(self),
                pszkey.param().abi(),
                punk.param().abi(),
            )
        }
    }
    pub(crate) unsafe fn GetObjectParam<P0>(
        &self,
        pszkey: P0,
    ) -> windows_core::Result<windows_core::IUnknown>
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetObjectParam)(
                windows_core::Interface::as_raw(self),
                pszkey.param().abi(),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub(crate) unsafe fn EnumObjectParam(&self) -> windows_core::Result<IEnumString> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).EnumObjectParam)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub(crate) unsafe fn RevokeObjectParam<P0>(&self, pszkey: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).RevokeObjectParam)(
                windows_core::Interface::as_raw(self),
                pszkey.param().abi(),
            )
        }
    }
}
#[repr(C)]
pub struct IBindCtx_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub RegisterObjectBound: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RevokeObjectBound: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ReleaseBoundObjects:
        unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub SetBindOptions: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const BIND_OPTS,
    ) -> windows_core::HRESULT,
    pub GetBindOptions:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut BIND_OPTS) -> windows_core::HRESULT,
    pub GetRunningObjectTable: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RegisterObjectParam: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetObjectParam: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub EnumObjectParam: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub RevokeObjectParam: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
}
pub trait IBindCtx_Impl: windows_core::IUnknownImpl {
    fn RegisterObjectBound(
        &self,
        punk: windows_core::Ref<windows_core::IUnknown>,
    ) -> windows_core::Result<()>;
    fn RevokeObjectBound(
        &self,
        punk: windows_core::Ref<windows_core::IUnknown>,
    ) -> windows_core::Result<()>;
    fn ReleaseBoundObjects(&self) -> windows_core::Result<()>;
    fn SetBindOptions(&self, pbindopts: *const BIND_OPTS) -> windows_core::Result<()>;
    fn GetBindOptions(&self, pbindopts: *mut BIND_OPTS) -> windows_core::Result<()>;
    fn GetRunningObjectTable(&self) -> windows_core::Result<IRunningObjectTable>;
    fn RegisterObjectParam(
        &self,
        pszkey: &windows_core::PCWSTR,
        punk: windows_core::Ref<windows_core::IUnknown>,
    ) -> windows_core::Result<()>;
    fn GetObjectParam(
        &self,
        pszkey: &windows_core::PCWSTR,
    ) -> windows_core::Result<windows_core::IUnknown>;
    fn EnumObjectParam(&self) -> windows_core::Result<IEnumString>;
    fn RevokeObjectParam(&self, pszkey: &windows_core::PCWSTR) -> windows_core::Result<()>;
}
impl IBindCtx_Vtbl {
    pub const fn new<Identity: IBindCtx_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn RegisterObjectBound<
            Identity: IBindCtx_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            punk: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IBindCtx_Impl::RegisterObjectBound(this, core::mem::transmute_copy(&punk)).into()
            }
        }
        unsafe extern "system" fn RevokeObjectBound<
            Identity: IBindCtx_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            punk: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IBindCtx_Impl::RevokeObjectBound(this, core::mem::transmute_copy(&punk)).into()
            }
        }
        unsafe extern "system" fn ReleaseBoundObjects<
            Identity: IBindCtx_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IBindCtx_Impl::ReleaseBoundObjects(this).into()
            }
        }
        unsafe extern "system" fn SetBindOptions<Identity: IBindCtx_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pbindopts: *const BIND_OPTS,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IBindCtx_Impl::SetBindOptions(this, core::mem::transmute_copy(&pbindopts)).into()
            }
        }
        unsafe extern "system" fn GetBindOptions<Identity: IBindCtx_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pbindopts: *mut BIND_OPTS,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IBindCtx_Impl::GetBindOptions(this, core::mem::transmute_copy(&pbindopts)).into()
            }
        }
        unsafe extern "system" fn GetRunningObjectTable<
            Identity: IBindCtx_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pprot: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IBindCtx_Impl::GetRunningObjectTable(this) {
                    Ok(ok__) => {
                        pprot.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn RegisterObjectParam<
            Identity: IBindCtx_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pszkey: windows_core::PCWSTR,
            punk: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IBindCtx_Impl::RegisterObjectParam(
                    this,
                    core::mem::transmute(&pszkey),
                    core::mem::transmute_copy(&punk),
                )
                .into()
            }
        }
        unsafe extern "system" fn GetObjectParam<Identity: IBindCtx_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pszkey: windows_core::PCWSTR,
            ppunk: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IBindCtx_Impl::GetObjectParam(this, core::mem::transmute(&pszkey)) {
                    Ok(ok__) => {
                        ppunk.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn EnumObjectParam<Identity: IBindCtx_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            ppenum: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IBindCtx_Impl::EnumObjectParam(this) {
                    Ok(ok__) => {
                        ppenum.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn RevokeObjectParam<
            Identity: IBindCtx_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pszkey: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IBindCtx_Impl::RevokeObjectParam(this, core::mem::transmute(&pszkey)).into()
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            RegisterObjectBound: RegisterObjectBound::<Identity, OFFSET>,
            RevokeObjectBound: RevokeObjectBound::<Identity, OFFSET>,
            ReleaseBoundObjects: ReleaseBoundObjects::<Identity, OFFSET>,
            SetBindOptions: SetBindOptions::<Identity, OFFSET>,
            GetBindOptions: GetBindOptions::<Identity, OFFSET>,
            GetRunningObjectTable: GetRunningObjectTable::<Identity, OFFSET>,
            RegisterObjectParam: RegisterObjectParam::<Identity, OFFSET>,
            GetObjectParam: GetObjectParam::<Identity, OFFSET>,
            EnumObjectParam: EnumObjectParam::<Identity, OFFSET>,
            RevokeObjectParam: RevokeObjectParam::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IBindCtx as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IBindCtx {}
windows_core::imp::define_interface!(
    IClassFactory,
    IClassFactory_Vtbl,
    0x00000001_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IClassFactory, windows_core::IUnknown);
impl IClassFactory {
    pub(crate) unsafe fn CreateInstance<P0, T>(&self, punkouter: P0) -> windows_core::Result<T>
    where
        P0: windows_core::Param<windows_core::IUnknown>,
        T: windows_core::Interface,
    {
        let mut result__ = core::ptr::null_mut();
        unsafe {
            (windows_core::Interface::vtable(self).CreateInstance)(
                windows_core::Interface::as_raw(self),
                punkouter.param().abi(),
                &T::IID,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub(crate) unsafe fn LockServer(&self, flock: bool) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).LockServer)(
                windows_core::Interface::as_raw(self),
                flock.into(),
            )
        }
    }
}
#[repr(C)]
pub struct IClassFactory_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub CreateInstance: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *const windows_core::GUID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub LockServer: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::BOOL,
    ) -> windows_core::HRESULT,
}
pub trait IClassFactory_Impl: windows_core::IUnknownImpl {
    fn CreateInstance(
        &self,
        punkouter: windows_core::Ref<windows_core::IUnknown>,
        riid: *const windows_core::GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> windows_core::Result<()>;
    fn LockServer(&self, flock: windows_core::BOOL) -> windows_core::Result<()>;
}
impl IClassFactory_Vtbl {
    pub const fn new<Identity: IClassFactory_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn CreateInstance<
            Identity: IClassFactory_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            punkouter: *mut core::ffi::c_void,
            riid: *const windows_core::GUID,
            ppvobject: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IClassFactory_Impl::CreateInstance(
                    this,
                    core::mem::transmute_copy(&punkouter),
                    core::mem::transmute_copy(&riid),
                    core::mem::transmute_copy(&ppvobject),
                )
                .into()
            }
        }
        unsafe extern "system" fn LockServer<Identity: IClassFactory_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            flock: windows_core::BOOL,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IClassFactory_Impl::LockServer(this, core::mem::transmute_copy(&flock)).into()
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            CreateInstance: CreateInstance::<Identity, OFFSET>,
            LockServer: LockServer::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IClassFactory as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IClassFactory {}
windows_core::imp::define_interface!(
    IEnumExplorerCommand,
    IEnumExplorerCommand_Vtbl,
    0xa88826f8_186f_4987_aade_ea0cef8fbfe8
);
windows_core::imp::interface_hierarchy!(IEnumExplorerCommand, windows_core::IUnknown);
impl IEnumExplorerCommand {
    pub(crate) unsafe fn Next(
        &self,
        celt: u32,
        puicommand: *mut Option<IExplorerCommand>,
        pceltfetched: Option<*mut u32>,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Next)(
                windows_core::Interface::as_raw(self),
                celt,
                core::mem::transmute(puicommand),
                pceltfetched.unwrap_or(core::mem::zeroed()) as _,
            )
        }
    }
    pub(crate) unsafe fn Skip(&self, celt: u32) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Skip)(
                windows_core::Interface::as_raw(self),
                celt,
            )
        }
    }
    pub(crate) unsafe fn Reset(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Reset)(windows_core::Interface::as_raw(self))
        }
    }
    pub(crate) unsafe fn Clone(&self) -> windows_core::Result<Self> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).Clone)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
#[repr(C)]
pub struct IEnumExplorerCommand_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub Next: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        *mut *mut core::ffi::c_void,
        *mut u32,
    ) -> windows_core::HRESULT,
    pub Skip: unsafe extern "system" fn(*mut core::ffi::c_void, u32) -> windows_core::HRESULT,
    pub Reset: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub Clone: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
pub trait IEnumExplorerCommand_Impl: windows_core::IUnknownImpl {
    fn Next(
        &self,
        celt: u32,
        puicommand: *mut Option<IExplorerCommand>,
        pceltfetched: *mut u32,
    ) -> windows_core::Result<()>;
    fn Skip(&self, celt: u32) -> windows_core::Result<()>;
    fn Reset(&self) -> windows_core::Result<()>;
    fn Clone(&self) -> windows_core::Result<IEnumExplorerCommand>;
}
impl IEnumExplorerCommand_Vtbl {
    pub const fn new<Identity: IEnumExplorerCommand_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn Next<Identity: IEnumExplorerCommand_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            celt: u32,
            puicommand: *mut *mut core::ffi::c_void,
            pceltfetched: *mut u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IEnumExplorerCommand_Impl::Next(
                    this,
                    core::mem::transmute_copy(&celt),
                    core::mem::transmute_copy(&puicommand),
                    core::mem::transmute_copy(&pceltfetched),
                )
                .into()
            }
        }
        unsafe extern "system" fn Skip<Identity: IEnumExplorerCommand_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            celt: u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IEnumExplorerCommand_Impl::Skip(this, core::mem::transmute_copy(&celt)).into()
            }
        }
        unsafe extern "system" fn Reset<
            Identity: IEnumExplorerCommand_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IEnumExplorerCommand_Impl::Reset(this).into()
            }
        }
        unsafe extern "system" fn Clone<
            Identity: IEnumExplorerCommand_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            ppenum: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IEnumExplorerCommand_Impl::Clone(this) {
                    Ok(ok__) => {
                        ppenum.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            Next: Next::<Identity, OFFSET>,
            Skip: Skip::<Identity, OFFSET>,
            Reset: Reset::<Identity, OFFSET>,
            Clone: Clone::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IEnumExplorerCommand as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IEnumExplorerCommand {}
windows_core::imp::define_interface!(
    IEnumShellItems,
    IEnumShellItems_Vtbl,
    0x70629033_e363_4a28_a567_0db78006e6d7
);
windows_core::imp::interface_hierarchy!(IEnumShellItems, windows_core::IUnknown);
#[repr(C)]
pub struct IEnumShellItems_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    Next: usize,
    Skip: usize,
    Reset: usize,
    Clone: usize,
}
impl windows_core::RuntimeName for IEnumShellItems {}
windows_core::imp::define_interface!(
    IEnumString,
    IEnumString_Vtbl,
    0x00000101_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IEnumString, windows_core::IUnknown);
#[repr(C)]
pub struct IEnumString_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    Next: usize,
    Skip: usize,
    Reset: usize,
    Clone: usize,
}
impl windows_core::RuntimeName for IEnumString {}
windows_core::imp::define_interface!(
    IExplorerCommand,
    IExplorerCommand_Vtbl,
    0xa08ce4d0_fa25_44ab_b57c_c7b1c323e0b9
);
windows_core::imp::interface_hierarchy!(IExplorerCommand, windows_core::IUnknown);
impl IExplorerCommand {
    pub(crate) unsafe fn GetTitle<P0>(
        &self,
        psiitemarray: P0,
    ) -> windows_core::Result<windows_core::PWSTR>
    where
        P0: windows_core::Param<IShellItemArray>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetTitle)(
                windows_core::Interface::as_raw(self),
                psiitemarray.param().abi(),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub(crate) unsafe fn GetIcon<P0>(
        &self,
        psiitemarray: P0,
    ) -> windows_core::Result<windows_core::PWSTR>
    where
        P0: windows_core::Param<IShellItemArray>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetIcon)(
                windows_core::Interface::as_raw(self),
                psiitemarray.param().abi(),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub(crate) unsafe fn GetToolTip<P0>(
        &self,
        psiitemarray: P0,
    ) -> windows_core::Result<windows_core::PWSTR>
    where
        P0: windows_core::Param<IShellItemArray>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetToolTip)(
                windows_core::Interface::as_raw(self),
                psiitemarray.param().abi(),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub(crate) unsafe fn GetCanonicalName(&self) -> windows_core::Result<windows_core::GUID> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCanonicalName)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub(crate) unsafe fn GetState<P0>(
        &self,
        psiitemarray: P0,
        foktobeslow: bool,
    ) -> windows_core::Result<EXPCMDSTATE>
    where
        P0: windows_core::Param<IShellItemArray>,
    {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetState)(
                windows_core::Interface::as_raw(self),
                psiitemarray.param().abi(),
                foktobeslow.into(),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub(crate) unsafe fn Invoke<P0, P1>(&self, psiitemarray: P0, pbc: P1) -> windows_core::HRESULT
    where
        P0: windows_core::Param<IShellItemArray>,
        P1: windows_core::Param<IBindCtx>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).Invoke)(
                windows_core::Interface::as_raw(self),
                psiitemarray.param().abi(),
                pbc.param().abi(),
            )
        }
    }
    pub(crate) unsafe fn GetFlags(&self) -> windows_core::Result<EXPCMDFLAGS> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetFlags)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub(crate) unsafe fn EnumSubCommands(&self) -> windows_core::Result<IEnumExplorerCommand> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).EnumSubCommands)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
#[repr(C)]
pub struct IExplorerCommand_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub GetTitle: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut windows_core::PWSTR,
    ) -> windows_core::HRESULT,
    pub GetIcon: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut windows_core::PWSTR,
    ) -> windows_core::HRESULT,
    pub GetToolTip: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut windows_core::PWSTR,
    ) -> windows_core::HRESULT,
    pub GetCanonicalName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::GUID,
    ) -> windows_core::HRESULT,
    pub GetState: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        windows_core::BOOL,
        *mut EXPCMDSTATE,
    ) -> windows_core::HRESULT,
    pub Invoke: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetFlags: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut EXPCMDFLAGS,
    ) -> windows_core::HRESULT,
    pub EnumSubCommands: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
pub trait IExplorerCommand_Impl: windows_core::IUnknownImpl {
    fn GetTitle(
        &self,
        psiitemarray: windows_core::Ref<IShellItemArray>,
    ) -> windows_core::Result<windows_core::PWSTR>;
    fn GetIcon(
        &self,
        psiitemarray: windows_core::Ref<IShellItemArray>,
    ) -> windows_core::Result<windows_core::PWSTR>;
    fn GetToolTip(
        &self,
        psiitemarray: windows_core::Ref<IShellItemArray>,
    ) -> windows_core::Result<windows_core::PWSTR>;
    fn GetCanonicalName(&self) -> windows_core::Result<windows_core::GUID>;
    fn GetState(
        &self,
        psiitemarray: windows_core::Ref<IShellItemArray>,
        foktobeslow: windows_core::BOOL,
    ) -> windows_core::Result<EXPCMDSTATE>;
    fn Invoke(
        &self,
        psiitemarray: windows_core::Ref<IShellItemArray>,
        pbc: windows_core::Ref<IBindCtx>,
    ) -> windows_core::Result<()>;
    fn GetFlags(&self) -> windows_core::Result<EXPCMDFLAGS>;
    fn EnumSubCommands(&self) -> windows_core::Result<IEnumExplorerCommand>;
}
impl IExplorerCommand_Vtbl {
    pub const fn new<Identity: IExplorerCommand_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn GetTitle<Identity: IExplorerCommand_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            psiitemarray: *mut core::ffi::c_void,
            ppszname: *mut windows_core::PWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IExplorerCommand_Impl::GetTitle(
                    this,
                    core::mem::transmute_copy(&psiitemarray),
                ) {
                    Ok(ok__) => {
                        ppszname.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn GetIcon<Identity: IExplorerCommand_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            psiitemarray: *mut core::ffi::c_void,
            ppszicon: *mut windows_core::PWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IExplorerCommand_Impl::GetIcon(this, core::mem::transmute_copy(&psiitemarray))
                {
                    Ok(ok__) => {
                        ppszicon.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn GetToolTip<
            Identity: IExplorerCommand_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            psiitemarray: *mut core::ffi::c_void,
            ppszinfotip: *mut windows_core::PWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IExplorerCommand_Impl::GetToolTip(
                    this,
                    core::mem::transmute_copy(&psiitemarray),
                ) {
                    Ok(ok__) => {
                        ppszinfotip.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn GetCanonicalName<
            Identity: IExplorerCommand_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pguidcommandname: *mut windows_core::GUID,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IExplorerCommand_Impl::GetCanonicalName(this) {
                    Ok(ok__) => {
                        pguidcommandname.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn GetState<Identity: IExplorerCommand_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            psiitemarray: *mut core::ffi::c_void,
            foktobeslow: windows_core::BOOL,
            pcmdstate: *mut EXPCMDSTATE,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IExplorerCommand_Impl::GetState(
                    this,
                    core::mem::transmute_copy(&psiitemarray),
                    core::mem::transmute_copy(&foktobeslow),
                ) {
                    Ok(ok__) => {
                        pcmdstate.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn Invoke<Identity: IExplorerCommand_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            psiitemarray: *mut core::ffi::c_void,
            pbc: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IExplorerCommand_Impl::Invoke(
                    this,
                    core::mem::transmute_copy(&psiitemarray),
                    core::mem::transmute_copy(&pbc),
                )
                .into()
            }
        }
        unsafe extern "system" fn GetFlags<Identity: IExplorerCommand_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pflags: *mut EXPCMDFLAGS,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IExplorerCommand_Impl::GetFlags(this) {
                    Ok(ok__) => {
                        pflags.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn EnumSubCommands<
            Identity: IExplorerCommand_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            ppenum: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IExplorerCommand_Impl::EnumSubCommands(this) {
                    Ok(ok__) => {
                        ppenum.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            GetTitle: GetTitle::<Identity, OFFSET>,
            GetIcon: GetIcon::<Identity, OFFSET>,
            GetToolTip: GetToolTip::<Identity, OFFSET>,
            GetCanonicalName: GetCanonicalName::<Identity, OFFSET>,
            GetState: GetState::<Identity, OFFSET>,
            Invoke: Invoke::<Identity, OFFSET>,
            GetFlags: GetFlags::<Identity, OFFSET>,
            EnumSubCommands: EnumSubCommands::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IExplorerCommand as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IExplorerCommand {}
windows_core::imp::define_interface!(
    IRunningObjectTable,
    IRunningObjectTable_Vtbl,
    0x00000010_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IRunningObjectTable, windows_core::IUnknown);
#[repr(C)]
pub struct IRunningObjectTable_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    Register: usize,
    Revoke: usize,
    IsRunning: usize,
    GetObject: usize,
    NoteChangeTime: usize,
    GetTimeOfLastChange: usize,
    EnumRunning: usize,
}
impl windows_core::RuntimeName for IRunningObjectTable {}
windows_core::imp::define_interface!(
    IShellItem,
    IShellItem_Vtbl,
    0x43826d1e_e718_42ee_bc55_a1e261c37bfe
);
windows_core::imp::interface_hierarchy!(IShellItem, windows_core::IUnknown);
#[repr(C)]
pub struct IShellItem_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    BindToHandler: usize,
    GetParent: usize,
    GetDisplayName: usize,
    GetAttributes: usize,
    Compare: usize,
}
impl windows_core::RuntimeName for IShellItem {}
windows_core::imp::define_interface!(
    IShellItemArray,
    IShellItemArray_Vtbl,
    0xb63ea76d_1f85_456f_a19c_48159efa858b
);
windows_core::imp::interface_hierarchy!(IShellItemArray, windows_core::IUnknown);
impl IShellItemArray {
    pub(crate) unsafe fn BindToHandler<P0, T>(
        &self,
        pbc: P0,
        bhid: *const windows_core::GUID,
    ) -> windows_core::Result<T>
    where
        P0: windows_core::Param<IBindCtx>,
        T: windows_core::Interface,
    {
        let mut result__ = core::ptr::null_mut();
        unsafe {
            (windows_core::Interface::vtable(self).BindToHandler)(
                windows_core::Interface::as_raw(self),
                pbc.param().abi(),
                bhid,
                &T::IID,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub(crate) unsafe fn GetPropertyStore<T>(
        &self,
        flags: GETPROPERTYSTOREFLAGS,
    ) -> windows_core::Result<T>
    where
        T: windows_core::Interface,
    {
        let mut result__ = core::ptr::null_mut();
        unsafe {
            (windows_core::Interface::vtable(self).GetPropertyStore)(
                windows_core::Interface::as_raw(self),
                flags,
                &T::IID,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub(crate) unsafe fn GetPropertyDescriptionList<T>(
        &self,
        keytype: *const PROPERTYKEY,
    ) -> windows_core::Result<T>
    where
        T: windows_core::Interface,
    {
        let mut result__ = core::ptr::null_mut();
        unsafe {
            (windows_core::Interface::vtable(self).GetPropertyDescriptionList)(
                windows_core::Interface::as_raw(self),
                keytype,
                &T::IID,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub(crate) unsafe fn GetAttributes(
        &self,
        attribflags: SIATTRIBFLAGS,
        sfgaomask: SFGAOF,
    ) -> windows_core::Result<SFGAOF> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetAttributes)(
                windows_core::Interface::as_raw(self),
                attribflags,
                sfgaomask,
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub(crate) unsafe fn GetCount(&self) -> windows_core::Result<u32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCount)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub(crate) unsafe fn GetItemAt(&self, dwindex: u32) -> windows_core::Result<IShellItem> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetItemAt)(
                windows_core::Interface::as_raw(self),
                dwindex,
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
    pub(crate) unsafe fn EnumItems(&self) -> windows_core::Result<IEnumShellItems> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).EnumItems)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .and_then(|| windows_core::imp::Type::from_abi(result__))
        }
    }
}
#[repr(C)]
pub struct IShellItemArray_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub BindToHandler: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *const windows_core::GUID,
        *const windows_core::GUID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetPropertyStore: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        GETPROPERTYSTOREFLAGS,
        *const windows_core::GUID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetPropertyDescriptionList: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const PROPERTYKEY,
        *const windows_core::GUID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub GetAttributes: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        SIATTRIBFLAGS,
        SFGAOF,
        *mut SFGAOF,
    ) -> windows_core::HRESULT,
    pub GetCount:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows_core::HRESULT,
    pub GetItemAt: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub EnumItems: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
pub trait IShellItemArray_Impl: windows_core::IUnknownImpl {
    fn BindToHandler(
        &self,
        pbc: windows_core::Ref<IBindCtx>,
        bhid: *const windows_core::GUID,
        riid: *const windows_core::GUID,
        ppvout: *mut *mut core::ffi::c_void,
    ) -> windows_core::Result<()>;
    fn GetPropertyStore(
        &self,
        flags: GETPROPERTYSTOREFLAGS,
        riid: *const windows_core::GUID,
        ppv: *mut *mut core::ffi::c_void,
    ) -> windows_core::Result<()>;
    fn GetPropertyDescriptionList(
        &self,
        keytype: *const PROPERTYKEY,
        riid: *const windows_core::GUID,
        ppv: *mut *mut core::ffi::c_void,
    ) -> windows_core::Result<()>;
    fn GetAttributes(
        &self,
        attribflags: SIATTRIBFLAGS,
        sfgaomask: SFGAOF,
    ) -> windows_core::Result<SFGAOF>;
    fn GetCount(&self) -> windows_core::Result<u32>;
    fn GetItemAt(&self, dwindex: u32) -> windows_core::Result<IShellItem>;
    fn EnumItems(&self) -> windows_core::Result<IEnumShellItems>;
}
impl IShellItemArray_Vtbl {
    pub const fn new<Identity: IShellItemArray_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn BindToHandler<
            Identity: IShellItemArray_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pbc: *mut core::ffi::c_void,
            bhid: *const windows_core::GUID,
            riid: *const windows_core::GUID,
            ppvout: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellItemArray_Impl::BindToHandler(
                    this,
                    core::mem::transmute_copy(&pbc),
                    core::mem::transmute_copy(&bhid),
                    core::mem::transmute_copy(&riid),
                    core::mem::transmute_copy(&ppvout),
                )
                .into()
            }
        }
        unsafe extern "system" fn GetPropertyStore<
            Identity: IShellItemArray_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            flags: GETPROPERTYSTOREFLAGS,
            riid: *const windows_core::GUID,
            ppv: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellItemArray_Impl::GetPropertyStore(
                    this,
                    core::mem::transmute_copy(&flags),
                    core::mem::transmute_copy(&riid),
                    core::mem::transmute_copy(&ppv),
                )
                .into()
            }
        }
        unsafe extern "system" fn GetPropertyDescriptionList<
            Identity: IShellItemArray_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            keytype: *const PROPERTYKEY,
            riid: *const windows_core::GUID,
            ppv: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellItemArray_Impl::GetPropertyDescriptionList(
                    this,
                    core::mem::transmute_copy(&keytype),
                    core::mem::transmute_copy(&riid),
                    core::mem::transmute_copy(&ppv),
                )
                .into()
            }
        }
        unsafe extern "system" fn GetAttributes<
            Identity: IShellItemArray_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            attribflags: SIATTRIBFLAGS,
            sfgaomask: SFGAOF,
            psfgaoattribs: *mut SFGAOF,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IShellItemArray_Impl::GetAttributes(
                    this,
                    core::mem::transmute_copy(&attribflags),
                    core::mem::transmute_copy(&sfgaomask),
                ) {
                    Ok(ok__) => {
                        psfgaoattribs.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn GetCount<Identity: IShellItemArray_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pdwnumitems: *mut u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IShellItemArray_Impl::GetCount(this) {
                    Ok(ok__) => {
                        pdwnumitems.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn GetItemAt<Identity: IShellItemArray_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            dwindex: u32,
            ppsi: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IShellItemArray_Impl::GetItemAt(this, core::mem::transmute_copy(&dwindex)) {
                    Ok(ok__) => {
                        ppsi.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn EnumItems<Identity: IShellItemArray_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            ppenumshellitems: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IShellItemArray_Impl::EnumItems(this) {
                    Ok(ok__) => {
                        ppenumshellitems.write(core::mem::transmute(ok__));
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            BindToHandler: BindToHandler::<Identity, OFFSET>,
            GetPropertyStore: GetPropertyStore::<Identity, OFFSET>,
            GetPropertyDescriptionList: GetPropertyDescriptionList::<Identity, OFFSET>,
            GetAttributes: GetAttributes::<Identity, OFFSET>,
            GetCount: GetCount::<Identity, OFFSET>,
            GetItemAt: GetItemAt::<Identity, OFFSET>,
            EnumItems: EnumItems::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IShellItemArray as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IShellItemArray {}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PROPERTYKEY {
    pub fmtid: windows_core::GUID,
    pub pid: u32,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SFGAOF(pub u32);
pub type SIATTRIBFLAGS = u32;
pub const S_FALSE: windows_core::HRESULT = windows_core::HRESULT(0x1_u32 as _);

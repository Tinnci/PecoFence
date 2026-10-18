#[inline]
pub unsafe fn SafeArrayAccessData(
    psa: *const SAFEARRAY,
    ppvdata: *mut *mut core::ffi::c_void,
) -> windows_core::HRESULT {
    windows_core::link!("oleaut32.dll" "system" fn SafeArrayAccessData(psa : *const SAFEARRAY, ppvdata : *mut *mut core::ffi::c_void) -> windows_core::HRESULT);
    unsafe { SafeArrayAccessData(psa, ppvdata as _) }
}
#[inline]
pub unsafe fn SafeArrayCreateVector(vt: VARTYPE, llbound: i32, celements: u32) -> *mut SAFEARRAY {
    windows_core::link!("oleaut32.dll" "system" fn SafeArrayCreateVector(vt : VARTYPE, llbound : i32, celements : u32) -> *mut SAFEARRAY);
    unsafe { SafeArrayCreateVector(vt, llbound, celements) }
}
#[inline]
pub unsafe fn SafeArrayDestroy(psa: *const SAFEARRAY) -> windows_core::HRESULT {
    windows_core::link!("oleaut32.dll" "system" fn SafeArrayDestroy(psa : *const SAFEARRAY) -> windows_core::HRESULT);
    unsafe { SafeArrayDestroy(psa) }
}
#[inline]
pub unsafe fn SafeArrayUnaccessData(psa: *const SAFEARRAY) -> windows_core::HRESULT {
    windows_core::link!("oleaut32.dll" "system" fn SafeArrayUnaccessData(psa : *const SAFEARRAY) -> windows_core::HRESULT);
    unsafe { SafeArrayUnaccessData(psa) }
}
#[inline]
pub unsafe fn VariantClear(pvarg: *mut VARIANTARG) -> windows_core::HRESULT {
    windows_core::link!("oleaut32.dll" "system" fn VariantClear(pvarg : *mut VARIANTARG) -> windows_core::HRESULT);
    unsafe { VariantClear(pvarg) }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union CY {
    pub Anonymous: CY_0,
    pub int64: i64,
}
impl Default for CY {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CY_0 {
    pub Lo: u32,
    pub Hi: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DECIMAL {
    pub wReserved: u16,
    pub Anonymous: DECIMAL_0,
    pub Hi32: u32,
    pub Anonymous2: DECIMAL_1,
}
impl Default for DECIMAL {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union DECIMAL_0 {
    pub Anonymous: DECIMAL_0_0,
    pub signscale: u16,
}
impl Default for DECIMAL_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DECIMAL_0_0 {
    pub scale: u8,
    pub sign: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union DECIMAL_1 {
    pub Anonymous: DECIMAL_1_0,
    pub Lo64: u64,
}
impl Default for DECIMAL_1 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DECIMAL_1_0 {
    pub Lo32: u32,
    pub Mid32: u32,
}
windows_core::imp::define_interface!(
    IDispatch,
    IDispatch_Vtbl,
    0x00020400_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IDispatch, windows_core::IUnknown);
#[repr(C)]
pub struct IDispatch_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    GetTypeInfoCount: usize,
    GetTypeInfo: usize,
    GetIDsOfNames: usize,
    Invoke: usize,
}
impl windows_core::RuntimeName for IDispatch {}
windows_core::imp::define_interface!(
    IRecordInfo,
    IRecordInfo_Vtbl,
    0x0000002f_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IRecordInfo, windows_core::IUnknown);
#[repr(C)]
pub struct IRecordInfo_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    RecordInit: usize,
    RecordClear: usize,
    RecordCopy: usize,
    GetGuid: usize,
    GetName: usize,
    GetSize: usize,
    GetTypeInfo: usize,
    GetField: usize,
    GetFieldNoCopy: usize,
    PutField: usize,
    PutFieldNoCopy: usize,
    GetFieldNames: usize,
    IsMatchingType: usize,
    RecordCreate: usize,
    RecordCreateCopy: usize,
    RecordDestroy: usize,
}
impl windows_core::RuntimeName for IRecordInfo {}
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SAFEARRAY {
    pub cDims: u16,
    pub fFeatures: u16,
    pub cbElements: u32,
    pub cLocks: u32,
    pub pvData: *mut core::ffi::c_void,
    pub rgsabound: [SAFEARRAYBOUND; 1],
}
impl Default for SAFEARRAY {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SAFEARRAYBOUND {
    pub cElements: u32,
    pub lLbound: i32,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SCODE(pub i32);
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiaRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}
#[repr(C)]
pub struct VARIANT {
    pub Anonymous: VARIANT_0,
}
impl Clone for VARIANT {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub union VARIANT_0 {
    pub Anonymous: core::mem::ManuallyDrop<VARIANT_0_0>,
    pub decVal: DECIMAL,
}
impl Clone for VARIANT_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub struct VARIANT_0_0 {
    pub vt: VARTYPE,
    pub wReserved1: u16,
    pub wReserved2: u16,
    pub wReserved3: u16,
    pub Anonymous: VARIANT_0_0_0,
}
impl Clone for VARIANT_0_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT_0_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
pub union VARIANT_0_0_0 {
    pub llVal: i64,
    pub lVal: i32,
    pub bVal: u8,
    pub iVal: i16,
    pub fltVal: f32,
    pub dblVal: f64,
    pub boolVal: VARIANT_BOOL,
    pub __OBSOLETE__VARIANT_BOOL: VARIANT_BOOL,
    pub scode: SCODE,
    pub cyVal: CY,
    pub date: f64,
    pub bstrVal: core::mem::ManuallyDrop<windows_core::BSTR>,
    pub punkVal: core::mem::ManuallyDrop<Option<windows_core::IUnknown>>,
    pub pdispVal: core::mem::ManuallyDrop<Option<IDispatch>>,
    pub parray: *mut SAFEARRAY,
    pub pbVal: *mut u8,
    pub piVal: *mut i16,
    pub plVal: *mut i32,
    pub pllVal: *mut i64,
    pub pfltVal: *mut f32,
    pub pdblVal: *mut f64,
    pub pboolVal: *mut VARIANT_BOOL,
    pub __OBSOLETE__VARIANT_PBOOL: *mut VARIANT_BOOL,
    pub pscode: *mut SCODE,
    pub pcyVal: *mut CY,
    pub pdate: *mut f64,
    pub pbstrVal: *mut windows_core::BSTR,
    pub ppunkVal: *mut Option<windows_core::IUnknown>,
    pub ppdispVal: *mut Option<IDispatch>,
    pub pparray: *mut *mut SAFEARRAY,
    pub pvarVal: *mut VARIANT,
    pub byref: *mut core::ffi::c_void,
    pub cVal: i8,
    pub uiVal: u16,
    pub ulVal: u32,
    pub ullVal: u64,
    pub intVal: i32,
    pub uintVal: u32,
    pub pdecVal: *mut DECIMAL,
    pub pcVal: *mut i8,
    pub puiVal: *mut u16,
    pub pulVal: *mut u32,
    pub pullVal: *mut u64,
    pub pintVal: *mut i32,
    pub puintVal: *mut u32,
    pub Anonymous: core::mem::ManuallyDrop<VARIANT_0_0_0_0>,
}
impl Clone for VARIANT_0_0_0 {
    fn clone(&self) -> Self {
        unsafe { core::mem::transmute_copy(self) }
    }
}
impl Default for VARIANT_0_0_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VARIANT_0_0_0_0 {
    pub pvRecord: *mut core::ffi::c_void,
    pub pRecInfo: core::mem::ManuallyDrop<Option<IRecordInfo>>,
}
pub type VARIANTARG = VARIANT;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VARIANT_BOOL(pub i16);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VARTYPE(pub u16);

//! The material pixel shaders as Direct2D custom effects.
//!
//! - [liquid_glass.hlsl](../shaders/liquid_glass.hlsl) refracts the wallpaper through a
//!   rounded squircle surface for every pixel (two inputs: sharp and frosted wallpaper).
//! - [paper.hlsl](../shaders/paper.hlsl) draws a procedural sheet of paper (no inputs, so
//!   Direct2D can cache it; the wallpaper showing through is composited afterwards).
//!
//! This module registers them on the device's factory and implements the two COM
//! interfaces Direct2D asks of an effect: an `ID2D1EffectImpl` and a single
//! `ID2D1DrawTransform` node, both on one object, parameterised by a [`Kind`]. The
//! effect-author API is missing from the bundled Windows metadata, so its vtables are
//! declared here by hand, in the method order of d2d1effectauthor.h.
//!
//! Registration or creation can fail (a device without ps_4_0, a refused registration):
//! `gpu_glass` then keeps its built-in effects.

use crate::gpu_bindings as b;
use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use windows_core::{GUID, HRESULT, Interface, Result};

const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
const IID_EFFECT_IMPL: GUID = GUID::from_u128(0xa248fd3f_3e6c_4e63_9f03_7f68ecc91db9);
const IID_TRANSFORM_NODE: GUID = GUID::from_u128(0xb2efe1e7_729f_4102_949f_505fa21bf666);
const IID_TRANSFORM: GUID = GUID::from_u128(0xef1a287d_342a_4f76_8fdb_da0d6ea9f92b);
const IID_DRAW_TRANSFORM: GUID = GUID::from_u128(0x36bfdcb6_9739_435d_a30d_a653beff6a6f);
const IID_FACTORY1: GUID = GUID::from_u128(0xbb12d362_daee_4b9a_aa1d_14ba401cfa1f);

const S_OK: HRESULT = HRESULT(0);
const E_NOINTERFACE: HRESULT = HRESULT(0x80004002u32 as i32);
const E_NOTIMPL: HRESULT = HRESULT(0x80004001u32 as i32);
const E_INVALIDARG: HRESULT = HRESULT(0x80070057u32 as i32);
const D2D1_PROPERTY_TYPE_BLOB: i32 = 9;

/// Both shaders take a 64-byte constant buffer (16 floats).
const CONSTANTS_LEN: usize = 64;
type Bytes = [u8; CONSTANTS_LEN];

/// One registered effect class.
pub(crate) struct Kind {
    clsid: GUID,
    shader_id: GUID,
    shader: &'static [u8],
    inputs: u32,
    xml: &'static str,
    /// Output bounds and input padding (px) for the current constants.
    extent: fn(&Bytes) -> (RectL, i32),
    create: unsafe extern "system" fn(*mut *mut c_void) -> HRESULT,
}

pub(crate) static GLASS: Kind = Kind {
    clsid: GUID::from_u128(0xc6d795ca_ee48_41a6_a51a_b3e8ebccae13),
    shader_id: GUID::from_u128(0x505395d1_bcda_45fc_9f33_d32b8667ef26),
    shader: include_bytes!("../shaders/liquid_glass.cso"),
    inputs: 2,
    xml: "<?xml version='1.0'?>\
<Effect>\
<Property name='DisplayName' type='string' value='PecoFence Liquid Glass'/>\
<Property name='Author' type='string' value='PecoFence'/>\
<Property name='Category' type='string' value='Stylize'/>\
<Property name='Description' type='string' value='Refracts the input through a rounded glass plate.'/>\
<Inputs><Input name='Sharp'/><Input name='Frost'/></Inputs>\
<Property name='Constants' type='blob'>\
<Property name='DisplayName' type='string' value='Constants'/>\
</Property>\
</Effect>",
    extent: |bytes| {
        let c: GlassConstants = read(bytes);
        (bounds(c.size), c.max_offset.ceil() as i32 + 2)
    },
    create: create_glass,
};

pub(crate) static PAPER: Kind = Kind {
    clsid: GUID::from_u128(0xc3b7b49b_90ad_4956_9c46_96b3af26e5e6),
    shader_id: GUID::from_u128(0xb2be0a2c_ee79_42c1_b784_78269512ab1f),
    shader: include_bytes!("../shaders/paper.cso"),
    inputs: 0,
    xml: "<?xml version='1.0'?>\
<Effect>\
<Property name='DisplayName' type='string' value='PecoFence Paper'/>\
<Property name='Author' type='string' value='PecoFence'/>\
<Property name='Category' type='string' value='Stylize'/>\
<Property name='Description' type='string' value='A procedural sheet of matte paper.'/>\
<Inputs/>\
<Property name='Constants' type='blob'>\
<Property name='DisplayName' type='string' value='Constants'/>\
</Property>\
</Effect>",
    extent: |bytes| (bounds(read::<PaperConstants>(bytes).size), 0),
    create: create_paper,
};

/// Every 16-float constant struct below is plain data with no padding.
pub(crate) trait Constants: Copy {
    fn bytes(&self) -> Bytes {
        let mut out = [0u8; CONSTANTS_LEN];
        // SAFETY: implementors are repr(C), 64 bytes of f32 (asserted per type).
        out.copy_from_slice(unsafe {
            std::slice::from_raw_parts((self as *const Self).cast::<u8>(), CONSTANTS_LEN)
        });
        out
    }
}

fn read<T: Constants>(bytes: &Bytes) -> T {
    // SAFETY: T is a 64-byte repr(C) struct of f32s; every bit pattern is valid.
    unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<T>()) }
}

fn bounds(size: [f32; 2]) -> RectL {
    RectL {
        left: 0,
        top: 0,
        right: size[0].ceil() as i32,
        bottom: size[1].ceil() as i32,
    }
}

/// The light both materials share: from the upper left, as a unit screen-space vector.
fn light() -> [f32; 2] {
    let (x, y) = (-0.55f32, -0.835f32);
    let norm = (x * x + y * y).sqrt();
    [x / norm, y / norm]
}

/// `liquid_glass.hlsl`'s constant buffer, field for field. Lengths in device px.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlassConstants {
    pub size: [f32; 2],
    pub radius: f32,
    pub bezel: f32,
    pub thickness: f32,
    pub ior: f32,
    pub dispersion: f32,
    pub frost: f32,
    pub light: [f32; 2],
    pub specular: f32,
    pub fresnel: f32,
    pub max_offset: f32,
    pub saturation: f32,
    pub pad: [f32; 2],
}

const _: () = assert!(size_of::<GlassConstants>() == CONSTANTS_LEN);
impl Constants for GlassConstants {}

impl GlassConstants {
    /// The material for a `width` × `height` px plate with a `radius_dip` corner at `scale`.
    /// The rim is never wider than the corner, so its normals stay continuous around it.
    pub(crate) fn plate(width: u32, height: u32, scale: f32, radius_dip: f32) -> Self {
        let scale = scale.max(0.5);
        let (w, h) = (width.max(1) as f32, height.max(1) as f32);
        let radius = (radius_dip * scale).min(w.min(h) * 0.5).max(0.0);
        let bezel = (18.0 * scale).min(radius).min(w.min(h) * 0.5).max(1.0);
        Self {
            size: [w, h],
            radius,
            bezel,
            thickness: bezel * 0.75,
            ior: 1.5,
            dispersion: 0.02,
            frost: 0.5,
            light: light(),
            specular: 0.45,
            fresnel: 0.32,
            max_offset: bezel * 1.1,
            saturation: 1.12,
            pad: [0.0; 2],
        }
    }
}

/// `paper.hlsl`'s constant buffer, field for field.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaperConstants {
    pub size: [f32; 2],
    pub radius: f32,
    pub scale: f32,
    pub paper: [f32; 3],
    pub tone: f32,
    pub light: [f32; 2],
    pub fiber: f32,
    pub grain: f32,
    pub relief: f32,
    pub seed: f32,
    pub edge: f32,
    pub speckle: f32,
}

const _: () = assert!(size_of::<PaperConstants>() == CONSTANTS_LEN);
impl Constants for PaperConstants {}

impl PaperConstants {
    /// A sheet of `paper` colour (0..1) for a `width` × `height` px plate. `seed` varies the
    /// fibres between fences; dark paper gets less relief, as its light is weaker.
    pub(crate) fn plate(
        width: u32,
        height: u32,
        scale: f32,
        radius_dip: f32,
        paper: [f32; 3],
        seed: f32,
    ) -> Self {
        let scale = scale.max(0.5);
        let (w, h) = (width.max(1) as f32, height.max(1) as f32);
        let luminance = 0.2126 * paper[0] + 0.7152 * paper[1] + 0.0722 * paper[2];
        let dark = luminance < 0.4;
        Self {
            size: [w, h],
            radius: (radius_dip * scale).min(w.min(h) * 0.5).max(0.0),
            scale,
            paper,
            tone: if dark { 0.10 } else { 0.07 },
            light: light(),
            fiber: 0.18,
            grain: 0.1,
            relief: if dark { 0.55 } else { 0.8 },
            seed: seed.rem_euclid(997.0),
            edge: if dark { 0.10 } else { 0.06 },
            speckle: 1.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RectL {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

type QueryInterface =
    unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT;
type RefCount = unsafe extern "system" fn(*mut c_void) -> u32;

#[repr(C)]
struct EffectImplVtbl {
    query_interface: QueryInterface,
    add_ref: RefCount,
    release: RefCount,
    initialize: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> HRESULT,
    prepare_for_render: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
    set_graph: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
}

#[repr(C)]
struct DrawTransformVtbl {
    query_interface: QueryInterface,
    add_ref: RefCount,
    release: RefCount,
    get_input_count: unsafe extern "system" fn(*mut c_void) -> u32,
    map_output_rect_to_input_rects:
        unsafe extern "system" fn(*mut c_void, *const RectL, *mut RectL, u32) -> HRESULT,
    map_input_rects_to_output_rect: unsafe extern "system" fn(
        *mut c_void,
        *const RectL,
        *const RectL,
        u32,
        *mut RectL,
        *mut RectL,
    ) -> HRESULT,
    map_invalid_rect: unsafe extern "system" fn(*mut c_void, u32, RectL, *mut RectL) -> HRESULT,
    set_draw_info: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
}

/// One object, two interfaces: `effect` is the `ID2D1EffectImpl` pointer handed to Direct2D,
/// `transform` the `ID2D1DrawTransform` placed as the effect's only graph node.
#[repr(C)]
struct ShaderEffect {
    effect: *const EffectImplVtbl,
    transform: *const DrawTransformVtbl,
    refs: AtomicU32,
    kind: &'static Kind,
    /// The node's `ID2D1DrawInfo`, AddRef'd by `set_draw_info`.
    draw_info: Cell<*mut c_void>,
    constants: Cell<Bytes>,
}

static EFFECT_VTBL: EffectImplVtbl = EffectImplVtbl {
    query_interface: effect_query_interface,
    add_ref: effect_add_ref,
    release: effect_release,
    initialize,
    prepare_for_render,
    set_graph,
};

static TRANSFORM_VTBL: DrawTransformVtbl = DrawTransformVtbl {
    query_interface: transform_query_interface,
    add_ref: transform_add_ref,
    release: transform_release,
    get_input_count,
    map_output_rect_to_input_rects,
    map_input_rects_to_output_rect,
    map_invalid_rect,
    set_draw_info,
};

/// Reads vtable entry `index` of a foreign COM interface pointer.
///
/// # Safety
/// `this` must be a live interface whose vtable has a function of type `F` at `index`.
unsafe fn method<F: Copy>(this: *mut c_void, index: usize) -> F {
    unsafe {
        let vtbl = *(this as *const *const *const c_void);
        std::mem::transmute_copy(&*vtbl.add(index))
    }
}

unsafe fn release_raw(this: *mut c_void) {
    if !this.is_null() {
        unsafe { method::<RefCount>(this, 2)(this) };
    }
}

fn from_effect(this: *mut c_void) -> *mut ShaderEffect {
    this.cast()
}

fn from_transform(this: *mut c_void) -> *mut ShaderEffect {
    // SAFETY: `transform` is a field of ShaderEffect; Direct2D only hands back pointers we gave.
    unsafe {
        this.cast::<u8>()
            .sub(std::mem::offset_of!(ShaderEffect, transform))
            .cast()
    }
}

unsafe fn query(object: *mut ShaderEffect, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let iid = unsafe { *iid };
    let pointer: *mut c_void = if iid == IID_IUNKNOWN || iid == IID_EFFECT_IMPL {
        unsafe { (&raw mut (*object).effect).cast() }
    } else if iid == IID_DRAW_TRANSFORM || iid == IID_TRANSFORM || iid == IID_TRANSFORM_NODE {
        unsafe { (&raw mut (*object).transform).cast() }
    } else {
        unsafe { *out = std::ptr::null_mut() };
        return E_NOINTERFACE;
    };
    unsafe {
        (*object).refs.fetch_add(1, Ordering::Relaxed);
        *out = pointer;
    }
    S_OK
}

unsafe fn add_ref(object: *mut ShaderEffect) -> u32 {
    unsafe { (*object).refs.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe fn release(object: *mut ShaderEffect) -> u32 {
    let left = unsafe { (*object).refs.fetch_sub(1, Ordering::AcqRel) - 1 };
    if left == 0 {
        unsafe {
            release_raw((*object).draw_info.get());
            drop(Box::from_raw(object));
        }
    }
    left
}

unsafe extern "system" fn effect_query_interface(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    unsafe { query(from_effect(this), iid, out) }
}
unsafe extern "system" fn effect_add_ref(this: *mut c_void) -> u32 {
    unsafe { add_ref(from_effect(this)) }
}
unsafe extern "system" fn effect_release(this: *mut c_void) -> u32 {
    unsafe { release(from_effect(this)) }
}
unsafe extern "system" fn transform_query_interface(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    unsafe { query(from_transform(this), iid, out) }
}
unsafe extern "system" fn transform_add_ref(this: *mut c_void) -> u32 {
    unsafe { add_ref(from_transform(this)) }
}
unsafe extern "system" fn transform_release(this: *mut c_void) -> u32 {
    unsafe { release(from_transform(this)) }
}

/// `ID2D1EffectImpl::Initialize`: load the shader and make our transform the whole graph.
unsafe extern "system" fn initialize(
    this: *mut c_void,
    context: *mut c_void,
    graph: *mut c_void,
) -> HRESULT {
    if context.is_null() || graph.is_null() {
        return E_INVALIDARG;
    }
    type LoadPixelShader =
        unsafe extern "system" fn(*mut c_void, *const GUID, *const u8, u32) -> HRESULT;
    type SetSingleTransformNode = unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT;
    unsafe {
        let object = from_effect(this);
        let kind = (*object).kind;
        // ID2D1EffectContext: IUnknown (3), GetDpi … CreateBoundsAdjustmentTransform (8).
        let hr = method::<LoadPixelShader>(context, 11)(
            context,
            &kind.shader_id,
            kind.shader.as_ptr(),
            kind.shader.len() as u32,
        );
        if hr.is_err() {
            return hr;
        }
        // ID2D1TransformGraph: IUnknown (3), GetInputCount (1).
        method::<SetSingleTransformNode>(graph, 4)(graph, (&raw mut (*object).transform).cast())
    }
}

/// `ID2D1EffectImpl::PrepareForRender`: upload the current constants.
unsafe extern "system" fn prepare_for_render(this: *mut c_void, _change: u32) -> HRESULT {
    type SetConstantBuffer = unsafe extern "system" fn(*mut c_void, *const u8, u32) -> HRESULT;
    unsafe {
        let object = from_effect(this);
        let info = (*object).draw_info.get();
        if info.is_null() {
            return S_OK;
        }
        let bytes = (*object).constants.get();
        // ID2D1DrawInfo: IUnknown (3), ID2D1RenderInfo (4).
        method::<SetConstantBuffer>(info, 7)(info, bytes.as_ptr(), bytes.len() as u32)
    }
}

unsafe extern "system" fn set_graph(_this: *mut c_void, _graph: *mut c_void) -> HRESULT {
    E_NOTIMPL // the input count is fixed
}

unsafe extern "system" fn get_input_count(this: *mut c_void) -> u32 {
    unsafe { (*from_transform(this)).kind.inputs }
}

unsafe extern "system" fn map_output_rect_to_input_rects(
    this: *mut c_void,
    output: *const RectL,
    inputs: *mut RectL,
    count: u32,
) -> HRESULT {
    if output.is_null() || (count > 0 && inputs.is_null()) {
        return E_INVALIDARG;
    }
    unsafe {
        let object = from_transform(this);
        let (_, reach) = ((*object).kind.extent)(&(*object).constants.get());
        let o = *output;
        for i in 0..count as usize {
            *inputs.add(i) = RectL {
                left: o.left.saturating_sub(reach),
                top: o.top.saturating_sub(reach),
                right: o.right.saturating_add(reach),
                bottom: o.bottom.saturating_add(reach),
            };
        }
    }
    S_OK
}

unsafe extern "system" fn map_input_rects_to_output_rect(
    this: *mut c_void,
    _inputs: *const RectL,
    _opaque_inputs: *const RectL,
    _count: u32,
    output: *mut RectL,
    opaque: *mut RectL,
) -> HRESULT {
    if output.is_null() || opaque.is_null() {
        return E_INVALIDARG;
    }
    unsafe {
        let object = from_transform(this);
        let (plate, _) = ((*object).kind.extent)(&(*object).constants.get());
        *output = plate;
        *opaque = RectL::default();
    }
    S_OK
}

unsafe extern "system" fn map_invalid_rect(
    this: *mut c_void,
    _index: u32,
    _invalid: RectL,
    output: *mut RectL,
) -> HRESULT {
    if output.is_null() {
        return E_INVALIDARG;
    }
    // Any input pixel can be refracted anywhere on the rim: invalidate the whole plate.
    unsafe {
        let object = from_transform(this);
        *output = ((*object).kind.extent)(&(*object).constants.get()).0;
    }
    S_OK
}

unsafe extern "system" fn set_draw_info(this: *mut c_void, info: *mut c_void) -> HRESULT {
    if info.is_null() {
        return E_INVALIDARG;
    }
    type SetPixelShader = unsafe extern "system" fn(*mut c_void, *const GUID, u32) -> HRESULT;
    unsafe {
        let object = from_transform(this);
        method::<RefCount>(info, 1)(info);
        release_raw((*object).draw_info.replace(info));
        // ID2D1DrawInfo slot 10; D2D1_PIXEL_OPTIONS_NONE.
        method::<SetPixelShader>(info, 10)(info, &(*object).kind.shader_id, 0)
    }
}

/// `PD2D1_PROPERTY_SET_FUNCTION` for the `Constants` blob.
unsafe extern "system" fn set_constants(
    effect: *mut c_void,
    data: *const u8,
    size: u32,
) -> HRESULT {
    if effect.is_null() || data.is_null() || size as usize != CONSTANTS_LEN {
        return E_INVALIDARG;
    }
    unsafe {
        let mut bytes = [0u8; CONSTANTS_LEN];
        std::ptr::copy_nonoverlapping(data, bytes.as_mut_ptr(), CONSTANTS_LEN);
        (*from_effect(effect)).constants.set(bytes);
    }
    S_OK
}

/// `PD2D1_PROPERTY_GET_FUNCTION` for the `Constants` blob.
unsafe extern "system" fn get_constants(
    effect: *const c_void,
    data: *mut u8,
    size: u32,
    actual: *mut u32,
) -> HRESULT {
    if effect.is_null() {
        return E_INVALIDARG;
    }
    unsafe {
        if !actual.is_null() {
            *actual = CONSTANTS_LEN as u32;
        }
        if data.is_null() {
            return S_OK;
        }
        if (size as usize) < CONSTANTS_LEN {
            return E_INVALIDARG;
        }
        let bytes = (*from_effect(effect.cast_mut())).constants.get();
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, CONSTANTS_LEN);
    }
    S_OK
}

/// A new effect instance of `kind` with one reference for the caller.
unsafe fn create(kind: &'static Kind, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let object = Box::into_raw(Box::new(ShaderEffect {
        effect: &EFFECT_VTBL,
        transform: &TRANSFORM_VTBL,
        refs: AtomicU32::new(1),
        kind,
        draw_info: Cell::new(std::ptr::null_mut()),
        constants: Cell::new([0u8; CONSTANTS_LEN]),
    }));
    unsafe { *out = (&raw mut (*object).effect).cast() };
    S_OK
}

/// `PD2D1_EFFECT_FACTORY` for [`GLASS`].
unsafe extern "system" fn create_glass(out: *mut *mut c_void) -> HRESULT {
    unsafe { create(&GLASS, out) }
}

/// `PD2D1_EFFECT_FACTORY` for [`PAPER`].
unsafe extern "system" fn create_paper(out: *mut *mut c_void) -> HRESULT {
    unsafe { create(&PAPER, out) }
}

#[repr(C)]
struct PropertyBinding {
    name: *const u16,
    set: unsafe extern "system" fn(*mut c_void, *const u8, u32) -> HRESULT,
    get: unsafe extern "system" fn(*const c_void, *mut u8, u32, *mut u32) -> HRESULT,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Registers `kind` on `context`'s factory. Registering again on the same factory only adds
/// a reference, so a recreated device needs no bookkeeping.
pub(crate) fn register(context: &b::ID2D1DeviceContext, kind: &'static Kind) -> Result<()> {
    type GetFactory = unsafe extern "system" fn(*mut c_void, *mut *mut c_void);
    type RegisterFromString = unsafe extern "system" fn(
        *mut c_void,
        *const GUID,
        *const u16,
        *const PropertyBinding,
        u32,
        unsafe extern "system" fn(*mut *mut c_void) -> HRESULT,
    ) -> HRESULT;
    let xml = wide(kind.xml);
    let name = wide("Constants");
    let bindings = [PropertyBinding {
        name: name.as_ptr(),
        set: set_constants,
        get: get_constants,
    }];
    unsafe {
        let this = context.as_raw();
        let mut factory = std::ptr::null_mut();
        // ID2D1Resource::GetFactory, slot 3; returns an AddRef'd ID2D1Factory.
        method::<GetFactory>(this, 3)(this, &mut factory);
        if factory.is_null() {
            return Err(windows_core::Error::from_hresult(E_NOINTERFACE));
        }
        let mut factory1 = std::ptr::null_mut();
        let hr = method::<QueryInterface>(factory, 0)(factory, &IID_FACTORY1, &mut factory1);
        release_raw(factory);
        hr.ok()?;
        // ID2D1Factory1: IUnknown (3), ID2D1Factory (14), CreateDevice … RegisterEffectFromStream (6).
        let hr = method::<RegisterFromString>(factory1, 23)(
            factory1,
            &kind.clsid,
            xml.as_ptr(),
            bindings.as_ptr(),
            bindings.len() as u32,
            kind.create,
        );
        release_raw(factory1);
        hr.ok()
    }
}

/// Creates an instance of `kind` on `context` (after `register`).
pub(crate) fn create_effect(
    context: &b::ID2D1DeviceContext,
    kind: &'static Kind,
) -> Result<b::ID2D1Effect> {
    unsafe { context.CreateEffect(&kind.clsid) }
}

/// Sets the `Constants` blob (custom property 0).
pub(crate) fn set<T: Constants>(effect: &b::ID2D1Effect, constants: &T) -> Result<()> {
    unsafe {
        effect
            .SetValue(0, D2D1_PROPERTY_TYPE_BLOB, &constants.bytes())
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv(source: &[u8]) -> String {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for &byte in source.iter().filter(|&&b| b != b'\r') {
            hash = (hash ^ byte as u64).wrapping_mul(0x0100_0000_01b3);
        }
        format!("{hash:016x}")
    }

    #[test]
    fn compiled_shaders_match_their_sources() {
        for (source, recorded, kind) in [
            (
                &include_bytes!("../shaders/liquid_glass.hlsl")[..],
                include_str!("../shaders/liquid_glass.hlsl.fnv"),
                &GLASS,
            ),
            (
                &include_bytes!("../shaders/paper.hlsl")[..],
                include_str!("../shaders/paper.hlsl.fnv"),
                &PAPER,
            ),
        ] {
            assert_eq!(
                fnv(source),
                recorded.trim(),
                "a shader changed: run scripts/compile-shaders.sh"
            );
            assert_eq!(&kind.shader[..4], b"DXBC");
        }
        assert_ne!(GLASS.clsid, PAPER.clsid);
        assert_ne!(GLASS.shader_id, PAPER.shader_id);
    }

    #[test]
    fn rim_stays_within_the_corner_and_inputs_cover_the_reach() {
        for (w, h, scale) in [(600, 400, 1.0), (900, 600, 1.5), (60, 30, 2.0), (1, 1, 1.0)] {
            let c = GlassConstants::plate(w, h, scale, 24.0);
            assert!(c.radius <= (w.min(h) as f32) * 0.5 + 1e-3);
            assert!(
                c.bezel <= c.radius.max(1.0) + 1e-3,
                "{w}x{h}@{scale}: {c:?}"
            );
            let (plate, reach) = (GLASS.extent)(&c.bytes());
            assert!(reach as f32 > c.max_offset);
            assert_eq!((plate.right, plate.bottom), (w as i32, h as i32));
            let n = (c.light[0].powi(2) + c.light[1].powi(2)).sqrt();
            assert!((n - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn paper_round_trips_and_samples_nothing_outside_itself() {
        let c = PaperConstants::plate(480, 240, 1.5, 8.0, [0.96, 0.94, 0.9], 1234.5);
        assert_eq!(read::<PaperConstants>(&c.bytes()), c);
        let (plate, reach) = (PAPER.extent)(&c.bytes());
        assert_eq!((plate.right, plate.bottom, reach), (480, 240, 0));
        assert!((0.0..997.0).contains(&c.seed));
        assert_eq!(c.radius, 12.0);
        let dark = PaperConstants::plate(480, 240, 1.0, 8.0, [0.14, 0.13, 0.12], 1.0);
        assert!(dark.relief < c.relief, "dark paper is lit less");
    }
}

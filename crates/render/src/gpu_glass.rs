//! GPU glass over a shared wallpaper texture. Movement only updates a transform:
//! the lens, source upload and effect graph stay resident.
//!
//! The lens is a material pixel shader (`material_shaders`). Liquid Glass refracts every
//! pixel through the plate's curved surface; devices that cannot run it keep the built-in
//! chain (a CPU displacement field fed to three DisplacementMap effects plus a glint
//! bitmap). Paper is a cached procedural sheet blended with the blurred wallpaper behind it.
use crate::material_shaders::{self as shaders, GlassConstants, PaperConstants};
use crate::{Image, MonitorBackdrop, gpu_bindings as b};
use std::{collections::HashMap, rc::Rc};
use windows_canvas::{DrawingSession, Matrix3x2, Rect};
use windows_core::{Interface, Result};

fn property<T: Copy>(effect: &b::ID2D1Effect, index: i32, kind: i32, value: &T) -> Result<()> {
    // Only POD scalar/float arrays are passed by this module.
    let bytes = unsafe { std::slice::from_raw_parts((value as *const T).cast(), size_of::<T>()) };
    unsafe { effect.SetValue(index as u32, kind, bytes).ok() }
}

fn scalar(effect: &b::ID2D1Effect, index: i32, value: f32) -> Result<()> {
    property(effect, index, b::D2D1_PROPERTY_TYPE_FLOAT, &value)
}

fn enumeration(effect: &b::ID2D1Effect, index: i32, value: i32) -> Result<()> {
    property(effect, index, b::D2D1_PROPERTY_TYPE_ENUM, &value)
}

fn matrix(effect: &b::ID2D1Effect, value: [f32; 6]) -> Result<()> {
    property(
        effect,
        b::D2D1_2DAFFINETRANSFORM_PROP_TRANSFORM_MATRIX,
        b::D2D1_PROPERTY_TYPE_MATRIX_3X2,
        &value,
    )
}

fn color_matrix(effect: &b::ID2D1Effect, value: [f32; 20]) -> Result<()> {
    enumeration(
        effect,
        b::D2D1_COLORMATRIX_PROP_ALPHA_MODE,
        b::D2D1_COLORMATRIX_ALPHA_MODE_STRAIGHT,
    )?;
    property(
        effect,
        b::D2D1_COLORMATRIX_PROP_COLOR_MATRIX,
        b::D2D1_PROPERTY_TYPE_MATRIX_5X4,
        &value,
    )
}

fn bitmap(
    context: &b::ID2D1DeviceContext,
    width: u32,
    height: u32,
    data: *const std::ffi::c_void,
    pitch: u32,
    float: bool,
) -> Result<b::ID2D1Bitmap1> {
    let properties = b::D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: b::D2D1_PIXEL_FORMAT {
            format: if float {
                b::DXGI_FORMAT_R32G32B32A32_FLOAT
            } else {
                b::DXGI_FORMAT_B8G8R8A8_UNORM
            },
            alphaMode: b::D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: b::D2D1_BITMAP_OPTIONS_NONE,
        ..Default::default()
    };
    unsafe {
        context.CreateBitmap(
            b::D2D_SIZE_U { width, height },
            Some(data),
            pitch,
            &properties,
        )
    }
}

struct Wallpaper {
    image: b::ID2D1Image,
}

/// Shared by every fence on this D2D device. Rc identity tracks wallpaper/theme changes.
#[derive(Default)]
pub struct WallpaperCache {
    source: Option<Rc<Vec<MonitorBackdrop>>>,
    uploaded: Option<Rc<Wallpaper>>,
    pub uploads: u64,
}

impl WallpaperCache {
    pub fn clear(&mut self) {
        self.source = None;
        self.uploaded = None;
    }

    fn get(
        &mut self,
        context: &b::ID2D1DeviceContext,
        sources: &Rc<Vec<MonitorBackdrop>>,
    ) -> Result<Rc<Wallpaper>> {
        if self.source.as_ref().is_some_and(|s| Rc::ptr_eq(s, sources)) {
            return Ok(self.uploaded.as_ref().unwrap().clone());
        }
        let valid: Vec<_> = sources
            .iter()
            .filter(|s| s.image.width > 0 && s.image.height > 0)
            .collect();
        if valid.is_empty() {
            return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                0x80070057u32 as i32,
            )));
        }
        let composite = unsafe { context.CreateEffect(&b::CLSID_D2D1Composite)? };
        unsafe { composite.SetInputCount(valid.len() as u32).ok()? };
        for (index, source) in valid.into_iter().enumerate() {
            let Image {
                width,
                height,
                bgra,
            } = &source.image;
            let image = bitmap(
                context,
                *width,
                *height,
                bgra.as_ptr().cast(),
                width * 4,
                false,
            )?;
            let transform = unsafe { context.CreateEffect(&b::CLSID_D2D12DAffineTransform)? };
            matrix(
                &transform,
                [
                    source.downscale as f32,
                    0.0,
                    0.0,
                    source.downscale as f32,
                    source.left as f32,
                    source.top as f32,
                ],
            )?;
            unsafe {
                transform.SetInput(0, &image.cast::<b::ID2D1Image>()?, true);
                composite.SetInput(index as u32, &transform.GetOutput()?, true);
            }
            self.uploads += 1;
        }
        let uploaded = Rc::new(Wallpaper {
            image: unsafe { composite.GetOutput()? },
        });
        self.source = Some(sources.clone());
        self.uploaded = Some(uploaded.clone());
        Ok(uploaded)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub frames: u64,
    pub map_builds: u64,
    pub control_frames: u64,
    pub control_map_builds: u64,
}

/// What a plate is made of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Surface {
    /// Liquid Glass: refracted wallpaper inside the plate's own antialiased outline.
    Glass,
    /// Matte paper of `color` (0..1); `translucency` of the blurred wallpaper shows through,
    /// tinted by the paper; `seed` varies the fibres per fence. DWM rounds the window, so
    /// the plate fills its whole rectangle.
    Paper {
        color: [f32; 3],
        translucency: f32,
        seed: f32,
    },
}

impl Surface {
    fn is_paper(self) -> bool {
        matches!(self, Self::Paper { .. })
    }
}

/// How a plate bends the wallpaper.
enum Lens {
    /// The custom pixel shader; `sharp` positions the unblurred wallpaper (input 0).
    Shader {
        effect: b::ID2D1Effect,
        sharp_source: b::ID2D1Effect,
        sharp: b::ID2D1Effect,
    },
    /// Built-in fallback: three channel-offset DisplacementMaps and a glint overlay.
    Field {
        displace: [b::ID2D1Effect; 3],
        glints: Option<b::ID2D1Bitmap1>,
    },
    /// The cached paper sheet (`effect`) and its blend with the blurred wallpaper.
    Paper {
        effect: b::ID2D1Effect,
        blend: b::ID2D1Effect,
        sheet: Option<PaperConstants>,
        translucency: Option<u32>,
    },
}

struct Effects {
    source: Option<Rc<Wallpaper>>,
    blur: b::ID2D1Effect,
    /// Positions the frosted (blurred) wallpaper.
    position: b::ID2D1Effect,
    lens: Lens,
    output: b::ID2D1Effect,
    surface: Rc<Wallpaper>,
    geometry: Option<(u32, u32, u32, u32)>,
    opacity: Option<u32>,
}

/// Clamps an image's edge texels outward, so blurred and refracted samples past a monitor
/// edge repeat it instead of fading to transparent.
fn clamped(context: &b::ID2D1DeviceContext) -> Result<b::ID2D1Effect> {
    let border = unsafe { context.CreateEffect(&b::CLSID_D2D1Border)? };
    enumeration(
        &border,
        b::D2D1_BORDER_PROP_EDGE_MODE_X,
        b::D2D1_BORDER_EDGE_MODE_CLAMP,
    )?;
    enumeration(
        &border,
        b::D2D1_BORDER_PROP_EDGE_MODE_Y,
        b::D2D1_BORDER_EDGE_MODE_CLAMP,
    )?;
    Ok(border)
}

/// Registers `kind` once per device; `tried` remembers the outcome.
fn registered(
    context: &b::ID2D1DeviceContext,
    kind: &'static shaders::Kind,
    tried: &mut Option<bool>,
    what: &str,
) -> bool {
    if tried.is_none() {
        let result = shaders::register(context, kind);
        if let Err(error) = &result {
            tracing::warn!(%error, what, "material shader unavailable");
        }
        *tried = Some(result.is_ok());
    }
    *tried == Some(true)
}

/// Glass: the shader lens when this device registered it, else the built-in chain.
fn new_effects(context: &b::ID2D1DeviceContext, shader: &mut Option<bool>) -> Result<Effects> {
    if registered(context, &shaders::GLASS, shader, "Liquid Glass") {
        match Effects::new(context, true) {
            Ok(effects) => return Ok(effects),
            Err(error) => {
                tracing::warn!(%error, "Liquid Glass shader failed; using built-in effects");
                *shader = Some(false);
            }
        }
    }
    Effects::new(context, false)
}

impl Effects {
    fn new(context: &b::ID2D1DeviceContext, shader: bool) -> Result<Self> {
        let effect = |id| unsafe { context.CreateEffect(id) };
        let blur = effect(&b::CLSID_D2D1GaussianBlur)?;
        enumeration(
            &blur,
            b::D2D1_GAUSSIANBLUR_PROP_BORDER_MODE,
            b::D2D1_BORDER_MODE_HARD,
        )?;
        property(
            &blur,
            b::D2D1_PROPERTY_CACHED,
            b::D2D1_PROPERTY_TYPE_BOOL,
            &1u32,
        )?;
        let border = clamped(context)?;
        let position = effect(&b::CLSID_D2D12DAffineTransform)?;
        unsafe {
            border.SetInput(0, &blur.GetOutput()?, true);
            position.SetInput(0, &border.GetOutput()?, true);
        }
        if shader {
            let lens = shaders::create_effect(context, &shaders::GLASS)?;
            let sharp_source = clamped(context)?;
            let sharp = effect(&b::CLSID_D2D12DAffineTransform)?;
            let output = effect(&b::CLSID_D2D1ColorMatrix)?;
            unsafe {
                sharp.SetInput(0, &sharp_source.GetOutput()?, true);
                lens.SetInput(0, &sharp.GetOutput()?, true);
                lens.SetInput(1, &position.GetOutput()?, true);
                output.SetInput(0, &lens.GetOutput()?, true);
            }
            let surface = Rc::new(Wallpaper {
                image: unsafe { lens.GetOutput()? },
            });
            return Ok(Self {
                source: None,
                blur,
                position,
                lens: Lens::Shader {
                    effect: lens,
                    sharp_source,
                    sharp,
                },
                output,
                surface,
                geometry: None,
                opacity: None,
            });
        }
        let displace = [
            effect(&b::CLSID_D2D1DisplacementMap)?,
            effect(&b::CLSID_D2D1DisplacementMap)?,
            effect(&b::CLSID_D2D1DisplacementMap)?,
        ];
        let mut channels = Vec::new();
        for (i, displacement) in displace.iter().enumerate() {
            enumeration(
                displacement,
                b::D2D1_DISPLACEMENTMAP_PROP_X_CHANNEL_SELECT,
                b::D2D1_CHANNEL_SELECTOR_R,
            )?;
            enumeration(
                displacement,
                b::D2D1_DISPLACEMENTMAP_PROP_Y_CHANNEL_SELECT,
                b::D2D1_CHANNEL_SELECTOR_G,
            )?;
            unsafe { displacement.SetInput(0, &position.GetOutput()?, true) };
            let channel = effect(&b::CLSID_D2D1ColorMatrix)?;
            let mut values = [0.0; 20];
            values[i * 5] = 1.0;
            values[15] = 1.0;
            color_matrix(&channel, values)?;
            unsafe { channel.SetInput(0, &displacement.GetOutput()?, true) };
            channels.push(channel);
        }
        let add = |a: &b::ID2D1Effect, c: &b::ID2D1Effect| -> Result<b::ID2D1Effect> {
            let sum = effect(&b::CLSID_D2D1ArithmeticComposite)?;
            property(
                &sum,
                b::D2D1_ARITHMETICCOMPOSITE_PROP_COEFFICIENTS,
                b::D2D1_PROPERTY_TYPE_VECTOR4,
                &[0.0f32, 1.0, 1.0, 0.0],
            )?;
            property(
                &sum,
                b::D2D1_ARITHMETICCOMPOSITE_PROP_CLAMP_OUTPUT,
                b::D2D1_PROPERTY_TYPE_BOOL,
                &1u32,
            )?;
            unsafe {
                sum.SetInput(0, &a.GetOutput()?, true);
                sum.SetInput(1, &c.GetOutput()?, true);
            }
            Ok(sum)
        };
        let red_green = add(&channels[0], &channels[1])?;
        let rgb = add(&red_green, &channels[2])?;
        let saturation = effect(&b::CLSID_D2D1Saturation)?;
        scalar(&saturation, b::D2D1_SATURATION_PROP_SATURATION, 1.08)?;
        let output = effect(&b::CLSID_D2D1ColorMatrix)?;
        unsafe {
            saturation.SetInput(0, &rgb.GetOutput()?, true);
            output.SetInput(0, &saturation.GetOutput()?, true);
        }
        Ok(Self {
            source: None,
            blur,
            position,
            lens: Lens::Field {
                displace,
                glints: None,
            },
            output,
            surface: Rc::new(Wallpaper {
                image: unsafe { saturation.GetOutput()? },
            }),
            geometry: None,
            opacity: None,
        })
    }

    /// Paper: the cached sheet, blended with the positioned blurred wallpaper, then opacity.
    fn new_paper(context: &b::ID2D1DeviceContext) -> Result<Self> {
        let effect = |id| unsafe { context.CreateEffect(id) };
        let blur = effect(&b::CLSID_D2D1GaussianBlur)?;
        enumeration(
            &blur,
            b::D2D1_GAUSSIANBLUR_PROP_BORDER_MODE,
            b::D2D1_BORDER_MODE_HARD,
        )?;
        property(
            &blur,
            b::D2D1_PROPERTY_CACHED,
            b::D2D1_PROPERTY_TYPE_BOOL,
            &1u32,
        )?;
        let border = clamped(context)?;
        let position = effect(&b::CLSID_D2D12DAffineTransform)?;
        let sheet = shaders::create_effect(context, &shaders::PAPER)?;
        // The sheet depends only on size, colour and seed: keep its pixels between frames,
        // so moving or hovering a fence only re-runs the blend.
        property(
            &sheet,
            b::D2D1_PROPERTY_CACHED,
            b::D2D1_PROPERTY_TYPE_BOOL,
            &1u32,
        )?;
        let blend = effect(&b::CLSID_D2D1ArithmeticComposite)?;
        property(
            &blend,
            b::D2D1_ARITHMETICCOMPOSITE_PROP_CLAMP_OUTPUT,
            b::D2D1_PROPERTY_TYPE_BOOL,
            &1u32,
        )?;
        let output = effect(&b::CLSID_D2D1ColorMatrix)?;
        unsafe {
            border.SetInput(0, &blur.GetOutput()?, true);
            position.SetInput(0, &border.GetOutput()?, true);
            blend.SetInput(0, &sheet.GetOutput()?, true);
            blend.SetInput(1, &position.GetOutput()?, true);
            output.SetInput(0, &blend.GetOutput()?, true);
        }
        let surface = Rc::new(Wallpaper {
            image: unsafe { blend.GetOutput()? },
        });
        Ok(Self {
            source: None,
            blur,
            position,
            lens: Lens::Paper {
                effect: sheet,
                blend,
                sheet: None,
                translucency: None,
            },
            output,
            surface,
            geometry: None,
            opacity: None,
        })
    }

    fn is_paper(&self) -> bool {
        matches!(self.lens, Lens::Paper { .. })
    }

    /// Paper's constants, blur and blend, each set only when it changed.
    fn paper(&mut self, w: u32, h: u32, scale: f32, radius: f32, surface: Surface) -> Result<bool> {
        let (
            Surface::Paper {
                color,
                translucency,
                seed,
            },
            Lens::Paper {
                effect,
                blend,
                sheet,
                translucency: applied,
            },
        ) = (surface, &mut self.lens)
        else {
            return Ok(false);
        };
        let wanted = PaperConstants::plate(w, h, scale, radius, color, seed);
        let rebuilt = *sheet != Some(wanted);
        if rebuilt {
            shaders::set(effect, &wanted)?;
            *sheet = Some(wanted);
        }
        let t = translucency.clamp(0.0, 0.5);
        if *applied != Some(t.to_bits()) {
            // sheet × (1 − t) + sheet × wallpaper × 1.12 t: the wallpaper reads through the
            // paper's own colour, a little lifted, like light through vellum.
            property(
                blend,
                b::D2D1_ARITHMETICCOMPOSITE_PROP_COEFFICIENTS,
                b::D2D1_PROPERTY_TYPE_VECTOR4,
                &[1.12 * t, 1.0 - t, 0.0, 0.0f32],
            )?;
            *applied = Some(t.to_bits());
        }
        if self.geometry.is_none_or(|old| old.2 != scale.to_bits()) {
            scalar(
                &self.blur,
                b::D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION,
                10.0 * scale,
            )?;
        }
        self.geometry = Some((w, h, scale.to_bits(), radius.to_bits()));
        Ok(rebuilt)
    }

    fn set_source(&mut self, source: Rc<Wallpaper>) {
        if self
            .source
            .as_ref()
            .is_some_and(|old| Rc::ptr_eq(old, &source))
        {
            return;
        }
        unsafe {
            self.blur.SetInput(0, &source.image, true);
            if let Lens::Shader { sharp_source, .. } = &self.lens {
                sharp_source.SetInput(0, &source.image, true);
            }
        }
        self.source = Some(source);
    }

    /// Places the wallpaper so that pixel (`x`, `y`) of it lands on the plate's origin.
    fn set_position(&self, x: f32, y: f32) -> Result<()> {
        let at = [1.0, 0.0, 0.0, 1.0, -x, -y];
        matrix(&self.position, at)?;
        if let Lens::Shader { sharp, .. } = &self.lens {
            matrix(sharp, at)?;
        }
        Ok(())
    }

    /// The overlay drawn over the refracted wallpaper; the shader lights the rim itself.
    fn glints(&self) -> Option<&b::ID2D1Bitmap1> {
        match &self.lens {
            Lens::Field { glints, .. } => glints.as_ref(),
            Lens::Shader { .. } | Lens::Paper { .. } => None,
        }
    }

    fn geometry(
        &mut self,
        context: &b::ID2D1DeviceContext,
        w: u32,
        h: u32,
        scale: f32,
        radius: f32,
    ) -> Result<bool> {
        let key = (w, h, scale.to_bits(), radius.to_bits());
        if self.geometry == Some(key) {
            return Ok(false);
        }
        let frost_sigma = if let Lens::Shader { effect, .. } = &self.lens {
            shaders::set(effect, &GlassConstants::plate(w, h, scale, radius))?;
            // The frosted copy is genuinely soft; the shader still mixes in the sharp one.
            3.0 * scale
        } else {
            1.25 * scale
        };
        if self.geometry.is_none_or(|old| old.2 != scale.to_bits()) {
            scalar(
                &self.blur,
                b::D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION,
                frost_sigma,
            )?;
        }
        let Lens::Field { displace, glints } = &mut self.lens else {
            self.geometry = Some(key);
            return Ok(true);
        };
        let field = crate::liquid_glass::displacement_field(w, h, scale, radius);
        let map = bitmap(
            context,
            field.width,
            field.height,
            field.rgba.as_ptr().cast(),
            field.width * 16,
            true,
        )?;
        // Effects use pixel coordinates; bitmap DPI alone does not resize input 1.
        // Explicitly resample the cached field to the window, clamping its outer texels.
        let map_border = unsafe { context.CreateEffect(&b::CLSID_D2D1Border)? };
        enumeration(
            &map_border,
            b::D2D1_BORDER_PROP_EDGE_MODE_X,
            b::D2D1_BORDER_EDGE_MODE_CLAMP,
        )?;
        enumeration(
            &map_border,
            b::D2D1_BORDER_PROP_EDGE_MODE_Y,
            b::D2D1_BORDER_EDGE_MODE_CLAMP,
        )?;
        let map_scale = unsafe { context.CreateEffect(&b::CLSID_D2D12DAffineTransform)? };
        matrix(
            &map_scale,
            [
                w as f32 / field.width as f32,
                0.0,
                0.0,
                h as f32 / field.height as f32,
                0.0,
                0.0,
            ],
        )?;
        unsafe {
            map_border.SetInput(0, &map.cast::<b::ID2D1Image>()?, true);
            map_scale.SetInput(0, &map_border.GetOutput()?, true);
        }
        for (index, displacement) in displace.iter().enumerate() {
            let factor = [1.0 - 0.045, 1.0, 1.0 + 0.045][index];
            scalar(
                displacement,
                b::D2D1_DISPLACEMENTMAP_PROP_SCALE,
                field.scale_px * factor,
            )?;
            unsafe { displacement.SetInput(1, &map_scale.GetOutput()?, true) };
        }
        *glints = Some(bitmap(
            context,
            field.width,
            field.height,
            field.glints.as_ptr().cast(),
            field.width * 4,
            false,
        )?);
        self.geometry = Some(key);
        Ok(true)
    }
}

/// An image brush over `image` that clamps at its edges; callers set its source rectangle.
fn image_brush(
    context: &b::ID2D1DeviceContext,
    image: &b::ID2D1Image,
) -> Result<b::ID2D1ImageBrush> {
    let props = b::D2D1_IMAGE_BRUSH_PROPERTIES {
        sourceRectangle: b::D2D_RECT_F {
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
        },
        extendModeX: b::D2D1_EXTEND_MODE_CLAMP,
        extendModeY: b::D2D1_EXTEND_MODE_CLAMP,
        interpolationMode: b::D2D1_INTERPOLATION_MODE_LINEAR,
    };
    unsafe { context.CreateImageBrush(image, &props, None) }
}

#[derive(Default)]
pub struct GpuGlass {
    effects: Option<Effects>,
    /// Paints `effects`' output into the plate's rounded outline with antialiasing.
    plate: Option<b::ID2D1ImageBrush>,
    failed: bool,
    /// Whether this device runs the Liquid Glass shader; `None` until first tried.
    shader: Option<bool>,
    /// Whether this device runs the paper shader; `None` until first tried.
    paper_shader: Option<bool>,
    stats: Stats,
    controls: HashMap<u128, Control>,
    text_inks: HashMap<u128, (bool, u64)>,
}

struct Control {
    effects: Effects,
    brush: b::ID2D1ImageBrush,
    frame: u64,
}

impl GpuGlass {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }
    pub fn failed(&self) -> bool {
        self.failed
    }
    /// Whether plates use the pixel shader (false: built-in effects; `None`: not drawn yet).
    pub fn uses_shader(&self) -> Option<bool> {
        self.shader
    }
    /// Keeps this material on the built-in effect chain (comparison renders).
    pub fn use_builtin_effects(&mut self) {
        self.shader = Some(false);
    }

    pub fn draw(
        &mut self,
        session: &DrawingSession<'_>,
        cache: &mut WallpaperCache,
        sources: &Rc<Vec<MonitorBackdrop>>,
        rect: [i32; 4],
        scale: f32,
        radius: f32,
        opacity: f32,
        hover: f32,
        surface: Surface,
    ) -> Result<()> {
        if self.failed {
            return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                0x80004005u32 as i32,
            )));
        }
        let saved = session.transform();
        self.controls
            .retain(|key, control| *key < 5 || control.frame + 1 >= self.stats.frames);
        self.text_inks
            .retain(|_, (_, frame)| *frame + 1 >= self.stats.frames);
        session.set_transform(&Matrix3x2::identity());
        let result = self.draw_inner(
            session, cache, sources, rect, scale, radius, opacity, hover, surface,
        );
        session.set_transform(&saved);
        if let Err(error) = &result {
            if !windows_canvas::is_device_lost(error.code()) {
                tracing::warn!(%error, "GPU glass unavailable; using readable solid material");
                self.failed = true;
            }
        }
        result
    }

    pub fn control_foreground(
        &mut self,
        key: u128,
        theme: crate::Theme,
        sources: &[MonitorBackdrop],
        area: [i32; 4],
        opacity: f32,
        tint: Option<windows_canvas::ColorF>,
    ) -> crate::Theme {
        if self.failed {
            return theme;
        }
        // Selected tabs still own their inactive lens. Keep it warm for the next switch.
        if let Some(control) = self.controls.get_mut(&key) {
            control.frame = self.stats.frames;
        }
        let sample = crate::liquid_glass::foreground_sample(sources, area);
        let previous = self.text_inks.get(&key).map(|(dark, _)| *dark);
        let foreground = theme.over_glass_with_previous_ink(&sample, opacity, tint, previous);
        self.text_inks
            .insert(key, (foreground.text_primary.r < 0.5, self.stats.frames));
        foreground
    }

    /// A small lens samples the already-refracted fence surface, before its opacity and
    /// text. This shares the parent effect graph and never captures the desktop or uploads
    /// another wallpaper. ImageBrush provides an antialiased rounded mask at full DPI.
    pub fn draw_control(
        &mut self,
        session: &DrawingSession<'_>,
        key: u128,
        rect: Rect,
        scale: f32,
        radius: f32,
        opacity: f32,
    ) -> Result<bool> {
        let Some(parent) = self.effects.as_ref().filter(|_| !self.failed) else {
            return Ok(false);
        };
        if rect.width() <= 0.0 || rect.height() <= 0.0 || opacity <= 0.0 {
            return Ok(true);
        }
        let context: b::ID2D1DeviceContext = session.raw().cast()?;
        let source = parent.surface.clone();
        if let std::collections::hash_map::Entry::Vacant(entry) = self.controls.entry(key) {
            let effects = new_effects(&context, &mut self.shader)?;
            let brush = image_brush(&context, &effects.surface.image)?;
            entry.insert(Control {
                effects,
                brush,
                frame: self.stats.frames,
            });
        }
        let control = self.controls.get_mut(&key).unwrap();
        control.frame = self.stats.frames;
        let effects = &mut control.effects;
        effects.set_source(source);
        let (w, h) = (
            (rect.width() * scale).ceil().max(1.0) as u32,
            (rect.height() * scale).ceil().max(1.0) as u32,
        );
        if effects.geometry(&context, w, h, scale, radius)? {
            self.stats.control_map_builds += 1;
        }
        let (x, y) = (rect.left * scale, rect.top * scale);
        effects.set_position(x, y)?;
        let bounds = b::D2D_RECT_F {
            left: x,
            top: y,
            right: rect.right * scale,
            bottom: rect.bottom * scale,
        };
        let round = b::D2D1_ROUNDED_RECT {
            rect: bounds,
            radiusX: radius * scale,
            radiusY: radius * scale,
        };
        let saved = session.transform();
        session.set_transform(&Matrix3x2::identity());
        unsafe {
            control.brush.SetSourceRectangle(&b::D2D_RECT_F {
                left: 0.0,
                top: 0.0,
                right: w as f32,
                bottom: h as f32,
            });
            control
                .brush
                .SetTransform(&windows_numerics::Matrix3x2::translation(x, y));
            control.brush.SetOpacity(opacity.clamp(0.0, 1.0));
            context.FillRoundedRectangle(&round, &control.brush);
        }
        session.set_transform(&saved);
        self.stats.control_frames += 1;
        Ok(true)
    }

    fn draw_inner(
        &mut self,
        session: &DrawingSession<'_>,
        cache: &mut WallpaperCache,
        sources: &Rc<Vec<MonitorBackdrop>>,
        rect: [i32; 4],
        scale: f32,
        radius: f32,
        opacity: f32,
        hover: f32,
        surface: Surface,
    ) -> Result<()> {
        let context: b::ID2D1DeviceContext = session.raw().cast()?;
        let source = cache.get(&context, sources)?;
        if self
            .effects
            .as_ref()
            .is_some_and(|e| e.is_paper() != surface.is_paper())
        {
            // The theme style changed between glass and paper.
            self.effects = None;
            self.plate = None;
        }
        if self.effects.is_none() {
            let effects = if surface.is_paper() {
                if !registered(&context, &shaders::PAPER, &mut self.paper_shader, "Paper") {
                    return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                        0x80004001u32 as i32,
                    )));
                }
                Effects::new_paper(&context)?
            } else {
                new_effects(&context, &mut self.shader)?
            };
            self.plate = Some(image_brush(&context, &unsafe {
                effects.output.GetOutput()?
            })?);
            self.effects = Some(effects);
        }
        let effects = self.effects.as_mut().unwrap();
        effects.set_source(source);
        let w = rect[2].max(1) as u32;
        let h = rect[3].max(1) as u32;
        let built = if surface.is_paper() {
            effects.paper(w, h, scale, radius, surface)?
        } else {
            effects.geometry(&context, w, h, scale, radius)?
        };
        if built {
            self.stats.map_builds += 1;
        }
        effects.set_position(rect[0] as f32, rect[1] as f32)?;
        let opacity = opacity.clamp(0.0, 1.0);
        if effects.opacity != Some(opacity.to_bits()) {
            let mut values = [0.0; 20];
            for i in [0, 5, 10, 15] {
                values[i] = opacity;
            }
            color_matrix(&effects.output, values)?;
            effects.opacity = Some(opacity.to_bits());
        }
        let area = b::D2D_RECT_F {
            left: 0.0,
            top: 0.0,
            right: w as f32,
            bottom: h as f32,
        };
        // Glass: the compositor clip sits 1 px outside the plate; this fill draws the one
        // visible, antialiased outline (window pixels are physical, the transform identity).
        // Paper: DWM rounds the window, so the sheet fills its whole rectangle.
        let r = if surface.is_paper() {
            0.0
        } else {
            (radius * scale).min(w.min(h) as f32 * 0.5).max(0.0)
        };
        let plate = self.plate.as_ref().unwrap();
        unsafe {
            plate.SetSourceRectangle(&area);
            context.FillRoundedRectangle(
                &b::D2D1_ROUNDED_RECT {
                    rect: area,
                    radiusX: r,
                    radiusY: r,
                },
                plate,
            );
            if let Some(glints) = effects.glints() {
                let light = (0.85 + 0.15 * opacity + 0.18 * hover.clamp(0.0, 1.0)) / 1.18;
                context.DrawBitmap(
                    glints,
                    Some(&area),
                    opacity * light,
                    b::D2D1_INTERPOLATION_MODE_LINEAR,
                    None,
                    None,
                );
            }
        }
        self.stats.frames += 1;
        Ok(())
    }
}

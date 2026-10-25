//! What Bevy's PBR material cannot show by itself, drawn by a fragment
//! shader over it (`look.wgsl`): the Falloff map's view-dependent mix,
//! Ink 'n Paint's paint bands and ink outline, and Matte/Shadow's
//! shadow catching.

use bevy::app::{App, Plugin};
use bevy::asset::{load_internal_asset, weak_handle, Asset, Handle};
use bevy::math::Vec4;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, StandardMaterial};
use bevy::reflect::Reflect;
use bevy::render::render_resource::{AsBindGroup, Shader, ShaderRef, ShaderType};
use loom_scene::material::Surface;

const LOOK_SHADER: Handle<Shader> = weak_handle!("6d2f9a41-7c3e-4b85-9e10-2a4f8c6d1b73");

/// `LookParams::flags` bits.
pub const FALLOFF: u32 = 1;
pub const TOON: u32 = 2;
pub const MATTE: u32 = 4;

pub use uniform::LookParams;

// The ShaderType derive's generated layout checks are never called.
#[allow(dead_code)]
mod uniform {
    use super::*;

    /// The shader's uniform.
    #[derive(ShaderType, Reflect, Debug, Clone, Copy, Default, PartialEq)]
    pub struct LookParams {
        pub flags: u32,
        /// Falloff Type: 0 Towards/Away, 1 Perpendicular/Parallel, 2 Fresnel, 3 Shadow/Light, 4 Distance Blend.
        pub falloff_type: u32,
        /// Falloff Direction: 0 the viewing direction, 1 `dir` (world).
        pub falloff_dir: u32,
        /// Ink 'n Paint Paint Levels.
        pub levels: u32,
        /// Falloff: the Side colour (the material's base colour is the Front).
        pub side: Vec4,
        /// Falloff: the direction; xy of `near_far` the Distance Blend range, z the Fresnel IOR.
        pub dir: Vec4,
        pub near_far: Vec4,
        /// Ink 'n Paint: Lighted, Shaded, Highlight (w 1 = on), Ink colour (w = width, 0 = no ink).
        pub lighted: Vec4,
        pub shaded: Vec4,
        pub highlight: Vec4,
        pub ink: Vec4,
        /// Matte/Shadow: shadow colour (w = Shadow Brightness); x of `matte_flags` Receive Shadows.
        pub matte: Vec4,
        pub matte_flags: Vec4,
    }
}

/// The extension over Bevy's standard material.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct Look {
    #[uniform(100)]
    pub params: LookParams,
}

impl MaterialExtension for Look {
    fn fragment_shader() -> ShaderRef {
        LOOK_SHADER.into()
    }
}

/// Every scene object's material.
pub type LoomMaterial = ExtendedMaterial<StandardMaterial, Look>;

pub(crate) struct LookPlugin;

impl Plugin for LookPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, LOOK_SHADER, "look.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<LoomMaterial>::default());
    }
}

fn v4(c: glam::Vec3, w: f32) -> Vec4 {
    Vec4::new(c.x, c.y, c.z, w)
}

/// Linear colour of an sRGB 0–1 colour.
fn linear(c: glam::Vec3) -> glam::Vec3 {
    let f = |x: f32| if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
    glam::Vec3::new(f(c.x), f(c.y), f(c.z))
}

/// The shader values a surface needs (all off for a plain one).
pub fn params(surface: Option<&Surface>) -> LookParams {
    let mut p = LookParams::default();
    let Some(s) = surface else { return p };
    if let Some(ink) = &s.toon {
        p.flags |= TOON;
        p.levels = ink.paint_levels.max(1);
        let shaded = if ink.shaded_on { ink.lighted * (ink.shaded_percent / 100.0) } else { ink.shaded };
        p.lighted = v4(linear(if ink.lighted_on { ink.lighted } else { glam::Vec3::ZERO }), 1.0);
        p.shaded = v4(linear(if ink.lighted_on { shaded } else { glam::Vec3::ZERO }), 1.0);
        p.highlight = v4(linear(ink.highlight), if ink.highlight_on { 1.0 } else { 0.0 });
        let width = if ink.variable_width { (ink.ink_min + ink.ink_max) * 0.5 } else { ink.ink_width };
        p.ink = v4(linear(ink.ink_color), if ink.ink && ink.outline { width } else { 0.0 });
    }
    if let Some(m) = &s.matte {
        p.flags |= MATTE;
        p.matte = v4(linear(m.color), m.shadow_brightness);
        p.matte_flags = Vec4::new(if m.receive_shadows { 1.0 } else { 0.0 }, if m.opaque_alpha { 1.0 } else { 0.0 }, 0.0, 0.0);
    }
    if let Some(f) = &s.falloff {
        p.flags |= FALLOFF;
        p.falloff_type = f.kind as u32;
        p.side = v4(linear(f.side), 1.0);
        match f.direction {
            Some(d) => {
                p.falloff_dir = 1;
                p.dir = v4(d, 0.0);
            }
            None => p.falloff_dir = 0,
        }
        p.near_far = Vec4::new(f.near, f.far, f.ior, 0.0);
    }
    p
}

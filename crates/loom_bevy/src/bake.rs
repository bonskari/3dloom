//! Render To Texture on the GPU: an object is drawn in its texture space
//! (`uv_bake.wgsl`) with Bevy's PBR shading, so each texel is lit by the
//! scene lights and shadowed by the scene's objects exactly where that
//! point of the surface is. The lit elements (Complete, Diffuse, Specular,
//! Lighting, Shadows) are this render with the material changed to show
//! only their part.

use bevy::app::{App, Plugin};
use bevy::asset::{load_internal_asset, weak_handle, Asset, Assets, Handle};
use bevy::color::{Color, LinearRgba};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::entity::Entity;
use bevy::math::{Mat4, Vec3};
use bevy::pbr::{AmbientLight, ClusterConfig, ExtendedMaterial, MaterialExtension, MaterialPlugin, MeshMaterial3d, NotShadowCaster, NotShadowReceiver, StandardMaterial};
use bevy::reflect::Reflect;
use bevy::render::camera::{OrthographicProjection, Projection, ScalingMode};
use bevy::render::mesh::{Mesh, Mesh3d};
use bevy::render::render_resource::{AsBindGroup, Shader, ShaderRef};
use bevy::render::view::{NoFrustumCulling, RenderLayers};
use bevy::transform::components::Transform;
use loom_scene::material::{Surface, Surfaces};

use crate::{mesh_data, mesh_data_by_id, BevyRenderer, LIGHT_LAYER};

/// The object being baked is on this layer only: just the bake camera sees it.
pub(crate) const BAKE_LAYER: usize = 1022;
/// The default key light is on this layer too, so a bake with direct light sees it.
pub(crate) const BAKE_KEY_LAYER: usize = 1021;
/// Slot of the bake camera.
const BAKE_SLOT: u32 = u32::MAX - 1;
/// Frames a bake renders before it is read (new pipelines, the target image and the shadow maps settle).
const BAKE_FRAMES: usize = 4;

const UV_BAKE_SHADER: Handle<Shader> = weak_handle!("3a7e1c52-90d4-4b8e-a6f1-5c2d8e0b4f17");

/// The texture-space vertex shader over the standard material.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct UvBake {}

impl MaterialExtension for UvBake {
    fn vertex_shader() -> ShaderRef {
        UV_BAKE_SHADER.into()
    }
}

type BakeMaterial = ExtendedMaterial<StandardMaterial, UvBake>;

pub(crate) struct BakePlugin;

impl Plugin for BakePlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, UV_BAKE_SHADER, "uv_bake.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<BakeMaterial>::default());
    }
}

/// What a lit element records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BakeLook {
    /// The surface as renders show it.
    Complete,
    /// The diffuse colour (lit unless `lighting` is off).
    Diffuse { lighting: bool },
    /// The specular colour (its highlights unless `lighting` is off).
    Specular { lighting: bool },
    /// The light reaching the surface (a white material).
    Lighting,
    /// The shadows on the surface: white where lit, black in shadow.
    Shadows,
}

/// One lit element to bake.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakeOptions {
    pub look: BakeLook,
    /// Shadows component: shadows fall on the surface.
    pub shadows: bool,
    /// Lighting element: Direct Light On / Indirect Light On (ambient).
    pub direct: bool,
    pub indirect: bool,
    pub size: [u32; 2],
    /// Element Background, sRGB 0–1 (its alpha is 0).
    pub background: [f32; 3],
}

/// The object to bake: its world-space mesh, the map channel to bake into
/// and its material (Render To Texture bakes the original of a Shell).
pub struct BakeObject<'a> {
    pub mesh: &'a loom_mesh::Mesh,
    pub channel: u32,
    pub surfaces: Option<Surfaces>,
    pub wire_color: [u8; 3],
}

fn srgb_to_linear(c: u8) -> f32 {
    let x = c as f32 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(x: f32) -> u8 {
    let x = x.clamp(0.0, 1.0);
    let s = if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

impl BevyRenderer {
    /// Bakes `object` into a `options.size` map (sRGB RGBA; texels off the
    /// mesh have the background colour and alpha 0). Blocks for a few frames.
    pub fn bake(&mut self, object: &BakeObject, options: &BakeOptions) -> Result<image::RgbaImage, String> {
        if options.size[0] == 0 || options.size[1] == 0 {
            return Err(format!("map size {}x{}", options.size[0], options.size[1]));
        }
        if object.mesh.map(object.channel).is_none() {
            return Err(format!("no map channel {} to bake into", object.channel));
        }
        if options.look != BakeLook::Shadows {
            return self.bake_pass(object, options.look, options, true);
        }
        // Shadows: the direct light with shadows over the same light without them.
        let pass = BakeOptions { direct: true, indirect: false, ..*options };
        let shadowed = self.bake_pass(object, BakeLook::Lighting, &BakeOptions { shadows: true, ..pass }, false)?;
        let open = self.bake_pass(object, BakeLook::Lighting, &BakeOptions { shadows: false, ..pass }, false)?;
        let mut out = shadowed.clone();
        for (o, (s, u)) in out.pixels_mut().zip(shadowed.pixels().zip(open.pixels())) {
            if s.0[3] == 0 {
                continue;
            }
            let (ls, lu) = (srgb_to_linear(s.0[0].max(s.0[1]).max(s.0[2])), srgb_to_linear(u.0[0].max(u.0[1]).max(u.0[2])));
            // Unlit texels have no shadow to show.
            let v = if lu > 1e-4 { (ls / lu).min(1.0) } else { 1.0 };
            let g = linear_to_srgb(v);
            o.0 = [g, g, g, 255];
        }
        Ok(out)
    }

    /// The material a look draws with, from the object's surface.
    fn bake_material(&mut self, surface: Option<&Surface>, wire_color: [u8; 3], look: BakeLook) -> StandardMaterial {
        let m = self.material(surface, wire_color, 1.0);
        let mut m = match look {
            BakeLook::Complete => m,
            BakeLook::Lighting => StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 1.0, metallic: 0.0, reflectance: 0.0, normal_map_texture: m.normal_map_texture.clone(), ..Default::default() },
            BakeLook::Diffuse { lighting } => StandardMaterial { reflectance: 0.0, specular_texture: None, emissive: LinearRgba::BLACK, emissive_texture: None, unlit: !lighting, ..m },
            BakeLook::Specular { lighting: true } => StandardMaterial { base_color: Color::BLACK, base_color_texture: None, emissive: LinearRgba::BLACK, emissive_texture: None, ..m },
            BakeLook::Specular { lighting: false } => {
                let s = surface.map_or(Vec3::ZERO, |s| s.specular);
                StandardMaterial { base_color: Color::srgb(s.x.clamp(0.0, 1.0), s.y.clamp(0.0, 1.0), s.z.clamp(0.0, 1.0)), unlit: true, ..Default::default() }
            }
            BakeLook::Shadows => StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 1.0, reflectance: 0.0, ..Default::default() },
        };
        // UV islands can wind either way; normals are not flipped for back faces.
        m.cull_mode = None;
        m.double_sided = false;
        m
    }

    fn bake_pass(&mut self, object: &BakeObject, look: BakeLook, options: &BakeOptions, tonemap: bool) -> Result<image::RgbaImage, String> {
        let mut mesh = object.mesh.clone();
        if object.channel != 1 {
            let map = mesh.map(object.channel).cloned().ok_or("no map channel to bake into")?;
            mesh.set_map(1, map);
        }
        let parts: Vec<(Option<u16>, Option<Surface>)> = match &object.surfaces {
            Some(Surfaces::PerId(_)) => mesh_data_by_id(&mesh).into_iter().map(|(id, _)| (Some(id), object.surfaces.as_ref().map(|s| s.for_face(id).into_owned()))).collect(),
            Some(Surfaces::Single(s)) => vec![(None, Some((**s).clone()))],
            None => vec![(None, None)],
        };
        let mut entities: Vec<Entity> = Vec::new();
        for (id, surface) in parts {
            let mat = self.bake_material(surface.as_ref(), object.wire_color, look);
            let mut part = mesh.clone();
            let faceted = surface.as_ref().is_some_and(|s| s.faceted);
            for f in part.faces_mut() {
                f.hidden |= id.is_some_and(|id| f.material_id != id);
                if faceted {
                    f.smoothing_groups = 0;
                }
            }
            let mut m = mesh_data(&part).to_bevy();
            if mat.normal_map_texture.is_some() {
                let _ = m.generate_tangents();
            }
            let world = self.app.world_mut();
            let m = world.resource_mut::<Assets<Mesh>>().add(m);
            let mat = world.resource_mut::<Assets<BakeMaterial>>().add(BakeMaterial { base: mat, extension: UvBake {} });
            let mut e = world.spawn((Transform::IDENTITY, Mesh3d(m), MeshMaterial3d(mat), RenderLayers::layer(BAKE_LAYER), NotShadowCaster, NoFrustumCulling));
            if !options.shadows {
                e.insert(NotShadowReceiver);
            }
            entities.push(e.id());
        }
        // An orthographic camera from above that holds the whole scene, so
        // every light that reaches it is in its (single) light cluster.
        let (lo, hi) = object.mesh.aabb();
        let (mut centre, mut radius) = (Vec3::from((lo + hi) * 0.5), ((hi - lo) * 0.5).length().max(1e-3));
        if let Some((c, r)) = self.bounds {
            let d = (c - centre).length();
            if d + r > radius {
                let new_r = (radius + d + r) * 0.5;
                if d > 1e-6 {
                    centre += (c - centre) * ((new_r - radius) / d);
                }
                radius = new_r;
            }
        }
        let projection = Projection::Orthographic(OrthographicProjection { near: 0.0, far: radius * 4.0, scaling_mode: ScalingMode::Fixed { width: radius * 2.0, height: radius * 2.0 }, ..OrthographicProjection::default_3d() });
        let view = Mat4::look_at_rh(centre + Vec3::Z * radius * 2.0, centre, Vec3::Y);
        // Only the bake renders in these frames.
        for v in self.views.values_mut() {
            v.used = false;
        }
        let bg = options.background;
        self.place_view(BAKE_SLOT, options.size, projection, view, [bg[0], bg[1], bg[2], 0.0], false, RenderLayers::layer(BAKE_LAYER));
        let camera = self.views[&BAKE_SLOT].camera;
        {
            let mut e = self.app.world_mut().entity_mut(camera);
            let layers = if options.direct { RenderLayers::from_layers(&[BAKE_LAYER, BAKE_KEY_LAYER, LIGHT_LAYER]) } else { RenderLayers::layer(BAKE_LAYER) };
            e.insert((layers, ClusterConfig::Single, if tonemap { Tonemapping::AcesFitted } else { Tonemapping::None }));
            if options.indirect {
                e.remove::<AmbientLight>();
            } else {
                e.insert(AmbientLight { color: Color::BLACK, brightness: 0.0, affects_lightmapped_meshes: true });
            }
        }
        for _ in 0..BAKE_FRAMES {
            if let Some(v) = self.views.get_mut(&BAKE_SLOT) {
                v.used = true;
            }
            self.update();
        }
        let result = self.read_back(BAKE_SLOT);
        for e in entities {
            self.app.world_mut().despawn(e);
        }
        if let Some(v) = self.views.remove(&BAKE_SLOT) {
            self.app.world_mut().despawn(v.camera);
        }
        self.settle = self.settle.max(3);
        result
    }
}

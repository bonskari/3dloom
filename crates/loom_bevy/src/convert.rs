//! Pure conversions from loom data to Bevy: meshes, materials, cameras.
//! No GPU needed, so all of it is unit tested.

use bevy::asset::RenderAssetUsages;
use bevy::color::Color;
use bevy::math::{Mat4, Vec3};
use bevy::pbr::StandardMaterial;
use bevy::render::alpha::AlphaMode;
use bevy::render::camera::{OrthographicProjection, PerspectiveProjection, Projection, ScalingMode};
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::render::primitives::Aabb;
use loom_scene::maps::Image as MapImage;
use loom_scene::material::{Map, Shader, StandardMaterial as LoomMaterial, Surface};

/// A loom mesh triangulated for Bevy: one vertex per face corner.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Map channel 1, V flipped (Bevy's texture origin is the top left).
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// Triangulates `mesh` as the shaded viewport does (fan per face, smoothing
/// groups and explicit normals, hidden faces left out).
pub fn mesh_data(mesh: &loom_mesh::Mesh) -> MeshData {
    let (verts, indices) = loom_render::triangulate_with_face_normals(mesh, [1.0; 4], false);
    MeshData {
        positions: verts.iter().map(|v| v.position).collect(),
        normals: verts.iter().map(|v| v.normal).collect(),
        uvs: verts.iter().map(|v| [v.uv[0], 1.0 - v.uv[1]]).collect(),
        indices,
    }
}

impl MeshData {
    /// Content hash: a node whose data hashes the same keeps its GPU mesh.
    pub fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for list in [&self.positions, &self.normals] {
            list.len().hash(&mut h);
            for p in list.iter() {
                for c in p {
                    c.to_bits().hash(&mut h);
                }
            }
        }
        for uv in &self.uvs {
            uv[0].to_bits().hash(&mut h);
            uv[1].to_bits().hash(&mut h);
        }
        self.indices.hash(&mut h);
        h.finish()
    }

    /// World-space bounds (for culling); None for an empty mesh.
    pub fn aabb(&self) -> Option<Aabb> {
        let mut it = self.positions.iter().map(|p| Vec3::from_array(*p));
        let first = it.next()?;
        let (min, max) = it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p)));
        Some(Aabb::from_min_max(min, max))
    }

    /// The same triangles facing the other way (a Double Sided material's back).
    pub fn flipped(&self) -> Self {
        let mut out = self.clone();
        for n in out.normals.iter_mut() {
            *n = [-n[0], -n[1], -n[2]];
        }
        for t in out.indices.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
        out
    }

    pub fn to_bevy(&self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs.clone())
            .with_inserted_indices(Indices::U32(self.indices.clone()))
    }
}

/// `mesh` split by face material ID (0-based): one triangulation per ID in use.
pub fn mesh_data_by_id(mesh: &loom_mesh::Mesh) -> Vec<(u16, MeshData)> {
    let ids: std::collections::BTreeSet<u16> = mesh.faces().iter().filter(|f| !f.hidden).map(|f| f.material_id).collect();
    ids.into_iter()
        .map(|id| {
            let mut part = mesh.clone();
            for f in part.faces_mut() {
                f.hidden |= f.material_id != id;
            }
            (id, mesh_data(&part))
        })
        .collect()
}

/// A hash of the faces' material IDs (the per-ID split depends on them).
pub fn material_ids_hash(mesh: &loom_mesh::Mesh) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in mesh.faces() {
        f.material_id.hash(&mut h);
    }
    h.finish()
}

/// The map's Offset / Tiling / Angle as Bevy's `uv_transform` (Bevy's V runs
/// down, the map's up; textures are stored from the top row).
pub fn uv_transform(map: &Map) -> bevy::math::Affine2 {
    let m = map.coords.matrix();
    let f = |p: [f32; 2]| [p[0], 1.0 - p[1]];
    let a = |p: [f32; 2]| {
        let q = f(p);
        f([m[0] * q[0] + m[1] * q[1] + m[2], m[3] * q[0] + m[4] * q[1] + m[5]])
    };
    let (o, x, y) = (a([0.0, 0.0]), a([1.0, 0.0]), a([0.0, 1.0]));
    bevy::math::Affine2::from_cols(bevy::math::Vec2::new(x[0] - o[0], x[1] - o[1]), bevy::math::Vec2::new(y[0] - o[0], y[1] - o[1]), bevy::math::Vec2::new(o[0], o[1]))
}

/// The base colour texture: the diffuse map over the Diffuse colour by its
/// Amount (a decal leaves the colour around it: the edge texels carry it, the
/// sampler clamps), alpha from Opacity and the Opacity map. 8-bit sRGB.
pub fn compose_base(diffuse: glam::Vec3, map: Option<(&MapImage, &Map)>, opacity: f32, opacity_map: Option<(&MapImage, &Map)>) -> MapImage {
    let (w, h) = map.map(|(i, _)| (i.width, i.height)).or(opacity_map.map(|(i, _)| (i.width, i.height))).unwrap_or((1, 1));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    let decal = map.map(|(_, m)| m.coords.decal()).unwrap_or([false; 2]);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let mut c = [diffuse.x, diffuse.y, diffuse.z];
            let edge = (decal[0] && (x == 0 || x + 1 == w)) || (decal[1] && (y == 0 || y + 1 == h));
            if let Some((img, m)) = map.filter(|_| !edge) {
                let t = img.sample(u, v);
                let a = t[3] * (m.amount / 100.0).clamp(0.0, 1.0);
                for k in 0..3 {
                    c[k] += (t[k] - c[k]) * a;
                }
            }
            let alpha = match opacity_map {
                Some((img, m)) => {
                    let t = img.sample(u, v);
                    let lum = (t[0] + t[1] + t[2]) / 3.0 * t[3];
                    opacity + (lum - opacity) * (m.amount / 100.0).clamp(0.0, 1.0)
                }
                None => opacity,
            };
            rgba.extend([c[0], c[1], c[2], alpha].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    MapImage { width: w, height: h, rgba }
}

/// A Normal Bump map's normal image as Bevy reads normal maps (green
/// toward the top of the image): the reference's tangent normal maps point
/// green down, so green is inverted unless Flip Green; Flip Red inverts red.
pub fn normal_bump(img: &MapImage, flip_red: bool, flip_green: bool) -> MapImage {
    let mut out = img.clone();
    for px in out.rgba.chunks_exact_mut(4) {
        if flip_red {
            px[0] = 255 - px[0];
        }
        if !flip_green {
            px[1] = 255 - px[1];
        }
        px[3] = 255;
    }
    out
}

/// A grey (luminance) texture scaled by `amount` 0–1, alpha = the same value:
/// Specular Level and Self-Illumination maps. 8-bit, linear.
pub fn luminance(img: &MapImage, amount: f32) -> MapImage {
    let rgba = img
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let l = ((p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0 * amount).round().clamp(0.0, 255.0) as u8;
            [l, l, l, l]
        })
        .collect();
    MapImage { width: img.width, height: img.height, rgba }
}

/// The diffuse bitmap a material shows, if any (an enabled Diffuse Color map).
pub fn diffuse_map_path(material: Option<&LoomMaterial>) -> Option<&str> {
    material.and_then(|m| m.diffuse_map.as_ref()).filter(|b| b.enabled && !b.path.is_empty()).map(|b| b.path.as_str())
}

/// A loom Standard material as a Bevy PBR material (texture left unset):
/// Diffuse is the base colour, Glossiness 0–100 the inverse roughness,
/// Specular Level the reflectance (100 = Bevy's default dielectric 0.5),
/// Opacity below 100 blends, 2-Sided draws both sides. Without a material
/// the object's wire colour is the base colour.
pub fn material_from_loom(material: Option<&LoomMaterial>, wire_color: [u8; 3]) -> StandardMaterial {
    material_from_surface(material.map(Surface::from_standard).as_ref(), wire_color)
}

/// As `material_from_loom` for a resolved surface (a Multi/Sub-Object
/// sub-material, a Blend). Self-Illumination shows the Diffuse colour unlit:
/// that share moves from the lit base colour to the emissive colour.
pub fn material_from_surface(surface: Option<&Surface>, wire_color: [u8; 3]) -> StandardMaterial {
    match surface {
        None => StandardMaterial {
            base_color: Color::srgb_u8(wire_color[0], wire_color[1], wire_color[2]),
            perceptual_roughness: 0.6,
            reflectance: 0.3,
            ..Default::default()
        },
        Some(m) => {
            let sh = &m.shading;
            let mut d = m.diffuse;
            if sh.shader.has_diffuse_level() {
                d *= sh.diffuse_level / 100.0;
            }
            if sh.shader.has_roughness() {
                // PBR has no diffuse roughness: Oren-Nayar's darkening of facing surfaces.
                d *= loom_scene::material::oren_nayar_dim(sh.roughness);
            }
            let alpha = m.opacity;
            let s = m.self_illum;
            let lit = d * (1.0 - s);
            let glow = Color::srgb(d.x, d.y, d.z).to_linear() * s + Color::srgb(m.self_illum_color.x, m.self_illum_color.y, m.self_illum_color.z).to_linear();
            let two_sided = m.two_sided || sh.shader == Shader::Translucent;
            let mut out = StandardMaterial {
                base_color: Color::srgba(lit.x, lit.y, lit.z, alpha),
                emissive: bevy::color::LinearRgba::rgb(glow.red, glow.green, glow.blue),
                perceptual_roughness: (1.0 - m.glossiness / 100.0).clamp(0.0, 1.0),
                reflectance: (m.specular_level / 100.0 * 0.5).clamp(0.0, 1.0),
                // Advanced Transparency Type: Filter and Subtractive blend, Additive adds.
                alpha_mode: match (alpha < 1.0, m.extended.transparency_type) {
                    (false, _) => AlphaMode::Opaque,
                    (true, 2) => AlphaMode::Add,
                    (true, _) => AlphaMode::Blend,
                },
                ior: m.extended.ior.max(1.0),
                double_sided: two_sided,
                cull_mode: if two_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
                ..Default::default()
            };
            match sh.shader {
                Shader::Blinn | Shader::OrenNayarBlinn => {}
                // Phong's wider highlight at the same Glossiness.
                Shader::Phong => out.perceptual_roughness = (out.perceptual_roughness * 1.25).min(1.0),
                Shader::Metal => out.metallic = 1.0,
                Shader::Strauss => {
                    out.metallic = (sh.metalness / 100.0).clamp(0.0, 1.0);
                    out.reflectance = 0.5;
                }
                Shader::Anisotropic => {
                    out.anisotropy_strength = (sh.anisotropy / 100.0).clamp(0.0, 1.0);
                    out.anisotropy_rotation = sh.orientation.to_radians();
                }
                Shader::MultiLayer => {
                    // The first layer anisotropic, the second a clear coat over it.
                    out.anisotropy_strength = (sh.anisotropy / 100.0).clamp(0.0, 1.0);
                    out.anisotropy_rotation = sh.orientation.to_radians();
                    out.clearcoat = (sh.level2 / 100.0).clamp(0.0, 1.0);
                    out.clearcoat_perceptual_roughness = (1.0 - sh.glossiness2 / 100.0).clamp(0.0, 1.0);
                }
                Shader::Translucent => {
                    // Light scattered through: the Translucent colour times the Filter colour.
                    let t = sh.translucent_color * m.extended.filter_color;
                    out.diffuse_transmission = ((t.x + t.y + t.z) / 3.0).clamp(0.0, 1.0);
                    out.thickness = 0.0;
                }
            }
            apply_surface_hints(&mut out, m, lit);
            out
        }
    }
}

/// The other material types' values on the PBR material: metalness, the
/// Raytrace Reflect colour, refraction (transmission), translucency, a
/// Shellac clear coat, Raw Diffuse Texture, Ink 'n Paint and Matte/Shadow.
fn apply_surface_hints(out: &mut StandardMaterial, m: &Surface, lit: glam::Vec3) {
    let alpha = out.base_color.alpha();
    let mut base = lit;
    out.metallic = out.metallic.max(m.metallic.clamp(0.0, 1.0));
    let reflect = m.reflect.max_element().clamp(0.0, 1.0);
    if reflect > 0.0 {
        if m.fresnel {
            // Fresnel reflections are a dielectric's: brighter at grazing angles, not a metal.
            out.reflectance = out.reflectance.max(reflect);
        } else {
            // Reflect over a dark diffuse is coloured chrome.
            out.metallic = out.metallic.max(reflect);
            base = base.lerp(m.reflect / reflect, reflect);
        }
    }
    if m.transmission > 0.0 {
        // The renderer refracts what is behind (tinted by the filter colour)
        // instead of blending it as the viewports do.
        let t = m.transmission.clamp(0.0, 1.0);
        out.specular_transmission = t;
        out.thickness = 0.0;
        base = base.lerp(m.extended.filter_color, t);
        let a = m.opacity;
        out.base_color = Color::srgba(base.x, base.y, base.z, a);
        out.alpha_mode = if a < 1.0 { AlphaMode::Blend } else { AlphaMode::Opaque };
    } else {
        out.base_color = Color::srgba(base.x, base.y, base.z, alpha);
    }
    if m.translucency > 0.0 {
        out.diffuse_transmission = out.diffuse_transmission.max(m.translucency.clamp(0.0, 1.0));
        out.double_sided = true;
        out.cull_mode = None;
    }
    if m.coat > 0.0 {
        out.clearcoat = out.clearcoat.max(m.coat);
        out.clearcoat_perceptual_roughness = (1.0 - m.coat_glossiness / 100.0).clamp(0.0, 1.0);
    }
    if m.unlit {
        out.unlit = true;
    }
    if m.toon.is_some() {
        // Ink 'n Paint: a flat, matte fill (the paint bands and the ink need a toon shader).
        out.perceptual_roughness = 1.0;
        out.reflectance = 0.0;
    }
    if let Some(ms) = &m.matte {
        // Matte/Shadow: the background shows through (multiplied by white).
        let _ = ms;
        out.base_color = Color::WHITE;
        out.unlit = true;
        out.alpha_mode = AlphaMode::Multiply;
    }
}

/// A loom viewport camera: its world-to-view and projection matrices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewCamera {
    pub view: Mat4,
    pub projection: Mat4,
}

impl ViewCamera {
    /// Camera world position.
    pub fn eye(&self) -> Vec3 {
        self.view.inverse().w_axis.truncate()
    }

    pub fn is_orthographic(&self) -> bool {
        self.projection.w_axis.w == 1.0
    }

    /// Bevy projection with the same field of view / extent and clip
    /// planes as loom's right-handed, 0–1 depth matrix.
    pub fn bevy_projection(&self, aspect: f32) -> Projection {
        let p = self.projection;
        let near = p.w_axis.z / p.z_axis.z;
        if self.is_orthographic() {
            let far = near - 1.0 / p.z_axis.z;
            Projection::Orthographic(OrthographicProjection {
                near,
                far,
                scaling_mode: ScalingMode::Fixed { width: 2.0 / p.x_axis.x, height: 2.0 / p.y_axis.y },
                ..OrthographicProjection::default_3d()
            })
        } else {
            let r = p.z_axis.z;
            let far = r * near / (1.0 + r);
            Projection::Perspective(PerspectiveProjection { fov: 2.0 * (1.0 / p.y_axis.y).atan(), aspect_ratio: aspect, near, far })
        }
    }
}

/// Clip-space matrix taking the part `sub` = [u0, v0, u1, v1] of the frame
/// (fractions, v down) to the whole of normalised device coordinates.
pub fn crop_ndc(sub: [f32; 4]) -> Mat4 {
    let [u0, v0, u1, v1] = sub;
    let (sx, sy) = (1.0 / (u1 - u0), 1.0 / (v1 - v0));
    let (cx, cy) = (u0 + u1 - 1.0, 1.0 - v0 - v1);
    Mat4::from_cols(
        bevy::math::Vec4::new(sx, 0.0, 0.0, 0.0),
        bevy::math::Vec4::new(0.0, sy, 0.0, 0.0),
        bevy::math::Vec4::new(0.0, 0.0, 1.0, 0.0),
        bevy::math::Vec4::new(-sx * cx, -sy * cy, 0.0, 1.0),
    )
}

/// The projection of one render bucket: a part of the full frame's
/// projection. Bevy never resizes it to the bucket's image, and the shadow
/// cascades are fitted to the full frame, so the buckets join seamlessly.
#[derive(Debug, Clone)]
pub struct BucketProjection {
    pub full: Projection,
    pub crop: Mat4,
}

impl BucketProjection {
    /// `camera` at image aspect `aspect`, the part `sub` of the frame.
    pub fn new(camera: &ViewCamera, aspect: f32, sub: [f32; 4]) -> Self {
        use bevy::render::camera::CameraProjection;
        let mut full = camera.bevy_projection(aspect);
        full.update(aspect, 1.0);
        Self { full, crop: crop_ndc(sub) }
    }
}

impl bevy::render::camera::CameraProjection for BucketProjection {
    fn get_clip_from_view(&self) -> Mat4 {
        self.crop * self.full.get_clip_from_view()
    }
    fn get_clip_from_view_for_sub(&self, _sub_view: &bevy::render::camera::SubCameraView) -> Mat4 {
        self.get_clip_from_view()
    }
    fn update(&mut self, _width: f32, _height: f32) {}
    fn far(&self) -> f32 {
        self.full.far()
    }
    fn get_frustum_corners(&self, z_near: f32, z_far: f32) -> [bevy::math::Vec3A; 8] {
        self.full.get_frustum_corners(z_near, z_far)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_normal_bump_map_turns_green_up_for_bevy() {
        let img = MapImage { width: 1, height: 1, rgba: vec![200, 60, 255, 10] };
        assert_eq!(super::normal_bump(&img, false, false).rgba, vec![200, 195, 255, 255]);
        assert_eq!(super::normal_bump(&img, true, true).rgba, vec![55, 60, 255, 255]);
    }

    use super::*;
    use bevy::color::Alpha;
    use loom_scene::material::Map as BitmapMap;

    #[test]
    fn a_box_becomes_twelve_triangles_with_one_vertex_per_corner() {
        let mesh = loom_mesh::box_primitive(Vec3::splat(1.0));
        let d = mesh_data(&mesh);
        assert_eq!(d.positions.len(), 24);
        assert_eq!(d.normals.len(), 24);
        assert_eq!(d.uvs.len(), 24);
        assert_eq!(d.indices.len(), 36);
        // Flat box faces: each normal is an axis.
        for n in &d.normals {
            let v = Vec3::from_array(*n);
            assert!((v.length() - 1.0).abs() < 1e-5 && v.abs().max_element() > 0.999, "{v}");
        }
        let aabb = d.aabb().unwrap();
        assert!((Vec3::from(aabb.half_extents) - Vec3::ONE).abs().max_element() < 1e-5);
        let m = d.to_bevy();
        assert_eq!(m.count_vertices(), 24);
        assert_eq!(m.indices().map(|i| i.len()), Some(36));
    }

    #[test]
    fn uvs_flip_v_and_hidden_faces_are_left_out() {
        let mut mesh = loom_mesh::box_primitive(Vec3::splat(1.0));
        let a = mesh_data(&mesh);
        let (verts, _) = loom_render::triangulate_with_face_normals(&mesh, [1.0; 4], false);
        for (uv, v) in a.uvs.iter().zip(&verts) {
            assert_eq!(*uv, [v.uv[0], 1.0 - v.uv[1]]);
        }
        mesh.faces_mut()[0].hidden = true;
        let b = mesh_data(&mesh);
        assert_eq!(b.indices.len(), 30);
        assert_ne!(a.content_hash(), b.content_hash());
        assert_eq!(a.content_hash(), mesh_data(&loom_mesh::box_primitive(Vec3::splat(1.0))).content_hash());
    }

    #[test]
    fn no_material_uses_the_wire_colour() {
        let m = material_from_loom(None, [255, 0, 0]);
        assert_eq!(m.base_color, Color::srgb_u8(255, 0, 0));
        assert_eq!(m.alpha_mode, AlphaMode::Opaque);
        assert!(m.base_color_texture.is_none());
    }

    #[test]
    fn standard_material_maps_to_pbr() {
        let mut lm = LoomMaterial::new("m");
        lm.diffuse.set(Vec3::new(0.8, 0.2, 0.1));
        lm.glossiness.set(70.0);
        lm.specular_level.set(100.0);
        let m = material_from_loom(Some(&lm), [0, 0, 0]);
        assert_eq!(m.base_color, Color::srgba(0.8, 0.2, 0.1, 1.0));
        assert!((m.perceptual_roughness - 0.3).abs() < 1e-6);
        assert!((m.reflectance - 0.5).abs() < 1e-6);
        assert_eq!(m.alpha_mode, AlphaMode::Opaque);
        assert!(!m.double_sided);
        assert_eq!(m.cull_mode, Some(bevy::render::render_resource::Face::Back));

        // Out-of-range values clamp; opacity below 100 blends; 2-Sided draws both sides.
        lm.glossiness.set(150.0);
        lm.specular_level.set(999.0);
        lm.opacity.set(40.0);
        lm.two_sided = true;
        let m = material_from_loom(Some(&lm), [0, 0, 0]);
        assert_eq!(m.perceptual_roughness, 0.0);
        assert_eq!(m.reflectance, 1.0);
        assert_eq!(m.alpha_mode, AlphaMode::Blend);
        assert!((m.base_color.alpha() - 0.4).abs() < 1e-6);
        assert!(m.double_sided && m.cull_mode.is_none());
        // Specular Level 0: no specular reflection.
        lm.specular_level.set(0.0);
        assert_eq!(material_from_loom(Some(&lm), [0, 0, 0]).reflectance, 0.0);
    }

    #[test]
    fn each_shader_maps_to_its_pbr_fields() {
        let mut lm = LoomMaterial::new("m");
        lm.glossiness.set(60.0);
        let pbr = |lm: &LoomMaterial| material_from_loom(Some(lm), [0, 0, 0]);
        assert_eq!(pbr(&lm).metallic, 0.0);
        lm.shading.shader = Shader::Metal;
        assert_eq!(pbr(&lm).metallic, 1.0, "Metal is a metal");
        lm.shading.shader = Shader::Strauss;
        lm.shading.metalness = 40.0;
        assert!((pbr(&lm).metallic - 0.4).abs() < 1e-6, "Strauss Metalness");
        lm.shading.shader = Shader::Anisotropic;
        lm.shading.anisotropy = 80.0;
        lm.shading.orientation = 90.0;
        let a = pbr(&lm);
        assert!((a.anisotropy_strength - 0.8).abs() < 1e-6 && (a.anisotropy_rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        lm.shading.shader = Shader::MultiLayer;
        lm.shading.level2 = 50.0;
        lm.shading.glossiness2 = 80.0;
        let ml = pbr(&lm);
        assert!((ml.clearcoat - 0.5).abs() < 1e-6 && (ml.clearcoat_perceptual_roughness - 0.2).abs() < 1e-5, "the second layer is a clear coat");
        lm.shading.shader = Shader::OrenNayarBlinn;
        lm.diffuse.set(Vec3::splat(0.5));
        lm.shading.diffuse_level = 200.0;
        let onb = pbr(&lm);
        assert!((onb.base_color.to_srgba().red - 1.0).abs() < 1e-5, "Diffuse Level 200 doubles the colour");
        lm.shading.shader = Shader::Translucent;
        lm.shading.translucent_color = Vec3::ONE;
        lm.extended.filter_color = Vec3::ONE;
        let tr = pbr(&lm);
        assert!(tr.diffuse_transmission > 0.99 && tr.double_sided && tr.cull_mode.is_none(), "light passes through");
        // Extended Parameters: IOR and the Additive transparency type.
        lm.extended.ior = 2.4;
        lm.opacity.set(50.0);
        lm.extended.transparency_type = 2;
        let e = pbr(&lm);
        assert!((e.ior - 2.4).abs() < 1e-6);
        assert_eq!(e.alpha_mode, AlphaMode::Add);
        // Self-Illumination Color mode glows in its colour.
        lm.shading.shader = Shader::Blinn;
        lm.shading.self_illum_color_on = true;
        lm.shading.self_illum_color = Vec3::new(0.0, 0.0, 1.0);
        let g = pbr(&lm);
        assert!(g.emissive.blue > 0.99 && g.emissive.red == 0.0, "{:?}", g.emissive);
    }

    #[test]
    fn other_material_types_map_to_pbr() {
        use loom_scene::material::{MaterialKind, Surface};
        let pbr = |m: &LoomMaterial| material_from_surface(Some(&Surface::from_material(m)), [0, 0, 0]);
        // Raytrace: Reflect over black diffuse is coloured chrome; Transparency refracts.
        let mut rt = LoomMaterial::new("rt");
        rt.kind = MaterialKind::Raytrace(Box::default());
        rt.set_color("Diffuse", [0, 0, 0]).unwrap();
        rt.set_color("Reflect", [255, 0, 0]).unwrap();
        let chrome = pbr(&rt);
        assert_eq!(chrome.metallic, 1.0);
        assert!(chrome.base_color.to_srgba().red > 0.99 && chrome.base_color.to_srgba().green < 0.01);
        rt.set_color("Reflect", [0, 0, 0]).unwrap();
        rt.set_color("Transparency", [255, 255, 255]).unwrap();
        rt.set_parameter("Index of Refr.", 1.5).unwrap();
        let glass = pbr(&rt);
        assert!(glass.specular_transmission > 0.99 && (glass.ior - 1.5).abs() < 1e-6);
        assert_eq!(glass.alpha_mode, AlphaMode::Opaque, "refraction, not blending");
        assert!(glass.base_color.to_srgba().red > 0.99, "tinted by the (white) transparency colour");
        // Architectural Raw Diffuse Texture is unlit; Mirror is a metal.
        let mut arch = LoomMaterial::new("arch");
        arch.kind = MaterialKind::Architectural(Box::default());
        arch.set_parameter("Raw Diffuse Texture", 1.0).unwrap();
        assert!(pbr(&arch).unlit);
        arch.set_parameter("Template", 11.0).unwrap();
        assert_eq!(pbr(&arch).metallic, 1.0);
        // Matte/Shadow lets the background through.
        let mut matte = LoomMaterial::new("matte");
        matte.kind = MaterialKind::MatteShadow(Box::default());
        assert_eq!(pbr(&matte).alpha_mode, AlphaMode::Multiply);
    }

    #[test]
    fn diffuse_map_needs_an_enabled_bitmap() {
        let mut lm = LoomMaterial::new("m");
        assert_eq!(diffuse_map_path(Some(&lm)), None);
        lm.diffuse_map = Some(BitmapMap::bitmap("/x/wood.png", 100.0));
        assert_eq!(diffuse_map_path(Some(&lm)), Some("/x/wood.png"));
        lm.diffuse_map.as_mut().unwrap().enabled = false;
        assert_eq!(diffuse_map_path(Some(&lm)), None);
        assert_eq!(diffuse_map_path(None), None);
    }

    #[test]
    fn perspective_projection_matches_loom() {
        let fov = 45f32.to_radians();
        let cam = ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, -10.0, 5.0), Vec3::ZERO, Vec3::Z), projection: Mat4::perspective_rh(fov, 1.5, 0.1, 500.0) };
        assert!(!cam.is_orthographic());
        assert!((cam.eye() - Vec3::new(0.0, -10.0, 5.0)).length() < 1e-4);
        let Projection::Perspective(p) = cam.bevy_projection(1.5) else { panic!("perspective expected") };
        assert!((p.fov - fov).abs() < 1e-5);
        assert!((p.near - 0.1).abs() < 1e-5, "{}", p.near);
        assert!((p.far - 500.0).abs() < 0.5, "{}", p.far);
        assert_eq!(p.aspect_ratio, 1.5);
    }

    #[test]
    fn orthographic_projection_matches_loom() {
        let cam = ViewCamera { view: Mat4::IDENTITY, projection: Mat4::orthographic_rh(-30.0, 30.0, -20.0, 20.0, 1950.0, 2050.0) };
        assert!(cam.is_orthographic());
        let Projection::Orthographic(o) = cam.bevy_projection(1.5) else { panic!("orthographic expected") };
        assert!((o.near - 1950.0).abs() < 1e-2 && (o.far - 2050.0).abs() < 1e-2, "{} {}", o.near, o.far);
        match o.scaling_mode {
            ScalingMode::Fixed { width, height } => assert!((width - 60.0).abs() < 1e-3 && (height - 40.0).abs() < 1e-3),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn self_illumination_moves_to_emissive() {
        let mut lm = LoomMaterial::new("m");
        lm.diffuse.set(glam::Vec3::new(1.0, 0.5, 0.0));
        lm.self_illum.set(100.0);
        let m = material_from_loom(Some(&lm), [0, 0, 0]);
        assert_eq!(m.base_color.to_srgba().red, 0.0, "fully self-lit: nothing left to light");
        assert!((m.emissive.red - 1.0).abs() < 1e-5 && m.emissive.green > 0.1 && m.emissive.blue == 0.0, "{:?}", m.emissive);
        lm.self_illum.set(0.0);
        assert_eq!(material_from_loom(Some(&lm), [0, 0, 0]).emissive, bevy::color::LinearRgba::rgb(0.0, 0.0, 0.0));
    }

    #[test]
    fn faces_split_by_material_id() {
        let mut mesh = loom_mesh::box_primitive(Vec3::splat(1.0));
        for (i, f) in mesh.faces_mut().iter_mut().enumerate() {
            f.material_id = (i % 2) as u16;
        }
        let parts = mesh_data_by_id(&mesh);
        assert_eq!(parts.iter().map(|(id, d)| (*id, d.indices.len())).collect::<Vec<_>>(), vec![(0, 18), (1, 18)]);
        let before = material_ids_hash(&mesh);
        mesh.faces_mut()[0].material_id = 5;
        assert_ne!(material_ids_hash(&mesh), before);
    }

    #[test]
    fn diffuse_map_composes_over_the_colour_and_opacity_map_gives_alpha() {
        let img = MapImage { width: 2, height: 1, rgba: vec![255, 255, 255, 255, 255, 255, 255, 0] };
        let map = BitmapMap::bitmap("x", 50.0);
        let c = compose_base(glam::Vec3::new(0.0, 0.0, 1.0), Some((&img, &map)), 1.0, None);
        assert_eq!(&c.rgba[0..4], &[128, 128, 255, 255], "Amount 50 mixes half the map over blue");
        assert_eq!(&c.rgba[4..8], &[0, 0, 255, 255], "where the map is clear the colour shows");
        let mask = MapImage { width: 2, height: 1, rgba: vec![0, 0, 0, 255, 255, 255, 255, 255] };
        let om = BitmapMap::bitmap("o", 100.0);
        let c = compose_base(glam::Vec3::ONE, None, 1.0, Some((&mask, &om)));
        assert_eq!((c.rgba[3], c.rgba[7]), (0, 255));
    }

    #[test]
    fn uv_transform_matches_the_map_coordinates() {
        let mut map = BitmapMap::bitmap("x", 100.0);
        map.coords.tiling = [2.0, 3.0];
        map.coords.offset = [0.25, 0.0];
        let t = uv_transform(&map);
        // A loom UV (u, v) is (u, 1 - v) on the Bevy mesh; the texture row index runs down.
        for (u, v) in [(0.1f32, 0.2f32), (0.7, 0.9)] {
            let m = map.coords.apply([u, v]);
            let b = t.transform_point2(bevy::math::Vec2::new(u, 1.0 - v));
            assert!((b.x - m[0]).abs() < 1e-5 && (b.y - (1.0 - m[1])).abs() < 1e-5, "{b} vs {m:?}");
        }
    }

    #[test]
    fn a_bucket_projection_maps_its_part_of_the_frame_to_the_whole_image() {
        use bevy::render::camera::CameraProjection;
        let cam = ViewCamera { view: Mat4::IDENTITY, projection: Mat4::perspective_rh(50f32.to_radians(), 2.0, 0.1, 100.0) };
        let full = BucketProjection::new(&cam, 2.0, [0.0, 0.0, 1.0, 1.0]);
        let whole = cam.bevy_projection(2.0);
        assert!(full.get_clip_from_view().abs_diff_eq(whole.get_clip_from_view(), 1e-5), "the whole frame is the plain projection");
        // A view-space point and where it lands in the full frame.
        let p = bevy::math::Vec4::new(1.0, 0.6, -5.0, 1.0);
        let ndc = |m: Mat4| {
            let c = m * p;
            (c.x / c.w, c.y / c.w)
        };
        let (x, y) = ndc(full.get_clip_from_view());
        let (u, v) = ((x + 1.0) * 0.5, (1.0 - y) * 0.5);
        // The top-right quarter: the point's frame fraction maps into it.
        let q = BucketProjection::new(&cam, 2.0, [0.5, 0.0, 1.0, 0.5]);
        let (qx, qy) = ndc(q.get_clip_from_view());
        let (qu, qv) = ((qx + 1.0) * 0.5, (1.0 - qy) * 0.5);
        assert!((qu - (u - 0.5) * 2.0).abs() < 1e-4 && (qv - v * 2.0).abs() < 1e-4, "({u},{v}) -> ({qu},{qv})");
        // Resizing to the bucket's image does not change it; shadows see the full frame.
        let mut r = q.clone();
        r.update(64.0, 64.0);
        assert_eq!(r.get_clip_from_view(), q.get_clip_from_view());
        assert_eq!(q.get_frustum_corners(-1.0, -10.0), whole.get_frustum_corners(-1.0, -10.0));
    }
}

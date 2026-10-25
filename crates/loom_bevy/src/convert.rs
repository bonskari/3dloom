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
use loom_scene::material::StandardMaterial as LoomMaterial;

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

    pub fn to_bevy(&self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs.clone())
            .with_inserted_indices(Indices::U32(self.indices.clone()))
    }
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
    match material {
        None => StandardMaterial {
            base_color: Color::srgb_u8(wire_color[0], wire_color[1], wire_color[2]),
            perceptual_roughness: 0.6,
            reflectance: 0.3,
            ..Default::default()
        },
        Some(m) => {
            let d = m.diffuse.get();
            let alpha = (m.opacity.get() / 100.0).clamp(0.0, 1.0);
            StandardMaterial {
                base_color: Color::srgba(d.x, d.y, d.z, alpha),
                perceptual_roughness: (1.0 - m.glossiness.get() / 100.0).clamp(0.0, 1.0),
                reflectance: (m.specular_level.get() / 100.0 * 0.5).clamp(0.0, 1.0),
                alpha_mode: if alpha < 1.0 { AlphaMode::Blend } else { AlphaMode::Opaque },
                double_sided: m.two_sided,
                cull_mode: if m.two_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
                ..Default::default()
            }
        }
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
    use super::*;
    use bevy::color::Alpha;
    use loom_scene::material::BitmapMap;

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
    fn diffuse_map_needs_an_enabled_bitmap() {
        let mut lm = LoomMaterial::new("m");
        assert_eq!(diffuse_map_path(Some(&lm)), None);
        lm.diffuse_map = Some(BitmapMap { path: "/x/wood.png".into(), amount: 100.0, enabled: true });
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

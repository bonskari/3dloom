//! Unwired persistent posed-geometry transport for Bevy's StandardMaterial.
//!
//! CPU positions, smoothing normals and Mikk tangents are uploaded verbatim.
//! No skinning, normalization, tangent reconstruction or material replacement
//! occurs here. The host must keep topology fixed and insert the Aabb returned
//! by every update on the corresponding entity before rendering.

use bevy::app::{App, Plugin};
use bevy::asset::{load_internal_asset, weak_handle, Asset, Handle};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, StandardMaterial};
use bevy::reflect::TypePath;
use bevy::render::mesh::{Mesh, VertexAttributeValues};
use bevy::render::primitives::Aabb;
use bevy::render::render_resource::{AsBindGroup, Buffer, Shader, ShaderRef};
use wgpu::util::DeviceExt;

const FORWARD: Handle<Shader> = weak_handle!("c9a0d671-cc10-43ac-8d13-64af35dce100");
const PREPASS: Handle<Shader> = weak_handle!("c9a0d671-cc10-43ac-8d13-64af35dce101");

pub type DynamicMaterial = ExtendedMaterial<StandardMaterial, DynamicGeometryExtension>;

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct DynamicGeometryExtension {
    #[storage(100, read_only, buffer)]
    pub vertices: Buffer,
    #[storage(101, read_only, buffer)]
    pub previous_positions: Buffer,
}

impl MaterialExtension for DynamicGeometryExtension {
    fn vertex_shader() -> ShaderRef {
        FORWARD.into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        PREPASS.into()
    }
    fn deferred_vertex_shader() -> ShaderRef {
        PREPASS.into()
    }
}

/// Register before `App::finish`. All standard material fragment, alpha and
/// shadow behavior comes from Bevy's base StandardMaterial plugin.
pub struct DynamicGeometryPlugin;
impl Plugin for DynamicGeometryPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            FORWARD,
            "dynamic_geometry_forward.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            PREPASS,
            "dynamic_geometry_prepass.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<DynamicMaterial>::default());
    }
}

/// An immutable-size device allocation. Keep its extension handle unchanged
/// during animation; updating buffer contents needs no material asset event.
pub struct DynamicGeometryBuffer {
    buffer: Buffer,
    vertex_count: usize,
    has_tangents: bool,
    previous: Buffer,
    last_positions: Vec<u8>,
    indices: Option<Vec<u32>>,
}

impl DynamicGeometryBuffer {
    pub fn new(device: &wgpu::Device, mesh: &Mesh) -> Result<(Self, Aabb), String> {
        let (bytes, bounds, vertex_count, has_tangents) = pack(mesh)?;
        let limits = device.limits();
        if bytes.len() as u64 > limits.max_buffer_size
            || bytes.len() as u64 > u64::from(limits.max_storage_buffer_binding_size)
            || limits.max_storage_buffers_per_shader_stage < 2
        {
            return Err("Dynamic geometry exceeds device vertex storage limits".into());
        }
        let last_positions: Vec<u8> = bytes
            .chunks_exact(64)
            .flat_map(|v| v[..16].iter().copied())
            .collect();
        let previous = device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("previous posed positions"),
                contents: &last_positions,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            })
            .into();
        let buffer = device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("exact posed geometry"),
                contents: &bytes,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
            })
            .into();
        Ok((
            Self {
                buffer,
                vertex_count,
                has_tangents,
                previous,
                last_positions,
                indices: mesh.indices().map(|i| i.iter().map(|i| i as u32).collect()),
            },
            bounds,
        ))
    }

    pub fn extension(&self) -> DynamicGeometryExtension {
        DynamicGeometryExtension {
            vertices: self.buffer.clone(),
            previous_positions: self.previous.clone(),
        }
    }
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Before a render frame with no new pose, advance history to the current
    /// positions. Call `update` instead when a new pose is supplied; it records
    /// the preceding pose before writing the new one. Multiple views in one
    /// frame share the same history, so do this once per host frame.
    pub fn advance_history(&self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.previous, 0, &self.last_positions);
    }

    /// Mesh topology and source vertex order must match the immutable Mesh3d.
    /// Updates all four attributes in one write, returning the current local
    /// bounds for the host to insert as the entity's Aabb.
    pub fn update(&mut self, queue: &wgpu::Queue, mesh: &Mesh) -> Result<Aabb, String> {
        let (bytes, bounds, vertex_count, has_tangents) = pack(mesh)?;
        let same_indices = match (&self.indices, mesh.indices()) {
            (Some(expected), Some(actual)) => {
                expected.iter().copied().eq(actual.iter().map(|i| i as u32))
            }
            (None, None) => true,
            _ => false,
        };
        if vertex_count != self.vertex_count || has_tangents != self.has_tangents || !same_indices {
            return Err("Dynamic vertex layout changed; rebuild mesh and geometry buffer".into());
        }
        queue.write_buffer(&self.previous, 0, &self.last_positions);
        queue.write_buffer(&self.buffer, 0, &bytes);
        self.last_positions.clear();
        self.last_positions
            .extend(bytes.chunks_exact(64).flat_map(|v| v[..16].iter().copied()));
        Ok(bounds)
    }
}

fn pack(mesh: &Mesh) -> Result<(Vec<u8>, Aabb, usize, bool), String> {
    if mesh.has_morph_targets()
        || mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_some()
        || mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_some()
    {
        return Err("Dynamic geometry must contain final CPU posed attributes without a second morph/skin stage".into());
    }
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return Err("Dynamic geometry requires Float32x3 positions".into());
    };
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        return Err("Dynamic geometry requires Float32x3 normals".into());
    };
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        return Err("Dynamic geometry requires Float32x2 UV0".into());
    };
    let tangents = match mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
        Some(VertexAttributeValues::Float32x4(t)) => Some(t),
        Some(_) => return Err("Dynamic geometry tangents must be Float32x4".into()),
        None => None,
    };
    let count = positions.len();
    if count == 0
        || normals.len() != count
        || uvs.len() != count
        || tangents.is_some_and(|t| t.len() != count)
    {
        return Err("Dynamic geometry attributes must have equal nonzero lengths".into());
    }
    let mut bytes = Vec::with_capacity(
        count
            .checked_mul(64)
            .ok_or("Dynamic geometry size overflow")?,
    );
    let mut lo = glam::Vec3::splat(f32::INFINITY);
    let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
    for i in 0..count {
        let p = positions[i];
        let n = normals[i];
        let uv = uvs[i];
        // Inactive tangent slots are never consumed when VERTEX_TANGENTS is
        // absent; their padding does not create a tangent attribute.
        let t = tangents.map_or([0.0; 4], |t| t[i]);
        let values = [
            p[0], p[1], p[2], 1.0, n[0], n[1], n[2], 0.0, t[0], t[1], t[2], t[3], uv[0], uv[1],
            0.0, 0.0,
        ];
        if !values.iter().all(|f| f.is_finite()) {
            return Err("Dynamic geometry attributes must be finite".into());
        }
        for f in values {
            bytes.extend(f.to_le_bytes());
        }
        lo = lo.min(glam::Vec3::from_array(p));
        hi = hi.max(glam::Vec3::from_array(p));
    }
    Ok((
        bytes,
        Aabb::from_min_max(
            bevy::math::Vec3::from_array(lo.to_array()),
            bevy::math::Vec3::from_array(hi.to_array()),
        ),
        count,
        tangents.is_some(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use bevy::core_pipeline::prepass::{
        DeferredPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass,
    };
    use bevy::pbr::OpaqueRendererMethod;
    use bevy::render::mesh::allocator::MeshAllocator;

    fn renderer() -> BevyRenderer {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("real GPU required");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
                .unwrap();

        let info = adapter.get_info();
        let resources = RenderResources(
            RenderDevice::from(device.clone()),
            RenderQueue(std::sync::Arc::new(WgpuWrapper::new(queue.clone()))),
            RenderAdapterInfo(WgpuWrapper::new(info)),
            RenderAdapter(std::sync::Arc::new(WgpuWrapper::new(adapter))),
            RenderInstance(std::sync::Arc::new(WgpuWrapper::new(wgpu::Instance::new(
                &wgpu::InstanceDescriptor {
                    backends: wgpu::Backends::empty(),
                    ..Default::default()
                },
            )))),
        );
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .build()
                // The host program owns Ctrl+C.
                .disable::<TerminalCtrlCHandlerPlugin>()
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    close_when_requested: false,
                })
                .set(RenderPlugin {
                    render_creation: RenderCreation::Manual(resources),
                    synchronous_pipeline_compilation: true,
                    ..Default::default()
                }),
        );
        app.add_plugins((bake::BakePlugin, DynamicGeometryPlugin));
        while app.plugins_state() == PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        app.insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: AMBIENT_BRIGHTNESS,
            affects_lightmapped_meshes: true,
        });
        app.insert_resource(DirectionalLightShadowMap { size: 2048 });
        let light = Some(BevyRenderer::spawn_key_light(&mut app));
        BevyRenderer {
            app,
            device,
            queue,
            objects: HashMap::new(),
            geometry: HashMap::new(),
            views: HashMap::new(),
            textures: HashMap::new(),
            images: Default::default(),
            light,
            samples: Vec::new(),
            scene_lights: HashMap::new(),
            bounds: None,
            settle: 3,
            hidden: HashMap::new(),
            render: None,
        }
    }

    fn objects(meshes: &[loom_mesh::Mesh]) -> Vec<SceneObject<'_>> {
        meshes
            .iter()
            .enumerate()
            .map(|(i, mesh)| SceneObject {
                id: i as u64 + 1,
                mesh,
                transform: glam::Mat4::IDENTITY,
                wire_color: [200, 150, 100],
                surfaces: None,
                renderable: true,
                flags: Default::default(),
            })
            .collect()
    }

    fn posed_mesh(mesh: &loom_mesh::Mesh) -> Mesh {
        let mut data = mesh_data(mesh);
        for (p, uv) in data.positions.iter().zip(&mut data.uvs) {
            *uv = [(p[0] + p[2]) * 0.13, (p[1] - p[2]) * 0.17];
        }
        let mut m = data.to_bevy();
        m.generate_tangents().unwrap();
        m
    }

    fn material(r: &mut BevyRenderer) -> StandardMaterial {
        let albedo = r.texture_from(
            "dynamic validation albedo",
            TextureFormat::Rgba8UnormSrgb,
            [false; 2],
            || loom_scene::maps::Image {
                width: 2,
                height: 2,
                rgba: vec![
                    230, 80, 20, 255, 20, 180, 80, 255, 20, 50, 220, 255, 230, 230, 220, 255,
                ],
            },
        );
        let normal = r.texture_from(
            "dynamic validation normal",
            TextureFormat::Rgba8Unorm,
            [false; 2],
            || loom_scene::maps::Image {
                width: 2,
                height: 2,
                rgba: vec![
                    160, 110, 245, 255, 100, 150, 245, 255, 170, 145, 240, 255, 110, 100, 250, 255,
                ],
            },
        );
        StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: albedo,
            normal_map_texture: normal,
            perceptual_roughness: 0.6,
            ..Default::default()
        }
    }

    fn render(r: &mut BevyRenderer, deferred: bool) -> image::RgbaImage {
        let camera = ViewCamera {
            view: glam::Mat4::look_at_rh(
                glam::Vec3::new(-12.0, -17.0, 12.0),
                glam::Vec3::ZERO,
                glam::Vec3::Z,
            ),
            projection: glam::Mat4::perspective_rh(45f32.to_radians(), 1.0, 0.1, 100.0),
        };
        for _ in 0..5 {
            r.set_view(0, [96, 96], &camera, [0.0; 4], false);
            let entity = r.views[&0].camera;
            let mut e = r.app.world_mut().entity_mut(entity);
            e.insert((DepthPrepass, NormalPrepass, MotionVectorPrepass));
            if deferred {
                e.insert(DeferredPrepass);
            } else {
                e.remove::<DeferredPrepass>();
            }
            r.update();
        }
        r.read_view(0).unwrap()
    }

    #[test]
    fn renderer_sync_reuses_posed_resources_and_restores_render_material_settings() {
        let mut dynamic = renderer();
        let mut reference = renderer();
        let mut floor = loom_mesh::box_primitive(glam::Vec3::new(16.0, 16.0, 0.5));
        loom_mesh::edit_ops::translate_mesh(&mut floor, glam::Vec3::new(0.0, 0.0, -2.0)).unwrap();
        let mut meshes = vec![floor, loom_mesh::box_primitive(glam::Vec3::splat(3.0))];
        let ids = HashSet::from([2]);
        dynamic.sync_deforming(&objects(&meshes), &ids);
        reference.sync(&objects(&meshes));
        assert!(matches!(dynamic.objects[&1].parts[0].1, ObjectMaterial::Standard(_)));
        assert!(matches!(dynamic.objects[&2].parts[0].1, ObjectMaterial::Dynamic(_)));
        let (entity, material) = dynamic.objects[&2].parts[0].clone();
        let mesh = dynamic.app.world().get::<Mesh3d>(entity).unwrap().0.clone();
        let buffer = dynamic.objects[&2].buffers[0].buffer().id();
        let mut original = None;
        material.edit(dynamic.app.world_mut(), |m| { original = Some((m.double_sided, m.cull_mode)); });
        let options = RenderOptions { force_two_sided: true, ..Default::default() };
        dynamic.begin_render(options.clone());
        reference.begin_render(options);
        for pose in 0..2 {
            if pose == 1 {
                loom_mesh::edit_ops::transform_mesh(&mut meshes[1], glam::Mat4::from_scale_rotation_translation(glam::Vec3::new(1.2, 0.8, 1.1), glam::Quat::from_rotation_y(0.35), glam::Vec3::new(2.0, 0.5, 1.0))).unwrap();
            }
            dynamic.sync_deforming(&objects(&meshes), &ids);
            reference.sync(&objects(&meshes));
            let expected = render(&mut reference, false);
            let actual = render(&mut dynamic, false);
            assert_eq!(actual.as_raw(), expected.as_raw(), "production sync pose {pose}");
            assert_eq!(dynamic.objects[&2].parts[0].0, entity);
            assert_eq!(dynamic.objects[&2].parts[0].1.id(), material.id());
            assert_eq!(dynamic.app.world().get::<Mesh3d>(entity).unwrap().0, mesh);
            assert_eq!(dynamic.objects[&2].buffers[0].buffer().id(), buffer);
            let expected_bounds = mesh_data(&meshes[1]).aabb().unwrap();
            let bounds = dynamic.app.world().get::<Aabb>(entity).unwrap();
            assert_eq!(bounds.center, expected_bounds.center);
            assert_eq!(bounds.half_extents, expected_bounds.half_extents);
            material.edit(dynamic.app.world_mut(), |m| { assert!(m.double_sided); assert_eq!(m.cull_mode, None); });
        }
        dynamic.end_render();
        material.edit(dynamic.app.world_mut(), |m| { assert_eq!((m.double_sided, m.cull_mode), original.unwrap()); });
        let hidden = vec![meshes[1].clone()];
        dynamic.sync_hidden_deforming(&objects(&hidden), &HashSet::from([1]));
        let hidden_entity = dynamic.hidden[&1].parts[0].0;
        let mut hidden_next = hidden;
        loom_mesh::edit_ops::translate_mesh(&mut hidden_next[0], glam::Vec3::X).unwrap();
        dynamic.sync_hidden_deforming(&objects(&hidden_next), &HashSet::from([1]));
        assert_eq!(dynamic.hidden[&1].parts[0].0, hidden_entity);
        meshes[1] = loom_mesh::sphere_segmented(1.5, 12, 0.0);
        dynamic.sync_deforming(&objects(&meshes), &ids);
        assert_ne!(dynamic.objects[&2].parts[0].0, entity, "topology edit rebuilds the immutable mesh");
        let before_reload = dynamic.objects[&2].parts[0].1.id();
        dynamic.reload_bitmaps();
        dynamic.sync_deforming(&objects(&meshes), &ids);
        assert_ne!(dynamic.objects[&2].parts[0].1.id(), before_reload, "bitmap reload rebuilds material textures");
    }

    #[test]
    fn packed_attributes_preserve_bits_and_reject_invalid_frames() {
        let source = loom_mesh::box_primitive(glam::Vec3::splat(3.0));
        let mut mesh = posed_mesh(&source);
        let (bytes, _, count, _) = pack(&mesh).unwrap();
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!()
        };
        let Some(VertexAttributeValues::Float32x4(t)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
        else {
            panic!()
        };
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!()
        };
        for i in 0..count {
            for (component, expected) in [
                (0, p[i][0]),
                (1, p[i][1]),
                (2, p[i][2]),
                (4, n[i][0]),
                (5, n[i][1]),
                (6, n[i][2]),
                (8, t[i][0]),
                (9, t[i][1]),
                (10, t[i][2]),
                (11, t[i][3]),
                (12, uv[i][0]),
                (13, uv[i][1]),
            ] {
                let offset = i * 64 + component * 4;
                assert_eq!(
                    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()),
                    expected.to_bits()
                );
            }
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[f32::NAN, 0.0, 0.0]; count]);
        assert!(pack(&mesh).is_err());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; count - 1]);
        assert!(pack(&mesh).is_err());
    }

    #[test]
    fn device_storage_limit_is_checked_before_allocating_geometry() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("real GPU required");
        let mut descriptor = crate::device_descriptor(&adapter);
        descriptor.required_limits.max_storage_buffer_binding_size = 64;
        let (device, _) = pollster::block_on(adapter.request_device(&descriptor, None)).unwrap();
        let mesh = posed_mesh(&loom_mesh::box_primitive(glam::Vec3::splat(3.0)));
        assert!(DynamicGeometryBuffer::new(&device, &mesh).is_err());
    }

    #[test]
    fn posed_storage_matches_cpu_mesh_pbr_prepass_shadows_and_allocator_offsets() {
        let mut dynamic = renderer();
        let mut reference = renderer();
        let mut ground = loom_mesh::box_primitive(glam::Vec3::new(16.0, 16.0, 0.5));
        loom_mesh::edit_ops::translate_mesh(&mut ground, glam::Vec3::new(0.0, 0.0, -2.0)).unwrap();
        let mut meshes = vec![ground, loom_mesh::box_primitive(glam::Vec3::splat(3.0))];
        dynamic.sync(&objects(&meshes));
        let mut buffers = Vec::new();
        let mut entities = Vec::new();
        let mut mesh_handles = Vec::new();
        let mut material_handles = Vec::new();
        for (i, source) in meshes.iter().enumerate() {
            let entity = dynamic.objects[&(i as u64 + 1)].parts[0].0;
            let handle = dynamic.app.world().get::<Mesh3d>(entity).unwrap().0.clone();
            // Keep the immutable mesh's tangent layout identical to the CPU
            // reference, including normal-mapped StandardMaterial's inputs.
            let posed = posed_mesh(source);
            *dynamic
                .app
                .world_mut()
                .resource_mut::<Assets<Mesh>>()
                .get_mut(&handle)
                .unwrap() = posed.clone();
            let (buffer, bounds) = DynamicGeometryBuffer::new(&dynamic.device, &posed).unwrap();
            let base = material(&mut dynamic);
            let material = dynamic
                .app
                .world_mut()
                .resource_mut::<Assets<DynamicMaterial>>()
                .add(DynamicMaterial {
                    base,
                    extension: buffer.extension(),
                });
            dynamic
                .app
                .world_mut()
                .entity_mut(entity)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                .insert((MeshMaterial3d(material.clone()), bounds));
            buffers.push(buffer);
            entities.push(entity);
            mesh_handles.push(handle);
            material_handles.push(material);
        }
        let buffer_ids: Vec<_> = buffers.iter().map(|b| b.buffer().id()).collect();
        for pose in 0..2 {
            if pose == 1 {
                loom_mesh::edit_ops::transform_mesh(
                    &mut meshes[1],
                    glam::Mat4::from_scale_rotation_translation(
                        glam::Vec3::new(1.2, 0.8, 1.1),
                        glam::Quat::from_rotation_y(0.35),
                        glam::Vec3::new(2.0, 0.5, 1.0),
                    ),
                )
                .unwrap();
            }
            reference.sync(&objects(&meshes));
            for (i, source) in meshes.iter().enumerate() {
                let posed = posed_mesh(source);
                let bounds = buffers[i].update(&dynamic.queue, &posed).unwrap();
                dynamic
                    .app
                    .world_mut()
                    .entity_mut(entities[i])
                    .insert(bounds);
                let entity = reference.objects[&(i as u64 + 1)].parts[0].0;
                let handle = reference
                    .app
                    .world_mut()
                    .resource_mut::<Assets<Mesh>>()
                    .add(posed);
                reference
                    .app
                    .world_mut()
                    .entity_mut(entity)
                    .insert(Mesh3d(handle));
                let mat = reference
                    .app
                    .world()
                    .get::<MeshMaterial3d<StandardMaterial>>(entity)
                    .unwrap()
                    .0
                    .clone();
                let base = material(&mut reference);
                *reference
                    .app
                    .world_mut()
                    .resource_mut::<Assets<StandardMaterial>>()
                    .get_mut(&mat)
                    .unwrap() = base;
            }
            for deferred in [false, true] {
                for handle in &material_handles {
                    dynamic
                        .app
                        .world_mut()
                        .resource_mut::<Assets<DynamicMaterial>>()
                        .get_mut(handle)
                        .unwrap()
                        .base
                        .opaque_render_method = if deferred {
                        OpaqueRendererMethod::Deferred
                    } else {
                        OpaqueRendererMethod::Forward
                    };
                }
                let refs: Vec<_> = reference
                    .objects
                    .values()
                    .flat_map(|o| o.parts.iter().map(|(_, m)| m.clone()))
                    .collect();
                for handle in refs {
                    handle.edit(reference.app.world_mut(), |m| {
                        m.opaque_render_method = if deferred { OpaqueRendererMethod::Deferred } else { OpaqueRendererMethod::Forward };
                    });
                }
                let expected = render(&mut reference, deferred);
                let actual = render(&mut dynamic, deferred);
                assert!(
                    expected.pixels().any(|p| p[3] != 0),
                    "CPU reference must draw geometry"
                );
                let mismatches = actual
                    .pixels()
                    .zip(expected.pixels())
                    .filter(|(a, b)| a != b)
                    .count();
                if mismatches != 0 {
                    actual.save("/tmp/3dloom-dynamic-actual.png").unwrap();
                    expected.save("/tmp/3dloom-dynamic-reference.png").unwrap();
                }
                assert_eq!(
                    mismatches,
                    0,
                    "pose {pose}, deferred {deferred}; drawn dynamic={}, reference={}",
                    actual.pixels().filter(|p| p[3] != 0).count(),
                    expected.pixels().filter(|p| p[3] != 0).count()
                );
                if pose == 0 && !deferred {
                    for buffer in &buffers {
                        buffer.advance_history(&dynamic.queue);
                    }
                    let light = dynamic.light.unwrap();
                    dynamic
                        .app
                        .world_mut()
                        .get_mut::<DirectionalLight>(light)
                        .unwrap()
                        .shadows_enabled = false;
                    let unshadowed = render(&mut dynamic, false);
                    assert!(
                        actual
                            .pixels()
                            .zip(unshadowed.pixels())
                            .any(|(a, b)| a != b),
                        "shadow must affect visible pixels"
                    );
                    dynamic
                        .app
                        .world_mut()
                        .get_mut::<DirectionalLight>(light)
                        .unwrap()
                        .shadows_enabled = true;
                }
            }
            for i in 0..buffers.len() {
                assert_eq!(buffers[i].buffer().id(), buffer_ids[i]);
                assert_eq!(
                    dynamic.app.world().get::<Mesh3d>(entities[i]).unwrap().0,
                    mesh_handles[i]
                );
                assert_eq!(
                    dynamic
                        .app
                        .world()
                        .get::<MeshMaterial3d<DynamicMaterial>>(entities[i])
                        .unwrap()
                        .0,
                    material_handles[i]
                );
            }
        }
        let mut changed_topology = posed_mesh(&meshes[1]);
        let mut indices: Vec<u32> = changed_topology
            .indices()
            .unwrap()
            .iter()
            .map(|i| i as u32)
            .collect();
        indices.swap(0, 1);
        changed_topology.insert_indices(bevy::render::mesh::Indices::U32(indices));
        assert!(buffers[1]
            .update(&dynamic.queue, &changed_topology)
            .is_err());
        let mut changed_layout = posed_mesh(&meshes[1]);
        changed_layout.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
        assert!(buffers[1].update(&dynamic.queue, &changed_layout).is_err());
        let allocator = dynamic
            .app
            .sub_app(RenderApp)
            .world()
            .resource::<MeshAllocator>();
        let offsets: Vec<_> = mesh_handles
            .iter()
            .map(|h| allocator.mesh_vertex_slice(&h.id()).unwrap().range.start)
            .collect();
        assert_ne!(
            offsets[0], offsets[1],
            "both meshes must exercise distinct allocator offsets"
        );
        assert!(offsets.iter().any(|&i| i > 0));
    }
}

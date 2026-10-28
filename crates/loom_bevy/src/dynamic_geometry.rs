//! Persistent posed-geometry transport for Bevy's StandardMaterial.
//!
//! Final CPU attributes are uploaded verbatim, or validated GPU records copied.
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

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedVertex {
    position: [f32; 4],
    normal: [f32; 4],
    tangent: [f32; 4],
    uv: [f32; 4],
}
const _: [(); 64] = [(); std::mem::size_of::<PackedVertex>()];

/// Current CPU positions prepared in original corner order. Fields stay private:
/// the finite/bounds proof is invalidated whenever the owned staging is refilled.
#[derive(Default)]
pub(crate) struct PreparedPositions {
    positions: Vec<[f32; 4]>,
    min: glam::Vec3,
    max: glam::Vec3,
    valid: bool,
}
impl PreparedPositions {
    pub(crate) fn gather(&mut self, positions: impl ExactSizeIterator<Item = [f32; 3]>) -> Result<(), String> {
        self.valid = false;
        self.positions.clear();
        self.positions.reserve(positions.len());
        self.min = glam::Vec3::splat(f32::INFINITY);
        self.max = glam::Vec3::splat(f32::NEG_INFINITY);
        for p in positions {
            if !p.iter().all(|v| v.is_finite()) { return Err("GPU pose positions must be finite".into()); }
            self.min = self.min.min(glam::Vec3::from_array(p));
            self.max = self.max.max(glam::Vec3::from_array(p));
            self.positions.push([p[0], p[1], p[2], 1.0]);
        }
        self.valid = true;
        Ok(())
    }
}

#[cfg(test)]
impl PreparedPositions {
    pub(crate) fn assert_current_bits(&self, positions: &[[f32; 3]]) {
        assert!(self.valid);
        assert_eq!(self.positions.len(), positions.len());
        for (actual, expected) in self.positions.iter().zip(positions) {
            assert_eq!(actual.map(f32::to_bits), [expected[0], expected[1], expected[2], 1.0].map(f32::to_bits));
        }
    }
}

enum CurrentPositions<'a> {
    Raw(&'a [[f32; 3]]),
    Prepared(&'a mut PreparedPositions),
}

/// An immutable-size device allocation. Keep its extension handle unchanged
/// during animation; updating buffer contents needs no material asset event.
pub struct DynamicGeometryBuffer {
    buffer: Buffer,
    vertex_count: usize,
    has_tangents: bool,
    previous: Buffer,
    last_positions: Vec<[f32; 4]>,
    history_needs_advance: bool,
    next_positions: Vec<[f32; 4]>,
    packed: Vec<PackedVertex>,
    indices: Option<Vec<u32>>,
}

impl DynamicGeometryBuffer {
    /// Conservative object eligibility: every material part contains no more
    /// vertices than the whole expanded object. Unsupported objects retain the
    /// ordinary StandardMaterial path with the same geometry and attributes.
    pub(crate) fn supported(limits: &wgpu::Limits, vertices: usize) -> bool {
        let Some(bytes) = vertices
            .checked_mul(std::mem::size_of::<PackedVertex>())
            .and_then(|n| u64::try_from(n).ok())
        else {
            return false;
        };
        vertices > 0
            && limits.max_storage_buffers_per_shader_stage >= 2
            && bytes <= limits.max_buffer_size
            && bytes <= u64::from(limits.max_storage_buffer_binding_size)
    }

    pub fn new(device: &wgpu::Device, mesh: &Mesh) -> Result<(Self, Aabb), String> {
        let count = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .map_or(0, VertexAttributeValues::len);
        if !Self::supported(&device.limits(), count) {
            return Err("Dynamic geometry exceeds device vertex storage limits or is empty".into());
        }
        let mut packed = Vec::new();
        let mut last_positions = Vec::new();
        let (bounds, vertex_count, has_tangents) =
            pack_into(mesh, &mut packed, &mut last_positions)?;
        let bytes = bytemuck::cast_slice(&packed);
        let previous = device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("previous posed positions"),
                contents: bytemuck::cast_slice(&last_positions),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
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
                history_needs_advance: false,
                next_positions: Vec::with_capacity(vertex_count),
                packed,
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
    #[cfg(test)]
    pub(crate) fn assert_current_history_bits(&self, positions: &[[f32; 3]]) {
        assert_eq!(self.last_positions.len(), positions.len());
        for (actual, expected) in self.last_positions.iter().zip(positions) {
            assert_eq!(actual.map(f32::to_bits), [expected[0], expected[1], expected[2], 1.0].map(f32::to_bits));
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Advance to the rendered positions once after the scene frame finishes.
    /// Intermediate updates before that boundary share the preceding frame.
    pub fn advance_history(&mut self, queue: &wgpu::Queue) {
        if !self.history_needs_advance {
            return;
        }
        self.history_needs_advance = false;
        let _scope = loom_render::profile::Scope::new("bevy.dynamic.history_write");
        queue.write_buffer(
            &self.previous,
            0,
            bytemuck::cast_slice(&self.last_positions),
        );
    }

    /// Mesh topology and source vertex order must match the immutable Mesh3d.
    /// Updates all four attributes in one write, returning the current local
    /// bounds for the host to insert as the entity's Aabb.
    pub fn update(&mut self, queue: &wgpu::Queue, mesh: &Mesh) -> Result<Aabb, String> {
        let _scope = loom_render::profile::Scope::new("bevy.dynamic.update");
        let (bounds, vertex_count, has_tangents) =
            pack_into(mesh, &mut self.packed, &mut self.next_positions)?;
        let _indices = loom_render::profile::Scope::new("bevy.dynamic.indices");
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
        drop(_indices);
        let _write = loom_render::profile::Scope::new("bevy.dynamic.queue_write");
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.packed));
        drop(_write);
        let _positions = loom_render::profile::Scope::new("bevy.dynamic.copy_positions");
        std::mem::swap(&mut self.last_positions, &mut self.next_positions);
        self.history_needs_advance = true;
        Ok(bounds)
    }

    /// Validate once while borrowing the immutable source layout and exclusive
    /// target until the caller commits or drops this operation.
    pub(crate) fn prepare_gpu<'a>(
        &'a mut self, source: &'a wgpu::Buffer, source_offset: u64,
        positions: &'a [[f32; 3]], indices: Option<&[u32]>,
    ) -> Result<ValidatedGpuCopy<'a>, String> {
        let bytes = self.validate_gpu(source, source_offset, positions, indices)?;
        Ok(ValidatedGpuCopy { target: self, source, source_offset, positions: CurrentPositions::Raw(positions), bytes })
    }

    #[cfg(test)]
    pub(crate) fn commit_gpu(
        &mut self, encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Buffer, source_offset: u64, positions: &[[f32; 3]], indices: Option<&[u32]>,
    ) -> Result<Aabb, String> {
        Ok(self.prepare_gpu(source, source_offset, positions, indices)?.commit(encoder).0)
    }

    pub(crate) fn prepare_gpu_fused<'a>(&'a mut self, source: &'a wgpu::Buffer, source_offset: u64, positions: &'a mut PreparedPositions, indices: Option<&[u32]>) -> Result<ValidatedGpuCopy<'a>, String> {
        let bytes = self.validate_gpu_layout(source, source_offset, positions.positions.len(), positions.valid, indices)?;
        Ok(ValidatedGpuCopy { target: self, source, source_offset, positions: CurrentPositions::Prepared(positions), bytes })
    }

    pub(crate) fn validate_gpu(&self, source: &wgpu::Buffer, source_offset: u64, positions: &[[f32; 3]], indices: Option<&[u32]>) -> Result<u64, String> {
        let _scope = loom_render::profile::Scope::new("gpu_pose.layout_validation");
        let bytes = (self.vertex_count as u64).checked_mul(64).ok_or("GPU pose size overflow")?;
        if positions.len() != self.vertex_count || self.indices.as_deref() != indices
            || !positions.iter().flatten().all(|v| v.is_finite())
            || source_offset % 64 != 0 || source == &*self.buffer
            || !source.usage().contains(wgpu::BufferUsages::COPY_SRC)
            || source_offset.checked_add(bytes).is_none_or(|end| end > source.size()) {
            return Err("GPU pose layout, positions or source range is invalid".into());
        }
        Ok(bytes)
    }

    fn validate_gpu_layout(&self, source: &wgpu::Buffer, source_offset: u64, count: usize, finite: bool, indices: Option<&[u32]>) -> Result<u64, String> {
        let _scope = loom_render::profile::Scope::new("gpu_pose.layout_validation");
        let bytes = (self.vertex_count as u64).checked_mul(64).ok_or("GPU pose size overflow")?;
        if count != self.vertex_count || self.indices.as_deref() != indices || !finite
            || source_offset % 64 != 0
            || source == &*self.buffer
            || !source.usage().contains(wgpu::BufferUsages::COPY_SRC)
            || source_offset.checked_add(bytes).is_none_or(|end| end > source.size())
        {
            return Err("GPU pose layout, positions or source range is invalid".into());
        }
        Ok(bytes)
    }
}

/// Fields stay private: an operation can only be obtained from checked layout.
pub(crate) struct ValidatedGpuCopy<'a> {
    target: &'a mut DynamicGeometryBuffer,
    source: &'a wgpu::Buffer,
    source_offset: u64,
    positions: CurrentPositions<'a>,
    bytes: u64,
}
impl ValidatedGpuCopy<'_> {
    /// Whole-pose callers must validate all compute guards before any commit.
    pub(crate) fn commit(self, encoder: &mut wgpu::CommandEncoder) -> (Aabb, (glam::Vec3, glam::Vec3)) {
        let _scope = loom_render::profile::Scope::new("bevy.dynamic.gpu_copy");
        let target = self.target;
        let (lo, hi) = match &self.positions {
            CurrentPositions::Raw(positions) => {
                let mut lo = glam::Vec3::splat(f32::INFINITY);
                let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
                target.next_positions.clear();
                target.next_positions.reserve(positions.len());
                for &p in *positions {
                    lo = lo.min(glam::Vec3::from_array(p));
                    hi = hi.max(glam::Vec3::from_array(p));
                    target.next_positions.push([p[0], p[1], p[2], 1.0]);
                }
                (lo, hi)
            }
            CurrentPositions::Prepared(prepared) => {
                (prepared.min, prepared.max)
            }
        };
        encoder.copy_buffer_to_buffer(self.source, self.source_offset, &target.buffer, 0, self.bytes);
        match self.positions {
            CurrentPositions::Raw(_) => std::mem::swap(&mut target.last_positions, &mut target.next_positions),
            CurrentPositions::Prepared(prepared) => {
                std::mem::swap(&mut target.last_positions, &mut prepared.positions);
                prepared.valid = false;
            }
        }
        target.history_needs_advance = true;
        (Aabb::from_min_max(lo, hi), (lo, hi))
    }
}

pub(crate) fn validate_geometry(mesh: &Mesh) -> Result<(), String> {
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
    if normals.len() != count || uvs.len() != count || tangents.is_some_and(|t| t.len() != count) {
        return Err("Dynamic geometry attributes must have equal lengths".into());
    }
    if !positions
        .iter()
        .flatten()
        .chain(normals.iter().flatten())
        .chain(uvs.iter().flatten())
        .all(|f| f.is_finite())
        || tangents.is_some_and(|t| !t.iter().flatten().all(|f| f.is_finite()))
    {
        return Err("Dynamic geometry attributes must be finite".into());
    }
    Ok(())
}

fn pack_into(
    mesh: &Mesh,
    vertices: &mut Vec<PackedVertex>,
    output_positions: &mut Vec<[f32; 4]>,
) -> Result<(Aabb, usize, bool), String> {
    let _scope = loom_render::profile::Scope::new("bevy.dynamic.pack");
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
    count
        .checked_mul(64)
        .ok_or("Dynamic geometry size overflow")?;
    vertices.clear();
    output_positions.clear();
    vertices.reserve(count);
    output_positions.reserve(count);
    let mut lo = glam::Vec3::splat(f32::INFINITY);
    let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
    for i in 0..count {
        let p = positions[i];
        let n = normals[i];
        let uv = uvs[i];
        let t = tangents.map_or([0.0; 4], |t| t[i]);
        let vertex = PackedVertex {
            position: [p[0], p[1], p[2], 1.0],
            normal: [n[0], n[1], n[2], 0.0],
            tangent: t,
            uv: [uv[0], uv[1], 0.0, 0.0],
        };
        if !vertex
            .position
            .iter()
            .chain(&vertex.normal)
            .chain(&vertex.tangent)
            .chain(&vertex.uv)
            .all(|f| f.is_finite())
        {
            return Err("Dynamic geometry attributes must be finite".into());
        }
        output_positions.push(vertex.position);
        vertices.push(vertex);
        lo = lo.min(glam::Vec3::from_array(p));
        hi = hi.max(glam::Vec3::from_array(p));
    }
    Ok((
        Aabb::from_min_max(
            bevy::math::Vec3::from_array(lo.to_array()),
            bevy::math::Vec3::from_array(hi.to_array()),
        ),
        count,
        tangents.is_some(),
    ))
}

#[cfg(test)]
fn pack_reference(mesh: &Mesh) -> Result<(Vec<u8>, Aabb, usize, bool), String> {
    let _scope = loom_render::profile::Scope::new("bevy.dynamic.pack");
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

    #[test]
    fn history_ignores_intermediate_poses_until_the_scene_frame_finishes() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None)).unwrap();
        let mut geometry = loom_mesh::box_primitive(glam::Vec3::ONE);
        let first = posed_mesh(&geometry);
        loom_mesh::edit_ops::translate_mesh(&mut geometry, glam::Vec3::X * 2.0).unwrap();
        let middle = posed_mesh(&geometry);
        loom_mesh::edit_ops::translate_mesh(&mut geometry, glam::Vec3::X * 2.0).unwrap();
        let last = posed_mesh(&geometry);
        let mut buffer = DynamicGeometryBuffer::new(&device, &first).unwrap().0;
        let first_x = buffer.last_positions[0][0].to_bits();
        let mut reader = crate::gpu_pose_commit::GuardReadback::new(&device, 1).unwrap();
        buffer.update(&queue, &middle).unwrap();
        buffer.update(&queue, &last).unwrap();
        assert_eq!(reader.read(&device, &queue, &[&buffer.previous]).unwrap(), vec![first_x]);
        let last_x = buffer.last_positions[0][0].to_bits();
        buffer.advance_history(&queue);
        assert_eq!(reader.read(&device, &queue, &[&buffer.previous]).unwrap(), vec![last_x]);
        for mesh in [&middle, &first] {
            let output = DynamicGeometryBuffer::new(&device, mesh).unwrap().0;
            let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!() };
            let indices = mesh.indices().map(|i| i.iter().map(|i| i as u32).collect::<Vec<_>>());
            let mut encoder = device.create_command_encoder(&Default::default());
            buffer.commit_gpu(&mut encoder, output.buffer(), 0, positions, indices.as_deref()).unwrap();
            queue.submit([encoder.finish()]);
        }
        assert_eq!(reader.read(&device, &queue, &[&buffer.previous]).unwrap(), vec![last_x]);
        buffer.advance_history(&queue);
        assert_eq!(reader.read(&device, &queue, &[&buffer.previous]).unwrap(), vec![first_x]);
    }

    #[test]
    fn gpu_pose_commit_preserves_storage_and_last_committed_history() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None)).unwrap();
        let mut source = loom_mesh::box_primitive(glam::Vec3::splat(3.0));
        let first = posed_mesh(&source);
        let (mut buffer, _) = DynamicGeometryBuffer::new(&device, &first).unwrap();
        let previous = buffer.last_positions.clone();
        loom_mesh::edit_ops::translate_mesh(&mut source, glam::Vec3::new(1.0, 2.0, 4.0)).unwrap();
        let second = posed_mesh(&source);
        let (expected, bounds, _, _) = pack_reference(&second).unwrap();
        let VertexAttributeValues::Float32x3(positions) = second.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!() };
        let indices = second.indices().map(|v| v.iter().map(|v| v as u32).collect::<Vec<_>>());
        let mut bytes = vec![0u8; 64];
        bytes.extend_from_slice(&expected);
        let candidate = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("validated pose transport test"), contents: &bytes, usage: wgpu::BufferUsages::COPY_SRC,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        let alias = buffer.buffer().clone();
        assert!(buffer.commit_gpu(&mut encoder, &alias, 0, positions, indices.as_deref()).is_err());
        assert!(buffer.commit_gpu(&mut encoder, &candidate, 1, positions, indices.as_deref()).is_err());
        assert!(buffer.commit_gpu(&mut encoder, &candidate, u64::MAX - 63, positions, indices.as_deref()).is_err());
        assert!(buffer.commit_gpu(&mut encoder, &candidate, 128, positions, indices.as_deref()).is_err());
        assert!(buffer.commit_gpu(&mut encoder, &candidate, 64, &positions[1..], indices.as_deref()).is_err());
        let no_copy_source = device.create_buffer(&wgpu::BufferDescriptor {
            label: None, size: candidate.size(), usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false,
        });
        assert!(buffer.commit_gpu(&mut encoder, &no_copy_source, 64, positions, indices.as_deref()).is_err());
        let mut nonfinite = positions.clone(); nonfinite[0][0] = f32::NAN;
        assert!(buffer.commit_gpu(&mut encoder, &candidate, 64, &nonfinite, indices.as_deref()).is_err());
        let mut changed_indices = indices.clone().unwrap(); changed_indices.swap(0, 1);
        assert!(buffer.commit_gpu(&mut encoder, &candidate, 64, positions, Some(&changed_indices)).is_err());
        assert_eq!(buffer.last_positions, previous);
        let actual_bounds = buffer.commit_gpu(&mut encoder, &candidate, 64, positions, indices.as_deref()).unwrap();
        assert_eq!(actual_bounds, bounds);
        let readback = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: expected.len() as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        encoder.copy_buffer_to_buffer(buffer.buffer(), 0, &readback, 0, expected.len() as u64);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device.poll(wgpu::Maintain::Wait); rx.recv().unwrap().unwrap();
        assert_eq!(&*readback.slice(..).get_mapped_range(), expected.as_slice());
        readback.unmap();
        let committed: Vec<_> = positions.iter().map(|p| [p[0], p[1], p[2], 1.0]).collect();
        assert_eq!(buffer.last_positions, committed);
        // Switching back to the CPU producer must retain the GPU pose as history.
        buffer.update(&queue, &first).unwrap();
        assert_eq!(buffer.next_positions, committed);
    }

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
            gpu_poses: None,
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
    #[ignore = "actual Ray GPU packing benchmark requires LOOM_PERF_SCENE"]
    fn actual_ray_dynamic_packing_performance() {
        let path = std::env::var("LOOM_PERF_SCENE").expect("real saved scene path");
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let scene: loom_scene::Scene = serde_json::from_value(value["scene"].take()).unwrap();
        let ids: Vec<_> = scene
            .nodes()
            .filter(|n| {
                n.modifiers
                    .iter()
                    .any(|m| matches!(m.modifier, loom_scene::Modifier::Skin(_)))
            })
            .map(|n| n.id)
            .collect();
        let mut frames = Vec::new();
        for frame in [0.0, 39.25, 149.0] {
            scene.set_time(frame);
            let mut meshes = Vec::new();
            for id in &ids {
                let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
                loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(*id)).unwrap();
                for (_, data) in mesh_data_by_id(&mesh) {
                    let mut mesh = data.to_bevy();
                    mesh.generate_tangents().unwrap();
                    meshes.push(mesh);
                }
            }
            frames.push(meshes);
        }
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("real GPU required");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
                .unwrap();
        let mut buffers: Vec<_> = frames[0]
            .iter()
            .map(|m| DynamicGeometryBuffer::new(&device, m).unwrap().0)
            .collect();
        let mut packed = Vec::new();
        let mut positions = Vec::new();
        for frame in &frames {
            for mesh in frame {
                let (expected, _, _, _) = pack_reference(mesh).unwrap();
                pack_into(mesh, &mut packed, &mut positions).unwrap();
                assert_eq!(
                    bytemuck::cast_slice::<_, u8>(&packed),
                    expected,
                    "actual Ray packed bytes must remain exact"
                );
                let expected_positions: Vec<u8> = expected
                    .chunks_exact(64)
                    .flat_map(|v| v[..16].iter().copied())
                    .collect();
                assert_eq!(
                    bytemuck::cast_slice::<_, u8>(&positions),
                    expected_positions
                );
            }
        }
        let mut pack_time = 0.0;
        let mut update_time = 0.0;
        let mut history_time = 0.0;
        for i in 0..12 {
            let frame = &frames[i % frames.len()];
            let t = std::time::Instant::now();
            for mesh in frame {
                std::hint::black_box(pack_into(mesh, &mut packed, &mut positions).unwrap());
            }
            pack_time += t.elapsed().as_secs_f64();
            let t = std::time::Instant::now();
            for (buffer, mesh) in buffers.iter_mut().zip(frame) {
                buffer.update(&queue, mesh).unwrap();
            }
            update_time += t.elapsed().as_secs_f64();
            let t = std::time::Instant::now();
            for buffer in &mut buffers {
                buffer.advance_history(&queue);
            }
            history_time += t.elapsed().as_secs_f64();
            queue.submit([]);
            device.poll(wgpu::Maintain::Wait);
        }
        eprintln!(
            "RAY_DYNAMIC parts={} vertices={} pack_ms={:.3} update_ms={:.3} history_ms={:.3}",
            buffers.len(),
            buffers.iter().map(|b| b.vertex_count).sum::<usize>(),
            pack_time * 1000.0 / 12.0,
            update_time * 1000.0 / 12.0,
            history_time * 1000.0 / 12.0
        );
    }

    #[test]
    fn renderer_gpu_batch_updates_current_pose_and_preserves_resources() {
        let mut r = renderer();
        let mut source = loom_mesh::sphere_segmented(2.0, 8, 0.0);
        loom_mesh::uvmap::auto_map(&mut source, loom_mesh::uvmap::AutoMap::Spherical, 1);
        let mut meshes = vec![source.clone(), source];
        let ids = HashSet::from([1, 2]);
        r.enable_gpu_poses();
        r.sync_deforming(&objects(&meshes), &ids);
        assert!(r.gpu_pose_sources().is_empty(), "cold CPU initialization is not a committed GPU source");
        let handles: Vec<_> = (1..=2).map(|id| (r.objects[&id].parts[0].0, r.objects[&id].buffers[0].buffer().id())).collect();
        for (i, mesh) in meshes.iter_mut().enumerate() {
            loom_mesh::edit_ops::translate_mesh(mesh, glam::Vec3::X * (i + 2) as f32).unwrap();
        }
        r.sync_deforming(&objects(&meshes), &ids);
        let (min, max) = meshes.iter().flat_map(|mesh| mesh.vertices()).fold(
            (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)),
            |(min, max), vertex| (min.min(vertex.position), max.max(vertex.position)),
        );
        assert_eq!(r.bounds, Some(((min + max) * 0.5, ((max - min) * 0.5).length().max(1e-3))), "global bounds must include both differently posed objects without Aabb roundtrip");
        let sources = r.gpu_pose_sources();
        assert_eq!(sources.len(), 2);
        for source in sources { assert_eq!(source.vertex_count, meshes[source.id as usize - 1].vertex_count()); }
        let mut reader = crate::gpu_pose_commit::GuardReadback::new(&r.device, 2).unwrap();
        let mut expected = Vec::new();
        for (i, mesh) in meshes.iter().enumerate() {
            let object = &r.objects[&(i as u64 + 1)];
            assert!(object.gpu_pose_current);
            assert_eq!((object.parts[0].0, object.buffers[0].buffer().id()), handles[i]);
            expected.push(crate::mesh_data(mesh).positions[0][0].to_bits());
        }
        let refs: Vec<_> = (1..=2).map(|id| &**r.objects[&id].buffers[0].buffer()).collect();
        assert_eq!(reader.read(&r.device, &r.queue, &refs).unwrap(), expected);
        let position = meshes[0].vertices()[0].position;
        meshes[0].vertices_mut()[1].position = position;
        r.sync_deforming(&objects(&meshes), &ids);
        assert!(r.gpu_poses.as_ref().unwrap().recovered_recipes > 0);
        for (i, id) in (1..=2).enumerate() {
            let object = &r.objects[&id];
            assert!(object.gpu_pose_current);
            assert_eq!((object.parts[0].0, object.buffers[0].buffer().id()), handles[i]);
        }
        r.sync(&objects(&meshes));
        assert!(r.gpu_pose_sources().is_empty(), "CPU sync must withdraw preceding GPU sources");
        r.sync_deforming(&objects(&meshes), &ids);
        assert!(r.gpu_pose_sources().is_empty(), "return to deformation initializes dynamic resources on the ordinary path");
        r.sync_deforming(&objects(&meshes), &ids);
        assert_eq!(r.gpu_pose_sources().len(), 2);
        r.sync_deforming(&[], &ids);
        assert!(r.gpu_pose_sources().is_empty(), "removed objects cannot export cached sources");

    }

    #[test]
    fn cpu_sync_replaces_gpu_pose_even_when_the_last_cpu_hash_matches() {
        let mut r = renderer();
        let meshes = vec![loom_mesh::box_primitive(glam::Vec3::ONE)];
        let ids = HashSet::from([1]);
        r.sync_deforming(&objects(&meshes), &ids);
        let old_hash = r.objects[&1].mesh_hash;
        let mut changed = meshes[0].clone();
        loom_mesh::edit_ops::translate_mesh(&mut changed, glam::Vec3::X * 7.0).unwrap();
        let posed = posed_mesh(&changed);
        let source = DynamicGeometryBuffer::new(&r.device, &posed).unwrap().0;
        let VertexAttributeValues::Float32x3(positions) = posed.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!() };
        let indices = posed.indices().map(|v| v.iter().map(|v| v as u32).collect::<Vec<_>>());
        let mut encoder = r.device.create_command_encoder(&Default::default());
        let object = r.objects.get_mut(&1).unwrap();
        object.buffers[0].commit_gpu(&mut encoder, source.buffer(), 0, positions, indices.as_deref()).unwrap();
        object.gpu_pose_current = true;
        r.queue.submit([encoder.finish()]);
        let mut readback = crate::gpu_pose_commit::GuardReadback::new(&r.device, 1).unwrap();
        assert_eq!(readback.read(&r.device, &r.queue, &[r.objects[&1].buffers[0].buffer()]).unwrap(), vec![positions[0][0].to_bits()]);
        // The CPU source has not changed since its last sync, but the GPU has.
        r.sync_deforming(&objects(&meshes), &ids);
        assert!(!r.objects[&1].gpu_pose_current);
        assert_eq!(r.objects[&1].mesh_hash, old_hash);
        let expected = crate::mesh_data(&meshes[0]).positions[0][0].to_bits();
        assert_eq!(readback.read(&r.device, &r.queue, &[r.objects[&1].buffers[0].buffer()]).unwrap(), vec![expected]);
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
        assert!(matches!(
            dynamic.objects[&1].parts[0].1,
            ObjectMaterial::Standard(_)
        ));
        assert!(matches!(
            dynamic.objects[&2].parts[0].1,
            ObjectMaterial::Dynamic(_)
        ));
        let (entity, material) = dynamic.objects[&2].parts[0].clone();
        let mesh = dynamic.app.world().get::<Mesh3d>(entity).unwrap().0.clone();
        let buffer = dynamic.objects[&2].buffers[0].buffer().id();
        let mut original = None;
        material.edit(dynamic.app.world_mut(), |m| {
            original = Some((m.double_sided, m.cull_mode));
        });
        let options = RenderOptions {
            force_two_sided: true,
            ..Default::default()
        };
        dynamic.begin_render(options.clone());
        reference.begin_render(options);
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
            dynamic.sync_deforming(&objects(&meshes), &ids);
            reference.sync(&objects(&meshes));
            let expected = render(&mut reference, false);
            let actual = render(&mut dynamic, false);
            assert_eq!(
                actual.as_raw(),
                expected.as_raw(),
                "production sync pose {pose}"
            );
            assert_eq!(dynamic.objects[&2].parts[0].0, entity);
            assert_eq!(dynamic.objects[&2].parts[0].1.id(), material.id());
            assert_eq!(dynamic.app.world().get::<Mesh3d>(entity).unwrap().0, mesh);
            assert_eq!(dynamic.objects[&2].buffers[0].buffer().id(), buffer);
            let expected_bounds = mesh_data(&meshes[1]).aabb().unwrap();
            let bounds = dynamic.app.world().get::<Aabb>(entity).unwrap();
            assert_eq!(bounds.center, expected_bounds.center);
            assert_eq!(bounds.half_extents, expected_bounds.half_extents);
            material.edit(dynamic.app.world_mut(), |m| {
                assert!(m.double_sided);
                assert_eq!(m.cull_mode, None);
            });
        }
        dynamic.end_render();
        material.edit(dynamic.app.world_mut(), |m| {
            assert_eq!((m.double_sided, m.cull_mode), original.unwrap());
        });
        let hidden = vec![meshes[1].clone()];
        dynamic.sync_hidden_deforming(&objects(&hidden), &HashSet::from([1]));
        let hidden_entity = dynamic.hidden[&1].parts[0].0;
        let mut hidden_next = hidden;
        loom_mesh::edit_ops::translate_mesh(&mut hidden_next[0], glam::Vec3::X).unwrap();
        dynamic.sync_hidden_deforming(&objects(&hidden_next), &HashSet::from([1]));
        assert_eq!(dynamic.hidden[&1].parts[0].0, hidden_entity);
        meshes[1] = loom_mesh::sphere_segmented(1.5, 12, 0.0);
        dynamic.sync_deforming(&objects(&meshes), &ids);
        assert_ne!(
            dynamic.objects[&2].parts[0].0, entity,
            "topology edit rebuilds the immutable mesh"
        );
        let before_reload = dynamic.objects[&2].parts[0].1.id();
        dynamic.reload_bitmaps();
        dynamic.sync_deforming(&objects(&meshes), &ids);
        assert_ne!(
            dynamic.objects[&2].parts[0].1.id(),
            before_reload,
            "bitmap reload rebuilds material textures"
        );
    }

    #[test]
    fn packed_attributes_preserve_bits_and_reject_invalid_frames() {
        let source = loom_mesh::box_primitive(glam::Vec3::splat(3.0));
        let mut mesh = posed_mesh(&source);
        let (expected, _, count, _) = pack_reference(&mesh).unwrap();
        let mut packed = Vec::new();
        let mut positions = Vec::new();
        pack_into(&mesh, &mut packed, &mut positions).unwrap();
        let bytes: &[u8] = bytemuck::cast_slice(&packed);
        assert_eq!(bytes, expected);
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
        assert!(pack_into(&mesh, &mut packed, &mut positions).is_err());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; count - 1]);
        assert!(pack_into(&mesh, &mut packed, &mut positions).is_err());
    }

    #[test]
    fn storage_eligibility_checks_empty_overflow_and_exact_device_boundaries() {
        let mut limits = wgpu::Limits::default();
        limits.max_storage_buffer_binding_size = 64;
        assert!(!DynamicGeometryBuffer::supported(&limits, 0));
        assert!(DynamicGeometryBuffer::supported(&limits, 1));
        assert!(!DynamicGeometryBuffer::supported(&limits, 2));
        assert!(!DynamicGeometryBuffer::supported(&limits, usize::MAX));
        limits.max_buffer_size = 63;
        assert!(!DynamicGeometryBuffer::supported(&limits, 1));
        limits.max_buffer_size = 64;
        limits.max_storage_buffers_per_shader_stage = 1;
        assert!(!DynamicGeometryBuffer::supported(&limits, 1));
        let empty = mesh_data(&loom_mesh::Mesh::new()).to_bevy();
        assert!(
            validate_geometry(&empty).is_ok(),
            "empty geometry is valid for the ordinary path"
        );
    }

    #[test]
    fn storage_ineligible_deforming_objects_keep_full_fidelity_standard_rendering() {
        let mut dynamic = renderer();
        let mut reference = renderer();
        let mut meshes = vec![loom_mesh::box_primitive(glam::Vec3::splat(3.0))];
        let ids = HashSet::from([1]);
        let mut limits = dynamic.device.limits();
        limits.max_storage_buffer_binding_size = 64;
        for pose in 0..2 {
            if pose == 1 {
                loom_mesh::edit_ops::translate_mesh(&mut meshes[0], glam::Vec3::X).unwrap();
            }
            let list = objects(&meshes);
            let mut set = std::mem::take(&mut dynamic.objects);
            dynamic.bounds = dynamic.sync_set(&mut set, &list, false, &ids, &limits);
            dynamic.objects = set;
            assert!(!dynamic.objects[&1].dynamic);
            assert!(dynamic.objects[&1].buffers.is_empty());
            assert!(matches!(
                dynamic.objects[&1].parts[0].1,
                ObjectMaterial::Standard(_)
            ));
            let entity = dynamic.objects[&1].parts[0].0;
            let mut set = std::mem::take(&mut dynamic.objects);
            dynamic.bounds = dynamic.sync_set(&mut set, &list, false, &ids, &limits);
            dynamic.objects = set;
            assert_eq!(
                dynamic.objects[&1].parts[0].0, entity,
                "unchanged ineligible geometry must not rebuild"
            );
            reference.sync(&list);
            let expected = render(&mut reference, false);
            let actual = render(&mut dynamic, false);
            assert!(
                actual.pixels().any(|p| p[3] != 0),
                "all geometry must still render"
            );
            assert_eq!(
                actual.as_raw(),
                expected.as_raw(),
                "ineligible pose {pose} retains full quality"
            );
        }
        dynamic.sync_deforming(&objects(&meshes), &ids);
        assert!(
            dynamic.objects[&1].dynamic,
            "normal device limits still enable storage"
        );
        meshes[0] = loom_mesh::Mesh::new();
        dynamic.sync_deforming(&objects(&meshes), &ids);
        assert!(!dynamic.objects[&1].dynamic);
        assert!(dynamic.objects[&1].buffers.is_empty());
        let empty_image = render(&mut dynamic, false);
        assert!(
            empty_image.pixels().all(|p| p[3] == 0),
            "valid empty Skin geometry must render empty without panicking"
        );
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
        posed_storage_render_parity(false);
    }

    #[test]
    fn gpu_committed_storage_matches_cpu_mesh_pbr_prepass_shadows_and_allocator_offsets() {
        posed_storage_render_parity(true);
    }

    fn posed_storage_render_parity(gpu_commit: bool) {
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
                let bounds = if gpu_commit {
                    let (bytes, _, _, _) = pack_reference(&posed).unwrap();
                    let candidate = dynamic.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("render transport candidate"), contents: &bytes,
                        usage: wgpu::BufferUsages::COPY_SRC,
                    });
                    let VertexAttributeValues::Float32x3(positions) = posed.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!() };
                    let indices = posed.indices().map(|v| v.iter().map(|v| v as u32).collect::<Vec<_>>());
                    let mut encoder = dynamic.device.create_command_encoder(&Default::default());
                    let bounds = buffers[i].commit_gpu(&mut encoder, &candidate, 0, positions, indices.as_deref()).unwrap();
                    dynamic.queue.submit([encoder.finish()]);
                    bounds
                } else {
                    buffers[i].update(&dynamic.queue, &posed).unwrap()
                };
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
                        m.opaque_render_method = if deferred {
                            OpaqueRendererMethod::Deferred
                        } else {
                            OpaqueRendererMethod::Forward
                        };
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
                    for buffer in &mut buffers {
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

#[cfg(test)]
mod fused_position_tests {
    use super::*;
    #[test]
    fn prepared_current_positions_keep_order_bits_bounds_and_reject_nonfinite() {
        let positions = [[-0.0, 2.0, -3.0], [0.0, -4.0, 5.0], [7.0, 0.0, -0.0]];
        let mut prepared = PreparedPositions::default();
        prepared.gather(positions.into_iter()).unwrap();
        let mut lo = glam::Vec3::splat(f32::INFINITY);
        let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
        for p in positions { lo = lo.min(p.into()); hi = hi.max(p.into()); }
        assert_eq!(prepared.min.to_array().map(f32::to_bits), lo.to_array().map(f32::to_bits));
        assert_eq!(prepared.max.to_array().map(f32::to_bits), hi.to_array().map(f32::to_bits));
        assert_eq!(prepared.positions.iter().map(|p| p.map(f32::to_bits)).collect::<Vec<_>>(), positions.map(|p| [p[0],p[1],p[2],1.0].map(f32::to_bits)));
        assert!(prepared.gather([[f32::NAN, 0.0, 0.0]].into_iter()).is_err());
        assert!(!prepared.valid);
        prepared.gather(positions.into_iter()).unwrap();
        assert!(prepared.valid);
        prepared.gather(std::iter::empty()).unwrap();
        assert!(prepared.valid);
    }
    #[test]
    fn fused_commit_rejects_later_layout_and_guard_without_partial_history_then_recovers() {
        use wgpu::util::DeviceExt;
        use crate::gpu_pose_commit::{commit_pose, GuardReadback, PoseCandidate, PoseCommitError};
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).expect("real GPU");
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default(), None)).unwrap();
        let initial = crate::mesh_data(&loom_mesh::box_primitive(glam::Vec3::ONE));
        let mut current = initial.clone();
        for p in &mut current.positions { p[0] += 3.0; }
        let mut targets: Vec<_> = (0..2).map(|_| DynamicGeometryBuffer::new(&device, &initial.to_bevy()).unwrap().0).collect();
        let output = DynamicGeometryBuffer::new(&device, &current.to_bevy()).unwrap().0;
        let guards: Vec<_> = [0u32, 8].iter().map(|flag| device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::bytes_of(flag), usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST })).collect();
        let mut reader = GuardReadback::new(&device, 2).unwrap();
        let mut prepared = [PreparedPositions::default(), PreparedPositions::default()];
        let before: Vec<_> = targets.iter().map(|t| (t.last_positions.clone(), t.history_needs_advance)).collect();
        for stage in 0..3 {
            for (i, p) in prepared.iter_mut().enumerate() { p.gather(current.positions[if stage == 0 && i == 1 { 1 } else { 0 }..].iter().copied()).unwrap(); }
            let candidates = targets.iter_mut().zip(&guards).zip(&mut prepared).map(|((target, guard), p)| PoseCandidate { target, guard, output: output.buffer(), offset: 0, positions: &[], prepared: Some(p), indices: Some(&current.indices) }).collect();
            let result = commit_pose(&device, &queue, &mut reader, candidates);
            if stage == 0 { assert!(matches!(result, Err(PoseCommitError::Layout { part: 1, .. }))); }
            else if stage == 1 { assert!(matches!(result, Err(PoseCommitError::Guards(_)))); }
            else { assert!(result.is_ok()); }
            for (i, target) in targets.iter().enumerate() {
                if stage < 2 { assert_eq!((target.last_positions.clone(), target.history_needs_advance), before[i]); assert!(prepared[i].valid); }
                else {
                    assert_eq!(target.last_positions.iter().map(|p| p.map(f32::to_bits)).collect::<Vec<_>>(), current.positions.iter().map(|p| [p[0],p[1],p[2],1.0].map(f32::to_bits)).collect::<Vec<_>>());
                    assert!(target.history_needs_advance); assert!(!prepared[i].valid);
                }
            }
            if stage == 1 { queue.write_buffer(&guards[1], 0, bytemuck::bytes_of(&0u32)); }
        }
        // A consumed proof cannot publish swapped-out old positions twice.
        let candidates = targets.iter_mut().zip(&guards).zip(&mut prepared).map(|((target, guard), p)| PoseCandidate { target, guard, output: output.buffer(), offset: 0, positions: &[], prepared: Some(p), indices: Some(&current.indices) }).collect();
        assert!(matches!(commit_pose(&device, &queue, &mut reader, candidates), Err(PoseCommitError::Layout { part: 0, .. })));
        let read_history = |target: &DynamicGeometryBuffer| {
            let size = target.last_positions.len() as u64 * 16;
            let staging = device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            let mut encoder = device.create_command_encoder(&Default::default()); encoder.copy_buffer_to_buffer(&target.previous, 0, &staging, 0, size); queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel(); staging.slice(..).map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap()); device.poll(wgpu::Maintain::Wait); rx.recv().unwrap().unwrap();
            let bytes = staging.slice(..).get_mapped_range().to_vec(); staging.unmap(); bytes
        };
        for (i, target) in targets.iter_mut().enumerate() {
            target.advance_history(&queue);
            assert!(!target.history_needs_advance);
            assert_eq!(read_history(target), bytemuck::cast_slice::<_, u8>(&target.last_positions));
            // Repeated same pose refills the consumed proof before commit.
            prepared[i].gather(current.positions.iter().copied()).unwrap();
            let mut encoder = device.create_command_encoder(&Default::default());
            target.prepare_gpu_fused(output.buffer(), 0, &mut prepared[i], Some(&current.indices)).unwrap().commit(&mut encoder);
            queue.submit([encoder.finish()]); target.advance_history(&queue);
            assert_eq!(read_history(target), bytemuck::cast_slice::<_, u8>(&target.last_positions));
            // Ordinary CPU fallback then advances the same history stream.
            target.update(&queue, &initial.to_bevy()).unwrap(); target.advance_history(&queue);
            target.assert_current_history_bits(&initial.positions);
            assert_eq!(read_history(target), bytemuck::cast_slice::<_, u8>(&target.last_positions));
        }
    }

    #[test]
    #[ignore = "actual 317 CPU poses require LOOM_PERF_SCENE"]
    fn actual_ray_317_fused_position_history_bounds_bits() {
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(std::env::var("LOOM_PERF_SCENE").unwrap()).unwrap()).unwrap();
        let scene: loom_scene::Scene = serde_json::from_value(value["scene"].clone()).unwrap();
        let ids: Vec<_> = scene.nodes().filter(|n| n.modifiers.iter().any(|m| matches!(&m.modifier, loom_scene::Modifier::Skin(_)))).map(|n| n.id).collect();
        assert_eq!(ids.len(), 5);
        let mut prepared = PreparedPositions::default();
        let mut parts = 0; let mut vertices = 0;
        for time in (0..=157).map(|t| t as f32).chain([39.25]).chain((0..=157).map(|t| t as f32 + 0.5)) {
            scene.set_time(time);
            let mut pose_parts = 0;
            for id in &ids {
                let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
                loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(*id)).unwrap();
                for (_, data) in crate::convert::mesh_data_by_id(&mesh) {
                    prepared.gather(data.positions.iter().copied()).unwrap();
                    let mut lo = glam::Vec3::splat(f32::INFINITY); let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
                    let mut history = Vec::new();
                    for p in &data.positions { lo = lo.min((*p).into()); hi = hi.max((*p).into()); history.push([p[0],p[1],p[2],1.0]); }
                    assert_eq!(prepared.min.to_array().map(f32::to_bits), lo.to_array().map(f32::to_bits));
                    assert_eq!(prepared.max.to_array().map(f32::to_bits), hi.to_array().map(f32::to_bits));
                    assert_eq!(bytemuck::cast_slice::<_,u8>(&prepared.positions), bytemuck::cast_slice::<_,u8>(&history));
                    vertices += data.positions.len(); pose_parts += 1;
                }
            }
            assert_eq!(pose_parts, 13); parts += pose_parts;
        }
        assert_eq!(parts, 4121); eprintln!("FUSED_CPU_POSITION_PARITY poses=317 parts={parts} vertices={vertices}");
    }

}

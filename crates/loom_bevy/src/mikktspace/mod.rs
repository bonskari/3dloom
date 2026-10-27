//! Prepared parallel MikkTSpace with corrected final neighbor-sort buckets.
//! Global current-pose welding and tangent arithmetic are retained. Guarded
//! manifold adjacency reuse never substitutes a rest-pose tangent.
use crate::convert::MeshData;
use bevy_mikktspace::Geometry;
use glam::{Vec2, Vec3};

mod generated;
mod neighbor_cache;
pub(crate) use neighbor_cache::NeighborCache;

struct MeshGeometry<'a> {
    data: &'a MeshData,
    tangents: Vec<[f32; 4]>,
}

impl Geometry for MeshGeometry<'_> {
    fn num_faces(&self) -> usize { self.data.indices.len() / 3 }
    fn num_vertices_of_face(&self, _: usize) -> usize { 3 }
    fn position(&self, face: usize, corner: usize) -> [f32; 3] { self.data.positions[self.data.indices[face * 3 + corner] as usize] }
    fn normal(&self, face: usize, corner: usize) -> [f32; 3] { self.data.normals[self.data.indices[face * 3 + corner] as usize] }
    fn tex_coord(&self, face: usize, corner: usize) -> [f32; 2] { self.data.uvs[self.data.indices[face * 3 + corner] as usize] }
    fn set_tangent_encoded(&mut self, tangent: [f32; 4], face: usize, corner: usize) { self.tangents[self.data.indices[face * 3 + corner] as usize] = tangent; }
}

struct ReadGeometry<'a, I>(&'a I);
impl<I: Geometry> Geometry for ReadGeometry<'_, I> {
    fn num_faces(&self) -> usize { self.0.num_faces() }
    fn num_vertices_of_face(&self, face: usize) -> usize { self.0.num_vertices_of_face(face) }
    fn position(&self, face: usize, corner: usize) -> [f32; 3] { self.0.position(face, corner) }
    fn normal(&self, face: usize, corner: usize) -> [f32; 3] { self.0.normal(face, corner) }
    fn tex_coord(&self, face: usize, corner: usize) -> [f32; 2] { self.0.tex_coord(face, corner) }
}

#[cfg(test)]
pub(crate) fn generate_parallel(pool: &bevy_tasks::TaskPool, data: &MeshData) -> Option<Vec<[f32; 4]>> {
    generate_cached_inner::<true>(pool, data, None)
}

#[cfg(test)]
pub(crate) fn generate_serial(data: &MeshData) -> Option<Vec<[f32; 4]>> {
    let mut geometry = MeshGeometry { data, tangents: vec![[0.0; 4]; data.positions.len()] };
    if !unsafe { generated::genTangSpace(&mut geometry, 180.0, None) } { return None; }
    for tangent in &mut geometry.tangents { tangent[3] = -tangent[3]; }
    Some(geometry.tangents)
}

pub(crate) fn generate_cached(pool: &bevy_tasks::TaskPool, data: &MeshData, cache: &mut NeighborCache) -> Option<Vec<[f32; 4]>> {
    generate_cached_inner::<true>(pool, data, Some(cache))
}

#[cfg(test)]
fn generate_cached_original(pool: &bevy_tasks::TaskPool, data: &MeshData, cache: &mut NeighborCache) -> Option<Vec<[f32; 4]>> {
    generate_cached_inner::<false>(pool, data, Some(cache))
}

fn generate_cached_inner<const REUSE_MEMBERS: bool>(pool: &bevy_tasks::TaskPool, data: &MeshData, cache: Option<&mut NeighborCache>) -> Option<Vec<[f32; 4]>> {
    let mut geometry = MeshGeometry { data, tangents: vec![[0.0; 4]; data.positions.len()] };
    // The vendor's original preparation/epilogue retain their checked Geometry
    // indexing and pointer invariants. Worker boundaries contain owned indices.
    if !unsafe { generated::genTangSpaceCached::<_, REUSE_MEMBERS>(&mut geometry, 180.0, Some(pool), cache) } { return None; }
    // Match Bevy's existing handedness conversion, including unwritten vertices.
    for tangent in &mut geometry.tangents { tangent[3] = -tangent[3]; }
    Some(geometry.tangents)
}

fn get_position<I: Geometry>(geometry: &mut I, index: usize) -> Vec3 { geometry.position(index >> 2, index & 3).into() }
fn get_normal<I: Geometry>(geometry: &mut I, index: usize) -> Vec3 { geometry.normal(index >> 2, index & 3).into() }
fn get_tex_coord<I: Geometry>(geometry: &mut I, index: usize) -> Vec3 { Vec2::from(geometry.tex_coord(index >> 2, index & 3)).extend(1.0) }
fn face_vert_to_index(face: usize, corner: usize) -> usize { face << 2 | corner & 3 }

#[cfg(test)]
mod tests;

#[cfg(test)]
mod capture;

#[cfg(test)]
pub(crate) use capture::{enable_group_capture, take_group_captures, GroupFace, GroupFixture};

//! Half-edge mesh data structure and modeling operations.

pub mod deform;
pub mod edit_ops;
pub mod csg;
pub mod ffd;
pub mod geom_ops;
pub mod indexed;
pub mod half_edge;
pub mod mesh;
pub mod mesh_mods;
pub mod obj;
pub mod stl;
pub mod tds;
pub mod dxf;
pub mod uvmap;
pub mod unwrap;
pub mod optimize;
pub mod pc2;
pub mod primitives;
pub mod select_ops;
pub mod soft_select;
pub mod poly_ops;
pub mod topo_ops;
pub mod tri_mesh;
pub mod patch;
pub mod standard_primitives;
mod teapot_data;

pub use half_edge::{Face, FaceId, HalfEdge, HalfEdgeId, Vertex, VertexId};
pub use mesh::{HalfEdgeSnapshot, Mesh, Polygon};

/// The map channel that carries explicit normals (Edit Normals): one
/// vector per corner, zero where the normal is computed from smoothing
/// groups. It rides through every edit like a texture channel; exporters
/// write it as normals, never as a UV set.
pub const NORMALS_CHANNEL: u32 = u32::MAX - 1;
pub use standard_primitives::{
    bone_mesh, dummy_mesh, BoneFinParams,
    box_segmented, build_merged, cone_primitive, cylinder_segmented, geosphere_primitive, plane_segmented,
    pyramid_primitive, sphere_full, sphere_segmented, teapot_primitive, torus_full, torus_primitive, torus_smoothed, tube_primitive, GeoBase, Slice,
    TeapotParts, TorusSmooth,
};
pub use primitives::{
    box_primitive, cylinder_primitive, quad_primitive, sphere_primitive, test_room,
};

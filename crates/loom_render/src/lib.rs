//! wgpu-based viewport renderer.

pub mod camera;
pub mod grid;
pub mod gpu_edges;
pub mod mesh_buffer;
pub mod pick;
pub mod profile;
pub mod viewport;

pub use camera::Camera;
pub use grid::{GridParams, GridPipeline};
pub use mesh_buffer::{triangulate_styled, triangulate_with_face_normals, Bitmap, DrawRange, GpuVertex, LineBuffer, MeshBuffer, MeshStyle, SubStyles, SurfaceStyle, TextureSel, UV_IDENTITY};
pub use viewport::{GridPlane, TextureSet, ViewLight, ViewLightKind, ViewShading, ViewportRenderer, MAX_VIEWPORTS, MAX_VIEW_LIGHTS};

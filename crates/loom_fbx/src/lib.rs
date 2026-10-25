//! FBX in and out of 3dloom: `read` through ufbx (any version, binary or
//! ASCII, converted to the scene's unit and Z-up) and `write`, an own
//! FBX 7.4 / 7.5 writer (binary or ASCII) Unreal, Blender and ufbx read back.

pub mod ascii;
pub mod binary;
pub mod curves;
pub mod document;
pub mod read;
pub mod write;

pub use document::{FbxAnim, FbxBlendShape, FbxDocument, FbxKind, FbxLayer, FbxLight, FbxNode, FbxSkin, FbxTake, WriteAxes, WriteOpts};
pub use document::{CurveFilters, FbxInfo, FbxMaterial, FbxTexture, ReadOpts};
pub use read::{info, read, read_with};
pub use write::{material_names, write, write_raw_faces};

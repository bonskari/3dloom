//! FBX in and out of 3dloom: `read` through ufbx (any version, binary or
//! ASCII, converted to the scene's unit and Z-up) and `write`, an own
//! FBX 7.4 binary writer Unreal and ufbx read back.

pub mod binary;
pub mod document;
pub mod read;
pub mod write;

pub use document::{FbxAnim, FbxBlendShape, FbxDocument, FbxKind, FbxLight, FbxNode, FbxSkin, FbxTake, WriteAxes, WriteOpts};
pub use read::read;
pub use write::{material_names, write, write_raw_faces};

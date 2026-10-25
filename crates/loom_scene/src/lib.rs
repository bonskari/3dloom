//! Scene graph: nodes with animatable parameters, parametric objects, undo.

pub mod biped;
pub mod camera;
pub mod light;
pub mod modifier;
pub mod ik;
pub mod skin;
pub mod morph;
pub mod node;
pub mod anim;
pub mod constraint;
pub mod edit_normals;
pub mod select_mods;
pub mod deform9;
pub mod mapping9;
pub mod caches9;
pub mod param;
pub mod tracks;
pub mod scene;
pub mod material;
pub mod maps;
pub mod render;

pub use node::{Axes3, DisplayProps, TextureDisplay, Node, NodeId, NodeProps, ObjectKind, PaintDeform, PaintDirection, ParamValue, PolyObject, ShapeCommon, ShapeObject, SoftSelection, SubdivSurface, Transform, OBJECT_COLORS};
pub use param::{Controller, Param};
pub use scene::{Layer, Scene};
pub use material::{Map, MaterialEditorSlots, MaterialId, MaterialLibrary, StandardMaterial};
pub use modifier::{EvalCtx, Evaluated, Modifier, ModifierSlot, StackValue, StoredSelection, UnwrapUvw};
pub mod change;
pub mod undo;
pub use change::Change;
pub use undo::UndoStack;

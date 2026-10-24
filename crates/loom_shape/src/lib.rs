//! Splines and shapes (Editable Spline / Line / parametric shapes):
//! Bezier knots, interpolation, 2D spline operations and the viewport
//! rendering tube.

pub mod boolean2d;
pub mod interp;
pub mod loft;
pub mod mesh_from_shape;
pub mod outline;
pub mod parametric;
pub mod spline;
pub mod trim;

pub use boolean2d::{boolean, BoolOp};
pub use spline::{Knot, KnotKind, RenderParams, Shape, Spline};

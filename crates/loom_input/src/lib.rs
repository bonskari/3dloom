//! Framework-agnostic input events and the viewport camera controller.

pub mod camera_control;
pub mod event;

pub use camera_control::{ViewportNavigator, NavMode, OrbitCamera};
pub use event::{InputEvent, Modifiers, MouseButton};

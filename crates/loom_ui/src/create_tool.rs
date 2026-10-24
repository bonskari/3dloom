//! Interactive creation of standard primitives, step by step
//! (e.g. Box: drag the base, release, move the mouse for the height, click).
//! The object exists from the first drag and its parameters follow the
//! mouse; the whole creation is one undo entry.

use crate::{CreateKind, LineDrag, LoomApp};
use eframe::egui;
use glam::Vec3;
use loom_commands::{Command, PrimitiveKind, ShapeKind};
use loom_scene::NodeId;

/// Creation Method rollout choices of a standard primitive, in rollout order.
pub fn creation_methods(kind: PrimitiveKind) -> &'static [&'static str] {
    match kind {
        PrimitiveKind::Box => &["Cube", "Box"],
        PrimitiveKind::GeoSphere => &["Diameter", "Center"],
        PrimitiveKind::Pyramid => &["Base/Apex", "Center"],
        PrimitiveKind::Plane => &["Rectangle", "Square"],
        _ => &["Edge", "Center"],
    }
}

/// The choice selected when the rollout has not been touched.
fn default_creation_method(kind: PrimitiveKind) -> usize {
    match kind {
        PrimitiveKind::Pyramid | PrimitiveKind::Plane => 0,
        _ => 1,
    }
}

fn kind_index(kind: PrimitiveKind) -> usize {
    PrimitiveKind::ALL.iter().position(|k| *k == kind).unwrap_or(0)
}

impl LoomApp {
    /// The Creation Method in use for `kind`.
    pub fn creation_method(&self, kind: PrimitiveKind) -> &'static str {
        let methods = creation_methods(kind);
        methods[self.creation_method[kind_index(kind)].unwrap_or(default_creation_method(kind)).min(methods.len() - 1)]
    }

    /// Picks a Creation Method by its rollout label; unknown labels are ignored.
    pub fn set_creation_method(&mut self, kind: PrimitiveKind, method: &str) {
        if let Some(i) = creation_methods(kind).iter().position(|m| *m == method) {
            self.creation_method[kind_index(kind)] = Some(i);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Button held: dragging the base or the first radius.
    Dragging,
    /// Mouse moves set the height (Box, Pyramid, Cylinder, Cone, Tube) or the
    /// second radius (Torus); click confirms.
    Second,
    /// Cone: second radius. Tube: height. Click confirms.
    Third,
}

#[derive(Debug, Clone)]
pub struct CreateSession {
    pub kind: PrimitiveKind,
    /// A shape instead of a primitive.
    pub shape: Option<ShapeKind>,
    pub node: NodeId,
    /// Undo label of this creation's transaction.
    label: String,
    step: Step,
    pub anchor: Vec3,
    /// Construction plane basis (right, up, normal) the object is built on.
    basis: (Vec3, Vec3, Vec3),
    /// Pointer position where the current step began.
    step_origin: egui::Pos2,
    /// World units per screen pixel at the anchor, for height/radius steps.
    units_per_px: f32,
    radius1: f32,
    height: f32,
    /// Creation Method rollout choice for this object.
    method: &'static str,
}

impl CreateSession {
    /// Starts creation at a construction-plane point. Returns None when the
    /// node could not be created.
    pub fn begin(app: &mut LoomApp, what: CreateKind, anchor: Vec3, basis: (Vec3, Vec3, Vec3), pointer: egui::Pos2, units_per_px: f32) -> Option<Self> {
        let (kind, shape) = match what {
            CreateKind::Primitive(k) => (k, None),
            CreateKind::Shape(sk) => (PrimitiveKind::Box, Some(sk)),
            CreateKind::Line | CreateKind::Bones | CreateKind::Helper(_) | CreateKind::Camera { .. } => return None,
        };
        let label = format!("Create {}", shape.map(|s| s.label()).unwrap_or(kind.label()));
        app.state.undo.begin(&label);
        let created = match shape {
            Some(sk) => app.dispatch(Command::CreateShape { kind: sk, position: anchor.to_array(), size: [0.0, 0.0, 0.0] }).ok(),
            None => app.dispatch(Command::CreatePrimitive { kind, position: anchor.to_array(), size: [0.0, 0.0, 0.0] }).ok(),
        };
        let node = created.and_then(|_| app.state.selection.iter().next().copied());
        let Some(node) = node else {
            app.state.undo.end();
            return None;
        };
        let method = if shape.is_some() { "" } else { app.creation_method(kind) };
        let session = Self { kind, shape, node, label, step: Step::Dragging, anchor, basis, step_origin: pointer, units_per_px, radius1: 0.0, height: 0.0, method };
        // Orient the object to the construction plane (local Z = plane normal).
        let rot = glam::Quat::from_rotation_arc(Vec3::Z, basis.2);
        let e = rot.to_euler(glam::EulerRot::XYZ);
        let _ = app.dispatch(Command::SetTransform { id: node.0, position: None, rotation_deg: Some([e.0.to_degrees(), e.1.to_degrees(), e.2.to_degrees()]), scale: None });
        Some(session)
    }

    /// Plane-space coordinates (right, up) of a world point relative to the anchor.
    fn plane_uv(&self, g: Vec3) -> (f32, f32) {
        let d = g - self.anchor;
        (d.dot(self.basis.0), d.dot(self.basis.1))
    }

    /// Normal of the construction plane this session builds on.
    pub fn normal(&self) -> Vec3 {
        self.basis.2
    }

    fn set(&self, app: &mut LoomApp, name: &str, value: f32) {
        let _ = app.dispatch(Command::SetParameter { id: self.node.0, name: name.into(), value: value as f64 });
    }

    fn place(&self, app: &mut LoomApp, position: Vec3) {
        let _ = app.dispatch(Command::SetTransform { id: self.node.0, position: Some(position.to_array()), rotation_deg: None, scale: None });
    }

    /// Pointer moved; `ground` is the pointer on the construction plane.
    pub fn on_move(&mut self, app: &mut LoomApp, ground: Option<Vec3>, pointer: egui::Pos2) {
        use PrimitiveKind as K;
        let dy_px = self.step_origin.y - pointer.y;
        let dx_px = pointer.x - self.step_origin.x;
        if let Some(sk) = self.shape {
            self.shape_move(app, sk, ground, dx_px, dy_px);
            return;
        }
        match self.step {
            Step::Dragging => {
                let Some(g) = ground else { return };
                match self.kind {
                    K::Box | K::Plane if matches!(self.method, "Cube" | "Square") => {
                        // Equal sides, corner to corner towards the pointer.
                        let (u, v) = self.plane_uv(g);
                        let side = u.abs().max(v.abs());
                        let centre = self.anchor + self.basis.0 * (side * 0.5 * u.signum()) + self.basis.1 * (side * 0.5 * v.signum());
                        self.place(app, centre);
                        self.set(app, "Width", side);
                        self.set(app, "Length", side);
                        if self.kind == K::Box {
                            self.set(app, "Height", side);
                        }
                    }
                    K::Box | K::Plane | K::Pyramid => {
                        let (u, v) = self.plane_uv(g);
                        // Base/Apex and Box draw corner to corner; Center draws from the centre out.
                        let from_centre = self.method == "Center";
                        let centre = if from_centre { self.anchor } else { self.anchor + self.basis.0 * (u * 0.5) + self.basis.1 * (v * 0.5) };
                        self.place(app, centre);
                        let (w, l) = if from_centre { (u.abs() * 2.0, v.abs() * 2.0) } else { (u.abs(), v.abs()) };
                        match self.kind {
                            K::Pyramid => {
                                self.set(app, "Width", w);
                                self.set(app, "Depth", l);
                            }
                            _ => {
                                self.set(app, "Width", w);
                                self.set(app, "Length", l);
                            }
                        }
                    }
                    K::Sphere | K::GeoSphere | K::Teapot | K::Cylinder | K::Cone | K::Tube | K::Torus => {
                        if matches!(self.method, "Edge" | "Diameter") {
                            // Edge to edge: the centre follows the midpoint of the drag.
                            self.place(app, (self.anchor + g) * 0.5);
                            self.radius1 = (g - self.anchor).length() * 0.5;
                        } else {
                            self.radius1 = (g - self.anchor).length();
                        }
                        let name = match self.kind {
                            K::Cone | K::Tube | K::Torus => "Radius 1",
                            _ => "Radius",
                        };
                        self.set(app, name, self.radius1);
                        if self.kind == K::Tube {
                            self.set(app, "Radius 2", self.radius1 * 0.75);
                        }
                        if self.kind == K::Torus {
                            self.set(app, "Radius 2", self.radius1 * 0.25);
                        }
                    }
                }
            }
            Step::Second => match self.kind {
                K::Box | K::Pyramid | K::Cylinder | K::Cone => {
                    self.height = dy_px * self.units_per_px;
                    self.set(app, "Height", self.height);
                }
                K::Tube => {
                    let r2 = (self.radius1 + dx_px * self.units_per_px).max(0.0);
                    self.set(app, "Radius 2", r2);
                }
                K::Torus => {
                    let r2 = (dx_px.abs() * self.units_per_px).max(0.0);
                    self.set(app, "Radius 2", r2);
                }
                _ => {}
            },
            Step::Third => match self.kind {
                K::Cone => {
                    // Starts at Radius 1; moving down narrows the top towards a point.
                    let r2 = (self.radius1 + dy_px * self.units_per_px).max(0.0);
                    self.set(app, "Radius 2", r2);
                }
                K::Tube => {
                    self.height = dy_px * self.units_per_px;
                    self.set(app, "Height", self.height);
                }
                _ => {}
            },
        }
    }

    /// Shapes: Rectangle / Ellipse drag corner to corner; the round ones drag
    /// a radius; Donut, Star and Helix then take a second radius (Helix a
    /// height first) with mouse moves.
    fn shape_move(&mut self, app: &mut LoomApp, sk: ShapeKind, ground: Option<Vec3>, dx_px: f32, dy_px: f32) {
        match self.step {
            Step::Dragging => {
                let Some(g) = ground else { return };
                match sk {
                    ShapeKind::Rectangle | ShapeKind::Ellipse => {
                        let (u, v) = self.plane_uv(g);
                        self.place(app, self.anchor + self.basis.0 * (u * 0.5) + self.basis.1 * (v * 0.5));
                        self.set(app, "Width", u.abs());
                        self.set(app, "Length", v.abs());
                    }
                    _ => {
                        self.radius1 = (g - self.anchor).length();
                        let name = if matches!(sk, ShapeKind::Donut | ShapeKind::Star | ShapeKind::Helix) { "Radius 1" } else { "Radius" };
                        self.set(app, name, self.radius1);
                        match sk {
                            ShapeKind::Donut => self.set(app, "Radius 2", self.radius1 * 0.5),
                            ShapeKind::Star => self.set(app, "Radius 2", self.radius1 * 0.5),
                            ShapeKind::Helix => self.set(app, "Radius 2", self.radius1),
                            _ => {}
                        }
                    }
                }
            }
            Step::Second => match sk {
                ShapeKind::Donut | ShapeKind::Star => self.set(app, "Radius 2", (self.radius1 + dx_px * self.units_per_px).max(0.0)),
                ShapeKind::Helix => {
                    self.height = dy_px * self.units_per_px;
                    self.set(app, "Height", self.height);
                }
                _ => {}
            },
            Step::Third => {
                if sk == ShapeKind::Helix {
                    self.set(app, "Radius 2", (self.radius1 + dx_px * self.units_per_px).max(0.0));
                }
            }
        }
    }

    /// Button released after the first drag. Returns true when creation is done.
    pub fn on_release(&mut self, app: &mut LoomApp, pointer: egui::Pos2) -> bool {
        use PrimitiveKind as K;
        if self.step != Step::Dragging {
            return false;
        }
        if let Some(sk) = self.shape {
            return match sk {
                ShapeKind::Donut | ShapeKind::Star | ShapeKind::Helix => {
                    self.step = Step::Second;
                    self.step_origin = pointer;
                    false
                }
                _ => {
                    self.finish(app);
                    true
                }
            };
        }
        match self.kind {
            K::Plane | K::Sphere | K::GeoSphere | K::Teapot => {
                self.finish(app);
                true
            }
            // Cube is a one-step operation.
            K::Box if self.method == "Cube" => {
                self.finish(app);
                true
            }
            K::Box | K::Pyramid | K::Cylinder | K::Cone | K::Tube | K::Torus => {
                if self.kind == K::Cone {
                    // Straight sides while the height is set.
                    self.set(app, "Radius 2", self.radius1);
                }
                self.step = Step::Second;
                self.step_origin = pointer;
                false
            }
        }
    }

    /// Click during a mouse-move step. Returns true when creation is done.
    pub fn on_click(&mut self, app: &mut LoomApp, pointer: egui::Pos2) -> bool {
        use PrimitiveKind as K;
        if self.shape == Some(ShapeKind::Helix) && self.step == Step::Second {
            self.step = Step::Third;
            self.step_origin = pointer;
            return false;
        }
        match (self.step, self.kind) {
            (Step::Second, K::Cone) | (Step::Second, K::Tube) => {
                self.step = Step::Third;
                self.step_origin = pointer;
                false
            }
            (Step::Second, _) | (Step::Third, _) => {
                self.finish(app);
                true
            }
            (Step::Dragging, _) => false,
        }
    }

    fn finish(&self, app: &mut LoomApp) {
        app.state.undo.end();
    }

    /// Right-click: abandon the object being created. Only this session's
    /// own transaction is undone, never an older entry.
    pub fn cancel(&self, app: &mut LoomApp) {
        app.state.undo.end();
        if app.state.undo.labels().last().map(String::as_str) == Some(self.label.as_str()) {
            let _ = loom_commands::execute(&mut app.state, Command::UndoSceneOperation);
            app.sync_after_command();
        }
    }

    pub fn prompt(&self) -> &'static str {
        match self.step {
            Step::Dragging => "Drag to set the base size",
            Step::Second => "Move the mouse to set the height, then click",
            Step::Third => "Move the mouse, then click to finish",
        }
    }
}

/// The Line tool: each click adds a knot (Initial Type; Shift lines it up
/// with the last), a drag after the click shapes it (Drag Type), Backspace
/// removes the last knot, clicking the first knot asks "Close spline?" and a
/// right-click ends the line.
#[derive(Debug, Clone)]
pub struct LineSession {
    pub node: NodeId,
    /// Construction plane basis the knots land on.
    pub normal: Vec3,
    press: Option<egui::Pos2>,
    dragged: bool,
}

impl LineSession {
    /// First click: the Line object with its first knot; one undo entry until the line ends.
    pub fn begin(app: &mut LoomApp, point: Vec3, normal: Vec3) -> Option<Self> {
        app.state.undo.begin("Create Line");
        let ok = app.dispatch(Command::LineStart { point: point.to_array() }).is_ok();
        let node = ok.then(|| app.state.selection.iter().next().copied()).flatten();
        let Some(node) = node else {
            app.state.undo.end();
            return None;
        };
        Some(Self { node, normal, press: None, dragged: false })
    }

    /// Screen position of the first knot, to close the spline by clicking it.
    pub fn first_knot_screen(&self, app: &LoomApp, projection: &crate::gizmo::Projection) -> Option<egui::Pos2> {
        let n = app.state.scene.node(self.node)?;
        let k = n.edit_spline()?.shape.splines.first()?.knots.first()?.pos;
        projection.screen(app.state.scene.object_matrix(self.node).transform_point3(k))
    }

    /// Shift: the new knot lines up with the last one horizontally or
    /// vertically on the construction plane (`basis` = right, up, normal).
    pub fn constrained(&self, app: &LoomApp, point: Vec3, basis: (Vec3, Vec3, Vec3)) -> Vec3 {
        let Some(n) = app.state.scene.node(self.node) else { return point };
        let Some(last) = n.edit_spline().and_then(|s| s.shape.splines.first()?.knots.last().map(|k| k.pos)) else { return point };
        let last = app.state.scene.object_matrix(self.node).transform_point3(last);
        let d = point - last;
        let (u, v) = (d.dot(basis.0), d.dot(basis.1));
        if u.abs() >= v.abs() { last + basis.0 * u } else { last + basis.1 * v }
    }

    /// The line was closed (the "Close spline?" prompt was answered Yes).
    pub fn is_closed(&self, app: &LoomApp) -> bool {
        app.state.scene.node(self.node).and_then(|n| n.edit_spline().and_then(|s| s.shape.splines.first().map(|s| s.closed))).unwrap_or(false)
    }

    /// A click at `point`: on the first knot it asks "Close spline?", else adds a knot. Returns true when the line ended.
    pub fn on_press(&mut self, app: &mut LoomApp, point: Vec3, pointer: egui::Pos2, first_knot: Option<egui::Pos2>) -> bool {
        let knots = app.state.scene.node(self.node).and_then(|n| n.edit_spline().map(|s| s.shape.splines[0].knots.len())).unwrap_or(0);
        if knots >= 3 && first_knot.is_some_and(|f| f.distance(pointer) <= 8.0) {
            // Yes closes the line (and the frame loop ends it); No adds this knot.
            let _ = app.dispatch(Command::LineCloseAsk { node: self.node.0, point: point.to_array(), smooth: app.line_initial_smooth });
            self.press = None;
            return false;
        }
        let _ = app.dispatch(Command::LineAddKnot { node: self.node.0, point: point.to_array(), smooth: app.line_initial_smooth });
        self.press = Some(pointer);
        self.dragged = false;
        false
    }

    /// Mouse moved with the button down: Drag Type shapes the knot just added.
    pub fn on_drag(&mut self, app: &mut LoomApp, point: Option<Vec3>, pointer: egui::Pos2) {
        let Some(press) = self.press else { return };
        if press.distance(pointer) < 4.0 {
            return;
        }
        let Some(point) = point else { return };
        match app.line_drag {
            LineDrag::Corner => {}
            LineDrag::Smooth => {
                if !self.dragged {
                    let _ = app.dispatch(Command::LineSmoothLastKnot { node: self.node.0 });
                }
            }
            LineDrag::Bezier => {
                let _ = app.dispatch(Command::LineSetHandle { node: self.node.0, point: point.to_array() });
            }
        }
        self.dragged = true;
    }

    pub fn on_release(&mut self) {
        self.press = None;
    }

    /// Right-click: the line ends as drawn (the Line tool stays active).
    /// A line drawn for Create Line (Spline) joins that shape.
    pub fn finish(&self, app: &mut LoomApp) {
        let _ = app.dispatch(Command::LineEnd { node: self.node.0 });
        if app.state.scene.node(self.node).is_none() {
            app.state.undo.end();
            return;
        }
        if let Some(target) = app.state.spline_create_line_target {
            if target != self.node && app.state.scene.node(target).is_some() {
                let reorient = app.state.spline_settings.attach_reorient;
                app.state.selection = std::collections::BTreeSet::from([target]);
                let _ = app.dispatch(Command::SplineAttach { node: self.node.0, reorient });
            }
        }
        app.state.undo.end();
    }

    pub fn backspace(&self, app: &mut LoomApp) {
        let _ = app.dispatch(Command::LineRemoveLastKnot { node: self.node.0 });
    }
}

/// Create > Cameras > Target: the press places the camera, dragging moves
/// its target over the construction plane, the release finishes. One undo
/// entry.
#[derive(Debug, Clone)]
pub struct CameraSession {
    pub target: NodeId,
    /// Normal of the construction plane the target slides on.
    pub normal: Vec3,
}

impl CameraSession {
    pub fn begin(app: &mut LoomApp, anchor: Vec3, normal: Vec3) -> Option<Self> {
        app.state.undo.begin("Create Camera");
        let r = app.dispatch(Command::CreateCameraAt { target: true, position: anchor.to_array(), look_at: anchor.to_array() });
        let target = r.ok().and_then(|v| v["target"].as_u64()).map(NodeId);
        if target.is_none() {
            app.state.undo.end();
        }
        target.map(|target| Self { target, normal })
    }

    pub fn on_drag(&self, app: &mut LoomApp, at: Vec3) {
        let _ = app.dispatch(Command::SetTransform { id: self.target.0, position: Some(at.to_array()), rotation_deg: None, scale: None });
    }

    pub fn finish(self, app: &mut LoomApp) {
        app.state.undo.end();
    }

    /// Create > Cameras > Free: a click places a camera looking down the
    /// construction plane's -Z (away from the viewer in the axis views,
    /// straight down in Perspective and Top).
    pub fn place_free(app: &mut LoomApp, anchor: Vec3, normal: Vec3) {
        let look = anchor - normal * loom_scene::camera::DEFAULT_TARGET_DISTANCE;
        let _ = app.dispatch(Command::CreateCameraAt { target: false, position: anchor.to_array(), look_at: look.to_array() });
    }
}

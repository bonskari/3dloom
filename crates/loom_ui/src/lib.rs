//! egui/eframe UI shell: menubar, command panel, statusbar, viewport widget.
//! All scene mutations go through `loom_commands::execute` via `dispatch`.

mod command_panel;
pub mod create_tool;
pub mod dialogs;
pub mod gizmo;
mod hotkeys;
pub mod label_menus;
pub mod main_toolbar;
pub mod quad_menu;
pub mod region;
pub mod time_slider;
mod menubar;
mod statusbar;
pub mod tools;
pub mod viewport_gizmo;
mod viewport_widget;
pub mod poly_modes;
pub mod spline_modes;
pub mod ribbon;
pub mod viewports;
pub mod uv_editor;
pub mod track_view;

use eframe::egui;
use glam::Vec3;
use loom_commands::{AppState, Command, Keymap, PrimitiveKind, ShapeKind, Tool};
pub use command_panel::create::Category;
pub use command_panel::Tab;
pub use command_panel::hierarchy::Sub as HierarchySub;
pub use command_panel::utilities::Utility;
use create_tool::{CreateSession, LineSession};
use loom_input::ViewportNavigator;
use loom_mesh::Mesh;
use loom_render::Camera;
use loom_scene::NodeId;
use gizmo::DragState;

pub use loom_commands;

#[derive(Debug, Clone, Copy)]
pub struct ViewAnimation {
    start_yaw: f32,
    start_pitch: f32,
    end_yaw: f32,
    end_pitch: f32,
    t: f32,
}

/// Meshes and colours waiting for GPU upload, shared with the viewport callbacks.
pub type PendingUpload = std::sync::Arc<std::sync::Mutex<Option<(Vec<Mesh>, Vec<loom_render::MeshStyle>, Vec<(String, std::sync::Arc<loom_render::Bitmap>)>)>>>;

/// Keyboard Entry rollout values.
#[derive(Debug, Clone, Copy)]
pub struct KeyboardEntry {
    pub position: [f32; 3],
    pub size: [f32; 3],
}

/// Top-level application: command state plus UI-only interaction state.
/// A live preview pushed by a dialog: which dialog, and the undo depth
/// right after it was applied (so it is only undone while still on top).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preview {
    pub dialog: loom_commands::Dialog,
    /// `UndoStack::version` right after the preview commit.
    pub version: u64,
}

/// Soft Selection > Paint Soft Selection brush settings.
#[derive(Debug, Clone, Copy)]
pub struct SoftBrush {
    pub value: f32,
    pub size: f32,
    pub strength: f32,
}

/// A manipulator handle being dragged.
#[derive(Debug, Clone)]
pub struct ManipDrag {
    pub id: loom_scene::NodeId,
    pub name: &'static str,
    pub last: egui::Pos2,
    pub origin: glam::Vec3,
    pub dir: glam::Vec3,
}

/// Result of a Shift+drag on a gizmo, awaiting the Clone Options dialog.
#[derive(Debug, Clone)]
pub struct ShiftClone {
    pub tool: Tool,
    /// The nodes that were selected when the drag began (the originals).
    pub sources: Vec<loom_scene::NodeId>,
    /// Move: offset in the reference system. Rotate: Euler degrees (axis
    /// handles) or the world quaternion (Screen / Free). Scale: factors.
    pub offset: glam::Vec3,
    pub rotate_deg: glam::Vec3,
    pub quat: glam::Quat,
    pub scale: glam::Vec3,
    /// Undo depth right after the drag, so only that step is undone.
    pub undo_len: usize,
}

/// What the Create tab's pressed Object Type button creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateKind {
    Primitive(PrimitiveKind),
    Shape(ShapeKind),
    Line,
    /// Systems > Bones: clicks lay joints, a right-click ends the chain.
    Bones,
    /// Helpers > Dummy / Point: a click places one.
    Helper(HelperKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperKind {
    Dummy,
    Point,
}

impl HelperKind {
    pub fn api_name(self) -> &'static str {
        match self {
            HelperKind::Dummy => "dummy",
            HelperKind::Point => "point",
        }
    }
    pub fn default_size(self) -> f32 {
        match self {
            HelperKind::Dummy => 10.0,
            HelperKind::Point => 20.0,
        }
    }
}

/// Line > Creation Method > Drag Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineDrag {
    Corner,
    Smooth,
    Bezier,
}

pub struct LoomApp {
    pub state: AppState,
    pub keymap: Keymap,
    pub camera_controller: ViewportNavigator,
    /// World-space meshes of visible nodes, rebuilt when `state.mesh_dirty`.
    world_meshes: Vec<(NodeId, Mesh)>,
    /// Draw styles parallel to `world_meshes`.
    world_styles: Vec<loom_render::MeshStyle>,
    /// Meshes and colours waiting to be uploaded to the GPU. Every viewport
    /// callback holds this; whichever runs first takes and uploads them, so
    /// the upload never depends on one particular viewport being painted.
    pub pending_upload: PendingUpload,
    /// Object Type button pressed in the Create tab.
    pub create_kind: Option<CreateKind>,
    /// Create > Geometry class: Standard Primitives or Compound Objects.
    pub create_compound: bool,
    /// Compound Objects > Object Type button pressed.
    pub compound_kind: Option<command_panel::compound::CompoundKind>,
    /// A Line being drawn in the viewport.
    pub line: Option<LineSession>,
    /// Joints clicked so far with the Bones tool (world space).
    pub bone_points: Vec<Vec3>,
    /// Bone Tools > Mirror settings.
    pub bone_mirror_axis: u32,
    pub bone_mirror_offset: f32,
    /// Line > Creation Method: Initial Type Smooth (else Corner).
    pub line_initial_smooth: bool,
    /// Line > Creation Method: Drag Type.
    pub line_drag: LineDrag,
    pub create_category: Category,
    pub autogrid: bool,
    pub keyboard_entry: KeyboardEntry,
    /// Creation in progress in the viewport.
    pub create: Option<CreateSession>,
    pub panel_tab: Tab,
    pub hierarchy_sub: HierarchySub,
    pub utility: Utility,
    pub rescale_factor: f32,
    pub transform_drag: Option<DragState>,
    /// A Shift+drag clone in progress: the drag moved a Copy of the original;
    /// on release the Clone Options dialog finalises kind and count.
    pub shift_clone: Option<ShiftClone>,
    /// A dialog whose result is previewed live (Align, Align to View, Normal
    /// Align, Mirror, Array): closing it any way but OK undoes the preview,
    /// but only while that preview is still the top undo entry.
    pub preview: Option<Preview>,
    /// The dialog shown last frame, to tear down per-dialog temp state.
    pub last_dialog: Option<loom_commands::Dialog>,
    /// Directory the file dialog opens in (the last one used).
    pub file_dir: std::path::PathBuf,
    /// System (xdg-desktop-portal) file dialogs; off when remote-controlled.
    pub native_file_dialogs: bool,
    /// Title last sent to the window, so it is only resent on change.
    pub last_title: String,
    /// A transform or region drag in the Edit UVWs window.
    pub uv_drag: Option<uv_editor::UvDrag>,
    /// Sketch: the stroke being drawn (UV) and whether vertices were picked yet.
    pub uv_sketch: Vec<[f32; 2]>,
    pub uv_sketch_picked: bool,
    /// A drag in the Track View key window.
    pub tv_drag: Option<track_view::TvDrag>,
    /// Track bar key selection and drag.
    pub track_bar: time_slider::TrackBarState,
    /// The Motion panel shows Trajectories (the viewports draw the paths).
    pub motion_trajectories: bool,
    /// A manipulator drag in progress (Select and Manipulate).
    pub manip_drag: Option<ManipDrag>,
    /// Selection rollout > By Angle: clicking a polygon also selects
    /// neighbours within this many degrees.
    pub by_angle: Option<f32>,
    /// Paint Soft Selection brush values (Selection Value, Brush Size, Strength).
    pub soft_brush: SoftBrush,
    /// A soft selection brush stroke in progress.
    pub soft_painting: bool,
    /// An Extrude / Bevel / … drag mode owns the left button (no click-select).
    pub poly_dragging: bool,
    pub ribbon_tab: ribbon::RibbonTab,
    pub ribbon_minimized: bool,
    /// A Paint Deformation stroke is in progress.
    pub deform_painting: bool,
    /// Edit Geometry > Named Selections name field.
    pub named_sub_selection_name: String,
    /// Originals of a Shift+drag in progress.
    pub shift_sources: Vec<loom_scene::NodeId>,
    pub view_animation: Option<ViewAnimation>,
    pub drag_select_start: Option<egui::Pos2>,
    /// Alt+LMB press not yet moved 4 px: a click subtracts from the selection, a drag orbits.
    pub alt_click: Option<egui::Pos2>,
    /// Lasso/Paint path or Fence corners being collected.
    pub region_points: Vec<egui::Pos2>,
    pub quad_menu: Option<quad_menu::Open>,
    /// Loaded viewport background images, keyed by (path, version).
    pub background_textures: std::collections::HashMap<(String, u32), Option<egui::TextureHandle>>,
    /// Show Map in Viewport bitmaps decoded once per path.
    pub bitmap_cache: std::collections::HashMap<String, Option<std::sync::Arc<loom_render::Bitmap>>>,
    pub last_error: Option<String>,
}

impl LoomApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let wgpu = cc
            .wgpu_render_state
            .as_ref()
            .expect("wgpu backend required");
        viewport_widget::init_render_resources(wgpu);
        Self::headless()
    }

    /// App state without GPU resources, for tests of UI models.
    pub fn headless() -> Self {
        Self {
            state: AppState::new(),
            keymap: Keymap::main_ui(),
            camera_controller: ViewportNavigator::new(),
            world_meshes: Vec::new(),
            world_styles: Vec::new(),
            pending_upload: std::sync::Arc::new(std::sync::Mutex::new(Some((Vec::new(), Vec::new(), Vec::new())))),
            bitmap_cache: std::collections::HashMap::new(),
            create_kind: None,
            create_compound: false,
            compound_kind: None,
            line: None,
            bone_points: Vec::new(),
            bone_mirror_axis: 0,
            bone_mirror_offset: 0.0,
            line_initial_smooth: false,
            line_drag: LineDrag::Bezier,
            create_category: Category::Geometry,
            autogrid: false,
            keyboard_entry: KeyboardEntry { position: [0.0; 3], size: [10.0, 10.0, 10.0] },
            create: None,
            panel_tab: Tab::Create,
            hierarchy_sub: HierarchySub::Pivot,
            utility: Utility::None,
            rescale_factor: 1.0,
            transform_drag: None,
            shift_clone: None,
            preview: None,
            last_dialog: None,
            file_dir: std::env::current_dir().unwrap_or_default(),
            native_file_dialogs: dialogs::file_dialog::use_native(std::env::var("LOOM_CONTROL_PORT").ok().as_deref()),
            last_title: String::new(),
            uv_drag: None,
            uv_sketch: Vec::new(),
            uv_sketch_picked: false,
            tv_drag: None,
            track_bar: Default::default(),
            motion_trajectories: false,
            manip_drag: None,
            by_angle: None,
            soft_brush: SoftBrush { value: 1.0, size: 20.0, strength: 1.0 },
            soft_painting: false,
            poly_dragging: false,
            ribbon_tab: ribbon::RibbonTab::Modeling,
            ribbon_minimized: false,
            deform_painting: false,
            named_sub_selection_name: String::new(),
            shift_sources: Vec::new(),
            view_animation: None,
            drag_select_start: None,
            alt_click: None,
            region_points: Vec::new(),
            quad_menu: None,
            background_textures: std::collections::HashMap::new(),
            last_error: None,
        }
    }

    /// Runs a command, records errors for the status bar and marks render
    /// state dirty when the scene changed.
    pub fn dispatch(&mut self, cmd: Command) -> Result<serde_json::Value, String> {
        if matches!(cmd, Command::UndoSceneOperation | Command::RedoSceneOperation) && self.create.is_some() {
            // Undo during creation ends the creation; its transaction closes first.
            self.create = None;
            self.state.undo.end();
        }
        let opens_editor = matches!(&cmd, Command::AddModifier { name } if name == "Unwrap UVW") && self.state.uv_editor.always_bring_up;
        let r = loom_commands::execute(&mut self.state, cmd);
        match &r {
            Err(e) => self.last_error = Some(e.clone()),
            Ok(_) => self.last_error = None,
        }
        if r.is_ok() && opens_editor {
            // Always Bring Up The Edit Window.
            let _ = loom_commands::execute(&mut self.state, Command::EditUvws);
        }
        self.sync_after_command();
        r
    }

    pub fn dispatch_json(&mut self, body: &serde_json::Value) -> serde_json::Value {
        let r = loom_commands::execute_json(&mut self.state, body);
        self.sync_after_command();
        r
    }

    /// Runs a command by its action name (hotkeys, menus).
    pub fn dispatch_named(&mut self, name: &str) {
        let body = serde_json::json!({ "cmd": name });
        let r = self.dispatch_json(&body);
        if r["ok"] == false {
            self.last_error = r["error"].as_str().map(str::to_owned);
        }
    }

    pub(crate) fn sync_after_command(&mut self) {
        if self.state.mesh_dirty {
            self.rebuild_world_meshes();
            self.state.mesh_dirty = false;
        }
        if self.state.pending_file_action.is_some() && self.state.dirty() && self.state.dialog.is_none() {
            // The Save As was cancelled: so is the action.
            self.state.pending_file_action = None;
        }
    }

    /// File > New / Reset / Open / Exit: asks to save first when the scene is dirty.
    pub fn request_file_action(&mut self, action: loom_commands::PendingFileAction) {
        if self.state.dirty() {
            self.state.dialog = Some(loom_commands::Dialog::SaveChanges(action));
        } else {
            self.run_file_action(action);
        }
    }

    pub fn run_file_action(&mut self, action: loom_commands::PendingFileAction) {
        match loom_commands::file_action(&mut self.state, action) {
            Err(e) => self.last_error = Some(e),
            Ok(_) => self.last_error = None,
        }
        self.sync_after_command();
    }

    fn rebuild_world_meshes(&mut self) {
        let srgb = |c: u8| {
            let x = c as f32 / 255.0;
            if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
        };
        let mut meshes = Vec::new();
        let mut display = Vec::new();
        let mut styles = Vec::new();
        let mut bitmaps: Vec<(String, std::sync::Arc<loom_render::Bitmap>)> = Vec::new();
        let mut bitmap_index: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        let bones_only = self.state.display_bones_only;
        let visible: Vec<NodeId> = self.state.scene.nodes().filter(|n| self.state.is_visible(n) && (!bones_only || n.object.is_bone() || n.object.is_helper())).map(|n| n.id).collect();
        for id in visible {
            let n = self.state.scene.node(id).expect("collected above");
            let mut mesh = n.cage_mesh().into_owned();
            // What the GPU draws: the stack result (NURMS included), or with
            // Show End Result off only up to the active entry; the cage stays
            // the base mesh for sub-object picking.
            let nurms = matches!(&n.object, loom_scene::ObjectKind::EditablePoly(p) if p.subdiv.use_nurms.get());
            let mut shown = if n.modifiers.is_empty() && !nurms {
                None
            } else if !self.state.show_end_result {
                match n.active_entry {
                    None => Some(n.object.display_output()),
                    Some(a) => Some(self.state.scene.evaluated(id, Some(a)).value.mesh.clone()),
                }
            } else {
                Some(self.state.scene.eval_mesh(id).value.mesh.clone())
            };
            if n.display.display_as_box {
                // Display as Box: the object's local bounding box.
                let (min, max) = mesh.aabb();
                mesh = loom_mesh::box_primitive((max - min) * 0.5);
                let _ = loom_mesh::edit_ops::translate_mesh(&mut mesh, (min + max) * 0.5);
                shown = None;
            }
            let world = self.state.scene.object_matrix(n.id);
            let _ = loom_mesh::edit_ops::transform_mesh(&mut mesh, world);
            if let Some(d) = shown.as_mut() {
                let _ = loom_mesh::edit_ops::transform_mesh(d, world);
            }
            let gray = n.frozen && n.display.show_frozen_in_gray;
            let color = if gray {
                [0.35, 0.35, 0.35]
            } else {
                [srgb(n.wire_color[0]), srgb(n.wire_color[1]), srgb(n.wire_color[2])]
            };
            // Helpers (Dummy, Point, IK goals) draw see-through like wireframe helpers.
            let see_through = n.display.see_through || n.object.is_helper();
            let texture = match &n.display.texture {
                loom_scene::TextureDisplay::None => loom_render::TextureSel::None,
                loom_scene::TextureDisplay::Checker => loom_render::TextureSel::Checker,
                loom_scene::TextureDisplay::Bitmap(path) => match bitmap_index.get(path) {
                    Some(i) => loom_render::TextureSel::Bitmap(*i),
                    None => match load_bitmap(&mut self.bitmap_cache, path) {
                        Some(bm) => {
                            bitmaps.push((path.clone(), bm));
                            bitmap_index.insert(path.clone(), bitmaps.len() as u32 - 1);
                            loom_render::TextureSel::Bitmap(bitmaps.len() as u32 - 1)
                        }
                        None => {
                            self.last_error = Some(format!("could not load map {path}"));
                            loom_render::TextureSel::None
                        }
                    },
                },
            };
            // A visibility track below 1 shows the object partly transparent (never fully hidden in shaded views).
            let visibility = n.visibility.as_ref().map(|p| p.get().clamp(0.08, 1.0)).unwrap_or(1.0);
            styles.push(loom_render::MeshStyle { color, alpha: if see_through { 0.35 * visibility } else { visibility }, backface_cull: n.display.backface_cull, vertex_colors: n.display.vertex_colors, texture });
            display.push(shown.unwrap_or_else(|| mesh.clone()));
            meshes.push((n.id, mesh));
        }
        self.world_meshes = meshes;
        self.world_styles = styles;
        if let Ok(mut slot) = self.pending_upload.lock() {
            *slot = Some((display, self.world_styles.clone(), bitmaps));
        }
    }

    pub fn world_styles(&self) -> &[loom_render::MeshStyle] {
        &self.world_styles
    }

    /// Visible nodes with their world-space meshes, in render order.
    pub fn world_meshes(&self) -> &[(NodeId, Mesh)] {
        &self.world_meshes
    }

    pub fn render_camera(&self) -> Camera {
        self.camera_for(self.state.active_viewport)
    }

    pub fn camera_for(&self, index: usize) -> Camera {
        let vp = &self.state.viewports[index.min(self.state.viewports.len() - 1)];
        if let loom_commands::ViewKind::Camera(id) = vp.kind {
            if let Some(node) = self.state.scene.node(id) {
                // Look through the camera node: it looks down its local -Z.
                let world = self.state.scene.world_matrix(id);
                let eye = world.w_axis.truncate();
                let forward = -world.z_axis.truncate().normalize_or_zero();
                let distance = 10.0;
                let dir = -forward;
                let mut orbit = loom_input::OrbitCamera { target: eye + forward * distance, distance, yaw: dir.y.atan2(dir.x), pitch: dir.z.clamp(-1.0, 1.0).asin() };
                if forward.length_squared() < 1e-6 {
                    orbit = vp.camera.clone();
                }
                let fov = match &node.object {
                    loom_scene::ObjectKind::Camera { fov, .. } => fov.get().to_radians(),
                    _ => vp.fov_y_radians,
                };
                return Camera { orbit, perspective: true, fov_y_radians: fov, ..Camera::default() };
            }
        }
        let (auto_near, auto_far) = Camera::auto_clip(vp.camera.distance);
        let (near, far) = if vp.clipping { (vp.clip.0.max(auto_near), vp.clip.1.max(vp.clip.0.max(auto_near) + 1e-3)) } else { (auto_near, auto_far) };
        Camera {
            orbit: vp.camera.clone(),
            perspective: !vp.is_orthographic(),
            fov_y_radians: vp.fov_y_radians,
            near,
            far,
        }
    }

    // ---- gizmo drags: one undo step per drag ------------------------------

    pub fn begin_transform_drag(&mut self, drag: DragState) {
        self.transform_drag = Some(drag);
        let label = match self.state.active_tool {
            Tool::Rotate => "Rotate",
            Tool::Scale => "Scale",
            _ => "Move",
        };
        self.state.undo.begin(label);
    }

    pub fn end_transform_drag(&mut self) {
        self.transform_drag = None;
        self.state.undo.end();
    }

    /// Display-unit factor and suffix (Units Setup) for length spinners.
    pub fn unit_display(&self) -> (f64, String) {
        self.state.units.display_factor()
    }

    /// A length spinner in display units: the value stays in scene units.
    pub fn length_drag<'a>(&self, value: &'a mut f32, speed: f64) -> egui::DragValue<'a> {
        let (factor, suffix) = self.unit_display();
        let decimals = self.state.prefs.spinner_precision;
        egui::DragValue::new(value)
            .speed(speed)
            .custom_formatter(move |v, _| format!("{:.*}{}", decimals, v * factor, suffix))
            .custom_parser(move |text| {
                let t = text.trim().trim_end_matches(|c: char| c.is_alphabetic() || c == '"' || c == '\'');
                t.parse::<f64>().ok().map(|v| v / factor)
            })
    }

    /// Spinner drag speed: the Preferences snap value when Spinner Snap is on.
    pub fn spinner_speed(&self, default: f64) -> f64 {
        if self.state.prefs.spinner_snap { self.state.prefs.spinner_snap_value as f64 } else { default }
    }

    pub fn preview_dialog(&self) -> Option<loom_commands::Dialog> {
        self.preview.map(|p| p.dialog)
    }

    /// Runs `f` as a live preview for `dialog`; remembered only if it pushed
    /// an undo entry.
    pub fn preview_apply(&mut self, dialog: loom_commands::Dialog, f: impl FnOnce(&mut Self)) {
        let before = self.state.undo.version();
        f(self);
        let after = self.state.undo.version();
        self.preview = (after > before && self.state.undo.can_undo()).then_some(Preview { dialog, version: after });
    }

    /// Marks an already-applied operation (a pick that just ran) as the preview.
    pub fn preview_mark(&mut self, dialog: loom_commands::Dialog) {
        self.preview = Some(Preview { dialog, version: self.state.undo.version() });
    }

    /// Undoes the preview if it is still the top undo entry; otherwise it is
    /// left alone (something else was edited meanwhile).
    pub fn preview_undo(&mut self) {
        if let Some(p) = self.preview.take() {
            if self.state.undo.version() == p.version && self.state.undo.can_undo() {
                let _ = self.dispatch(Command::UndoSceneOperation);
            }
        }
    }

    /// Keeps the preview as a real result.
    pub fn preview_keep(&mut self) {
        self.preview = None;
    }

    /// A Snap Override applies to one snap only; clear it once used.
    pub fn consume_snap_override(&mut self) {
        if self.state.snaps.override_target.is_some() {
            let _ = self.dispatch(Command::SnapOverride { name: None });
        }
    }

    // ---- view animation ----------------------------------------------------

    pub fn start_view_animation(&mut self, yaw: f32, pitch: f32) {
        self.view_animation = Some(ViewAnimation {
            start_yaw: self.state.active().camera.yaw,
            start_pitch: self.state.active().camera.pitch,
            end_yaw: yaw,
            end_pitch: pitch,
            t: 0.0,
        });
    }

    pub fn update_view_animation(&mut self, dt: f32) -> bool {
        let Some(mut anim) = self.view_animation else {
            return false;
        };
        anim.t = (anim.t + dt / 0.25).min(1.0);
        let s = anim.t * anim.t * (3.0 - 2.0 * anim.t);
        let mut dyaw = anim.end_yaw - anim.start_yaw;
        while dyaw > std::f32::consts::PI {
            dyaw -= std::f32::consts::TAU;
        }
        while dyaw < -std::f32::consts::PI {
            dyaw += std::f32::consts::TAU;
        }
        let cam = &mut self.state.active_mut().camera;
        cam.yaw = anim.start_yaw + dyaw * s;
        cam.pitch = anim.start_pitch + (anim.end_pitch - anim.start_pitch) * s;
        if anim.t >= 1.0 {
            self.view_animation = None;
        } else {
            self.view_animation = Some(anim);
        }
        true
    }

    pub fn selection_pivot(&self) -> Vec3 {
        self.state.selection_pivot()
    }

    /// Texture for a viewport background file, loaded once per version.
    pub fn background_texture(&mut self, ctx: &egui::Context, path: &str, version: u32) -> Option<egui::TextureHandle> {
        let key = (path.to_owned(), version);
        if let Some(t) = self.background_textures.get(&key) {
            return t.clone();
        }
        let loaded = image::open(path).ok().map(|img| {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let ci = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            ctx.load_texture(format!("bg:{path}:{version}"), ci, egui::TextureOptions::LINEAR)
        });
        if loaded.is_none() {
            self.last_error = Some(format!("could not load background image {path}"));
        }
        self.background_textures.insert(key, loaded.clone());
        loaded
    }

    pub fn has_selection(&self) -> bool {
        match self.state.selection_mode {
            loom_commands::SelectionMode::Object => !self.state.selection.is_empty(),
            _ => !self.state.sub_selection.is_empty(),
        }
    }
}

impl eframe::App for LoomApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && self.state.dirty() && !self.state.exit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_file_action(loom_commands::PendingFileAction::Exit);
        }
        if self.state.exit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Auto Backup: a cheap check each frame; the clock decides.
        match loom_commands::autosave::tick(&mut self.state, std::time::Instant::now()) {
            Ok(Some(path)) => self.state.status_prompt = format!("Auto Backup: {}", path.display()),
            Err(e) => self.last_error = Some(e),
            Ok(None) => {}
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        let title = format!("{}  [{}]", self.state.title(), env!("LOOM_VERSION"));
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        hotkeys::handle(ctx, self);
        // Playback: advance by the wall clock (Real Time) or a frame per redraw.
        if self.state.playing {
            let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.25);
            let _ = self.dispatch(Command::TickPlayback { seconds: dt });
            ctx.request_repaint();
        }
        if self.camera_controller.mode() != self.state.nav_mode {
            self.camera_controller.set_mode(self.state.nav_mode);
        }
        menubar::show(ctx, self);
        main_toolbar::show(ctx, self);
        ribbon::show(ctx, self);
        // Bottom panels first so they span the full width under the Command Panel.
        statusbar::show(ctx, self);
        time_slider::show(ctx, self);
        command_panel::show(ctx, self);
        viewports::show(ctx, self);
        if let Some(open) = self.quad_menu {
            if quad_menu::show(ctx, self, open) {
                self.quad_menu = None;
            }
        }
        dialogs::show(ctx, self);
        uv_editor::show(ctx, self);
        track_view::show(ctx, self);
    }
}

/// Decodes a Show Map bitmap once per path (a failed load is remembered too).
fn load_bitmap(cache: &mut std::collections::HashMap<String, Option<std::sync::Arc<loom_render::Bitmap>>>, path: &str) -> Option<std::sync::Arc<loom_render::Bitmap>> {
    cache
        .entry(path.to_owned())
        .or_insert_with(|| {
            image::open(path).ok().map(|img| {
                // GPU textures are capped at 8192 px a side (the common wgpu limit): larger maps shrink.
                let max = 8192u32;
                let img = if img.width() > max || img.height() > max { img.thumbnail(max, max) } else { img };
                let rgba = img.to_rgba8();
                std::sync::Arc::new(loom_render::Bitmap { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
            })
        })
        .clone()
}

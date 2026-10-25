//! Everything a command can touch. UI-only state (gizmo drags, pointer
//! positions) lives in `loom_ui::LoomApp`, not here.

use glam::Vec3;
use loom_input::OrbitCamera;
use loom_mesh::{FaceId, HalfEdgeId, VertexId};
use crate::viewport::{default_kinds, Dialog, HideCategories, LayoutSplits, NavMode, Viewport, ViewportLayout};
use loom_scene::{Node, NodeId, ObjectKind, Scene, UndoStack};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionMode {
    Object,
    Vertex,
    Edge,
    /// Open edges only (Editable Poly > Border).
    Border,
    /// Polygons (the level is called "Polygon").
    Face,
    /// Connected groups of polygons (Editable Poly > Element).
    Element,
}

impl SelectionMode {
    /// Editable Poly sub-object levels in order (keys 1–5).
    pub const ALL: [SelectionMode; 5] = [
        SelectionMode::Vertex,
        SelectionMode::Edge,
        SelectionMode::Border,
        SelectionMode::Face,
        SelectionMode::Element,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SelectionMode::Object => "Object",
            SelectionMode::Vertex => "Vertex",
            SelectionMode::Edge => "Edge",
            SelectionMode::Border => "Border",
            SelectionMode::Face => "Polygon",
            SelectionMode::Element => "Element",
        }
    }

    /// Which id kind this level selects.
    pub fn is_edge_level(self) -> bool {
        matches!(self, SelectionMode::Edge | SelectionMode::Border)
    }

    pub fn is_face_level(self) -> bool {
        matches!(self, SelectionMode::Face | SelectionMode::Element)
    }
}

/// The base object's sub-object levels as the modifier stack lists them under
/// its `+`, with the selection mode each one enters. Empty for objects without
/// levels (parametric primitives and shapes).
pub fn base_sub_levels(object: &loom_scene::ObjectKind) -> Vec<(&'static str, SelectionMode)> {
    match object {
        loom_scene::ObjectKind::EditablePoly(_) => SelectionMode::ALL.iter().map(|m| (m.label(), *m)).collect(),
        loom_scene::ObjectKind::EditableSpline(_) => vec![("Vertex", SelectionMode::Vertex), ("Segment", SelectionMode::Edge), ("Spline", SelectionMode::Face)],
        _ => Vec::new(),
    }
}

/// Paint Soft Selection brush.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SoftPaint {
    Paint,
    Blur,
    Revert,
}

/// Selection rollout > Preview Selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PreviewSelect {
    #[default]
    Off,
    SubObj,
    Multi,
}

impl PreviewSelect {
    pub fn next(self) -> Self {
        match self {
            PreviewSelect::Off => PreviewSelect::SubObj,
            PreviewSelect::SubObj => PreviewSelect::Multi,
            PreviewSelect::Multi => PreviewSelect::Off,
        }
    }
}

/// Purpose of the Select From Scene dialog when opened by a "by Name" button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DialogPurpose {
    #[default]
    Select,
    Hide,
    Unhide,
    Freeze,
    Unfreeze,
    /// Editable Poly > Attach List.
    Attach,
    /// Skin > Bones > Add.
    SkinAddBones,
    /// Select By Name while Select and Link is on: the Select Parent dialog.
    LinkParent,
    /// Select By Name while Group > Attach is on: the Attach To Group dialog.
    GroupAttach,
}

/// A one-shot viewport pick mode (Display panel "by Hit" buttons, Select and Link).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PickMode {
    /// Boolean > Pick Operand B.
    BooleanOperandB,
    /// ProBoolean > Start Picking.
    ProBooleanOperand,
    /// Loft > Get Path.
    LoftGetPath,
    /// Loft > Get Shape.
    LoftGetShape,
    /// Editable Spline > Attach: click a shape.
    SplineAttach,
    /// Editable Spline > Refine: click a segment to add a knot.
    SplineRefine,
    /// Editable Spline > Insert: click a segment, the new knot follows the mouse.
    SplineInsert,
    /// Editable Spline > Fillet / Chamfer: drag on a selected vertex.
    SplineFillet,
    SplineChamfer,
    /// Editable Poly > Extrude Along Spline: click a shape.
    ExtrudeAlongSpline,
    HideByHit,
    FreezeByHit,
    UnfreezeByHit,
    /// Select and Link: the next clicked object becomes the selection's parent.
    Link,
    /// Align (Alt+A): pick the target object, then the Align dialog opens.
    AlignTarget,
    /// Quick Align (Shift+A): pick the target object.
    QuickAlignTarget,
    /// Normal Align (Alt+N): pick a face on the source, then on the target.
    NormalAlignSource,
    NormalAlignTarget,
    /// Align Camera: pick a face; the selected camera looks down its normal.
    AlignCamera,
    /// Unwrap UVW > Point To Point Edge Selection: first and next vertex.
    SeamPointA,
    SeamPointB,
    /// Edit UVWs > Target Weld: the tv to weld, then its target.
    TvTargetWeldSource,
    TvTargetWeldTarget,
    /// UVW Map > Normal Align: pick a face.
    UvwMapNormal,
    /// UVW Map > Acquire: pick the object to copy from.
    UvwMapAcquire,
    /// UVW Map > Region Fit: first and second corner.
    UvwMapRegionA,
    UvwMapRegionB,
    /// Spacing Tool > Pick Points: first and second point.
    SpacingPointA,
    SpacingPointB,
    /// Target Weld: pick the vertex to move, then the vertex to weld it onto.
    TargetWeldSource,
    TargetWeldTarget,
    /// Insert Vertex: clicks land on an edge or inside a polygon.
    InsertVertex,
    /// Edit Triangulation: two vertices of one polygon.
    EditTriangulationA,
    EditTriangulationB,
    /// Create (polygon level): vertices in order.
    CreatePolygon,
    /// Create (vertex level): a world point per click.
    CreateVertex,
    /// Hinge From Edge: the next edge pick is the hinge.
    HingeEdge,
    /// Attach: the next clicked object joins the selected poly.
    AttachObject,
    /// QuickSlice: two points.
    QuickSliceA,
    QuickSliceB,
    /// Cut: start and end surface points.
    CutA,
    CutB,
    /// Swift Loop: each click on an edge inserts a loop across its ring.
    SwiftLoop,
    /// Bridge Edges > Pick Edge 1 / Pick Edge 2: the next edge pick.
    BridgeEdge1,
    BridgeEdge2,
    /// Attach to Group: the next clicked group takes the selection.
    GroupAttach,
    /// Linked XForm / Preserve: the next clicked object is the control / original.
    ModifierNode,
    /// Editable Spline > Trim / Extend / Cross Insert / Bind: a click in the viewport (Pick Point).
    SplineTrim,
    SplineExtend,
    SplineCrossInsert,
    SplineBind,
    /// Trim/Extend modifier > Pick Locations: each click trims or extends there.
    TrimExtendModifier,
    /// Unwrap UVW > Spline Map: the next clicked shape is the mapping spline.
    UnwrapSplineMap,
    /// Animation > Constraints / Motion panel Add … Target: the clicked object is the target.
    Constraint(crate::motion::ConstraintPick),
    /// Reference Coordinate System > Pick: the clicked object's axes become the system.
    CoordSystem,
    /// Working Pivot > Place Pivot To: View (a point in the screen plane) / Surface.
    WorkingPivotView,
    WorkingPivotSurface,
}

/// Paint Deformation brushes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaintMode {
    PushPull,
    Relax,
    Revert,
}

/// Tessellate > Operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TessKind {
    Edge,
    #[default]
    Face,
}

/// Edit Geometry > Constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PolyConstraint {
    #[default]
    None,
    Edges,
    Faces,
    Normals,
}

/// Hierarchy > Working Pivot: the scene's one working pivot (world space).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct WorkingPivot {
    pub origin: Vec3,
    pub rotation: glam::Quat,
}

/// Transform manager settings put aside while a Working Pivot mode is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SavedManagers {
    pub coord_by_tool: [CoordSystem; 4],
    pub center_by_tool: [(CenterMode, CenterMode); 4],
    pub center: (CenterMode, CenterMode),
    pub tool: Tool,
}

/// The Slice Plane gizmo (world space).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SlicePlane {
    pub origin: Vec3,
    pub rotation: glam::Quat,
    /// Half-size of the drawn square.
    pub size: f32,
}

impl Default for SlicePlane {
    fn default() -> Self {
        Self { origin: Vec3::ZERO, rotation: glam::Quat::IDENTITY, size: 10.0 }
    }
}

impl SlicePlane {
    pub fn normal(&self) -> Vec3 {
        self.rotation * Vec3::Z
    }
}

/// Extrude Polygons > Extrusion Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExtrudeKind {
    #[default]
    Group,
    LocalNormal,
    ByPolygon,
}

/// Drag modes of the Edit rollouts (the mode buttons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolyDrag {
    Extrude,
    Chamfer,
    Inset,
    Bevel,
    Outline,
    Hinge,
}

/// Last-used values of the Editable Poly settings windows; the plain buttons
/// use them and the Loom commands with arguments update them.
/// Compound Objects rollouts: Boolean operation / Copy-Move, ProBoolean
/// operation, Loft path percent for the next Get Shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompoundSettings {
    pub boolean_op: u32,
    pub move_operand: bool,
    pub proboolean_op: u32,
    pub loft_path_percent: f32,
}

impl Default for CompoundSettings {
    fn default() -> Self {
        Self { boolean_op: 2, move_operand: true, proboolean_op: 0, loft_path_percent: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PolySettings {
    pub weld_threshold: f32,
    pub chamfer_amount: f32,
    pub chamfer_segments: u32,
    pub chamfer_open: bool,
    pub connect_segments: u32,
    pub connect_pinch: f32,
    pub connect_slide: f32,
    pub bridge_segments: u32,
    pub bridge_smooth: f32,
    pub bridge_adjacent: f32,
    pub bridge_twist1: i32,
    pub bridge_twist2: i32,
    pub extrude_height: f32,
    pub extrude_width: f32,
    pub extrude_kind: ExtrudeKind,
    pub inset_amount: f32,
    pub inset_by_polygon: bool,
    pub bevel_height: f32,
    pub bevel_outline: f32,
    pub bevel_kind: ExtrudeKind,
    pub outline_amount: f32,
    pub hinge_angle: f32,
    pub hinge_segments: u32,
    /// The picked hinge edge (node, raw edge id).
    pub hinge_edge: Option<(NodeId, u32)>,
    /// Bridge Edges: Bridge Specific Edges (the picked edges) instead of the selection.
    pub bridge_specific: bool,
    /// Bridge Edges > Pick Edge 1 / 2 (node, raw edge id).
    pub bridge_edges: [Option<(NodeId, u32)>; 2],
    pub msmooth_smoothness: f32,
    pub msmooth_separate_by_smoothing: bool,
    pub msmooth_separate_by_materials: bool,
    pub tess_kind: TessKind,
    pub tess_tension: f32,
    pub relax_amount: f32,
    pub relax_iterations: u32,
    pub relax_hold_boundary: bool,
    pub relax_hold_outer: bool,
    pub auto_smooth_threshold: f32,
}

impl Default for PolySettings {
    fn default() -> Self {
        Self {
            weld_threshold: 0.1,
            chamfer_amount: 1.0,
            chamfer_segments: 1,
            chamfer_open: false,
            connect_segments: 1,
            connect_pinch: 0.0,
            connect_slide: 0.0,
            bridge_segments: 1,
            bridge_smooth: 45.0,
            bridge_adjacent: 45.0,
            bridge_twist1: 0,
            bridge_twist2: 0,
            extrude_height: 1.0,
            extrude_width: 0.5,
            extrude_kind: ExtrudeKind::Group,
            inset_amount: 1.0,
            inset_by_polygon: false,
            bevel_height: 1.0,
            bevel_outline: 0.0,
            bevel_kind: ExtrudeKind::Group,
            outline_amount: 1.0,
            hinge_angle: 30.0,
            hinge_segments: 1,
            hinge_edge: None,
            bridge_specific: false,
            bridge_edges: [None, None],
            msmooth_smoothness: 1.0,
            msmooth_separate_by_smoothing: false,
            msmooth_separate_by_materials: false,
            tess_kind: TessKind::Face,
            tess_tension: 25.0,
            relax_amount: 0.5,
            relax_iterations: 1,
            relax_hold_boundary: true,
            relax_hold_outer: false,
            auto_smooth_threshold: 45.0,
        }
    }
}

/// Align dialog > Current Object / Target Object point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AlignPoint {
    Minimum,
    #[default]
    Center,
    PivotPoint,
    Maximum,
}

impl AlignPoint {
    pub const ALL: [AlignPoint; 4] = [AlignPoint::Minimum, AlignPoint::Center, AlignPoint::PivotPoint, AlignPoint::Maximum];

    pub fn label(self) -> &'static str {
        match self {
            AlignPoint::Minimum => "Minimum",
            AlignPoint::Center => "Center",
            AlignPoint::PivotPoint => "Pivot Point",
            AlignPoint::Maximum => "Maximum",
        }
    }
}

/// Mirror dialog > Mirror Axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MirrorAxis {
    #[default]
    X,
    Y,
    Z,
    XY,
    YZ,
    ZX,
}

impl MirrorAxis {
    pub const ALL: [MirrorAxis; 6] = [MirrorAxis::X, MirrorAxis::Y, MirrorAxis::Z, MirrorAxis::XY, MirrorAxis::YZ, MirrorAxis::ZX];

    pub fn label(self) -> &'static str {
        match self {
            MirrorAxis::X => "X",
            MirrorAxis::Y => "Y",
            MirrorAxis::Z => "Z",
            MirrorAxis::XY => "XY",
            MirrorAxis::YZ => "YZ",
            MirrorAxis::ZX => "ZX",
        }
    }

    /// Which frame axes are negated.
    pub fn flips(self) -> [bool; 3] {
        match self {
            MirrorAxis::X => [true, false, false],
            MirrorAxis::Y => [false, true, false],
            MirrorAxis::Z => [false, false, true],
            MirrorAxis::XY => [true, true, false],
            MirrorAxis::YZ => [false, true, true],
            MirrorAxis::ZX => [true, false, true],
        }
    }
}

/// Spacing Tool > Context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SpacingContext {
    #[default]
    Centers,
    Edges,
}

/// The two surface picks of a Normal Align, kept for the dialog's offsets.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormalAlignPicks {
    pub source_point: Vec3,
    pub source_normal: Vec3,
    pub target_point: Vec3,
    pub target_normal: Vec3,
}

/// Main Toolbar > Selection Filter list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelectionFilter {
    #[default]
    All,
    Geometry,
    Shapes,
    Lights,
    Cameras,
    Helpers,
    Bone,
    IKChainObject,
    Point,
}

impl SelectionFilter {
    pub const ALL: [SelectionFilter; 9] = [
        SelectionFilter::All, SelectionFilter::Geometry, SelectionFilter::Shapes, SelectionFilter::Lights,
        SelectionFilter::Cameras, SelectionFilter::Helpers, SelectionFilter::Bone, SelectionFilter::IKChainObject, SelectionFilter::Point,
    ];
    pub fn label(self) -> &'static str {
        match self {
            SelectionFilter::All => "All",
            SelectionFilter::Geometry => "Geometry",
            SelectionFilter::Shapes => "Shapes",
            SelectionFilter::Lights => "Lights",
            SelectionFilter::Cameras => "Cameras",
            SelectionFilter::Helpers => "Helpers",
            SelectionFilter::Bone => "Bone",
            SelectionFilter::IKChainObject => "IK Chain Object",
            SelectionFilter::Point => "Point",
        }
    }
    /// Whether a node passes the filter.
    pub fn accepts(self, node: &Node) -> bool {
        match self {
            SelectionFilter::All => true,
            SelectionFilter::Geometry => !matches!(node.object, ObjectKind::Camera { .. } | ObjectKind::Light(_)) && !node.object.is_shape(),
            SelectionFilter::Shapes => node.object.is_shape(),
            SelectionFilter::Cameras => matches!(node.object, ObjectKind::Camera { .. }),
            SelectionFilter::Lights => matches!(node.object, ObjectKind::Light(_)),
            // No helpers, bones, IK chains or points exist yet.
            _ => false,
        }
    }
}

/// Main Toolbar > Selection Region flyout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelectionRegion {
    #[default]
    Rectangular,
    Circular,
    Fence,
    Lasso,
    Paint,
}

impl SelectionRegion {
    pub const ALL: [SelectionRegion; 5] = [SelectionRegion::Rectangular, SelectionRegion::Circular, SelectionRegion::Fence, SelectionRegion::Lasso, SelectionRegion::Paint];
    pub fn label(self) -> &'static str {
        match self {
            SelectionRegion::Rectangular => "Rectangular Selection Region",
            SelectionRegion::Circular => "Circular Selection Region",
            SelectionRegion::Fence => "Fence Selection Region",
            SelectionRegion::Lasso => "Lasso Selection Region",
            SelectionRegion::Paint => "Paint Selection Region",
        }
    }
}

/// Main Toolbar > Scale flyout: Uniform, Non-Uniform or Squash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScaleKind {
    #[default]
    Uniform,
    NonUniform,
    Squash,
}

impl ScaleKind {
    pub const ALL: [ScaleKind; 3] = [ScaleKind::Uniform, ScaleKind::NonUniform, ScaleKind::Squash];
    pub fn label(self) -> &'static str {
        match self {
            ScaleKind::Uniform => "Select and Uniform Scale",
            ScaleKind::NonUniform => "Select and Non-uniform Scale",
            ScaleKind::Squash => "Select and Squash",
        }
    }
    /// The next flyout button (Scale Cycle).
    pub fn next(self) -> ScaleKind {
        match self {
            ScaleKind::Uniform => ScaleKind::NonUniform,
            ScaleKind::NonUniform => ScaleKind::Squash,
            ScaleKind::Squash => ScaleKind::Uniform,
        }
    }
}

/// Main Toolbar > Reference Coordinate System.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CoordSystem {
    #[default]
    View,
    Screen,
    World,
    Parent,
    Local,
    Gimbal,
    Grid,
    /// The working pivot's axes (Hierarchy > Working Pivot).
    Working,
    Pick,
}

impl CoordSystem {
    pub const ALL: [CoordSystem; 9] = [CoordSystem::View, CoordSystem::Screen, CoordSystem::World, CoordSystem::Parent, CoordSystem::Local, CoordSystem::Gimbal, CoordSystem::Grid, CoordSystem::Working, CoordSystem::Pick];
    pub fn label(self) -> &'static str {
        match self {
            CoordSystem::View => "View",
            CoordSystem::Screen => "Screen",
            CoordSystem::World => "World",
            CoordSystem::Parent => "Parent",
            CoordSystem::Local => "Local",
            CoordSystem::Gimbal => "Gimbal",
            CoordSystem::Grid => "Grid",
            CoordSystem::Working => "Working",
            CoordSystem::Pick => "Pick",
        }
    }
}

/// Preferences > Gizmos (the settings the gizmos read).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GizmoPrefs {
    /// "Transform Gizmos: On" (Shift+Ctrl+X toggles).
    pub enabled: bool,
    pub show_axis_labels: bool,
    pub allow_multiple: bool,
    /// Gizmo size as a percent of the viewport height (1–100; "=" / "-" step 2).
    /// Older prefs stored `size_px` (pixels); serde ignores that key and this defaults.
    #[serde(default = "default_gizmo_size")]
    pub size: f32,
    /// Move gizmo: plane handle size as a percent of the axis length (default 50).
    pub move_plane_size: f32,
    /// Move gizmo: plane handle offset as a percent of the axis length (default 50).
    pub move_plane_offset: f32,
    /// Move gizmo: dragging the centre box moves in screen space.
    pub center_box_handle: bool,
    pub rotate_free_rotation: bool,
    pub rotate_show_tripod: bool,
    pub rotate_screen_handle: bool,
    pub rotate_show_pie_slice: bool,
    pub rotate_angle_data: bool,
    /// Scale gizmo size as a percent of the gizmo size (default 100).
    pub scale_size: f32,
    /// Keyboard rotation increment in degrees (default 10).
    pub rotation_increment: f32,
    /// Viewport orbit snap angle in degrees (default 10).
    pub orbit_snap_angle: f32,
}

fn default_gizmo_size() -> f32 {
    20.0
}

fn default_true() -> bool {
    true
}

impl Default for GizmoPrefs {
    fn default() -> Self {
        Self {
            enabled: true,
            show_axis_labels: true,
            allow_multiple: false,
            size: default_gizmo_size(),
            move_plane_size: 50.0,
            move_plane_offset: 50.0,
            center_box_handle: true,
            rotate_free_rotation: true,
            rotate_show_tripod: false,
            rotate_screen_handle: true,
            rotate_show_pie_slice: true,
            rotate_angle_data: true,
            scale_size: 100.0,
            rotation_increment: 10.0,
            orbit_snap_angle: 10.0,
        }
    }
}

/// Keep Creating's default (the reference keeps the create tool on; this
/// build makes one object, then returns to Select).
pub const KEEP_CREATING_DEFAULT: bool = false;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub gizmos: GizmoPrefs,
    /// Preferences > General > Spinners > Use Snap (Spinner Snap Toggle).
    pub spinner_snap: bool,
    /// Preferences > General > Spinners > Snap value (default 1.0).
    pub spinner_snap_value: f32,
    /// Preferences > General > Spinners > Precision (decimals, default 3).
    pub spinner_precision: usize,
    pub spinner_wrap_cursor: bool,
    /// Preferences > General > Scene Undo > Levels (default 20).
    pub undo_levels: usize,
    /// Preferences > General > Reference Coordinate System > Constant.
    pub ref_coord_constant: bool,
    /// Preferences > General > Auto Window/Crossing by Direction.
    pub auto_window_crossing: bool,
    /// Preferences > General > Paint Selection Brush Size (px, default 20).
    pub paint_brush_size: f32,
    /// Preferences > Viewports > Viewport tooltips / Show cross hair cursor.
    pub viewport_tooltips: bool,
    pub show_cross_hair: bool,
    /// Preferences > General: Space toggles Selection Lock (off by default).
    #[serde(default)]
    pub space_selection_lock: bool,
    /// Preferences > General / Create menu / the Object Type Pin: the create
    /// tool stays on after an object is made (off: one object, then Select).
    pub keep_creating: bool,
    /// Preferences > Files > Auto Backup: Enable (default on).
    pub auto_backup_enable: bool,
    /// Number of Autobak files (default 3).
    pub auto_backup_count: u32,
    /// Backup Interval (minutes, default 5.0).
    pub auto_backup_interval_min: f32,
    /// Auto Backup File Name (default "AutoBackup").
    pub auto_backup_name: String,
    /// Preferences > Animation > Local Center During Animate: with Auto Key
    /// on, Rotate and Scale use each object's pivot (default on).
    #[serde(default = "default_true")]
    pub local_center_during_animate: bool,
    /// Preferences > Files > Recent Files in File Menu (0-50, default 10).
    pub recent_files_count: usize,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            gizmos: GizmoPrefs::default(),
            spinner_snap: false,
            spinner_snap_value: 1.0,
            spinner_precision: 3,
            spinner_wrap_cursor: true,
            undo_levels: 20,
            ref_coord_constant: false,
            auto_window_crossing: false,
            paint_brush_size: 20.0,
            viewport_tooltips: true,
            show_cross_hair: false,
            space_selection_lock: false,
            keep_creating: KEEP_CREATING_DEFAULT,
            auto_backup_enable: true,
            auto_backup_count: 3,
            auto_backup_interval_min: 5.0,
            auto_backup_name: "AutoBackup".into(),
            local_center_during_animate: true,
            recent_files_count: 10,
        }
    }
}

impl Prefs {
    /// Preference names for `Set Preference`, with their current values.
    pub const NAMES: [&'static str; 31] = [
        "Auto Backup Enable", "Auto Backup Count", "Auto Backup Interval",
        "Scene Undo Levels", "Reference Coordinate System Constant", "Auto Window/Crossing by Direction", "Paint Selection Brush Size",
        "Spinner Precision", "Spinner Snap", "Spinner Use Snap", "Spinner Wrap Cursor", "Viewport Tooltips", "Show Cross Hair Cursor",
        "Transform Gizmos", "Show Axis Labels", "Allow Multiple Gizmos", "Gizmo Size", "Move Plane Handle Size", "Move Plane Handle Offset",
        "Center Box Handle", "Rotate Free Rotation", "Rotate Show Tripod", "Rotate Screen Handle", "Rotate Show Pie Slice", "Rotate Angle Data",
        "Scale Size", "Rotation Increment", "Space Locks Selection", "Local Center During Animate", "Recent Files in File Menu", "Keep Creating",
    ];

    pub fn get(&self, name: &str) -> Option<f64> {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        Some(match name {
            "Scene Undo Levels" => self.undo_levels as f64,
            "Reference Coordinate System Constant" => b(self.ref_coord_constant),
            "Auto Window/Crossing by Direction" => b(self.auto_window_crossing),
            "Paint Selection Brush Size" => self.paint_brush_size as f64,
            "Spinner Precision" => self.spinner_precision as f64,
            "Spinner Snap" => self.spinner_snap_value as f64,
            "Spinner Use Snap" => b(self.spinner_snap),
            "Spinner Wrap Cursor" => b(self.spinner_wrap_cursor),
            "Viewport Tooltips" => b(self.viewport_tooltips),
            "Space Locks Selection" => b(self.space_selection_lock),
            "Keep Creating" => b(self.keep_creating),
            "Local Center During Animate" => b(self.local_center_during_animate),
            "Show Cross Hair Cursor" => b(self.show_cross_hair),
            "Auto Backup Enable" => b(self.auto_backup_enable),
            "Auto Backup Count" => self.auto_backup_count as f64,
            "Auto Backup Interval" => self.auto_backup_interval_min as f64,
            "Recent Files in File Menu" => self.recent_files_count as f64,
            "Transform Gizmos" => b(self.gizmos.enabled),
            "Show Axis Labels" => b(self.gizmos.show_axis_labels),
            "Allow Multiple Gizmos" => b(self.gizmos.allow_multiple),
            "Gizmo Size" => self.gizmos.size as f64,
            "Move Plane Handle Size" => self.gizmos.move_plane_size as f64,
            "Move Plane Handle Offset" => self.gizmos.move_plane_offset as f64,
            "Center Box Handle" => b(self.gizmos.center_box_handle),
            "Rotate Free Rotation" => b(self.gizmos.rotate_free_rotation),
            "Rotate Show Tripod" => b(self.gizmos.rotate_show_tripod),
            "Rotate Screen Handle" => b(self.gizmos.rotate_screen_handle),
            "Rotate Show Pie Slice" => b(self.gizmos.rotate_show_pie_slice),
            "Rotate Angle Data" => b(self.gizmos.rotate_angle_data),
            "Scale Size" => self.gizmos.scale_size as f64,
            "Rotation Increment" => self.gizmos.rotation_increment as f64,
            "Viewport Orbit Snap Angle" => self.gizmos.orbit_snap_angle as f64,
            _ => return None,
        })
    }

    pub fn set(&mut self, name: &str, v: f64) -> Result<(), String> {
        let on = v != 0.0;
        let f = v as f32;
        match name {
            "Scene Undo Levels" => self.undo_levels = (v.max(1.0)) as usize,
            "Reference Coordinate System Constant" => self.ref_coord_constant = on,
            "Auto Window/Crossing by Direction" => self.auto_window_crossing = on,
            "Paint Selection Brush Size" => self.paint_brush_size = f.max(1.0),
            "Spinner Precision" => self.spinner_precision = v.clamp(0.0, 10.0) as usize,
            "Spinner Snap" => self.spinner_snap_value = f.max(1e-6),
            "Spinner Use Snap" => self.spinner_snap = on,
            "Spinner Wrap Cursor" => self.spinner_wrap_cursor = on,
            "Viewport Tooltips" => self.viewport_tooltips = on,
            "Space Locks Selection" => self.space_selection_lock = on,
            "Keep Creating" => self.keep_creating = on,
            "Local Center During Animate" => self.local_center_during_animate = on,
            "Auto Backup Enable" => self.auto_backup_enable = on,
            "Auto Backup Count" => self.auto_backup_count = v.clamp(1.0, 99.0) as u32,
            "Auto Backup Interval" => self.auto_backup_interval_min = f.clamp(0.1, 1440.0),
            "Recent Files in File Menu" => self.recent_files_count = v.clamp(0.0, 50.0) as usize,
            "Show Cross Hair Cursor" => self.show_cross_hair = on,
            "Transform Gizmos" => self.gizmos.enabled = on,
            "Show Axis Labels" => self.gizmos.show_axis_labels = on,
            "Allow Multiple Gizmos" => self.gizmos.allow_multiple = on,
            "Gizmo Size" => self.gizmos.size = f.clamp(1.0, 100.0),
            "Move Plane Handle Size" => self.gizmos.move_plane_size = f.clamp(1.0, 100.0),
            "Move Plane Handle Offset" => self.gizmos.move_plane_offset = f.clamp(0.0, 100.0),
            "Center Box Handle" => self.gizmos.center_box_handle = on,
            "Rotate Free Rotation" => self.gizmos.rotate_free_rotation = on,
            "Rotate Show Tripod" => self.gizmos.rotate_show_tripod = on,
            "Rotate Screen Handle" => self.gizmos.rotate_screen_handle = on,
            "Rotate Show Pie Slice" => self.gizmos.rotate_show_pie_slice = on,
            "Rotate Angle Data" => self.gizmos.rotate_angle_data = on,
            "Scale Size" => self.gizmos.scale_size = f.clamp(10.0, 400.0),
            "Rotation Increment" => self.gizmos.rotation_increment = f.max(0.0),
            "Viewport Orbit Snap Angle" => self.gizmos.orbit_snap_angle = f.max(0.0),
            _ => return Err(format!("unknown preference {name:?}")),
        }
        Ok(())
    }
}

/// Units Setup: system unit and display unit scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SystemUnit {
    #[default]
    Inches,
    Feet,
    Millimeters,
    Centimeters,
    Meters,
    Kilometers,
}

impl SystemUnit {
    pub const ALL: [SystemUnit; 6] = [SystemUnit::Inches, SystemUnit::Feet, SystemUnit::Millimeters, SystemUnit::Centimeters, SystemUnit::Meters, SystemUnit::Kilometers];

    pub fn label(self) -> &'static str {
        match self {
            SystemUnit::Inches => "Inches",
            SystemUnit::Feet => "Feet",
            SystemUnit::Millimeters => "Millimeters",
            SystemUnit::Centimeters => "Centimeters",
            SystemUnit::Meters => "Meters",
            SystemUnit::Kilometers => "Kilometers",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.label().eq_ignore_ascii_case(s))
    }

    pub fn metres(self) -> f64 {
        match self {
            SystemUnit::Inches => 0.0254,
            SystemUnit::Feet => 0.3048,
            SystemUnit::Millimeters => 0.001,
            SystemUnit::Centimeters => 0.01,
            SystemUnit::Meters => 1.0,
            SystemUnit::Kilometers => 1000.0,
        }
    }
}

/// Display Unit Scale choices (Metric, US Standard, Custom, Generic).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum DisplayUnits {
    #[default]
    Generic,
    Millimeters,
    Centimeters,
    Meters,
    Kilometers,
    DecimalInches,
    DecimalFeet,
    FeetDecimalInches,
    Custom { name: String, value: f32, unit: SystemUnit },
}

impl DisplayUnits {
    pub const NAMES: [&'static str; 9] = ["Generic", "Millimeters", "Centimeters", "Meters", "Kilometers", "Decimal Inches", "Decimal Feet", "Feet w/ Decimal Inches", "Custom"];

    pub fn label(&self) -> &'static str {
        match self {
            DisplayUnits::Generic => "Generic",
            DisplayUnits::Millimeters => "Millimeters",
            DisplayUnits::Centimeters => "Centimeters",
            DisplayUnits::Meters => "Meters",
            DisplayUnits::Kilometers => "Kilometers",
            DisplayUnits::DecimalInches => "Decimal Inches",
            DisplayUnits::DecimalFeet => "Decimal Feet",
            DisplayUnits::FeetDecimalInches => "Feet w/ Decimal Inches",
            DisplayUnits::Custom { .. } => "Custom",
        }
    }

    /// Size of one display unit in metres and its suffix (None = generic).
    fn scale(&self) -> Option<(f64, String)> {
        Some(match self {
            DisplayUnits::Generic => return None,
            DisplayUnits::Millimeters => (0.001, "mm".into()),
            DisplayUnits::Centimeters => (0.01, "cm".into()),
            DisplayUnits::Meters => (1.0, "m".into()),
            DisplayUnits::Kilometers => (1000.0, "km".into()),
            DisplayUnits::DecimalInches => (0.0254, "\"".into()),
            DisplayUnits::DecimalFeet | DisplayUnits::FeetDecimalInches => (0.3048, "'".into()),
            DisplayUnits::Custom { name, value, unit } => ((*value as f64) * unit.metres(), name.clone()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitsSetup {
    pub system_unit: SystemUnit,
    /// System Unit Setup: 1 unit = `system_scale` system units.
    pub system_scale: f32,
    pub display: DisplayUnits,
}

impl Default for UnitsSetup {
    fn default() -> Self {
        Self { system_unit: SystemUnit::Inches, system_scale: 1.0, display: DisplayUnits::Generic }
    }
}

impl UnitsSetup {
    /// Display units per scene unit, and the suffix ("" for generic units).
    pub fn display_factor(&self) -> (f64, String) {
        match self.display.scale() {
            None => (1.0, String::new()),
            Some((metres_per_display, suffix)) => ((self.system_scale as f64) * self.system_unit.metres() / metres_per_display, suffix),
        }
    }

    /// A typed length in scene units. A plain number is in display units; a
    /// number with a unit (7", -3.5in, 2', 1ft, 5'7", 10mm, 2.5cm, 1m, 0.1km
    /// or the custom unit's name) is converted. Typographic quotes and the
    /// en dash count as " ' and -.
    pub fn parse(&self, text: &str) -> Option<f64> {
        let t: String = text
            .trim()
            .chars()
            .map(|c| match c {
                '\u{201c}' | '\u{201d}' | '\u{2033}' => '"',
                '\u{2018}' | '\u{2019}' | '\u{2032}' => '\'',
                '\u{2013}' | '\u{2212}' => '-',
                c => c,
            })
            .collect();
        let scene_metres = self.system_scale as f64 * self.system_unit.metres();
        if let Some((ft, rest)) = t.split_once('\'') {
            let feet: f64 = ft.trim().parse().ok()?;
            let rest = rest.trim().trim_end_matches('"').trim();
            let inches: f64 = if rest.is_empty() { 0.0 } else { rest.parse().ok()? };
            let sign = if ft.trim().starts_with('-') { -1.0 } else { 1.0 };
            return Some((feet + sign * inches / 12.0) * (0.3048 / scene_metres));
        }
        let i = t.find(|c: char| c.is_alphabetic() || c == '"').unwrap_or(t.len());
        let v: f64 = t[..i].trim().parse().ok()?;
        let unit = t[i..].trim().to_ascii_lowercase();
        let metres = match unit.as_str() {
            "" => return Some(v / self.display_factor().0),
            "\"" | "in" => 0.0254,
            "ft" => 0.3048,
            "mm" => 0.001,
            "cm" => 0.01,
            "m" => 1.0,
            "km" => 1000.0,
            u if !u.is_empty() && u.eq_ignore_ascii_case(&self.display_factor().1) => return Some(v / self.display_factor().0),
            _ => return None,
        };
        Some(v * (metres / scene_metres))
    }

    pub fn format(&self, v: f32, decimals: usize) -> String {
        let (f, suffix) = self.display_factor();
        let d = v as f64 * f;
        let rounded = (d * 10f64.powi(decimals as i32)).round() / 10f64.powi(decimals as i32);
        if suffix.is_empty() {
            return trim_float(rounded, decimals);
        }
        if self.display == DisplayUnits::FeetDecimalInches {
            let feet = rounded.trunc();
            let inches = (rounded - feet) * 12.0;
            return format!("{}'{}\"", feet as i64, trim_float((inches * 1000.0).round() / 1000.0, decimals));
        }
        format!("{}{suffix}", trim_float(rounded, decimals))
    }
}

/// Decimal string with at most `decimals` places and at least one.
fn trim_float(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = s.trim_end_matches('0');
    let s = s.strip_suffix('.').map(|t| format!("{t}.0")).unwrap_or_else(|| s.to_owned());
    if s.is_empty() || s == "-" { "0.0".into() } else { s }
}

/// Clone Options dialog > Object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CloneKind {
    #[default]
    Copy,
    Instance,
    /// Until the modifier stack exists a reference behaves as an instance.
    Reference,
}

/// Axis Constraints toolbar state (F5/F6/F7 axes, F8 cycles the planes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AxisConstraint {
    X,
    Y,
    Z,
    #[default]
    XY,
    YZ,
    ZX,
    XYZ,
}

/// Main Toolbar > Use Center flyout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CenterMode {
    #[default]
    PivotPoint,
    Selection,
    TransformCoordinate,
}

impl CenterMode {
    pub const ALL: [CenterMode; 3] = [CenterMode::PivotPoint, CenterMode::Selection, CenterMode::TransformCoordinate];
    pub fn label(self) -> &'static str {
        match self {
            CenterMode::PivotPoint => "Use Pivot Point Center",
            CenterMode::Selection => "Use Selection Center",
            CenterMode::TransformCoordinate => "Use Transform Coordinate Center",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SubObjectId {
    Vertex(VertexId),
    Edge(HalfEdgeId),
    Face(FaceId),
}

/// Sub-object selection per node.
#[derive(Debug, Clone, Default)]
pub struct SubSelection {
    items: BTreeMap<NodeId, BTreeSet<SubObjectId>>,
}

impl SubSelection {
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.items.values().all(|s| s.is_empty())
    }
    pub fn len(&self) -> usize {
        self.items.values().map(|s| s.len()).sum()
    }
    pub fn insert(&mut self, node: NodeId, id: SubObjectId) {
        self.items.entry(node).or_default().insert(id);
    }
    pub fn remove(&mut self, node: NodeId, id: SubObjectId) -> bool {
        self.items.get_mut(&node).is_some_and(|s| s.remove(&id))
    }
    pub fn contains(&self, node: NodeId, id: SubObjectId) -> bool {
        self.items.get(&node).is_some_and(|s| s.contains(&id))
    }
    pub fn toggle(&mut self, node: NodeId, id: SubObjectId) {
        if !self.remove(node, id) {
            self.insert(node, id);
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, SubObjectId)> + '_ {
        self.items
            .iter()
            .flat_map(|(n, s)| s.iter().map(move |id| (*n, *id)))
    }
    pub fn by_node(&self) -> impl Iterator<Item = (NodeId, &BTreeSet<SubObjectId>)> + '_ {
        self.items.iter().map(|(n, s)| (*n, s))
    }
    pub fn retain_nodes(&mut self, keep: impl Fn(NodeId) -> bool) {
        self.items.retain(|n, _| keep(*n));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Select,
    Move,
    Rotate,
    Scale,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Select, Tool::Move, Tool::Rotate, Tool::Scale];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select Object",
            Tool::Move => "Select and Move",
            Tool::Rotate => "Select and Rotate",
            Tool::Scale => "Select and Scale",
        }
    }
}

/// Viewport Configuration > ViewCube.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewCubeSettings {
    /// Show the ViewCube (Alt+Ctrl+V).
    pub show: bool,
    /// Show in All Views (else only in the active one).
    pub all_views: bool,
    /// ViewCube Size: 0 Large, 1 Normal, 2 Small, 3 Tiny.
    pub size: u8,
    /// Inactive Opacity, 0..=1.
    pub inactive_opacity: f32,
    /// Show Compass Below the ViewCube.
    pub compass: bool,
}

impl Default for ViewCubeSettings {
    fn default() -> Self {
        Self { show: true, all_views: false, size: 1, inactive_opacity: 0.5, compass: true }
    }
}

/// A stored view: the ViewCube Home view and Views > Save Active View.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SavedView {
    pub target: [f32; 3],
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    #[serde(default)]
    pub roll: f32,
    pub fov_y_radians: f32,
    /// Save Active View keeps the shading; the Home view does not.
    #[serde(default)]
    pub shading: Option<crate::viewport::Shading>,
}

impl SavedView {
    pub fn of(vp: &crate::viewport::Viewport) -> Self {
        let c = &vp.camera;
        Self { target: c.target.to_array(), distance: c.distance, yaw: c.yaw, pitch: c.pitch, roll: c.roll, fov_y_radians: vp.fov_y_radians, shading: None }
    }

    pub fn camera(&self) -> OrbitCamera {
        OrbitCamera { target: Vec3::from_array(self.target), distance: self.distance, yaw: self.yaw, pitch: self.pitch, roll: self.roll }
    }
}

/// Which viewport view the (single, for now) viewport shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewKind {
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
    Perspective,
    Orthographic,
    Camera(NodeId),
}

impl ViewKind {
    /// Viewport label.
    pub fn label(self) -> &'static str {
        match self {
            ViewKind::Top => "Top",
            ViewKind::Bottom => "Bottom",
            ViewKind::Front => "Front",
            ViewKind::Back => "Back",
            ViewKind::Left => "Left",
            ViewKind::Right => "Right",
            ViewKind::Perspective => "Perspective",
            ViewKind::Orthographic => "Orthographic",
            ViewKind::Camera(_) => "Camera",
        }
    }

    /// Orbit (yaw, pitch) in radians for the axis views; `None` for the
    /// free views, which keep the current angles.
    pub fn angles(self) -> Option<(f32, f32)> {
        use std::f32::consts::{FRAC_PI_2, PI};
        Some(match self {
            ViewKind::Front => (-FRAC_PI_2, 0.0),
            ViewKind::Back => (FRAC_PI_2, 0.0),
            ViewKind::Left => (PI, 0.0),
            ViewKind::Right => (0.0, 0.0),
            // Eye on the -Y side so +X is screen-right and +Y screen-up.
            ViewKind::Top => (-FRAC_PI_2, 1.5697),
            ViewKind::Bottom => (-FRAC_PI_2, -1.5697),
            ViewKind::Perspective | ViewKind::Orthographic | ViewKind::Camera(_) => return None,
        })
    }

    pub fn is_perspective(self) -> bool {
        matches!(self, ViewKind::Perspective | ViewKind::Camera(_))
    }

    /// Construction plane of the view as (right, up, normal) world axes:
    /// Objects are created on the plane the view's home grid lies in.
    pub fn construction_plane(self) -> (Vec3, Vec3, Vec3) {
        match self {
            ViewKind::Front => (Vec3::X, Vec3::Z, -Vec3::Y),
            ViewKind::Back => (-Vec3::X, Vec3::Z, Vec3::Y),
            ViewKind::Left => (-Vec3::Y, Vec3::Z, -Vec3::X),
            ViewKind::Right => (Vec3::Y, Vec3::Z, Vec3::X),
            ViewKind::Bottom => (Vec3::X, -Vec3::Y, -Vec3::Z),
            _ => (Vec3::X, Vec3::Y, Vec3::Z),
        }
    }
}

/// Snapshot of what Shift+Z (Undo Viewport Operation) restores.
#[derive(Debug, Clone)]
pub struct ViewState {
    pub camera: OrbitCamera,
    pub view_kind: ViewKind,
    pub fov_y_radians: f32,
}

/// Simple snapshot stack for viewport undo (Shift+Z / Shift+Y).
#[derive(Debug, Default, Clone)]
pub struct ViewUndo {
    undo: Vec<ViewState>,
    redo: Vec<ViewState>,
}

impl ViewUndo {
    pub fn record(&mut self, before: ViewState) {
        self.undo.push(before);
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn undo(&mut self, current: ViewState) -> Option<ViewState> {
        let prev = self.undo.pop()?;
        self.redo.push(current);
        Some(prev)
    }
    pub fn redo(&mut self, current: ViewState) -> Option<ViewState> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }
}

pub struct AppState {
    pub scene: Scene,
    pub undo: UndoStack,
    pub selection: BTreeSet<NodeId>,
    pub sub_selection: SubSelection,
    /// Selected control points of the active FFD entry (lattice indices).
    pub ffd_selection: BTreeSet<u32>,
    /// VertexPaint: the viewport brush is on.
    pub vertex_paint_brush: bool,
    /// VertexPaint: Pick Color From Object (the eyedropper) is on.
    pub vertex_paint_pick: bool,
    /// VertexPaint: Face Selection is on (viewport clicks select the faces painting is limited to).
    pub vertex_paint_faces: bool,
    pub selection_mode: SelectionMode,
    pub active_tool: Tool,
    pub viewports: Vec<Viewport>,
    pub active_viewport: usize,
    pub layout: ViewportLayout,
    pub splits: LayoutSplits,
    pub maximized: bool,
    pub nav_mode: NavMode,
    pub show_vertices: bool,
    pub show_statistics: bool,
    /// Nodes hidden by Isolate Selection (restored on exit).
    pub isolation: Option<Vec<NodeId>>,
    pub expert_mode: bool,
    pub show_main_toolbar: bool,
    pub show_ribbon: bool,
    /// Customize > Show UI > Show Command Panel.
    pub show_command_panel: bool,
    /// The Show UI toggles as loaded at startup (Revert to Startup Layout).
    pub startup_ui: crate::settings::UiLayout,
    /// Modify panel: Show End Result, Pin Stack, modifier button set.
    pub show_end_result: bool,
    pub pin_stack: bool,
    pub show_modifier_set: bool,
    pub modifier_set: Vec<String>,
    /// Edit Modifier Stack asked the UI to focus the Modify tab.
    pub focus_modify: bool,
    pub ui_locked: bool,
    pub shade_selected_faces: bool,
    /// Views > Shade Selected: selected objects shaded in wireframe views.
    pub shade_selected: bool,
    /// Viewport Configuration > ViewCube.
    pub viewcube: ViewCubeSettings,
    /// The Viewport Configuration panel to show when the dialog opens next.
    pub config_panel: Option<String>,
    /// ViewCube Home view (Set Current View as Home); None = the default Perspective.
    pub home_view: Option<SavedView>,
    /// Views > Save Active View, one per view label ("Top", "Perspective"...).
    pub saved_views: BTreeMap<String, SavedView>,
    pub selection_brackets: bool,
    pub hide_floating_dialogs: bool,
    pub hide_category: HideCategories,
    pub dialog: Option<Dialog>,
    /// Rendering > Material Editor (M): its own modeless window, open beside
    /// `dialog` so the bitmap chooser its Maps buttons open does not close it.
    pub material_editor_open: bool,
    /// Display panel and viewport display settings (Display Color, statistics, Object Color dialog...).
    pub display: crate::display::DisplaySettings,
    /// What a Select From Scene style dialog is for (Hide/Freeze by Name...).
    pub dialog_purpose: DialogPurpose,
    pub pick_mode: Option<PickMode>,
    /// Target picked for the Align dialog.
    pub align_target: Option<NodeId>,
    /// Normal Align: the source pick while waiting for the target, then both.
    pub normal_align_source: Option<(Vec3, Vec3)>,
    pub normal_align: Option<NormalAlignPicks>,
    /// Spacing Tool > Pick Points: the two picked path ends.
    pub spacing_points: [Option<Vec3>; 2],
    pub selection_filter: SelectionFilter,
    /// Main Toolbar > Window/Crossing: true = Window (fully enclosed only).
    pub window_selection: bool,
    pub selection_region: SelectionRegion,
    /// The Scale flyout's button: how Select and Scale scales.
    pub scale_kind: ScaleKind,
    pub coord_system: CoordSystem,
    /// Node whose coordinate system `CoordSystem::Pick` uses.
    pub pick_coord_node: Option<NodeId>,
    /// The four most recently picked systems, newest first (names in the list).
    pub pick_coord_recent: Vec<NodeId>,
    pub center_mode: CenterMode,
    /// The centre is remembered separately for single and multiple selections.
    pub center_single: CenterMode,
    pub center_multi: CenterMode,
    pub axis_constraint: AxisConstraint,
    /// Customize > Preferences values that commands and the UI read.
    pub prefs: Prefs,
    /// Customize > Units Setup.
    pub units: UnitsSetup,
    /// The units Units Setup last set: new and reset scenes start with them
    /// and the settings file keeps them (an opened file's units do not).
    pub default_units: UnitsSetup,
    /// Customize User Interface > Keyboard: the user's shortcut edits.
    pub custom_keys: crate::keymap::CustomKeys,
    /// File > Recent: opened and saved scenes, newest first.
    pub recent_files: Vec<std::path::PathBuf>,
    /// Set when the selection region shape changes so the UI drops a
    /// half-drawn fence/lasso.
    pub region_points_reset: bool,
    /// Select and Manipulate (Main Toolbar): manipulators active.
    pub manipulate: bool,
    /// Reference coordinate system remembered per transform tool
    /// (Preferences > Reference Coordinate System > Constant off).
    pub coord_by_tool: [CoordSystem; 4],
    /// Use Center (single, multiple) and axis constraint remembered per
    /// transform tool, like `coord_by_tool`; the constraint also per level
    /// ([object, sub-object][tool]).
    pub center_by_tool: [(CenterMode, CenterMode); 4],
    pub constraint_by_tool: [[AxisConstraint; 4]; 2],
    /// Which level `axis_constraint` belongs to (true = sub-object).
    pub constraint_level_sub: bool,
    /// Keyboard Shortcut Override Toggle: when off only Main UI shortcuts work.
    pub keyboard_override: bool,
    pub named_sets: BTreeMap<String, Vec<NodeId>>,
    /// Editable Poly > Selection rollout toggles.
    pub select_by_vertex: bool,
    pub ignore_backfacing: bool,
    /// Modify Selection > Loop Mode: a click selects the whole loop.
    pub loop_mode: bool,
    pub preview_selection: PreviewSelect,
    /// Edit Geometry > Named Selections (sub-object).
    pub named_sub_selections: BTreeMap<String, Vec<(NodeId, SubObjectId)>>,
    /// Soft Selection > Paint Soft Selection brush mode.
    pub soft_paint: Option<SoftPaint>,
    /// Soft Selection > Shaded Face Toggle.
    pub soft_shaded_faces: bool,
    /// Editable Poly settings-window values.
    pub poly_settings: PolySettings,
    /// Editable Spline > Geometry / Selection rollout settings.
    pub spline_settings: crate::spline::SplineSettings,
    /// Create Line (Spline): the shape the Line tool's next line attaches to.
    pub spline_create_line_target: Option<NodeId>,
    /// Compound Objects rollout settings.
    pub compound_settings: CompoundSettings,
    /// Active Extrude / Chamfer drag mode button.
    pub poly_drag: Option<PolyDrag>,
    /// The last geometry operation, for Repeat Last Operation (;).
    pub last_poly_op: Option<crate::command::Command>,
    /// Target Weld / Edit Triangulation first pick.
    pub pick_source: Option<(NodeId, u32)>,
    /// Create (polygon level): vertices picked so far.
    pub create_verts: Vec<(NodeId, u32)>,
    /// Slice Plane gizmo and whether it is shown (Slice Plane mode).
    pub slice_plane: SlicePlane,
    pub slice_plane_mode: bool,
    pub slice_split: bool,
    /// Edit Geometry > Constraints.
    pub poly_constraint: PolyConstraint,
    pub full_interactivity: bool,
    pub preserve_uvs: bool,
    /// QuickSlice / Cut first pick (world point; Cut also the face).
    pub pick_point_a: Option<(NodeId, u32, Vec3)>,
    /// Active Paint Deformation brush.
    pub paint_deform: Option<PaintMode>,
    /// Soft selection weights per node, keyed by a hash of what they depend on.
    pub soft_cache: std::cell::RefCell<std::collections::HashMap<NodeId, (u64, Vec<f32>)>>,
    /// Hierarchy > Pivot: Move/Rotate act on the pivot only.
    pub affect_pivot_only: bool,
    /// Hierarchy > Pivot: Move/Rotate act on the object offset only.
    pub affect_object_only: bool,
    /// Hierarchy > Pivot: Rotate/Scale act on the links to the children only.
    pub affect_hierarchy_only: bool,
    /// Hierarchy > Adjust Transform > Don't Affect Children.
    pub dont_affect_children: bool,
    pub working_pivot: WorkingPivot,
    /// Working Pivot > Edit Working Pivot: the transform tools move the working pivot.
    pub edit_working_pivot: bool,
    /// Working Pivot > Use Working Pivot: every transform uses the Working system.
    pub use_working_pivot: bool,
    /// Working Pivot > Place Pivot To > Align To View.
    pub working_pivot_align_to_view: bool,
    /// What Edit / Use Working Pivot restore when they end.
    pub working_pivot_saved: (Option<SavedManagers>, Option<SavedManagers>),
    pub selection_locked: bool,
    pub snaps_on: bool,
    pub angle_snap_on: bool,
    pub percent_snap_on: bool,
    /// Grid and Snap Settings > Snaps / Options.
    pub snaps: crate::snap::SnapSettings,
    /// Grid and Snap Settings > Home Grid.
    pub home_grid: crate::snap::HomeGrid,
    /// The renderer rebuilds its grid buffers when set.
    pub grid_dirty: bool,
    pub auto_key: bool,
    /// Set Key mode, and the history position / node versions its pose started from.
    pub set_key_mode: bool,
    /// Modifier List use counts (the most-used section); the UI loads and saves them.
    pub modifier_usage: crate::modifier_list::Usage,
    /// Selection List: the named selection set Set Keys keys; None = "Selected".
    pub key_selection_list: Option<String>,
    pub set_key_buffer: Option<crate::anim_cmds::SetKeyBuffer>,
    /// Key Filters (which tracks Set Keys keys).
    pub key_filters: crate::anim_cmds::KeyFilters,
    /// Default In / Out tangents for new keys.
    pub default_tangent: (loom_scene::anim::TangentType, loom_scene::anim::TangentType),
    /// Key Mode: , and . jump between keys.
    pub key_step_mode: bool,
    pub playing: bool,
    pub playback: crate::anim_cmds::Playback,
    /// Track bar right-click > Filter.
    pub track_bar_filter: crate::anim_cmds::TrackBarFilter,
    /// Show Track Bar Toggle / Show Trajectories Toggle.
    pub show_track_bar: bool,
    pub show_trajectories: bool,
    /// Motion > Trajectories > Sub-Object Keys, and the selected key frames.
    pub trajectory_keys: bool,
    pub trajectory_selected: BTreeSet<i32>,
    /// Editable Spline > Tangent Copy: the copied handle.
    pub tangent_clipboard: Option<Vec3>,
    /// Refine with Connect: the node, its connecting spline once it exists, and
    /// the last connected point (object space).
    pub refine_chain: Option<(NodeId, Option<usize>, Vec3)>,
    /// The Refine prompt's click: node, flat knot, flat segment and parameter.
    pub refine_pending: Option<(NodeId, u32, u32, f32)>,
    /// The "Close spline?" prompt's click: line node, point, Initial Type smooth.
    pub line_close_pending: Option<(NodeId, [f32; 3], bool)>,
    /// What the last File > Import left out (the Import Summary window lists it; empty = nothing to show).
    pub import_report: Vec<String>,
    /// Soft Selection > Edit Soft Selection: viewport drags set Falloff / Pinch / Bubble.
    pub soft_edit: bool,
    /// The Track View window (Curve Editor / Dope Sheet).
    pub track_view: crate::track_view::TrackViewState,
    /// Track bar: each node's tracks and key times, by (version, geom_version).
    pub key_time_cache: crate::anim_cmds::KeyTimeCache,
    /// Bone Tools > Bone Edit Mode.
    pub bone_edit_mode: bool,
    /// Display Bones: the viewports draw bones only (true) or bones and objects (false).
    pub display_bones_only: bool,
    /// Skin > Edit Envelopes: the selected vertices of the active Skin entry.
    pub skin_selection: BTreeSet<u32>,
    /// The Skin entry (node, stack index) `skin_selection` belongs to.
    pub skin_selection_owner: Option<(NodeId, usize)>,
    /// Skin > Bones list: the current bone (its envelope shows; weights edit it).
    pub skin_bone: Option<NodeId>,
    /// Skin Pose Mode: the live bone transforms, restored when the mode ends.
    pub skin_pose_live: Option<Vec<(NodeId, loom_scene::Transform)>>,
    /// Weight Tool custom value.
    pub skin_custom_weight: f32,
    /// Weight Tool > Scale Weight field.
    pub skin_scale_weight: f32,
    /// Weight Tool > Paste-Pos Tolerance.
    pub skin_paste_tolerance: f32,
    /// Weight Properties > Paint Weights: LMB drags brush weights onto the current bone.
    pub skin_paint: bool,
    /// Painter Options and Paint Blend Weights.
    pub skin_painter: crate::skin::Painter,
    /// Edit Envelopes > Select filters (Vertices, Envelopes, Cross Sections) and Backface Cull.
    pub skin_filter_vertices: bool,
    pub skin_filter_envelopes: bool,
    pub skin_filter_cross_sections: bool,
    pub skin_backface_cull: bool,
    /// The selected envelope handle: 0 start, 1 end.
    pub skin_handle: Option<u8>,
    pub skin_clipboard_envelope: Option<loom_scene::skin::Envelope>,
    /// Copied weights: (position in the skinned object's space, weights by bone node).
    pub skin_clipboard_weights: Vec<(Vec3, Vec<(NodeId, f32)>)>,
    /// Skin > Gizmos: the current gizmo index.
    pub skin_gizmo: Option<usize>,
    /// Morpher > Channel List: the current channel; Skin Morph: the current morph.
    pub morph_channel: usize,
    pub skin_morph_current: Option<usize>,
    /// Systems > Bones > IK Chain Assignment: new chains get an HI solver.
    pub bones_ik_chain: bool,
    pub current_frame: i32,
    pub frame_range: (i32, i32),
    pub fps: f32,
    /// Status bar coordinate display: Offset mode (vs Absolute).
    pub offset_mode: bool,
    pub status_prompt: String,
    /// Edit > Hold buffer (in memory until the .loom format lands in phase 4).
    /// Auto Backup clock (`autosave::tick`).
    pub autosave: crate::autosave::AutosaveState,
    /// New / Reset / Open / Exit waiting for the Save Changes prompt's save to finish.
    pub pending_file_action: Option<crate::viewport::PendingFileAction>,
    /// Set by File > Exit (after the prompt); the window closes.
    pub exit_requested: bool,
    /// The Edit UVWs window (Unwrap UVW).
    pub uv_editor: UvEditor,
    /// Unwrap options saved as defaults (Save Current Settings As Default).
    pub uv_defaults: UvEditor,
    /// Point To Point Seam: the last picked vertex.
    pub seam_point: Option<u32>,
    /// Merge dialog: the chosen file and its object names.
    pub merge_pending: Option<(std::path::PathBuf, Vec<String>)>,
    /// Customize > Show UI > Show Layers Toolbar.
    pub show_layers_toolbar: bool,
    /// File > File Properties and Summary Info > Description (saved in the scene file).
    pub file_properties: crate::file::FileProperties,
    /// The scene's file (File > Save writes here); None = Untitled.
    pub file: Option<std::path::PathBuf>,
    /// Set by commands that changed geometry; the renderer clears it.
    pub mesh_dirty: bool,
    /// A Render waiting for the UI's renderer (the command layer has no GPU).
    pub pending_render: Option<RenderRequest>,
    /// The Rendered Frame Window, its image and the render in progress.
    pub rendered: crate::render::RenderedFrame,
}

/// A render for the UI's renderer: viewport `viewport` with `settings`
/// (Render Setup at the time of the request), into the Rendered Frame Window.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderRequest {
    pub viewport: usize,
    pub settings: loom_scene::render::RenderSettings,
    /// The selection, for Area to Render > Selected.
    pub selection: Vec<NodeId>,
    /// Also write the image here when done.
    pub save_path: Option<String>,
    /// Iterative: Region and Selected keep the rest of the window's image.
    pub iterative: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            scene: Scene::new(),
            undo: UndoStack::new(100),
            selection: BTreeSet::new(),
            sub_selection: SubSelection::default(),
            selection_mode: SelectionMode::Object,
            active_tool: Tool::Select,
            viewports: default_kinds(ViewportLayout::FourGrid).into_iter().map(Viewport::new).collect(),
            active_viewport: 3,
            layout: ViewportLayout::FourGrid,
            splits: LayoutSplits::default(),
            maximized: false,
            nav_mode: NavMode::None,
            show_vertices: false,
            show_statistics: false,
            isolation: None,
            expert_mode: false,
            show_main_toolbar: true,
            show_ribbon: false,
            show_command_panel: true,
            startup_ui: crate::settings::UiLayout::DEFAULT,
            show_end_result: false,
            pin_stack: false,
            show_modifier_set: false,
            modifier_set: vec!["XForm".into(), "Normal".into(), "Smooth".into(), "Poly Select".into()],
            focus_modify: false,
            ui_locked: false,
            shade_selected_faces: false,
            shade_selected: false,
            viewcube: ViewCubeSettings::default(),
            config_panel: None,
            home_view: None,
            saved_views: BTreeMap::new(),
            selection_brackets: true,
            hide_floating_dialogs: false,
            hide_category: HideCategories::default(),
            dialog: None,
            material_editor_open: false,
            display: Default::default(),
            dialog_purpose: DialogPurpose::Select,
            pick_mode: None,
            align_target: None,
            normal_align_source: None,
            normal_align: None,
            spacing_points: [None, None],
            selection_filter: SelectionFilter::All,
            window_selection: false,
            selection_region: SelectionRegion::Rectangular,
            scale_kind: ScaleKind::Uniform,
            coord_system: CoordSystem::View,
            pick_coord_node: None,
            pick_coord_recent: Vec::new(),
            center_mode: CenterMode::PivotPoint,
            center_single: CenterMode::PivotPoint,
            center_multi: CenterMode::Selection,
            axis_constraint: AxisConstraint::XY,
            prefs: Prefs::default(),
            units: UnitsSetup::default(),
            default_units: UnitsSetup::default(),
            custom_keys: Default::default(),
            recent_files: Vec::new(),
            region_points_reset: false,
            manipulate: false,
            coord_by_tool: [CoordSystem::View; 4],
            center_by_tool: [(CenterMode::PivotPoint, CenterMode::Selection); 4],
            constraint_by_tool: [[AxisConstraint::XY; 4]; 2],
            constraint_level_sub: false,
            keyboard_override: true,
            ffd_selection: BTreeSet::new(),
            vertex_paint_brush: false,
            vertex_paint_pick: false,
            vertex_paint_faces: false,
            named_sets: BTreeMap::new(),
            select_by_vertex: false,
            ignore_backfacing: false,
            loop_mode: false,
            preview_selection: PreviewSelect::Off,
            named_sub_selections: BTreeMap::new(),
            soft_paint: None,
            soft_shaded_faces: false,
            poly_settings: PolySettings::default(),
            spline_settings: crate::spline::SplineSettings::default(),
            spline_create_line_target: None,
            compound_settings: CompoundSettings::default(),
            poly_drag: None,
            last_poly_op: None,
            pick_source: None,
            create_verts: Vec::new(),
            slice_plane: SlicePlane::default(),
            slice_plane_mode: false,
            slice_split: false,
            poly_constraint: PolyConstraint::None,
            full_interactivity: true,
            preserve_uvs: false,
            pick_point_a: None,
            paint_deform: None,
            soft_cache: Default::default(),
            affect_pivot_only: false,
            affect_object_only: false,
            affect_hierarchy_only: false,
            dont_affect_children: false,
            working_pivot: WorkingPivot::default(),
            edit_working_pivot: false,
            use_working_pivot: false,
            working_pivot_align_to_view: false,
            working_pivot_saved: (None, None),
            selection_locked: false,
            snaps_on: false,
            angle_snap_on: false,
            percent_snap_on: false,
            snaps: crate::snap::SnapSettings::default(),
            home_grid: crate::snap::HomeGrid::default(),
            grid_dirty: true,
            auto_key: false,
            set_key_mode: false,
            key_selection_list: None,
            modifier_usage: Default::default(),
            set_key_buffer: None,
            key_filters: Default::default(),
            default_tangent: (loom_scene::anim::TangentType::Auto, loom_scene::anim::TangentType::Auto),
            key_step_mode: false,
            playing: false,
            playback: Default::default(),
            track_bar_filter: Default::default(),
            show_track_bar: true,
            show_trajectories: false,
            trajectory_keys: false,
            trajectory_selected: BTreeSet::new(),
            tangent_clipboard: None,
            refine_chain: None,
            refine_pending: None,
            line_close_pending: None,
            import_report: Vec::new(),
            soft_edit: false,
            track_view: Default::default(),
            key_time_cache: Default::default(),
            bone_edit_mode: false,
            display_bones_only: false,
            skin_selection: BTreeSet::new(),
            skin_selection_owner: None,
            skin_bone: None,
            skin_pose_live: None,
            skin_custom_weight: 0.5,
            skin_scale_weight: 0.95,
            skin_paste_tolerance: 0.1,
            skin_paint: false,
            skin_painter: crate::skin::Painter::default(),
            skin_filter_vertices: true,
            skin_filter_envelopes: true,
            skin_filter_cross_sections: true,
            skin_backface_cull: false,
            skin_handle: None,
            skin_clipboard_envelope: None,
            skin_clipboard_weights: Vec::new(),
            skin_gizmo: None,
            morph_channel: 0,
            skin_morph_current: None,
            bones_ik_chain: false,
            current_frame: 0,
            frame_range: (0, 100),
            fps: 30.0,
            offset_mode: false,
            status_prompt: "Click or click-and-drag to select objects".to_owned(),
            autosave: crate::autosave::AutosaveState::default(),
            pending_file_action: None,
            exit_requested: false,
            uv_editor: UvEditor::default(),
            pending_render: None,
            rendered: Default::default(),
            uv_defaults: UvEditor::default(),
            seam_point: None,
            merge_pending: None,
            show_layers_toolbar: false,
            file_properties: Default::default(),
            file: None,
            mesh_dirty: false,
        }
    }

    pub fn active(&self) -> &Viewport {
        &self.viewports[self.active_viewport.min(self.viewports.len() - 1)]
    }

    pub fn active_mut(&mut self) -> &mut Viewport {
        let i = self.active_viewport.min(self.viewports.len() - 1);
        &mut self.viewports[i]
    }

    pub fn view_state(&self) -> ViewState {
        let v = self.active();
        ViewState {
            camera: v.camera.clone(),
            view_kind: v.kind,
            fov_y_radians: v.fov_y_radians,
        }
    }

    /// Records the active viewport's state for Shift+Z.
    pub fn record_view(&mut self) {
        let before = self.view_state();
        self.active_mut().view_undo.record(before);
    }

    /// A scene length in the display units (Units Setup), e.g. "2.54m".
    pub fn format_units(&self, v: f32) -> String {
        self.units.format(v, self.prefs.spinner_precision)
    }

    /// Applies the per-single/multiple centre memory after a selection change.
    pub fn refresh_center_mode(&mut self) {
        self.center_mode = if self.selection.len() > 1 { self.center_multi } else { self.center_single };
    }

    /// Viewport label text: the view name, or the camera's name for a camera view.
    pub fn view_label(&self, index: usize) -> String {
        let Some(vp) = self.viewports.get(index) else { return String::new() };
        match vp.kind {
            ViewKind::Camera(id) => self.scene.node(id).map(|n| n.name.clone()).unwrap_or_else(|| "Camera".to_owned()),
            k => k.label().to_owned(),
        }
    }

    /// Whether a node may be selected (visible, not frozen, passes the filter).
    pub fn is_selectable(&self, node: &Node) -> bool {
        let layer_frozen = self.scene.layer(node.layer).is_some_and(|l| l.frozen);
        self.is_visible(node) && !node.frozen && !layer_frozen && self.selection_filter.accepts(node)
    }

    /// What a click on `id` selects: the outermost closed group it belongs
    /// to, else the node itself.
    pub fn selection_target(&self, id: NodeId) -> NodeId {
        self.scene.closed_group_of(id).unwrap_or(id)
    }

    /// Whether a node shows in viewports (own flag and Hide by Category).
    pub fn is_visible(&self, node: &Node) -> bool {
        if node.hidden || self.scene.layer(node.layer).is_some_and(|l| l.hidden) {
            return false;
        }
        // Display > Hide Frozen Objects, Hide by Category > Bone Objects.
        if (self.display.hide_frozen && (node.frozen || self.scene.layer(node.layer).is_some_and(|l| l.frozen))) || (self.display.hide_bones && node.object.is_bone()) {
            return false;
        }
        // A hidden group head hides its members.
        let mut cur = node.parent;
        while let Some(p) = cur {
            let Some(pn) = self.scene.node(p) else { break };
            if matches!(pn.object, ObjectKind::Group { .. }) && pn.hidden {
                return false;
            }
            cur = pn.parent;
        }
        let c = &self.hide_category;
        !match node.object {
            ObjectKind::Camera { .. } => c.cameras,
            ObjectKind::Light(_) => c.lights,
            ref o if o.is_shape() => c.shapes,
            _ => c.geometry,
        }
    }

    /// World-space bounds of the given nodes (visible ones only).
    pub fn bounds_of(&self, ids: impl Iterator<Item = NodeId>) -> Option<(Vec3, Vec3)> {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut any = false;
        for id in ids {
            let Some(node) = self.scene.node(id) else { continue };
            if !self.is_visible(node) || self.scene.display_of(node).ignore_extents {
                continue;
            }
            let world = self.scene.object_matrix(id);
            for v in node.cage_mesh().vertices() {
                let p = world.transform_point3(v.position);
                min = min.min(p);
                max = max.max(p);
                any = true;
            }
        }
        any.then_some((min, max))
    }

    pub fn scene_bounds(&self) -> Option<(Vec3, Vec3)> {
        self.bounds_of(self.scene.nodes().map(|n| n.id))
    }

    /// World bounds of one node's mesh regardless of visibility or extents flags.
    pub fn object_bounds(&self, id: NodeId) -> Option<(Vec3, Vec3)> {
        let node = self.scene.node(id)?;
        let world = self.scene.object_matrix(id);
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut any = false;
        for v in node.cage_mesh().vertices() {
            let p = world.transform_point3(v.position);
            min = min.min(p);
            max = max.max(p);
            any = true;
        }
        any.then_some((min, max))
    }

    pub fn selection_bounds(&self) -> Option<(Vec3, Vec3)> {
        if !self.sub_selection.is_empty() {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            let mut any = false;
            for (node_id, ids) in self.sub_selection.by_node() {
                let Some(node) = self.scene.node(node_id) else { continue };
                let world = self.scene.object_matrix(node_id);
                let mesh = node.cage_mesh();
                for v in crate::execute::vertices_of(&mesh, ids) {
                    if let Some(p) = mesh.vertex_position(v) {
                        let p = world.transform_point3(p);
                        min = min.min(p);
                        max = max.max(p);
                        any = true;
                    }
                }
            }
            return any.then_some((min, max));
        }
        self.bounds_of(self.selection.iter().copied())
    }

    /// Soft selection weight per vertex of a poly node: 0 everywhere when
    /// Use Soft Selection is off; the locked / painted weights when any;
    /// else the falloff curve from the current sub-object selection.
    pub fn soft_weights(&self, id: NodeId) -> Vec<f32> {
        let Some(node) = self.scene.node(id) else { return Vec::new() };
        if let Some(so) = node.edit_spline() {
            // Spline soft selection: straight-line distance to the nearest selected vertex.
            let pts = so.shape.knot_positions();
            if !so.soft.on {
                return vec![0.0; pts.len()];
            }
            let selected: Vec<usize> = crate::execute::selected_vertices(self).into_iter().filter(|(nid, _)| *nid == id).flat_map(|(_, vs)| vs).map(|v| v.0 as usize).collect();
            let s = &so.soft;
            // Edge Distance: only vertices within that many steps along the same spline.
            let near = |i: usize, j: usize| -> bool {
                let Some(n) = s.edge_distance else { return true };
                let (Some((si, ki)), Some((sj, kj))) = (so.shape.knot_at(i), so.shape.knot_at(j)) else { return false };
                if si != sj {
                    return false;
                }
                let sp = &so.shape.splines[si];
                let d = ki.abs_diff(kj);
                let d = if sp.closed { d.min(sp.knots.len() - d) } else { d };
                d <= n as usize
            };
            return pts
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if selected.contains(&i) {
                        return 1.0;
                    }
                    let d = selected.iter().filter(|j| near(i, **j)).filter_map(|j| pts.get(*j)).map(|q| q.distance(*p)).fold(f32::INFINITY, f32::min);
                    if d.is_finite() { loom_mesh::soft_select::curve(d, s.falloff.get(), s.pinch.get(), s.bubble.get()) } else { 0.0 }
                })
                .collect();
        }
        let Some(poly) = node.edit_poly() else { return Vec::new() };
        let n = poly.mesh.vertex_count();
        if !poly.soft.on {
            return vec![0.0; n];
        }
        if let Some(p) = &poly.soft.painted {
            if p.len() == n {
                return p.clone();
            }
        }
        let selected: Vec<loom_mesh::VertexId> = crate::execute::selected_vertices(self).into_iter().filter(|(nid, _)| *nid == id).flat_map(|(_, vs)| vs).collect();
        if selected.is_empty() {
            return vec![0.0; n];
        }
        let view_dir = {
            let cam = &self.active().camera;
            let world = (cam.target - cam.eye_position()).normalize_or_zero();
            // Face normals are in object space: bring the view there too.
            glam::Mat3::from_mat4(self.scene.object_matrix(id)).transpose() * world
        };
        // Cached per node version + selection + view (the viewports ask every frame).
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            node.version.hash(&mut h);
            for v in &selected {
                v.0.hash(&mut h);
            }
            view_dir.x.to_bits().hash(&mut h);
            view_dir.y.to_bits().hash(&mut h);
            view_dir.z.to_bits().hash(&mut h);
            poly.soft.affect_backfacing.hash(&mut h);
            poly.soft.edge_distance.hash(&mut h);
            poly.soft.falloff.get().to_bits().hash(&mut h);
            poly.soft.pinch.get().to_bits().hash(&mut h);
            poly.soft.bubble.get().to_bits().hash(&mut h);
            h.finish()
        };
        if let Some((k, w)) = self.soft_cache.borrow().get(&id) {
            if *k == key {
                return w.clone();
            }
        }
        let w = loom_mesh::soft_select::weights(
            &poly.mesh,
            &selected,
            poly.soft.falloff.get(),
            poly.soft.pinch.get(),
            poly.soft.bubble.get(),
            poly.soft.edge_distance,
            poly.soft.affect_backfacing,
            Some(view_dir),
        );
        self.soft_cache.borrow_mut().insert(id, (key, w.clone()));
        w
    }

    /// The first selected node whose active stack entry is an FFD, with the
    /// entry index: the FFD shortcut group is live.
    /// The selected node's Unwrap UVW entry: the active one, else the topmost.
    pub fn unwrap_entry(&self) -> Option<(NodeId, usize)> {
        self.selection.iter().find_map(|id| {
            let n = self.scene.node(*id)?;
            if let Some(a) = n.active_entry {
                if matches!(n.modifiers.get(a)?.modifier, loom_scene::Modifier::UnwrapUVW(_)) {
                    return Some((*id, a));
                }
            }
            n.modifiers.iter().rposition(|s| matches!(s.modifier, loom_scene::Modifier::UnwrapUVW(_))).map(|i| (*id, i))
        })
    }

    /// The Unwrap UVW keyboard group is live while the Edit UVWs window is open.
    pub fn unwrap_active(&self) -> bool {
        self.uv_editor.open && self.unwrap_entry().is_some()
    }

    /// The selected node's Skin entry: the active one, else the topmost.
    pub fn skin_entry(&self) -> Option<(NodeId, usize)> {
        self.selection.iter().find_map(|id| {
            let n = self.scene.node(*id)?;
            if let Some(a) = n.active_entry {
                if matches!(n.modifiers.get(a)?.modifier, loom_scene::Modifier::Skin(_)) {
                    return Some((*id, a));
                }
            }
            n.modifiers.iter().rposition(|s| matches!(s.modifier, loom_scene::Modifier::Skin(_))).map(|i| (*id, i))
        })
    }

    /// Edit Envelopes is on: the Skin entry is active at its Envelope level.
    pub fn skin_active(&self) -> bool {
        self.skin_entry().is_some_and(|(id, a)| self.scene.node(id).is_some_and(|n| n.active_entry == Some(a) && n.modifiers[a].sub_level == 1))
    }

    /// The selected node's UVW Map entry: the active one, else the topmost.
    pub fn uvw_map_entry(&self) -> Option<(NodeId, usize)> {
        self.selection.iter().find_map(|id| {
            let n = self.scene.node(*id)?;
            if let Some(a) = n.active_entry {
                if matches!(n.modifiers.get(a)?.modifier, loom_scene::Modifier::UVWMap { .. }) {
                    return Some((*id, a));
                }
            }
            n.modifiers.iter().rposition(|s| matches!(s.modifier, loom_scene::Modifier::UVWMap { .. })).map(|i| (*id, i))
        })
    }

    pub fn ffd_active(&self) -> Option<(NodeId, usize)> {
        self.selection.iter().find_map(|id| {
            let n = self.scene.node(*id)?;
            let a = n.active_entry?;
            matches!(n.modifiers.get(a)?.modifier, loom_scene::Modifier::FFD { .. }).then_some((*id, a))
        })
    }

    /// An Editable Spline is selected (sub-object levels mean Vertex / Segment / Spline).
    pub fn spline_selected(&self) -> bool {
        self.selection.iter().any(|id| self.scene.node(*id).is_some_and(|n| n.edit_spline().is_some()))
    }

    /// Sub-object level name as the selected object calls it (Editable Spline:
    /// Vertex / Segment / Spline; Editable Poly: Vertex / Edge / Border / Polygon / Element).
    pub fn level_label(&self, mode: SelectionMode) -> &'static str {
        if self.spline_selected() {
            match mode {
                SelectionMode::Edge | SelectionMode::Border => "Segment",
                SelectionMode::Face | SelectionMode::Element => "Spline",
                other => other.label(),
            }
        } else {
            mode.label()
        }
    }

    /// A selected node's active stack entry is an Edit Poly: its shortcut group is live.
    pub fn edit_poly_active(&self) -> bool {
        self.selection.iter().any(|id| self.scene.node(*id).is_some_and(|n| n.editing_entry().is_some()))
    }

    /// An Editable Poly is selected, so the Editable Poly shortcut group is live.
    pub fn poly_selected(&self) -> bool {
        self.selection.iter().any(|id| self.scene.node(*id).is_some_and(|n| n.edit_poly().is_some()))
    }

    /// Keyboard resolution: with Keyboard Shortcut Override on, the
    /// Editable Poly group wins while an Editable Poly is selected; the
    /// Main UI group always applies.
    /// Unsaved changes since the last Save / Open / Reset.
    pub fn dirty(&self) -> bool {
        !self.undo.is_saved()
    }

    /// Window title: "name.loom * - 3dLoom".
    pub fn title(&self) -> String {
        let name = self.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        let star = if self.dirty() { " *" } else { "" };
        format!("{name}{star} - 3dLoom")
    }

    /// The shortcut groups live in this state, highest priority first; Main UI is last.
    /// With Keyboard Shortcut Override off only Main UI is live.
    pub fn active_keymaps(&self) -> Vec<&crate::keymap::Keymap> {
        let mut groups = Vec::new();
        if self.keyboard_override {
            if crate::normals::entry(self).is_some_and(|(id, i)| self.scene.node(id).is_some_and(|n| n.active_entry == Some(i) && n.modifiers[i].sub_level == 1)) {
                groups.push("Edit Normals");
            }
            // A focused Track View window takes its own group first.
            if self.track_view.open && self.track_view.focused {
                groups.push("Track View");
            }
            if self.unwrap_active() {
                groups.push("Unwrap UVW");
            }
            if self.skin_active() {
                groups.push("Skin");
            }
            if self.spline_selected() {
                groups.push("Editable Spline");
            }
            if self.ffd_active().is_some() {
                groups.push("FFD");
            }
            if self.edit_poly_active() {
                groups.push("Edit Poly");
            }
            if self.poly_selected() {
                groups.push("Editable Poly");
            }
        }
        groups.push("Main UI");
        groups.into_iter().map(|g| self.custom_keys.get(g)).collect()
    }

    pub fn hotkey(&self, chord: crate::keymap::Chord) -> Option<&'static str> {
        let action = self.active_keymaps().into_iter().find_map(|km| km.lookup(chord));
        if action == Some("Selection Lock Toggle") && !self.prefs.space_selection_lock {
            return None;
        }
        action
    }

    /// The chord that runs `action` here, as a label ("W", "Ctrl+Z"), from the live
    /// groups in priority order. `None` when no live group binds it.
    pub fn shortcut_label(&self, action: &str) -> Option<String> {
        self.active_keymaps().into_iter().find_map(|km| km.chord_for(action)).map(|c| c.label())
    }

    /// World-space pivot of the current selection (centre of bounds).
    pub fn selection_pivot(&self) -> Vec3 {
        if self.slice_plane_mode {
            return self.slice_plane.origin;
        }
        if self.edit_working_pivot {
            return self.working_pivot.origin;
        }
        self.selection_bounds()
            .map(|(a, b)| (a + b) * 0.5)
            .unwrap_or(Vec3::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    /// Screen-right vector of an orbit camera looking at the origin.
    fn screen_right(yaw: f32, pitch: f32) -> Vec3 {
        let cam = OrbitCamera { yaw, pitch, distance: 10.0, target: Vec3::ZERO, roll: 0.0 };
        let forward = (cam.target - cam.eye_position()).normalize();
        forward.cross(Vec3::Z).normalize()
    }

    #[test]
    fn top_and_front_views_put_x_on_screen_right() {
        for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Bottom] {
            let (yaw, pitch) = kind.angles().unwrap();
            let right = screen_right(yaw, pitch);
            assert!(right.abs_diff_eq(Vec3::X, 1e-3), "{kind:?}: right = {right}");
        }
    }
}

/// Edit UVWs > the editor's transform tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TvMode {
    #[default]
    Move,
    MoveHorizontal,
    MoveVertical,
    Rotate,
    Scale,
    ScaleHorizontal,
    ScaleVertical,
    Freeform,
    Pan,
}

/// Edit UVWs > sub-object level of the texture coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TvLevel {
    #[default]
    Vertex,
    Edge,
    Face,
}

/// What the editor draws behind the coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UvTexture {
    #[default]
    Checker,
    None,
    Bitmap(String),
}

/// Edit UVWs window state and its dialogs' settings (Flatten / Normal /
/// Unfold / Pack / Relax / Stitch / Pelt / Render UVW Template / Options).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UvEditor {
    pub open: bool,
    pub zoom: f32,
    pub pan: [f32; 2],
    pub mode: TvMode,
    pub level: TvLevel,
    pub show_map: bool,
    /// 0 UV, 1 VW, 2 WU.
    pub plane: u32,
    pub texture: UvTexture,
    pub soft_selection: bool,
    pub soft_falloff: f32,
    /// Soft selection falloff measured in XY (3D) space instead of UV.
    pub soft_xy: bool,
    pub soft_edge_distance: bool,
    pub soft_edge_range: u32,
    pub select_element: bool,
    pub sync_selection: bool,
    pub lock_selected: bool,
    pub filter_selected_polygons: bool,
    pub ignore_backfacing: bool,
    pub auto_pin_moved: bool,
    pub always_bring_up: bool,
    pub show_vertex_connections: bool,
    pub show_hidden_edges: bool,
    pub show_shared: bool,
    pub show_counter: bool,
    pub show_edge_distortion: bool,
    pub show_local_distortion: bool,
    pub grid_visible: bool,
    pub snap: bool,
    pub grid_snap: bool,
    pub vertex_snap: bool,
    pub edge_snap: bool,
    pub freeform_snap: bool,
    pub blend_tiles: bool,
    pub brightness_center_tile: bool,
    pub open_edge_mode: bool,
    pub uv_edge_mode: bool,
    pub geom_element_mode: bool,
    pub display_quick_map_gizmo: bool,
    /// Display Seams (in the editor).
    pub display_seams_in_editor: bool,
    /// Map Parameters > Edit Seams mode: viewport edge clicks toggle seams.
    pub edit_seams: bool,
    /// Bumped by Update Map so the editor reloads its bitmap.
    pub map_version: u32,
    /// Pivot offset of the transform gizmo from the selection centre (UV).
    pub pivot: [f32; 2],
    /// Freeform mode: a click inside the gizmo box selects.
    #[serde(default)]
    pub allow_select_inside_gizmo: bool,
    /// Paint Select Mode and its brush radius in pixels.
    #[serde(default)]
    pub paint_select: bool,
    #[serde(default = "default_paint_size")]
    pub paint_size: f32,
    /// Sketch: the tool is waiting for a stroke; the order vertices were picked in.
    #[serde(default)]
    pub sketch: bool,
    #[serde(default)]
    pub sketch_order: Vec<u32>,
    #[serde(default)]
    pub sketch_dialog: bool,
    /// Sketch Tool dialog: Select By 0 Pick, 1 Drag Pick, 2 Use Current Selection.
    #[serde(default)]
    pub sketch_select_by: u32,
    /// Align To: 0 Free Form, 1 Line, 2 Box, 3 Circle.
    #[serde(default)]
    pub sketch_align: u32,
    #[serde(default)]
    pub sketch_show_order: bool,
    #[serde(default)]
    pub sketch_interactive: bool,
    /// Spline Map: 0 Circular, 1 Planar (Normalize Map comes from the modifier).
    #[serde(default)]
    pub spline_map_mapping: u32,
    // Options dialog.
    pub weld_threshold: f32,
    pub constant_update: bool,
    pub grid_size: f32,
    pub checker_tiling: f32,
    pub tile_bitmap: bool,
    pub tiles: u32,
    pub tile_brightness: f32,
    pub bitmap_size: [u32; 2],
    // Flatten / Normal / Unfold / Pack / Relax / Stitch / Pelt / Planar.
    pub flatten_angle: f32,
    pub flatten_spacing: f32,
    pub flatten_normalize: bool,
    pub flatten_rotate: bool,
    pub flatten_fill_holes: bool,
    /// 0 Angle, 1 Smoothing Group, 2 Material ID.
    pub flatten_by: u32,
    /// 0 Back/Front, 1 Left/Right, 2 Top/Bottom, 3 Box, 4 Diamond.
    pub normal_kind: u32,
    pub normal_spacing: f32,
    pub normal_normalize: bool,
    pub normal_rotate: bool,
    pub normal_align_width: bool,
    /// 0 Walk to closest face, 1 Walk to farthest face.
    pub unfold_walk: u32,
    pub unfold_normalize: bool,
    /// 0 Linear, 1 Recursive.
    pub pack_method: u32,
    pub pack_spacing: f32,
    pub pack_normalize: bool,
    pub pack_rotate: bool,
    pub pack_fill_holes: bool,
    pub relax_iterations: u32,
    pub relax_amount: f32,
    pub relax_stretch: f32,
    /// 0 Face Angles, 1 Edge Angles, 2 Centers.
    pub relax_by: u32,
    pub relax_keep_boundary: bool,
    pub stitch_bias: f32,
    pub stitch_average: bool,
    pub pelt_iterations: u32,
    pub planar_threshold: f32,
    pub template_width: u32,
    pub template_height: u32,
    /// 0 Wire, 1 Solid.
    pub template_mode: u32,
    /// Copied tvs for Paste (uv per corner of the copied faces).
    pub clipboard: Vec<[f32; 3]>,
    /// Named groups of tvs (Group Selected).
    pub groups: Vec<Vec<u32>>,
}

impl Default for UvEditor {
    fn default() -> Self {
        Self {
            open: false,
            zoom: 1.0,
            pan: [0.0, 0.0],
            mode: TvMode::Move,
            level: TvLevel::Vertex,
            show_map: true,
            plane: 0,
            texture: UvTexture::Checker,
            soft_selection: false,
            soft_falloff: 0.1,
            soft_xy: false,
            soft_edge_distance: false,
            soft_edge_range: 1,
            select_element: false,
            sync_selection: true,
            lock_selected: false,
            filter_selected_polygons: false,
            ignore_backfacing: false,
            auto_pin_moved: false,
            always_bring_up: true,
            show_vertex_connections: false,
            show_hidden_edges: false,
            show_shared: false,
            show_counter: true,
            show_edge_distortion: false,
            show_local_distortion: false,
            grid_visible: true,
            snap: false,
            grid_snap: false,
            vertex_snap: false,
            edge_snap: false,
            freeform_snap: false,
            blend_tiles: false,
            brightness_center_tile: true,
            open_edge_mode: false,
            uv_edge_mode: false,
            geom_element_mode: false,
            display_quick_map_gizmo: false,
            display_seams_in_editor: true,
            edit_seams: false,
            map_version: 0,
            pivot: [0.0, 0.0],
            allow_select_inside_gizmo: false,
            paint_select: false,
            paint_size: 10.0,
            sketch: false,
            sketch_order: Vec::new(),
            sketch_dialog: false,
            sketch_select_by: 2,
            sketch_align: 1,
            sketch_show_order: false,
            sketch_interactive: false,
            spline_map_mapping: 0,
            weld_threshold: 0.01,
            constant_update: false,
            grid_size: 0.1,
            checker_tiling: 1.0,
            tile_bitmap: true,
            tiles: 1,
            tile_brightness: 0.5,
            bitmap_size: [256, 256],
            flatten_angle: 45.0,
            flatten_spacing: 0.02,
            flatten_normalize: true,
            flatten_rotate: true,
            flatten_fill_holes: true,
            flatten_by: 0,
            normal_kind: 0,
            normal_spacing: 0.02,
            normal_normalize: true,
            normal_rotate: true,
            normal_align_width: true,
            unfold_walk: 0,
            unfold_normalize: true,
            pack_method: 1,
            pack_spacing: 0.02,
            pack_normalize: true,
            pack_rotate: true,
            pack_fill_holes: true,
            relax_iterations: 100,
            relax_amount: 0.1,
            relax_stretch: 0.0,
            relax_by: 0,
            relax_keep_boundary: true,
            stitch_bias: 0.5,
            stitch_average: true,
            pelt_iterations: 50,
            planar_threshold: 15.0,
            template_width: 1024,
            template_height: 1024,
            template_mode: 0,
            clipboard: Vec::new(),
            groups: Vec::new(),
        }
    }
}

impl UvEditor {
    /// Unwrap Options and dialog values by name (`Set Unwrap Option`).
    pub fn set(&mut self, name: &str, v: f64) -> Result<(), String> {
        let b = v != 0.0;
        let f = v as f32;
        let u = v.max(0.0) as u32;
        match name {
            "Weld Threshold" => self.weld_threshold = f.max(0.0),
            "Spline Map Mapping" => self.spline_map_mapping = (v.max(0.0) as u32).min(1),
            "Paint Select Size" => self.paint_size = f.clamp(1.0, 200.0),
            "Sketch Select By" => self.sketch_select_by = (v.max(0.0) as u32).min(2),
            "Sketch Align" => self.sketch_align = (v.max(0.0) as u32).min(3),
            "Sketch Show Vertex Order" => self.sketch_show_order = v != 0.0,
            "Sketch Interactive Mode" => self.sketch_interactive = v != 0.0,
            "Constant Update" => self.constant_update = b,
            "Grid Size" => self.grid_size = f.max(0.001),
            "Checker Tiling" => self.checker_tiling = f.max(0.01),
            "Tile Bitmap" => self.tile_bitmap = b,
            "Tiles" => self.tiles = u.clamp(1, 10),
            "Tile Brightness" => self.tile_brightness = f.clamp(0.0, 1.0),
            "Bitmap Width" => self.bitmap_size[0] = u.clamp(16, 8192),
            "Bitmap Height" => self.bitmap_size[1] = u.clamp(16, 8192),
            "Soft Selection Falloff" => self.soft_falloff = f.max(0.0),
            "Flatten Angle" => self.flatten_angle = f.clamp(0.0, 180.0),
            "Flatten Spacing" => self.flatten_spacing = f.clamp(0.0, 1.0),
            "Flatten Normalize" => self.flatten_normalize = b,
            "Flatten Rotate Clusters" => self.flatten_rotate = b,
            "Flatten Fill Holes" => self.flatten_fill_holes = b,
            "Flatten By" => self.flatten_by = u.min(2),
            "Normal Mapping" => self.normal_kind = u.min(4),
            "Normal Spacing" => self.normal_spacing = f.clamp(0.0, 1.0),
            "Normal Normalize" => self.normal_normalize = b,
            "Normal Rotate Clusters" => self.normal_rotate = b,
            "Normal Align By Width" => self.normal_align_width = b,
            "Unfold Walk" => self.unfold_walk = u.min(1),
            "Unfold Normalize" => self.unfold_normalize = b,
            "Pack Method" => self.pack_method = u.min(1),
            "Pack Spacing" => self.pack_spacing = f.clamp(0.0, 1.0),
            "Pack Normalize" => self.pack_normalize = b,
            "Pack Rotate Clusters" => self.pack_rotate = b,
            "Pack Fill Holes" => self.pack_fill_holes = b,
            "Relax Iterations" => self.relax_iterations = u.clamp(1, 10000),
            "Relax Amount" => self.relax_amount = f.clamp(0.0, 1.0),
            "Relax Stretch" => self.relax_stretch = f.clamp(0.0, 1.0),
            "Relax By" => self.relax_by = u.min(2),
            "Relax Keep Boundary Points Fixed" => self.relax_keep_boundary = b,
            "Stitch Bias" => self.stitch_bias = f.clamp(0.0, 1.0),
            "Stitch Average" => self.stitch_average = b,
            "Pelt Iterations" => self.pelt_iterations = u.clamp(1, 10000),
            "Planar Threshold" => self.planar_threshold = f.clamp(0.0, 180.0),
            "Template Width" => self.template_width = u.clamp(16, 8192),
            "Template Height" => self.template_height = u.clamp(16, 8192),
            "Template Mode" => self.template_mode = u.min(1),
            "Zoom" => self.zoom = f.clamp(0.01, 1000.0),
            "Pan U" => self.pan[0] = f,
            "Pan V" => self.pan[1] = f,
            "Pivot U" => self.pivot[0] = f,
            "Pivot V" => self.pivot[1] = f,
            _ => return Err(format!("no Unwrap option {name:?}")),
        }
        Ok(())
    }
}

fn default_paint_size() -> f32 {
    10.0
}

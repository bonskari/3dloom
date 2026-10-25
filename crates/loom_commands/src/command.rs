//! The command registry. Every user action, hotkey, menu item and MCP tool
//! resolves to one `Command`. Names are the action names.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::OnceLock;

/// Answer to the Save Changes prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SaveChangesReply {
    Yes,
    No,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Group {
    /// The "Main UI" action group.
    MainUI,
    /// The "Editable Poly" action group (active with an Editable Poly selected).
    EditablePoly,
    /// The "FFD" action group (active with an FFD entry active in the stack).
    FFD,
    /// The "Unwrap UVW" action group (active while the Edit UVWs window is open).
    UnwrapUVW,
    /// The "Skin" action group (active with a Skin entry at its Envelope level).
    Skin,
    /// The "Track View" action group (live while a Track View window has focus).
    TrackView,
    /// The "Edit Normals" action group (live at an Edit Normals entry's Normal level).
    EditNormals,
    /// Commands that have no UI action (agent/MCP API, parametric calls).
    Loom,
}

#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub name: &'static str,
    pub group: Group,
    pub description: &'static str,
    /// JSON schema (`{"type":"object","properties":{..},"required":[..]}`).
    pub args_schema: Value,
}

/// Describes how an argument type appears in the JSON schema.
pub trait ArgSchema {
    fn schema() -> Value;
    fn optional() -> bool {
        false
    }
}

macro_rules! schema_for {
    ($($t:ty => $v:expr),* $(,)?) => { $(impl ArgSchema for $t { fn schema() -> Value { $v } })* };
}
schema_for! {
    f32 => json!({"type":"number"}),
    f64 => json!({"type":"number"}),
    u32 => json!({"type":"integer","minimum":0}),
    u64 => json!({"type":"integer","minimum":0}),
    i32 => json!({"type":"integer"}),
    bool => json!({"type":"boolean"}),
    String => json!({"type":"string"}),
    [f32; 3] => json!({"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}),
}
impl ArgSchema for loom_scene::render::RenderArea {
    fn schema() -> Value {
        json!({"type":"string","enum":["View","Selected","Region","Crop","Blowup"]})
    }
}
impl ArgSchema for loom_scene::anim::Ort {
    fn schema() -> Value {
        json!({"type":"string","enum":["Constant","Cycle","Loop","PingPong","Linear","RelativeRepeat"]})
    }
}
impl ArgSchema for crate::track_view::KeyArg {
    fn schema() -> Value {
        json!({"type":"object","properties":{"id":{"type":"integer","minimum":0},"track":{"type":"string"},"comp":{"type":"integer","minimum":0},"time":{"type":"number"}},"required":["id","track","time"]})
    }
}
impl ArgSchema for Value {
    fn schema() -> Value {
        json!({})
    }
}
impl ArgSchema for crate::file::MergeDuplicate {
    fn schema() -> Value {
        json!({"type":"object","properties":{"name":{"type":"string"},"action":{"type":"string","enum":["merge","skip","delete_old","rename","auto_rename"]},"new_name":{"type":"string"}},"required":["name","action"]})
    }
}
impl<T: ArgSchema> ArgSchema for Option<T> {
    fn schema() -> Value {
        T::schema()
    }
    fn optional() -> bool {
        true
    }
}
impl<T: ArgSchema> ArgSchema for Vec<T> {
    fn schema() -> Value {
        json!({"type":"array","items": T::schema()})
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrimitiveKind {
    Box,
    Cone,
    Sphere,
    GeoSphere,
    Cylinder,
    Tube,
    Torus,
    Pyramid,
    Teapot,
    Plane,
}
impl PrimitiveKind {
    /// Object Type rollout order of the Standard Primitives.
    pub const ALL: [PrimitiveKind; 10] = [
        PrimitiveKind::Box, PrimitiveKind::Cone, PrimitiveKind::Sphere, PrimitiveKind::GeoSphere, PrimitiveKind::Cylinder,
        PrimitiveKind::Tube, PrimitiveKind::Torus, PrimitiveKind::Pyramid, PrimitiveKind::Teapot, PrimitiveKind::Plane,
    ];
    pub fn label(self) -> &'static str {
        match self {
            PrimitiveKind::Box => "Box", PrimitiveKind::Cone => "Cone", PrimitiveKind::Sphere => "Sphere",
            PrimitiveKind::GeoSphere => "GeoSphere", PrimitiveKind::Cylinder => "Cylinder", PrimitiveKind::Tube => "Tube",
            PrimitiveKind::Torus => "Torus", PrimitiveKind::Pyramid => "Pyramid", PrimitiveKind::Teapot => "Teapot",
            PrimitiveKind::Plane => "Plane",
        }
    }
}
impl ArgSchema for PrimitiveKind {
    fn schema() -> Value {
        json!({"type":"string","enum":["box","cone","sphere","geosphere","cylinder","tube","torus","pyramid","teapot","plane"]})
    }
}
/// Create > Shapes > Splines object types (Line is the Line tool's own commands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    Rectangle,
    Circle,
    Ellipse,
    Arc,
    Donut,
    NGon,
    Star,
    Helix,
}
impl ShapeKind {
    /// Object Type rollout order (after Line).
    pub const ALL: [ShapeKind; 8] = [ShapeKind::Rectangle, ShapeKind::Circle, ShapeKind::Ellipse, ShapeKind::Arc, ShapeKind::Donut, ShapeKind::NGon, ShapeKind::Star, ShapeKind::Helix];
    pub fn label(self) -> &'static str {
        match self {
            ShapeKind::Rectangle => "Rectangle", ShapeKind::Circle => "Circle", ShapeKind::Ellipse => "Ellipse", ShapeKind::Arc => "Arc",
            ShapeKind::Donut => "Donut", ShapeKind::NGon => "NGon", ShapeKind::Star => "Star", ShapeKind::Helix => "Helix",
        }
    }
}
impl ArgSchema for ShapeKind {
    fn schema() -> Value {
        json!({"type":"string","enum":["rectangle","circle","ellipse","arc","donut","ngon","star","helix"]})
    }
}

impl ArgSchema for [u32; 2] {
    fn schema() -> Value {
        json!({"type":"array","items":{"type":"integer"},"minItems":2,"maxItems":2})
    }
}

impl ArgSchema for [f32; 2] {
    fn schema() -> Value {
        json!({"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2})
    }
}

impl ArgSchema for [u8; 3] {
    fn schema() -> Value {
        json!({"type":"array","items":{"type":"integer","minimum":0,"maximum":255},"minItems":3,"maxItems":3})
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectOp {
    Replace,
    Add,
    Subtract,
}
impl ArgSchema for SaveChangesReply {
    fn schema() -> Value {
        json!({ "type": "string", "enum": ["yes", "no", "cancel"] })
    }
}

impl ArgSchema for SelectOp {
    fn schema() -> Value {
        json!({"type":"string","enum":["replace","add","subtract"]})
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
    Z,
}
impl Axis {
    pub fn label(self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        }
    }
}
impl ArgSchema for crate::snap::SnapMode {
    fn schema() -> Value {
        json!({"type":"string","enum":["Snap2D","Snap25D","Snap3D"]})
    }
}
impl ArgSchema for loom_scene::anim::TangentType {
    fn schema() -> Value {
        json!({"type":"string","enum":["Auto","Smooth","Linear","Step","Fast","Slow","Custom"]})
    }
}
impl ArgSchema for crate::anim_cmds::TrackBarFilter {
    fn schema() -> Value {
        json!({"type":"string","enum":["AllKeys","AllTransformKeys","CurrentTransform","Object","Material"]})
    }
}
impl ArgSchema for crate::anim_cmds::PlayDirection {
    fn schema() -> Value {
        json!({"type":"string","enum":["Forward","Reverse","PingPong"]})
    }
}
impl ArgSchema for crate::state::AlignPoint {
    fn schema() -> Value {
        json!({"type":"string","enum":["Minimum","Center","PivotPoint","Maximum"]})
    }
}
impl ArgSchema for crate::state::PaintMode {
    fn schema() -> Value {
        json!({"type":"string","enum":["PushPull","Relax","Revert"]})
    }
}
impl ArgSchema for u16 {
    fn schema() -> Value {
        json!({"type":"integer","minimum":0,"maximum":65535})
    }
}
impl ArgSchema for crate::state::TessKind {
    fn schema() -> Value {
        json!({"type":"string","enum":["Edge","Face"]})
    }
}
impl ArgSchema for crate::state::ExtrudeKind {
    fn schema() -> Value {
        json!({"type":"string","enum":["Group","LocalNormal","ByPolygon"]})
    }
}
impl ArgSchema for crate::state::MirrorAxis {
    fn schema() -> Value {
        json!({"type":"string","enum":["X","Y","Z","XY","YZ","ZX"]})
    }
}
impl ArgSchema for crate::state::SpacingContext {
    fn schema() -> Value {
        json!({"type":"string","enum":["Centers","Edges"]})
    }
}
impl ArgSchema for crate::state::CloneKind {
    fn schema() -> Value {
        json!({"type":"string","enum":["Copy","Instance","Reference"]})
    }
}
impl ArgSchema for Axis {
    fn schema() -> Value {
        json!({"type":"string","enum":["x","y","z"]})
    }
}

impl ArgSchema for crate::viewport::ViewportLayout {
    fn schema() -> Value {
        json!({"type":"string","enum":["Single","TwoV","TwoH","ThreeLeftBig","ThreeTopBig","ThreeRightBig","ThreeBottomBig","ThreeCols","ThreeRows","FourGrid","FourLeftBig","FourRightBig","FourTopBig","FourBottomBig"]})
    }
}
impl ArgSchema for crate::viewport::Shading {
    fn schema() -> Value {
        json!({"type":"string","enum":["SmoothHighlights","Wireframe","HiddenLine","Flat","Realistic"]})
    }
}
impl ArgSchema for crate::state::ViewKind {
    fn schema() -> Value {
        json!({"type":"string","enum":["Top","Bottom","Front","Back","Left","Right","Perspective","Orthographic"]})
    }
}

impl ArgSchema for crate::state::SelectionMode {
    fn schema() -> Value {
        json!({"type":"string","enum":["object","vertex","edge","border","face","element"]})
    }
}
impl ArgSchema for crate::state::SelectionFilter {
    fn schema() -> Value {
        json!({"type":"string","enum":["All","Geometry","Shapes","Lights","Cameras","Helpers","Bone","IKChainObject","Point"]})
    }
}
impl ArgSchema for crate::state::SelectionRegion {
    fn schema() -> Value {
        json!({"type":"string","enum":["Rectangular","Circular","Fence","Lasso","Paint"]})
    }
}
impl ArgSchema for crate::state::ScaleKind {
    fn schema() -> Value {
        json!({"type":"string","enum":["Uniform","NonUniform","Squash"]})
    }
}
impl ArgSchema for crate::state::CoordSystem {
    fn schema() -> Value {
        json!({"type":"string","enum":["View","Screen","World","Parent","Local","Gimbal","Grid","Pick"]})
    }
}
impl ArgSchema for crate::state::CenterMode {
    fn schema() -> Value {
        json!({"type":"string","enum":["PivotPoint","Selection","TransformCoordinate"]})
    }
}
impl ArgSchema for [f32; 4] {
    fn schema() -> Value {
        json!({"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4})
    }
}
impl ArgSchema for [u32; 3] {
    fn schema() -> Value {
        json!({"type":"array","items":{"type":"integer","minimum":0},"minItems":3,"maxItems":3})
    }
}
impl ArgSchema for [bool; 3] {
    fn schema() -> Value {
        json!({"type":"array","items":{"type":"boolean"},"minItems":3,"maxItems":3})
    }
}

/// Declares the `Command` enum and its registry in one place so a variant
/// cannot exist without a name, group and description.
macro_rules! commands {
    ($( $variant:ident => $name:literal, $group:ident, $desc:literal $({ $($field:ident : $ty:ty),* $(,)? })? ; )*) => {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[serde(tag = "cmd", content = "args")]
        pub enum Command {
            $( #[serde(rename = $name)] $variant $({ $($field: $ty),* })? , )*
        }

        impl Command {
            pub fn name(&self) -> &'static str {
                match self { $( Command::$variant { .. } => $name, )* }
            }

            /// The command's action group (from the registry).
            pub fn group(&self) -> Group {
                let n = self.name();
                registry().iter().find(|i| i.name == n).map(|i| i.group).unwrap_or(Group::Loom)
            }
        }

        pub fn registry() -> &'static [CommandInfo] {
            static REG: OnceLock<Vec<CommandInfo>> = OnceLock::new();
            REG.get_or_init(|| vec![ $( CommandInfo {
                name: $name,
                group: Group::$group,
                description: $desc,
                args_schema: {
                    #[allow(unused_mut)]
                    let mut props = serde_json::Map::new();
                    #[allow(unused_mut)]
                    let mut required: Vec<Value> = Vec::new();
                    $( $(
                        props.insert(stringify!($field).to_owned(), <$ty as ArgSchema>::schema());
                        if !<$ty as ArgSchema>::optional() { required.push(json!(stringify!($field))); }
                    )* )?
                    json!({"type":"object","properties": props, "required": required})
                },
            }, )* ])
        }
    };
}

commands! {
    // --- File / Edit (Main UI) ---
    NewScene => "New Scene", MainUI, "Clear the scene (Ctrl+N).";
    UndoSceneOperation => "Undo Scene Operation", MainUI, "Undo the last scene change (Ctrl+Z).";
    RedoSceneOperation => "Redo Scene Operation", MainUI, "Redo the last undone scene change (Ctrl+Y).";
    UndoViewportOperation => "Undo Viewport Operation", MainUI, "Undo the last viewport change (Shift+Z).";
    RedoViewportOperation => "Redo Viewport Operation", MainUI, "Redo the last undone viewport change (Shift+Y).";
    OpenFile => "Open File", MainUI, "Open a .loom scene (Ctrl+O); without `path` the Open dialog shows. Replaces the scene and clears undo." { path: Option<String> };
    SaveFile => "Save File", MainUI, "Save the scene to its file (Ctrl+S), to `path`, or through the Save As dialog when it has none." { path: Option<String> };
    SaveFileAs => "Save File As", MainUI, "Save the scene to `path` and make that the current file; without `path` the Save As dialog shows." { path: Option<String> };
    SaveFileAsCopy => "Save File As Copy", MainUI, "Write a copy of the scene to `path` without changing the current file; without `path` the dialog shows." { path: Option<String> };
    SaveIncremental => "Save Incremental (+)", MainUI, "Save to the next free numbered name (scene01.loom, scene02.loom…) and make it current.";
    ResetFile => "Reset File", MainUI, "Reset: an empty untitled scene with default viewports, units and time configuration; undo is cleared.";
    Hold => "Hold", MainUI, "Save the scene to the hold file autoback/loomhold.loom (Ctrl+H).";
    Fetch => "Fetch", MainUI, "Restore the scene from the hold file (Alt+Ctrl+F); not undoable.";
    UvwMapModifier => "UVW Map Modifier", MainUI, "Add a UVW Map modifier to the selected objects.";
    UnwrapUvwModifier => "Unwrap UVW Modifier", MainUI, "Add an Unwrap UVW modifier to the selected objects.";
    // --- Unwrap UVW (chart group; active while the Edit UVWs window is open) ---
    EditUvws => "Edit UVW's", UnwrapUVW, "Unwrap UVW > Edit...: open the Edit UVWs window (Ctrl+E).";
    CloseUvws => "Close Edit UVWs", UnwrapUVW, "Close the Edit UVWs window.";
    UnwrapOptions => "Unwrap Options", UnwrapUVW, "Edit UVWs > Options (Ctrl+O): open the Unwrap Options dialog.";
    SetUnwrapOption => "Set Unwrap Option", UnwrapUVW, "Unwrap Options and dialog values by name: Weld Threshold, Constant Update, Grid Size, Checker Tiling, Flatten Angle / Spacing / Normalize / Rotate Clusters / Fill Holes / By, Normal Mapping / Spacing / Normalize / Rotate Clusters / Align By Width, Unfold Walk / Normalize, Pack Method / Spacing / Normalize / Rotate Clusters / Fill Holes, Relax Iterations / Amount / Stretch / By / Keep Boundary Points Fixed, Stitch Bias / Average, Pelt Iterations, Planar Threshold, Template Width / Height / Mode, Soft Selection Falloff, Zoom, Pan U / V, Pivot U / V." { name: String, value: f64 };
    LoadDefaults => "Load Defaults", UnwrapUVW, "Edit UVWs > Options > Load Defaults: the Unwrap options back to their defaults.";
    SaveCurrentSettingsAsDefault => "Save Current Settings As Default", UnwrapUVW, "Edit UVWs > Options > Save Current Settings As Default: the current Unwrap options become the defaults for new editors this session.";
    TvVertexSubobjectMode => "TV Vertex Sub-object Mode", UnwrapUVW, "Edit UVWs: Vertex level.";
    TvEdgeSubobjectMode => "TV Edge Sub-object Mode", UnwrapUVW, "Edit UVWs: Edge level.";
    TvPolygonSubobjectMode => "TV Polygon Sub-object Mode", UnwrapUVW, "Edit UVWs: Face level.";
    PolygonMode => "Polygon Mode", UnwrapUVW, "Edit UVWs: Face level (the Polygon button).";
    PolygonSelect => "Polygon Select", UnwrapUVW, "Edit UVWs: Face level.";
    TvElementMode => "TV Element Mode", UnwrapUVW, "Edit UVWs: Select Element toggle.";
    SelectTv => "Select TV", UnwrapUVW, "Edit UVWs: select texture vertices by index." { ids: Vec<u32>, op: SelectOp };
    SelectTvEdges => "Select TV Edges", UnwrapUVW, "Edit UVWs: select texture edges as [a, b] tv pairs." { edges: Vec<[u32; 2]>, op: SelectOp };
    SelectTvFaces => "Select TV Faces", UnwrapUVW, "Edit UVWs: select faces by id (their tvs)." { faces: Vec<u32>, op: SelectOp };
    SelectTvRegion => "Select TV Region", UnwrapUVW, "Edit UVWs: select the tvs inside a UV rectangle." { min: [f32; 2], max: [f32; 2], op: SelectOp };
    SelectAllTv => "Select All TV", UnwrapUVW, "Edit UVWs > Select All.";
    SelectNoneTv => "Select None TV", UnwrapUVW, "Edit UVWs > Select None.";
    SelectInvertTv => "Select Invert TV", UnwrapUVW, "Edit UVWs > Select Invert.";
    TextureVertexExpandSelection => "Texture Vertex Expand Selection", UnwrapUVW, "Edit UVWs > Expand Selection (NumPad +).";
    TextureVertexContractSelection => "Texture Vertex Contract Selection", UnwrapUVW, "Edit UVWs > Contract Selection (NumPad -).";
    EdgeToVertexSelect => "Edge to Vertex Select", UnwrapUVW, "Edit UVWs: the selected edges' tvs become the vertex selection.";
    EdgeToPolygonSelect => "Edge to Polygon Select", UnwrapUVW, "Edit UVWs: faces with a selected edge become the face selection.";
    PolygonToEdgeSelect => "Polygon to Edge  Select", UnwrapUVW, "Edit UVWs: the selected faces' edges become the edge selection.";
    PolygonToVertexSelect => "Polygon to Vertex Select", UnwrapUVW, "Edit UVWs: the selected faces' tvs become the vertex selection.";
    GetSelectionFromPolygons => "Get Selection From Polygons", UnwrapUVW, "The viewport's face selection becomes the editor's selection (Alt+Shift+Ctrl+P).";
    GetPolygonSelectionFromStack => "Get Polygon Selection From Stack", UnwrapUVW, "The face selection passed up the stack becomes the editor's selection (Alt+Shift+Ctrl+F).";
    SelectByMaterialIdUnwrap => "Select By Material ID (Unwrap)", UnwrapUVW, "Edit UVWs > Select > by Material ID (Selection Parameters > Select MatID).";
    SelectBySmoothingGroupUnwrap => "Select By Smoothing Group (Unwrap)", UnwrapUVW, "Edit UVWs > Select > by Smoothing Group (Selection Parameters > Select SG).";
    SelectInvertedPolygons => "Select Inverted Polygons", UnwrapUVW, "Edit UVWs > Select Inverted Faces.";
    SelectOverlappedPolygons => "Select Overlapped Polygons", UnwrapUVW, "Edit UVWs > Select Overlapped Faces.";
    SelectPinnedVerts => "Select Pinned Verts", UnwrapUVW, "Edit UVWs > Select Pinned.";
    TvLoopSelect => "TV Loop Select", UnwrapUVW, "Edit UVWs > Loop (edge loops from the selected edges).";
    TvRingSelect => "TV Ring Select", UnwrapUVW, "Edit UVWs > Ring (edge rings from the selected edges).";
    TvLoopGrowSelect => "TV Loop Grow Select", UnwrapUVW, "Edit UVWs: grow the edge selection along its loops by one edge.";
    TvLoopShrinkSelect => "TV Loop Shrink Select", UnwrapUVW, "Edit UVWs: shrink the edge selection along its loops by one edge.";
    TvRingGrowSelect => "TV Ring Grow Select", UnwrapUVW, "Edit UVWs: grow the edge selection across its rings by one edge.";
    TvRingShrinkSelect => "TV Ring Shrink Select", UnwrapUVW, "Edit UVWs: shrink the edge selection across its rings by one edge.";
    GeomEdgeLoopSelection => "Geom. Edge Loop Selection", UnwrapUVW, "Selection Parameters > Loop: the viewport edge selection grows to its loops.";
    GeomEdgeRingSelection => "Geom. Edge Ring Selection", UnwrapUVW, "Selection Parameters > Ring: the viewport edge selection grows to its rings.";
    GeomElement => "Geom Element", UnwrapUVW, "Selection Parameters: the viewport selection grows to whole elements.";
    GeomElementSelectMode => "Geom. Element Select Mode", UnwrapUVW, "Selection Parameters > Select By Element toggle.";
    ExpandGeomPolygons => "Expand Geom. Polygons", UnwrapUVW, "Selection Parameters > + : grow the viewport face selection.";
    ContractGeomPolygons => "Contract Geom. Polygons", UnwrapUVW, "Selection Parameters > - : shrink the viewport face selection.";
    FilterSelectedPolygons => "Filter Selected Polygons", UnwrapUVW, "Edit UVWs > Filter Selected Faces toggle (Alt+F).";
    IgnoreBackPolygons => "Ignore Back Polygons", UnwrapUVW, "Selection Parameters > Ignore Backfacing toggle.";
    OpenEdgeSelect => "Open Edge Select", UnwrapUVW, "Edit UVWs: select the open (border) texture edges.";
    OpenEdgeMode => "Open Edge Mode", UnwrapUVW, "Edit UVWs: open-edge selection mode toggle.";
    UvEdgeSelect => "UV Edge Select", UnwrapUVW, "Edit UVWs: select the edges that are seams in UV.";
    UvEdgeMode => "UV Edge Mode", UnwrapUVW, "Edit UVWs: UV-edge selection mode toggle.";
    SyncSelectionMode => "Sync Selection Mode", UnwrapUVW, "Edit UVWs > Sync Selection toggle.";
    SyncTextureSelectionToViewport => "Sync Texture Selection To Viewport", UnwrapUVW, "Edit UVWs: the editor's face selection becomes the viewport's.";
    SyncViewportSelectionToTexture => "Sync Viewport Selection To Texture", UnwrapUVW, "Edit UVWs: the viewport's face selection becomes the editor's.";
    SelectGroup => "Select Group", UnwrapUVW, "Edit UVWs: select the groups the selected tvs belong to.";
    GroupSelected => "Group Selected", UnwrapUVW, "Edit UVWs: the selected tvs become a group.";
    UngroupSelected => "Ungroup Selected", UnwrapUVW, "Edit UVWs: the selected tvs leave their groups.";
    FreezeSelected => "Freeze Selected", UnwrapUVW, "Edit UVWs > Freeze Selected (Ctrl+F).";
    UnfreezeAllUnwrap => "Unfreeze All (Unwrap)", UnwrapUVW, "Edit UVWs > Unfreeze All.";
    HideSelectedUnwrap => "Hide Selected", UnwrapUVW, "Edit UVWs > Hide Selected (Ctrl+H).";
    UnhideAllUnwrap => "Unhide All (Unwrap)", UnwrapUVW, "Edit UVWs > Unhide All.";
    LockSelectedVertices => "Lock selected vertices", UnwrapUVW, "Edit UVWs > Lock Selected Vertices toggle (Space).";
    PinSelected => "Pin Selected", UnwrapUVW, "Edit UVWs > Pin Selected (Relax and Pelt keep pinned tvs).";
    UnpinSelected => "Unpin Selected", UnwrapUVW, "Edit UVWs > Unpin Selected.";
    AutoPinMovedVertices => "Auto-Pin Moved Vertices", UnwrapUVW, "Edit UVWs: moved tvs are pinned, toggle.";
    AlwaysBringUpTheEditWindow => "Always Bring Up The Edit Window", UnwrapUVW, "Unwrap option toggle: adding the modifier opens the editor.";
    ShowSubobjectCounter => "Show Subobject Counter", UnwrapUVW, "Edit UVWs display toggle.";
    ShowSharedSubObjects => "Show Shared Sub-objects", UnwrapUVW, "Edit UVWs display toggle.";
    ShowVertexConnections => "Show Vertex Connections", UnwrapUVW, "Edit UVWs display toggle.";
    ShowHiddenEdges => "Show Hidden Edges", UnwrapUVW, "Edit UVWs display toggle.";
    DisplaySeams => "Display Seams", UnwrapUVW, "Edit UVWs display toggle: seams in the editor.";
    ShowSeamsInViewport => "Show Seams In Viewport", UnwrapUVW, "Unwrap UVW > Show Seams toggle (Alt+E).";
    PeltAlwaysShowSeams => "Pelt Always Show Seams", UnwrapUVW, "Unwrap UVW > Always Show Pelt Seam toggle.";
    ShowMap => "Show Map", UnwrapUVW, "Edit UVWs > Show Map toggle.";
    BlendTilesToBackground => "Blend Tiles To background", UnwrapUVW, "Edit UVWs option toggle.";
    BrightnessAffectsCenterTile => "Brightness Affects Center Tile", UnwrapUVW, "Edit UVWs option toggle.";
    GridVisible => "Grid Visible", UnwrapUVW, "Edit UVWs > Show Grid toggle.";
    Snap => "Snap", UnwrapUVW, "Edit UVWs > Snap toggle (Ctrl+S).";
    SnapToGridVertexEdge => "Snap To Grid/Vertex/Edge", UnwrapUVW, "Edit UVWs: cycle the snap target grid → vertex → edge.";
    GridSnap => "Grid Snap", UnwrapUVW, "Edit UVWs > Grid Snap toggle.";
    VertexSnap => "Vertex Snap", UnwrapUVW, "Edit UVWs > Vertex Snap toggle.";
    EdgeSnap => "Edge Snap", UnwrapUVW, "Edit UVWs > Edge Snap toggle.";
    FreeformSnap => "Freeform Snap", UnwrapUVW, "Edit UVWs > Freeform gizmo snap toggle: a dragged gizmo pivot snaps to texture vertices and the gizmo's corners and centre.";
    AllowSelectionsInsideTranformGizmo => "Allow Selections Inside Tranform Gizmo", UnwrapUVW, "Edit UVWs Freeform mode: a click inside the gizmo box selects instead of moving (toggle).";
    Sketch => "Sketch", UnwrapUVW, "Tools > Sketch Vertices: the next drawing in the Edit UVWs window lines up the selected vertices along it (Sketch Dialog settings) (toggle).";
    SketchDialog => "Sketch Dialog", UnwrapUVW, "Open the Sketch Tool dialog (Select By, Align To, Show Vertex Order, Interactive Mode, Drag Cursor Size).";
    SketchReverseVertexOrder => "Sketch Reverse Vertex Order", UnwrapUVW, "Sketch: reverse the order the picked vertices are placed in.";
    SketchTv => "Sketch TV", Loom, "Sketch: place texture vertices `tvs` (null = the picked order of the selection) evenly along a drawn shape in UV; align 0 Free Form (points = the stroke), 1 Line, 2 Box (two corners), 3 Circle (centre, rim point)." { tvs: Option<Vec<u32>>, align: u32, points: Vec<[f32; 2]> };
    PaintSelectMode => "Paint Select Mode", UnwrapUVW, "Edit UVWs: paint to select texture vertices under the brush (toggle).";
    PaintSelectIncrementCursorSize => "Paint Select Increment Cursor Size", UnwrapUVW, "Paint Select: a larger brush.";
    PaintSelectDecrementCursorSize => "Paint Select Decrement Cursor Size", UnwrapUVW, "Paint Select: a smaller brush.";
    PaintSelectTv => "Paint Select TV", Loom, "Paint Select stroke: the texture vertices within `radius` (UV units) of the stroke points." { points: Vec<[f32; 2]>, radius: f32, op: SelectOp };
    SplineMap => "Spline Map", UnwrapUVW, "Map Parameters > Spline: pick a spline; the selected faces are mapped along it (Edit UVWs Spline Map settings).";
    UnwrapSplineMap => "Unwrap Spline Map", Loom, "Spline mapping of the selected faces along shape `spline`: V runs along it (normalized 0-1 or in scene units), U around it (mapping 0 Circular) or across it (1 Planar)." { spline: u64, mapping: u32, normalize: bool };
    ShowEdgeDistortion => "Show Edge Distortion", UnwrapUVW, "Edit UVWs display: edges coloured by stretch.";
    ShowLocalDistortion => "Show Local Distortion", UnwrapUVW, "Edit UVWs display: faces coloured by stretch.";
    DisplayQuickMapGizmo => "Display Quick Map Gizmo", UnwrapUVW, "Unwrap UVW: show the quick planar map gizmo toggle.";
    UpdateMap => "Update Map", UnwrapUVW, "Edit UVWs: reload the map bitmap (Ctrl+U).";
    PickTexture => "Pick Texture", UnwrapUVW, "Edit UVWs map list > Pick Texture: a bitmap file behind the coordinates." { path: String };
    RemoveTexture => "Remove Texture", UnwrapUVW, "Edit UVWs map list > Remove Texture (back to the checker).";
    CheckerPattern => "CheckerPattern", UnwrapUVW, "Edit UVWs map list > CheckerPattern.";
    UvEditorPlane => "UV Editor Plane", UnwrapUVW, "Edit UVWs: the coordinate pair shown, 0 UV, 1 VW, 2 WU." { plane: u32 };
    UnwrapMapChannel => "Unwrap Map Channel", UnwrapUVW, "Unwrap UVW > Channel: Map Channel 1–99, or 0 for the Vertex Color Channel." { channel: u32 };
    TextureVertexMoveMode => "Texture Vertex Move Mode", UnwrapUVW, "Edit UVWs > Move (W).";
    TextureVertexRotateMode => "Texture Vertex Rotate Mode", UnwrapUVW, "Edit UVWs > Rotate (E).";
    TextureVertexScaleMode => "Texture Vertex Scale Mode", UnwrapUVW, "Edit UVWs > Scale (R).";
    MoveHorizontal => "Move Horizontal", UnwrapUVW, "Edit UVWs > Move Horizontal mode (Alt+Shift+Ctrl+J).";
    MoveVertical => "Move Vertical", UnwrapUVW, "Edit UVWs > Move Vertical mode (Alt+Shift+Ctrl+K).";
    ScaleHorizontal => "Scale Horizontal", UnwrapUVW, "Edit UVWs > Scale Horizontal mode.";
    ScaleVertical => "Scale Vertical", UnwrapUVW, "Edit UVWs > Scale Vertical mode.";
    Rotate => "Rotate", UnwrapUVW, "Edit UVWs > Rotate mode.";
    FreeformMode => "Freeform Mode", UnwrapUVW, "Edit UVWs > Freeform mode.";
    Pan => "Pan", UnwrapUVW, "Edit UVWs > Pan mode (Ctrl+P).";
    MoveTv => "Move TV", UnwrapUVW, "Edit UVWs: move the selected tvs by a UV delta (soft selection applies)." { delta: [f32; 2] };
    RotateTv => "Rotate TV", UnwrapUVW, "Edit UVWs: rotate the selected tvs about the selection pivot." { degrees: f32 };
    ScaleTv => "Scale TV", UnwrapUVW, "Edit UVWs: scale the selected tvs about the selection pivot." { factor: [f32; 2] };
    SetTvPosition => "Set TV Position", UnwrapUVW, "Edit UVWs: type U / V / W for the selection (a null leaves that coordinate)." { u: Option<f32>, v: Option<f32>, w: Option<f32> };
    RotatePlus90 => "Rotate +90", UnwrapUVW, "Edit UVWs > Rotate +90°.";
    RotateMinus90 => "Rotate -90", UnwrapUVW, "Edit UVWs > Rotate -90°.";
    MirrorHorizontal => "Mirror Horizontal", UnwrapUVW, "Edit UVWs > Mirror Horizontal (Alt+Shift+Ctrl+N).";
    MirrorVertical => "Mirror Vertical", UnwrapUVW, "Edit UVWs > Mirror Vertical (Alt+Shift+Ctrl+M).";
    FlipHorizontal => "Flip Horizontal", UnwrapUVW, "Edit UVWs > Flip Horizontal (the selected faces detach, then mirror).";
    FlipVertical => "Flip Vertical", UnwrapUVW, "Edit UVWs > Flip Vertical.";
    BreakSelectedVertices => "Break Selected Vertices", UnwrapUVW, "Edit UVWs > Break (Ctrl+B).";
    BreakBySubobject => "Break By Subobject", UnwrapUVW, "Edit UVWs > Break at the current level (edges split the faces beside them, faces detach).";
    DetachEdgeVerts => "Detach Edge Verts", UnwrapUVW, "Edit UVWs > Detach Edge Verts (D).";
    TextureVertexWeldSelected => "Texture Vertex Weld Selected", UnwrapUVW, "Edit UVWs > Weld Selected (Ctrl+W), within Weld Threshold.";
    TextureVertexTargetWeld => "Texture VertexTarget Weld", UnwrapUVW, "Edit UVWs > Target Weld mode (Ctrl+T): click a tv, then its target.";
    TargetWeldTv => "Target Weld TV", UnwrapUVW, "Edit UVWs: weld tv `from` into tv `to`." { from: u32, to: u32 };
    Stitch => "Stitch", UnwrapUVW, "Edit UVWs > Stitch Selected with the Stitch dialog's bias / average.";
    StitchAverage => "Stitch Average", UnwrapUVW, "Edit UVWs > Stitch: both sides meet half way.";
    StitchToSource => "Stitch To Source", UnwrapUVW, "Edit UVWs > Stitch: the other side moves onto the selection.";
    StitchToTarget => "Stitch To Target", UnwrapUVW, "Edit UVWs > Stitch: the selection moves onto the other side.";
    StitchDialog => "Stitch Dialog", UnwrapUVW, "Edit UVWs > Stitch Tool dialog.";
    AlignHorizontallyToPivot => "Align Horizontally to Pivot", UnwrapUVW, "Edit UVWs: the selected tvs onto the pivot's v.";
    AlignVerticallyToPivot => "Align Vertically to Pivot", UnwrapUVW, "Edit UVWs: the selected tvs onto the pivot's u.";
    AlignSelectionHorizontal => "Align Selection Horizontal", UnwrapUVW, "Edit UVWs > Align Horizontal (average v).";
    AlignSelectionVertical => "Align Selection Vertical", UnwrapUVW, "Edit UVWs > Align Vertical (average u).";
    AlignToEdge => "Align to Edge", UnwrapUVW, "Edit UVWs > Align to Edge: the selection rotates so its first selected edge is horizontal.";
    LinearAlign => "Linear Align", UnwrapUVW, "Edit UVWs > Linear Align (best-fit line).";
    SpaceHorizontally => "Space Horizontally", UnwrapUVW, "Edit UVWs > Space Horizontally.";
    SpaceVertically => "Space Vertically", UnwrapUVW, "Edit UVWs > Space Vertically.";
    StraightenEdges => "Straighten Edges", UnwrapUVW, "Edit UVWs > Straighten Edges.";
    RelaxUnwrap => "Relax (Unwrap)", UnwrapUVW, "Edit UVWs > Relax with the Relax dialog's settings.";
    RelaxDialog => "Relax Dialog", UnwrapUVW, "Edit UVWs > Relax Tool dialog.";
    RelaxOneClick => "Relax One Click", UnwrapUVW, "Edit UVWs > Relax: one iteration pass.";
    Pack => "Pack", UnwrapUVW, "Edit UVWs > Pack UVs with the Pack dialog's settings.";
    PackDialog => "Pack Dialog", UnwrapUVW, "Edit UVWs > Pack UVs dialog.";
    PackButtons => "Pack Buttons", UnwrapUVW, "Edit UVWs > Pack (the toolbar button).";
    PackNormalize => "Pack Normalize", UnwrapUVW, "Edit UVWs > Pack with Normalize.";
    PackRegion => "Pack Region", UnwrapUVW, "Edit UVWs > Pack into the selection's region.";
    PackTogether => "Pack Together", UnwrapUVW, "Edit UVWs > Pack the selected clusters together (no normalize).";
    Rescale => "Rescale", UnwrapUVW, "Edit UVWs > Rescale Elements: each selected cluster scaled to its 3D size.";
    RescaleClusters => "Rescale Clusters", UnwrapUVW, "Edit UVWs > Rescale Clusters (same as Rescale on every cluster).";
    FlattenMap => "Flatten Map", UnwrapUVW, "Edit UVWs > Mapping > Flatten Mapping with the dialog's settings.";
    FlattenMapDialog => "Flatten Map Dialog", UnwrapUVW, "Edit UVWs > Mapping > Flatten Mapping... dialog.";
    FlattenByMaterialId => "Flatten By Material ID", UnwrapUVW, "Flatten Mapping grouped by material id.";
    FlattenByMaterialIds => "Flatten By Material IDs", UnwrapUVW, "Flatten Mapping grouped by material id.";
    FlattenBySmoothingGroup => "Flatten By Smoothing Group", UnwrapUVW, "Flatten Mapping grouped by smoothing group.";
    NormalMap => "Normal Map", UnwrapUVW, "Edit UVWs > Mapping > Normal Mapping with the dialog's settings.";
    NormalMapDialog => "Normal Map Dialog", UnwrapUVW, "Edit UVWs > Mapping > Normal Mapping... dialog.";
    UnfoldMap => "Unfold Map", UnwrapUVW, "Edit UVWs > Mapping > Unfold Mapping with the dialog's settings.";
    UnfoldMapDialog => "Unfold Map Dialog", UnwrapUVW, "Edit UVWs > Mapping > Unfold Mapping... dialog.";
    UnfoldEdge => "Unfold Edge", UnwrapUVW, "Edit UVWs: unfold the faces beside the selected edges.";
    PeltMap => "Pelt Map", UnwrapUVW, "Map Parameters > Pelt: pelt-map the selected faces at the seams.";
    PeltDialog => "Pelt Dialog", UnwrapUVW, "Map Parameters > Pelt > Edit Pelt Map dialog.";
    PeltDialogRunSimulation => "Pelt Dialog Run Simulation", UnwrapUVW, "Pelt dialog > Simulate Pelt Pulling: relax the pelt.";
    PeltDialogResetStretcher => "Pelt Dialog Reset Stretcher", UnwrapUVW, "Pelt dialog > Reset: the pelt back to its boundary circle.";
    PeltDialogRelaxSimulationLight => "Pelt Dialog Relax Simulation Light", UnwrapUVW, "Pelt dialog > Relax (Light): a few relax iterations.";
    PeltDialogRelaxSimulationHeavy => "Pelt Dialog Relax Simulation Heavy", UnwrapUVW, "Pelt dialog > Relax (Heavy): many relax iterations.";
    PeltDialogSelectPeltUvs => "Pelt Dialog Select Pelt UV's", UnwrapUVW, "Pelt dialog > Select Pelt UVs: the pelted faces' tvs.";
    PeltEditSeams => "Pelt Edit Seams", UnwrapUVW, "Map Parameters > Edit Seams toggle: viewport edges clicked become seams.";
    PeltExpandSelectionToSeams => "Pelt Expand Selection To Seams", UnwrapUVW, "Map Parameters > Exp. Face Sel To Seams: the face selection grows up to the seams.";
    PeltSeamToEdgeSelAdd => "Pelt Seam to Edge Sel(Add)", UnwrapUVW, "The seams join the viewport edge selection.";
    PeltSeamToEdgeSelReplace => "Pelt Seam to Edge Sel(Replace)", UnwrapUVW, "The seams become the viewport edge selection.";
    EdgeSelToPeltSeamAdd => "Edge Sel to Pelt Seam(Add)", UnwrapUVW, "Map Parameters > Edge Sel To Seams: the viewport edge selection joins the seams.";
    EdgeSelToPeltSeamReplace => "Edge Sel to Pelt Seam(Replace)", UnwrapUVW, "The viewport edge selection becomes the seams.";
    PointToPointEdgeSelection => "Point To Point Edge Selection", UnwrapUVW, "Map Parameters > Point To Point Seam: click vertices; the shortest edge path between each pair becomes a seam.";
    SeamPoint => "Seam Point", UnwrapUVW, "Point To Point Seam pick: a vertex of `node`." { node: u64, vertex: u32 };
    ClearSeams => "Clear Seams", UnwrapUVW, "Unwrap UVW: remove every seam.";
    ToggleSeamEdge => "Toggle Seam Edge", UnwrapUVW, "Edit Seams click: the geometry edge (half-edge id) of `node` becomes a seam, or stops being one." { node: u64, edge: u32 };
    PlanarMapFaces => "Planar map faces/patches", UnwrapUVW, "Map Parameters > Planar (Enter): planar-map the selected faces with the map gizmo.";
    PlanarThreshold => "Planar Threshold", UnwrapUVW, "Selection Parameters > Planar Angle: grow the face selection to faces within the threshold.";
    QuickPlanarMap => "Quick Planar map", UnwrapUVW, "Map Parameters > Quick Planar Map: the selected faces planar-mapped along their average normal into the unit square.";
    BoxMap => "Box Map", UnwrapUVW, "Map Parameters > Box: box-map the selected faces with the map gizmo.";
    CylindricalMap => "Cylindrical Map", UnwrapUVW, "Map Parameters > Cylindrical: cylinder-map the selected faces with the map gizmo.";
    SphericalMap => "Spherical Map", UnwrapUVW, "Map Parameters > Spherical: sphere-map the selected faces with the map gizmo.";
    MappingAlignX => "Mapping Align X", UnwrapUVW, "Map Parameters > Align X: the map gizmo faces the object's X axis.";
    MappingAlignY => "Mapping Align Y", UnwrapUVW, "Map Parameters > Align Y.";
    MappingAlignZ => "Mapping Align Z", UnwrapUVW, "Map Parameters > Align Z.";
    MappingAlignNormal => "Mapping Align Normal", UnwrapUVW, "Map Parameters > Best Align: the gizmo faces the selected faces' average normal.";
    MappingAlignToView => "Mapping Align To View", UnwrapUVW, "Map Parameters > Align To View.";
    MappingCenter => "Mapping Center", UnwrapUVW, "Map Parameters > Center: the gizmo at the selection's centre.";
    MappingFit => "Mapping Fit", UnwrapUVW, "Map Parameters > Fit: the gizmo spans the selected faces.";
    MappingReset => "Mapping Reset", UnwrapUVW, "Map Parameters > Reset: Z alignment, then Fit.";
    PreventReflattening => "Prevent Reflattening", UnwrapUVW, "Unwrap UVW > Prevent Reflattening toggle.";
    OpenRenderTemplateDialog => "Render UVs Dialog", UnwrapUVW, "Edit UVWs > Tools > Render UVW Template...: the Render UVs dialog.";
    RenderUvwTemplate => "Render UVW Template", UnwrapUVW, "Edit UVWs > Tools > Render UVW Template: write a PNG of the texture edges (`mode` wire or solid), at most 8192 px." { path: String, width: u32, height: u32, mode: String };
    SaveUvw => "Save UVW", UnwrapUVW, "Unwrap UVW > Save: write the channel to a .uvw file (Alt+Shift+Ctrl+L loads); without `path` the dialog shows." { path: Option<String> };
    LoadUvw => "Load UVW", UnwrapUVW, "Unwrap UVW > Load a .uvw file onto the same topology; without `path` the dialog shows." { path: Option<String> };
    ResetUvws => "Reset UVWs", UnwrapUVW, "Unwrap UVW > Reset UVWs: the input's mapping comes back.";
    CopyTv => "Copy", UnwrapUVW, "Edit UVWs > Copy the selected faces' coordinates.";
    PasteTv => "Paste", UnwrapUVW, "Edit UVWs > Paste onto the selected faces (same corner count).";
    PasteInstance => "Paste Instance", UnwrapUVW, "Edit UVWs > Paste Instance: paste and weld onto the copied tvs.";
    PasteSymmetrical => "Paste Symmetrical", UnwrapUVW, "Edit UVWs > Paste Symmetrical: paste mirrored horizontally.";
    SoftSelection => "Soft Selection", UnwrapUVW, "Edit UVWs > Soft Selection toggle.";
    SoftSelectionEdgeDistance => "Soft Selection Edge Distance", UnwrapUVW, "Edit UVWs > Soft Selection > Edge Distance toggle.";
    SoftSelectionEdgeDistanceRange1 => "Soft Selection Edge Distance Range 1", UnwrapUVW, "Soft Selection edge distance 1.";
    SoftSelectionEdgeDistanceRange2 => "Soft Selection Edge Distance Range 2", UnwrapUVW, "Soft Selection edge distance 2.";
    SoftSelectionEdgeDistanceRange3 => "Soft Selection Edge Distance Range 3", UnwrapUVW, "Soft Selection edge distance 3.";
    SoftSelectionEdgeDistanceRange4 => "Soft Selection Edge Distance Range 4", UnwrapUVW, "Soft Selection edge distance 4.";
    SoftSelectionEdgeDistanceRange5 => "Soft Selection Edge Distance Range 5", UnwrapUVW, "Soft Selection edge distance 5.";
    SoftSelectionEdgeDistanceRange6 => "Soft Selection Edge Distance Range 6", UnwrapUVW, "Soft Selection edge distance 6.";
    SoftSelectionEdgeDistanceRange7 => "Soft Selection Edge Distance Range 7", UnwrapUVW, "Soft Selection edge distance 7.";
    SoftSelectionEdgeDistanceRange8 => "Soft Selection Edge Distance Range 8", UnwrapUVW, "Soft Selection edge distance 8.";
    PivotSnapToCenter => "Pivot Snap To Center", UnwrapUVW, "Edit UVWs: the transform pivot at the selection centre.";
    PivotSnapUpperLeft => "Pivot Snap Upperr Left", UnwrapUVW, "Edit UVWs: the pivot at the selection's upper left.";
    PivotSnapUpperCenter => "Pivot Snap Upper Center", UnwrapUVW, "Edit UVWs: the pivot at the selection's upper centre.";
    PivotSnapUpperRight => "Pivot Snap Upper Right", UnwrapUVW, "Edit UVWs: the pivot at the selection's upper right.";
    PivotSnapLeftCenter => "Pivot Snap Left Center", UnwrapUVW, "Edit UVWs: the pivot at the selection's left centre.";
    PivotSnapRightCenter => "Pivot Snap Right Center", UnwrapUVW, "Edit UVWs: the pivot at the selection's right centre.";
    PivotSnapLowerLeft => "Pivot Snap Lower Left", UnwrapUVW, "Edit UVWs: the pivot at the selection's lower left.";
    PivotSnapLowerCenter => "Pivot Snap Lower Center", UnwrapUVW, "Edit UVWs: the pivot at the selection's lower centre.";
    PivotSnapLowerRight => "Pivot Snap Lower Right", UnwrapUVW, "Edit UVWs: the pivot at the selection's lower right.";
    ResetPivotOnSelection => "Reset Pivot On Selection", UnwrapUVW, "Edit UVWs: the pivot back to the selection centre on every new selection.";
    ZoomExtentsUv => "Zoom Extents UV", UnwrapUVW, "Edit UVWs > Zoom Extents: fit every tv in the editor.";
    ZoomExtentsSelectedUv => "Zoom Extents Selected UV", UnwrapUVW, "Edit UVWs > Zoom Extents Selected.";
    UvwMapFit => "UVW Map Fit", Loom, "UVW Map > Fit: the gizmo spans the object's bounds along its alignment.";
    UvwMapCenter => "UVW Map Center", Loom, "UVW Map > Center: the gizmo moves to the object's centre.";
    UvwMapBitmapFit => "UVW Map Bitmap Fit", Loom, "UVW Map > Bitmap Fit: the gizmo takes the aspect ratio of an image file." { path: String };
    UvwMapNormalAlign => "UVW Map Normal Align", Loom, "UVW Map > Normal Align: the next clicked face's normal becomes the gizmo's Z.";
    UvwMapViewAlign => "UVW Map View Align", Loom, "UVW Map > View Align: the gizmo faces the active viewport.";
    UvwMapRegionFit => "UVW Map Region Fit", Loom, "UVW Map > Region Fit: two world points (a drag in the viewport) size the gizmo's width and length." { a: [f32; 3], b: [f32; 3] };
    UvwMapReset => "UVW Map Reset", Loom, "UVW Map > Reset: default alignment, then Fit.";
    UvwMapAcquire => "UVW Map Acquire", Loom, "UVW Map > Acquire: copy another object's UVW Map gizmo and parameters (absolute: same world placement)." { node: u64 };
    UvwMapAlignTo => "UVW Map Align To", Loom, "Normal Align's pick: the gizmo's Z along `normal` at `point` (world), then Fit." { point: [f32; 3], normal: [f32; 3] };
    SetTextureDisplay => "Set Texture Display", Loom, "Display > Show Map in Viewport: `kind` none / checker / bitmap (with `path`)." { id: u64, kind: String, path: Option<String> };
    ImportFile => "Import File", MainUI, "Import an FBX, a Wavefront OBJ (each `o` / `g` block an Editable Poly) or a .max scene file (nodes, hierarchy, transforms, pivots, Editable Poly / Mesh with UVs and material ids, Box / Cylinder / Plane; modifier stacks and render objects are reported, not imported), one undo entry; without `path` the Import dialog shows." { path: Option<String> };
    ExportFile => "Export File", MainUI, "Export the visible objects by the file's extension: .fbx (hierarchy, meshes, cameras, bones, skin / morph deformers, animation, materials and textures) or .obj (+ .mtl), with the FBX / OBJ export options (`Set Exchange Option`); without `path` the Export dialog shows, then the format's options dialog." { path: Option<String> };
    ExportSelected => "Export Selected", MainUI, "Export the selected objects to .fbx or .obj as Export File does; without `path` the dialog shows." { path: Option<String> };
    SetExchangeOption => "Set Exchange Option", Loom, "Set one FBX / OBJ import or export option. `dialog`: fbx_export, fbx_import, obj_export or obj_import; `name`: the option's field as `Exchange Options` lists it (e.g. fbx_export triangulate, up_axis \"Y-up\"/\"Z-up\", version 7400/7500, ascii, units_automatic, units; fbx_import file_content \"Add\"/\"Add and update animation\"/\"Update animation\", take; obj_export faces \"Triangles\"/\"Quads\"/\"Polygons\"; obj_import normals, objects [names]), or `preset` with a preset name; `value` any JSON value of the option's type. A change makes the preset \"User defined\"." { dialog: String, name: String, value: Value };
    ExchangeOptions => "Exchange Options", Loom, "The current options and presets of an FBX / OBJ import or export dialog (`dialog`: fbx_export, fbx_import, obj_export, obj_import)." { dialog: String };
    SaveExchangePreset => "Save Exchange Preset", Loom, "Edit/Save preset: save the dialog's current options as a named preset (kept in the settings)." { dialog: String, name: String };
    DeleteExchangePreset => "Delete Exchange Preset", Loom, "Delete a user preset of an FBX / OBJ options dialog." { dialog: String, name: String };
    FbxFileInfo => "FBX File Info", Loom, "The FBX Importer's Statistics for a file: name, folder, version, creator, creation time, axis and units of the file and the system, the units scale factor, frame rates, takes and element counts." { path: String };
    ObjFileInfo => "OBJ File Info", Loom, "The OBJ Importer's file info: the file's objects with their face counts, the total face count and which features it holds (normals, texture coordinates, smoothing groups, materials)." { path: String };
    MergeFile => "Merge File", MainUI, "Merge objects from another .loom file: all of them, or those in `names`; fresh ids, objects keep their layers (by name, created when missing), one undo entry. A name the scene already has follows its Duplicate Name answer in `duplicates` (merge keeps both, skip, delete_old, rename to `new_name`, auto_rename = the default). Without `path` the Merge dialog shows." { path: Option<String>, names: Option<Vec<String>>, duplicates: Option<Vec<crate::file::MergeDuplicate>> };
    SaveChangesReply => "Save Changes Reply", Loom, "Answer the open Save Changes prompt: Yes saves (Save As when untitled) and then runs the pending New / Reset / Open / Exit, No runs it without saving, Cancel keeps the scene." { reply: crate::command::SaveChangesReply };
    AutoBackupNow => "Auto Backup Now", Loom, "Write the next Auto Backup file (autoback/<name>NN.loom) now, regardless of the interval.";
    SetAutoBackupName => "Set Auto Backup Name", Loom, "Preferences > Files > Auto Backup File Name." { name: String };
    SelectAll => "Select All", MainUI, "Select all objects or sub-objects (Ctrl+A).";
    SelectNone => "Select None", MainUI, "Deselect everything (Ctrl+D).";
    SelectInvert => "Select Invert", MainUI, "Invert the selection (Ctrl+I).";
    DeleteObjects => "Delete Objects", MainUI, "Delete the selected objects or sub-objects (Delete).";
    Clone => "Clone", MainUI, "Open the Clone Options dialog for the selection (Ctrl+V, Edit > Clone).";
    CloneOptions => "Clone Options", Loom, "Clone the selection as Copy, Instance or Reference. `copies` repeats the clone; each copy applies `offset` (reference system), `rotate_deg` and `scale` factors once more than the previous, as a Shift+drag clone does. `name` names the first copy." { kind: crate::state::CloneKind, copies: u32, name: Option<String>, offset: Option<[f32; 3]>, rotate_deg: Option<[f32; 3]>, scale: Option<[f32; 3]> };
    TransformTypeIn => "Transform Type-In", Loom, "Apply Transform Type-In values for the active tool: Move = position (world for View/Screen, else reference system), Rotate = Euler degrees about the transform centre, Scale = percent (absolute scale is local). `absolute` false applies an offset." { absolute: bool, values: [f32; 3] };
    TransformTypeInDialogToggle => "Transform Type-In Dialog Toggle", MainUI, "Open or close the Transform Type-In dialog (F12).";
    SpinnerSnapToggle => "Spinner Snap Toggle", MainUI, "Toggle spinner snap: every spinner steps by the Preferences snap value.";
    KeepCreatingToggle => "Keep Creating Toggle", MainUI, "Keep Creating (the Create panel's Pin): on, the create tool stays on after each object; off, one object, then the Select tool with the object in the Modify panel.";
    // --- Tools ---
    SelectObject => "Select Object", MainUI, "Activate the Select Object tool (Q).";
    SelectAndMove => "Select and Move", MainUI, "Activate the Select and Move tool (W).";
    SelectAndRotate => "Select and Rotate", MainUI, "Activate the Select and Rotate tool (E).";
    SelectAndScale => "Select and Scale", MainUI, "Activate the Select and Scale tool (R).";
    ScaleCycle => "Scale Cycle", MainUI, "Cycle the Scale flyout: Uniform, Non-Uniform, Squash (Ctrl+E); from another tool it first turns on Scale.";
    ScaleType => "Scale Type", Loom, "Main Toolbar > Scale flyout: Uniform (all axes alike), NonUniform (the handle's axes) or Squash (the handle's axes, the others the opposite way); turns on Scale." { kind: crate::state::ScaleKind };
    SubObjectLevelCycle => "Sub-object Level Cycle", MainUI, "Cycle vertex, edge, face sub-object levels (Insert).";
    SubObjectSelectionToggle => "Sub-object Selection Toggle", MainUI, "Toggle between object and sub-object level (Ctrl+B).";
    SelectionLockToggle => "Selection Lock Toggle", MainUI, "Lock or unlock the selection (Space).";
    // --- Views ---
    TopView => "Top View", MainUI, "Show the Top view (T).";
    BottomView => "Bottom View", MainUI, "Show the Bottom view (B).";
    FrontView => "Front View", MainUI, "Show the Front view (F).";
    BackView => "Back View", MainUI, "Show the Back view.";
    LeftView => "Left View", MainUI, "Show the Left view (L).";
    RightView => "Right View", MainUI, "Show the Right view.";
    PerspectiveUserView => "Perspective User View", MainUI, "Show the Perspective view (P).";
    OrthographicUserView => "Orthographic User View", MainUI, "Show the Orthographic user view (U).";
    ZoomExtents => "Zoom Extents", MainUI, "Frame the whole scene in the active viewport (Alt+Ctrl+Z).";
    ZoomExtentsAll => "Zoom Extents All", MainUI, "Frame the whole scene in all viewports (Shift+Ctrl+Z).";
    ZoomExtentsSelected => "Zoom Extents Selected", MainUI, "Frame the selection (Z).";
    ZoomViewportIn => "Zoom Viewport In", MainUI, "Zoom the viewport in ([).";
    ZoomViewportOut => "Zoom Viewport Out", MainUI, "Zoom the viewport out (]).";
    HideGridsToggle => "Hide Grids Toggle", MainUI, "Show or hide the home grid in the active viewport (G).";
    MaximizeViewportToggle => "Maximize Viewport Toggle", MainUI, "Maximize or restore the active viewport (Alt+W).";
    DisableViewport => "Disable Viewport", MainUI, "Toggle updates of the active viewport (D).";
    RedrawAllViews => "Redraw All Views", MainUI, "Redraw every viewport (`).";
    HomeView => "Home View", MainUI, "ViewCube Home (Alt+Home): the active viewport goes to the Home view, a Perspective view (the default one, or the one Set Current View as Home stored).";
    ZoomIn2X => "Zoom In 2X", MainUI, "Zoom the active viewport in by a factor of two (Alt+Shift+Ctrl+Z).";
    ZoomOut2X => "Zoom Out 2X", MainUI, "Zoom the active viewport out by a factor of two (Alt+Shift+Z).";
    ZoomMode => "Zoom Mode", MainUI, "Toggle the modal Zoom tool (Alt+Z).";
    PanView => "Pan View", MainUI, "Toggle the modal Pan View tool (Ctrl+P).";
    PanViewport => "Pan Viewport", MainUI, "I: pan the active view so `point` (the world point under the cursor) becomes its centre; without a point, toggle the Pan tool." { point: Option<[f32; 3]> };
    OrbitViewMode => "Orbit View Mode", MainUI, "Toggle the modal Orbit tool (Ctrl+R).";
    ZoomRegionMode => "Zoom Region Mode", MainUI, "Toggle the modal Zoom Region tool (Ctrl+W).";
    FieldOfViewMode => "Field-of-View Mode", Loom, "Toggle the modal Field-of-View tool (Perspective viewports).";
    ViewportSafeframesToggle => "Viewport Safeframes Toggle", MainUI, "Show or hide safe frames in the active viewport (Shift+F).";
    ViewportClipping => "Viewport Clipping", MainUI, "General viewport label menu > Viewport Clipping: toggle the near / far clip slider of the active viewport.";
    SetViewportClipping => "Set Viewport Clipping", Loom, "Set a viewport's clip planes (distances from the eye; index omitted = active) and turn Viewport Clipping on." { index: Option<u32>, near: f32, far: f32 };
    ShowMainToolbarToggle => "Show Main Toolbar Toggle", MainUI, "Show or hide the Main Toolbar (Alt+6).";
    EditModifierStack => "Edit Modifier Stack", MainUI, "Focus the Modify panel's modifier stack.";
    AddModifier => "Add Modifier", Loom, "Add a modifier from the Modifier List above the active entry of the selected objects." { name: String };
    DeleteModifier => "Delete Modifier", Loom, "Remove stack entry `index` (0 = bottom) from the selected objects." { index: u32 };
    MoveModifier => "Move Modifier", Loom, "Reorder the stack: entry `from` goes to position `to`." { from: u32, to: u32 };
    CopyModifier => "Copy Modifier", Loom, "Shift-drag (copy) / Ctrl-drag (`instance`) in the stack: a copy of entry `from` is inserted so it ends up at position `to` (0 = bottom, up to the stack length)." { from: u32, to: u32, instance: bool };
    ToggleModifier => "Toggle Modifier", Loom, "Modifier on / off (the stack lightbulb)." { index: u32, on: bool };
    ToggleModifierInViewport => "Toggle Modifier In Viewport", Loom, "Modifier on / off in viewports only." { index: u32, on: bool };
    RenameModifier => "Rename Modifier", Loom, "Rename a stack entry." { index: u32, name: String };
    SelectStackEntry => "Select Stack Entry", Loom, "Make a stack entry current (null = the base object); its rollouts show." { index: Option<u32> };
    SetModifierParameter => "Set Modifier Parameter", Loom, "Set a modifier's rollout value by name; instanced entries edit their source." { index: u32, name: String, value: f64 };
    ModifierSubobjectLevel => "Modifier Sub-object Level", Loom, "Enter a modifier's sub-object level (1-based per its list, 0 = top)." { index: u32, level: u32 };
    ShowEndResultToggle => "Show End Result Toggle", Loom, "Show End Result on / off: the viewport shows the whole stack or only up to the active entry.";
    PinStackToggle => "Pin Stack Toggle", Loom, "Pin Stack: the Modify panel keeps showing this object.";
    MakeUnique => "Make Unique", Loom, "Make the active instanced modifier (or the instanced object) unique.";
    InstanceModifier => "Instance Modifier", Loom, "Paste Instanced: add an instance of another node's stack entry to the selected objects." { source: u64, index: u32 };
    CollapseTo => "Collapse To", Loom, "Collapse the stack up to entry `index` into the base Editable Poly." { index: u32 };
    CollapseAll => "Collapse All", Loom, "Collapse the whole stack into an Editable Poly.";
    ConfigureModifierSets => "Configure Modifier Sets", Loom, "The modifier button set shown under the Modifier List." { names: Vec<String> };
    PickModifierNode => "Pick Modifier Node", Loom, "Linked XForm > Pick Control Object / Preserve > Pick Original: the node stack entry `index` follows or references." { index: u32, node: u64 };
    ModifierSetButtonsToggle => "Modifier Set Buttons Toggle", Loom, "Show / hide the modifier button set.";
    SwitchToControlPointLevel => "Switch To Control Point Level", FFD, "FFD: enter the Control Points level of the active FFD entry (Alt+Shift+C).";
    SwitchToLatticeLevel => "Switch To Lattice Level", FFD, "FFD: enter the Lattice level of the active FFD entry (Alt+Shift+L).";
    SwitchToSetVolumeLevel => "Switch To Set Volume Level", FFD, "FFD: enter the Set Volume level of the active FFD entry (Alt+Shift+S).";
    SwitchToTopLevel => "Switch To Top Level", FFD, "FFD: leave the sub-object levels of the active FFD entry (Alt+Shift+T).";
    SelectControlPoints => "Select Control Points", Loom, "FFD: select control points of the active FFD entry by lattice index (x + y*nx + z*nx*ny)." { op: SelectOp, indices: Vec<u32> };
    ResetLattice => "Reset Lattice", Loom, "FFD: put every control point back to its rest position.";
    ConformToShape => "Conform To Shape", Loom, "FFD: move each control point along the line to the lattice centre onto the object's surface.";
    VertexPaintBrushToggle => "Vertex Paint Brush", Loom, "VertexPaint: toggle painting in the viewports with the entry's colour, brush size and opacity.";
    VertexPaintPickColorToggle => "Vertex Paint Pick Color", Loom, "VertexPaint > Pick Color From Object: toggle the eyedropper (drag over the object to take its colour).";
    VertexPaintPickColor => "Vertex Paint Pick Color At", Loom, "VertexPaint: the active entry's colour becomes the colour shown at an object-space point of `node`." { node: u64, point: [f32; 3] };
    VertexPaintFaceSelectToggle => "Vertex Paint Face Selection", Loom, "VertexPaint > Face Selection: toggle selecting the faces the brush and Paint All are limited to.";
    VertexPaintSelectFaces => "Vertex Paint Select Faces", Loom, "VertexPaint: select faces of the entry's input (none = paint everywhere)." { ids: Vec<u32>, op: SelectOp };
    VertexPaintPaintAll => "Vertex Paint Paint All", Loom, "VertexPaint > Paint All: fill the selected faces (every face when none is selected) with the entry's colour at its opacity.";
    VertexPaintNewLayer => "Vertex Paint New Layer", Loom, "VertexPaint > Layer > New Layer: another VertexPaint above the active one, named `name`, blending by `mode` (index in the Mode list)." { name: String, mode: u32 };
    AssignVertexColors => "Assign Vertex Colors", Loom, "Utilities > Assign Vertex Colors: bake the scene lights and the diffuse colour into vertex colours (Light Model 0 Lighting + Diffuse, 1 Lighting Only, 2 Diffuse; Color by Vertex or by Face; Shadows). `update` false = Assign To Selected (a new VertexPaint on each selected object), true = the active VertexPaint's Assign." { light_model: u32, by_vertex: bool, shadows: bool, update: bool };
    VertexPaintStroke => "Vertex Paint Stroke", Loom, "VertexPaint: paint the active VertexPaint entry at object-space points (0-255 colour, brush size, opacity)." { node: u64, points: Vec<[f32; 3]>, rgb: [u8; 3], size: f32, opacity: f32 };
    AttachSpline => "Attach (Spline)", MainUI, "Editable Spline > Attach: click a shape to attach it.";
    AttachMultipleSpline => "Attach Multiple (Spline)", MainUI, "Editable Spline > Attach Mult.: the other selected shapes join the first.";
    BreakVerticesSpline => "Break Vertices (Spline)", MainUI, "Editable Spline > Break: split the spline at the selected vertices.";
    ChamferVertexSpline => "Chamfer Vertex (Spline)", MainUI, "Editable Spline > Chamfer the selected vertices by the Chamfer spinner.";
    CloseSpline => "Close (Spline)", MainUI, "Editable Spline > Close the selected open splines.";
    ConnectSpline => "Connect (Spline)", MainUI, "Editable Spline > Connect the two selected end vertices with a segment.";
    CreateLineSpline => "Create Line (Spline)", MainUI, "Editable Spline > Create Line: the next Line drawn joins this shape.";
    CycleVerticesSpline => "Cycle Vertices (Spline)", MainUI, "Editable Spline > Cycle: select the next vertex of the spline.";
    DeleteSegmentSpline => "Delete Segment (Spline)", MainUI, "Editable Spline > Delete the selected segments.";
    DeleteSplineSpline => "Delete Spline (Spline)", MainUI, "Editable Spline > Delete the selected splines.";
    DeleteVertexSpline => "Delete Vertex (Spline)", MainUI, "Editable Spline > Delete the selected vertices.";
    DetachSegmentSpline => "Detach Segment (Spline)", MainUI, "Editable Spline > Detach the selected segments (Same Shp / Reorient / Copy settings).";
    DetachSplineSpline => "Detach Spline (Spline)", MainUI, "Editable Spline > Detach the selected splines (Same Shp / Reorient / Copy settings).";
    DivideSpline => "Divide (Spline)", MainUI, "Editable Spline > Divide the selected segments by the Divisions spinner.";
    ExplodeSpline => "Explode (Spline)", MainUI, "Editable Spline > Explode the selected splines into segments (Splines / Objects setting).";
    FilletSpline => "Fillet (Spline)", MainUI, "Editable Spline > Fillet the selected vertices by the Fillet spinner.";
    FuseVerticesSpline => "Fuse Vertices (Spline)", MainUI, "Editable Spline > Fuse: the selected vertices move to their average.";
    HideSpline => "Hide (Spline)", MainUI, "Editable Spline > Hide the splines of the selection.";
    UnhideAllSpline => "Unhide All (Spline)", Loom, "Editable Spline > Unhide All.";
    InsertSelectionSpline => "Insert Selection (Spline)", MainUI, "Editable Spline > Insert: click a segment to add a vertex and drag it.";
    MakeFirstSpline => "Make First (Spline)", MainUI, "Editable Spline > Make First: the selected vertex becomes the spline's first.";
    MirrorBothSpline => "Mirror Both H&V (Spline)", MainUI, "Editable Spline > Mirror both ways (Copy / About Pivot settings).";
    MirrorHorizontallySpline => "Mirror Horizontally (Spline)", MainUI, "Editable Spline > Mirror horizontally (Copy / About Pivot settings).";
    MirrorVerticallySpline => "Mirror Vertically (Spline)", MainUI, "Editable Spline > Mirror vertically (Copy / About Pivot settings).";
    OutlineSpline => "Outline (Spline)", MainUI, "Editable Spline > Outline the selected splines by the Outline spinner.";
    RefineSpline => "Refine (Spline)", MainUI, "Editable Spline > Refine: click a segment to add a vertex.";
    RefineConnectSpline => "Refine Connect (Spline)", MainUI, "Editable Spline > Refine > Connect toggle.";
    ReverseSpline => "Reverse (Spline)", MainUI, "Editable Spline > Reverse the selected splines.";
    SubtractSpline => "Subtract (Spline)", MainUI, "Editable Spline > Boolean Subtract the second selected closed spline from the first.";
    WeldVerticesSpline => "Weld Vertices (Spline)", MainUI, "Editable Spline > Weld the selected vertices within the Weld threshold.";
    SplineRefine => "Spline Refine", Loom, "Editable Spline: add a knot at parameter `t` of flat segment `segment` of node `node`." { node: u64, segment: u32, t: f32 };
    SplineRefineAtVertex => "Spline Refine At Vertex", Loom, "Editable Spline > Refine with Connect: a click on existing flat knot `knot` (`segment`/`t` is the segment point there) asks Refine / Connect Only, or runs the remembered choice." { node: u64, knot: u32, segment: u32, t: f32 };
    SplineRefineReply => "Spline Refine Reply", Loom, "Answer the open Refine prompt: Connect Only joins the existing vertex to the connection, Refine adds a vertex next to it; `dont_show` remembers the answer." { connect_only: bool, dont_show: bool };
    SplineFilletMode => "Spline Fillet Mode", Loom, "Editable Spline > Geometry > Fillet (or Chamfer) button: dragging on a selected vertex rounds (cuts) the selected vertices." { chamfer: bool };
    SplineFillet => "Spline Fillet", Loom, "Editable Spline: fillet the selected vertices by `amount`." { amount: f32 };
    SplineChamfer => "Spline Chamfer", Loom, "Editable Spline: chamfer the selected vertices by `amount`." { amount: f32 };
    SetVertexType => "Set Vertex Type", Loom, "Editable Spline: Corner / Smooth / Bezier / Bezier Corner for the selected vertices." { kind: String };
    SetKnotHandle => "Set Knot Handle", Loom, "Editable Spline: move the in (side_in) or out Bezier handle of flat knot `knot` to world `point` (Lock Handles moves the selection's alike)." { node: u64, knot: u32, side_in: bool, point: [f32; 3] };
    SplineOutline => "Spline Outline", Loom, "Editable Spline: outline the selected splines by `amount` (`center` splits it both ways)." { amount: f32, center: bool };
    SplineBoolean => "Spline Boolean", Loom, "Editable Spline: Union / Subtract / Intersect the selected closed spline with spline `other` of the same shape." { op: String, other: u32 };
    SplineMirror => "Spline Mirror", Loom, "Editable Spline: mirror the selected splines (axis 0 horizontal, 1 vertical, 2 both)." { axis: u32, copy: bool, about_pivot: bool };
    SplineDivide => "Spline Divide", Loom, "Editable Spline: add `n` vertices along each selected segment." { n: u32 };
    SplineWeld => "Spline Weld", Loom, "Editable Spline: weld selected vertices within `threshold`." { threshold: f32 };
    SplineFuse => "Spline Fuse", Loom, "Editable Spline: move the selected vertices to their average position.";
    SplineConnect => "Spline Connect", Loom, "Editable Spline: a segment between two end knots (flat indices)." { from: u32, to: u32 };
    SplineAttach => "Spline Attach", Loom, "Editable Spline: attach shape `node` to the selected shape." { node: u64, reorient: bool };
    SplineDetach => "Spline Detach", Loom, "Editable Spline: detach the selected segments / splines." { same_shape: bool, reorient: bool, copy: bool };
    SplineExplode => "Spline Explode", Loom, "Editable Spline: explode the selected splines into segments, as splines or objects." { to_objects: bool };
    SplineSettings => "Spline Settings", Loom, "Editable Spline rollout settings by name (Weld Threshold, Fillet, Chamfer, Outline, Outline Center, Divisions, Boolean, Mirror Axis, Mirror Copy, Mirror About Pivot, New Vertex Type, Refine Connect, Refine Linear, Lock Handles, Area Selection, …)." { name: String, value: f64 };
    CreateShapeFromEdges => "Create Shape From Edges", EditablePoly, "Editable Poly > Create Shape From Selection: the selected edges become a new Editable Spline (Smooth or Linear)." { name: Option<String>, smooth: bool };
    ExtrudeAlongSpline => "Extrude Along Spline", Loom, "Editable Poly: sweep the selected polygons along shape `spline`'s first spline: the end scales by 1 + `taper`, `taper_curve` bulges (+) or pinches (−) the middle, `twist` degrees along it, and with `align` the path starts along the face normal, turned `rotation` degrees about it." { spline: u64, segments: u32, taper: f32, taper_curve: f32, twist: f32, rotation: f32, align: bool };
    CreateBoolean => "Create Boolean", Loom, "Compound Objects > Boolean: the selected object (A) and `operand_b` become a Boolean (operation 0 Union, 1 Intersection, 2 A-B, 3 B-A, 4-7 Cut); `move_operand` removes B." { operand_b: u64, operation: u32, move_operand: bool };
    PickOperandB => "Pick Operand B", Loom, "Compound Objects > Boolean > Pick Operand B: click the second object.";
    CreateProBoolean => "Create ProBoolean", Loom, "Compound Objects > ProBoolean: the selected object becomes a ProBoolean base.";
    ProBooleanPickOperand => "ProBoolean Pick Operand", Loom, "ProBoolean > Start Picking: click an operand.";
    ProBooleanAddOperand => "ProBoolean Add Operand", Loom, "ProBoolean: add `node` with operation 0 Union, 1 Intersection, 2 Subtraction, 3 Merge, 4 Attach, 5 Insert." { node: u64, operation: u32, move_operand: bool };
    CreateLoft => "Create Loft", Loom, "Compound Objects > Loft > Get Path: the selected shape is the first cross-section along shape `path`." { path: u64 };
    LoftGetPathMode => "Loft Get Path", Loom, "Loft > Get Path: click the path shape.";
    LoftGetShapeMode => "Loft Get Shape", Loom, "Loft > Get Shape: click a shape to place at the Path percent.";
    LoftAddShape => "Loft Add Shape", Loom, "Loft: add shape `shape` at path `percent` to the selected Loft." { shape: u64, percent: f32 };
    CompoundSettings => "Compound Settings", Loom, "Compound rollout settings: Boolean Operation, Move Operand, ProBoolean Operation, Loft Path (percent)." { name: String, value: f64 };
    SetNumberOfPoints => "Set Number of Points", Loom, "FFD(box) / FFD(cyl): rebuild the lattice with this many points per dimension (cyl: side, radial, height)." { length: u32, width: u32, height: u32 };
    SelectAncestor => "Select Ancestor", MainUI, "Select the parents of the selected objects (PageUp).";
    SelectChild => "Select Child", MainUI, "Select the first child of each selected object (PageDown).";
    SelectChildren => "Select Children", MainUI, "Select every child of the selected objects (Ctrl+PageDown).";
    SelectSimilar => "Select Similar", MainUI, "Select every object of the same kind as the selection (Ctrl+Q).";
    SelectInstances => "Select Instances", MainUI, "Edit > Select Instances: select every instance of the one selected object.";
    SelectByColorPick => "Select By Color Pick", MainUI, "Edit > Select By > Color: the next clicked object selects every visible, unfrozen object of its object colour.";
    Group => "Group", MainUI, "Group the selected objects under a new group head (Group menu > Group)." { name: Option<String> };
    Ungroup => "Ungroup", MainUI, "Ungroup the selected groups: members keep their place, the head goes.";
    GroupOpen => "Open Group", MainUI, "Open the selected groups so their members can be selected and edited.";
    GroupClose => "Close Group", MainUI, "Close the selected (or the selected members') open groups.";
    GroupAttach => "Attach to Group", MainUI, "Toggle Attach: click a group to add the selected objects to it.";
    GroupDetach => "Detach from Group", MainUI, "Detach the selected members from their open group.";
    GroupExplode => "Explode Group", MainUI, "Ungroup the selected groups and every nested group inside them.";
    LayerManager => "Layer Manager", MainUI, "Open or close the Layer Manager (Manage Layers button).";
    CreateLayer => "Create Layer", Loom, "Layer Manager > New Layer: a new layer (named or Layer001…) that becomes current; the selection moves onto it." { name: Option<String> };
    DeleteLayer => "Delete Layer", Loom, "Delete an empty layer (never layer 0)." { layer: u32 };
    RenameLayer => "Rename Layer", Loom, "Rename a layer." { layer: u32, name: String };
    SetCurrentLayer => "Set Current Layer", Loom, "Make a layer current: new objects go onto it." { layer: u32 };
    SetLayerHidden => "Set Layer Hidden", Loom, "Hide or unhide every object on a layer." { layer: u32, on: bool };
    SetLayerFrozen => "Set Layer Frozen", Loom, "Freeze or unfreeze every object on a layer." { layer: u32, on: bool };
    MoveToLayer => "Move To Layer", Loom, "Move the selected objects onto a layer." { layer: u32 };
    AddSelectionToCurrentLayer => "Add Selection To Current Layer", Loom, "Layer Manager > Add Selection to Current Layer.";
    SelectLayerObjects => "Select Layer Objects", Loom, "Select every selectable object on a layer." { layer: u32 };
    SetCurrentLayerToSelection => "Set Current Layer to Selection's Layer", Loom, "Layers toolbar: the layer of the selected objects becomes current (refused when they are on different layers).";
    SetLayerColor => "Set Layer Color", Loom, "Layer Manager > Color: the layer colour (0-255 RGB); objects whose colour is By Layer show it." { layer: u32, rgb: [u8; 3] };
    SetLayerRenderable => "Set Layer Renderable", Loom, "Layer Manager > Render: the layer's Renderable; off, renders leave out its objects whose Rendering Control is By Layer." { layer: u32, on: bool };
    SetAllLayersHidden => "Set All Layers Hidden", Loom, "Layer Manager > Hide/Unhide All Layers." { on: bool };
    SetAllLayersFrozen => "Set All Layers Frozen", Loom, "Layer Manager > Freeze/Unfreeze All Layers." { on: bool };
    SetDisplayByLayer => "Set Display By Layer", Loom, "Object Properties > Display Properties By Layer / By Object for the selection: By Layer draws with the layer's display properties." { on: bool };
    SetRenderByLayer => "Set Render By Layer", Loom, "Object Properties > Rendering Control By Layer / By Object for the selection: By Layer renders with the layer's rendering control (Renderable, shadows, camera visibility)." { on: bool };
    LayerProperties => "Layer Properties", Loom, "Layer Manager: open the Layer Properties dialog of a layer." { layer: u32 };
    SetLayerDisplayProperty => "Set Layer Display Property", Loom, "Layer Properties > Display Properties checkbox by name (as Set Display Property) for objects set By Layer." { layer: u32, name: String, value: bool };
    SetLayerRenderProperty => "Set Layer Render Property", Loom, "Layer Properties > Rendering Control checkbox by name (Renderable, Inherit Visibility, Visible to Camera, Receive Shadows, Cast Shadows) for objects set By Layer." { layer: u32, name: String, value: bool };
    SetColorByLayer => "Set Color By Layer", Loom, "Object Color > By Layer / By Object for the selected objects: By Layer shows the layer's colour." { on: bool };
    ShowLayersToolbarToggle => "Show Layers Toolbar Toggle", Loom, "Customize > Show UI > Show Layers Toolbar (layer list, New Layer, Add Selection to Current Layer, Select Objects in Current Layer, Set Current Layer to Selection's Layer).";
    NewSceneKeep => "New Scene Keep", Loom, "File > New > New Scene dialog: `objects_and_hierarchy` keeps the objects and their links but removes animation keys; `objects` also unlinks them; `all` is New Scene. The scene becomes untitled." { keep: String };
    SaveSelected => "Save Selected", Loom, "File > Save As > Save Selected: the selected objects with their ancestors (and group members) to a new .loom; the current file stays. Without `path` the dialog shows." { path: Option<String> };
    ArchiveFile => "Archive", Loom, "File > Save As > Archive: a .zip of the scene file and every bitmap it uses, or with a .txt path a list of those files. Without `path` the dialog shows." { path: Option<String> };
    SummaryInfo => "Summary Info", Loom, "File > Summary Info: open the dialog with scene totals, the description and the materials list.";
    FileProperties => "File Properties", Loom, "File > File Properties: open the Summary / Contents / Custom dialog.";
    SetFileProperty => "Set File Property", Loom, "File Properties > Summary field (Title, Subject, Author, Manager, Company, Category, Keywords, Comments) or Summary Info > Description; saved with the scene." { name: String, value: String };
    SetCustomFileProperty => "Set Custom File Property", Loom, "File Properties > Custom: add or change a named property; no `value` deletes it." { name: String, value: Option<String> };
    ShowRibbonToggle => "Show Ribbon Toggle", Loom, "Show or hide the Graphite Modeling Tools ribbon (Main Toolbar button, Customize > Show UI > Show Ribbon).";
    LockUserInterfaceToggle => "Lock User Interface Toggle", MainUI, "Lock or unlock the UI layout (Alt+0).";
    ExpertModeToggle => "Expert Mode Toggle", MainUI, "Toggle Expert Mode: only the viewports are shown (Ctrl+X).";
    DisplayAsSeeThroughToggle => "Display as See-Through Toggle", MainUI, "Toggle see-through display of the selected objects (Alt+X).";
    HideCamerasToggle => "Hide Cameras Toggle", MainUI, "Hide or show all cameras (Shift+C).";
    HideGeometryToggle => "Hide Geometry Toggle", MainUI, "Hide or show all geometry (Shift+G).";
    HideHelpersToggle => "Hide Helpers Toggle", MainUI, "Hide or show all helpers (Shift+H).";
    HideLightsToggle => "Hide Lights Toggle", MainUI, "Hide or show all lights (Shift+L).";
    HideShapesToggle => "Hide Shapes Toggle", MainUI, "Hide or show all shapes (Shift+S).";
    IsolateSelection => "Isolate Selection", MainUI, "Hide everything but the selection, or exit isolation (Alt+Q).";
    ExitIsolation => "Exit Isolation", Loom, "Leave Isolate Selection mode and unhide what it hid.";
    ConfigureViewports => "Configure Viewports", Loom, "Open the Viewport Configuration dialog, optionally on a panel (\"Rendering Method\", \"Layout\", \"Safe Frames\", \"ViewCube\")." { panel: Option<String> };
    CloseDialog => "Close Dialog", Loom, "Close the open modeless dialog.";
    SelectByName => "Select By Name", MainUI, "Open the Select From Scene dialog (H).";
    CameraView => "Camera View", MainUI, "Switch the active viewport to the selected or only camera (C); with several cameras and none selected, open the Select Camera list.";
    ViewCubeToggle => "Show the ViewCube Toggle", MainUI, "Show or hide the ViewCube (Alt+Ctrl+V).";
    SetViewCubeOptions => "Set ViewCube Options", Loom, "Viewport Configuration > ViewCube: show it, in all views or the active one only, size (0 Large, 1 Normal, 2 Small, 3 Tiny), inactive opacity 0-100, compass." { show: Option<bool>, all_views: Option<bool>, size: Option<u32>, inactive_opacity: Option<f32>, compass: Option<bool> };
    SetCurrentViewAsHome => "Set Current View as Home", MainUI, "ViewCube menu: the active view becomes the Home view (saved with the scene).";
    SaveActiveView => "Save Active View", MainUI, "Views > Save Active <View> View: store the active view (one slot per view: Top, Front, Perspective...).";
    RestoreActiveView => "Restore Active View", MainUI, "Views > Restore Active <View> View: bring back the view saved for the active view's kind.";
    ShadeSelectedToggle => "Shade Selected", MainUI, "Views > Shade Selected: selected objects are shaded in wireframe viewports.";
    CreateCameraFromView => "Create Camera From View", MainUI, "Views > Create Camera From View (Ctrl+C): a Target camera matching the active Perspective view, which then looks through it; with a camera selected, that camera is moved to match the view.";
    DollyCameraMode => "Dolly Camera Mode", MainUI, "Camera viewport: drag to move the camera along its line of sight.";
    DollyTargetMode => "Dolly Target Mode", MainUI, "Camera viewport: drag to move the target along the line of sight (Target cameras).";
    DollyCameraTargetMode => "Dolly Camera + Target Mode", MainUI, "Camera viewport: drag to move camera and target together (Target cameras).";
    PerspectiveMode => "Perspective Mode", MainUI, "Camera viewport: drag to change FOV and dolly together, keeping the composition.";
    RollCameraMode => "Roll Camera Mode", MainUI, "Camera viewport: drag horizontally to roll the camera about its line of sight.";
    TruckCameraMode => "Truck Camera Mode", MainUI, "Camera viewport: drag to move the camera (and target) parallel to the view plane.";
    OrbitCameraMode => "Orbit Camera Mode", MainUI, "Camera viewport: drag to rotate the camera about its target.";
    PanCameraMode => "Pan Camera Mode", MainUI, "Camera viewport: drag to rotate the target about the camera.";
    NavigateCamera => "Navigate Camera", Loom, "Camera viewport controls on the active viewport's camera (one scene undo step): op dolly / dolly_target / dolly_both / perspective (x = factor on the target distance, < 1 closer), roll (x degrees), truck (x, y world units along the view's right / up), orbit / pan (x degrees about world Z, y degrees vertically), fov (x degrees)." { op: String, x: f32, y: Option<f32> };
    ViewportBackground => "Viewport Background", MainUI, "Open the Viewport Background dialog (Alt+B).";
    BackgroundLockToggle => "Background Lock Toggle", MainUI, "Lock the background image to the view (Alt+Ctrl+B).";
    UpdateBackgroundImage => "Update Background Image", MainUI, "Reload the active viewport's background image (Alt+Shift+Ctrl+B).";
    ShowFloatingDialogs => "Show Floating Dialogs", MainUI, "Show or hide floating dialogs (Ctrl+`).";
    ViewportLightingToggle => "Viewport Lighting Illuminate with Default / Scene Lights Toggle", MainUI, "Toggle default versus scene lighting in the active viewport (Ctrl+L).";
    ShadeSelectedFacesToggle => "Viewport Selection Shade Selected Faces Toggle", MainUI, "Shade selected faces red in the viewports (F2).";
    SelectionBracketToggle => "Viewport Selection Show Selection Bracket Toggle", MainUI, "Show or hide selection brackets (J).";
    TimeConfiguration => "Time Configuration", Loom, "Open the Time Configuration dialog.";
    SetFrame => "Set Frame", Loom, "Jump the time slider to a frame (clamped to the active time segment)." { frame: i32 };
    SetTimeConfiguration => "Set Time Configuration", Loom, "Time Configuration dialog: active time segment start/end and frame rate." { start: Option<i32>, end: Option<i32>, fps: Option<f32> };
    SetFov => "Set FOV", Loom, "Set the perspective field of view in degrees (Field-of-View button)." { degrees: f32 };
    AbsoluteOffsetToggle => "Absolute/Offset Mode Transform Type-In", Loom, "Status bar coordinate display: toggle between absolute and offset values.";
    OrbitSelectedMode => "Orbit Selected Mode", Loom, "Toggle the modal Orbit Selected tool (orbits around the selection).";
    OrbitSubObjectMode => "Orbit SubObject Mode", Loom, "Toggle the modal Orbit Sub-Object tool (orbits around the sub-object selection).";
    ZoomAll => "Zoom All", Loom, "Toggle the modal Zoom All tool (zooms every viewport).";
    ZoomExtentsAllSelected => "Zoom Extents All Selected", Loom, "Frame the selection in every viewport.";
    ViewportLayout => "Viewport Layout", Loom, "Set one of the 14 Viewport Configuration layouts." { layout: crate::viewport::ViewportLayout };
    ActiveViewport => "Active Viewport", Loom, "Activate a viewport by index." { index: u32 };
    ViewportView => "Viewport View", Loom, "Set the point of view of a viewport by index." { index: u32, kind: crate::state::ViewKind };
    ViewportShading => "Viewport Shading", Loom, "Set the active viewport's shading mode and edged faces." { shading: crate::viewport::Shading, edged_faces: Option<bool> };
    SetSplits => "Set Splits", Loom, "Move the viewport layout splitters (fractions 0.05–0.95)." { x: Option<f32>, y: Option<f32> };
    SetViewportBackground => "Set Viewport Background", Loom, "Viewport Background dialog OK: the image file (omitted = keep, empty = remove), Aspect Ratio (Match Viewport, Match Bitmap, Match Rendering Output), Lock Zoom/Pan, Display Background, for the active viewport or all of them." { path: Option<String>, aspect: Option<String>, lock: Option<bool>, display: Option<bool>, all_views: Option<bool> };
    ResetBackgroundTransform => "Reset Background Transform", MainUI, "Views > Viewport Background > Reset Background Transform: a locked background image is fitted to the active view again.";
    ShowStatisticsToggle => "Show Statistics Toggle", MainUI, "Show or hide polygon statistics (7).";
    ViewEdgedFacesToggle => "View Edged Faces Toggle", MainUI, "Toggle edged faces (F4).";
    WireframeSmoothHighlightsToggle => "Wireframe / Smooth+Highlights Toggle", MainUI, "Toggle wireframe and shaded display (F3).";
    // --- Align flyout / Mirror ---
    Align => "Align", MainUI, "Align the selection to a picked target object: pick the target, then the Align dialog (Alt+A).";
    QuickAlign => "Quick Align", MainUI, "Move the selection's pivot (or centre of several) onto a picked target's pivot (Shift+A).";
    NormalAlign => "Normal Align ", MainUI, "Pick a face on the source, then a face on the target: the source moves so the normals oppose, then the Normal Align dialog (Alt+N).";
    AlignCamera => "Align Camera", Loom, "Pick a face: the selected camera moves to look down its normal (Align flyout).";
    AlignToView => "Align to View Dialog", Loom, "Open the Align to View dialog (Align flyout).";
    AlignToViewAxis => "Align To View Axis", Loom, "Point the selection's local axis at the active viewport (toward the viewer; flip points away)." { axis: Axis, flip: bool };
    AlignObjects => "Align Objects", Loom, "Align dialog OK: per-axis position in the reference system using Minimum / Center / Pivot Point / Maximum of the current selection and the target, optional orientation and scale match." { target: u64, x: bool, y: bool, z: bool, current: crate::state::AlignPoint, target_pt: crate::state::AlignPoint, orientation: [bool; 3], match_scale: [bool; 3] };
    NormalAlignFaces => "Normal Align Faces", Loom, "Rotate and move the selection so the source normal (world point/normal on the selection) opposes the target normal (flip: matches it), then offset along the target frame and rotate about the normal." { source_point: [f32; 3], source_normal: [f32; 3], target_point: [f32; 3], target_normal: [f32; 3], offset: [f32; 3], angle_deg: f32, flip: bool };
    AlignCameraTo => "Align Camera To", Loom, "Move the selected camera onto the normal through a surface point, looking at the point, keeping its distance." { point: [f32; 3], normal: [f32; 3] };
    PickSurface => "Pick Surface", Loom, "Surface pick for Normal Align / Align Camera: the node, world point and normal under the cursor." { id: u64, point: [f32; 3], normal: [f32; 3] };
    Mirror => "Mirror", MainUI, "Open the Mirror dialog for the selection (Main Toolbar, Tools > Mirror).";
    MirrorObjects => "Mirror Objects", Loom, "Mirror the selection about the transform centre in the reference coordinate system; `clone` None mirrors in place, else Copy / Instance / Reference." { axis: crate::state::MirrorAxis, offset: f32, clone: Option<crate::state::CloneKind> };

    // --- Array / Spacing Tool ---
    Array => "Array", Loom, "Open the Array dialog for the selection (Tools > Array).";
    ArrayObjects => "Array Objects", Loom, "Array dialog OK: `count` = 1D/2D/3D counts; per-member increments `mv` (units), `rotate` (degrees), `scale` (percent) in the reference system about the transform centre, or totals over the row when `totals` [move, rotate, scale]; `row_2d` / `row_3d` incremental row offsets." { mv: [f32; 3], rotate: [f32; 3], scale: [f32; 3], totals: [bool; 3], re_orient: bool, uniform: bool, kind: crate::state::CloneKind, count: [u32; 3], row_2d: [f32; 3], row_3d: [f32; 3] };
    SpacingTool => "Spacing Tool", MainUI, "Open the Spacing Tool dialog (Shift+I).";
    SpacingPickPath => "Spacing Pick Path", Loom, "Spacing Tool > Pick Path: the next clicked shape is the path (all its splines).";
    SpaceAlongPath => "Space Along Path", Loom, "Spacing Tool > Apply: clones of the selection along the shape `path` (each spline) or the line from start to end, placed by Distribution mode `distribution` (0 Free Center … 18 Divide Evenly, No Objects at Ends; see spacing::DISTRIBUTIONS) from count / spacing / offsets; Edges spaces between bounding-box edges; follow turns local X along the path." { path: Option<u64>, start: [f32; 3], end: [f32; 3], distribution: u32, count: u32, spacing: f32, start_offset: f32, end_offset: f32, context: crate::state::SpacingContext, follow: bool, kind: crate::state::CloneKind };
    SpacingPickPoints => "Spacing Pick Points", Loom, "Spacing Tool > Pick Points: start picking the two path ends.";
    PickPoint => "Pick Point", Loom, "Spacing Tool > Pick Points: a world point for the current pick stage." { point: [f32; 3] };
    SpaceObjects => "Space Objects", Loom, "Spacing Tool Apply: distribute `count` clones of the selection along the line from `start` to `end` (offsets shrink the span); `spacing` fixes the step instead of dividing evenly; Edges context measures between bounding boxes; `follow` turns local X along the line." { start: [f32; 3], end: [f32; 3], count: u32, spacing: Option<f32>, start_offset: f32, end_offset: f32, context: crate::state::SpacingContext, follow: bool, kind: crate::state::CloneKind };

    // --- Properties, preferences, units, customize ---
    ObjectProperties => "Object Properties", MainUI, "Open the Object Properties dialog for the selection (Edit menu, quad menu).";
    Preferences => "Preferences", Loom, "Open Customize > Preferences.";
    SetPreference => "Set Preference", Loom, "Set a Preferences value by name (booleans as 0/1): Auto Backup Enable, Auto Backup Count, Auto Backup Interval (minutes), Scene Undo Levels, Reference Coordinate System Constant, Auto Window/Crossing by Direction, Paint Selection Brush Size, Spinner Precision, Spinner Snap, Spinner Use Snap, Spinner Wrap Cursor, Viewport Tooltips, Show Cross Hair Cursor, Transform Gizmos, Show Axis Labels, Allow Multiple Gizmos, Gizmo Size, Move Plane Handle Size, Move Plane Handle Offset, Center Box Handle, Rotate Free Rotation, Rotate Show Tripod, Rotate Screen Handle, Rotate Show Pie Slice, Rotate Angle Data, Scale Size, Rotation Increment, Viewport Orbit Snap Angle, Space Locks Selection, Recent Files in File Menu, Keep Creating." { name: String, value: f64 };
    UnitsSetup => "Units Setup", Loom, "Open Customize > Units Setup.";
    SetUnits => "Set Units", Loom, "Units Setup: `system_unit` Inches/Feet/Millimeters/Centimeters/Meters/Kilometers, `system_scale` units per system unit, `display` Generic / Millimeters / Centimeters / Meters / Kilometers / Decimal Inches / Decimal Feet / Feet w/ Decimal Inches / Custom (with custom_name, custom_value, custom_unit)." { system_unit: Option<String>, system_scale: Option<f32>, display: Option<String>, custom_name: Option<String>, custom_value: Option<f32>, custom_unit: Option<String> };
    CustomizeUserInterface => "Customize User Interface", Loom, "Open Customize > Customize User Interface (Keyboard panel).";
    AssignShortcut => "Assign Shortcut", Loom, "Customize User Interface > Keyboard > Assign: bind `chord` (chart spelling, e.g. Alt+Ctrl+Z, NumPad +) to `action` in `group` (Main UI, Editable Poly, Edit Poly, Editable Spline, FFD, Unwrap UVW, Skin, Track View, Edit Normals); the action that had the chord there loses it. Saved in the settings file." { group: String, action: String, chord: String };
    RemoveShortcut => "Remove Shortcut", Loom, "Customize User Interface > Keyboard > Remove: unbind every shortcut of `action` in `group`." { group: String, action: String };
    ResetShortcuts => "Reset Shortcuts", Loom, "Customize User Interface > Keyboard > Reset: every group back to the default shortcuts.";
    SaveShortcuts => "Save Shortcuts", Loom, "Customize User Interface > Keyboard > Save: write every group's shortcuts to a .kbd file (3dLoom's own JSON format); without `path` the Save dialog shows." { path: Option<String> };
    LoadShortcuts => "Load Shortcuts", Loom, "Customize User Interface > Keyboard > Load: read shortcuts written by Save Shortcuts; the groups in the file replace the current ones. Without `path` the Open dialog shows." { path: Option<String> };
    LoadCustomUiScheme => "Load Custom UI Scheme", MainUI, "Customize > Load Custom UI Scheme: apply a .ui file written by Save Custom UI Scheme (Show UI toggles and shortcuts); without `path` the Open dialog shows." { path: Option<String> };
    SaveCustomUiScheme => "Save Custom UI Scheme", MainUI, "Customize > Save Custom UI Scheme: write the Show UI toggles and shortcuts to a .ui file; without `path` the Save dialog shows." { path: Option<String> };
    RevertToStartupUi => "Revert to Startup UI", MainUI, "Customize > Revert to Startup Layout: the Show UI toggles as they were when 3dLoom started.";
    ShowCommandPanelToggle => "Show Command Panel Toggle", MainUI, "Customize > Show UI > Show Command Panel: show or hide the command panel.";
    SelectAndManipulate => "Select and Manipulate", MainUI, "Toggle manipulators: drag the radius / height handles of primitives in viewports.";
    Manipulate => "Manipulate", Loom, "Add `delta` to a float parameter of a node (what a manipulator drag does)." { id: u64, name: String, delta: f64 };

    // --- Editable Poly: Selection rollout (chart "Editable Poly" table) ---
    VertexLevel => "Vertex Level", EditablePoly, "Vertex sub-object level; again returns to the top level (1).";
    EdgeLevel => "Edge Level", EditablePoly, "Edge sub-object level (2).";
    BorderLevel => "Border Level", EditablePoly, "Border sub-object level (3).";
    FaceLevel => "Face Level", EditablePoly, "Polygon sub-object level (4).";
    ElementLevel => "Element Level", EditablePoly, "Element sub-object level (5).";
    ObjectLevel => "Object Level", EditablePoly, "Top (object) level (6).";
    GrowSelection => "Grow Selection", EditablePoly, "Grow the sub-object selection by adjacency (Ctrl+PageUp).";
    ShrinkSelection => "Shrink Selection", EditablePoly, "Shrink the sub-object selection by one ring (Ctrl+PageDown).";
    SelectEdgeRing => "Select Edge Ring", EditablePoly, "Select the ring of the selected edges.";
    SelectEdgeLoop => "Select Edge Loop", EditablePoly, "Select the loop of the selected edges.";
    SelectEdgeRingUp => "Select Edge Ring Up", EditablePoly, "Shift the ring selection one step along its loop.";
    SelectEdgeRingDown => "Select Edge Ring Down", EditablePoly, "Shift the ring selection one step back along its loop.";
    SelectEdgeRingUpAdd => "Select Edge Ring Up Add", EditablePoly, "Ring Up, keeping the current selection.";
    SelectEdgeRingDownAdd => "Select Edge Ring Down Add", EditablePoly, "Ring Down, keeping the current selection.";
    SelectEdgeRingUpSubtract => "Select Edge Ring Up Subtract", EditablePoly, "Ring Up, removing the stepped edges from the selection.";
    SelectEdgeRingDownSubtract => "Select Edge Ring Down Subtract", EditablePoly, "Ring Down, removing the stepped edges from the selection.";
    SelectEdgeLoopUp => "Select Edge Loop Up", EditablePoly, "Shift the loop selection to the neighbouring loop.";
    SelectEdgeLoopDown => "Select Edge Loop Down", EditablePoly, "Shift the loop selection to the neighbouring loop on the other side.";
    SelectEdgeLoopUpAdd => "Select Edge Loop Up Add", EditablePoly, "Loop Up, keeping the current selection.";
    SelectEdgeLoopDownAdd => "Select Edge Loop Down Add", EditablePoly, "Loop Down, keeping the current selection.";
    SelectEdgeLoopUpSubtract => "Select Edge Loop Up SubTract", EditablePoly, "Loop Up, removing the stepped edges from the selection.";
    SelectEdgeLoopDownSubtract => "Select Edge Loop Down Subtract", EditablePoly, "Loop Down, removing the stepped edges from the selection.";
    SelectByVertex => "Select By Vertex", EditablePoly, "Toggle By Vertex: clicking a vertex selects the sub-objects using it.";
    IgnoreBackfacingInSelections => "Ignore Backfacing in Selections", EditablePoly, "Toggle Ignore Backfacing.";
    PreviewSelectMode => "Preview Select Mode", EditablePoly, "Cycle Preview Selection: Off, SubObj, Multi.";
    Hide => "Hide", EditablePoly, "Hide the selected vertices or polygons (Alt+H).";
    Connect => "Connect", EditablePoly, "Vertex level: connect the selected vertex pairs across their polygons. Edge / Border level: Connect Edges with the current settings (Shift+Ctrl+E).";
    Remove => "Remove", EditablePoly, "Vertex level: remove the selected vertices and merge their polygons (Backspace). Edge level: remove the edges (Backspace).";
    Break => "Break", EditablePoly, "Break the selected vertices so every polygon gets its own corner.";
    SplitEdges => "Split Edges", EditablePoly, "Split the mesh along the selected edges (open them into borders).";
    TargetWeldMode => "Target Weld Mode", EditablePoly, "Toggle Target Weld: pick a vertex, then the vertex to weld it onto (Shift+Ctrl+W). Picks arrive as Pick Sub.";
    WeldSettings => "Weld Settings", EditablePoly, "Open the Weld settings window (threshold with live preview).";
    ChamferMode => "Chamfer Mode", EditablePoly, "Toggle Chamfer drag mode: drag a selected vertex or edge to chamfer it (Shift+Ctrl+C).";
    ChamferSettings => "Chamfer Settings", EditablePoly, "Open the Chamfer settings window.";
    ConnectEdgeSettings => "Connect Edge Settings", EditablePoly, "Open the Connect Edges settings window.";
    RemoveIsolatedVertices => "Remove Isolated Vertices", EditablePoly, "Delete vertices that belong to no polygon.";
    RemoveUnusedMapVertices => "Remove Unused Map Vertices", EditablePoly, "Delete map vertices no map face uses (maps arrive in phase 5; until then nothing to remove).";
    InsertVertexMode => "Insert Vertex Mode", EditablePoly, "Toggle Insert Vertex: click an edge or polygon to add a vertex there.";
    Cap => "Cap", EditablePoly, "Cap the selected borders with one polygon each.";
    BridgeSettings => "Bridge Settings", EditablePoly, "Open the Bridge settings window.";
    ExtrudeMode => "Extrude Mode", EditablePoly, "Toggle Extrude drag mode: drag the selection to set the extrusion height (Shift+E).";
    ExtrudeSettings => "Extrude Settings", EditablePoly, "Open the Extrude settings window.";
    Turn => "Turn", EditablePoly, "Turn the selected edges between triangles (flip the diagonal).";
    EditTriangulationMode => "Edit Triangulation Mode", EditablePoly, "Toggle Edit Triangulation: pick two vertices of a polygon to add the diagonal.";
    Retriangulate => "Retriangulate", EditablePoly, "Recompute the triangulation of the selected polygons (Loom triangulates on display; no stored diagonals to reset).";
    Create => "Create", EditablePoly, "Vertex level: Create Vertex at clicked points. Polygon level: pick vertices in order (Pick Sub), finish on the first vertex or with Exit Pick Mode.";
    InsetMode => "Inset Mode", EditablePoly, "Toggle Inset drag mode: drag the selected polygons to inset them.";
    InsetSettings => "Inset Settings", EditablePoly, "Open the Inset settings window.";
    BevelMode => "Bevel Mode", EditablePoly, "Toggle Bevel drag mode: drag up/down for height, sideways for outline (Shift+Ctrl+B).";
    BevelSettings => "Bevel Settings", EditablePoly, "Open the Bevel settings window.";
    OutlineMode => "Outline Mode", EditablePoly, "Toggle Outline drag mode: drag the selected polygons to grow or shrink their outline.";
    OutlineSettings => "Outline Settings", EditablePoly, "Open the Outline settings window.";
    HingeFromEdgeMode => "Hinge from Edge Mode", EditablePoly, "Toggle Hinge From Edge: click the hinge edge, then drag to swing the selected polygons.";
    HingeFromEdgeSettings => "Hinge from Edge Settings", EditablePoly, "Open the Hinge From Edge settings window (Pick Hinge, angle, segments).";
    ExtrudeAlongSplineMode => "Extrude along Spline Mode", EditablePoly, "Extrude Along Spline: click a shape to sweep the selected polygons along it.";
    ExtrudeAlongSplineSettings => "Extrude along Spline Settings", EditablePoly, "Extrude Along Spline settings dialog (segments, taper, twist, align).";
    FlipNormals => "Flip Normals", EditablePoly, "Flip the selected polygons' normals (whole elements).";
    MakePlanar => "Make Planar", EditablePoly, "Move the selected sub-objects onto their average plane.";
    ViewAlign => "Align To View", EditablePoly, "View Align: flatten the selection onto the active view's plane.";
    AlignToGrid => "Align To Grid", EditablePoly, "Grid Align: flatten the selection onto the home grid plane.";
    Attach => "Attach", EditablePoly, "Toggle Attach: click another object to attach it to the selected Editable Poly.";
    AttachList => "Attach List", EditablePoly, "Open the Attach List window (pick objects by name).";
    Detach => "Detach", EditablePoly, "Open the Detach window for the selected sub-objects.";
    SlicePlaneMode => "Slice Plane Mode", EditablePoly, "Toggle the Slice Plane gizmo; move / rotate it with the transform tools, then Slice.";
    Slice => "Slice", EditablePoly, "Slice the mesh (or the selected polygons) with the Slice Plane.";
    ResetSlicePlane => "Reset Slice Plane", EditablePoly, "Put the Slice Plane back at the selection's centre, level.";
    QuickSliceMode => "Quickslice Mode", EditablePoly, "Toggle QuickSlice: click two points; the plane through them and the view direction slices the mesh (Shift+Ctrl+Q).";
    Cut => "Cut", EditablePoly, "Toggle Cut: click a start point, then each click cuts on from the last point; right-click ends the cut, a second right-click leaves Cut (Alt+C).";
    MeshSmooth => "MeshSmooth", EditablePoly, "MSmooth the selection with the current settings.";
    MeshSmoothSettings => "MeshSmooth Settings", EditablePoly, "Open the MeshSmooth settings window.";
    Tessellate => "Tessellate", EditablePoly, "Tessellate the selected polygons with the current settings.";
    TessellateSettings => "Tessellate Settings", EditablePoly, "Open the Tessellate settings window.";
    ConstrainToNone => "Constrain to None", EditablePoly, "Transform constraint off.";
    ConstrainToEdges => "Constrain to Edges", EditablePoly, "Constrain sub-object transforms to the adjacent edges (Shift+X toggles).";
    ConstrainToFaces => "Constrain to Faces", EditablePoly, "Constrain sub-object transforms to the surface.";
    ConstrainToNormals => "Constrain to Normals", EditablePoly, "Constrain sub-object transforms to the vertex normals.";
    AutoSmooth => "Auto Smooth", EditablePoly, "Auto Smooth the selected polygons (all when none) with the rollout threshold.";
    PaintDeformPushPull => "Paint Deform Push/Pull", EditablePoly, "Toggle the Push/Pull paint brush (Ctrl inverts, Shift relaxes).";
    PaintDeformRelax => "Paint Deform Relax", EditablePoly, "Toggle the Relax paint brush.";
    RepeatLastOperation => "Repeat Last Operation", EditablePoly, "Repeat the last Editable Poly geometry operation with its settings (;).";
    AffectBackfacing => "Affect Backfacing", EditablePoly, "Toggle Soft Selection > Affect Backfacing.";
    ConvertSelection => "Convert Selection", Loom, "Change the sub-object level converting the selection as a Ctrl-click (touching) or Shift-click (contained) on a level button does." { to: crate::state::SelectionMode, contained: bool };
    NamedSubSelectionCopy => "Named Sub Selection Copy", Loom, "Edit Geometry > Named Selections > Copy: remember the sub-object selection under a name." { name: String };
    NamedSubSelectionPaste => "Named Sub Selection Paste", Loom, "Edit Geometry > Named Selections > Paste: restore a remembered sub-object selection." { name: String };

    // --- Editable Poly: Soft Selection rollout ---
    UseSoftSelection => "Use Soft Selection", EditablePoly, "Toggle Soft Selection for the selected Editable Poly.";
    LockSoftSelection => "Lock Soft Selection", EditablePoly, "Lock the current soft selection weights so reselecting does not change them.";
    EditSoftSelectionMode => "Edit Soft Selection Mode", EditablePoly, "Toggle the soft selection paint mode (Paint Soft Selection).";
    PaintSoftSelection => "Paint Soft Selection", EditablePoly, "Paint soft selection weights with a brush (Paint; Blur and Revert are the Loom variants).";
    ShadedFaceToggle => "Shaded Face Toggle", EditablePoly, "Shade faces by soft selection weight instead of only vertices.";
    SoftSelectionSettings => "Soft Selection Settings", Loom, "Soft Selection rollout values for the selected polys; omitted fields keep their value. edge_distance 0 turns Edge Distance off." { falloff: Option<f32>, pinch: Option<f32>, bubble: Option<f32>, edge_distance: Option<u32>, affect_backfacing: Option<bool> };
    PaintSoftSelectionBlur => "Paint Soft Selection Blur", Loom, "Blur brush for the painted soft selection.";
    SoftSelectionRevert => "Soft Selection Revert", Loom, "Revert painted soft selection weights (Paint Soft Selection > Revert).";
    Weld => "Weld", Loom, "Weld selected vertices closer than `threshold` (the Weld button); also sets the Weld threshold." { threshold: f32 };
    Chamfer => "Chamfer", Loom, "Chamfer the selected vertices (amount, open) or edges (amount, segments, open); sets the Chamfer settings." { amount: f32, segments: u32, open: bool };
    ConnectEdges => "Connect Edges", Loom, "Connect the selected edges with `segments` new edges; pinch and slide; sets the Connect settings." { segments: u32, pinch: f32, slide: f32 };
    ConnectVertices => "Connect Vertices", Loom, "Connect the selected vertex pairs that share a polygon.";
    Bridge => "Bridge", Loom, "Bridge two selected borders or two selected polygons; at Edge level the selected border edges pair up in increasing order of id (or the two picked edges with Bridge Specific Edges); sets the Bridge settings." { segments: u32, smooth: f32, bridge_adjacent: f32, twist1: i32, twist2: i32 };
    Extrude => "Extrude", Loom, "Extrude the selection: vertices / edges by height and base width, polygons by height with `kind`; sets the Extrude settings." { height: f32, width: f32, kind: crate::state::ExtrudeKind };
    Inset => "Inset", Loom, "Inset the selected polygons by `amount`, as a group or by polygon; sets the Inset settings." { amount: f32, by_polygon: bool };
    Bevel => "Bevel", Loom, "Bevel the selected polygons: extrude by `height`, then outline by `outline`; sets the Bevel settings." { height: f32, outline: f32, kind: crate::state::ExtrudeKind };
    Outline => "Outline", Loom, "Outline the selected polygons by `amount` (negative shrinks); sets the Outline settings." { amount: f32 };
    HingeFromEdge => "Hinge From Edge", Loom, "Swing the selected polygons `angle` degrees about the hinge edge (raw edge id; omitted = the picked hinge) in `segments` steps." { angle: f32, segments: u32, edge: Option<u32> };
    PickHinge => "Pick Hinge", Loom, "Hinge From Edge > Pick Hinge: the next edge pick (Pick Sub with an edge id) becomes the hinge.";
    MakePlanarAxis => "Make Planar Axis", Loom, "Make Planar X / Y / Z: flatten the selection along one object axis." { axis: Axis };
    AttachNode => "Attach Node", Loom, "Attach a node to the selected Editable Poly (what the Attach click does)." { id: u64 };
    DetachSelection => "Detach Selection", Loom, "Detach the selected polygons (or the polygons of the selected sub-objects) as a new object `name`; `to_element` keeps them in the mesh as an element; `as_clone` leaves the source untouched." { name: Option<String>, to_element: bool, as_clone: bool };
    SetSlicePlane => "Set Slice Plane", Loom, "Place the Slice Plane: world position, Euler rotation in degrees and half-size." { position: Option<[f32; 3]>, rotation_deg: Option<[f32; 3]>, size: Option<f32> };
    SliceSplit => "Slice Split", Loom, "Slice Plane > Split: slicing also tears the mesh." { on: bool };
    QuickSlice => "Quick Slice", Loom, "Slice with the plane through two world points and the active view direction." { a: [f32; 3], b: [f32; 3] };
    PickFacePoint => "Pick Face Point", Loom, "A click on a polygon for Cut / QuickSlice: node, raw face id and world point." { node: u64, face: u32, point: [f32; 3] };
    MSmooth => "MSmooth", Loom, "MeshSmooth: one Catmull-Clark pass over the selected polygons' object; sets the MeshSmooth settings." { smoothness: f32, separate_by_smoothing: bool, separate_by_materials: bool };
    TessellateFaces => "Tessellate Faces", Loom, "Tessellate the selected polygons by `kind` (Edge or Face) with `tension` percent; sets the Tessellate settings." { kind: crate::state::TessKind, tension: f32 };
    Relax => "Relax", Loom, "Relax the selected vertices (or those of the selected sub-objects); sets the Relax settings." { amount: f32, iterations: u32, hold_boundary: bool, hold_outer: bool };
    RelaxSettings => "Relax Settings", Loom, "Open the Relax settings window.";
    FullInteractivity => "Full Interactivity", Loom, "Edit Geometry > Full Interactivity: settings windows preview on every spinner change." { on: bool };
    PreserveUVs => "Preserve UVs", Loom, "Edit Geometry > Preserve UVs toggle (map channels arrive in phase 5; the flag is kept)." { on: bool };
    DeleteIsolatedVertices => "Delete Isolated Vertices", Loom, "Delete vertices no polygon uses (Edit Geometry button).";
    SetMaterialId => "Set Material ID", Loom, "Polygon Properties > Set ID for the selected polygons (1-based)." { id: u16 };
    SelectByMaterialId => "Select By Material ID", Loom, "Select the polygons with material id `id`; `clear` replaces the selection." { id: u16, clear: bool };
    SetSmoothingGroups => "Set Smoothing Groups", Loom, "Set the smoothing groups of the selected polygons to `bits` (bit n = group n+1); `add` keeps the existing bits." { bits: u32, add: bool };
    ClearSmoothingGroups => "Clear Smoothing Groups", Loom, "Clear All: no smoothing groups on the selected polygons.";
    SelectBySmoothingGroup => "Select By Smoothing Group", Loom, "Select the polygons that use any of the groups in `bits`; `clear` replaces the selection." { bits: u32, clear: bool };
    AutoSmoothThreshold => "Auto Smooth Threshold", Loom, "Polygon Properties > Auto Smooth threshold in degrees." { degrees: f32 };
    SetVertexColor => "Set Vertex Color", Loom, "Set the vertex colour (0-255) of the selected vertices, or of the selected polygons' vertices." { rgb: [u8; 3] };
    SetVertexIllumination => "Set Vertex Illumination", Loom, "Set the vertex illumination colour (0-255) of the selection." { rgb: [u8; 3] };
    SetVertexAlpha => "Set Vertex Alpha", Loom, "Set the vertex alpha (0-100 %) of the selection." { alpha: f32 };
    SelectByVertexColor => "Select by Vertex Color", Loom, "Vertex Properties > Select Vertices By colour (or illumination) within an RGB range (0-255 per channel)." { rgb: [u8; 3], range: [u8; 3], illumination: bool };
    SubdivisionSurface => "Subdivision Surface", Loom, "Subdivision Surface rollout values for the selected polys; omitted fields keep their value." { use_nurms: Option<bool>, smooth_result: Option<bool>, isoline_display: Option<bool>, show_cage: Option<bool>, iterations: Option<u32>, smoothness: Option<f32>, separate_by_smoothing: Option<bool>, separate_by_materials: Option<bool> };
    PaintDeformRevert => "Paint Deform Revert", Loom, "Toggle the Revert paint brush (paints back toward the positions before painting).";
    PaintDeformCommit => "Paint Deform Commit", Loom, "Commit the painted deformation (forget the original positions).";
    PaintDeformCancel => "Paint Deform Cancel", Loom, "Cancel the painted deformation: restore the original positions.";
    PaintDeformSettings => "Paint Deform Settings", Loom, "Push/Pull Value, Brush Size, Brush Strength and Push/Pull Direction (OriginalNormals, DeformedNormals, X, Y, Z); omitted fields keep their value." { value: Option<f32>, size: Option<f32>, strength: Option<f32>, direction: Option<String> };
    PaintDeformStroke => "Paint Deform Stroke", Loom, "One brush stroke on a poly: world `points` along the stroke; `mode` PushPull, Relax or Revert; `invert` pushes instead of pulls." { node: u64, points: Vec<[f32; 3]>, mode: crate::state::PaintMode, invert: bool };
    PickSub => "Pick Sub", Loom, "A sub-object pick for the active pick mode (Target Weld, Edit Triangulation, Create: a vertex id; Hinge: an edge id)." { node: u64, id: u32 };
    InsertVertexOnEdge => "Insert Vertex On Edge", Loom, "Insert Vertex: add a vertex on an edge at parameter `t` (0..1 along the half-edge)." { node: u64, edge: u32, t: f32 };
    InsertVertexInFace => "Insert Vertex In Face", Loom, "Insert Vertex: add a vertex inside a polygon at a world point and fan the polygon around it." { node: u64, face: u32, point: [f32; 3] };
    CreateVertex => "Create Vertex", Loom, "Create (vertex level): add an isolated vertex at a world point." { node: u64, point: [f32; 3] };
    PaintSoftSelectionStroke => "Paint Soft Selection Stroke", Loom, "One brush stroke on a poly: `points` are world positions along the stroke, `size` the brush radius (world units), `value` the painted weight, `strength` 0..1." { node: u64, points: Vec<[f32; 3]>, value: f32, size: f32, strength: f32 };

    // --- Snaps / animation ---
    SnapsToggle => "Snaps Toggle", MainUI, "Toggle snaps (S).";
    AngleSnapToggle => "Angle Snap Toggle", MainUI, "Toggle angle snap (A).";
    PercentSnapToggle => "Percent Snap Toggle", MainUI, "Toggle percent snap (Shift+Ctrl+P).";
    CycleActiveSnapType => "Cycle Active Snap Type", MainUI, "Cycle the Snaps Toggle flyout 2D, 2.5D, 3D (Alt+S).";
    CycleSnapHit => "Cycle Snap Hit", MainUI, "Cycle between snap candidates inside the snap radius (Alt+Shift+S).";
    EnableAxisConstraintsInSnapsToggle => "Enable Axis Constraints in Snaps Toggle", MainUI, "Snaps respect the current axis constraints (Alt+D / Alt+F3).";
    SnapToFrozenObjectsToggle => "Snap To Frozen Objects Toggle", MainUI, "Snap to frozen objects (Alt+F2).";
    GridAndSnapSettings => "Grid and Snap Settings", Loom, "Open the Grid and Snap Settings dialog (Tools > Grids and Snaps; right-click a snap toggle).";
    SnapTarget => "Snap Target", Loom, "Turn one Snaps tab target on or off: Grid Points, Pivot, Perpendicular, Vertex, Edge/Segment, Face, Grid Lines, Bounding Box, Tangent, Endpoint, Midpoint, Center Face." { name: String, on: bool };
    SnapMarkerColor => "Snap Marker Color", Loom, "Grid and Snap Settings > Options > Marker > Color: the snap marker's colour (sRGB)." { rgb: [u8; 3] };
    UserGridSettings => "User Grid Settings", Loom, "Grid and Snap Settings > User Grids: Activate grids when creating; AutoGrid in Object space (else World space). Omitted fields keep their value." { activate_when_created: Option<bool>, autogrid_object_space: Option<bool> };
    SnapOverride => "Snap Override", Loom, "Use one snap target for the next snap only (Snaps shortcut menu); \"None\" disables snapping for the next click; omit to clear." { name: Option<String> };
    SnapSettings => "Snap Settings", Loom, "Set Options tab values; omitted fields keep their value." { mode: Option<crate::snap::SnapMode>, marker_display: Option<bool>, marker_size: Option<f32>, preview_radius_px: Option<f32>, radius_px: Option<f32>, angle_deg: Option<f32>, percent: Option<f32>, use_axis_constraints: Option<bool>, show_rubber_band: Option<bool> };
    HomeGrid => "Home Grid", Loom, "Set Home Grid tab values; omitted fields keep their value." { spacing: Option<f32>, major_every: Option<u32>, perspective_extent: Option<u32>, inhibit_subdivision: Option<bool>, inhibit_resize: Option<bool>, dynamic_update_all: Option<bool> };
    AutoKeyModeToggle => "Auto Key Mode Toggle", MainUI, "Toggle Auto Key (N).";
    GoToStartFrame => "Go to Start Frame", MainUI, "Jump to the first frame (Home).";
    GoToEndFrame => "Go to End Frame", MainUI, "Jump to the last frame (End).";
    ForwardTimeOneUnit => "Forward Time One Unit", MainUI, "Step one frame forward (.).";
    BackupTimeOneUnit => "Backup Time One Unit", MainUI, "Step one frame back (,).";
    SetKeyMode => "Set Key Mode", MainUI, "Toggle Set Key mode ('): pose freely, then Set Keys commits the pose as keys; changing frame discards it.";
    SetKeys => "Set Keys", MainUI, "Key the filtered tracks of the selection at the current frame (K); in Set Key mode, commit the pose.";
    MaterialEditorToggle => "Material Editor Toggle", MainUI, "Open or close the Material Editor (M).";
    Render => "Render", MainUI, "Render Production (Shift+Q): render the Rendered Frame Window's viewport (the active one unless Lock to Viewport) with the Render Setup settings into the Rendered Frame Window, without blocking; `path` also saves the image there (png, jpg, bmp, tga, exr), as Save File does with the Render Output file; `width` x `height` override the output size for this render." { path: Option<String>, width: Option<u32>, height: Option<u32> };
    RenderIterative => "Render Iterative", MainUI, "Render Iterative: render into the Rendered Frame Window without saving a file; Region and Selected keep the rest of the window's image.";
    RenderLast => "Render Last", MainUI, "Render Last (F9): repeat the last render (same viewport and area), without saving a file.";
    RenderSetup => "Render SetUp...", MainUI, "Render Setup > Common: output size (`width`, `height`, 1-8192; with Image Aspect locked one changes the other), `image_aspect` (sets Height), `pixel_aspect`, the two aspect locks, Options `render_hidden` / `force_two_sided`, Render Output `output` file (empty clears) and `save_file`; with no argument the dialog shows (F10)." { width: Option<u32>, height: Option<u32>, image_aspect: Option<f32>, pixel_aspect: Option<f32>, lock_image_aspect: Option<bool>, lock_pixel_aspect: Option<bool>, render_hidden: Option<bool>, force_two_sided: Option<bool>, save_file: Option<bool>, output: Option<String> };
    BevyRendererSettings => "Bevy Renderer Settings", Loom, "Render Setup > Renderer (Bevy): 4x MSAA `antialiasing`, `supersampling` 1-4 (samples per pixel side), key light `shadows`, `shadow_map_size` 512-8192, `background` and `ambient` colours (sRGB 0-1); omitted fields keep their value." { antialiasing: Option<bool>, supersampling: Option<u32>, shadows: Option<bool>, shadow_map_size: Option<u32>, background: Option<[f32; 3]>, ambient: Option<[f32; 3]> };
    AreaToRender => "Area to Render", Loom, "Rendered Frame Window > Area to Render: View, Selected, Region, Crop or Blowup, and the `region` rectangle [x0, y0, x1, y1] as fractions of the frame (y down) for Region / Crop, or the Blowup one (kept at the output's shape)." { area: Option<loom_scene::render::RenderArea>, region: Option<[f32; 4]> };
    EditRegion => "Edit Region", MainUI, "Rendered Frame Window > Edit Region: toggle editing the render region in the viewport (View and Selected switch to Region).";
    RenderViewport => "Render Viewport", Loom, "Rendered Frame Window / Render Setup: the `viewport` to render (index), Lock to Viewport (`lock`) and Production or Iterative (`iterative`)." { viewport: Option<u32>, lock: Option<bool>, iterative: Option<bool> };
    RenderedFrameWindow => "Rendered Frame Window", MainUI, "Open or close the Rendered Frame Window.";
    SaveRenderedImage => "Save Image", MainUI, "Rendered Frame Window > Save Image: write the rendered image to `path` (png, jpg, bmp, tga, exr by extension); without it the file dialog shows." { path: Option<String> };
    CopyRenderedImage => "Copy Image", MainUI, "Rendered Frame Window > Copy Image: put the image as displayed (channel toggles applied) on the clipboard.";
    CloneRenderedFrameWindow => "Clone Rendered Frame Window", MainUI, "Rendered Frame Window > Clone: a new window holding a copy of the image, to compare with the next render.";
    ClearRenderedFrameWindow => "Clear Rendered Frame Window", MainUI, "Rendered Frame Window > Clear: remove the image from the window.";
    RenderedFrameWindowChannels => "Rendered Frame Window Channels", Loom, "Rendered Frame Window channel toggles: Enable Red / Green / Blue Channel, Display Alpha Channel (`alpha`), Monochrome (`mono`); omitted ones keep their state." { red: Option<bool>, green: Option<bool>, blue: Option<bool>, alpha: Option<bool>, mono: Option<bool> };
    CancelRender => "Cancel Render", MainUI, "Rendering progress > Cancel: stop the render in progress; the buckets done so far stay in the window.";
    PauseRender => "Pause Render", MainUI, "Rendering progress > Pause / Resume.";
    MaterialEditorSlot => "Material Editor Slot", Loom, "Make sample slot `slot` (0-23) the active one." { slot: u32 };
    SetMaterialColor => "Set Material Color", Loom, "Blinn Basic Parameters colour of a slot's material (slot omitted = active): Ambient, Diffuse or Specular, 0-255 RGB; Ambient follows Diffuse while locked." { slot: Option<u32>, which: String, rgb: [u8; 3] };
    SetMaterialParameter => "Set Material Parameter", Loom, "A slot's material value by label: Specular Level, Glossiness, Soften, Self-Illumination, Opacity, 2-Sided, Wire, Faceted, Ambient/Diffuse Lock, Show Map in Viewport (booleans as 0/1)." { slot: Option<u32>, name: String, value: f64 };
    SetMaterialMap => "Set Material Map", Loom, "Maps rollout: put a bitmap in a channel (Diffuse Color, Specular Level, Opacity, Bump); no path clears it; amount 0-100 (Bump -999-999)." { slot: Option<u32>, map: String, path: Option<String>, amount: Option<f32> };
    RenameMaterial => "Rename Material", Loom, "Name a slot's material." { slot: Option<u32>, name: String };
    AssignMaterialToSelection => "Assign Material to Selection", Loom, "Assign the active slot's material to the selected objects.";
    GetMaterialFromObject => "Get Material From Object", Loom, "Pick Material From Object: put a node's material (node omitted = the selected one) into the active slot." { node: Option<u64> };
    ResetMaterial => "Reset Material", Loom, "Reset a slot's material to the default Standard material (objects using it change too)." { slot: Option<u32> };
    SetMaterialType => "Set Material Type", Loom, "Type button: make the material at the Material Editor's current level Standard, Multi/Sub-Object or Blend. For a compound type `keep_old` (Replace Material dialog: Keep old material as sub-material) makes the old material the first sub-material; otherwise it is discarded." { kind: String, keep_old: Option<bool> };
    MultiSubSetNumber => "Multi/Sub Set Number", Loom, "Multi/Sub-Object Basic Parameters > Set Number: the number of sub-materials (fewer removes from the end, more adds new Standard ones)." { number: u32 };
    MultiSubAdd => "Multi/Sub Add", Loom, "Multi/Sub-Object > Add: a new Standard sub-material with the next unused material ID.";
    MultiSubDelete => "Multi/Sub Delete", Loom, "Multi/Sub-Object > Delete: remove the sub-material at list position `index` (0-based)." { index: u32 };
    MultiSubSetEntry => "Multi/Sub Set Entry", Loom, "A Multi/Sub-Object list row (0-based `index`): its material ID, Name, On/Off and the colour swatch (the sub-material's Diffuse, 0-255 RGB)." { index: u32, id: Option<u32>, name: Option<String>, enabled: Option<bool>, color: Option<[u8; 3]> };
    MaterialEditorGoToSub => "Material Editor Go To Sub-Material", Loom, "Click a sub-material button (Multi/Sub-Object row `index`, Blend Material 1 = 0 / 2 = 1): edit that sub-material; an empty one gets a new Standard material." { index: u32 };
    MaterialEditorGoToMap => "Material Editor Go To Map", Loom, "Click a map button that holds a map: edit that map (Maps rollout channel, a Blend's Mask, or a map's Color #1 / #2 / #3 / Mix Amount)." { map: String };
    NewMap => "New Map", Loom, "Material/Map Browser > New for a map button at the current level: put a new Bitmap, Checker, Gradient, Mix or Noise map in slot `map` and go to its level." { map: String, kind: String };
    SetMapType => "Set Map Type", Loom, "Type button at a map level: replace the map by a new one of `kind`; `keep_old` (Replace Map dialog) keeps the old map as the new one's Color #1 map." { kind: String, keep_old: Option<bool> };
    MaterialEditorGoToParent => "Go to Parent", Loom, "Material Editor > Go to Parent: up one level in the active slot's material.";
    MaterialEditorGoForwardToSibling => "Go Forward to Sibling", Loom, "Material Editor > Go Forward to Sibling: the next map or sub-material of the same parent.";
    MaterialEditorNavigate => "Material Editor Navigate", Loom, "Material/Map Navigator: go to the level reached by `path` from the active slot's top material; a step is a sub-material list position (\"0\") or a map slot name (\"Diffuse Color\")." { path: Vec<String> };
    SetMapParameter => "Set Map Parameter", Loom, "A value of the map at the current level by label: Offset U/V, Tiling U/V, Mirror U/V, Tile U/V, Angle W, Map Channel, Blur, Apply, Crop, Place, U, V, W, H, Soften, Color 2 Position, Radial, Noise Type (0 Regular, 1 Fractal, 2 Turbulence), Size, Levels, Phase, High, Low, Mix Amount, \"<component> Map On\" (booleans as 0/1)." { name: String, value: f64 };
    SetMapColor => "Set Map Color", Loom, "Color #1 / #2 / #3 of the map at the current level, 0-255 RGB." { which: String, rgb: [u8; 3] };
    SwapMapColors => "Swap Map Colors", Loom, "Checker / Noise / Mix > Swap: Color #1 and #2 (with their maps) trade places.";
    SetMapBitmap => "Set Map Bitmap", Loom, "Bitmap Parameters > Bitmap: the image file of the Bitmap map at the current level." { path: String };
    ReloadBitmaps => "Reload Bitmaps", Loom, "Bitmap Parameters > Reload: read the image files again.";
    SetMaterialMapAmount => "Set Material Map Amount", Loom, "Maps rollout Amount of channel `map` at the current material level, whatever the map's type: 0-100 (Bump -999-999)." { map: String, amount: f32 };
    SetMaterialMapEnabled => "Set Material Map Enabled", Loom, "Maps rollout check box of channel `map` at the current material level." { map: String, enabled: bool };
    CopyMap => "Copy Map", Loom, "Drag a map button onto another at the current level: Copy (Instance) Map dialog Copy, or Swap with `swap`." { from: String, to: String, swap: bool };
    CopyMaterialToSlot => "Copy Material To Slot", Loom, "Drag a sample slot onto another: the target slot gets a copy of the material (a new, unassigned material)." { from: u32, to: u32 };
    AssignMaterialToNode => "Assign Material To Node", Loom, "Drag a sample slot (omitted = the active one) onto an object in a viewport: assign its material to that object." { node: u64, slot: Option<u32> };
    AssignMaterialToFaces => "Assign Material To Faces", Loom, "Drag a sample slot (omitted = active) onto selected polygons: builds or extends a Multi/Sub-Object material on the object as the reference does and gives the faces the sub-material's ID." { slot: Option<u32> };
    DropBitmap => "Drop Bitmap", Loom, "Drop an image file on an object in a viewport: a new Standard material with it as the Diffuse Color map (Show Map in Viewport on) goes into the first unused sample slot and onto the object's selected polygons (building a Multi/Sub-Object material), or onto the whole object when none are selected; one undo step. `node` omitted = the selected object." { path: String, node: Option<u64> };
    GetMaterial => "Get Material", Loom, "Material/Map Browser (Browse From Scene / Mtl Editor): put material `material` into the active slot." { material: u32 };
    SetMaterialEditorOption => "Set Material Editor Option", Loom, "Material Editor Options / toolbar: Top Light Multiplier, Back Light Multiplier, Background Intensity, Backlight, Background, Sample UV Tiling (1-4), Slots (6, 15, 24), Antialias, Simple Multi Display Below Top Level, Display Maps as 2D (booleans as 0/1)." { name: String, value: f64 };
    SetSampleType => "Set Sample Type", Loom, "Sample Type flyout: Sphere, Cylinder or Cube for the active slot." { kind: String };
    PickMaterialFromObject => "Pick Material From Object", Loom, "Material Editor eyedropper: the next clicked object's material goes into the active slot (its sub-material when its selected polygons all use one ID).";
    MaterialMapNavigator => "Material/Map Navigator", Loom, "Open or close the Material/Map Navigator.";
    SetKeySelectionList => "Set Key Selection List", Loom, "The Selection List beside Auto Key / Set Key: which objects Set Keys keys (a named selection set; none = Selected)." { name: Option<String> };
    SetKeySelected => "Set Key Selected", MainUI, "Key the filtered tracks of the selected objects at the current frame.";
    ClearSetKeyModeBuffer => "Clear Set Key Mode Buffer", MainUI, "Discard the Set Key mode pose.";
    KeyAll => "Key All", MainUI, "Key every track of the selection at the current frame.";
    KeyPosition => "Key Position", MainUI, "Key the selection's Position at the current frame.";
    KeyPositionX => "Key Position X", MainUI, "Key the selection's X Position at the current frame.";
    KeyPositionY => "Key Position Y", MainUI, "Key the selection's Y Position at the current frame.";
    KeyPositionZ => "Key Position Z", MainUI, "Key the selection's Z Position at the current frame.";
    KeyRotation => "Key Rotation", MainUI, "Key the selection's Rotation at the current frame.";
    KeyRotationX => "Key Rotation X", MainUI, "Key the selection's X Rotation at the current frame.";
    KeyRotationY => "Key Rotation Y", MainUI, "Key the selection's Y Rotation at the current frame.";
    KeyRotationZ => "Key Rotation Z", MainUI, "Key the selection's Z Rotation at the current frame.";
    KeyScale => "Key Scale", MainUI, "Key the selection's Scale at the current frame.";
    KeyScaleX => "Key Scale X", MainUI, "Key the selection's X Scale at the current frame.";
    KeyScaleY => "Key Scale Y", MainUI, "Key the selection's Y Scale at the current frame.";
    KeyScaleZ => "Key Scale Z", MainUI, "Key the selection's Z Scale at the current frame.";
    KeyPositionRotationScale => "Key Position, Rotation, Scale", MainUI, "Key the selection's Position, Rotation and Scale at the current frame.";
    CreateKey => "Create Key", MainUI, "Track bar Create Key: copy the transform at Source Time to keys at Destination Time (both default to the current frame)." { source: Option<i32>, destination: Option<i32>, position: Option<bool>, rotation: Option<bool>, scale: Option<bool> };
    CreatePositionLockKey => "Create Position Lock Key", MainUI, "Key Position at the current frame with flat tangents, so it does not drift.";
    CreateRotationLockKey => "Create Rotation Lock Key", MainUI, "Key Rotation at the current frame with flat tangents, so it does not drift.";
    SetKeyAllFilter => "Set Key All Filter", MainUI, "Key Filters: All (turns every filter on, or all off when all are on).";
    SetKeyPositionFilter => "Set Key Position Filter", MainUI, "Key Filters: Position.";
    SetKeyRotationFilter => "Set Key Rotation Filter", MainUI, "Key Filters: Rotation.";
    SetKeyScaleFilter => "Set Key Scale Filter", MainUI, "Key Filters: Scale.";
    SetKeyIkParametersFilter => "Set Key IK Parameters Filter", MainUI, "Key Filters: IK Parameters.";
    SetKeyObjectParamsFilter => "Set Key Object Params Filter", MainUI, "Key Filters: Object Parameters.";
    SetKeyCustomAttributesFilter => "Set Key Custom Attributes Filter", MainUI, "Key Filters: Custom Attributes.";
    SetKeyModifiersFilter => "Set Key Modifiers Filter", MainUI, "Key Filters: Modifiers.";
    SetKeyMaterialsFilter => "Set Key Materials Filter", MainUI, "Key Filters: Materials.";
    SetKeyOtherFilter => "Set Key Other Filter", MainUI, "Key Filters: Other.";
    SetDefaultTangentAsAuto => "Set Default Tangent As Auto", MainUI, "New keys get Auto In/Out tangents.";
    SetDefaultTangentAsFast => "Set Default Tangent As Fast", MainUI, "New keys get Fast In/Out tangents.";
    SetDefaultTangentAsLinear => "Set Default Tangent As Linear", MainUI, "New keys get Linear In/Out tangents.";
    SetDefaultTangentAsSlow => "Set Default Tangent As Slow", MainUI, "New keys get Slow In/Out tangents.";
    SetDefaultTangentAsSmooth => "Set Default Tangent As Smooth", MainUI, "New keys get Smooth In/Out tangents.";
    SetDefaultTangentAsSpline => "Set Default Tangent As Spline", MainUI, "New keys get Custom (spline handle) In/Out tangents.";
    SetDefaultTangentAsStepped => "Set Default Tangent As Stepped", MainUI, "New keys get Step In/Out tangents.";
    SetDefaultTangents => "Set Default Tangents", Loom, "Default In / Out tangent picker: Auto, Smooth, Linear, Step, Fast, Slow or Custom; omitted sides keep their type." { tangent_in: Option<loom_scene::anim::TangentType>, tangent_out: Option<loom_scene::anim::TangentType> };
    PlayAnimation => "Play Animation", MainUI, "Play / stop the animation (/).";
    NextPreviousKeyModeToggle => "Next/Previous Key Mode Toggle", MainUI, "Key Mode: the previous / next frame buttons (, and .) jump between the selection's keys.";
    GoToNextKey => "Go To Next Key", Loom, "Jump to the selection's next key (Shift+.).";
    GoToPreviousKey => "Go To Previous Key", Loom, "Jump to the selection's previous key (Shift+,).";
    DeleteKeys => "Delete Keys", Loom, "Track bar: delete the selection's keys between two frames (inclusive)." { from: i32, to: i32 };
    MoveKeysInTime => "Move Keys In Time", Loom, "Track bar drag: move the selection's keys between two frames by `delta` frames; keys they land on are replaced." { from: i32, to: i32, delta: i32 };
    CopyKeys => "Copy Keys", Loom, "Track bar Shift-drag: copy the selection's keys between two frames `delta` frames away." { from: i32, to: i32, delta: i32 };
    SetPlayback => "Set Playback", Loom, "Time Configuration > Playback: Real Time, Loop, Speed (0.25, 0.5, 1, 2, 4) and Direction (Forward, Reverse, PingPong, when Real Time is off)." { real_time: Option<bool>, loop_playback: Option<bool>, speed: Option<f32>, direction: Option<crate::anim_cmds::PlayDirection> };
    RescaleTime => "Re-scale Time", Loom, "Time Configuration > Re-scale Time: stretch every key of the scene from the active segment to the new one." { start: i32, end: i32 };
    TickPlayback => "Tick Playback", Loom, "Advance a playing animation by `seconds` of wall time (Real Time) or one frame." { seconds: f32 };
    SaveAnimation => "Save Animation", MainUI, "Save the selection's animated tracks to an animation clip (.lxaf); without `path` the dialog shows." { path: Option<String> };
    LoadAnimation => "Load Animation", MainUI, "Load an animation clip (.lxaf) onto the objects with the same names (within the selection when there is one), shifted by `offset` frames." { path: Option<String>, offset: Option<i32> };
    // --- Track View (chart group "Track View") ---
    AccessHierarchySelectNameField => "Access Hierarchy Select Name Field", TrackView, "Focus the track-selection name field (lower left).";
    AccessTimeField => "Access Time Field", TrackView, "Focus the selected key's time field.";
    AccessTrackNameField => "Access Track Name Field", TrackView, "Focus the Track View window-name field.";
    AccessValueField => "Access Value Field", TrackView, "Focus the selected key's value field.";
    AddKeys => "Add Keys", TrackView, "Insert Keys mode (A): clicking a curve or track adds a key.";
    AddNoteTrack => "Add Note Track", TrackView, "Add a note track under each highlighted item.";
    AddVisibilityTrack => "Add Visibility Track", TrackView, "Add a visibility track (Bezier Float, 1 = visible) to the highlighted objects.";
    AlignKeys => "Align Keys", TrackView, "Align to Cursor: move the leftmost selected key of each track to the time slider, the others keep their offsets.";
    AssignControllerTrackView => "Assign Controller", TrackView, "Open Assign Controller for the highlighted track (C).";
    AutoExpandAnimated => "Auto Expand Animated", TrackView, "Options > Auto Expand > Animated.";
    AutoExpandKeyable => "Auto Expand Keyable", TrackView, "Options > Auto Expand > Keyable.";
    BreakTangents => "Break Tangents", TrackView, "Let the In and Out handles of the selected keys move independently.";
    CollapseAllTrackView => "Collapse All (Track View)", TrackView, "Collapse every branch under the highlighted items.";
    CollapseObjects => "Collapse Objects", TrackView, "Collapse the object branches under the highlighted items.";
    CollapseTracks => "Collapse Tracks", TrackView, "Collapse the track branches of the highlighted items.";
    CopyController => "Copy Controller", TrackView, "Copy the highlighted track's controller (Ctrl+C).";
    CopyTime => "Copy Time", TrackView, "Dope Sheet: copy the selected block of time from the highlighted tracks.";
    CutTime => "Cut Time", TrackView, "Dope Sheet: cut the selected block of time; later keys move back.";
    DeleteController => "Delete Controller", TrackView, "Delete a deletable controller: a visibility track or a List entry.";
    DeleteNoteTrack => "Delete Note Track", TrackView, "Remove the highlighted note tracks and their keys.";
    DeleteVisibilityTrack => "Delete Visibility Track", TrackView, "Remove the highlighted objects' visibility tracks.";
    DrawCurves => "Draw Curves", TrackView, "Draw Curves mode: sketch a curve on the highlighted track.";
    EditKeysMode => "Edit Keys Mode", TrackView, "Dope Sheet: Edit Keys.";
    EditRangesMode => "Edit Ranges Mode", TrackView, "Dope Sheet: Edit Ranges (range bars move or scale whole tracks).";
    EditTimeMode => "Edit Time Mode", TrackView, "Dope Sheet: Edit Time (work on selected blocks of time).";
    EditTrackSet => "Edit Track Set", TrackView, "Open the Track Sets Editor.";
    ExcludeLeftEndPointToggle => "Exclude Left End Point Toggle", TrackView, "Leave a key at the start of the time block out of Copy / Cut Time.";
    ExcludeRightEndPointToggle => "Exclude Right End Point Toggle", TrackView, "Leave a key at the end of the time block out of Copy / Cut Time.";
    ExpandAll => "Expand All", TrackView, "Expand every branch under the highlighted items.";
    ExpandObjectToggle => "Expand Object Toggle", TrackView, "Expand / collapse the highlighted object (O).";
    ExpandObjects => "Expand Objects", TrackView, "Expand the object branches under the highlighted items.";
    ExpandTrackToggle => "Expand Track Toggle", TrackView, "Expand / collapse the highlighted track (Enter, T).";
    ExpandTracks => "Expand Tracks", TrackView, "Expand the track branches of the highlighted items.";
    FilterAnimatedTracksToggle => "Filter - Animated Tracks Toggle", TrackView, "Show only animated tracks.";
    FilterKeyableTracksToggle => "Filter - Keyable Tracks Toggle", TrackView, "Show only keyable tracks.";
    FilterSelectedObjectsToggle => "Filter - Selected Objects Toggle", TrackView, "Show only the scene selection's tracks.";
    FilterSelectedTracksToggle => "Filter - Selected Tracks Toggle", TrackView, "Show only the tracks highlighted now.";
    FilterUnlockedAttributesToggle => "Filter - Unlocked Attributes Toggle", TrackView, "Show only unlocked tracks.";
    FilterVisibleObjectsToggle => "Filter - Visible Objects Toggle", TrackView, "Leave hidden objects out of the hierarchy.";
    Filters => "Filters", TrackView, "Open the Filters dialog (Q).";
    FrameHorizontalExtents => "Frame Horizontal Extents", TrackView, "Zoom Horizontal Extents: fit the active time segment.";
    FrameHorizontalExtentsKeys => "Frame Horizontal Extents Keys", TrackView, "Zoom Horizontal Extents Keys: fit every key (Alt+X).";
    FrameValueExtents => "Frame Value Extents", TrackView, "Zoom Value Extents: fit the displayed curves vertically.";
    FreezeNonselectedCurves => "Freeze Nonselected Curves", TrackView, "Draw non-highlighted curves dashed and uneditable.";
    FunctionCurvesMode => "Function Curves Mode", TrackView, "Switch Track View to the Curve Editor.";
    InsertTime => "Insert Time", TrackView, "Insert Time mode: drag to add time; later keys slide away.";
    IsolateCurve => "Isolate Curve", TrackView, "Show only the curves with selected keys.";
    KeyablePropertyToggle => "Keyable Property Toggle", TrackView, "Toggle whether the highlighted tracks can be keyed by Set Keys.";
    LockSelection => "Lock Selection", TrackView, "Lock the key selection (Space).";
    LockTangentsToggle => "Lock Tangents Toggle", TrackView, "Dragging one handle changes every selected key's handles (L).";
    Locked => "Locked", TrackView, "Lock / unlock the highlighted tracks: locked tracks reject key edits.";
    MakeControllerUnique => "Make Controller Unique", TrackView, "Make an instanced controller (modifier) unique (U).";
    ModifySubtreeToggle => "Modify Subtree Toggle", TrackView, "Editing a parent item edits the tracks under it.";
    MoveHighlightDown => "Move Highlight Down", TrackView, "Highlight the next hierarchy row (Down Arrow).";
    MoveHighlightUp => "Move Highlight Up", TrackView, "Highlight the previous hierarchy row (Up Arrow).";
    MoveKeys => "Move Keys", TrackView, "Move Keys mode (M).";
    MoveKeysHorizontal => "Move Keys Horizontal", TrackView, "Move Keys constrained to time.";
    MoveKeysVertical => "Move Keys Vertical", TrackView, "Move Keys constrained to value.";
    MoveObjectDown => "Move Object Down", TrackView, "Move the highlighted object down in the hierarchy.";
    MoveObjectUp => "Move Object Up", TrackView, "Move the highlighted object up in the hierarchy.";
    NudgeKeysLeft => "Nudge Keys Left", TrackView, "Move the selected keys one frame earlier (Left Arrow).";
    NudgeKeysRight => "Nudge Keys Right", TrackView, "Move the selected keys one frame later (Right Arrow).";
    PanTrackView => "Pan (Track View)", TrackView, "Pan mode for the key window (Ctrl+P).";
    ParameterCurveOutOfRangeTypes => "Parameter Curve Out-of-Range Types", TrackView, "Open the out-of-range types dialog for the highlighted tracks.";
    PasteController => "Paste Controller", TrackView, "Paste the copied controller onto highlighted tracks of its type (Ctrl+V).";
    PasteTime => "Paste Time", TrackView, "Dope Sheet: paste the copied time at the time selection (absolute).";
    PropertiesTrackView => "Properties", TrackView, "Open Key Info for the selected keys.";
    RegionKeysTool => "Region Keys Tool", TrackView, "Region Tool: a rectangle that moves or scales the enclosed keys.";
    ReverseTime => "Reverse Time", TrackView, "Dope Sheet: reverse the keys inside the time selection.";
    ScaleKeys => "Scale Keys", TrackView, "Scale Keys - Time mode (about the time slider).";
    ScaleTime => "Scale Time", TrackView, "Dope Sheet: Scale Time mode (the block scales from its start).";
    ScaleValues => "Scale Values", TrackView, "Scale Values mode (about the scale origin line).";
    ScrollDown => "Scroll Down", TrackView, "Scroll the hierarchy down (Ctrl+Down Arrow).";
    ScrollUp => "Scroll Up", TrackView, "Scroll the hierarchy up (Ctrl+Up Arrow).";
    SelectAllTrackView => "Select All (Track View)", TrackView, "Highlight every visible hierarchy item.";
    SelectChildrenTrackView => "Select Children (Track View)", TrackView, "Highlight the children of the highlighted items.";
    SelectInvertTrackView => "Select Invert (Track View)", TrackView, "Invert the hierarchy highlight.";
    SelectNoneTrackView => "Select None (Track View)", TrackView, "Clear the hierarchy highlight.";
    SelectTime => "Select Time", TrackView, "Dope Sheet: Select Time mode.";
    SetTangentsToAuto => "Set Tangents to Auto", TrackView, "Selected keys: Auto tangents.";
    SetTangentsToFast => "Set Tangents to Fast", TrackView, "Selected keys: Fast tangents.";
    SetTangentsToLinear => "Set Tangents to Linear", TrackView, "Selected keys: Linear tangents.";
    SetTangentsToSlow => "Set Tangents to Slow", TrackView, "Selected keys: Slow tangents.";
    SetTangentsToSmooth => "Set Tangents to Smooth", TrackView, "Selected keys: Smooth tangents.";
    SetTangentsToSpline => "Set Tangents to Spline", TrackView, "Selected keys: Custom (spline) tangents with editable handles.";
    SetTangentsToStepped => "Set Tangents to Stepped", TrackView, "Selected keys: Step tangents.";
    ShowKeysOnFrozenCurves => "Show Keys on Frozen Curves", TrackView, "Draw key markers on frozen curves.";
    ShowSelectedKeyStats => "Show Selected Key Stats", TrackView, "Draw frame and value next to each selected key.";
    ShowTangentsToggle => "Show Tangents Toggle", TrackView, "Show or hide the tangent handles.";
    SimplifyCurve => "Simplify Curve", TrackView, "Open Reduce Keys for the highlighted tracks.";
    SlideKeys => "Slide Keys", TrackView, "Slide Keys mode: the selection and every key toward the drag move.";
    SnapFrames => "Snap Frames", TrackView, "Moved and scaled keys land on whole frames (S).";
    TrackViewUtilities => "Track View Utilities", TrackView, "Open Track View Utilities.";
    UnifyTangents => "Unify Tangents", TrackView, "Rejoin the broken handles of the selected keys.";
    UnlockedAttributesToggle => "Unlocked Attributes Toggle", TrackView, "Show only unlocked tracks (as Filter - Unlocked Attributes Toggle).";
    ViewAllTangents => "View All Tangents", TrackView, "Show the handles of every key, not only the selected ones.";
    Zoom => "Zoom", TrackView, "Zoom mode: time and value (Alt+Z).";
    ZoomRegion => "Zoom Region", TrackView, "Zoom Region mode (Ctrl+W).";
    ZoomSelectedObject => "Zoom Selected Object", TrackView, "Scroll the hierarchy to the selected object.";
    ZoomTime => "Zoom Time", TrackView, "Zoom mode: time only.";
    ZoomValues => "Zoom Values", TrackView, "Zoom mode: values only (Curve Editor).";
    // --- Track View window and agent API ---
    TrackViewFloater => "Track View Floater", MainUI, "Open the Track View (Curve Editor).";
    OpenCurveEditor => "Open Curve Editor", Loom, "Graph Editors > Track View - Curve Editor.";
    OpenDopeSheet => "Open Dope Sheet", Loom, "Graph Editors > Track View - Dope Sheet.";
    CloseTrackView => "Close Track View", Loom, "Close the Track View window.";
    TrackViewHighlight => "Track View Highlight", Loom, "Highlight hierarchy items by id: \"12\" (object), \"12:Transform\" (group), \"12:Position\" (track by name), \"12:note:0\"." { items: Vec<String>, op: SelectOp };
    TrackViewSelectKeys => "Track View Select Keys", Loom, "Select keys by node, track name, component (0 X, 1 Y, 2 Z) and frame." { keys: Vec<crate::track_view::KeyArg>, op: SelectOp };
    TrackViewSelectKeysInRange => "Track View Select Keys In Range", Loom, "Region-select the highlighted tracks' keys between two frames (and values)." { start: f32, end: f32, low: Option<f32>, high: Option<f32>, op: SelectOp };
    TrackViewInsertKey => "Track View Insert Key", Loom, "Insert Keys: a key on a track at a frame (value defaults to the curve's)." { id: u64, track: String, time: f32, value: Option<f32>, comp: Option<u32> };
    TrackViewMoveKeys => "Track View Move Keys", Loom, "Move the selected keys by frames / value (Snap Frames, Soft Selection apply)." { dt: f32, dv: f32 };
    TrackViewSlideKeys => "Track View Slide Keys", Loom, "Slide the selected keys and every key toward the move." { dt: f32 };
    TrackViewScaleKeys => "Track View Scale Keys", Loom, "Scale the selected keys in time about `origin` (default the time slider)." { factor: f32, origin: Option<f32> };
    TrackViewScaleValues => "Track View Scale Values", Loom, "Scale the selected keys' values about `origin`." { factor: f32, origin: f32 };
    TrackViewDeleteKeys => "Track View Delete Keys", Loom, "Delete the selected keys (Delete).";
    TrackViewSetTangents => "Track View Set Tangents", Loom, "Set the selected keys' tangents; `side` \"in\", \"out\" or both." { tangent: loom_scene::anim::TangentType, side: Option<String> };
    TrackViewSetKeyValues => "Track View Set Key Values", Loom, "Key Stats / Current Value Editor: type the selected keys' time and value, absolute or relative." { time: Option<f32>, value: Option<f32>, relative: Option<bool> };
    TrackViewSetKeyInfo => "Track View Set Key Info", Loom, "Key Info (Basic / Advanced): tangent types and slopes, TCB tension / continuity / bias / ease (0-50)." { in_type: Option<loom_scene::anim::TangentType>, out_type: Option<loom_scene::anim::TangentType>, in_tan: Option<f32>, out_tan: Option<f32>, tension: Option<f32>, continuity: Option<f32>, bias: Option<f32>, ease_to: Option<f32>, ease_from: Option<f32> };
    TrackViewNormalizeTime => "Track View Normalize Time", Loom, "Key Info > Advanced > Normalize Time: space the selected keys of each track evenly between the first and last.";
    TrackViewSetConstantVelocity => "Track View Set Constant Velocity", Loom, "Key Info > Advanced > Constant Velocity of the selected keys." { on: bool };
    TrackViewSetOrt => "Track View Set Out-of-Range Types", Loom, "Parameter Curve Out-of-Range Types of the highlighted tracks." { before: loom_scene::anim::Ort, after: loom_scene::anim::Ort };
    TrackViewReduceKeys => "Track View Reduce Keys", Loom, "Reduce Keys on the highlighted tracks within `threshold`." { threshold: f32 };
    TrackViewDrawCurve => "Track View Draw Curve", Loom, "Draw Curves: keys along [frame, value] points, replacing the keys in that span." { id: u64, track: String, comp: u32, points: Vec<[f32; 2]> };
    TrackViewSelectTime => "Track View Select Time", Loom, "Dope Sheet: select a block of time." { start: f32, end: f32 };
    TrackViewInsertTime => "Track View Insert Time", Loom, "Insert `amount` frames at `at` in the highlighted tracks." { at: f32, amount: f32 };
    TrackViewScaleTime => "Track View Scale Time", Loom, "Scale the time selection from its start; later keys follow." { factor: f32 };
    TrackViewDeleteTime => "Track View Delete Time", Loom, "Delete the time selection and its keys (later keys move back).";
    TrackViewPasteTime => "Track View Paste Time", Loom, "Paste copied time at `at`, Absolute or Relative." { at: Option<f32>, relative: Option<bool> };
    TrackViewMoveRange => "Track View Move Range", Loom, "Edit Ranges: move the highlighted tracks' range bars (all their keys)." { dt: f32 };
    TrackViewScaleRange => "Track View Scale Range", Loom, "Edit Ranges: scale the highlighted tracks' range bars about `origin`." { factor: f32, origin: f32 };
    AddNoteKey => "Add Note Key", Loom, "A note key on a note track." { note: u32, time: f32, text: String };
    SetNoteKey => "Set Note Key", Loom, "Notes dialog: a note key's text, time or Lock Key." { note: u32, index: u32, text: Option<String>, time: Option<f32>, locked: Option<bool> };
    DeleteNoteKey => "Delete Note Key", Loom, "Delete a note key." { note: u32, index: u32 };
    TrackSetCreate => "Track Set Create", Loom, "Store the highlighted tracks as a named track set." { name: String };
    TrackSetDelete => "Track Set Delete", Loom, "Delete a track set." { name: String };
    TrackSetSelect => "Track Set Select", Loom, "Highlight a track set's tracks (Add / Subtract like Ctrl / Alt)." { name: String, op: SelectOp };
    TrackSetAddHighlighted => "Track Set Add Highlighted", Loom, "Track Sets Editor: add the highlighted tracks to a set." { name: String };
    TrackSetRemoveHighlighted => "Track Set Remove Highlighted", Loom, "Track Sets Editor: remove the highlighted tracks from a set." { name: String };
    TrackViewFilter => "Track View Filter", Loom, "Filters dialog: Transforms, Base Objects, Modified Objects, Note Tracks, Visibility Tracks, Controller Types, Animated Tracks, Selected Objects, Selected Tracks, Visible Objects, Keyable Tracks, Unlocked Attributes." { name: String, on: bool };
    TrackViewOption => "Track View Option", Loom, "Options menu: Interactive Update, Sync Cursor Time, Manual Navigation, Hide / Show Non-Selected Curves, Use Soft Select, Auto Expand Animated / Keyable, Modify Subtree." { name: String, on: bool };
    TrackViewSoftSelection => "Track View Soft Selection", Loom, "Soft Selection Settings: Soft, Range (frames), Falloff." { soft: Option<bool>, range: Option<f32>, falloff: Option<f32> };
    TrackViewSetView => "Track View Set View", Loom, "The key window's visible time and value range." { start: f32, end: f32, low: f32, high: f32 };
    RandomizeKeys => "Randomize Keys", Loom, "Track View Utilities > Randomize Keys: random +/- time and value offsets of the selected keys." { time: f32, value: f32, seed: u32 };
    CreateOutOfRangeKeys => "Create Out of Range Keys", Loom, "Track View Utilities: bake the out-of-range motion of the highlighted tracks into keys." { before: i32, after: i32, samples: u32 };
    SelectKeysByTime => "Select Keys By Time", Loom, "Track View Utilities: select the highlighted tracks' keys between two frames." { start: f32, end: f32, clear: bool };
    EulerFilter => "Euler Filter", Loom, "Track View Utilities: remove ±360° flips from the highlighted Euler rotation tracks." { start: Option<i32>, end: Option<i32> };
    CollapseController => "Collapse Controller", Loom, "Controller > Collapse: bake the highlighted tracks to keys every `samples` frames." { start: Option<i32>, end: Option<i32>, samples: Option<u32> };
    SetTrackBarFilter => "Set Track Bar Filter", Loom, "Track bar right-click > Filter: All Keys, All Transform Keys, Current Transform, Object, Material." { filter: crate::anim_cmds::TrackBarFilter };
    ShowTrackBarToggle => "Show Track Bar Toggle", MainUI, "Show or hide the track bar under the time slider.";
    ShowTrajectoriesToggle => "Show Trajectories Toggle", MainUI, "Draw the selection's motion paths in the viewports.";
    DeleteSelectedAnimation => "Delete Selected Animation", MainUI, "Animation > Delete Selected Animation: every animated track of the selection becomes a constant at the current frame.";
    StartConstraintPick => "Start Constraint Pick", Loom, "Animation > Constraints > … / Motion panel Add Target: the next clicked object becomes the target (Path, Position, Attachment, LookAt, Orientation or Link Constraint)." { kind: String };
    TrajectoryConvertTo => "Trajectory Convert To", Loom, "Motion > Trajectories > Convert To: a spline through the selection's path, one knot per sample." { start: Option<i32>, end: Option<i32>, samples: Option<u32> };
    TrajectoryConvertFrom => "Trajectory Convert From", Loom, "Motion > Trajectories > Convert From: position keys for the selection from a spline." { shape: u64, start: Option<i32>, end: Option<i32>, samples: Option<u32> };
    TrajectoryCollapse => "Trajectory Collapse", Loom, "Motion > Trajectories > Collapse: bake the selection's transform (constraints, controllers) to keys." { start: Option<i32>, end: Option<i32>, samples: Option<u32>, position: Option<bool>, rotation: Option<bool>, scale: Option<bool> };
    TrajectorySubObjectKeys => "Trajectory Sub-Object Keys", Loom, "Motion > Trajectories > Sub-Object Keys: edit position keys on the drawn path.";
    TrajectorySelectKeys => "Trajectory Select Keys", Loom, "Sub-Object Keys: select trajectory keys by frame." { frames: Vec<i32>, op: SelectOp };
    TrajectoryMoveKeys => "Trajectory Move Keys", Loom, "Sub-Object Keys: move the selected trajectory keys by a world offset." { delta: [f32; 3] };
    // --- Edit Normals (chart group "Edit Normals") ---
    AverageMode => "Average Mode", EditNormals, "Edit Normals > Average: Use Threshold toggle.";
    AverageNormals => "Average Normals", EditNormals, "Edit Normals > Average Selected: the selected normals take their mean direction.";
    BreakNormals => "Break Normals", EditNormals, "Edit Normals > Break: each corner of the selected normals gets its own normal (B).";
    CopyNormal => "Copy Normal", EditNormals, "Edit Normals > Copy Value (Ctrl+C).";
    EdgeLevelEditNormals => "Edge Level (Edit Normals)", EditNormals, "Edit Normals > Select By Edge (Ctrl+3).";
    FaceLevelEditNormals => "Face Level (Edit Normals)", EditNormals, "Edit Normals > Select By Face (Ctrl+4).";
    MakeExplicit => "Make Explicit", EditNormals, "Edit Normals > Make Explicit: the selected normals keep their direction (E).";
    NormalLevel => "Normal Level", EditNormals, "Edit Normals > Normal sub-object level, Select By Normal (Ctrl+1).";
    ObjectLevelEditNormals => "Object Level (Edit Normals)", EditNormals, "Edit Normals: leave the Normal level (Ctrl+0).";
    PasteNormal => "Paste Normal", EditNormals, "Edit Normals > Paste Value onto the selected normals (Ctrl+V).";
    ResetNormals => "Reset Normals", EditNormals, "Edit Normals > Reset: the selected normals follow the smoothing groups again (R).";
    SpecifyNormals => "Specify Normals", EditNormals, "Edit Normals > Specify: the selected normals average only their own faces (S).";
    UnifyNormals => "Unify Normals", EditNormals, "Edit Normals > Unify: the selected normals at each vertex become one (U).";
    VertexLevelEditNormals => "Vertex Level (Edit Normals)", EditNormals, "Edit Normals > Select By Vertex (Ctrl+2).";
    EditNormalsSelect => "Edit Normals Select", Loom, "Edit Normals: select normals by id under Select By (normal ids, vertex ids, edge ids or face ids)." { ids: Vec<u32>, op: SelectOp };
    EditNormalsSelectBy => "Edit Normals Select By", Loom, "Edit Normals > Select By: 0 Normal, 1 Vertex, 2 Edge, 3 Face." { level: u32 };
    EditNormalsSetNormal => "Edit Normals Set Normal", Loom, "Edit Normals: the selected normals point along `direction` (explicit)." { direction: [f32; 3] };
    EditNormalsRotate => "Edit Normals Rotate", Loom, "Edit Normals: rotate the selected normals about a world axis (explicit)." { axis: [f32; 3], degrees: f32 };
    SplineSelectSet => "Spline Select Set", Loom, "Spline Select: select knots (level 1, flat index), segments (2) or splines (3) of the active Spline Select entry." { level: u32, ids: Vec<u32>, op: SelectOp };
    BindSpline => "Bind (Spline)", MainUI, "Editable Spline > Bind: click a segment; the selected end vertices sit on its middle and follow it.";
    UnbindSpline => "Unbind (Spline)", MainUI, "Editable Spline > Unbind: the selected vertices are free again.";
    TrimSpline => "Trim (Spline)", MainUI, "Editable Spline > Trim: click a piece of a spline to cut it back to the crossings.";
    ExtendSpline => "Extend (Spline)", MainUI, "Editable Spline > Extend: click near an open end to lengthen it to the next spline it meets.";
    CrossInsertSpline => "Cross Insert (Spline)", MainUI, "Editable Spline > Cross Insert: click where two splines cross to add a vertex on both.";
    EditSoftSelection => "Edit Soft Selection", MainUI, "Soft Selection > Edit: drag in the viewport to set Falloff (Ctrl: Pinch, Shift: Bubble).";
    SplineTrim => "Spline Trim", Loom, "Editable Spline: trim the piece nearest `point` (world) back to its crossings." { point: [f32; 3] };
    SplineExtend => "Spline Extend", Loom, "Editable Spline: extend the open end nearest `point` (world) to the next spline it meets." { point: [f32; 3] };
    SplineCrossInsert => "Spline Cross Insert", Loom, "Editable Spline: a vertex on both splines where they cross within `threshold` height: the crossing nearest world `point`, or every crossing when `point` is absent." { threshold: f32, point: Option<[f32; 3]> };
    SplineBind => "Spline Bind", Loom, "Editable Spline: bind end vertex `knot` (flat) to segment `segment` (flat)." { knot: u32, segment: u32 };
    SplineCrossSection => "Spline Cross Section", Loom, "Editable Spline > Cross Section: splines through the vertices of the listed splines in order (kind 0 Linear, 1 Smooth, 2 Bezier, 3 Bezier Corner)." { splines: Vec<u32>, kind: u32 };
    SplineTangentCopy => "Spline Tangent Copy", Loom, "Editable Spline > Tangent Copy: copy the in (side_in) or out handle of flat knot `knot`." { knot: u32, side_in: bool };
    SplineTangentPaste => "Spline Tangent Paste", Loom, "Editable Spline > Tangent Paste: paste the copied handle's angle onto flat knot `knot` (`length` = Paste Length: its length too)." { knot: u32, side_in: bool, length: bool };
    HsdsSubdivide => "HSDS Subdivide", Loom, "HSDS > Subdivide: the polygons `faces` of the finest level (null = the current polygon selection) split into a new level." { faces: Option<Vec<u32>> };
    HsdsAdaptiveSubdivide => "HSDS Adaptive Subdivide", Loom, "HSDS > Adaptive Subdivision: up to `max_level` new levels, each splitting the polygons with an edge sharper than `max_angle` degrees." { max_level: u32, max_angle: f32 };
    HsdsSelectLevel => "HSDS Select Level", Loom, "HSDS > level of detail list: edit level `level` (0 = Base Level); the sub-object levels pick on its polygons." { level: u32 };
    HsdsDisplay => "HSDS Display", Loom, "HSDS > Display next to a level: the result shows only up to `level` (0 = Base Level; the finest level shows every level)." { level: u32 };
    PointCacheRecord => "Point Cache Record", Loom, "Point Cache > Record: the stack below entry `index` from frame `start` to `end` into the .pc2 file `path` (world positions for the WSM), then plays it." { index: u32, path: String, start: i32, end: i32 };
    PointCacheLoad => "Point Cache Load", Loom, "Point Cache > Load: play back the .pc2 file `path` on entry `index`." { index: u32, path: String };
    PointCacheUnload => "Point Cache Unload", Loom, "Point Cache > Unload: entry `index` stops playing its file." { index: u32 };
    SplineIkControlCreateHelpers => "Spline IK Control Create Helpers", Loom, "Spline IK Control > Create Helpers: a Point helper at every knot, linked by Link Types; the knots follow them.";
    FilletChamferApply => "Fillet/Chamfer Apply", Loom, "Fillet/Chamfer modifier: fillet (Radius) or chamfer (Distance) its selected vertices." { fillet: bool };
    TrimExtendPick => "Trim/Extend Pick", Loom, "Trim/Extend modifier: trim (or `extend`) the spline piece nearest `point` (world)." { point: [f32; 3], extend: bool };
    AssignController => "Assign Track Controller", Loom, "Assign Controller on one track (`track` = its name, e.g. \"Position\", \"Height\", \"Bend / Angle\"): Position XYZ, Bezier / TCB / Linear / Noise Position, Position List, Path / Position Constraint, Attachment; Euler XYZ, TCB Rotation, LookAt / Orientation Constraint…; Bezier / TCB / Linear / Noise Float, Float List." { id: u64, track: String, controller: String };
    ListAddController => "List Add Controller", Loom, "List controller: add a controller of type `controller` to the Available slot." { id: u64, track: String, controller: String };
    ListSetWeight => "List Set Weight", Loom, "List controller: an entry's weight (percent)." { id: u64, track: String, index: u32, weight: f32 };
    ListSetActive => "List Set Active", Loom, "List controller: Set Active — the entry edits go to." { id: u64, track: String, index: u32 };
    ListDeleteController => "List Delete Controller", Loom, "List controller: delete an entry (the last one stays)." { id: u64, track: String, index: u32 };
    PathConstraint => "Path Constraint", Loom, "Animation > Constraints > Path Constraint: the selection follows the shape `target` (% Along Path keyed 0 → 100 over the active segment; another path adds a weighted target)." { target: u64 };
    PositionConstraint => "Position Constraint", Loom, "Animation > Constraints > Position Constraint: add `target` (weight 50) to the selection's position targets." { target: u64 };
    AttachmentConstraint => "Attachment Constraint", Loom, "Animation > Constraints > Attachment Constraint: stick the selection to face `face` of `target` at barycentric A, B (Align to Surface on)." { target: u64, face: Option<u32>, a: Option<f32>, b: Option<f32> };
    LookAtConstraint => "LookAt Constraint", Loom, "Animation > Constraints > LookAt Constraint: aim the selection's X axis at `target` (more targets blend by weight)." { target: u64 };
    OrientationConstraint => "Orientation Constraint", Loom, "Animation > Constraints > Orientation Constraint: add `target` to the selection's orientation targets." { target: u64 };
    LinkConstraint => "Link Constraint", Loom, "Link Constraint > Add Link at the current frame: from here on the selection hangs from `target` (none = Link to World)." { target: Option<u64> };
    SetConstraintParameter => "Set Constraint Parameter", Loom, "A constraint rollout value: % Along Path, Follow, Bank, Bank Amount, Smoothness, Allow Upside Down, Constant Velocity, Loop, Relative, Axis (0 X, 1 Y, 2 Z), Flip, Keep Initial Offset, Align to Surface, Face, A, B." { id: u64, name: String, value: f64 };
    SetConstraintWeight => "Set Constraint Weight", Loom, "A constraint target's weight (0-100); `rotation` picks the rotation constraint, else the position one." { id: u64, rotation: bool, index: u32, weight: f32 };
    DeleteConstraintTarget => "Delete Constraint Target", Loom, "Remove a target from the position (or `rotation`) constraint." { id: u64, rotation: bool, index: u32 };
    DeleteLink => "Delete Link", Loom, "Link Constraint: remove a link from the list." { id: u64, index: u32 };
    // --- Display ---
    HideSelection => "Hide Selection", MainUI, "Hide the selected objects.";
    HideUnselected => "Hide Unselected", MainUI, "Hide every unselected object.";
    UnhideAll => "Unhide All", MainUI, "Unhide every object.";
    FreezeSelection => "Freeze Selection", MainUI, "Freeze the selected objects.";
    FreezeUnselected => "Freeze Unselected", Loom, "Freeze every unselected object (Display > Freeze > Freeze Unselected).";
    UnfreezeAll => "Unfreeze All", MainUI, "Unfreeze every object.";
    // --- Loom (agent API, no UI action) ---
    State => "State", Loom, "Scene snapshot: nodes, selection, tool, view and camera.";
    CreatePrimitive => "Create Primitive", Loom, "Create a standard primitive at a base-centre position with a size (width X, length Y, height Z); radii derive from the size." { kind: PrimitiveKind, position: [f32; 3], size: [f32; 3] };
    CreateShape => "Create Shape", Loom, "Create > Shapes: a parametric shape at `position` (size: [width, length, height] of its bounds)." { kind: ShapeKind, position: [f32; 3], size: [f32; 3] };
    ConvertToEditableSpline => "Convert to Editable Spline", Loom, "Convert the selected shapes to Editable Splines (quad menu > Convert To).";
    LineStart => "Line Start", Loom, "Line tool: start a new Line with its first knot at `point` (world space)." { point: [f32; 3] };
    LineAddKnot => "Line Add Knot", Loom, "Line tool: add a knot at `point`; `smooth` makes a Smooth vertex, else a Corner." { node: u64, point: [f32; 3], smooth: bool };
    LineSetHandle => "Line Set Handle", Loom, "Line tool: dragging after a click makes the last knot Bezier with its out handle at `point`." { node: u64, point: [f32; 3] };
    LineSmoothLastKnot => "Line Smooth Last Knot", Loom, "Line tool: Drag Type Smooth makes the knot just added a Smooth vertex." { node: u64 };
    LineRemoveLastKnot => "Line Remove Last Knot", Loom, "Line tool: Backspace removes the last knot." { node: u64 };
    LineEnd => "Line End", Loom, "Line tool: the line ends; a line with fewer than two knots is removed." { node: u64 };
    LineClose => "Line Close", Loom, "Line tool: clicking the first knot closes the spline." { node: u64 };
    LineCloseAsk => "Line Close Ask", Loom, "Line tool: a click on the first knot asks \"Close spline?\"; `point`/`smooth` are the knot a No adds there." { node: u64, point: [f32; 3], smooth: bool };
    LineCloseReply => "Line Close Reply", Loom, "Answer \"Close spline?\": Yes closes the line (the Line tool then ends it), No adds the clicked knot and drawing goes on." { close: bool };
    CreateCamera => "Create Camera", Loom, "Create a camera at the active viewport's eye looking at its target (Target camera gets a target node)." { target: bool };
    CreateCameraAt => "Create Camera At", Loom, "Create > Cameras: a camera at `position` looking at `look_at`; a Target camera puts its target node there, a Free camera keeps the gap as its Target Distance." { target: bool, position: [f32; 3], look_at: [f32; 3] };
    CameraType => "Camera Type", Loom, "Camera Parameters > Type: make a camera a Target camera (adds a target at Target Distance) or a Free camera (removes its target, keeps its aim)." { id: u64, target: bool };
    CreateLight => "Create Light", Loom, "Create > Lights > Standard: a light at `position`. Target Spot / Target Direct put their target node at `target`; Free Spot / Free Direct aim at `target` and keep the gap as Target Distance; Omni and Skylight ignore it." { kind: crate::light::LightKind, position: [f32; 3], target: [f32; 3] };
    LightType => "Light Type", Loom, "General Parameters > Light Type and Targeted: make a standard light an Omni, Spot or Directional light, with or without a target (a new target goes at Target Distance; removing one keeps the aim)." { id: u64, light_type: crate::light::LightTypeArg, targeted: bool };
    SetLightExclusion => "Set Light Exclusion", Loom, "Exclude/Include dialog: the objects light `id` excludes (or, with `include`, the only ones it affects) for `scope` 0 Illumination, 1 Shadow Casting, 2 Both." { id: u64, include: bool, scope: u32, nodes: Vec<u64> };
    SetLightProjectorMap => "Set Light Projector Map", Loom, "Advanced Effects > Projector Map: the bitmap light `id` projects (none clears it)." { id: u64, path: Option<String> };
    LightLister => "Light Lister...", Loom, "Tools > Light Lister: a dialog with every light's main settings.";
    SpotDirectionalLightView => "Spot/Directional Light View", MainUI, "Switch the active viewport to the view from the selected (or only) spot or directional light (Shift+4).";
    ViewportDefaultLights => "Viewport Default Lights", Loom, "Viewport Configuration > Lighting: the active viewport's default lighting uses `count` 1 or 2 lights." { count: u32 };
    CreateBiped => "Create Biped", Loom, "Systems > Biped: a biped standing with its feet at `position`, `height` tall (default 1.8 m in scene units), root named `name` (default Bip001, Bip002, ...), with optional Structure values. Only the centre of mass is selected; Figure Mode is on." { position: [f32; 3], height: Option<f32>, name: Option<String>, arms: Option<bool>, neck_links: Option<u32>, spine_links: Option<u32>, leg_links: Option<u32>, tail_links: Option<u32>, fingers: Option<u32>, finger_links: Option<u32>, toes: Option<u32>, toe_links: Option<u32> };
    SetBipedStructure => "Set Biped Structure", Loom, "Biped Structure rollout (Figure Mode only) of the biped `id` (its root or any part): Height, Arms, Neck Links 1-25, Spine Links 1-10, Leg Links 3-4, Tail Links 0-25, Fingers 0-5, Finger Links 1-4, Toes 1-5, Toe Links 1-3. Parts are rebuilt with their names kept." { id: u64, height: Option<f32>, arms: Option<bool>, neck_links: Option<u32>, spine_links: Option<u32>, leg_links: Option<u32>, tail_links: Option<u32>, fingers: Option<u32>, finger_links: Option<u32>, toes: Option<u32>, toe_links: Option<u32> };
    BipedFigureMode => "Biped Figure Mode", Loom, "Biped rollout > Figure Mode of the selected biped: toggles it, or sets it with `on`. On shows the figure pose, off brings back the animated pose." { on: Option<bool> };
    BipedRootName => "Biped Root Name", Loom, "Create Biped > Root Name: renames the centre of mass of the biped `id` and every part with it." { id: u64, name: String };
    CreateBoneChain => "Create Bone Chain", Loom, "Systems > Bones: a chain of bones through the points (local +X along each bone) plus a nub at the end; the chain becomes the selection." { points: Vec<[f32; 3]>, name: Option<String> };
    BoneTools => "Bone Tools...", MainUI, "Animation > Bone Tools... dialog.";
    BoneOptions => "Bone Options", MainUI, "The Bone Tools dialog (Object Properties rollout).";
    BoneEditModeToggle => "Bone Edit Mode Toggle", Loom, "Bone Tools > Bone Edit Mode: moving a bone stretches and re-aims its parent bone instead of moving the hierarchy.";
    RefineBone => "Refine Bone", Loom, "Bone Tools > Refine: split a bone at a fraction of its length; the new bone takes the children." { id: u64, t: f32 };
    MirrorBones => "Mirror Bones", Loom, "Bone Tools > Mirror: copy the selected bones mirrored across a world plane (axis 0 X, 1 Y, 2 Z) at offset; flip 0 none / 1 X / 2 Y / 3 Z." { axis: u32, offset: f32, flip: u32 };
    ReassignRoot => "Reassign Root", Loom, "Bone Tools > Reassign Root: the bone becomes the root of its chain (the chain reverses)." { id: u64 };
    DeleteBone => "Delete Bone", Loom, "Bone Tools > Remove: delete a bone; its children link to its parent, which stretches to them in Bone Edit Mode." { id: u64 };
    ConnectBones => "Connect Bones", Loom, "Bone Tools > Connect: a new bone from one bone's tip to another bone's base, which becomes its child." { id: u64, to: u64 };
    CreateEndBone => "Create End Bone", Loom, "Bone Tools > Create End: a nub bone at the tip of the bone." { id: u64 };
    SetBoneFins => "Set Bone Fins", Loom, "Bone Tools > Fin Adjustment Tools: side / front / back fins on the selected bones." { side: Option<bool>, front: Option<bool>, back: Option<bool> };
    CreateHelper => "Create Helper", Loom, "Helpers > Dummy / Point / Protractor / Compass / Grid (or an IK goal): kind \"dummy\", \"point\", \"protractor\", \"compass\" (size = radius), \"grid\" (size = length and width), \"ikgoal\"." { kind: String, position: [f32; 3], size: f32 };
    CreateTape => "Create Tape", Loom, "Helpers > Tape: Tape## at position and Tape##.Target at target; returns id and target." { position: [f32; 3], target: [f32; 3] };
    HelperReadout => "Helper Readout", Loom, "A Tape's Length and World Space Angles (to X/Y/Z axis, to XY/YZ/ZX plane) or a Protractor's object names and Angle." { id: u64 };
    ProtractorPickObject => "Protractor Pick Object", Loom, "Protractor > Pick Object 1 / 2: the next clicked object (which 1 or 2)." { which: u32 };
    SetProtractorObject => "Set Protractor Object", Loom, "Set a protractor's object 1 or 2 (null clears it)." { id: u64, which: u32, object: Option<u64> };
    ActivateGridObject => "Activate Grid Object", Loom, "Tools > Grids and Snaps > Activate Grid Object: the selected grid helper becomes the construction plane in every viewport.";
    AlignGridToView => "Align Grid to View", Loom, "Tools > Grids and Snaps > Align Grid to View: the active grid object turns to lie in the active view's plane, its top at the top of the view.";
    NudgeGridUp => "Nudge Grid Up", Loom, "Move the active grid object along its normal by Preferences > Viewports > Grid Nudge Distance.";
    NudgeGridDown => "Nudge Grid Down", Loom, "Move the active grid object against its normal by the Grid Nudge Distance.";
    ActivateHomeGrid => "Activate Home Grid", Loom, "Tools > Grids and Snaps > Activate Home Grid: the home grid is the construction plane again.";
    SnapshotDialog => "Snapshot Dialog", Loom, "Tools > Snapshot: open the Snapshot dialog for the selection.";
    Snapshot => "Snapshot", Loom, "Tools > Snapshot: clone the selection as it is at the current frame (range false) or at `copies` frames spread from `from` to `to`; method 0 Copy, 1 Instance, 2 Reference, 3 Mesh. Returns the created ids." { range: bool, from: i32, to: i32, copies: u32, method: u32 };
    CloneAndAlignDialog => "Clone and Align Dialog", Loom, "Tools > Align > Clone and Align: open the modeless Clone and Align dialog.";
    CloneAndAlignPick => "Clone and Align Pick", Loom, "Clone and Align > Pick: toggles picking destination objects in the viewports.";
    CloneAndAlignSetDestinations => "Clone and Align Set Destinations", Loom, "Clone and Align > Pick List / Clear All: set the destination objects (sources are left out)." { ids: Vec<u64> };
    CloneAndAlign => "Clone and Align", Loom, "Clone and Align > Apply: clone the selection onto each destination, aligning position / orientation on the chosen world axes with offsets in the destination's local axes; link makes each clone a child of its destination." { destinations: Vec<u64>, kind: crate::state::CloneKind, link: bool, position: [bool; 3], position_offset: [f32; 3], orientation: [bool; 3], orientation_offset: [f32; 3], match_scale: [bool; 3] };
    PlaceHighlight => "Place Highlight", Loom, "Align flyout > Place Highlight: toggles the mode; dragging over a surface places the selection on the reflected view ray.";
    PlaceHighlightAt => "Place Highlight At", Loom, "Place Highlight at a surface point with its normal, for a view ray direction: the selection moves onto the reflected ray at its original distance and aims its -Z at the point." { point: [f32; 3], normal: [f32; 3], ray: [f32; 3] };
    MeasureDistance => "Measure Distance", Loom, "Tools > Measure Distance: click two points; the status bar shows the distance and the X / Y / Z deltas.";
    Dummy => "Dummy", MainUI, "Create a Dummy helper at the origin (Dummy create mode).";
    HistoryIndependentIkSolver => "History-Independent IK Solver", MainUI, "Animation > IK Solvers > HI Solver: an IK chain from the first selected bone to the last, with a goal at the end joint.";
    IkLimbSolver => "IK Limb Solver", MainUI, "Animation > IK Solvers > IK Limb Solver: a two-bone chain on the selected bones.";
    SplineIkSolver => "Spline IK Solver", MainUI, "Animation > IK Solvers > Spline IK Solver: the selected bones follow a shape (Pick Spline IK Shape).";
    BonesIkChain => "Bones IK Chain", MainUI, "Systems > Bones with IK Chain Assignment: toggles creating an HI chain on every new bone chain.";
    IkSolverToggle => "IK Solver Toggle", MainUI, "Hierarchy > IK > Enabled: toggles the selected goal's (or bone's) chain.";
    IkChainFkSnap => "IK Chain FK Snap", MainUI, "Writes the solved IK rotations into the chain's bones (FK matches IK).";
    IkChainIkSnap => "IK Chain IK Snap", MainUI, "Moves the goal to the chain's FK end joint (IK matches FK).";
    IkChainSnapAction => "IK Chain Snap Action", MainUI, "FK snap then IK snap on the selected chain.";
    IkTerminatorToggle => "IK Terminator Toggle", MainUI, "The selected bone becomes the start (terminator) of its chain.";
    CreateIkChain => "Create IK Chain", Loom, "An IK chain from bone `start` to bone `end` with a goal node; solver \"hi\", \"limb\" or \"spline\"." { start: u64, end: u64, solver: String, name: Option<String> };
    SetIkChainParameter => "Set IK Chain Parameter", Loom, "Hierarchy > IK rollouts: Enabled, IK For FK Pose, Swivel Angle, Threshold, Iterations." { chain: u32, name: String, value: f64 };
    PickSplineIkShape => "Pick Spline IK Shape", Loom, "Spline IK Solver > Pick Shape: the shape node the chain follows." { chain: u32, node: u64 };
    SkinModifier => "Skin Modifier", Skin, "Add a Skin modifier to the selection (bind bones with Add Bones).";
    SkinMorphModifier => "Skin Morph Modifier", Skin, "Add a Skin Morph modifier to the selection.";
    SkinWrapModifier => "Skin Wrap Modifier", Skin, "Add a Skin Wrap modifier to the selection.";
    AddBonesSkin => "Add Bones (Skin)", Skin, "Skin > Bones > Add: pick bones from the scene (Select From Scene dialog).";
    SkinAddBones => "Skin Add Bones", Loom, "Skin > Bones > Add by id: bind the bones at their current pose." { ids: Vec<u64> };
    RemoveMultipleBonesSkin => "Remove Multiple Bones (Skin)", Skin, "Skin > Bones > Remove: the selected bone leaves the list; its weights go to the others.";
    SkinRemoveBone => "Skin Remove Bone", Loom, "Skin > Bones > Remove by id." { id: u64 };
    SkinSelectBone => "Skin Select Bone", Loom, "Skin > Bones list: the current bone (envelope shown, Weight Tool target)." { id: u64 };
    SkinSelectVertices => "Skin Select Vertices", Loom, "Skin > Edit Envelopes: select vertices by id (replace / add / subtract)." { ids: Vec<u32>, op: SelectOp };
    SkinSetVertexWeight => "Skin Set Vertex Weight", Loom, "Weight Tool: set (absolute) or add (relative) the current bone's weight on the selected vertices." { weight: f32, absolute: bool };
    SkinSetEnvelope => "Skin Set Envelope", Loom, "Envelope Properties of the current bone: inner / outer radii at the start and end, falloff 0 Linear 1 Sinual 2 Fast Out 3 Slow Out, Abs. Effect, Squash." { inner_start: Option<f32>, inner_end: Option<f32>, outer_start: Option<f32>, outer_end: Option<f32>, falloff: Option<u32>, abs_effect: Option<f32>, squash: Option<f32> };
    SkinSetCustomWeight => "Skin Set Custom Weight", Loom, "Weight Tool: the value Set Weight Custom / Scale Weight Custom use." { value: f32 };
    SkinNormalize => "Skin Normalize", Loom, "Weight Properties > Normalize: every selected vertex's weights sum to 1.";
    SkinRigidToggle => "Skin Rigid Toggle", Loom, "Weight Properties > Rigid: the selected vertices follow their strongest bone only.";
    SkinBakeSelectedVerts => "Skin Bake Selected Verts", Loom, "Weight Properties > Bake Selected Verts: the envelope weights become hand-set weights.";
    AddCrossSectionSkin => "Add Cross Section (Skin)", Skin, "Envelope > Add: a cross section halfway along the current bone's envelope.";
    RemoveCrossSectionSkin => "Remove Cross Section (Skin)", Skin, "Envelope > Remove: the last added cross section of the current bone.";
    AddWeightSkin => "Add Weight (Skin)", Skin, "Weight Tool +: the current bone gains 0.05 on the selected vertices.";
    SubtractWeightSkin => "Subtract Weight (Skin)", Skin, "Weight Tool -: the current bone loses 0.05 on the selected vertices.";
    ScaleWeightUpSkin => "Scale Weight Up (Skin)", Skin, "Weight Tool > Scale Weight +: the current bone's weight ×1.05 on the selected vertices.";
    ScaleWeightDownSkin => "Scale Weight Down (Skin)", Skin, "Weight Tool > Scale Weight -: the current bone's weight ×0.95 on the selected vertices.";
    ScaleWeightCustomSkin => "Scale Weight Custom (Skin)", Skin, "Weight Tool > Scale Weight: the current bone's weight × the Scale Weight field on the selected vertices.";
    SetWeightCustomSkin => "Set Weight Custom (Skin)", Skin, "Weight Tool: the current bone's weight = the custom value on the selected vertices.";
    SetWeightTo00Skin => "Set Weight To 0.0 (Skin)", Skin, "Weight Tool 0.";
    SetWeightTo010Skin => "Set Weight To 0.10 (Skin)", Skin, "Weight Tool .1.";
    SetWeightTo025Skin => "Set Weight To 0.25 (Skin)", Skin, "Weight Tool .25.";
    SetWeightTo05Skin => "Set Weight To 0.5 (Skin)", Skin, "Weight Tool .5.";
    SetWeightTo075Skin => "Set Weight To 0.75 (Skin)", Skin, "Weight Tool .75.";
    SetWeightTo090Skin => "Set Weight To 0.90 (Skin)", Skin, "Weight Tool .9.";
    SetWeightTo10Skin => "Set Weight To 1.0 (Skin)", Skin, "Weight Tool 1.";
    BlendWeightsSkin => "Blend Weights (Skin)", Skin, "Weight Tool > Blend: the selected vertices take the average of their neighbours' weights.";
    SetSkinPose => "Set Skin Pose", Skin, "Store every bone's current transform as the skin pose.";
    AssumeSkinPose => "Assume Skin Pose", Skin, "Move every bone back to the stored skin pose.";
    SkinPoseModeOnOffToggle => "Skin Pose Mode, On/Off Toggle", Skin, "Skin Pose Mode: the bones show the skin pose; leaving it restores the live pose.";
    BackfaceCullVerticesSkin => "Backface Cull Vertices (Skin)", Skin, "Edit Envelopes > Backface Cull Vertices toggle.";
    DrawCrossSectionsOnTopSkin => "Draw CrossSections On Top (Skin)", Skin, "Display > Draw Cross Sections On Top toggle.";
    DrawEnvelopeOnTopSkin => "Draw Envelope On Top (Skin)", Skin, "Display > Draw Envelope On Top toggle.";
    FilterCrossSectionsSkin => "Filter Cross Sections (Skin)", Skin, "Select > Cross Sections filter toggle.";
    FilterEnvelopesSkin => "Filter Envelopes (Skin)", Skin, "Select > Envelopes filter toggle.";
    FilterVerticesSkin => "Filter Vertices (Skin)", Skin, "Select > Vertices filter toggle.";
    ExcludeVertsSkin => "Exclude Verts (Skin)", Skin, "Exclude the selected vertices from the current bone.";
    IncludeVertsSkin => "Include Verts (Skin)", Skin, "Include the selected vertices in the current bone again.";
    SelectExcludedVertsSkin => "Select Excluded Verts (Skin)", Skin, "Select the vertices excluded from the current bone.";
    GrowSelectionSkin => "Grow Selection (Skin)", Skin, "Grow the vertex selection by one ring of neighbours.";
    ShrinkSelectionSkin => "Shrink Selection (Skin)", Skin, "Shrink the vertex selection by its border.";
    LoopSelectionSkin => "Loop Selection (Skin)", Skin, "Extend the vertex selection along edge loops.";
    RingSelectionSkin => "Ring Selection (Skin)", Skin, "Extend the vertex selection along edge rings.";
    SkinGrow => "Skin Grow", Skin, "Grow the vertex selection.";
    SkinShrink => "Skin Shrink", Skin, "Shrink the vertex selection.";
    SkinLoop => "Skin Loop", Skin, "Loop the vertex selection.";
    SkinRing => "Skin Ring", Skin, "Ring the vertex selection.";
    SkinElement => "Skin Element", Skin, "Select the whole elements the selected vertices belong to.";
    SelectVerticesByBoneSkin => "Select Vertices By Bone (Skin)", Skin, "Select every vertex the current bone weights.";
    SelectNextBoneSkin => "Select Next Bone (Skin)", Skin, "The next bone in the list becomes current.";
    SelectPreviousBoneSkin => "Select Previous Bone (Skin)", Skin, "The previous bone in the list becomes current.";
    SelectParentBoneSkin => "Select Parent Bone (Skin)", Skin, "The current bone's parent (when in the list) becomes current.";
    SelectChildBoneSkin => "Select Child Bone (Skin)", Skin, "The current bone's first child in the list becomes current.";
    SelectNextSiblingBoneSkin => "Select Next Sibling Bone (Skin)", Skin, "The next bone sharing the current bone's parent becomes current.";
    SelectPreviousSiblingBoneSkin => "Select Previous Sibling Bone (Skin)", Skin, "The previous bone sharing the current bone's parent becomes current.";
    SelectStartPointSkin => "Select Start Point (Skin)", Skin, "Select the current envelope's start handle.";
    SelectEndPointSkin => "Select End Point (Skin)", Skin, "Select the current envelope's end handle.";
    CopyEnvelopeSkin => "Copy Envelope (Skin)", Skin, "Copy the current bone's envelope.";
    PasteEnvelopeSkin => "Paste Envelope (Skin)", Skin, "Paste the copied envelope onto the current bone.";
    PasteToAllEnvelopesSkin => "Paste To All Envelopes (Skin)", Skin, "Paste the copied envelope onto every bone.";
    CopyWeightsSkin => "Copy Weights (Skin)", Skin, "Copy the selected vertices' weights (with their positions).";
    PasteWeightsSkin => "Paste Weights (Skin)", Skin, "Paste the copied weights onto the selected vertices by order.";
    PasteWeightsByPosSkin => "Paste Weights By Pos(Skin)", Skin, "Weight Tool > Paste-Pos: each selected vertex takes the weights of the nearest copied vertex within the Paste-Pos Tolerance.";
    RemoveZeroWeightsSkin => "Remove Zero Weights (Skin)", Skin, "Drop weights at or below the Remove Zero Limit (0.0005).";
    ZoomToSelectedBoneSkin => "Zoom To Selected Bone (Skin)", Skin, "Frame the current bone in the active viewport.";
    ZoomToSelectedGizmoSkin => "Zoom To Selected Gizmo (Skin)", Skin, "Frame the current gizmo's bone in the active viewport.";
    GizmoResetResetRotationPlaneSkin => "Gizmo Reset Reset Rotation Plane (Skin)", Skin, "Gizmos > Reset: the current gizmo's keys start over from the bones' current angle.";
    WeightTableDialogSkin => "Weight Table Dialog (Skin)", Skin, "Skin > Weight Table dialog.";
    WeightToolDialogSkin => "Weight Tool Dialog (Skin)", Skin, "Skin > Weight Tool dialog.";
    MirrorModeSkin => "Mirror Mode (Skin)", Loom, "Skin > Mirror Parameters > Mirror Mode toggle (green = negative side, blue = positive).";
    PasteGreenToBlue => "Paste Green to Blue", Loom, "Skin > Mirror Parameters: weights of the negative side land on the positive side, bones swapped.";
    PasteBlueToGreen => "Paste Blue to Green", Loom, "Skin > Mirror Parameters: weights of the positive side land on the negative side, bones swapped.";
    MirrorPaste => "Mirror Paste", Loom, "Skin > Mirror Parameters: the selected vertices' weights land on their mirrored vertices.";
    SkinAddGizmo => "Skin Add Gizmo", Loom, "Skin > Gizmos > Add: a Joint Angle (\"joint\") or Bulge Angle (\"bulge\") deformer between the current bone and its parent, with a key at the current angle." { kind: String };
    SkinRemoveGizmo => "Skin Remove Gizmo", Loom, "Skin > Gizmos > Remove the current gizmo.";
    SkinSelectGizmo => "Skin Select Gizmo", Loom, "Skin > Gizmos list: the current gizmo." { index: u32 };
    SkinAddGizmoKey => "Skin Add Gizmo Key", Loom, "Skin > Gizmos > Add Key: a key at the bones' current angle.";
    SkinSetGizmoKey => "Skin Set Gizmo Key", Loom, "Skin > Gizmos: a key's bulge amount / angle." { key: u32, bulge: Option<f32>, angle: Option<f32> };
    SkinSetGizmoDelta => "Skin Set Gizmo Delta", Loom, "Joint Angle Deformer > Edit Lattice: the delta (child-bone frame) of a vertex at the current key." { key: u32, vertex: u32, delta: [f32; 3] };
    SkinSetGizmoOption => "Skin Set Gizmo Option", Loom, "Skin > Gizmos: Influence or Enabled of the current gizmo." { name: String, value: f64 };
    SkinSetVertexWeights => "Skin Set Vertex Weights", Loom, "Weight Table: one bone's weight on the given vertices (the other bones share the rest)." { vertices: Vec<u32>, bone: u64, weight: f32 };
    SkinSetScaleWeight => "Skin Set Scale Weight", Loom, "Weight Tool > Scale Weight field: the factor Scale Weight Custom uses (default 0.95)." { value: f32 };
    SkinSetPasteTolerance => "Skin Set Paste Tolerance", Loom, "Weight Tool > Paste-Pos Tolerance: Paste-Pos fills only vertices this close to a copied one (default 0.1)." { value: f32 };
    SkinSelectBoneByName => "Skin Select Bone By Name", Loom, "Parameters > bone name field: the first bone whose name starts with the text (* is a wildcard) becomes current." { name: String };
    SkinEnvelopeVisibility => "Skin Envelope Visibility", Loom, "Envelope Properties > Envelope Visibility: the current bone's envelope stays drawn while another bone is current (toggle).";
    SkinPaintWeightsToggle => "Skin Paint Weights", Loom, "Weight Properties > Paint Weights: LMB drags brush weight onto the current bone (Alt erases, Ctrl+Shift drag sizes the brush, Alt+Shift drag sets its strength, right-click ends).";
    SkinPaintStroke => "Skin Paint Stroke", Loom, "Paint Weights: one brush dab per world point on the current bone, with the Painter Options (erase takes weight away)." { points: Vec<[f32; 3]>, erase: bool };
    SkinSetPainterOption => "Skin Set Painter Option", Loom, "Painter Options: Max Strength, Max Size, Additive, Falloff (0 Linear 1 Smooth 2 Slow 3 Fast 4 Flat), Draw Ring, Mirror, Mirror Axis (0 X 1 Y 2 Z), Mirror Offset; Paint Blend Weights." { name: String, value: f64 };
    SkinPainterOptionsDialog => "Skin Painter Options", Loom, "Weight Properties > Painter Options... dialog.";
    MorpherModifier => "Morpher Modifier", MainUI, "Add a Morpher modifier to the selection.";
    DisplayBones => "Display Bones", MainUI, "Viewports show bones only (other geometry hidden from display).";
    DisplayBonesAndObjects => "Display Bones and Objects", MainUI, "Viewports show bones and objects (the default).";
    MorpherPickTarget => "Morpher Pick Target", Loom, "Morpher > Channel Parameters > Pick Object From Scene: channel `channel` (0-based) takes the node as its target." { channel: u32, node: u64 };
    MorpherCaptureCurrentState => "Morpher Capture Current State", Loom, "Morpher > Capture Current State: the stack's current output becomes the channel's target." { channel: u32 };
    MorpherSetChannel => "Morpher Set Channel", Loom, "Morpher channel settings: Value, Active, Use Limits, Min, Max, Tension." { channel: u32, name: String, value: f64 };
    MorpherRenameChannel => "Morpher Rename Channel", Loom, "Morpher: the channel's name." { channel: u32, name: String };
    MorpherDeleteChannel => "Morpher Delete Channel", Loom, "Morpher > Delete: the channel empties." { channel: u32 };
    MorpherAddProgressiveTarget => "Morpher Add Progressive Target", Loom, "Morpher > Progressive Morph > Pick: another target on the channel (percent evenly spaced until set)." { channel: u32, node: u64 };
    MorpherSetProgressivePercent => "Morpher Set Progressive Percent", Loom, "Morpher > Progressive Morph: the percent at which a target is reached." { channel: u32, target: u32, percent: f32 };
    MorpherReload => "Morpher Reload", Loom, "Morpher: every target re-captures its scene object.";
    MorpherExtractTarget => "Morpher Extract Target", Loom, "Morpher > Extract: a new object from the channel's full target." { channel: u32 };
    MorpherSetGlobal => "Morpher Set Global", Loom, "Morpher > Global Parameters: Use Vertex Selection, Spinner Min, Spinner Max." { name: String, value: f64 };
    SkinMorphAddBones => "Skin Morph Add Bones", Loom, "Skin Morph > Add Bone: bones whose angles drive the morphs." { ids: Vec<u64> };
    SkinMorphRemoveBone => "Skin Morph Remove Bone", Loom, "Skin Morph > Remove Bone." { id: u64 };
    SkinMorphCreateMorph => "Skin Morph Create Morph", Loom, "Skin Morph > Create Morph: a morph keyed at the bone's current angle." { bone: u64 };
    SkinMorphSetVertexDelta => "Skin Morph Set Vertex Delta", Loom, "Skin Morph > Edit: a vertex's delta (bone frame) for a morph." { morph: u32, vertex: u32, delta: [f32; 3] };
    SkinMorphSetMorph => "Skin Morph Set Morph", Loom, "Skin Morph > Local Properties: Enabled, Angle, Influence Angle, Falloff." { morph: u32, name: String, value: f64 };
    SkinMorphRenameMorph => "Skin Morph Rename Morph", Loom, "Skin Morph: the morph's name." { morph: u32, name: String };
    SkinMorphDeleteMorph => "Skin Morph Delete Morph", Loom, "Skin Morph > Delete Morph." { morph: u32 };
    SkinMorphReload => "Skin Morph Reload", Loom, "Skin Morph > Reload: the morph's angle key becomes the bone's current angle." { morph: u32 };
    SkinWrapAddBase => "Skin Wrap Add Base", Loom, "Skin Wrap > Add: a base object the wrapped mesh follows (binds now)." { id: u64 };
    SkinWrapRemoveBase => "Skin Wrap Remove Base", Loom, "Skin Wrap > Remove a base object." { id: u64 };
    SkinWrapSetOption => "Skin Wrap Set Option", Loom, "Skin Wrap settings (rebinds): Deformation Engine 0 Face / 1 Vertex, Falloff, Distance Influence, Face Limit, Threshold, Weight All Points." { name: String, value: f64 };
    SkinWrapReset => "Skin Wrap Reset", Loom, "Skin Wrap > Reset: rebind to the bases' current pose.";
    SkinWrapConvertToSkin => "Skin Wrap Convert To Skin", Loom, "Skin Wrap > Convert To Skin: a Skin modifier with the base's bones and weights blended from the bound faces; the wrap turns off.";
    SetParameter => "Set Parameter", Loom, "Set a parametric object's rollout parameter by its name (e.g. \"Height Segs\", \"Radius 1\", \"Slice On\")." { id: u64, name: String, value: f64 };
    ConvertToEditablePoly => "Convert to Editable Poly", Loom, "Collapse the selected objects to Editable Poly (quad menu > Convert To).";
    ConvertToEditableMesh => "Convert to Editable Mesh", Loom, "Collapse the selected objects to Editable Mesh: triangles, the polygons' own edges visible and their cuts invisible (quad menu > Convert To).";
    ConvertToEditablePatch => "Convert to Editable Patch", Loom, "Collapse the selected objects to Editable Patch: tri / quad patches over the faces, or the patches flowing up the stack (quad menu > Convert To).";
    CreatePatchGrid => "Create Patch Grid", Loom, "Create > Patch Grids: a Quad Patch (`quad` true) or a Tri Patch centred at `position`, `length` along Y and `width` along X." { quad: bool, position: [f32; 3], length: f32, width: f32 };
    SubObjectLevel => "Sub-object Level", Loom, "Enter the selected object's sub-object level by its 1-based place in its list (Editable Mesh / Edit Mesh: Vertex, Edge, Face, Polygon, Element; Editable Patch / Edit Patch: Vertex, Handle, Edge, Patch, Element; Editable Poly: Vertex, Edge, Border, Polygon, Element); 0 = top level." { level: u32 };
    MeshPatchSetting => "Mesh Patch Setting", Loom, "Editable Mesh / Editable Patch rollout settings by name: Ignore Visible Edges, Planar Thresh, Auto Edge Threshold, Auto Edge Mode (0 Set and Clear, 1 Set, 2 Clear), Explode Angle, Explode To Objects, Tension, Face-Center, Extrude, Outline, Patch Extrude, Patch Outline, Patch Weld Threshold, Relax Value, Relax Iterations, Keep Boundary Points Fixed." { name: String, value: f64 };
    MeshEdgeVisibility => "Mesh Edge Visibility", Loom, "Editable Mesh > Surface Properties (Edge): Visible / Invisible on the selected edges." { visible: bool };
    MeshAutoEdge => "Mesh Auto Edge", Loom, "Editable Mesh > Surface Properties (Edge) > Auto Edge on the selected edges (Threshold and Set and Clear / Set / Clear settings).";
    MeshTurnEdge => "Mesh Turn Edge", Loom, "Editable Mesh > Edit Geometry (Edge) > Turn: the edges (the selected ones when `edges` is empty) become the other diagonal of their two triangles." { edges: Vec<u32> };
    MeshDivide => "Mesh Divide", Loom, "Editable Mesh > Edit Geometry > Divide: the selected edges split at their middle (Edge level), or the selected faces at their centre (Face levels); the new edges are invisible.";
    MeshExplode => "Mesh Explode", Loom, "Editable Mesh > Edit Geometry > Explode: the selected faces (all when none) split by the Explode angle into objects named after `name` (default Object) or into elements, per the setting." { name: Option<String> };
    MeshTessellate => "Mesh Tessellate", Loom, "Editable Mesh > Edit Geometry (Face) > Tessellate the selected faces by Edge or by Face-Center, lifted by Tension.";
    MeshExtrude => "Mesh Extrude", Loom, "Editable Mesh > Edit Geometry (Face) > Extrude the selected faces by `amount` along their normals." { amount: f32 };
    MeshBevel => "Mesh Bevel", Loom, "Editable Mesh > Edit Geometry (Face) > Bevel: Extrude the selected faces by `height`, then grow their outline by `outline`." { height: f32, outline: f32 };
    MeshFlipNormals => "Mesh Flip Normals", Loom, "Editable Mesh > Surface Properties (Face) > Flip: the selected faces (whole elements) turn over.";
    MeshUnifyNormals => "Mesh Unify Normals", Loom, "Editable Mesh > Surface Properties (Face) > Unify: the elements of the selected faces turn to face outward.";
    PatchAddTri => "Patch Add Tri", Loom, "Editable Patch > Geometry (Edge) > Add Tri: a tri patch on each selected open edge.";
    PatchAddQuad => "Patch Add Quad", Loom, "Editable Patch > Geometry (Edge) > Add Quad: a quad patch on each selected open edge.";
    PatchWeld => "Patch Weld", Loom, "Editable Patch > Geometry (Vertex) > Weld Selected: selected vertices within the Weld Threshold merge.";
    PatchDelete => "Patch Delete", Loom, "Editable Patch > Geometry > Delete: the patches of the selected vertices, edges or patches go.";
    PatchExtrude => "Patch Extrude", Loom, "Editable Patch > Geometry (Patch) > Extrude & Bevel: the selected patches extrude by `amount` along their normals and their outline grows by `outline`." { amount: f32, outline: f32 };
    PatchRelax => "Patch Relax", Loom, "Editable Patch > Surface > Relax: the selected vertices (all when none) move toward their neighbours by Relax Value, Iterations times (Keep Boundary Points Fixed).";
    PatchFlipNormals => "Patch Flip Normals", Loom, "Editable Patch > Surface Properties (Patch) > Flip: the selected patches turn over.";
    SelectionFilter => "Selection Filter", Loom, "Main Toolbar > Selection Filter list: which object kinds the viewport can select." { filter: crate::state::SelectionFilter };
    WindowCrossingSelectionToggle => "Window/Crossing Selection Toggle", Loom, "Main Toolbar > Window/Crossing: region selection takes fully enclosed objects only (Window) or anything touched (Crossing).";
    CycleSelectionMethod => "Cycle Selection Method", MainUI, "Cycle the Selection Region flyout: Rectangular, Circular, Fence, Lasso, Paint (Ctrl+F).";
    SelectionRegion => "Selection Region", Loom, "Main Toolbar > Selection Region flyout: Rectangular, Circular, Fence, Lasso or Paint." { shape: crate::state::SelectionRegion };
    ReferenceCoordinateSystem => "Reference Coordinate System", Loom, "Main Toolbar > Reference Coordinate System list (View, Screen, World, Parent, Local, Gimbal, Grid, Pick); Pick waits for a clicked object (Pick Hit)." { system: crate::state::CoordSystem };
    UseCenter => "Use Center", Loom, "Main Toolbar > Use Center flyout: Pivot Point, Selection or Transform Coordinate centre." { mode: crate::state::CenterMode };
    KeyboardShortcutOverrideToggle => "Keyboard Shortcut Override Toggle", Loom, "Main Toolbar: when off, only Main UI shortcuts are active (sub-object level keys 1–5 are a group shortcut).";
    SelectAndLink => "Select and Link", Loom, "Main Toolbar > Select and Link: the next clicked object becomes the parent of the selection.";
    UnlinkSelection => "Unlink Selection", Loom, "Main Toolbar > Unlink Selection: unlink the selected objects from their parents.";
    NamedSelectionSet => "Named Selection Set", Loom, "Create or update a named selection set from the current selection." { name: String };
    SelectNamedSet => "Select Named Set", Loom, "Select the objects of a named selection set." { name: String };
    DeleteNamedSet => "Delete Named Set", Loom, "Delete a named selection set." { name: String };
    EditNamedSelectionSets => "Edit Named Selection Sets", Loom, "Open the Named Selection Sets dialog.";
    HideNodes => "Hide Nodes", Loom, "Hide objects by id." { ids: Vec<u64> };
    UnhideNodes => "Unhide Nodes", Loom, "Unhide objects by id." { ids: Vec<u64> };
    FreezeNodes => "Freeze Nodes", Loom, "Freeze objects by id." { ids: Vec<u64> };
    UnfreezeNodes => "Unfreeze Nodes", Loom, "Unfreeze objects by id." { ids: Vec<u64> };
    AffectPivotOnly => "Affect Pivot Only", Loom, "Toggle Hierarchy > Pivot > Affect Pivot Only: Move/Rotate act on the pivot, the mesh stays (Scale has no effect).";
    AffectObjectOnly => "Affect Object Only", Loom, "Toggle Hierarchy > Pivot > Affect Object Only: Move/Rotate act on the object, the pivot stays.";
    CenterPivotToObject => "Center to Object", Loom, "Move the selected objects' pivots to their bounds centre (Hierarchy > Pivot > Center to Object).";
    AffectHierarchyOnly => "Affect Hierarchy Only", Loom, "Toggle Hierarchy > Pivot > Affect Hierarchy Only: Rotate/Scale swing and stretch the links to the children; no object turns or resizes.";
    AlignPivotToWorld => "Align to World", Loom, "Hierarchy > Pivot > Align to World: turn the pivot (Affect Pivot Only) or, with Affect Object Only, the object to the world axes.";
    AlignPivotToObject => "Align to Object", Loom, "Hierarchy > Pivot > Align to Object (Affect Pivot Only): turn the pivot to the object's own axes; the mesh stays.";
    CenterObjectToPivot => "Center to Pivot", Loom, "Hierarchy > Pivot > Center to Pivot (Affect Object Only): move the object so its bounds centre is at the pivot.";
    AlignObjectToPivot => "Align to Pivot", Loom, "Hierarchy > Pivot > Align to Pivot (Affect Object Only): turn the object to the pivot's axes.";
    DontAffectChildren => "Don't Affect Children", Loom, "Toggle Hierarchy > Adjust Transform > Don't Affect Children: transforms leave the children where they are.";
    ResetTransform => "Reset Transform", Loom, "Hierarchy > Adjust Transform > Reset: Transform: the local axes turn to the world; the mesh and the children stay.";
    EditWorkingPivot => "Edit Working Pivot", Loom, "Toggle Hierarchy > Working Pivot > Edit Working Pivot: the transform tools move and turn the working pivot (Local system at first).";
    UseWorkingPivot => "Use Working Pivot", Loom, "Toggle Hierarchy > Working Pivot > Use Working Pivot: every transform uses the Working system about the working pivot.";
    WorkingPivotAlignToView => "Working Pivot Align To View", Loom, "Hierarchy > Working Pivot > Align To View: the working pivot's XY plane faces the active view.";
    WorkingPivotReset => "Working Pivot Reset", Loom, "Hierarchy > Working Pivot > Reset: the working pivot goes to the selected object's pivot (sub-objects: their centre).";
    PlacePivotToView => "Place Pivot To View", Loom, "Hierarchy > Working Pivot > Place Pivot To: View: the next click places the working pivot in the screen plane through it.";
    PlacePivotToSurface => "Place Pivot To Surface", Loom, "Hierarchy > Working Pivot > Place Pivot To: Surface: the next click places the working pivot on the surface, Z along its normal.";
    PlacePivotAlignToViewToggle => "Place Pivot Align To View", Loom, "Toggle Hierarchy > Working Pivot > Place Pivot To > Align To View: placing also aligns the working pivot to the view.";
    SetWorkingPivot => "Set Working Pivot", Loom, "Place the working pivot: world position and/or Euler XYZ rotation in degrees." { position: Option<[f32; 3]>, rotation_deg: Option<[f32; 3]> };
    ResetScale => "Reset Scale", Loom, "Hierarchy > Adjust Transform > Reset: Scale: the node scale becomes 100 % with no visual change (the object takes the scale).";
    ResetPivot => "Reset Pivot", Loom, "Reset the selected objects' pivots to the object origin (Hierarchy > Pivot > Reset Pivot).";
    SetPivot => "Set Pivot", Loom, "Set a node's pivot (node transform) without moving the mesh." { id: u64, position: Option<[f32; 3]>, rotation_deg: Option<[f32; 3]> };
    SetNodeLocks => "Set Node Locks", Loom, "Hierarchy > Link Info > Locks: per-axis move/rotate/scale locks (true = locked)." { id: u64, move_axes: Option<[bool; 3]>, rotate_axes: Option<[bool; 3]>, scale_axes: Option<[bool; 3]> };
    SetNodeInherit => "Set Node Inherit", Loom, "Hierarchy > Link Info > Inherit: which parent transform components the node inherits (true = inherit)." { id: u64, move_axes: Option<[bool; 3]>, rotate_axes: Option<[bool; 3]>, scale_axes: Option<[bool; 3]> };
    SetSelectionDisplayProperty => "Set Selection Display Property", Loom, "Display > Display Properties checkbox by name on every selected object, one undo step." { name: String, value: bool };
    SetDisplayProperty => "Set Display Property", Loom, "Display > Display Properties / Link Display checkbox by name: See-Through, Display as Box, Backface Cull, Edges Only, Vertex Ticks, Trajectory, Ignore Extents, Show Frozen in Gray, Vertex Color, Vertex Color Shaded, Display Links, Link Replaces Object." { id: u64, name: String, value: bool };
    SetRenderProperty => "Set Render Property", Loom, "Object Properties > Rendering Control checkbox by name for the selection: Renderable, Inherit Visibility, Visible to Camera, Receive Shadows, Cast Shadows." { name: String, value: bool };
    SetObjectVisibility => "Set Object Visibility", Loom, "Object Properties > Rendering Control > Visibility (0–1) for the selection: sets (and under Auto Key keys) the visibility track." { value: f32 };
    SetUserProperties => "Set User Properties", Loom, "Object Properties > User Defined: the object's free-text user properties." { id: u64, text: String };
    SetDisplayColor => "Set Display Color", Loom, "Display > Display Color: Wireframe and Shaded each \"Object Color\" or \"Material Color\"." { wireframe: Option<String>, shaded: Option<String> };
    HideFrozenObjectsToggle => "Hide Frozen Objects Toggle", Loom, "Display > Hide > Hide Frozen Objects: frozen objects leave the viewports.";
    HideBoneObjectsToggle => "Hide Bone Objects Toggle", Loom, "Display > Hide by Category > Bone Objects.";
    SetStatistics => "Set Statistics", Loom, "Viewport Configuration > Statistics: Polygon / Triangle / Edge / Vertex Count, Frames Per Second, scope Total, Selection or Total + Selection; default_settings restores the defaults first." { polygons: Option<bool>, triangles: Option<bool>, edges: Option<bool>, vertices: Option<bool>, fps: Option<bool>, scope: Option<String>, default_settings: Option<bool> };
    DisplaySelectedEdgedFacesToggle => "Display Selected with Edged Faces Toggle", Loom, "Viewport Configuration > Rendering Method: shaded viewports draw the edges of selected objects.";
    SetSafeFrames => "Set Safe Frames", Loom, "Viewport Configuration > Safe Frames: Live Area, Action / Title / User Safe (insets in percent, horizontal and vertical), 12-Field Grid (4x3 or 12x9)." { live_area: Option<bool>, action_safe: Option<bool>, title_safe: Option<bool>, user_safe: Option<bool>, grid: Option<bool>, action: Option<[f32; 2]>, title: Option<[f32; 2]>, user: Option<[f32; 2]>, grid_4x3: Option<bool> };
    ObjectColorDialog => "Object Color Dialog", Loom, "Open the Object Color dialog (the object colour swatch).";
    SetObjectColor => "Set Object Color", Loom, "Object Color dialog OK: colour the selection (one undo step); the colour becomes the current colour for new objects." { rgb: [u8; 3] };
    SetAssignRandomColors => "Set Assign Random Colors", Loom, "Object Color dialog > Assign Random Colors: new objects get palette colours in turn; off, the current colour." { on: bool };
    SetCustomColor => "Set Custom Color", Loom, "Object Color dialog > Custom Colors slot (0-15)." { index: u32, rgb: [u8; 3] };
    SelectByColor => "Select By Color", Loom, "Select the visible, unfrozen objects whose object colour is `rgb`." { rgb: [u8; 3] };
    HideByHit => "Hide By Hit", Loom, "Display > Hide > Hide by Hit: the next clicked object is hidden (right-click ends the mode).";
    FreezeByHit => "Freeze By Hit", Loom, "Display > Freeze > Freeze by Hit: the next clicked object is frozen.";
    UnfreezeByHit => "Unfreeze By Hit", Loom, "Display > Freeze > Unfreeze by Hit: the next clicked frozen object is unfrozen.";
    PickHit => "Pick Hit", Loom, "Apply the active by-Hit mode to a node (what a viewport click does)." { id: u64 };
    ExitPickMode => "Exit Pick Mode", Loom, "Leave the by-Hit pick mode (right-click).";
    HideByName => "Hide By Name", Loom, "Open the Select From Scene dialog to hide objects by name.";
    UnhideByName => "Unhide By Name", Loom, "Open the Select From Scene dialog to unhide objects by name.";
    FreezeByName => "Freeze By Name", Loom, "Open the Select From Scene dialog to freeze objects by name.";
    UnfreezeByName => "Unfreeze By Name", Loom, "Open the Select From Scene dialog to unfreeze objects by name.";
    ResetXForm => "Reset XForm", Loom, "Utilities > Reset XForm > Reset Selected: the node's rotation and scale (and pivot offset) move into a new XForm modifier on top of the stack, the node keeps only its position; collapse the stack to bake them.";
    RescaleWorldUnits => "Rescale World Units", Loom, "Utilities > Rescale World Units: scale the whole scene by a factor." { factor: f32 };
    Measure => "Measure", Loom, "Utilities > Measure: object name (or Multiple Objects Selected), dimensions, surface area, volume (volume_open: the mesh has holes), solid centre of mass and shape length of the selection.";
    MeasureNodes => "Measure Nodes", Loom, "Utilities > Measure with Lock Selected: the same read-outs for the given nodes." { ids: Vec<u64> };
    MeasureCreateCenterPoint => "Measure Create Center Point", Loom, "Utilities > Measure > Create Center Point: a Point helper at the selection's centre of mass.";
    SetWireColor => "Set Wire Color", Loom, "Set a node's object colour (Name and Color rollout)." { id: u64, rgb: [u8; 3] };
    SetTransform => "Set Transform", Loom, "Set a node's local position, rotation (degrees, Euler XYZ) and/or scale." { id: u64, position: Option<[f32; 3]>, rotation_deg: Option<[f32; 3]>, scale: Option<[f32; 3]> };
    MoveSelection => "Move Selection", Loom, "Translate the selection; the delta is in the reference coordinate system (`system`, default the toolbar's)." { delta: [f32; 3], system: Option<crate::state::CoordSystem> };
    RotateSelection => "Rotate Selection", Loom, "Rotate the selection around an axis of the reference coordinate system about the transform centre (Use Center)." { axis: Axis, degrees: f32, system: Option<crate::state::CoordSystem> };
    ScaleSelection => "Scale Selection", Loom, "Scale the selection along the reference coordinate system's axes about the transform centre." { factors: [f32; 3], system: Option<crate::state::CoordSystem> };
    RotateSelectionFree => "Rotate Selection Free", Loom, "Rotate the selection by a world-space quaternion [x,y,z,w] about the transform centre (trackball / screen handle)." { quat: [f32; 4] };
    TransformGizmoXConstraint => "Transform Gizmo X Constraint", MainUI, "Restrict transforms to X (F5).";
    TransformGizmoYConstraint => "Transform Gizmo Y Constraint", MainUI, "Restrict transforms to Y (F6).";
    TransformGizmoZConstraint => "Transform Gizmo Z Constraint", MainUI, "Restrict transforms to Z (F7).";
    TransformGizmoToggle => "Transform Gizmo Toggle", MainUI, "Show or hide the transform gizmos (Shift+Ctrl+X).";
    TransformGizmoSizeUp => "Transform Gizmo Size Up", MainUI, "Grow the transform gizmo by 2 % of the viewport (=).";
    TransformGizmoSizeDown => "Transform Gizmo Size Down", MainUI, "Shrink the transform gizmo by 2 % of the viewport (-).";
    TransformGizmoPlaneConstraintCycle => "Transform Gizmo Plane Constraint Cycle", MainUI, "Cycle the plane constraint XY, YZ, ZX (F8).";
    PickCoordinateSystem => "Pick Coordinate System", Loom, "Use a node's coordinate system as the reference coordinate system (Pick)." { id: u64 };
    SelectionMode => "Selection Mode", Loom, "Set object or sub-object selection level; clears the selection." { mode: crate::state::SelectionMode };
    SelectNodes => "Select Nodes", Loom, "Select nodes by id." { ids: Vec<u64>, op: SelectOp };
    SelectSub => "Select Sub", Loom, "Select sub-objects of a node by index in the current sub-object level." { node: Option<u64>, ids: Vec<u32>, op: SelectOp };
    TestRoom => "Test Room", Loom, "Replace the scene with a five-wall test room (floor plus four inward-facing walls).";
    SelectFacesFacing => "Select Faces Facing", Loom, "Select faces whose normal points along a direction." { node: Option<u64>, direction: [f32; 3], tolerance: f32 };
    EdgeRing => "Edge Ring", Loom, "Grow the selected edges into their rings.";
    EdgeLoop => "Edge Loop", Loom, "Grow the selected edges into their loops.";
    ExtrudeFaces => "Extrude Faces", Loom, "Extrude the selected faces along their normals." { distance: f32 };
    DissolveEdges => "Dissolve Edges", Loom, "Remove the selected edges, merging their faces.";
    CleanRemove => "Clean Remove", Loom, "Ctrl+Remove at Edge level (also Ctrl+Backspace): remove the selected edges and the vertices they leave with only two edges.";
    SwiftLoopMode => "Swift Loop Mode", Loom, "Toggle Swift Loop: each click on the surface inserts an edge loop across the ring of the nearest edge; right-click exits.";
    SwiftLoop => "Swift Loop", Loom, "Insert the edge loop through raw edge `edge` of `node`, `t` along it from its canonical origin (what a Swift Loop click does)." { node: u64, edge: u32, t: f32 };
    LoopMode => "Loop Mode", Loom, "Toggle Modify Selection > Loop Mode: a clicked edge selects its loop (Alt deselects it); two adjacent vertices or polygons select their loop.";
    SelectByHalf => "Select By Half", Loom, "Selection > By Half: select the sub-objects whose centre is on the positive side of the pivot along `axis`; `invert` selects the negative side (Invert Axis)." { axis: Axis, invert: bool };
    BranchesMode => "Branches", Loom, "Freeform > PolyDraw > Branches on / off: drag from a polygon (Shift: from the selected polygons) to grow a tapering branch.";
    Branch => "Branch", Loom, "Grow a branch from polygons `faces` of `node` through the world points `path` (the first is where the drag started): one segment per further point, the end scaled by 1 + `taper`." { node: u64, faces: Vec<u32>, path: Vec<[f32; 3]>, taper: f32 };
    BranchSettings => "Branch Settings", Loom, "PolyDraw settings: Branch Taper and Minimum Distance (pixels between branch segments); omitted fields keep their value." { taper: Option<f32>, min_distance: Option<f32> };
    BridgeSpecificEdges => "Bridge Specific Edges", Loom, "Bridge Edges dialog: on bridges the two picked edges (Pick Edge 1 / 2), off bridges the edge selection (Use Edge Selection)." { on: bool };
    PickBridgeEdge => "Pick Bridge Edge", Loom, "Bridge Edges dialog > Pick Edge 1 (`which` 1) / Pick Edge 2 (2): the next edge pick (Pick Sub) becomes that edge." { which: u32 };
    DeletePolygons => "Delete Polygons", Loom, "Delete the selected polygons; `delete_isolated` also deletes the vertices left with no polygon (the answer to the Delete prompt)." { delete_isolated: bool };
    Collapse => "Collapse", Loom, "Collapse the selected sub-objects to their centre.";
    ConnectEdgeRing => "Connect Edge Ring", Loom, "Connect the selected edge ring with a new loop.";
    Rename => "Rename", Loom, "Rename a node." { id: u64, name: String };
    SetParent => "Set Parent", Loom, "Link a node to a parent (null unlinks), keeping its world transform." { id: u64, parent: Option<u64> };
    Orbit => "Orbit", Loom, "Set the orbit camera (degrees for angles; roll turns the view about its line of sight)." { yaw: Option<f32>, pitch: Option<f32>, distance: Option<f32>, target: Option<[f32; 3]>, roll: Option<f32> };
    Hotkey => "Hotkey", Loom, "Press a keyboard shortcut by name, e.g. \"W\", \"Ctrl+Z\", \"Alt+Ctrl+Z\"." { chord: String };
    Overlays => "Overlays", Loom, "Toggle edge and vertex overlays." { edges: Option<bool>, vertices: Option<bool> };
}

/// Editable Poly chart actions a later phase implements (the plan id).
pub const DEFERRED_POLY: &[(&str, &str)] = &[];

/// Chart "(Spline)" rows left for phase 9 (Trim / Extend / Cross Insert /
/// Bind need the geometry helpers that phase adds).
/// Chart "Unwrap UVW" rows this program does not implement: 2012+ tools
/// (LSCM, Tweak, Sketch's redesign), Paint Select, Spline Map and the
/// pelt stretcher springs (the pelt here relaxes a boundary circle).
pub const DEFERRED_UNWRAP: &[(&str, &str)] = &[
    ("LSCM Interactive", "not in 2010"),
    ("LSCM Reset", "not in 2010"),
    ("LSCM Solve", "not in 2010"),
    ("Tweak UVW", "not in 2010"),
    ("Pelt Dialog Mirror Stretcher", "no stretcher"),
    ("Pelt Dialog Straighten Stretcher", "no stretcher"),
    ("Pelt Dialog Select Stretcher", "no stretcher"),
    ("Pelt Dialog Snap Seams", "no stretcher"),
];

/// Chart "Unwrap UVW" rows whose names collide with Main UI / Editable
/// Poly actions: registered with an "(Unwrap)" suffix.
pub const UNWRAP_ALIASES: &[(&str, &str)] = &[
    ("Relax", "Relax (Unwrap)"),
    ("Unhide All", "Unhide All (Unwrap)"),
    ("Unfreeze All", "Unfreeze All (Unwrap)"),
    ("Select By Material ID", "Select By Material ID (Unwrap)"),
    ("Select By Smoothing Group", "Select By Smoothing Group (Unwrap)"),
];

/// Track View chart rows Loom does not implement, with the reason.
pub const DEFERRED_TRACK_VIEW: &[(&str, &str)] = &[
    ("Apply Ease Curve", "spec 6.6: Ease and Multiplier Curves out"),
    ("Apply Multiplier Curve", "spec 6.6: Ease and Multiplier Curves out"),
    ("Delete Ease/Multiplier Curve", "spec 6.6: Ease and Multiplier Curves out"),
    ("Ease Curve Out-of-Range Types", "spec 6.6: Ease and Multiplier Curves out"),
    ("Multiplier Curve Out-of-Range Types", "spec 6.6: Ease and Multiplier Curves out"),
    ("Ease/Multiplier Curve Enable Toggle", "spec 6.6: Ease and Multiplier Curves out"),
    ("Ease In", "not in the 2012 reference: no 2010 behaviour to match"),
    ("Ease Out", "not in the 2012 reference: no 2010 behaviour to match"),
    ("Linear", "not in the 2012 reference: no 2010 behaviour to match"),
    ("Retime Tool", "Retime tools are after 2012: no 2010 behaviour to match"),
    ("Retime All Tool", "Retime tools are after 2012: no 2010 behaviour to match"),
    ("Retime Move Left", "Retime tools are after 2012: no 2010 behaviour to match"),
    ("Retime Move Right", "Retime tools are after 2012: no 2010 behaviour to match"),
    ("Auto Expand - Unlocked", "not in the 2012 reference: no 2010 behaviour to match"),
    ("Auto Expand Limits", "Limit controller: not in the spec 6.6 controller list"),
    ("Copy Limit Only", "Limit controller: not in the spec 6.6 controller list"),
    ("Paste Limit Only", "Limit controller: not in the spec 6.6 controller list"),
    ("Remove Limit", "Limit controller: not in the spec 6.6 controller list"),
    ("Set Lower Limit", "Limit controller: not in the spec 6.6 controller list"),
    ("Set Upper Limit", "Limit controller: not in the spec 6.6 controller list"),
    ("Toggle Limit", "Limit controller: not in the spec 6.6 controller list"),
    ("Filter - Active Layer Toggle", "animation layers: not in spec 6.6"),
    ("Position Ranges Mode", "decoupled ranges: Loom ranges always span the keys"),
    ("Respect Animation Range", "parametric controller ranges: Loom Noise / List act over all time"),
    ("Ignore Animation Range", "parametric controller ranges: Loom Noise / List act over all time"),
];

/// Edit Normals rows whose chart names are taken by other groups' commands.
pub const EDIT_NORMALS_ALIASES: &[(&str, &str)] = &[
    ("Edge Level", "Edge Level (Edit Normals)"),
    ("Face Level", "Face Level (Edit Normals)"),
    ("Object Level", "Object Level (Edit Normals)"),
    ("Vertex Level", "Vertex Level (Edit Normals)"),
];

/// Track View rows whose chart names are taken by other groups' commands.
pub const TRACK_VIEW_ALIASES: &[(&str, &str)] = &[
    ("Collapse All", "Collapse All (Track View)"),
    ("Pan", "Pan (Track View)"),
    ("Select All", "Select All (Track View)"),
    ("Select Children", "Select Children (Track View)"),
    ("Select Invert", "Select Invert (Track View)"),
    ("Select None", "Select None (Track View)"),
];

pub const DEFERRED_SPLINE: &[(&str, &str)] = &[];

/// Actions that this program deliberately does not implement, with the
/// spec section that excludes them.
pub const OUT_OF_SCOPE: &[(&str, &str)] = &[
    ("Environment Dialog Toggle", "3"),
    ("Create Animated Sequence File...", "3"),
    ("Hide Particle Systems Toggle", "3"),
    ("Hide Space Warps Toggle", "3"),
    ("Sound Toggle", "3"),
    ("Adaptive Degradation Toggle", "3"),
    // Legacy OpenGL-driver-only feature, not in the spec's viewport list.
    ("Virtual Viewport Pan Down", "6.1"),
    ("Virtual Viewport Pan Left", "6.1"),
    ("Virtual Viewport Pan Right", "6.1"),
    ("Virtual Viewport Pan Up", "6.1"),
    ("Virtual Viewport Toggle", "6.1"),
    ("Virtual Viewport Zoom In", "6.1"),
    ("Virtual Viewport Zoom Out", "6.1"),
];

/// Actions that later plans implement; each names its plan.
pub const DEFERRED: &[(&str, &str)] = &[
    ("History-Dependent IK Solver", "spec: HD Solver out of scope"),
    ("Skin Wrap Patch Modifier", "no patch objects"),
    ];

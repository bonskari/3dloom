//! Viewports and the 14 viewport layouts (Viewport Configuration >
//! Layout panel).

use crate::state::{ViewKind, ViewUndo};
use loom_input::OrbitCamera;
use serde::{Deserialize, Serialize};

/// Legacy shading modes of the Shading viewport label menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shading {
    SmoothHighlights,
    Wireframe,
    HiddenLine,
    Flat,
    /// Physically based shading with lights and shadows (the Bevy renderer).
    Realistic,
}

impl Shading {
    pub fn label(self) -> &'static str {
        match self {
            Shading::SmoothHighlights => "Smooth + Highlights",
            Shading::Wireframe => "Wireframe",
            Shading::HiddenLine => "Hidden Line",
            Shading::Flat => "Flat",
            Shading::Realistic => "Realistic",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Viewport {
    pub kind: ViewKind,
    pub camera: OrbitCamera,
    pub shading: Shading,
    pub edged_faces: bool,
    pub show_grid: bool,
    pub disabled: bool,
    pub safe_frames: bool,
    /// Viewport Clipping: `clip` (near, far, distances from the eye) applies while on.
    pub clipping: bool,
    pub clip: (f32, f32),
    /// Perspective field of view (vertical), per viewport.
    pub fov_y_radians: f32,
    /// Illuminate with Scene Lights (Ctrl+L switches to Default Lights);
    /// a scene with no light that is on is lit by the default lights.
    pub scene_lights: bool,
    /// Default Lights: 1 light (from the viewer) or 2 (key and fill).
    pub default_lights: u32,
    pub background: Option<String>,
    pub background_locked: bool,
    /// Lock Zoom/Pan: the view (target, distance) the image was locked in; it
    /// zooms and pans with the scene from there.
    pub background_anchor: Option<(glam::Vec3, f32)>,
    /// Aspect Ratio: Match Viewport / Match Bitmap / Match Rendering Output.
    pub background_aspect: BackgroundAspect,
    /// Display Background.
    pub background_display: bool,
    /// Bumped by Update Background Image so the UI reloads the file.
    pub background_version: u32,
    pub view_undo: ViewUndo,
}

/// Viewport Background > Aspect Ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BackgroundAspect {
    /// Stretched to the viewport.
    #[default]
    Viewport,
    /// The bitmap's own aspect, fitted in the viewport.
    Bitmap,
    /// The render output's aspect (Render Setup size), fitted in the viewport.
    Rendering,
}

impl BackgroundAspect {
    pub const ALL: [BackgroundAspect; 3] = [BackgroundAspect::Viewport, BackgroundAspect::Bitmap, BackgroundAspect::Rendering];

    pub fn label(self) -> &'static str {
        match self {
            BackgroundAspect::Viewport => "Match Viewport",
            BackgroundAspect::Bitmap => "Match Bitmap",
            BackgroundAspect::Rendering => "Match Rendering Output",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.label().eq_ignore_ascii_case(s))
    }
}

/// The default Perspective (and ViewCube Home) angles: looking at the origin
/// from the front-left and above, so X runs right and Y runs left.
pub const HOME_PERSPECTIVE: (f32, f32) = (-3.0 * std::f32::consts::FRAC_PI_4, 0.65);

impl Viewport {
    pub fn new(kind: ViewKind) -> Self {
        let mut camera = OrbitCamera::default();
        if let Some((yaw, pitch)) = kind.angles() {
            camera.yaw = yaw;
            camera.pitch = pitch;
        } else {
            (camera.yaw, camera.pitch) = HOME_PERSPECTIVE;
        }
        camera.distance = 12.0;
        Self {
            kind,
            camera,
            // Defaults: axis views wireframe, Perspective shaded.
            shading: if kind.angles().is_some() { Shading::Wireframe } else { Shading::SmoothHighlights },
            edged_faces: false,
            show_grid: true,
            disabled: false,
            safe_frames: false,
            clipping: false,
            clip: (0.0, 1000.0),
            fov_y_radians: 45f32.to_radians(),
            scene_lights: true,
            default_lights: 1,
            background: None,
            background_locked: false,
            background_anchor: None,
            background_aspect: BackgroundAspect::Viewport,
            background_display: true,
            background_version: 0,
            view_undo: ViewUndo::default(),
        }
    }

    pub fn is_orthographic(&self) -> bool {
        !self.kind.is_perspective()
    }
}

/// The 14 layouts in the order of the Layout panel icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewportLayout {
    Single,
    TwoV,
    TwoH,
    ThreeLeftBig,
    ThreeTopBig,
    ThreeRightBig,
    ThreeBottomBig,
    ThreeCols,
    ThreeRows,
    FourGrid,
    FourLeftBig,
    FourRightBig,
    FourTopBig,
    FourBottomBig,
}

/// Draggable split fractions, clamped to 0.05..=0.95.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LayoutSplits {
    pub x: f32,
    pub y: f32,
}

impl Default for LayoutSplits {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

impl LayoutSplits {
    pub fn set_x(&mut self, v: f32) {
        self.x = v.clamp(0.05, 0.95);
    }
    pub fn set_y(&mut self, v: f32) {
        self.y = v.clamp(0.05, 0.95);
    }
}

impl ViewportLayout {
    pub const ALL: [ViewportLayout; 14] = [
        ViewportLayout::Single,
        ViewportLayout::TwoV,
        ViewportLayout::TwoH,
        ViewportLayout::ThreeLeftBig,
        ViewportLayout::ThreeTopBig,
        ViewportLayout::ThreeRightBig,
        ViewportLayout::ThreeBottomBig,
        ViewportLayout::ThreeCols,
        ViewportLayout::ThreeRows,
        ViewportLayout::FourGrid,
        ViewportLayout::FourLeftBig,
        ViewportLayout::FourRightBig,
        ViewportLayout::FourTopBig,
        ViewportLayout::FourBottomBig,
    ];

    pub fn cell_count(self) -> usize {
        use ViewportLayout::*;
        match self {
            Single => 1,
            TwoV | TwoH => 2,
            ThreeLeftBig | ThreeTopBig | ThreeRightBig | ThreeBottomBig | ThreeCols | ThreeRows => 3,
            FourGrid | FourLeftBig | FourRightBig | FourTopBig | FourBottomBig => 4,
        }
    }

    pub fn label(self) -> &'static str {
        use ViewportLayout::*;
        match self {
            Single => "1 viewport",
            TwoV => "2 side by side",
            TwoH => "2 stacked",
            ThreeLeftBig => "3: large left",
            ThreeTopBig => "3: large top",
            ThreeRightBig => "3: large right",
            ThreeBottomBig => "3: large bottom",
            ThreeCols => "3 columns",
            ThreeRows => "3 rows",
            FourGrid => "4 equal",
            FourLeftBig => "4: large left",
            FourRightBig => "4: large right",
            FourTopBig => "4: large top",
            FourBottomBig => "4: large bottom",
        }
    }

    /// Cell rects `[x0, y0, x1, y1]` in unit coordinates, big cell first.
    pub fn cells(self, s: &LayoutSplits) -> Vec<[f32; 4]> {
        use ViewportLayout::*;
        let (x, y) = (s.x, s.y);
        let col = |n: usize, i: usize, x0: f32, x1: f32| -> (f32, f32) {
            let w = (x1 - x0) / n as f32;
            (x0 + w * i as f32, x0 + w * (i + 1) as f32)
        };
        match self {
            Single => vec![[0.0, 0.0, 1.0, 1.0]],
            TwoV => vec![[0.0, 0.0, x, 1.0], [x, 0.0, 1.0, 1.0]],
            TwoH => vec![[0.0, 0.0, 1.0, y], [0.0, y, 1.0, 1.0]],
            ThreeLeftBig => vec![[0.0, 0.0, x, 1.0], [x, 0.0, 1.0, y], [x, y, 1.0, 1.0]],
            ThreeTopBig => vec![[0.0, 0.0, 1.0, y], [0.0, y, x, 1.0], [x, y, 1.0, 1.0]],
            ThreeRightBig => vec![[x, 0.0, 1.0, 1.0], [0.0, 0.0, x, y], [0.0, y, x, 1.0]],
            ThreeBottomBig => vec![[0.0, y, 1.0, 1.0], [0.0, 0.0, x, y], [x, 0.0, 1.0, y]],
            ThreeCols => (0..3).map(|i| { let (a, b) = col(3, i, 0.0, 1.0); [a, 0.0, b, 1.0] }).collect(),
            ThreeRows => (0..3).map(|i| { let (a, b) = col(3, i, 0.0, 1.0); [0.0, a, 1.0, b] }).collect(),
            FourGrid => vec![[0.0, 0.0, x, y], [x, 0.0, 1.0, y], [0.0, y, x, 1.0], [x, y, 1.0, 1.0]],
            FourLeftBig => {
                let mut v = vec![[0.0, 0.0, x, 1.0]];
                v.extend((0..3).map(|i| { let (a, b) = col(3, i, 0.0, 1.0); [x, a, 1.0, b] }));
                v
            }
            FourRightBig => {
                let mut v = vec![[x, 0.0, 1.0, 1.0]];
                v.extend((0..3).map(|i| { let (a, b) = col(3, i, 0.0, 1.0); [0.0, a, x, b] }));
                v
            }
            FourTopBig => {
                let mut v = vec![[0.0, 0.0, 1.0, y]];
                v.extend((0..3).map(|i| { let (a, b) = col(3, i, 0.0, 1.0); [a, y, b, 1.0] }));
                v
            }
            FourBottomBig => {
                let mut v = vec![[0.0, y, 1.0, 1.0]];
                v.extend((0..3).map(|i| { let (a, b) = col(3, i, 0.0, 1.0); [a, 0.0, b, y] }));
                v
            }
        }
    }
}

/// The default views per cell. Four cells: Top, Front, Left, Perspective;
/// a layout with one big cell puts Perspective there.
pub fn default_kinds(layout: ViewportLayout) -> Vec<ViewKind> {
    use ViewportLayout::*;
    let n = layout.cell_count();
    match layout {
        Single => vec![ViewKind::Perspective],
        ThreeLeftBig | ThreeTopBig | ThreeRightBig | ThreeBottomBig | FourLeftBig | FourRightBig
        | FourTopBig | FourBottomBig => {
            let mut v = vec![ViewKind::Perspective, ViewKind::Top, ViewKind::Front, ViewKind::Left];
            v.truncate(n);
            v
        }
        _ => {
            let mut v = vec![ViewKind::Top, ViewKind::Front, ViewKind::Left, ViewKind::Perspective];
            if n < 4 {
                // Keep a Perspective view in the last cell.
                v.truncate(n);
                if let Some(last) = v.last_mut() {
                    *last = ViewKind::Perspective;
                }
            }
            v
        }
    }
}

pub use loom_input::NavMode;

/// Display panel > Hide by Category.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HideCategories {
    pub geometry: bool,
    pub shapes: bool,
    pub lights: bool,
    pub cameras: bool,
    pub helpers: bool,
}

/// Modeless dialogs the UI shows; commands open them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dialog {
    SelectFromScene,
    /// C with several cameras and none selected: pick the camera to look through.
    SelectCamera,
    /// Render Setup > Render Output > Files...: the file a Production render saves to.
    RenderFile,
    /// Rendering > Render Setup (F10).
    RenderSetup,
    /// Tools > Light Lister.
    LightLister,
    /// General Parameters > Exclude...: the Exclude/Include dialog of a light.
    LightExclude { light: u64 },
    /// Rendered Frame Window > Save Image.
    SaveRenderedImage,
    /// Material Editor > Maps: choose the bitmap for channel `channel` (MAP_CHANNELS order).
    MaterialMap { channel: u8 },
    /// Material Editor, Bitmap level > Bitmap: choose the image of the map being edited.
    MaterialBitmap,
    /// Material/Map Browser, choosing what `for_` asks for.
    MaterialBrowser(BrowserFor),
    /// Replace Material / Replace Map dialog before the Type button turns the
    /// current level into the compound type at `MaterialKind::TYPES[kind]`
    /// (a map: `MapType::ALL[kind]`).
    ReplaceMaterial { kind: u8, map: bool },
    /// Material Editor Options.
    MaterialEditorOptions,
    /// Bitmap Parameters > View Image: Specify Cropping/Placement.
    SpecifyCropping,
    /// Copy (Instance) Map: dragging map button `from` onto `to` (indexes into MAP_SLOTS).
    CopyMap { from: u8, to: u8 },
    ViewportConfiguration,
    TimeConfiguration,
    ViewportBackground,
    /// Viewport Background > Files...: pick the image; then back to the dialog.
    BackgroundFile,
    NamedSelectionSets,
    TransformTypeIn,
    CloneOptions,
    GridAndSnapSettings,
    Align,
    AlignToView,
    NormalAlign,
    Mirror,
    Array,
    SpacingTool,
    /// Tools > Snapshot.
    Snapshot,
    /// Tools > Align > Clone and Align (modeless).
    CloneAndAlign,
    ObjectProperties,
    Preferences,
    UnitsSetup,
    CustomizeUI,
    /// Customize User Interface > Keyboard > Save / Load (.kbd).
    SaveShortcuts,
    LoadShortcuts,
    /// Customize > Save / Load Custom UI Scheme (.ui).
    SaveCustomUiScheme,
    LoadCustomUiScheme,
    /// Editable Poly settings windows (caddy-less dialogs).
    PolySettings(PolySettingsKind),
    /// Editable Poly > Detach.
    Detach,
    /// Deleting polygons would leave vertices with no polygon: "Delete isolated vertices?"
    DeleteIsolatedVertices,
    LayerManager,
    /// File > Open.
    OpenFile,
    /// File > Save As (also Save with no current file).
    SaveFileAs,
    /// File > Save Copy As.
    SaveFileAsCopy,
    /// "Save changes?" before the pending action discards the scene.
    SaveChanges(PendingFileAction),
    /// Line tool: "Close spline?" when the first vertex is clicked.
    CloseSpline,
    /// Editable Spline > Refine with Connect on an existing vertex: Refine / Connect Only.
    RefineVertex,
    /// A base parameter change under a topology-dependent modifier: Yes / Hold/Yes / No.
    TopologyWarning,
    /// File > Import (FBX, OBJ, .max).
    ImportFile,
    SaveAnimation,
    LoadAnimation,
    OutOfRangeTypes,
    ReduceKeys,
    TrackSets,
    AssignController,
    KeyInfo,
    TrackViewFilters,
    TrackViewUtilities,
    /// File > Export (FBX, OBJ).
    ExportFile,
    /// File > Export Selected (FBX, OBJ).
    ExportSelected,
    /// File > Merge: pick the file; then the object list.
    MergeFile,
    /// File > Merge: the file's object list (`AppState::merge_pending`).
    MergeObjects,
    /// File > XRef Objects: pick the file; then the object list.
    XrefObjectsFile,
    /// File > XRef Scene: pick the file.
    XrefSceneFile,
    /// FBX Importer / Exporter and OBJ Importer / Exporter options for the
    /// file in `AppState::exchange_pending` (after Import / Export picks it).
    FbxImportOptions,
    FbxExportOptions,
    ObjImportOptions,
    ObjExportOptions,
    /// Animation > Bone Tools...
    BoneTools,
    /// Skin > Weight Table.
    WeightTable,
    /// Skin > Weight Tool.
    WeightTool,
    /// Skin > Weight Properties > Painter Options.
    PainterOptions,
    /// Display > Show Map in Viewport > Bitmap: pick the image file.
    PickTexture,
    /// UVW Map > Bitmap Fit: pick the image whose aspect the gizmo takes.
    BitmapFit,
    /// Edit UVWs > Options.
    UnwrapOptions,
    /// Edit UVWs tool dialogs.
    StitchTool,
    RelaxTool,
    PackUvs,
    FlattenMapping,
    NormalMapping,
    UnfoldMapping,
    PeltMap,
    /// Unwrap UVW > Save / Load (.uvw) file pickers.
    SaveUvw,
    LoadUvw,
    /// Edit UVWs > Pick Texture file picker.
    PickUvTexture,
    /// Edit UVWs > Render UVW Template dialog.
    RenderUvwTemplate,
    /// Point Cache > Record / Load file pickers for stack entry `index`.
    PointCacheRecord { index: u32, start: i32, end: i32 },
    PointCacheLoad { index: u32 },
    /// File > New on a scene with objects: Keep Objects and Hierarchy / Keep Objects / New All.
    NewScene,
    /// File > Reset after the save prompt: "Do you really want to reset?"
    ConfirmReset,
    /// File > Save As > Save Selected.
    SaveSelected,
    /// File > Save As > Archive.
    ArchiveFile,
    /// File > Summary Info.
    SummaryInfo,
    /// File > File Properties.
    FileProperties,
    /// Group > Group: the group name.
    GroupName,
    /// Layer Manager > Layer Properties of `layer`.
    LayerProperties { layer: u32 },
}

/// What the Material/Map Browser is choosing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserFor {
    /// The Type button at a material level: a material type.
    MaterialType,
    /// The Type button at a map level: a map type.
    MapType,
    /// An empty map button at the current level: index into `MAP_SLOTS`.
    Map(u8),
    /// Get Material: a material from the scene or the editor's slots.
    GetMaterial,
}

/// What runs after the Save Changes prompt is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingFileAction {
    Open,
    /// File > Recent: the n-th file of `AppState::recent_files`.
    OpenRecent(usize),
    New,
    /// New Scene dialog: keep the objects (and their hierarchy).
    NewKeeping(NewKeep),
    Reset,
    Exit,
}

/// New Scene dialog: what the new scene keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewKeep {
    ObjectsAndHierarchy,
    Objects,
}

impl NewKeep {
    /// The `NewSceneKeep` argument.
    pub fn arg(self) -> &'static str {
        match self {
            NewKeep::ObjectsAndHierarchy => "objects_and_hierarchy",
            NewKeep::Objects => "objects",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolySettingsKind {
    Weld,
    Chamfer,
    ConnectEdges,
    Bridge,
    Extrude,
    Inset,
    Bevel,
    Outline,
    Hinge,
    MSmooth,
    Tessellate,
    Relax,
    ExtrudeAlongSpline,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourteen_layouts_have_expected_cell_counts() {
        let counts: Vec<usize> = ViewportLayout::ALL.iter().map(|l| l.cell_count()).collect();
        assert_eq!(counts, vec![1, 2, 2, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4]);
        for l in ViewportLayout::ALL {
            assert_eq!(l.cells(&LayoutSplits::default()).len(), l.cell_count());
            assert_eq!(default_kinds(l).len(), l.cell_count());
        }
    }

    #[test]
    fn four_grid_cells_cover_unit_square_without_overlap() {
        for l in ViewportLayout::ALL {
            let cells = l.cells(&LayoutSplits::default());
            let area: f32 = cells.iter().map(|c| (c[2] - c[0]) * (c[3] - c[1])).sum();
            assert!((area - 1.0).abs() < 1e-5, "{l:?} area {area}");
            for (i, a) in cells.iter().enumerate() {
                for b in &cells[i + 1..] {
                    let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
                    let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
                    assert!(w * h < 1e-6, "{l:?} overlap {a:?} {b:?}");
                }
            }
        }
    }

    #[test]
    fn splits_clamp_to_five_percent() {
        let mut s = LayoutSplits::default();
        s.set_x(1.2);
        assert_eq!(s.x, 0.95);
        s.set_x(-1.0);
        assert_eq!(s.x, 0.05);
        s.set_y(0.3);
        assert_eq!(s.y, 0.3);
    }

    #[test]
    fn cells_never_negative_for_tiny_rects() {
        for l in ViewportLayout::ALL {
            for (x, y) in [(0.05, 0.05), (0.95, 0.95), (0.05, 0.95)] {
                let mut s = LayoutSplits::default();
                s.set_x(x);
                s.set_y(y);
                for c in l.cells(&s) {
                    assert!(c[2] >= c[0] && c[3] >= c[1], "{l:?} {c:?}");
                }
            }
        }
    }

    #[test]
    fn default_kinds_follow_reference() {
        assert_eq!(default_kinds(ViewportLayout::Single), vec![ViewKind::Perspective]);
        assert_eq!(default_kinds(ViewportLayout::FourGrid), vec![ViewKind::Top, ViewKind::Front, ViewKind::Left, ViewKind::Perspective]);
        assert_eq!(default_kinds(ViewportLayout::ThreeLeftBig)[0], ViewKind::Perspective, "the big cell is Perspective");
    }
}

//! The default quad menu: display (upper right), transform (lower
//! right), tools 1 (upper left) and tools 2 (lower left) for Editable Poly.
//! A quadrant's title repeats the last command picked from it, which stays
//! highlighted; the small icon right of an item opens its settings.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectionMode};

#[derive(Debug, Clone)]
pub struct Entry {
    pub label: String,
    pub action: Action,
    /// The settings icon right of the item (Move → Transform Type-In, Extrude → Extrude Settings).
    pub settings: Option<Command>,
}

/// The last command picked from a quad menu: its quadrant's title repeats it.
#[derive(Debug, Clone)]
pub struct Last {
    pub kind: QuadKind,
    pub quadrant: &'static str,
    pub label: String,
    pub action: Action,
}

#[derive(Debug, Clone)]
pub enum Action {
    Command(Command),
    /// An action the user added (Customize User Interface > Quads), by name.
    Named(String),
    /// A menu the user added: a submenu with that menu's entries.
    Menu(loom_commands::ui_custom::CustomItem),
    /// A separator line the user added.
    Separator,
    Submenu(Vec<Entry>),
    /// Greyed out, with the reason as tooltip.
    Disabled(String),
    /// Start creating a Standard Primitive (Ctrl+right-click quad).
    Primitive(loom_commands::PrimitiveKind),
}

/// Which quad menu a right-click opens: plain, Shift (snaps), Ctrl
/// (modeling), Alt (animation), Ctrl+Alt (lighting and rendering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuadKind {
    #[default]
    Default,
    Snaps,
    Create,
    Animation,
    Render,
    /// A quad set the user made (index into `UiCustom::quad_sets`), opened by its Quad Shortcut.
    User(u16),
}

impl QuadKind {
    /// The quad set's name in the Quads panel.
    pub fn set_name(self, app: &LoomApp) -> String {
        match self {
            QuadKind::Default => "Default Viewport Quad".into(),
            QuadKind::Snaps => "Snap".into(),
            QuadKind::Create => "Modeling".into(),
            QuadKind::Animation => "Animation".into(),
            QuadKind::Render => "Lighting | Render".into(),
            QuadKind::User(i) => app.state.ui_custom.quad_sets().get(i as usize).cloned().unwrap_or_default(),
        }
    }

    /// The quad kind of a set by its name.
    pub fn of_set(app: &LoomApp, set: &str) -> Option<QuadKind> {
        Some(match set {
            "Default Viewport Quad" => QuadKind::Default,
            "Snap" => QuadKind::Snaps,
            "Modeling" => QuadKind::Create,
            "Animation" => QuadKind::Animation,
            "Lighting | Render" => QuadKind::Render,
            _ => QuadKind::User(app.state.ui_custom.quad_sets().iter().position(|s| s == set)? as u16),
        })
    }

    /// The quad a right-click with these modifiers opens.
    pub fn for_modifiers(m: loom_input::Modifiers) -> Self {
        if m.shift {
            QuadKind::Snaps
        } else if m.ctrl && m.alt {
            QuadKind::Render
        } else if m.ctrl {
            QuadKind::Create
        } else if m.alt {
            QuadKind::Animation
        } else {
            QuadKind::Default
        }
    }
}

#[derive(Debug, Clone)]
pub struct Quadrant {
    pub title: &'static str,
    pub items: Vec<Entry>,
    /// The label the user gave the quadrant (Quads panel > Label), shown instead of the title.
    pub label: Option<String>,
}

impl Quadrant {
    pub fn new(title: &'static str, items: Vec<Entry>) -> Self {
        Quadrant { title, items, label: None }
    }
    /// What the quadrant's title bar shows.
    pub fn shown_title(&self) -> &str {
        self.label.as_deref().unwrap_or(self.title)
    }
}

/// A label without its check mark or current-level marker.
pub fn plain_label(label: &str) -> &str {
    label.trim_start_matches(['\u{2714}', ' ', '>']).trim()
}

pub struct Quadrants {
    pub display: Quadrant,
    pub transform: Quadrant,
    pub tools1: Option<Quadrant>,
    pub tools2: Option<Quadrant>,
}

fn e(label: &str, cmd: Command) -> Entry {
    Entry { label: label.to_owned(), action: Action::Command(cmd), settings: None }
}

/// An item with a settings icon.
fn es(label: &str, cmd: Command, settings: Command) -> Entry {
    Entry { settings: Some(settings), ..e(label, cmd) }
}

fn off(label: &str, why: &str) -> Entry {
    Entry { label: label.to_owned(), action: Action::Disabled(why.to_owned()), settings: None }
}

/// A toggle item: a check mark shows its state, as in the menu bar.
fn check(label: &str, on: bool, cmd: Command) -> Entry {
    e(&if on { format!("✔ {label}") } else { format!("    {label}") }, cmd)
}

pub fn quadrants(app: &LoomApp) -> Quadrants {
    let s = &app.state;
    let display = Quadrant {
        title: "display",
        items: vec![
            e("Isolate Selection", Command::IsolateSelection),
            e("Unfreeze All", Command::UnfreezeAll),
            e("Freeze Selection", Command::FreezeSelection),
            e("Unhide by Name", Command::UnhideByName),
            e("Unhide All", Command::UnhideAll),
            e("Hide Unselected", Command::HideUnselected),
            e("Hide Selection", Command::HideSelection),
        ],
        label: None,
    };
    let type_in = Command::TransformTypeInDialogToggle;
    let mut transform_items = vec![
        es("Move", Command::SelectAndMove, type_in.clone()),
        es("Rotate", Command::SelectAndRotate, type_in.clone()),
        es("Scale", Command::SelectAndScale, type_in),
        e("Select", Command::SelectObject),
    ];
    // Items that act on a selected object show only when one is selected, as in the reference.
    let any = !s.selection.is_empty();
    if any {
        transform_items.push(e("Select Similar", Command::SelectSimilar));
    }
    transform_items.push(e("Clone", Command::Clone));
    if any {
        transform_items.extend([
            e("Object Properties...", Command::ObjectProperties),
            e("Curve Editor...", Command::OpenCurveEditor),
            e("Dope Sheet...", Command::OpenDopeSheet),
            Entry {
                label: "Convert To".into(),
                action: Action::Submenu(vec![
                    e("Convert to Editable Mesh", Command::ConvertToEditableMesh),
                    e("Convert to Editable Poly", Command::ConvertToEditablePoly),
                    e("Convert to Editable Patch", Command::ConvertToEditablePatch),
                    e("Convert to Editable Spline", Command::ConvertToEditableSpline),
                ]),
                settings: None,
            },
        ]);
    }
    let transform = Quadrant::new("transform", transform_items);
    let poly_selected = s
        .selection
        .iter()
        .any(|id| s.scene.node(*id).is_some_and(|n| n.edit_poly().is_some()));
    let spline_selected = s.spline_selected();
    // An active modifier with sub-object levels (Gizmo / Center, FFD lattice …): tools 1 lists them.
    let modifier_levels: Option<(usize, u32, &'static [&'static str], String)> = s.selection.iter().find_map(|id| {
        let n = s.scene.node(*id)?;
        let a = n.active_entry?;
        let slot = n.modifiers.get(a)?;
        let levels = slot.modifier.sub_levels();
        (!levels.is_empty() && n.edit_poly().filter(|_| n.editing_entry().is_some()).is_none() && n.editing_spline_entry().is_none() && n.editing_patch_entry().is_none()).then(|| (a, slot.sub_level, levels, slot.name.clone()))
    });
    let (tools1, tools2) = if let Some((index, current, levels, name)) = modifier_levels {
        let mut t1: Vec<Entry> = vec![e(if current == 0 { "> Top-level" } else { "Top-level" }, Command::ModifierSubobjectLevel { index: index as u32, level: 0 })];
        for (i, l) in levels.iter().enumerate() {
            let lvl = i as u32 + 1;
            let label = if current == lvl { format!("> {l}") } else { (*l).to_owned() };
            t1.push(e(&label, Command::ModifierSubobjectLevel { index: index as u32, level: lvl }));
        }
        (Some(Quadrant::new("tools 1", t1)), Some(Quadrant::new("tools 2", vec![e(&format!("Collapse To {name}"), Command::CollapseTo { index: index as u32 }), e("Make Unique", Command::MakeUnique), e("Delete Modifier", Command::DeleteModifier { index: index as u32 })])))
    } else if spline_selected {
        let level = s.selection_mode;
        let sub = level != SelectionMode::Object;
        let mut t1 = Vec::new();
        if sub {
            t1.push(e("Hide", Command::HideSpline));
            t1.push(e("Unhide All", Command::UnhideAllSpline));
        }
        t1.push(e("Attach", Command::AttachSpline));
        t1.push(if sub { e("Detach", Command::DetachSplineSpline) } else { off("Detach", "needs a sub-object selection") });
        t1.push(e("Reverse", Command::ReverseSpline));
        t1.push(e("Create Line", Command::CreateLineSpline));
        let mut t2: Vec<Entry> = [("Top-level", SelectionMode::Object), ("Vertex", SelectionMode::Vertex), ("Segment", SelectionMode::Edge), ("Spline", SelectionMode::Face)]
            .iter()
            .map(|(l, m)| {
                let label = if *m == level { format!("> {l}") } else { (*l).to_owned() };
                e(&label, Command::SelectionMode { mode: *m })
            })
            .collect();
        let ops: Vec<Entry> = match level {
            SelectionMode::Vertex => vec![
                e("Refine", Command::RefineSpline),
                e("Break", Command::BreakVerticesSpline),
                e("Weld", Command::WeldVerticesSpline),
                e("Fuse", Command::FuseVerticesSpline),
                e("Fillet", Command::FilletSpline),
                e("Chamfer", Command::ChamferVertexSpline),
                e("Make First", Command::MakeFirstSpline),
                e("Corner", Command::SetVertexType { kind: "Corner".into() }),
                e("Smooth", Command::SetVertexType { kind: "Smooth".into() }),
                e("Bezier", Command::SetVertexType { kind: "Bezier".into() }),
                e("Bezier Corner", Command::SetVertexType { kind: "Bezier Corner".into() }),
            ],
            SelectionMode::Edge | SelectionMode::Border => vec![e("Refine", Command::RefineSpline), e("Divide", Command::DivideSpline), e("Insert", Command::InsertSelectionSpline), e("Detach", Command::DetachSegmentSpline)],
            SelectionMode::Face | SelectionMode::Element => vec![e("Outline", Command::OutlineSpline), e("Close", Command::CloseSpline), e("Explode", Command::ExplodeSpline), e("Mirror", Command::MirrorHorizontallySpline)],
            SelectionMode::Object => Vec::new(),
        };
        t2.extend(ops);
        (Some(Quadrant::new("tools 1", t1)), Some(Quadrant::new("tools 2", t2)))
    } else if s.patch_selected() || s.tri_mesh_selected() {
        // Editable Mesh / Patch: their own five levels (two share a selection mode) and tools.
        let patch = s.patch_selected();
        let current = loom_commands::mesh_edit::current_level(s);
        let names: [&str; 5] = if patch { ["Vertex", "Handle", "Edge", "Patch", "Element"] } else { ["Vertex", "Edge", "Face", "Polygon", "Element"] };
        let mut t2: Vec<Entry> = vec![e(if current == 0 { "> Top-level" } else { "Top-level" }, Command::SubObjectLevel { level: 0 })];
        for (i, l) in names.iter().enumerate() {
            let lvl = i as u32 + 1;
            let label = if current == lvl { format!("> {l}") } else { (*l).to_owned() };
            t2.push(e(&label, Command::SubObjectLevel { level: lvl }));
        }
        let mp = &s.mesh_patch;
        let ops: Vec<Entry> = match (patch, current) {
            (true, 1) => vec![e("Weld", Command::PatchWeld), e("Delete", Command::PatchDelete)],
            (true, 3) => vec![e("Add Tri", Command::PatchAddTri), e("Add Quad", Command::PatchAddQuad), e("Delete", Command::PatchDelete)],
            (true, 4 | 5) => vec![e("Extrude", Command::PatchExtrude { amount: mp.patch_extrude, outline: mp.patch_outline }), e("Flip", Command::PatchFlipNormals), e("Delete", Command::PatchDelete)],
            (false, 1) => vec![es("Weld", Command::Weld { threshold: s.poly_settings.weld_threshold }, Command::WeldSettings), e("Collapse", Command::Collapse)],
            (false, 2) => vec![e("Turn", Command::MeshTurnEdge { edges: Vec::new() }), e("Divide", Command::MeshDivide), e("Visible", Command::MeshEdgeVisibility { visible: true }), e("Invisible", Command::MeshEdgeVisibility { visible: false })],
            (false, 3..=5) => vec![
                e("Extrude", Command::MeshExtrude { amount: mp.extrude }),
                e("Bevel", Command::MeshBevel { height: mp.extrude, outline: mp.outline }),
                e("Tessellate", Command::MeshTessellate),
                e("Divide", Command::MeshDivide),
                e("Flip Normals", Command::MeshFlipNormals),
            ],
            _ => Vec::new(),
        };
        t2.extend(ops);
        let t1 = if patch {
            vec![e("Relax", Command::PatchRelax)]
        } else {
            let sub = current > 0;
            vec![e("Attach", Command::Attach), if sub { e("Detach", Command::Detach) } else { off("Detach", "needs a sub-object selection") }, e("Explode", Command::MeshExplode { name: None })]
        };
        (Some(Quadrant::new("tools 1", t1)), Some(Quadrant::new("tools 2", t2)))
    } else if poly_selected {
        let level = s.selection_mode;
        let sub = level != SelectionMode::Object;
        // tools 1: the general Editable Poly tools (upper-left quad).
        let mut t1 = Vec::new();
        if sub {
            t1.push(e("Ignore Backfacing", Command::IgnoreBackfacingInSelections));
            t1.push(e("Hide Unselected", Command::HideUnselected));
            t1.push(e("Hide Selected", Command::Hide));
            t1.push(e("Unhide All", Command::UnhideAll));
            t1.push(e("Collapse", Command::Collapse));
        }
        t1.push(e("Cut", Command::Cut));
        t1.push(e("QuickSlice", Command::QuickSliceMode));
        t1.push(e("Create", Command::Create));
        t1.push(e("Attach", Command::Attach));
        t1.push(if sub { e("Detach", Command::Detach) } else { off("Detach", "needs a sub-object selection") });
        // tools 2: the sub-object levels, then the level's own tools (lower-left quad).
        let mut t2: Vec<Entry> = [
            ("Top-level", SelectionMode::Object),
            ("Vertex", SelectionMode::Vertex),
            ("Edge", SelectionMode::Edge),
            ("Border", SelectionMode::Border),
            ("Polygon", SelectionMode::Face),
            ("Element", SelectionMode::Element),
        ]
        .iter()
        .map(|(l, m)| {
            let label = if *m == level { format!("> {l}") } else { (*l).to_owned() };
            e(&label, Command::SelectionMode { mode: *m })
        })
        .collect();
        let ps = s.poly_settings;
        let ops: Vec<Entry> = match level {
            SelectionMode::Vertex => vec![
                e("Remove", Command::Remove),
                e("Break", Command::Break),
                es("Weld", Command::Weld { threshold: ps.weld_threshold }, Command::WeldSettings),
                e("Target Weld", Command::TargetWeldMode),
                es("Chamfer", Command::ChamferMode, Command::ChamferSettings),
                e("Connect", Command::ConnectVertices),
                es("Extrude", Command::ExtrudeMode, Command::ExtrudeSettings),
            ],
            SelectionMode::Edge => vec![
                e("Loop", Command::SelectEdgeLoop),
                e("Ring", Command::SelectEdgeRing),
                e("Remove", Command::Remove),
                e("Split", Command::SplitEdges),
                es("Chamfer", Command::ChamferMode, Command::ChamferSettings),
                es("Connect", Command::Connect, Command::ConnectEdgeSettings),
                es("Extrude", Command::ExtrudeMode, Command::ExtrudeSettings),
                e("Create Shape", Command::CreateShapeFromEdges { name: None, smooth: s.spline_settings.create_shape_smooth }),
            ],
            SelectionMode::Border => vec![
                e("Cap", Command::Cap),
                es("Bridge", Command::Bridge { segments: ps.bridge_segments, smooth: ps.bridge_smooth, bridge_adjacent: ps.bridge_adjacent, twist1: ps.bridge_twist1, twist2: ps.bridge_twist2 }, Command::BridgeSettings),
                es("Extrude", Command::ExtrudeMode, Command::ExtrudeSettings),
                es("Chamfer", Command::ChamferMode, Command::ChamferSettings),
                es("Connect", Command::Connect, Command::ConnectEdgeSettings),
            ],
            SelectionMode::Face => vec![
                es("Extrude", Command::ExtrudeMode, Command::ExtrudeSettings),
                es("Bevel", Command::BevelMode, Command::BevelSettings),
                es("Inset", Command::InsetMode, Command::InsetSettings),
                es("Outline", Command::OutlineMode, Command::OutlineSettings),
                es("Hinge From Edge", Command::HingeFromEdgeMode, Command::HingeFromEdgeSettings),
                e("Flip Normals", Command::FlipNormals),
                es("Bridge", Command::Bridge { segments: ps.bridge_segments, smooth: ps.bridge_smooth, bridge_adjacent: ps.bridge_adjacent, twist1: ps.bridge_twist1, twist2: ps.bridge_twist2 }, Command::BridgeSettings),
                e("Retriangulate", Command::Retriangulate),
            ],
            SelectionMode::Element => vec![e("Flip Normals", Command::FlipNormals)],
            SelectionMode::Object => Vec::new(),
        };
        t2.extend(ops);
        (Some(Quadrant::new("tools 1", t1)), Some(Quadrant::new("tools 2", t2)))
    } else if s.selection.iter().any(|id| s.scene.node(*id).is_some_and(|n| matches!(n.object, loom_scene::ObjectKind::Grid { .. }))) {
        // A selected grid object: tools 1 activates it or the home grid.
        (Some(Quadrant::new("tools 1", vec![e("Activate Grid", Command::ActivateGridObject), e("Activate Home Grid", Command::ActivateHomeGrid), e("Align Grid to View", Command::AlignGridToView)])), None)
    } else {
        (None, None)
    };
    Quadrants { display, transform, tools1, tools2 }
}

/// Ctrl+right-click: the modeling quad. Standard Primitives and cameras on
/// the right; with an editable object selected, its tools on the left, as in
/// the default quad.
pub fn create_quadrants(app: &LoomApp) -> Quadrants {
    let prims: Vec<Entry> = loom_commands::PrimitiveKind::ALL.iter().map(|k| Entry { label: k.label().to_owned(), action: Action::Primitive(*k), settings: None }).collect();
    let tools = quadrants(app);
    Quadrants {
        display: Quadrant::new("standard primitives", prims),
        transform: Quadrant::new("cameras", vec![e("Target Camera", Command::CreateCamera { target: true }), e("Free Camera", Command::CreateCamera { target: false })]),
        tools1: tools.tools1,
        tools2: tools.tools2,
    }
}

/// Alt+right-click: the animation quad. Set (upper right): Keyframe, Show
/// Trajectories, Delete Selected Animation; Coordinates (lower right): the
/// reference coordinate systems.
pub fn animation_quadrants(app: &LoomApp) -> Quadrants {
    let s = &app.state;
    let set = vec![
        e("Keyframe", Command::SetKeys),
        check("Show Trajectories Toggle", s.show_trajectories, Command::ShowTrajectoriesToggle),
        e("Delete Selected Animation", Command::DeleteSelectedAnimation),
    ];
    let coordinates = loom_commands::CoordSystem::ALL.iter().map(|c| check(c.label(), s.coord_system == *c, Command::ReferenceCoordinateSystem { system: *c })).collect();
    Quadrants { display: Quadrant::new("set", set), transform: Quadrant::new("coordinates", coordinates), tools1: None, tools2: None }
}

/// Ctrl+Alt+right-click: the lighting and render quad. Render (upper
/// right) and the selection's rendering controls (lower right).
pub fn render_quadrants(app: &LoomApp) -> Quadrants {
    let s = &app.state;
    let render = vec![
        e("Render", Command::Render { path: None, width: None, height: None }),
        e("Render Setup...", Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, save_file: None, output: None }),
        e("Rendered Frame Window...", Command::RenderedFrameWindow),
        e("Material Editor...", Command::MaterialEditorToggle),
    ];
    // The first selected object's state: a click sets every selected object to the opposite.
    let first = s.selection.iter().next().and_then(|id| s.scene.node(*id));
    let properties = first.map(|n| {
        ["Renderable", "Visible to Camera", "Receive Shadows", "Cast Shadows"]
            .iter()
            .map(|name| {
                let on = n.render.get(name).unwrap_or(true);
                check(name, on, Command::SetRenderProperty { name: (*name).to_owned(), value: !on })
            })
            .collect()
    });
    Quadrants {
        display: Quadrant::new("render", render),
        transform: Quadrant::new("object properties", properties.unwrap_or_default()),
        tools1: None,
        tools2: None,
    }
}

/// Snaps shortcut menu (Shift+right-click): Snap Override, Snap Toggles,
/// Snap Options.
pub fn snap_quadrants(app: &LoomApp) -> Quadrants {
    use loom_commands::snap::SnapTargets;
    let s = &app.state.snaps;
    let standard: Vec<Entry> = SnapTargets::NAMES.iter().map(|n| e(n, Command::SnapOverride { name: Some((*n).to_owned()) })).collect();
    let mut override_items = vec![Entry { label: "Standard".into(), action: Action::Submenu(standard), settings: None }];
    if s.override_target.is_some() || s.last_override.is_some() {
        override_items.push(e("None", Command::SnapOverride { name: Some("None".into()) }));
    }
    if let Some(last) = &s.last_override {
        override_items.push(e(&format!("Last: {last}"), Command::SnapOverride { name: Some("Last".into()) }));
    }
    let toggles = ["Grid Points", "Pivot", "Vertex", "Edge/Segment", "Midpoint", "Endpoint", "Face"]
        .iter()
        .map(|n| {
            let on = s.targets.get(n).unwrap_or(false);
            check(n, on, Command::SnapTarget { name: (*n).to_owned(), on: !on })
        })
        .collect();
    let options = vec![
        e("Grid and Snap Settings", Command::GridAndSnapSettings),
        check("Snaps To Frozen Objects", s.snap_to_frozen, Command::SnapToFrozenObjectsToggle),
        check("Snaps Use Axis Constraints", s.use_axis_constraints, Command::EnableAxisConstraintsInSnapsToggle),
    ];
    Quadrants {
        display: Quadrant::new("snap override", override_items),
        transform: Quadrant::new("snap options", options),
        tools1: Some(Quadrant::new("snap toggles", toggles)),
        tools2: None,
    }
}

/// Where the menu was opened, if open.
#[derive(Debug, Clone, Copy)]
pub struct Open {
    pub at: egui::Pos2,
    pub viewport: usize,
    /// The press that opened the menu must not also close it.
    pub armed: bool,
    /// Which quad: plain, Shift = snaps, Ctrl = modeling, Alt = animation, Ctrl+Alt = render, or a user set.
    pub kind: QuadKind,
    /// Seconds (egui time) the menu opened at, for the open animation; None until first drawn.
    pub opened: Option<f64>,
    /// Where the right-click was, when the menu was moved to fit the screen (Return Cursor After Repositioned).
    pub origin: Option<egui::Pos2>,
}

impl Open {
    pub fn new(at: egui::Pos2, viewport: usize) -> Self {
        Self { at, viewport, armed: false, kind: QuadKind::Default, opened: None, origin: None }
    }
}

/// Room right of an item for its settings icon.
const SETTINGS_W: f32 = 22.0;

/// How a quadrant is drawn (Advanced Quad Menu Options for its corner).
#[derive(Clone)]
struct Look {
    title: egui::Color32,
    title_bg: egui::Color32,
    background: egui::Color32,
    text: egui::Color32,
    highlight: egui::Color32,
    disabled: egui::Color32,
    border: egui::Color32,
    title_font: egui::FontId,
    menu_font: egui::FontId,
    margin: f32,
    /// Text toward the menu's centre (Mirror Quads on a left quadrant).
    right_align: bool,
    width: f32,
}

fn rgb(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

fn font(family: &str, size: f32) -> egui::FontId {
    egui::FontId::new(size, if family == "Monospace" { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional })
}

/// Full-width rows aligned as the look says, so every entry of a quadrant lines up.
fn row(ui: &mut egui::Ui, look: &Look, label: &str, enabled: bool, last: bool) -> egui::Response {
    let w = ui.available_width() - SETTINGS_W;
    let text = egui::RichText::new(label).font(look.menu_font.clone()).color(if enabled { look.text } else { look.disabled });
    aligned(ui, look, w, |ui| ui.add_enabled(enabled, egui::Button::new(text).min_size(egui::vec2(w, 20.0)).frame(false).selected(last)))
}

/// A button `width` wide whose label sits at its left (or right): a top-down
/// child layout places button text by the layout's horizontal alignment.
fn aligned(ui: &mut egui::Ui, look: &Look, width: f32, add: impl FnOnce(&mut egui::Ui) -> egui::Response) -> egui::Response {
    let align = if look.right_align { egui::Align::Max } else { egui::Align::Min };
    ui.allocate_ui_with_layout(egui::vec2(width, 20.0), egui::Layout::top_down(align), add).inner
}

/// A command row: its chord right-aligned and in the hover text; `last`
/// highlights the quad's last command.
fn cmd_row(ui: &mut egui::Ui, state: &loom_commands::AppState, look: &Look, label: &str, action: &str, width: f32, last: bool) -> egui::Response {
    let text = egui::RichText::new(label).font(look.menu_font.clone()).color(look.text);
    let mut b = egui::Button::new(text).min_size(egui::vec2(width, 20.0)).frame(false).selected(last);
    if let Some(c) = crate::shortcuts::chord(state, action) {
        b = b.shortcut_text(c.clone());
    }
    let r = aligned(ui, look, width, |ui| ui.add(b));
    match crate::shortcuts::chord(state, action) {
        Some(c) => r.on_hover_text(format!("{label} ({c})")),
        None => r,
    }
}

/// Runs an item; false for items that run nothing (submenus, disabled).
fn run(app: &mut LoomApp, action: &Action) -> bool {
    match action {
        Action::Command(cmd) => {
            let _ = app.dispatch(cmd.clone());
            true
        }
        Action::Named(name) => {
            app.dispatch_named(name);
            true
        }
        Action::Primitive(kind) => {
            app.create_kind = Some(crate::CreateKind::Primitive(*kind));
            app.create_category = crate::Category::Geometry;
            app.panel_tab = crate::Tab::Create;
            true
        }
        Action::Submenu(_) | Action::Disabled(_) | Action::Menu(_) | Action::Separator => false,
    }
}

/// The settings icon of `item`: Transform Type-In picks the item's tool and
/// opens the dialog (never closes it); the others open their settings window.
fn run_settings(app: &mut LoomApp, item: &Entry, settings: &Command) {
    if matches!(settings, Command::TransformTypeInDialogToggle) {
        run(app, &item.action);
        if app.state.dialog != Some(loom_commands::Dialog::TransformTypeIn) {
            let _ = app.dispatch(Command::TransformTypeInDialogToggle);
        }
    } else {
        let _ = app.dispatch(settings.clone());
    }
}

/// The quadrant's title bar; a click repeats the quadrant's last command.
fn title_bar(ui: &mut egui::Ui, app: &mut LoomApp, kind: QuadKind, q: &Quadrant, look: &Look, close: &mut bool) {
    let last = app.quad_last.clone().filter(|l| l.kind == kind && l.quadrant == q.title);
    let text = egui::RichText::new(q.shown_title()).font(look.title_font.clone()).color(look.title);
    let w = ui.available_width();
    let title = egui::Frame::NONE.fill(look.title_bg).show(ui, |ui| aligned(ui, look, w, |ui| ui.add(egui::Button::new(text).frame(false)))).inner;
    if let Some(l) = &last {
        if title.on_hover_text(format!("Repeat {}", l.label.trim_start_matches(['\u{2714}', ' ']))).clicked() && run(app, &l.action) {
            *close = true;
        }
    }
}

fn quadrant_ui(ui: &mut egui::Ui, app: &mut LoomApp, kind: QuadKind, q: &Quadrant, look: &Look, close: &mut bool, picked_cmd: &mut bool) {
    ui.spacing_mut().item_spacing.y = look.margin;
    ui.spacing_mut().button_padding = egui::vec2(8.0, 2.0);
    let v = ui.visuals_mut();
    v.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    v.widgets.inactive.fg_stroke.color = look.text;
    v.widgets.hovered.fg_stroke.color = look.highlight;
    v.widgets.active.fg_stroke.color = look.highlight;
    v.widgets.noninteractive.fg_stroke.color = look.text;
    // The last command picked from this quadrant: the title repeats it.
    let last = app.quad_last.clone().filter(|l| l.kind == kind && l.quadrant == q.title);
    title_bar(ui, app, kind, q, look, close);
    ui.separator();
    let picked = |label: &str| last.as_ref().is_some_and(|l| l.label == label);
    for item in &q.items {
        if matches!(item.action, Action::Separator) {
            ui.separator();
            continue;
        }
        let width = ui.available_width() - SETTINGS_W;
        // Cross-aligned to the left (not ui.horizontal's centre): button labels stay left-aligned.
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            match &item.action {
                Action::Command(cmd) => {
                    if cmd_row(ui, &app.state, look, &item.label, crate::shortcuts::action_of(cmd), width, picked(&item.label)).clicked() {
                        run(app, &item.action);
                        app.quad_last = Some(Last { kind, quadrant: q.title, label: item.label.clone(), action: item.action.clone() });
                        *close = true;
                        *picked_cmd = true;
                    }
                }
                Action::Named(name) => {
                    if cmd_row(ui, &app.state, look, &item.label, name, width, picked(&item.label)).clicked() {
                        run(app, &item.action);
                        app.quad_last = Some(Last { kind, quadrant: q.title, label: item.label.clone(), action: item.action.clone() });
                        *close = true;
                        *picked_cmd = true;
                    }
                }
                Action::Menu(custom) => {
                    // A menu the user put in the quadrant: its entries in a submenu.
                    let before = app.state.undo.version();
                    crate::menubar::custom_items(ui, app, std::slice::from_ref(custom));
                    if app.state.undo.version() != before {
                        *close = true;
                    }
                }
                Action::Separator => {}
                Action::Submenu(children) => {
                    ui.menu_button(format!("{}  {}", item.label, crate::icons::SUBMENU), |ui| {
                        ui.set_min_width(170.0);
                        for c in children {
                            if let Action::Command(cmd) = &c.action {
                                if cmd_row(ui, &app.state, look, &c.label, crate::shortcuts::action_of(cmd), ui.available_width(), false).clicked() {
                                    run(app, &c.action);
                                    app.quad_last = Some(Last { kind, quadrant: q.title, label: c.label.clone(), action: c.action.clone() });
                                    ui.close_menu();
                                    *picked_cmd = true;
                                }
                            }
                        }
                    });
                }
                Action::Disabled(why) => {
                    row(ui, look, &item.label, false, false).on_disabled_hover_text(why);
                }
                Action::Primitive(_) => {
                    if row(ui, look, &item.label, true, picked(&item.label)).clicked() {
                        run(app, &item.action);
                        app.quad_last = Some(Last { kind, quadrant: q.title, label: item.label.clone(), action: item.action.clone() });
                        *close = true;
                        *picked_cmd = true;
                    }
                }
            }
            if let Some(settings) = &item.settings {
                if crate::icons::small(ui, crate::icons::SETTINGS, &format!("{} Settings", item.label)).clicked() {
                    run_settings(app, item, settings);
                    *close = true;
                }
            }
        });
    }
}

/// The program's quadrants of quad `kind` (a user set has none of its own).
pub fn program_quadrants(app: &LoomApp, kind: QuadKind) -> Quadrants {
    match kind {
        QuadKind::Snaps => snap_quadrants(app),
        QuadKind::Create => create_quadrants(app),
        QuadKind::Animation => animation_quadrants(app),
        QuadKind::Render => render_quadrants(app),
        QuadKind::Default => quadrants(app),
        QuadKind::User(_) => Quadrants { display: Quadrant::new("", Vec::new()), transform: Quadrant::new("", Vec::new()), tools1: None, tools2: None },
    }
}

/// The quadrants as the user customized them: their labels, without the
/// entries deleted in the Quads panel, with the added entries at the end.
pub fn customized_quadrants(app: &LoomApp, kind: QuadKind) -> Quadrants {
    let mut q = program_quadrants(app, kind);
    let set = kind.set_name(app);
    let custom = app.state.ui_custom.quad(&set);
    let apply = |quad: &mut Quadrant, i: usize| {
        if !custom.labels[i].is_empty() {
            quad.label = Some(custom.labels[i].clone());
        } else if quad.title.is_empty() {
            quad.label = Some(loom_commands::ui_custom::QUADRANTS[i].to_lowercase());
        }
        quad.items.retain(|e| !custom.hidden[i].iter().any(|h| h == plain_label(&e.label)));
        for it in &custom.items[i] {
            let action = if it.is_separator() {
                Action::Separator
            } else if it.menu().is_some() {
                Action::Menu(it.clone())
            } else {
                Action::Named(it.item.clone())
            };
            let label = loom_commands::ui_custom::accelerator(it.label()).0.trim().to_owned();
            quad.items.push(Entry { label, action, settings: None });
        }
    };
    apply(&mut q.display, 0);
    apply(&mut q.transform, 1);
    for (slot, i) in [(&mut q.tools1, 2usize), (&mut q.tools2, 3)] {
        let mut quad = slot.take().unwrap_or_else(|| Quadrant::new("", Vec::new()));
        if quad.title.is_empty() {
            quad.title = if i == 2 { "tools 1" } else { "tools 2" };
        }
        apply(&mut quad, i);
        // A quadrant with nothing in it stays hidden, as before.
        *slot = (!quad.items.is_empty()).then_some(quad);
    }
    if q.display.title.is_empty() {
        q.display.title = "display";
    }
    if q.transform.title.is_empty() {
        q.transform.title = "transform";
    }
    q
}

/// The quadrant under the pointer, relative to the menu's centre (0 upper
/// right, 1 lower right, 2 upper left, 3 lower left).
fn quadrant_at(center: egui::Pos2, p: egui::Pos2) -> usize {
    match (p.x >= center.x, p.y < center.y) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    }
}

/// Draws the four quadrants around `open.at`. Returns true when it should close.
pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: Open) -> bool {
    let q = customized_quadrants(app, open.kind);
    let set = open.kind.set_name(app);
    let show_all = app.state.ui_custom.quad(&set).show_all;
    let o = app.state.ui_custom.quad_options.clone();
    let now = ctx.input(|i| i.time);
    let opened = open.opened.unwrap_or(now);
    // Quadrant width: each fits its widest entry; Uniform Quad Width makes them all the widest one's.
    let menu_font = font(&o.menu_font, o.menu_size);
    let title_font = font(&o.title_font, o.title_size);
    let text_w = |s: &str, f: &egui::FontId| ctx.fonts(|fonts| fonts.layout_no_wrap(s.to_owned(), f.clone(), egui::Color32::WHITE).size().x);
    let quads: [Option<&Quadrant>; 4] = [Some(&q.display), Some(&q.transform), q.tools1.as_ref(), q.tools2.as_ref()];
    let widths: Vec<f32> = quads
        .iter()
        .map(|quad| {
            quad.map(|quad| {
                let items = quad.items.iter().map(|e| text_w(&e.label, &menu_font) + 60.0).fold(0.0f32, f32::max);
                items.max(text_w(quad.shown_title(), &title_font) + 20.0).max(120.0) + SETTINGS_W
            })
            .unwrap_or(0.0)
        })
        .collect();
    let widest = widths.iter().copied().fold(0.0f32, f32::max).max(170.0 + SETTINGS_W);
    let width = |i: usize| if o.uniform_width { widest } else { widths[i].max(120.0 + SETTINGS_W) };
    let gap = 4.0;
    // Reposition When Off Screen: the menu moves so every quadrant fits (heights from the last frame).
    let mut at = open.at;
    if o.reposition_off_screen {
        let screen = ctx.screen_rect();
        let h = |id: &str| ctx.memory(|m| m.area_rect(egui::Id::new(("quad", id)))).map(|r| r.height()).unwrap_or(0.0);
        let up = h("display").max(h("tools1")) + gap;
        let down = h("transform").max(h("tools2")) + gap;
        let left = if q.tools1.is_some() || q.tools2.is_some() { width(2).max(width(3)) + gap } else { 0.0 };
        let right = width(0).max(width(1)) + gap;
        at.x = at.x.clamp(screen.min.x + left, (screen.max.x - right).max(screen.min.x + left));
        at.y = at.y.clamp(screen.min.y + up, (screen.max.y - down).max(screen.min.y + up));
    }
    if open.opened.is_none() {
        // First frame: remember the time; a moved menu takes the cursor along when asked to.
        let moved = (at - open.at).length() > 0.5;
        if let Some(m) = app.quad_menu.as_mut() {
            m.opened = Some(now);
            if moved {
                m.origin = Some(open.at);
                m.at = at;
            }
        }
        if moved && o.move_cursor_when_repositioned {
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorPosition(at));
        }
    }
    // Animation: Fade ramps the opacity, Stretch opens one quadrant after another clockwise.
    let total = (o.steps.max(1) as f64) * (o.pause as f64) / 1000.0;
    let t = if total > 0.0 { ((now - opened) / total).clamp(0.0, 1.0) as f32 } else { 1.0 };
    if t < 1.0 && o.animation != "None" {
        ctx.request_repaint();
    }
    let fade = if o.animation == "Fade" { t } else { 1.0 };
    let stretch_order = [0usize, 1, 3, 2];
    let stretched = |i: usize| o.animation != "Stretch" || t >= stretch_order.iter().position(|q| *q == i).unwrap_or(0) as f32 / 4.0;
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    // Show All Quads off: only the quadrant toward the pointer (the starting quadrant until it moves).
    let active = pointer.filter(|p| (*p - at).length() > gap).map(|p| quadrant_at(at, p)).unwrap_or(o.starting_quadrant.min(3) as usize);
    let mut close = false;
    let mut picked_cmd = false;
    let mut place = |ctx: &egui::Context, app: &mut LoomApp, i: usize, id: &str, quad: &Quadrant, pivot: egui::Align2| {
        if quad.items.is_empty() || !stretched(i) {
            return; // a quadrant with nothing to show is hidden
        }
        let full = show_all || i == active;
        let look = Look {
            title: rgb(o.color("Title Bar Text", i)),
            title_bg: rgb(o.color("Title Bar Background", i)),
            background: rgb(o.color("Background", i)),
            text: rgb(o.color("Text", i)),
            highlight: rgb(o.color("Highlighted Text", i)),
            disabled: rgb(o.color("Disabled Text", i)),
            border: rgb(o.color("Border", i)),
            title_font: title_font.clone(),
            menu_font: menu_font.clone(),
            margin: o.vertical_margins,
            right_align: o.mirror && i >= 2,
            width: width(i),
        };
        let area = egui::Area::new(egui::Id::new(("quad", id)))
            .order(egui::Order::Foreground)
            .pivot(pivot)
            .fixed_pos(at + match pivot {
                egui::Align2::RIGHT_BOTTOM => egui::vec2(-gap, -gap),
                egui::Align2::LEFT_BOTTOM => egui::vec2(gap, -gap),
                egui::Align2::RIGHT_TOP => egui::vec2(-gap, gap),
                _ => egui::vec2(gap, gap),
            });
        area.show(ctx, |ui| {
            ui.set_opacity((o.opacity / 100.0).clamp(0.1, 1.0) * fade);
            let frame = egui::Frame::popup(ui.style()).fill(look.background).stroke(egui::Stroke::new(1.0, look.border));
            frame.show(ui, |ui| {
                ui.set_min_width(look.width);
                ui.set_max_width(look.width);
                if full {
                    quadrant_ui(ui, app, open.kind, quad, &look, &mut close, &mut picked_cmd);
                } else {
                    // Only the title shows; pointing toward it opens it.
                    title_bar(ui, app, open.kind, quad, &look, &mut close);
                }
            });
        });
    };
    // Display upper-right, transform lower-right, tools 1 upper-left, tools 2 lower-left.
    place(ctx, app, 0, "display", &q.display, egui::Align2::LEFT_BOTTOM);
    place(ctx, app, 1, "transform", &q.transform, egui::Align2::LEFT_TOP);
    if let Some(t1) = &q.tools1 {
        place(ctx, app, 2, "tools1", t1, egui::Align2::RIGHT_BOTTOM);
    }
    if let Some(t2) = &q.tools2 {
        place(ctx, app, 3, "tools2", t2, egui::Align2::RIGHT_TOP);
    }
    // Return Cursor After Repositioned: the cursor goes back where the right-click was.
    if picked_cmd && o.return_cursor_after {
        if let Some(origin) = app.quad_menu.and_then(|m| m.origin) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorPosition(origin));
        }
    }
    // Submenus are their own areas, so test "over any menu area" rather than our panels
    // (not "over any area": the panels and the viewports' central panel count as areas too).
    let over_menu = ctx.input(|i| i.pointer.interact_pos()).and_then(|p| ctx.layer_id_at(p)).is_some_and(|l| l.order == egui::Order::Foreground);
    let clicked_outside = open.armed && ctx.input(|i| i.pointer.any_pressed()) && !over_menu;
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if !open.armed {
        if let Some(o) = app.quad_menu.as_mut() {
            o.armed = true;
        }
    }
    close || clicked_outside || escape
}

/// A Quad Shortcut or Show Quad Menu opens that set at the pointer, in the
/// viewport under it (the active one when the pointer is elsewhere).
pub fn open_requested(ctx: &egui::Context, app: &mut LoomApp) {
    let Some(set) = app.state.quad_request.take() else { return };
    let Some(kind) = QuadKind::of_set(app, &set) else { return };
    let p = ctx.input(|i| i.pointer.hover_pos()).unwrap_or_else(|| app.viewport_rects.get(app.state.active_viewport).map(|r| r.center()).unwrap_or(egui::pos2(400.0, 300.0)));
    let viewport = app.viewport_rects.iter().position(|r| r.contains(p)).unwrap_or(app.state.active_viewport);
    let mut open = Open::new(p, viewport);
    open.kind = kind;
    app.quad_menu = Some(open);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadrants_match_defaults() {
        let mut app = LoomApp::headless();
        let q = quadrants(&app);
        assert_eq!(q.display.title, "display");
        assert_eq!(q.transform.title, "transform");
        let display: Vec<&str> = q.display.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(display, vec!["Isolate Selection", "Unfreeze All", "Freeze Selection", "Unhide by Name", "Unhide All", "Hide Unselected", "Hide Selection"]);
        let transform: Vec<&str> = q.transform.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(transform, vec!["Move", "Rotate", "Scale", "Select", "Clone"], "nothing selected: no object items");
        assert!(q.tools1.is_none() && q.tools2.is_none(), "no Editable Poly selected: left quadrants hidden");
        let _ = loom_commands::execute(&mut app.state, loom_commands::Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] });
        let transform: Vec<String> = quadrants(&app).transform.items.iter().map(|i| i.label.clone()).collect();
        assert_eq!(transform, ["Move", "Rotate", "Scale", "Select", "Select Similar", "Clone", "Object Properties...", "Curve Editor...", "Dope Sheet...", "Convert To"]);
        let _ = loom_commands::execute(&mut app.state, loom_commands::Command::ConvertToEditablePoly);
        let q = quadrants(&app);
        let tools2: Vec<&str> = q.tools2.as_ref().unwrap().items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(tools2, vec!["> Top-level", "Vertex", "Edge", "Border", "Polygon", "Element"]);
        assert_eq!(q.tools1.as_ref().unwrap().title, "tools 1");
        let tools1: Vec<&str> = q.tools1.as_ref().unwrap().items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(tools1, vec!["Cut", "QuickSlice", "Create", "Attach", "Detach"]);
    }

    #[test]
    fn tools2_lists_the_level_tools() {
        let mut app = LoomApp::headless();
        let _ = loom_commands::execute(&mut app.state, loom_commands::Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] });
        let _ = loom_commands::execute(&mut app.state, loom_commands::Command::ConvertToEditablePoly);
        let expect: [(SelectionMode, &[&str]); 5] = [
            (SelectionMode::Vertex, &["Remove", "Break", "Weld", "Target Weld", "Chamfer", "Connect", "Extrude"]),
            (SelectionMode::Edge, &["Loop", "Ring", "Remove", "Split", "Chamfer", "Connect", "Extrude", "Create Shape"]),
            (SelectionMode::Border, &["Cap", "Bridge", "Extrude", "Chamfer", "Connect"]),
            (SelectionMode::Face, &["Extrude", "Bevel", "Inset", "Outline", "Hinge From Edge", "Flip Normals", "Bridge", "Retriangulate"]),
            (SelectionMode::Element, &["Flip Normals"]),
        ];
        for (mode, ops) in expect {
            let _ = loom_commands::execute(&mut app.state, loom_commands::Command::SelectionMode { mode });
            let q = quadrants(&app);
            let items: Vec<&str> = q.tools2.as_ref().unwrap().items.iter().skip(6).map(|i| i.label.as_str()).collect();
            assert_eq!(items, ops, "{mode:?}");
            let t1: Vec<&str> = q.tools1.as_ref().unwrap().items.iter().map(|i| i.label.as_str()).collect();
            assert_eq!(t1[0], "Ignore Backfacing");
            assert!(t1.contains(&"Hide Unselected") && t1.contains(&"Collapse"));
        }
        let create = create_quadrants(&app);
        assert_eq!(create.display.items.len(), loom_commands::PrimitiveKind::ALL.len());
        assert!(create.tools1.is_some() && create.tools2.is_some(), "the modeling quad has the Editable Poly tools");
    }

    fn labels(q: &Quadrant) -> Vec<String> {
        q.items.iter().map(|i| i.label.trim_start_matches(['✔', ' ']).to_owned()).collect()
    }

    #[test]
    fn modifiers_pick_the_quad() {
        let m = |shift, ctrl, alt| loom_input::Modifiers { alt, ctrl, shift };
        let cases = [(m(false, false, false), QuadKind::Default), (m(true, false, false), QuadKind::Snaps), (m(false, true, false), QuadKind::Create), (m(false, false, true), QuadKind::Animation), (m(false, true, true), QuadKind::Render)];
        for (m, kind) in cases {
            assert_eq!(QuadKind::for_modifiers(m), kind, "{m:?}");
        }
    }

    #[test]
    fn animation_quad_has_set_and_coordinates() {
        let mut app = LoomApp::headless();
        let q = animation_quadrants(&app);
        assert_eq!((q.display.title, q.transform.title), ("set", "coordinates"));
        assert_eq!(labels(&q.display), ["Keyframe", "Show Trajectories Toggle", "Delete Selected Animation"]);
        assert_eq!(labels(&q.transform), loom_commands::CoordSystem::ALL.map(|c| c.label().to_owned()));
        assert!(q.transform.items[0].label.starts_with('✔'), "View, the current system, is checked");
        let _ = loom_commands::execute(&mut app.state, Command::ReferenceCoordinateSystem { system: loom_commands::CoordSystem::Local });
        let q = animation_quadrants(&app);
        let checked: Vec<String> = q.transform.items.iter().filter(|i| i.label.starts_with('✔')).map(|i| i.label.clone()).collect();
        assert_eq!(checked, ["✔ Local"]);
    }

    #[test]
    fn render_quad_renders_and_toggles_the_selections_rendering_controls() {
        let mut app = LoomApp::headless();
        let q = render_quadrants(&app);
        assert_eq!(labels(&q.display), ["Render", "Render Setup...", "Rendered Frame Window...", "Material Editor..."]);
        assert!(q.transform.items.is_empty(), "nothing selected: no object properties");
        let _ = loom_commands::execute(&mut app.state, Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] });
        let q = render_quadrants(&app);
        assert_eq!(labels(&q.transform), ["Renderable", "Visible to Camera", "Receive Shadows", "Cast Shadows"]);
        let Action::Command(c) = q.transform.items[3].action.clone() else { panic!("a command") };
        let _ = loom_commands::execute(&mut app.state, c);
        let id = *app.state.selection.iter().next().unwrap();
        assert!(!app.state.scene.node(id).unwrap().render.cast_shadows, "Cast Shadows turned off");
        assert!(!render_quadrants(&app).transform.items[3].label.starts_with('✔'));
    }
}

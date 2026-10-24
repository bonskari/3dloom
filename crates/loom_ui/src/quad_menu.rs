//! The default quad menu: display (upper right), transform (lower
//! right), tools 1 (upper left) and tools 2 (lower left) for Editable Poly.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectionMode};

#[derive(Debug, Clone)]
pub struct Entry {
    pub label: String,
    pub action: Action,
}

#[derive(Debug, Clone)]
pub enum Action {
    Command(Command),
    Submenu(Vec<Entry>),
    /// Greyed out, with the reason as tooltip.
    Disabled(String),
    /// Start creating a Standard Primitive (Ctrl+right-click quad).
    Primitive(loom_commands::PrimitiveKind),
}

/// Which quad menu a right-click opens: plain, Shift (snaps), Ctrl
/// (creation), Alt (animation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuadKind {
    #[default]
    Default,
    Snaps,
    Create,
    Animation,
}

#[derive(Debug, Clone)]
pub struct Quadrant {
    pub title: &'static str,
    pub items: Vec<Entry>,
}

pub struct Quadrants {
    pub display: Quadrant,
    pub transform: Quadrant,
    pub tools1: Option<Quadrant>,
    pub tools2: Option<Quadrant>,
}

fn e(label: &str, cmd: Command) -> Entry {
    Entry { label: label.to_owned(), action: Action::Command(cmd) }
}

fn off(label: &str, why: &str) -> Entry {
    Entry { label: label.to_owned(), action: Action::Disabled(why.to_owned()) }
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
    };
    let transform = Quadrant {
        title: "transform",
        items: vec![
            e("Move", Command::SelectAndMove),
            e("Rotate", Command::SelectAndRotate),
            e("Scale", Command::SelectAndScale),
            e("Select", Command::SelectObject),
            e("Clone", Command::Clone),
            e("Object Properties...", Command::ObjectProperties),
            Entry { label: "Convert To".into(), action: Action::Submenu(vec![e("Convert to Editable Poly", Command::ConvertToEditablePoly), e("Convert to Editable Spline", Command::ConvertToEditableSpline)]) },
        ],
    };
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
        (!levels.is_empty() && n.edit_poly().filter(|_| n.editing_entry().is_some()).is_none() && n.editing_spline_entry().is_none()).then(|| (a, slot.sub_level, levels, slot.name.clone()))
    });
    let (tools1, tools2) = if let Some((index, current, levels, name)) = modifier_levels {
        let mut t1: Vec<Entry> = vec![e(if current == 0 { "> Top-level" } else { "Top-level" }, Command::ModifierSubobjectLevel { index: index as u32, level: 0 })];
        for (i, l) in levels.iter().enumerate() {
            let lvl = i as u32 + 1;
            let label = if current == lvl { format!("> {l}") } else { (*l).to_owned() };
            t1.push(e(&label, Command::ModifierSubobjectLevel { index: index as u32, level: lvl }));
        }
        (Some(Quadrant { title: "tools 1", items: t1 }), Some(Quadrant { title: "tools 2", items: vec![e(&format!("Collapse To {name}"), Command::CollapseTo { index: index as u32 }), e("Make Unique", Command::MakeUnique), e("Delete Modifier", Command::DeleteModifier { index: index as u32 })] }))
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
        (Some(Quadrant { title: "tools 1", items: t1 }), Some(Quadrant { title: "tools 2", items: t2 }))
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
                e("Weld", Command::Weld { threshold: ps.weld_threshold }),
                e("Target Weld", Command::TargetWeldMode),
                e("Chamfer", Command::ChamferMode),
                e("Connect", Command::ConnectVertices),
                e("Extrude", Command::ExtrudeMode),
            ],
            SelectionMode::Edge => vec![
                e("Loop", Command::SelectEdgeLoop),
                e("Ring", Command::SelectEdgeRing),
                e("Remove", Command::Remove),
                e("Split", Command::SplitEdges),
                e("Chamfer", Command::ChamferMode),
                e("Connect", Command::Connect),
                e("Extrude", Command::ExtrudeMode),
                e("Create Shape", Command::CreateShapeFromEdges { name: None, smooth: s.spline_settings.create_shape_smooth }),
            ],
            SelectionMode::Border => vec![
                e("Cap", Command::Cap),
                e("Bridge", Command::Bridge { segments: ps.bridge_segments, smooth: ps.bridge_smooth, bridge_adjacent: ps.bridge_adjacent, twist1: ps.bridge_twist1, twist2: ps.bridge_twist2 }),
                e("Extrude", Command::ExtrudeMode),
                e("Chamfer", Command::ChamferMode),
                e("Connect", Command::Connect),
            ],
            SelectionMode::Face => vec![
                e("Extrude", Command::ExtrudeMode),
                e("Bevel", Command::BevelMode),
                e("Inset", Command::InsetMode),
                e("Outline", Command::OutlineMode),
                e("Hinge From Edge", Command::HingeFromEdgeMode),
                e("Flip Normals", Command::FlipNormals),
                e("Bridge", Command::Bridge { segments: ps.bridge_segments, smooth: ps.bridge_smooth, bridge_adjacent: ps.bridge_adjacent, twist1: ps.bridge_twist1, twist2: ps.bridge_twist2 }),
                e("Retriangulate", Command::Retriangulate),
            ],
            SelectionMode::Element => vec![e("Flip Normals", Command::FlipNormals)],
            SelectionMode::Object => Vec::new(),
        };
        t2.extend(ops);
        (Some(Quadrant { title: "tools 1", items: t1 }), Some(Quadrant { title: "tools 2", items: t2 }))
    } else {
        (None, None)
    };
    Quadrants { display, transform, tools1, tools2 }
}

/// Ctrl+right-click: the creation quad (Standard Primitives).
pub fn create_quadrants() -> Quadrants {
    let prims: Vec<Entry> = loom_commands::PrimitiveKind::ALL.iter().map(|k| Entry { label: k.label().to_owned(), action: Action::Primitive(*k) }).collect();
    Quadrants {
        display: Quadrant { title: "standard primitives", items: prims },
        transform: Quadrant { title: "cameras", items: vec![e("Target Camera", Command::CreateCamera { target: true }), e("Free Camera", Command::CreateCamera { target: false })] },
        tools1: None,
        tools2: None,
    }
}

/// Alt+right-click: the animation quad (default); keys arrive in phase 8.
pub fn animation_quadrants() -> Quadrants {
    Quadrants {
        display: Quadrant { title: "animation", items: vec![off("Set Key Mode", "animation keys arrive in phase 8"), off("Set Keys", "animation keys arrive in phase 8"), off("Add Key", "animation keys arrive in phase 8")] },
        transform: Quadrant { title: "time", items: vec![e("Time Configuration...", Command::TimeConfiguration)] },
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
    let mut override_items = vec![Entry { label: "Standard".into(), action: Action::Submenu(standard) }];
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
            e(&format!("{} {n}", if on { "[x]" } else { "[  ]" }), Command::SnapTarget { name: (*n).to_owned(), on: !on })
        })
        .collect();
    let options = vec![
        e("Grid and Snap Settings", Command::GridAndSnapSettings),
        e(&format!("{} Snaps To Frozen Objects", if s.snap_to_frozen { "[x]" } else { "[  ]" }), Command::SnapToFrozenObjectsToggle),
        e(&format!("{} Snaps Use Axis Constraints", if s.use_axis_constraints { "[x]" } else { "[  ]" }), Command::EnableAxisConstraintsInSnapsToggle),
    ];
    Quadrants {
        display: Quadrant { title: "snap override", items: override_items },
        transform: Quadrant { title: "snap options", items: options },
        tools1: Some(Quadrant { title: "snap toggles", items: toggles }),
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
    /// Which quad: plain, Shift = snaps, Ctrl = creation, Alt = animation.
    pub kind: QuadKind,
}

impl Open {
    pub fn new(at: egui::Pos2, viewport: usize) -> Self {
        Self { at, viewport, armed: false, kind: QuadKind::Default }
    }
}

/// Full-width, left-aligned rows so every entry of a quadrant lines up.
fn row(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let w = ui.available_width();
    ui.add_enabled(enabled, egui::Button::new(label).min_size(egui::vec2(w, 20.0)).frame(false))
}

/// A command row: its chord right-aligned and in the hover text.
fn cmd_row(ui: &mut egui::Ui, state: &loom_commands::AppState, label: &str, cmd: &Command) -> egui::Response {
    let w = ui.available_width();
    let action = crate::shortcuts::action_of(cmd);
    let r = ui.add(crate::shortcuts::menu_widget(state, label, action).min_size(egui::vec2(w, 20.0)).frame(false));
    match crate::shortcuts::chord(state, action) {
        Some(c) => r.on_hover_text(format!("{label} ({c})")),
        None => r,
    }
}

fn quadrant_ui(ui: &mut egui::Ui, app: &mut LoomApp, q: &Quadrant, close: &mut bool) {
    ui.spacing_mut().item_spacing.y = 1.0;
    ui.spacing_mut().button_padding = egui::vec2(8.0, 2.0);
    ui.style_mut().visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    ui.label(egui::RichText::new(q.title).small().color(egui::Color32::from_rgb(255, 200, 80)));
    ui.separator();
    for item in &q.items {
        match &item.action {
            Action::Command(cmd) => {
                if cmd_row(ui, &app.state, &item.label, cmd).clicked() {
                    let _ = app.dispatch(cmd.clone());
                    *close = true;
                }
            }
            Action::Submenu(children) => {
                ui.menu_button(format!("{}  ▸", item.label), |ui| {
                    ui.set_min_width(170.0);
                    for c in children {
                        if let Action::Command(cmd) = &c.action {
                            if cmd_row(ui, &app.state, &c.label, cmd).clicked() {
                                let _ = app.dispatch(cmd.clone());
                                ui.close_menu();
                            }
                        }
                    }
                });
            }
            Action::Disabled(why) => {
                row(ui, &item.label, false).on_disabled_hover_text(why);
            }
            Action::Primitive(kind) => {
                if row(ui, &item.label, true).clicked() {
                    app.create_kind = Some(crate::CreateKind::Primitive(*kind));
                    app.create_category = crate::Category::Geometry;
                    app.panel_tab = crate::Tab::Create;
                    *close = true;
                }
            }
        }
    }
}

/// Draws the four quadrants around `open.at`. Returns true when it should close.
pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: Open) -> bool {
    let q = match open.kind {
        QuadKind::Snaps => snap_quadrants(app),
        QuadKind::Create => create_quadrants(),
        QuadKind::Animation => animation_quadrants(),
        QuadKind::Default => quadrants(app),
    };
    let mut close = false;
    let gap = 4.0;
    let mut any_hover = false;
    let mut place = |ctx: &egui::Context, app: &mut LoomApp, id: &str, quad: &Quadrant, pivot: egui::Align2| {
        let area = egui::Area::new(egui::Id::new(("quad", id)))
            .order(egui::Order::Foreground)
            .pivot(pivot)
            .fixed_pos(open.at + match pivot {
                egui::Align2::RIGHT_BOTTOM => egui::vec2(-gap, -gap),
                egui::Align2::LEFT_BOTTOM => egui::vec2(gap, -gap),
                egui::Align2::RIGHT_TOP => egui::vec2(-gap, gap),
                _ => egui::vec2(gap, gap),
            });
        let r = area.show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(170.0);
                ui.set_max_width(170.0);
                quadrant_ui(ui, app, quad, &mut close);
            });
        });
        any_hover |= r.response.contains_pointer();
    };
    // Display upper-right, transform lower-right, tools 1 upper-left, tools 2 lower-left.
    place(ctx, app, "display", &q.display, egui::Align2::LEFT_BOTTOM);
    place(ctx, app, "transform", &q.transform, egui::Align2::LEFT_TOP);
    if let Some(t1) = &q.tools1 {
        place(ctx, app, "tools1", t1, egui::Align2::RIGHT_BOTTOM);
    }
    if let Some(t2) = &q.tools2 {
        place(ctx, app, "tools2", t2, egui::Align2::RIGHT_TOP);
    }
    // Submenus are their own areas, so test "over any area" rather than our panels.
    let clicked_outside = open.armed && ctx.input(|i| i.pointer.any_pressed()) && !ctx.is_pointer_over_area();
    let _ = any_hover;
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if !open.armed {
        if let Some(o) = app.quad_menu.as_mut() {
            o.armed = true;
        }
    }
    close || clicked_outside || escape
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
        assert_eq!(transform, vec!["Move", "Rotate", "Scale", "Select", "Clone", "Object Properties...", "Convert To"]);
        assert!(q.tools1.is_none() && q.tools2.is_none(), "no Editable Poly selected: left quadrants hidden");
        let _ = loom_commands::execute(&mut app.state, loom_commands::Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] });
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
        assert_eq!(create_quadrants().display.items.len(), loom_commands::PrimitiveKind::ALL.len());
        assert!(matches!(animation_quadrants().display.items[0].action, Action::Disabled(_)));
    }
}

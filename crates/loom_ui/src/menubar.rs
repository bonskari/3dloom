//! The menu bar with the items that exist. Items of later
//! plans are added by those plans (no disabled placeholders).

use crate::LoomApp;
use eframe::egui;
use crate::shortcuts::{menu_button, HoverShortcut};
use loom_commands::{Command, PrimitiveKind};

fn item(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, action: &str) {
    if menu_button(ui, &app.state, label, action).clicked() {
        app.dispatch_named(action);
        ui.close_menu();
    }
}

/// A toggle item: a check mark shows its state (as in the viewport label menus).
fn check_item(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, action: &str, on: bool) {
    let label = if on { format!("✔ {label}") } else { format!("    {label}") };
    item(ui, app, &label, action);
}

fn cmd(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, c: Command) {
    if menu_button(ui, &app.state, label, c.name()).clicked() {
        let _ = app.dispatch(c);
        ui.close_menu();
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.expert_mode {
        return;
    }
    egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                use loom_commands::PendingFileAction as A;
                if menu_button(ui, &app.state, "New", "New Scene").clicked() {
                    app.new_scene();
                    ui.close_menu();
                }
                if ui.button("Reset").clicked() {
                    app.request_file_action(A::Reset);
                    ui.close_menu();
                }
                ui.separator();
                if menu_button(ui, &app.state, "Open...", "Open File").clicked() {
                    app.request_file_action(A::Open);
                    ui.close_menu();
                }
                ui.menu_button("Recent", |ui| {
                    if app.state.recent_files.is_empty() {
                        ui.label("(none)");
                    }
                    for (i, path) in app.state.recent_files.clone().iter().enumerate() {
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        if ui.button(format!("{} {name}", i + 1)).on_hover_text(path.display().to_string()).clicked() {
                            app.request_file_action(A::OpenRecent(i));
                            ui.close_menu();
                        }
                    }
                });
                item(ui, app, "Save", "Save File");
                item(ui, app, "Save As...", "Save File As");
                item(ui, app, "Save Copy As...", "Save File As Copy");
                item(ui, app, "Save Incremental", "Save Incremental (+)");
                cmd(ui, app, "Save Selected...", Command::SaveSelected { path: None });
                cmd(ui, app, "Set Project Folder...", Command::SetProjectFolder { path: None });
                cmd(ui, app, "Archive...", Command::ArchiveFile { path: None });
                ui.separator();
                item(ui, app, "Merge...", "Merge File");
                cmd(ui, app, "Replace...", Command::ReplaceFile { path: None, names: None, materials: None });
                ui.menu_button("References", |ui| {
                    cmd(ui, app, "XRef Objects...", Command::XrefObjects { path: None, names: None, automatic: None });
                    cmd(ui, app, "XRef Scene...", Command::XrefScene { path: None, automatic: None });
                    cmd(ui, app, "Update XRefs", Command::UpdateXrefs { path: None });
                });
                item(ui, app, "Import...", "Import File");
                item(ui, app, "Export...", "Export File");
                item(ui, app, "Export Selected...", "Export Selected");
                ui.separator();
                cmd(ui, app, "Asset Tracking...", Command::AssetTracking);
                ui.separator();
                cmd(ui, app, "Summary Info...", Command::SummaryInfo);
                cmd(ui, app, "File Properties...", Command::FileProperties);
                ui.separator();
                if ui.button("Exit").clicked() {
                    app.request_file_action(A::Exit);
                    ui.close_menu();
                }
            });
            ui.menu_button("Edit", |ui| {
                let undo = app.state.undo.labels().last().cloned().unwrap_or_default();
                item(ui, app, &format!("Undo {undo}"), "Undo Scene Operation");
                item(ui, app, "Redo", "Redo Scene Operation");
                ui.separator();
                item(ui, app, "Hold", "Hold");
                item(ui, app, "Fetch", "Fetch");
                ui.separator();
                item(ui, app, "Delete", "Delete Objects");
                item(ui, app, "Clone", "Clone");
                ui.separator();
                item(ui, app, "Move", "Select and Move");
                item(ui, app, "Rotate", "Select and Rotate");
                item(ui, app, "Scale", "Select and Scale");
                item(ui, app, "Transform Type-In...", "Transform Type-In Dialog Toggle");
                ui.separator();
                item(ui, app, "Select All", "Select All");
                item(ui, app, "Select None", "Select None");
                item(ui, app, "Select Invert", "Select Invert");
                ui.separator();
                item(ui, app, "Select Similar", "Select Similar");
                // Unavailable unless one object with instances is selected, as in the reference.
                let instances = has_instances(&app.state);
                ui.add_enabled_ui(instances, |ui| item(ui, app, "Select Instances", "Select Instances"));
                ui.menu_button("Select By", |ui| {
                    item(ui, app, "Color", "Select By Color Pick");
                    item(ui, app, "Name...", "Select By Name");
                    cmd(ui, app, "Layer...", Command::SelectByLayer { layers: None });
                });
                ui.menu_button("Region", |ui| {
                    let window = app.state.window_selection;
                    for (label, on) in [("Window", window), ("Crossing", !window)] {
                        let label = if on { format!("✔ {label}") } else { format!("    {label}") };
                        if ui.button(label).clicked() {
                            if !on {
                                let _ = app.dispatch(Command::WindowCrossingSelectionToggle);
                            }
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    for shape in loom_commands::SelectionRegion::ALL {
                        // "Rectangular Region" … "Paint Selection Region", as the reference's menu names them.
                        let name = match shape {
                            loom_commands::SelectionRegion::Paint => shape.label().to_owned(),
                            _ => shape.label().replace(" Selection Region", " Region"),
                        };
                        let label = if app.state.selection_region == shape { format!("✔ {name}") } else { format!("    {name}") };
                        if ui.button(label).clicked() {
                            let _ = app.dispatch(Command::SelectionRegion { shape });
                            ui.close_menu();
                        }
                    }
                });
                ui.separator();
                cmd(ui, app, "Edit Named Selection Sets...", Command::EditNamedSelectionSets);
                ui.separator();
                item(ui, app, "Object Properties...", "Object Properties");
            });
            ui.menu_button("Tools", |ui| {
                // The reference's order; its items Loom has no tool for are left out.
                item(ui, app, "Isolate Selection", "Isolate Selection");
                cmd(ui, app, "Manage Layers...", Command::LayerManager);
                ui.separator();
                cmd(ui, app, "Light Lister...", Command::LightLister);
                ui.separator();
                item(ui, app, "Mirror...", "Mirror");
                cmd(ui, app, "Array...", Command::Array);
                ui.menu_button("Align", |ui| {
                    item(ui, app, "Align...", "Align");
                    item(ui, app, "Quick Align", "Quick Align");
                    item(ui, app, "Spacing Tool...", "Spacing Tool");
                    cmd(ui, app, "Clone and Align...", Command::CloneAndAlignDialog);
                    ui.separator();
                    item(ui, app, "Align to View...", "Align to View Dialog");
                    item(ui, app, "Normal Align...", "Normal Align ");
                    item(ui, app, "Align Camera", "Align Camera");
                    cmd(ui, app, "Place Highlight", Command::PlaceHighlight);
                });
                cmd(ui, app, "Snapshot...", Command::SnapshotDialog);
                ui.separator();
                ui.menu_button("Grids and Snaps", |ui| {
                    cmd(ui, app, "Grid and Snap Settings...", Command::GridAndSnapSettings);
                    ui.separator();
                    let grid = app.state.active().show_grid;
                    check_item(ui, app, "Show Home Grid", "Hide Grids Toggle", grid);
                    cmd(ui, app, "Activate Home Grid", Command::ActivateHomeGrid);
                    let grid_selected = app.state.selection.iter().any(|id| app.state.scene.node(*id).is_some_and(|n| matches!(n.object, loom_scene::ObjectKind::Grid { .. })));
                    if ui.add_enabled(grid_selected, egui::Button::new("Activate Grid Object")).clicked() {
                        let _ = app.dispatch(Command::ActivateGridObject);
                        ui.close_menu();
                    }
                    if ui.add_enabled(app.state.active_grid().is_some(), egui::Button::new("Align Grid to View")).clicked() {
                        let _ = app.dispatch(Command::AlignGridToView);
                        ui.close_menu();
                    }
                    ui.separator();
                    check_item(ui, app, "Snaps Toggle", "Snaps Toggle", app.state.snaps_on);
                    check_item(ui, app, "Angle Snap Toggle", "Angle Snap Toggle", app.state.angle_snap_on);
                    check_item(ui, app, "Percent Snap Toggle", "Percent Snap Toggle", app.state.percent_snap_on);
                    check_item(ui, app, "Spinner Snap Toggle", "Spinner Snap Toggle", app.state.prefs.spinner_snap);
                    let axis = app.state.snaps.use_axis_constraints;
                    check_item(ui, app, "Snaps Use Axis Constraints", Command::EnableAxisConstraintsInSnapsToggle.name(), axis);
                });
                cmd(ui, app, "Measure Distance", Command::MeasureDistance);
                ui.separator();
                let ribbon = app.state.show_ribbon;
                check_item(ui, app, "Graphite Modeling Tools", "Show Ribbon Toggle", ribbon);
            });
            ui.menu_button("Group", |ui| {
                if menu_button(ui, &app.state, "Group...", "Group").clicked() {
                    // The reference asks for the group name first.
                    if app.state.selection.is_empty() {
                        app.state.status_prompt = "select objects to group".into();
                    } else {
                        app.state.dialog = Some(loom_commands::Dialog::GroupName);
                    }
                    ui.close_menu();
                }
                cmd(ui, app, "Ungroup", Command::Ungroup);
                ui.separator();
                cmd(ui, app, "Open", Command::GroupOpen);
                cmd(ui, app, "Close", Command::GroupClose);
                ui.separator();
                cmd(ui, app, "Attach", Command::GroupAttach);
                cmd(ui, app, "Detach", Command::GroupDetach);
                ui.separator();
                cmd(ui, app, "Explode", Command::GroupExplode);
                ui.separator();
                ui.add_enabled(false, egui::Button::new("Assembly")).on_disabled_hover_text("Assemblies (luminaires) are out of scope");
            });
            ui.menu_button("Views", |ui| {
                let (undo_view, redo_view) = (app.state.active().view_undo.undo_label().unwrap_or("Change").to_owned(), app.state.active().view_undo.redo_label().unwrap_or("Change").to_owned());
                item(ui, app, &format!("Undo View {undo_view}"), "Undo Viewport Operation");
                item(ui, app, &format!("Redo View {redo_view}"), "Redo Viewport Operation");
                ui.separator();
                // The active view's name is part of the item, as in the reference.
                let view = app.state.active().kind.label();
                item(ui, app, &format!("Save Active {view} View"), "Save Active View");
                item(ui, app, &format!("Restore Active {view} View"), "Restore Active View");
                ui.separator();
                item(ui, app, "Viewport Configuration...", "Configure Viewports");
                item(ui, app, "Viewport Background...", "Viewport Background");
                item(ui, app, "Update Background Image", "Update Background Image");
                item(ui, app, "Reset Background Transform", "Reset Background Transform");
                item(ui, app, "Show Floating Dialogs", "Show Floating Dialogs");
                ui.separator();
                let (gizmo, shade) = (app.state.prefs.gizmos.enabled, app.state.shade_selected);
                check_item(ui, app, "Show Transform Gizmo", "Transform Gizmo Toggle", gizmo);
                check_item(ui, app, "Shade Selected", "Shade Selected", shade);
                let spinner = app.state.update_during_spinner_drag;
                check_item(ui, app, "Update During Spinner Drag", "Update During Spinner Drag", spinner);
                item(ui, app, "Create Camera From View", "Create Camera From View");
                item(ui, app, "Add Default Lights to Scene...", "Add Default Lights to Scene");
                item(ui, app, "Redraw All Views", "Redraw All Views");
                ui.separator();
                for (label, action) in [
                    ("Top", "Top View"), ("Bottom", "Bottom View"), ("Front", "Front View"), ("Back", "Back View"),
                    ("Left", "Left View"), ("Right", "Right View"), ("Perspective", "Perspective User View"), ("Orthographic", "Orthographic User View"),
                ] {
                    item(ui, app, label, action);
                }
                ui.separator();
                item(ui, app, "Zoom Extents", "Zoom Extents");
                item(ui, app, "Zoom Extents All", "Zoom Extents All");
                item(ui, app, "Zoom Extents Selected", "Zoom Extents Selected");
                ui.separator();
                let vp = app.state.active();
                let (grid, edged, safe) = (vp.show_grid, vp.edged_faces, vp.safe_frames);
                check_item(ui, app, "Show Grid", "Hide Grids Toggle", grid);
                check_item(ui, app, "Show Statistics", "Show Statistics Toggle", app.state.show_statistics);
                check_item(ui, app, "Edged Faces", "View Edged Faces Toggle", edged);
                item(ui, app, "Wireframe / Smooth+Highlights", "Wireframe / Smooth+Highlights Toggle");
                check_item(ui, app, "Show Safe Frames", "Viewport Safeframes Toggle", safe);
                ui.separator();
                check_item(ui, app, "Expert Mode", "Expert Mode Toggle", app.state.expert_mode);
            });
            ui.menu_button("Create", |ui| {
                ui.menu_button("Standard Primitives", |ui| {
                    for kind in PrimitiveKind::ALL {
                        if ui.button(kind.label()).clicked() {
                            app.create_kind = Some(crate::CreateKind::Primitive(kind));
                            app.create_category = crate::Category::Geometry;
                            app.panel_tab = crate::Tab::Create;
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("Lights", |ui| {
                    ui.menu_button("Photometric Lights", |ui| {
                        for kind in loom_commands::light::LightKind::PHOTOMETRIC {
                            if ui.button(kind.label()).clicked() {
                                let _ = app.dispatch(Command::SelectObject);
                                app.photometric_lights = true;
                                app.create_kind = Some(crate::CreateKind::Light(kind));
                                app.create_category = crate::Category::Lights;
                                app.panel_tab = crate::Tab::Create;
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("Standard Lights", |ui| {
                        for (kind, label) in [
                            (loom_commands::light::LightKind::TargetSpot, "Target Spotlight"), (loom_commands::light::LightKind::FreeSpot, "Free Spotlight"),
                            (loom_commands::light::LightKind::TargetDirect, "Target Directional"), (loom_commands::light::LightKind::FreeDirect, "Directional"),
                            (loom_commands::light::LightKind::Omni, "Omni"), (loom_commands::light::LightKind::Skylight, "Skylight"),
                        ] {
                            if ui.button(label).clicked() {
                                let _ = app.dispatch(Command::SelectObject);
                                app.photometric_lights = false;
                                app.create_kind = Some(crate::CreateKind::Light(kind));
                                app.create_category = crate::Category::Lights;
                                app.panel_tab = crate::Tab::Create;
                                ui.close_menu();
                            }
                        }
                    });
                    ui.separator();
                    for (label, daylight) in [("Daylight System", true), ("Sunlight System", false)] {
                        if ui.button(label).clicked() {
                            app.create_category = crate::Category::Systems;
                            app.panel_tab = crate::Tab::Create;
                            app.start_create(Some(crate::CreateKind::SunSystem { daylight }));
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("Helpers", |ui| {
                    for (kind, label) in [(crate::HelperKind::Dummy, "Dummy"), (crate::HelperKind::Point, "Point"), (crate::HelperKind::Grid, "Grid"), (crate::HelperKind::Tape, "Tape Measure"), (crate::HelperKind::Protractor, "Protractor"), (crate::HelperKind::Compass, "Compass")] {
                        if ui.button(label).clicked() {
                            app.create_category = crate::Category::Helpers;
                            app.panel_tab = crate::Tab::Create;
                            app.start_create(Some(crate::CreateKind::Helper(kind)));
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("Cameras", |ui| {
                    // Turns on the creation tool: drag (Target) or click (Free) in a viewport.
                    for (label, target) in [("Target Camera", true), ("Free Camera", false)] {
                        if ui.button(label).clicked() {
                            app.create_kind = Some(crate::CreateKind::Camera { target });
                            app.create_category = crate::Category::Cameras;
                            app.panel_tab = crate::Tab::Create;
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    cmd(ui, app, "Create Camera From View", Command::CreateCameraFromView);
                });
                ui.menu_button("Shapes", |ui| {
                    let kinds = std::iter::once((crate::CreateKind::Line, "Line")).chain(loom_commands::ShapeKind::ALL.iter().map(|k| (crate::CreateKind::Shape(*k), k.label())));
                    for (kind, label) in kinds {
                        if ui.button(label).clicked() {
                            if let Some(line) = app.line.take() {
                                line.finish(app);
                            }
                            app.create_category = crate::Category::Shapes;
                            app.panel_tab = crate::Tab::Create;
                            app.start_create(Some(kind));
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("Compound", |ui| {
                    for kind in crate::command_panel::compound::CompoundKind::ALL {
                        if ui.button(kind.label()).clicked() {
                            app.create_category = crate::Category::Geometry;
                            app.create_compound = true;
                            app.compound_kind = Some(kind);
                            app.create_kind = None;
                            app.panel_tab = crate::Tab::Create;
                            let _ = app.dispatch(Command::SelectObject);
                            ui.close_menu();
                        }
                    }
                });
                ui.separator();
                let mut keep = app.state.prefs.keep_creating;
                if ui.checkbox(&mut keep, "Keep Creating").changed() {
                    let _ = app.dispatch(Command::KeepCreatingToggle);
                    ui.close_menu();
                }
                ui.separator();
                item(ui, app, "Test Room", "Test Room");
            });
            ui.menu_button("Modifiers", |ui| {
                let has = app.has_selection();
                for (category, entries) in MODIFIERS_MENU {
                    ui.menu_button(*category, |ui| {
                        for (label, name) in *entries {
                            if ui.add_enabled(has, egui::Button::new(*label)).clicked() {
                                if let Err(e) = app.dispatch(Command::AddModifier { name: (*name).into() }) {
                                    app.state.status_prompt = e;
                                }
                                ui.close_menu();
                            }
                        }
                    });
                }
            });
            ui.menu_button("Animation", |ui| {
                cmd(ui, app, "Load Animation...", Command::LoadAnimation { path: None, offset: None });
                cmd(ui, app, "Save Animation...", Command::SaveAnimation { path: None });
                ui.separator();
                ui.menu_button("Constraints", |ui| {
                    for (label, kind) in [("Attachment Constraint", "Attachment Constraint"), ("Path Constraint", "Path Constraint"), ("Position Constraint", "Position Constraint"), ("Link Constraint", "Link Constraint"), ("LookAt Constraint", "LookAt Constraint"), ("Orientation Constraint", "Orientation Constraint")] {
                        cmd(ui, app, label, Command::StartConstraintPick { kind: kind.into() });
                    }
                });
                ui.menu_button("Position Controllers", |ui| {
                    for c in ["Position XYZ", "Bezier Position", "TCB Position", "Linear Position", "Noise Position", "Position List"] {
                        controller_item(ui, app, "Position", c);
                    }
                });
                ui.menu_button("Rotation Controllers", |ui| {
                    for c in ["Euler XYZ", "Bezier Rotation", "TCB Rotation", "Linear Rotation", "Noise Rotation", "Rotation List"] {
                        controller_item(ui, app, "Rotation", c);
                    }
                });
                ui.menu_button("Scale Controllers", |ui| {
                    for c in ["ScaleXYZ", "Bezier Scale", "TCB Scale", "Linear Scale", "Noise Scale", "Scale List"] {
                        controller_item(ui, app, "Scale", c);
                    }
                });
                ui.separator();
                cmd(ui, app, "Delete Selected Animation", Command::DeleteSelectedAnimation);
                ui.separator();
                ui.menu_button("IK Solvers", |ui| {
                    item(ui, app, "HI Solver", "History-Independent IK Solver");
                    item(ui, app, "IK Limb Solver", "IK Limb Solver");
                    item(ui, app, "Spline IK Solver", "Spline IK Solver");
                });
                ui.separator();
                item(ui, app, "Bone Tools...", "Bone Tools...");
                ui.separator();
                item(ui, app, "Display Bones", "Display Bones");
                item(ui, app, "Display Bones and Objects", "Display Bones and Objects");
            });
            ui.menu_button("Graph Editors", |ui| {
                cmd(ui, app, "Track View - Curve Editor...", Command::OpenCurveEditor);
                cmd(ui, app, "Track View - Dope Sheet...", Command::OpenDopeSheet);
            });
            ui.menu_button("Rendering", |ui| {
                cmd(ui, app, "Render", Command::Render { path: None, width: None, height: None });
                cmd(ui, app, "Render Setup...", Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, save_file: None, output: None });
                cmd(ui, app, "Rendered Frame Window...", Command::RenderedFrameWindow);
                ui.separator();
                cmd(ui, app, "Render To Texture...", Command::RenderToTexture);
                ui.separator();
                cmd(ui, app, "Material Editor...", Command::MaterialEditorToggle);
            });
            ui.menu_button("Customize", |ui| {
                cmd(ui, app, "Customize User Interface...", Command::CustomizeUserInterface);
                ui.separator();
                cmd(ui, app, "Load Custom UI Scheme...", Command::LoadCustomUiScheme { path: None });
                cmd(ui, app, "Save Custom UI Scheme...", Command::SaveCustomUiScheme { path: None });
                item(ui, app, "Revert to Startup Layout", "Revert to Startup UI");
                ui.separator();
                ui.menu_button("Show UI", |ui| {
                    check_item(ui, app, "Show Command Panel", "Show Command Panel Toggle", app.state.show_command_panel);
                    check_item(ui, app, "Show Main Toolbar", "Show Main Toolbar Toggle", app.state.show_main_toolbar);
                    check_item(ui, app, "Show Track Bar", "Show Track Bar Toggle", app.state.show_track_bar);
                    check_item(ui, app, "Show Ribbon", "Show Ribbon Toggle", app.state.show_ribbon);
                    check_item(ui, app, "Show Layers Toolbar", "Show Layers Toolbar Toggle", app.state.show_layers_toolbar);
                });
                check_item(ui, app, "Lock UI Layout", "Lock User Interface Toggle", app.state.ui_locked);
                ui.separator();
                cmd(ui, app, "Grid and Snap Settings...", Command::GridAndSnapSettings);
                cmd(ui, app, "Units Setup...", Command::UnitsSetup);
                ui.separator();
                cmd(ui, app, "Preferences...", Command::Preferences);
            });
            ui.menu_button("Help", |ui| {
                if ui.button("3dLoom on GitHub").on_hover_text(REPO_URL).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(REPO_URL));
                    ui.close_menu();
                }
                if ui.button("Releases").clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(format!("{REPO_URL}/releases")));
                    ui.close_menu();
                }
                ui.separator();
                ui.label(format!("3dLoom {} — a 3D modelling application", env!("LOOM_VERSION")));
            });
            if let Some(handle) = app.update.clone() {
                crate::update_menu::show(ui, &handle);
            }
        });
    });
}

/// One object is selected and it has instances (Edit > Select Instances).
fn has_instances(s: &loom_commands::AppState) -> bool {
    let ids: Vec<loom_scene::NodeId> = s.selection.iter().copied().collect();
    if ids.len() != 1 {
        return false;
    }
    let root = |mut n: loom_scene::NodeId| {
        while let Some(src) = s.scene.node(n).and_then(|x| x.instance_of) {
            n = src;
        }
        n
    };
    let r = root(ids[0]);
    s.scene.nodes().filter(|n| root(n.id) == r).count() > 1
}

/// Animation > Position / Rotation / Scale Controllers: assign to every selected object.
fn controller_item(ui: &mut egui::Ui, app: &mut LoomApp, track: &str, controller: &str) {
    if ui.button(controller).clicked() {
        let ids: Vec<u64> = app.state.selection.iter().map(|n| n.0).collect();
        for id in ids {
            if let Err(e) = app.dispatch(Command::AssignController { id, track: track.into(), controller: controller.into() }) {
                app.state.status_prompt = e;
            }
        }
        ui.close_menu();
    }
}

/// The project's public repository (Help menu).
const REPO_URL: &str = "https://github.com/bonskari/3dloom";

/// Modifiers menu: the reference's categories, each with the modifiers of
/// the Modifier List that belong to it (label, Modifier List name).
/// Categories with no modifier in Loom (NURBS Editing, Cameras) are left out.
pub const MODIFIERS_MENU: &[(&str, &[(&str, &str)])] = &[
    ("Selection Modifiers", &[("Mesh Select", "Mesh Select"), ("Poly Select", "Poly Select"), ("Spline Select", "Spline Select"), ("Vol. Select", "Vol. Select")]),
    ("Patch/Spline Editing", &[
        ("CrossSection", "CrossSection"), ("Delete Spline", "Delete Spline"), ("Edit Patch", "Edit Patch"), ("Edit Spline", "Edit Spline"), ("Fillet/Chamfer", "Fillet/Chamfer"), ("Lathe", "Lathe"),
        ("Normalize Spline", "Normalize Spline"), ("Renderable Spline", "Renderable Spline"), ("Surface", "Surface"), ("Sweep", "Sweep"), ("Trim/Extend", "Trim/Extend"),
    ]),
    ("Mesh Editing", &[
        ("Bevel", "Bevel"), ("Bevel Profile", "Bevel Profile"), ("Cap Holes", "Cap Holes"), ("DeleteMesh", "DeleteMesh"), ("Edit Mesh", "Edit Mesh"), ("Edit Normals", "Edit Normals"),
        ("Edit Poly", "Edit Poly"), ("Extrude", "Extrude"), ("Face Extrude", "Face Extrude"), ("MultiRes", "MultiRes"), ("Normal", "Normal"), ("Optimize", "Optimize"),
        ("ProOptimizer", "ProOptimizer"), ("Smooth", "Smooth"), ("STL Check", "STL Check"), ("Symmetry", "Symmetry"), ("Tessellate", "Tessellate"), ("VertexPaint", "VertexPaint"),
        ("Vertex Weld", "Vertex Weld"),
    ]),
    ("Conversion", &[("Turn To Mesh", "Turn To Mesh"), ("Turn To Poly", "Turn To Poly")]),
    ("Animation Modifiers", &[
        ("Flex", "Flex"), ("Linked XForm", "Linked XForm"), ("Melt", "Melt"), ("Morpher", "Morpher"), ("PathDeform", "PathDeform"), ("PathDeform (WSM)", "PathDeform (WSM)"),
        ("Skin", "Skin"), ("Skin Morph", "Skin Morph"), ("Skin Wrap", "Skin Wrap"), ("Skin Wrap Patch", "Skin Wrap Patch"), ("Spline IK Control", "Spline IK Control"),
    ]),
    ("UV Coordinates", &[
        ("Camera Map", "Camera Map"), ("Camera Map (WSM)", "Camera Map (WSM)"), ("MapScaler", "MapScaler"), ("MapScaler (WSM)", "MapScaler (WSM)"), ("Projection", "Projection"), ("Unwrap UVW", "Unwrap UVW"),
        ("UVW Map", "UVW Map"), ("UVW Mapping Add", "UVW Mapping Add"), ("UVW Mapping Clear", "UVW Mapping Clear"), ("UVW XForm", "UVW XForm"),
    ]),
    ("Cache Tools", &[("Point Cache", "Point Cache"), ("Point Cache (WSM)", "Point Cache (WSM)")]),
    ("Subdivision Surfaces", &[("HSDS Modifier", "HSDS"), ("MeshSmooth", "MeshSmooth"), ("TurboSmooth", "TurboSmooth")]),
    ("Free Form Deformers", &[("FFD 2x2x2", "FFD 2x2x2"), ("FFD 3x3x3", "FFD 3x3x3"), ("FFD 4x4x4", "FFD 4x4x4"), ("FFD(box)", "FFD(box)"), ("FFD(cyl)", "FFD(cyl)")]),
    ("Parametric Deformers", &[
        ("Affect Region", "Affect Region"), ("Bend", "Bend"), ("Displace", "Displace"), ("Lattice", "Lattice"), ("Mirror", "Mirror"), ("Noise", "Noise"), ("Preserve", "Preserve"),
        ("Push", "Push"), ("Relax", "Relax"), ("Ripple", "Ripple"), ("Shell", "Shell"), ("Skew", "Skew"), ("Slice", "Slice"), ("Spherify", "Spherify"), ("Squeeze", "Squeeze"),
        ("Stretch", "Stretch"), ("Substitute", "Substitute"), ("Taper", "Taper"), ("Twist", "Twist"), ("Wave", "Wave"), ("XForm", "XForm"),
    ]),
    ("Surface", &[("Material", "Material"), ("Material By Element", "Material By Element")]),
    ("Radiosity", &[("Subdivide", "Subdivide"), ("Subdivide (WSM)", "Subdivide (WSM)")]),
];

#[cfg(test)]
mod tests {
    use super::MODIFIERS_MENU;
    use loom_scene::Modifier;

    #[test]
    fn modifiers_menu_holds_every_modifier_list_entry_once() {
        let mut names: Vec<&str> = MODIFIERS_MENU.iter().flat_map(|(_, e)| e.iter().map(|(_, n)| *n)).collect();
        for n in &names {
            assert!(Modifier::by_name(n).is_some(), "{n} is not a modifier");
        }
        names.sort();
        let mut list: Vec<&str> = Modifier::LIST.iter().chain(Modifier::WSM_LIST.iter()).copied().collect();
        list.sort();
        assert_eq!(names, list);
    }
}

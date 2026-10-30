//! Modeless dialogs opened by commands (`AppState::dialog`).

pub mod rename_objects;
pub mod display_floater;
pub mod configure_paths;
pub mod align;
pub mod array;
pub mod spacing_tool;
pub mod snapshot;
pub mod clone_and_align;
pub mod align_to_view;
pub mod clone_options;
pub mod customize_ui;
pub mod object_properties;
pub mod object_color;
pub mod preferences;
pub mod units_setup;
pub mod mirror;
pub mod poly_settings;
pub mod delete_isolated;
pub mod detach;
pub mod manage_layers;
pub mod normal_align;
pub mod grid_and_snap_settings;
pub mod named_selection_sets;
pub mod transform_type_in;
pub mod select_from_scene;
pub mod time_configuration;
pub mod viewport_background;
pub mod viewport_configuration;
pub mod file_dialog;
pub mod biped_footsteps;
pub mod material_editor;
pub mod render_setup;
pub mod render_to_texture;
pub mod lights;
pub mod assets;
pub mod save_changes;
pub mod spline_prompts;
pub mod topology_warning;
pub mod merge;
pub mod exchange_options;
pub mod scene_dialogs;
pub mod unwrap_dialogs;
pub mod rig_dialogs;
pub mod skin_dialogs;
pub mod track_view_dialogs;
pub mod vertex_paint;

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, Dialog};

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    import_summary(ctx, app);
    // A previewing dialog that went away without OK (Close Dialog, Escape,
    // another dialog) leaves the scene as it was. An Extrude-mode drag
    // previews with no window open; its release keeps the result.
    if let Some(p) = app.preview {
        if app.state.dialog != Some(p.dialog) && !app.poly_dragging {
            app.preview_undo();
            app.state.align_target = None;
            app.state.normal_align = None;
            // A poly settings window previews on a selection the op replaces: give it back.
            if let Dialog::PolySettings(kind) = p.dialog {
                let sel_id = egui::Id::new("poly_settings").with(format!("{kind:?}")).with("selection");
                if let Some(sel) = ctx.data(|d| d.get_temp::<Vec<(u64, loom_commands::SubObjectId)>>(sel_id)) {
                    app.state.sub_selection.clear();
                    for (n, so) in sel {
                        app.state.sub_selection.insert(loom_scene::NodeId(n), so);
                    }
                }
            }
        }
    }
    // A Shift+drag clone whose Clone Options dialog went away is cancelled.
    if app.shift_clone.is_some() && app.state.dialog != Some(Dialog::CloneOptions) {
        if let Some(sc) = app.shift_clone.take() {
            if app.state.undo.len() == sc.undo_len {
                let _ = app.dispatch(Command::UndoSceneOperation);
            }
        }
    }
    if app.last_dialog != app.state.dialog {
        // Per-dialog session state must not leak into the next time it opens.
        ctx.data_mut(|d| {
            d.remove::<Vec<u64>>(egui::Id::new("mirror_dialog_sources"));
            d.remove::<Vec<u64>>(egui::Id::new("array_dialog").with("sources"));
            d.remove::<bool>(egui::Id::new("array_dialog").with("preview"));
            d.remove::<bool>(egui::Id::new("align_dialog_kept"));
            d.remove::<String>(egui::Id::new("clone_options_name"));
            d.remove::<u32>(egui::Id::new("clone_options_copies"));
            file_dialog::clear_state(d);
            merge::clear_state(d);
            exchange_options::clear_state(d);
            select_from_scene::clear_state(d);
            d.remove::<String>(egui::Id::new("auto_backup_name_edit"));
            for k in ["Weld", "Chamfer", "ConnectEdges", "Bridge", "Extrude", "Inset", "Bevel", "Outline", "Hinge", "MSmooth", "Tessellate", "Relax"] {
                let id = egui::Id::new("poly_settings").with(k);
                d.remove::<loom_commands::PolySettings>(id);
                d.remove::<Vec<(u64, loom_commands::SubObjectId)>>(id.with("selection"));
            }
        });
        app.last_dialog = app.state.dialog;
    }
    if app.state.hide_floating_dialogs {
        return;
    }
    // The Material Editor is a window of its own: its bitmap chooser (a
    // dialog) opens over it and leaves it open.
    if app.state.material_editor_open {
        let mut open = true;
        material_editor::show(ctx, app, &mut open);
        if !open {
            app.state.material_editor_open = false;
        }
    }
    // The VertexPaint dialog floats while a VertexPaint entry is active in the Modify panel.
    vertex_paint::show(ctx, app);
    // Render To Texture is modeless: it stays open beside the other dialogs.
    render_to_texture::show(ctx, app);
    if app.state.material_navigator_open {
        let mut open = true;
        material_editor::navigator(ctx, app, &mut open);
        if !open {
            app.state.material_navigator_open = false;
        }
    }
    // Object Color opens over the Object Properties dialog and leaves it open.
    if app.state.display.object_color_open {
        let mut open = true;
        object_color::show(ctx, app, &mut open);
        if !open {
            app.state.display.object_color_open = false;
        }
    }
    let Some(dialog) = app.state.dialog else { return };
    let mut open = true;
    match dialog {
        Dialog::MaterialMap { .. } | Dialog::MaterialBitmap => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::OpenMaterialLibrary | Dialog::MergeMaterialLibrary | Dialog::SaveMaterialLibrary => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::PutToLibrary => material_editor::put_to_library(ctx, app, &mut open),
        Dialog::MaterialBrowser(for_) => material_editor::browser(ctx, app, for_, &mut open),
        Dialog::ReplaceMaterial { kind, map } => material_editor::replace_material(ctx, app, kind, map, &mut open),
        Dialog::MaterialEditorOptions => material_editor::options(ctx, app, &mut open),
        Dialog::SpecifyCropping => material_editor::specify_cropping(ctx, app, &mut open),
        Dialog::CopyMap { from, to } => material_editor::copy_map(ctx, app, from, to, &mut open),
        Dialog::RenderFile | Dialog::SaveRenderedImage => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::RenderSetup => render_setup::show(ctx, app, &mut open),
        Dialog::LightLister => lights::light_lister(ctx, app, &mut open),
        Dialog::LightExclude { light } => lights::light_exclude(ctx, app, light, &mut open),
        Dialog::AddDefaultLights => lights::add_default_lights(ctx, app, &mut open),
        Dialog::GeographicLocation { sun } => lights::geographic_location(ctx, app, sun, &mut open),
        Dialog::ViewportConfiguration => viewport_configuration::show(ctx, app, &mut open),
        Dialog::SelectFromScene => select_from_scene::show(ctx, app, &mut open),
        Dialog::SelectCamera => select_camera(ctx, app, &mut open),
        Dialog::NamedSelectionSets => named_selection_sets::show(ctx, app, &mut open),
        Dialog::TimeConfiguration => time_configuration::show(ctx, app, &mut open),
        Dialog::ViewportBackground => viewport_background::show(ctx, app, &mut open),
        Dialog::TransformTypeIn => transform_type_in::show(ctx, app, &mut open),
        Dialog::CloneOptions => clone_options::show(ctx, app, &mut open),
        Dialog::GridAndSnapSettings => grid_and_snap_settings::show(ctx, app, &mut open),
        Dialog::Align => align::show(ctx, app, &mut open),
        Dialog::AlignToView => align_to_view::show(ctx, app, &mut open),
        Dialog::NormalAlign => normal_align::show(ctx, app, &mut open),
        Dialog::Mirror => mirror::show(ctx, app, &mut open),
        Dialog::Array => array::show(ctx, app, &mut open),
        Dialog::SpacingTool => spacing_tool::show(ctx, app, &mut open),
        Dialog::Snapshot => snapshot::show(ctx, app, &mut open),
        Dialog::CloneAndAlign => clone_and_align::show(ctx, app, &mut open),
        Dialog::ObjectProperties => object_properties::show(ctx, app, &mut open),
        Dialog::Preferences => preferences::show(ctx, app, &mut open),
        Dialog::UnitsSetup => units_setup::show(ctx, app, &mut open),
        Dialog::UnitsMismatch => units_setup::units_mismatch(ctx, app, &mut open),
        Dialog::ConfigurePaths => configure_paths::show(ctx, app, &mut open),
        Dialog::Switcher => switcher(ctx, app, &mut open),
        Dialog::RenameObjects => rename_objects::show(ctx, app, &mut open),
        Dialog::CustomizeUI => customize_ui::show(ctx, app, &mut open),
        Dialog::PolySettings(kind) => poly_settings::show(ctx, app, kind, &mut open),
        Dialog::Detach => detach::show(ctx, app, &mut open),
        Dialog::DeleteIsolatedVertices => delete_isolated::show(ctx, app, &mut open),
        Dialog::LayerManager => manage_layers::show(ctx, app, &mut open),
        Dialog::OpenFile | Dialog::SaveFileAs | Dialog::SaveFileAsCopy | Dialog::ImportFile | Dialog::SaveAnimation | Dialog::LoadAnimation | Dialog::SaveShortcuts | Dialog::LoadShortcuts | Dialog::SaveCustomUiScheme | Dialog::LoadCustomUiScheme | Dialog::UiFile { .. } | Dialog::ExportFile | Dialog::ExportSelected | Dialog::MergeFile | Dialog::XrefObjectsFile | Dialog::XrefSceneFile => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::SaveChanges(action) => save_changes::show(ctx, app, action, &mut open),
        Dialog::CloseSpline => spline_prompts::close_spline(ctx, app, &mut open),
        Dialog::RefineVertex => spline_prompts::refine_vertex(ctx, app, &mut open),
        Dialog::TopologyWarning => topology_warning::show(ctx, app, &mut open),
        Dialog::PickTexture | Dialog::BackgroundFile | Dialog::BitmapFit | Dialog::SaveUvw | Dialog::LoadUvw | Dialog::BipedSaveFig | Dialog::BipedLoadFig | Dialog::PickUvTexture | Dialog::PointCacheRecord { .. } | Dialog::PointCacheLoad { .. } => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::UnwrapOptions | Dialog::StitchTool | Dialog::RelaxTool | Dialog::PackUvs | Dialog::FlattenMapping | Dialog::NormalMapping | Dialog::UnfoldMapping | Dialog::PeltMap | Dialog::RenderUvwTemplate => unwrap_dialogs::show(ctx, app, dialog, &mut open),
        Dialog::MergeObjects => merge::show(ctx, app, &mut open),
        Dialog::FbxImportOptions => exchange_options::fbx_import(ctx, app, &mut open),
        Dialog::FbxExportOptions => exchange_options::fbx_export(ctx, app, &mut open),
        Dialog::ObjImportOptions => exchange_options::obj_import(ctx, app, &mut open),
        Dialog::ObjExportOptions => exchange_options::obj_export(ctx, app, &mut open),
        Dialog::StlImportOptions => exchange_options::stl_import(ctx, app, &mut open),
        Dialog::StlExportOptions => exchange_options::stl_export(ctx, app, &mut open),
        Dialog::TdsImportOptions => exchange_options::tds_import(ctx, app, &mut open),
        Dialog::ReplaceFile => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::ReplaceObjects => assets::replace_objects(ctx, app, &mut open),
        Dialog::ProjectFolder => assets::project_folder(ctx, app, &mut open),
        Dialog::AssetTracking => assets::asset_tracking(ctx, app, &mut open),
        Dialog::MissingFiles => assets::missing_files(ctx, app, &mut open),
        Dialog::SelectByLayer => assets::select_by_layer(ctx, app, &mut open),
        Dialog::BoneTools => rig_dialogs::bone_tools(ctx, app, &mut open),
        Dialog::WeightTable => skin_dialogs::weight_table(ctx, app, &mut open),
        Dialog::WeightTool => rig_dialogs::weight_tool(ctx, app, &mut open),
        Dialog::PainterOptions => skin_dialogs::painter_options(ctx, app, &mut open),
        Dialog::SkinSaveEnvelopes | Dialog::SkinLoadEnvelopes => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::PasteToMultipleBones => skin_dialogs::paste_to_multiple_bones(ctx, app, &mut open),
        Dialog::LoadEnvelopes => skin_dialogs::load_envelopes(ctx, app, &mut open),
        Dialog::AngleKeysCurves => skin_dialogs::angle_keys_curves(ctx, app, &mut open),
        Dialog::OutOfRangeTypes => track_view_dialogs::out_of_range_types(ctx, app, &mut open),
        Dialog::ReduceKeys => track_view_dialogs::reduce_keys(ctx, app, &mut open),
        Dialog::TrackSets => track_view_dialogs::track_sets(ctx, app, &mut open),
        Dialog::AssignController => track_view_dialogs::assign_controller(ctx, app, &mut open),
        Dialog::KeyInfo => track_view_dialogs::key_info(ctx, app, &mut open),
        Dialog::TrackViewFilters => track_view_dialogs::filters(ctx, app, &mut open),
        Dialog::TrackViewUtilities => track_view_dialogs::utilities(ctx, app, &mut open),
        Dialog::SaveSelected | Dialog::ArchiveFile => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::NewScene => scene_dialogs::new_scene(ctx, app, &mut open),
        Dialog::ConfirmReset => scene_dialogs::confirm_reset(ctx, app, &mut open),
        Dialog::SummaryInfo => scene_dialogs::summary_info(ctx, app, &mut open),
        Dialog::FileProperties => scene_dialogs::file_properties(ctx, app, &mut open),
        Dialog::GroupName => scene_dialogs::group_name(ctx, app, &mut open),
        Dialog::BipedMultipleFootsteps => biped_footsteps::show(ctx, app, &mut open),
        Dialog::LayerProperties { layer } => scene_dialogs::layer_properties(ctx, app, layer, &mut open),
    }
    // A dialog that handed over to another (Save Changes → Open) must not close the new one.
    if !open && app.state.dialog == Some(dialog) {
        let _ = app.dispatch(Command::CloseDialog);
        // A cancelled Keyboard panel Save / Load goes back to the panel.
        if matches!(dialog, Dialog::SaveShortcuts | Dialog::LoadShortcuts | Dialog::UiFile { .. }) {
            let _ = app.dispatch(Command::CustomizeUserInterface);
        }
    }
}

/// Select Camera (C with several cameras and none selected): double-click
/// or OK looks through the chosen camera in the active viewport.
fn select_camera(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let cams: Vec<(loom_scene::NodeId, String)> = app.state.scene.nodes().filter(|n| matches!(n.object, loom_scene::ObjectKind::Camera { .. })).map(|n| (n.id, n.name.clone())).collect();
    let id = egui::Id::new("select_camera_pick");
    let mut pick: Option<loom_scene::NodeId> = ctx.data(|d| d.get_temp(id)).or(cams.first().map(|c| c.0));
    let mut chosen = None;
    let mut cancel = false;
    egui::Window::new("Select Camera").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            for (cid, name) in &cams {
                let r = ui.selectable_label(pick == Some(*cid), name);
                if r.clicked() {
                    pick = Some(*cid);
                }
                if r.double_clicked() {
                    chosen = Some(*cid);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                chosen = pick;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    if let Some(p) = pick {
        ctx.data_mut(|d| d.insert_temp(id, p));
    }
    if let Some(cid) = chosen {
        let index = app.state.active_viewport as u32;
        let _ = app.dispatch(Command::ViewportView { index, kind: loom_commands::ViewKind::Camera(cid) });
        cancel = true;
    }
    if cancel {
        ctx.data_mut(|d| d.remove::<loom_scene::NodeId>(id));
        *open = false;
    }
}

/// File > Import's summary: what the file held that Loom did not import.
fn import_summary(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.import_report.is_empty() {
        return;
    }
    let mut open = true;
    let mut ok = false;
    egui::Window::new("Import Summary").open(&mut open).collapsible(false).resizable(true).default_width(460.0).show(ctx, |ui| {
        ui.label(format!("{} item(s) were not imported as they are in the file:", app.state.import_report.len()));
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for w in &app.state.import_report {
                ui.label(egui::RichText::new(w).small());
            }
        });
        ok = ui.button("OK").clicked();
    });
    if !open || ok {
        app.state.import_report.clear();
    }
}

/// Custom UI and Defaults Switcher: the defaults set for tool options and the
/// UI scheme; Set applies both.
fn switcher(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("switcher_picks");
    let mut picks: (String, String, String) = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| ("Default".into(), "Default UI".into(), String::new()));
    let sets = loom_commands::switcher::defaults_sets();
    let schemes = loom_commands::switcher::schemes();
    let mut done = false;
    egui::Window::new("Choose initial settings for tool options and UI layout").id(egui::Id::new("Custom UI and Defaults Switcher")).open(open).collapsible(false).default_width(520.0).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(240.0);
                ui.label(egui::RichText::new("Initial settings for tool options").strong());
                for s in &sets {
                    if ui.selectable_label(picks.0 == *s, s).clicked() {
                        picks.0 = s.clone();
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut picks.2).hint_text("new set name").desired_width(130.0));
                    if ui.button("Save Current").on_hover_text("Save the current preferences and units as a defaults set").clicked() && app.dispatch(Command::SaveDefaultsSet { name: picks.2.clone() }).is_ok() {
                        picks.0 = picks.2.trim().to_owned();
                        picks.2.clear();
                    }
                });
            });
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("UI schemes").strong());
                for s in &schemes {
                    if ui.selectable_label(picks.1 == *s, s).clicked() {
                        picks.1 = s.clone();
                    }
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Set").clicked() {
                let _ = app.dispatch(Command::SwitchDefaults { name: picks.0.clone() });
                let _ = app.dispatch(Command::SwitchUiScheme { name: picks.1.clone() });
                done = true;
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, picks));
    if done {
        *open = false;
    }
}

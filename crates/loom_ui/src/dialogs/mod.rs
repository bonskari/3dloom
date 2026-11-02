//! Modeless dialogs opened by commands (`AppState::dialog`).

pub mod rename_objects;
pub mod xref_records;
pub mod wiring;
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
pub mod environment;
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
    // Environment and Effects is a window of its own too (8).
    environment::show(ctx, app);
    // The VertexPaint dialog floats while a VertexPaint entry is active in the Modify panel.
    vertex_paint::show(ctx, app);
    // Render To Texture is modeless: it stays open beside the other dialogs.
    render_to_texture::show(ctx, app);
    material_editor::run_material_preview(app);
    if app.state.bitmap_path_editor_open {
        let mut open = true;
        material_editor::bitmap_path_editor(ctx, app, &mut open);
        if !open {
            app.state.bitmap_path_editor_open = false;
        }
    }
    if app.state.material_explorer_open {
        let mut open = true;
        material_editor::explorer(ctx, app, &mut open);
        if !open {
            app.state.material_explorer_open = false;
        }
    }
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
        Dialog::CleanMultiMaterial => material_editor::clean_multi_material(ctx, app, &mut open),
        Dialog::RenderMap => material_editor::render_map(ctx, app, &mut open),
        Dialog::CopySwapColors => material_editor::copy_swap_colors(ctx, app, &mut open),
        Dialog::MissingMapCoordinates => material_editor::missing_map_coordinates(ctx, app, &mut open),
        Dialog::MaterialPreview => material_editor::material_preview(ctx, app, &mut open),
        Dialog::SpecifyCropping => material_editor::specify_cropping(ctx, app, &mut open),
        Dialog::CopyMap { from, to } => material_editor::copy_map(ctx, app, from, to, &mut open),
        Dialog::RenderFile | Dialog::ViewImageFile | Dialog::SaveRenderedImage | Dialog::SaveRenderPreset | Dialog::LoadRenderPreset | Dialog::RamPlayerFile { .. } => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::RenderSetup => render_setup::show(ctx, app, &mut open),
        Dialog::LightLister => lights::light_lister(ctx, app, &mut open),
        Dialog::LightExclude { light } => lights::light_exclude(ctx, app, light, &mut open),
        Dialog::LightIncludeExclude => lights::light_include_exclude(ctx, app, &mut open),
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
        Dialog::ConfigureModifierSets => configure_modifier_sets(ctx, app, &mut open),
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
        Dialog::ParameterWiring => wiring::parameter_wiring(ctx, app, &mut open),
        Dialog::AddCustomAttributes => wiring::add_custom_attributes(ctx, app, &mut open),
        Dialog::ParameterCollector => wiring::parameter_collector(ctx, app, &mut open),
        Dialog::CustomizeUI => customize_ui::show(ctx, app, &mut open),
        Dialog::PolySettings(kind) => poly_settings::show(ctx, app, kind, &mut open),
        Dialog::Detach => detach::show(ctx, app, &mut open),
        Dialog::DeleteIsolatedVertices => delete_isolated::show(ctx, app, &mut open),
        Dialog::LayerManager => manage_layers::show(ctx, app, &mut open),
        Dialog::OpenFile | Dialog::SaveFileAs | Dialog::SaveFileAsCopy | Dialog::ImportFile | Dialog::SaveAnimation | Dialog::LoadAnimation | Dialog::SaveShortcuts | Dialog::LoadShortcuts | Dialog::SaveCustomUiScheme | Dialog::LoadCustomUiScheme | Dialog::UiFile { .. } | Dialog::ExportFile | Dialog::ExportSelected | Dialog::MergeFile | Dialog::XrefObjectsFile | Dialog::XrefSceneFile | Dialog::InheritContainerFile => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::SaveChanges(action) => save_changes::show(ctx, app, action, &mut open),
        Dialog::CloseSpline => spline_prompts::close_spline(ctx, app, &mut open),
        Dialog::RefineVertex => spline_prompts::refine_vertex(ctx, app, &mut open),
        Dialog::TopologyWarning => topology_warning::show(ctx, app, &mut open),
        Dialog::MakeUnique => make_unique(ctx, app, &mut open),
        Dialog::PropagateLayers => propagate_layers(ctx, app, &mut open),
        Dialog::XViewConfigure => xview_configure(ctx, app, &mut open),
        Dialog::ReactionManager => wiring::reaction_manager(ctx, app, &mut open),
        Dialog::AnimationLayers => wiring::animation_layers(ctx, app, &mut open),
        Dialog::BrushPresetManager => wiring::brush_preset_manager(ctx, app, &mut open),
        Dialog::PickTexture | Dialog::BackgroundFile | Dialog::EnvironmentMap | Dialog::BitmapFit | Dialog::SaveUvw | Dialog::LoadUvw | Dialog::BipedSaveFig | Dialog::BipedLoadFig | Dialog::PickUvTexture | Dialog::PointCacheRecord { .. } | Dialog::PointCacheLoad { .. } | Dialog::PhotometricWeb { .. } => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::UnwrapOptions | Dialog::StitchTool | Dialog::RelaxTool | Dialog::PackUvs | Dialog::FlattenMapping | Dialog::NormalMapping | Dialog::UnfoldMapping | Dialog::PeltMap | Dialog::RenderUvwTemplate => unwrap_dialogs::show(ctx, app, dialog, &mut open),
        Dialog::MergeObjects => merge::show(ctx, app, &mut open),
        Dialog::XrefRecords { scene } => xref_records::show(ctx, app, scene, &mut open),
        Dialog::MergeEffectsFile | Dialog::FileOutputFile { .. } => file_dialog::show(ctx, app, dialog, &mut open),
        Dialog::MergeEffects => environment::merge_effects(ctx, app, &mut open),
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
        Dialog::MissingXrefPaths => assets::missing_xref_paths(ctx, app, &mut open),
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
        Dialog::AssembleName => scene_dialogs::assemble_name(ctx, app, &mut open),
        Dialog::BipedMultipleFootsteps => biped_footsteps::show(ctx, app, &mut open),
        Dialog::LayerProperties { layer } => scene_dialogs::layer_properties(ctx, app, layer, &mut open),
        Dialog::ManageSceneExplorer => crate::scene_explorer::manage(ctx, app, &mut open),
        Dialog::MakePreview => make_preview(ctx, app, &mut open),
        Dialog::ManageSceneStates => manage_scene_states(ctx, app, &mut open),
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

fn configure_modifier_sets(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let slot_id = egui::Id::new("cfg_mod_sets_slot");
    let mut active_slot: usize = ctx.data(|d| d.get_temp(slot_id)).unwrap_or(0);
    let mut set = app.state.modifier_set.clone();
    let mut total = set.len().clamp(1, 32) as u32;
    let mut changed = false;
    let mut close = false;
    egui::Window::new("Configure Modifier Sets").open(open).collapsible(false).resizable(true).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Modifiers:");
                egui::ScrollArea::vertical().max_height(220.0).max_width(180.0).show(ui, |ui| {
                    for name in loom_scene::Modifier::WSM_LIST.iter().chain(loom_scene::Modifier::LIST.iter()) {
                        if ui.selectable_label(false, *name).clicked() && active_slot < set.len() {
                            set[active_slot] = (*name).to_owned();
                            active_slot = (active_slot + 1) % set.len().max(1);
                            changed = true;
                        }
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Total Buttons:");
                    if ui.add(crate::spinner::spin(&mut total, ..).range(1..=32)).changed() {
                        set.resize_with(total as usize, || "Bend".to_owned());
                        active_slot = active_slot.min(set.len().saturating_sub(1));
                        changed = true;
                    }
                });
                ui.separator();
                egui::Grid::new("cfg_mod_sets_grid").num_columns(2).spacing([4.0, 2.0]).show(ui, |ui| {
                    for (i, name) in set.iter().enumerate() {
                        if ui.selectable_label(active_slot == i, name).clicked() {
                            active_slot = i;
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(slot_id, active_slot));
    if changed {
        let _ = app.dispatch(Command::ConfigureModifierSets { names: set });
    }
    if close {
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

fn make_preview(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("make_preview_state");
    let (mut start, mut end, mut nth, mut pct, mut dir): (i32, i32, u32, u32, String) = ctx
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| (app.state.frame_range.0, app.state.frame_range.1, 1, 100, "_preview".into()));
    let mut create = false;
    let mut cancel = false;
    egui::Window::new("Make Preview").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Preview Range").strong());
            ui.horizontal(|ui| {
                ui.label("Start:");
                ui.add(crate::spinner::spin(&mut start, ..));
                ui.label("End:");
                ui.add(crate::spinner::spin(&mut end, ..));
                ui.label("Every Nth Frame:");
                ui.add(crate::spinner::spin(&mut nth, ..).range(1..=100));
            });
        });
        ui.group(|ui| {
            ui.label(egui::RichText::new("Image Size").strong());
            ui.horizontal(|ui| {
                ui.label("Percent of Output:");
                ui.add(crate::spinner::spin(&mut pct, ..).range(10..=200).suffix("%"));
            });
        });
        ui.group(|ui| {
            ui.label(egui::RichText::new("Output").strong());
            ui.horizontal(|ui| {
                ui.label("Folder:");
                ui.text_edit_singleline(&mut dir);
            });
        });
        ui.horizontal(|ui| {
            if ui.button("Create").clicked() {
                create = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, (start, end, nth, pct, dir.clone())));
    if create {
        let _ = app.dispatch(Command::MakePreview {
            path: Some(dir),
            start: Some(start),
            end: Some(end),
            every_nth: Some(nth),
            percent: Some(pct),
        });
        *open = false;
    } else if cancel {
        *open = false;
    }
}

/// Make Unique with several objects selected: Yes makes each unique, No gives
/// them one instance of their own, Cancel leaves them.
/// Unhide / Unfreeze of objects on hidden or frozen layers (Preferences >
/// Propagate ... > Ask): apply to the layers too?
fn propagate_layers(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let mut answer = None;
    let what = match app.state.pending_layer_propagation.as_ref().map(|p| (p.1, p.2)) {
        Some((true, _)) => "unhide",
        _ => "unfreeze",
    };
    egui::Window::new("Propagate to Layers").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.label(format!("Some objects are on layers that are hidden or frozen. Also {what} those layers?"));
        ui.horizontal(|ui| {
            if ui.button("Yes").clicked() {
                answer = Some(true);
            }
            if ui.button("No").clicked() {
                answer = Some(false);
            }
        });
    });
    if let Some(yes) = answer {
        let _ = app.dispatch(Command::PropagateLayerAnswer { yes });
    }
}

fn make_unique(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let mut answer: Option<Option<Command>> = None;
    egui::Window::new("Make Unique").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.label("Do you want to make the selected items unique with respect to each other?");
        ui.horizontal(|ui| {
            if ui.button("Yes").clicked() {
                answer = Some(Some(Command::MakeUnique));
            }
            if ui.button("No").clicked() {
                answer = Some(Some(Command::MakeUniqueShared));
            }
            if ui.button("Cancel").clicked() {
                answer = Some(None);
            }
        });
    });
    if let Some(a) = answer {
        let _ = app.dispatch(Command::CloseDialog);
        if let Some(c) = a {
            let _ = app.dispatch(c);
        }
    }
}

/// xView > Configure: the active test's Tolerance (world units).
fn xview_configure(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let title = app.state.xview.test.map_or("xView", |t| t.label()).to_owned();
    egui::Window::new(title).id(egui::Id::new("xview_configure")).open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Tolerance:");
            let mut v = app.state.xview.tolerance;
            if ui.add(crate::spinner::spin(&mut v, 0.0..=1_000_000.0).speed(0.001).fixed_decimals(3)).changed() {
                let _ = app.dispatch(Command::XViewTolerance { value: v });
            }
        });
        if ui.button("OK").clicked() {
            let _ = app.dispatch(Command::CloseDialog);
        }
    });
}

fn manage_scene_states(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("manage_scene_states_ui");
    let (mut selected, mut new_name, mut parts): (String, String, [bool; 5]) = ctx
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| (String::new(), "SceneState01".into(), [true; 5]));
    let part_names = [
        "Light Properties",
        "Light Transforms",
        "Object Properties",
        "Camera Transforms",
        "Materials",
    ];
    let states: Vec<String> = app.state.scene.scene_states.iter().map(|s| s.name.clone()).collect();
    if selected.is_empty() {
        if let Some(first) = states.first() {
            selected = first.clone();
        }
    }
    let mut action: Option<Command> = None;
    let mut close = false;
    egui::Window::new("Manage Scene States").open(open).collapsible(false).resizable(true).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Scene States:").strong());
                egui::ScrollArea::vertical().max_height(160.0).min_scrolled_width(160.0).show(ui, |ui| {
                    for name in &states {
                        if ui.selectable_label(&selected == name, name).clicked() {
                            selected = name.clone();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut new_name);
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Parts:").strong());
                for (i, label) in part_names.iter().enumerate() {
                    ui.checkbox(&mut parts[i], *label);
                }
            });
        });
        ui.separator();
        let chosen_parts: Vec<String> = part_names
            .iter()
            .enumerate()
            .filter(|&(i, _p)| parts[i]).map(|(_i, p)| (*p).to_owned())
            .collect();
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() && !new_name.trim().is_empty() {
                selected = new_name.trim().to_owned();
                action = Some(Command::SaveSceneState {
                    name: selected.clone(),
                    parts: Some(chosen_parts.clone()),
                });
            }
            if ui.add_enabled(!selected.is_empty(), egui::Button::new("Restore")).clicked() {
                action = Some(Command::RestoreSceneState {
                    name: selected.clone(),
                    parts: Some(chosen_parts.clone()),
                });
            }
            if ui.add_enabled(!selected.is_empty(), egui::Button::new("Delete")).clicked() {
                action = Some(Command::DeleteSceneState { name: selected.clone() });
            }
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, (selected, new_name, parts)));
    if let Some(cmd) = action {
        let _ = app.dispatch(cmd);
    }
    if close {
        *open = false;
    }
}

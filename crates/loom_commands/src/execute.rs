//! Executes commands against `AppState`. Every scene mutation here goes
//! through `UndoStack::commit`, so undo always matches what the user did.

use crate::command::{Axis, Command, PrimitiveKind, SelectOp, ShapeKind};
use crate::state::{AlignPoint, AppState, DialogPurpose, NormalAlignPicks, SpacingContext, PickMode, SelectionMode, SubObjectId, Tool, ViewKind};
use glam::Mat3;
use crate::viewport::Dialog as Dlg;
use crate::state::{AxisConstraint, CenterMode, CloneKind, CoordSystem, SavedManagers};
use crate::transform::frame_for;
use loom_scene::NodeProps;
use crate::viewport::{default_kinds, Dialog, NavMode, Shading, Viewport};
use glam::{Mat4, Quat, Vec3};
use loom_mesh::{edit_ops, FaceId, HalfEdgeId, Mesh, VertexId};
use loom_scene::{Modifier, Change, Node, NodeId, ObjectKind, Transform};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    // Every edit happens at the time slider's frame, keyed when Auto Key is on.
    state.scene.set_time(state.current_frame as f32);
    let animate = loom_scene::anim::animate();
    loom_scene::anim::set_animate(state.auto_key && !state.set_key_mode);
    loom_scene::anim::set_default_tangents(state.default_tangent);
    // Set Key mode keeps only pose edits in its buffer: any other edit
    // (creating, deleting, modifiers…) becomes the buffer's new start.
    let rebases_pose = state.set_key_mode && !crate::anim_cmds::is_pose_edit(&cmd);
    let position = state.undo.position();
    // Selection undo: the outermost command records what it did to the selection.
    let outer = crate::sel_undo::enter(state);
    // Undo / Redo restore selections themselves; Open, Reset and Fetch start the history over.
    let undo_cmd = matches!(cmd, Command::UndoSceneOperation | Command::RedoSceneOperation);
    if outer && matches!(cmd, Command::OpenFile { .. } | Command::ResetFile | Command::Fetch) {
        crate::sel_undo::forget(state);
    }
    let undo_cmd = undo_cmd || matches!(cmd, Command::OpenFile { .. } | Command::ResetFile | Command::Fetch);
    let sel_before = (outer && !undo_cmd).then(|| crate::sel_undo::snap(state));
    let r = execute_inner(state, cmd);
    crate::sel_undo::leave(state);
    // Axis constraints are kept separately at object and sub-object level.
    let sub = state.selection_mode != SelectionMode::Object;
    if sub != state.constraint_level_sub && !state.prefs.ref_coord_constant {
        let t = tool_index(state.active_tool);
        state.constraint_by_tool[state.constraint_level_sub as usize][t] = state.axis_constraint;
        state.axis_constraint = state.constraint_by_tool[sub as usize][t];
    }
    state.constraint_level_sub = sub;
    if rebases_pose && state.set_key_mode && state.undo.position() != position {
        crate::anim_cmds::start_pose(state);
    }
    loom_scene::anim::set_animate(animate);
    state.scene.set_time(state.current_frame as f32);
    crate::stack::sync_selection(state);
    if let Some(before) = sel_before {
        crate::sel_undo::record(state, before, position);
    }
    r
}

fn execute_inner(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::State => Ok(snapshot(state)),
        C::MoveAnimationClip { id, start } => crate::animation_clips::move_clip(state, id, start),
        C::TrimAnimationClip { id, start, end } => crate::animation_clips::trim_clip(state, id, start, end),
        c @ (C::SetLoomRigBlendKey { .. } | C::MoveLoomRigBlendKey { .. }) => crate::loomrig_keys::execute(state,c),
        C::RetargetAnimationClip { source_clip, target_root, mapping } => crate::retarget::retarget(state, source_clip, NodeId(target_root), mapping.as_deref().unwrap_or(&[])),
        C::SplineSelectSet { level, ids, op } => spline_select_set(state, level, ids, op),
        c if crate::anim_cmds::handles(&c) => crate::anim_cmds::execute(state, c),
        c if crate::material::handles(&c) => crate::material::execute(state, c),
        c if crate::display::handles(&c) => crate::display::execute(state, c),
        c if crate::render::handles(&c) => crate::render::execute(state, c),
        c if crate::bake::handles(&c) => crate::bake::execute(state, c),
        c if crate::controllers::handles(&c) => crate::controllers::execute(state, c),
        c if crate::motion::handles(&c) => crate::motion::execute(state, c),
        c if c.group() == crate::Group::EditNormals || crate::normals::handles(&c) => crate::normals::execute(state, c),
        c if crate::vertex_paint::handles(&c) => crate::vertex_paint::execute(state, c),
        c if crate::mesh_edit::handles(&c) => crate::mesh_edit::execute(state, c),
        c if crate::patch_edit::handles(&c) => crate::patch_edit::execute(state, c),
        C::AssignVertexColors { light_model, by_vertex, shadows, update } => crate::assign_vertex_colors::assign(state, light_model, by_vertex, shadows, update),
        c if c.group() == crate::Group::TrackView || crate::track_view::handles(&c) => crate::track_view::execute(state, c),
        C::UnwrapUvwModifier => crate::unwrap::execute(state, cmd),
        c if c.group() == crate::Group::UnwrapUVW => crate::unwrap::execute(state, c),
        c if c.group() == crate::Group::Skin => crate::skin::execute(state, c),
        c @ (C::SkinAddBones { .. } | C::SkinRemoveBone { .. } | C::SkinSelectBone { .. } | C::SkinSelectVertices { .. } | C::SkinSetVertexWeight { .. } | C::SkinSetEnvelope { .. } | C::SkinSetCustomWeight { .. } | C::SkinNormalize | C::SkinRigidToggle | C::SkinBakeSelectedVerts | C::MirrorModeSkin | C::PasteGreenToBlue | C::PasteBlueToGreen | C::MirrorPaste | C::SkinAddGizmo { .. } | C::SkinRemoveGizmo | C::SkinSelectGizmo { .. } | C::SkinAddGizmoKey | C::SkinSetGizmoKey { .. } | C::SkinSetGizmoDelta { .. } | C::SkinSetGizmoOption { .. } | C::SkinSetVertexWeights { .. } | C::SkinSetScaleWeight { .. } | C::SkinSetPasteTolerance { .. } | C::SkinSelectBoneByName { .. } | C::SkinEnvelopeVisibility | C::SkinPaintWeightsToggle | C::SkinPaintStroke { .. } | C::SkinSetPainterOption { .. } | C::SkinPainterOptionsDialog) => crate::skin::execute(state, c),
        c if crate::skin::handles(&c) => crate::skin::execute(state, c),
        c @ (C::MorpherModifier | C::MorpherPickTarget { .. } | C::MorpherCaptureCurrentState { .. } | C::MorpherSetChannel { .. } | C::MorpherRenameChannel { .. } | C::MorpherDeleteChannel { .. } | C::MorpherAddProgressiveTarget { .. } | C::MorpherSetProgressivePercent { .. } | C::MorpherReload | C::MorpherExtractTarget { .. } | C::MorpherSetGlobal { .. } | C::SkinMorphAddBones { .. } | C::SkinMorphRemoveBone { .. } | C::SkinMorphCreateMorph { .. } | C::SkinMorphSetVertexDelta { .. } | C::SkinMorphSetMorph { .. } | C::SkinMorphRenameMorph { .. } | C::SkinMorphDeleteMorph { .. } | C::SkinMorphReload { .. } | C::SkinWrapAddBase { .. } | C::SkinWrapRemoveBase { .. } | C::SkinWrapSetOption { .. } | C::SkinWrapReset | C::SkinWrapConvertToSkin | C::SkinMorphSelect { .. } | C::SkinMorphPickBone | C::SkinMorphSelectPoints { .. } | C::SkinMorphSelectionTool { .. } | C::SkinMorphSetOption { .. } | C::SkinMorphSetCurve { .. } | C::SkinMorphResetGraph | C::SkinMorphEdit { .. } | C::SkinMorphClearVerts { .. } | C::SkinMorphRemoveVerts { .. } | C::SkinMorphSetJointType { .. } | C::SkinMorphSetExternal { .. } | C::SkinMorphPickExternal | C::SkinMorphReloadTarget { .. } | C::SkinMorphPasteMirror { .. } | C::SkinWrapPickControl | C::SkinWrapSelectControlVerts { .. } | C::SkinWrapSetLocal { .. } | C::SkinWrapMirrorSelected | C::SkinWrapBakeControlVerts | C::SkinWrapRetrieveControlVerts | C::SkinWrapPatchPick { .. } | C::SkinWrapPatchSetSampleRate { .. } | C::SkinWrapPatchResample) => crate::morph::execute(state, c),
        C::NewScene => {
            state.scene.renew_instance();
            let ids: Vec<NodeId> = state.scene.nodes().map(|n| n.id).collect();
            if ids.is_empty() {
                state.scene.animation_clips.clear();
                state.scene.loom_rigs.clear();
                state.file = None;
                state.undo.mark_saved();
                return Ok(json!({}));
            }
            let change = Change::Group(ids.into_iter().map(Change::RemoveNode).collect());
            commit(state, "New Scene", change)?;
            clear_selection(state);
            prune_camera_views(state);
            // New gives an untitled, unmodified scene.
            state.file = None;
            state.undo.mark_saved();
            Ok(json!({}))
        }
        C::UndoSceneOperation => {
            let label = state.undo.undo(&mut state.scene);
            if label.is_some() {
                let timing_id = state.undo.redo_entry_id();
                crate::timing_undo::restore(state, timing_id, false);
            }
            // The step undone sat at the position just above the new top.
            let pos = state.undo.position() + 1;
            if label.is_none() || !crate::sel_undo::undone(state, pos) {
                prune_selection(state);
            }
            state.mesh_dirty = true;
            Ok(json!({ "undone": label }))
        }
        C::RedoSceneOperation => {
            let label = state.undo.redo(&mut state.scene);
            if label.is_some() {
                let timing_id = state.undo.undo_entry_id();
                crate::timing_undo::restore(state, timing_id, true);
            }
            let pos = state.undo.position();
            if label.is_none() || !crate::sel_undo::redone(state, pos) {
                prune_selection(state);
            }
            state.mesh_dirty = true;
            Ok(json!({ "redone": label }))
        }
        C::UndoViewportOperation => {
            let cur = state.view_state();
            let vp = state.active_mut();
            if let Some(v) = vp.view_undo.undo(cur) {
                vp.camera = v.camera;
                vp.kind = v.view_kind;
                vp.fov_y_radians = v.fov_y_radians;
            }
            Ok(camera_json(state))
        }
        C::RedoViewportOperation => {
            let cur = state.view_state();
            let vp = state.active_mut();
            if let Some(v) = vp.view_undo.redo(cur) {
                vp.camera = v.camera;
                vp.kind = v.view_kind;
                vp.fov_y_radians = v.fov_y_radians;
            }
            Ok(camera_json(state))
        }
        C::OpenFile { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::OpenFile);
                return Ok(json!({ "dialog": "OpenFile" }));
            };
            let path = std::path::PathBuf::from(path);
            let (doc, warnings) = crate::file::read_any(&path)?;
            crate::settings::note_recent(state, &path);
            // A .max scene opens untitled: Save asks for a .loom file (there is no .max writer).
            let max = crate::max_io::is_max(&path);
            load_document(state, doc, (!max).then_some(path));
            if max {
                state.import_report = warnings;
            }
            // Missing External Files: the files the scene names that are not there.
            let missing = crate::assets::missing_assets(state);
            if !missing.is_empty() && state.dialog.is_none() {
                state.dialog = Some(Dlg::MissingFiles);
            }
            Ok(json!({ "nodes": state.scene.len(), "missing": missing }))
        }
        C::SaveFile { path } => {
            let target = match (path, state.file.clone()) {
                (Some(p), _) => std::path::PathBuf::from(p),
                (None, Some(f)) => f,
                (None, None) => {
                    state.dialog = Some(Dlg::SaveFileAs);
                    return Ok(json!({ "dialog": "SaveFileAs" }));
                }
            };
            save_to(state, &target)?;
            crate::settings::note_recent(state, &target);
            state.file = Some(target.clone());
            state.undo.mark_saved();
            run_pending(state)?;
            Ok(json!({ "path": target }))
        }
        C::SaveFileAs { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::SaveFileAs);
                return Ok(json!({ "dialog": "SaveFileAs" }));
            };
            let target = std::path::PathBuf::from(path);
            save_to(state, &target)?;
            crate::settings::note_recent(state, &target);
            state.file = Some(target.clone());
            state.undo.mark_saved();
            run_pending(state)?;
            Ok(json!({ "path": target }))
        }
        C::SaveFileAsCopy { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::SaveFileAsCopy);
                return Ok(json!({ "dialog": "SaveFileAsCopy" }));
            };
            let target = std::path::PathBuf::from(path);
            save_to(state, &target)?;
            Ok(json!({ "path": target }))
        }
        C::SaveIncremental => {
            let Some(current) = state.file.clone() else {
                state.dialog = Some(Dlg::SaveFileAs);
                return Ok(json!({ "dialog": "SaveFileAs" }));
            };
            let target = incremental_path(&current);
            save_to(state, &target)?;
            crate::settings::note_recent(state, &target);
            state.file = Some(target.clone());
            state.undo.mark_saved();
            run_pending(state)?;
            Ok(json!({ "path": target }))
        }
        C::ResetFile => {
            let fresh = AppState::new();
            state.scene = fresh.scene;
            state.key_time_cache = Default::default();
            state.viewports = fresh.viewports;
            state.layout = fresh.layout;
            state.active_viewport = 0;
            state.units = state.default_units.clone();
            state.frame_range = fresh.frame_range;
            state.fps = fresh.fps;
            state.current_frame = fresh.current_frame;
            state.named_sets.clear();
            state.named_sub_selections.clear();
            state.undo = loom_scene::UndoStack::new(state.undo.limit());
            state.autosave.last_version = None;
            clear_selection(state);
            state.pick_mode = None;
            state.dialog = None;
            state.file = None;
            state.mesh_dirty = true;
            // Reset starts from the startup scene when there is one.
            let startup = crate::assets::load_startup_scene(state)?;
            Ok(json!({ "startup_scene": startup }))
        }
        C::SaveChangesReply { reply } => {
            let Some(Dlg::SaveChanges(action)) = state.dialog else {
                return Err("no Save Changes prompt is open".into());
            };
            state.dialog = None;
            match reply {
                crate::command::SaveChangesReply::Cancel => Ok(json!({ "cancelled": true })),
                crate::command::SaveChangesReply::No => file_action(state, action),
                crate::command::SaveChangesReply::Yes => {
                    state.pending_file_action = Some(action);
                    // Saves and runs the action, or opens Save As and leaves it pending.
                    execute(state, C::SaveFile { path: None })
                }
            }
        }
        C::Hold => {
            let path = crate::autosave::hold_path();
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            save_to(state, &path)?;
            Ok(json!({ "path": path }))
        }
        C::Fetch => {
            let path = crate::autosave::hold_path();
            if !path.exists() {
                return Err("nothing held".into());
            }
            let doc = crate::file::read(&path)?;
            // Fetch is not undoable; the fetched scene is unsaved work.
            let file = state.file.clone();
            load_document(state, doc, file);
            state.undo.mark_unsaved();
            Ok(json!({ "nodes": state.scene.len() }))
        }
        C::SetTextureDisplay { id, kind, path } => {
            let texture = match kind.to_ascii_lowercase().as_str() {
                "none" => loom_scene::TextureDisplay::None,
                "checker" => loom_scene::TextureDisplay::Checker,
                "bitmap" => loom_scene::TextureDisplay::Bitmap(path.filter(|p| !p.trim().is_empty()).ok_or("a bitmap needs a path")?),
                other => return Err(format!("unknown texture display {other:?} (none / checker / bitmap)")),
            };
            props_change(state, "Show Map in Viewport", vec![NodeId(id)], |p, _| {
                p.display.texture = texture.clone();
            })?;
            Ok(json!({}))
        }
        C::UvwMapModifier => execute(state, C::AddModifier { name: "UVW Map".into() }),
        C::UvwMapFit | C::UvwMapCenter | C::UvwMapBitmapFit { .. } | C::UvwMapNormalAlign | C::UvwMapViewAlign | C::UvwMapRegionFit { .. } | C::UvwMapReset | C::UvwMapAcquire { .. } | C::UvwMapAlignTo { .. } => {
            crate::stack::execute(state, cmd)
        }
        C::ImportAnimationClip { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::ImportAnimationClip);
                return Ok(json!({ "dialog": "ImportAnimationClip" }));
            };
            let path = std::path::PathBuf::from(path);
            let (ids, take) = if crate::fbx_io::is_fbx(&path) {
                crate::fbx_io::import_animation(state, &path)?
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bvh")) {
                (crate::bvh_io::import_animation(state, &path)?, None)
            } else {
                return Err("animation clips support .bvh and animated .fbx files".into());
            };
            Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "clip": state.scene.animation_clips.last().map(|c| c.id), "take": take }))
        }
        C::ImportFile { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::ImportFile);
                return Ok(json!({ "dialog": "ImportFile" }));
            };
            let path = std::path::PathBuf::from(path);
            if crate::max_io::is_max(&path) {
                return crate::max_io::import(state, &path);
            }
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bvh")) {
                let ids = crate::bvh_io::import(state, &path)?;
                return Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "clip": state.scene.animation_clips.last().map(|c| c.id) }));
            }
            if crate::fbx_io::is_fbx(&path) {
                let ids = crate::fbx_io::import(state, &path)?;
                return Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>() }));
            }
            if crate::geom_io::is_3ds(&path) {
                return crate::geom_io::import_3ds(state, &path);
            }
            if crate::geom_io::is_stl(&path) {
                return crate::geom_io::import_stl(state, &path);
            }
            if crate::geom_io::is_dxf(&path) {
                return crate::geom_io::import_dxf(state, &path);
            }
            crate::obj_io::import(state, &path)
        }
        C::ExportFile { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::ExportFile);
                return Ok(json!({ "dialog": "ExportFile" }));
            };
            // OBJ Hidden Objects: hidden objects go too.
            let hidden = !crate::fbx_io::is_fbx(std::path::Path::new(&path)) && state.exchange.obj_export.hidden_objects;
            let ids: Vec<NodeId> = state.scene.nodes().filter(|n| hidden || state.is_visible(n)).map(|n| n.id).collect();
            export_any(state, &ids, std::path::Path::new(&path))
        }
        C::SetExchangeOption { dialog, name, value } => {
            state.exchange.set(&dialog, &name, value)?;
            state.exchange.describe(&dialog)
        }
        C::ExchangeOptions { dialog } => state.exchange.describe(&dialog),
        C::ReplaceFile { path, names, materials } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::ReplaceFile);
                return Ok(json!({ "dialog": "ReplaceFile" }));
            };
            let path = std::path::PathBuf::from(path);
            if names.is_none() && materials.is_none() && state.replace_source.as_ref() != Some(&path) {
                // From the file dialog: the list of matching objects comes next.
                let doc = crate::file::read(&path)?;
                let candidates = crate::assets::replace_candidates(state, &doc);
                state.replace_source = Some(path);
                state.dialog = Some(Dlg::ReplaceObjects);
                return Ok(json!({ "dialog": "ReplaceObjects", "objects": candidates }));
            }
            state.replace_source = None;
            crate::assets::replace(state, &path, names.as_deref(), materials.unwrap_or(false))
        }
        C::SetProjectFolder { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::ProjectFolder);
                return Ok(json!({ "dialog": "ProjectFolder" }));
            };
            crate::assets::set_project_folder(state, std::path::Path::new(&path))
        }
        C::AssetTracking => {
            state.dialog = Some(Dlg::AssetTracking);
            Ok(crate::assets::asset_list(state))
        }
        C::RepathAsset { from, to } => {
            let n = crate::assets::repath(state, &[(from, to)], "Set Path")?;
            Ok(json!({ "changed": n }))
        }
        C::FindMissingFiles { folder } => {
            let found = crate::assets::find_missing(state, std::path::Path::new(&folder))?;
            Ok(json!({ "found": found, "missing": crate::assets::missing_assets(state) }))
        }
        C::SaveExchangePreset { dialog, name } => {
            state.exchange.save_preset(&dialog, &name)?;
            state.exchange.describe(&dialog)
        }
        C::DeleteExchangePreset { dialog, name } => {
            state.exchange.delete_preset(&dialog, &name)?;
            state.exchange.describe(&dialog)
        }
        C::FbxFileInfo { path } => crate::fbx_io::file_info(state, std::path::Path::new(&path)),
        C::ObjFileInfo { path } => crate::obj_io::file_info(std::path::Path::new(&path)),
        C::ExportSelected { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::ExportSelected);
                return Ok(json!({ "dialog": "ExportSelected" }));
            };
            let ids: Vec<NodeId> = state.selection.iter().copied().collect();
            export_any(state, &ids, std::path::Path::new(&path))
        }
        C::MergeFile { path, names, duplicates } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::MergeFile);
                return Ok(json!({ "dialog": "MergeFile" }));
            };
            let path = std::path::PathBuf::from(path);
            let (doc, warnings) = crate::file::read_any(&path)?;
            let label = format!("Merge {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            let ids = crate::file::merge(state, &doc, names.as_deref(), duplicates.as_deref().unwrap_or_default(), &label)?;
            if crate::max_io::is_max(&path) {
                state.import_report = warnings;
            }
            state.selection_mode = SelectionMode::Object;
            Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
        }
        C::XrefObjects { path, names, automatic } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::XrefObjectsFile);
                return Ok(json!({ "dialog": "XrefObjectsFile" }));
            };
            state.selection_mode = SelectionMode::Object;
            crate::xref::add(state, std::path::Path::new(&path), names.as_deref(), false, automatic.unwrap_or(true))
        }
        C::XrefScene { path, automatic } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::XrefSceneFile);
                return Ok(json!({ "dialog": "XrefSceneFile" }));
            };
            crate::xref::add(state, std::path::Path::new(&path), None, true, automatic.unwrap_or(true))
        }
        C::UpdateXrefs { path } => crate::xref::update(state, path.as_deref()),
        C::RemoveXref { path } => crate::xref::remove(state, &path),
        C::AutoBackupNow => {
            let path = crate::autosave::backup_now(state)?;
            Ok(json!({ "path": path }))
        }
        C::SetAutoBackupName { name } => {
            let name = name.trim().to_owned();
            if name.is_empty() || name.contains(['/', '\\']) {
                return Err(format!("{name:?} is not a file name"));
            }
            state.prefs.auto_backup_name = name.clone();
            Ok(json!({ "name": name }))
        }

        C::SelectAll => {
            if state.selection_locked {
                return Ok(json!({ "locked": true }));
            }
            match state.selection_mode {
                SelectionMode::Object => {
                    state.selection = state
                        .scene
                        .nodes()
                        .filter(|n| state.is_selectable(n))
                        .map(|n| n.id)
                        .collect();
                }
                mode => {
                    let targets: Vec<NodeId> = state.selection.iter().copied().collect();
                    state.sub_selection.clear();
                    for id in targets {
                        if let Some(node) = state.scene.node(id) {
                            for sid in all_sub_ids(&node.cage_mesh(), mode) {
                                state.sub_selection.insert(id, sid);
                            }
                        }
                    }
                }
            }
            state.refresh_center_mode();
            Ok(selection_json(state))
        }
        C::SelectNone => {
            if !state.selection_locked {
                match state.selection_mode {
                    SelectionMode::Object => clear_selection(state),
                    _ => state.sub_selection.clear(),
                }
            }
            Ok(selection_json(state))
        }
        C::SelectInvert => {
            if state.selection_locked {
                return Ok(json!({ "locked": true }));
            }
            match state.selection_mode {
                SelectionMode::Object => {
                    let all: BTreeSet<NodeId> = state
                        .scene
                        .nodes()
                        .filter(|n| state.is_selectable(n))
                        .map(|n| n.id)
                        .collect();
                    state.selection = all.difference(&state.selection).copied().collect();
                }
                mode => {
                    let targets: Vec<NodeId> = state.selection.iter().copied().collect();
                    for id in targets {
                        if let Some(node) = state.scene.node(id) {
                            for sid in all_sub_ids(&node.cage_mesh(), mode) {
                                state.sub_selection.toggle(id, sid);
                            }
                        }
                    }
                }
            }
            Ok(selection_json(state))
        }
        C::DeleteObjects => delete(state, None),
        C::DeletePolygons { delete_isolated } => {
            if state.dialog == Some(Dialog::DeleteIsolatedVertices) {
                state.dialog = None;
            }
            if !state.selection_mode.is_face_level() {
                return Err("Delete Polygons works at Polygon and Element level".into());
            }
            delete(state, Some(delete_isolated))
        }
        C::Clone => {
            if state.selection.is_empty() {
                return Ok(json!({}));
            }
            state.dialog = Some(Dlg::CloneOptions);
            Ok(json!({}))
        }
        C::CloneOptions { kind, copies, name, offset, rotate_deg, scale } => {
            // A closed group clones with its members (they are what the user sees).
            let ids: Vec<NodeId> = with_group_members(state, state.selection.iter().copied().collect());
            if ids.is_empty() {
                return Ok(json!({}));
            }
            let copies = copies.max(1);
            state.undo.begin("Clone");
            let mut all_new = Vec::new();
            let r: Result<(), String> = (|| {
                for i in 1..=copies {
                    let mut changes = Vec::new();
                    let mut new_ids = Vec::new();
                    let mut sim = state.scene.clone();
                    // Parents before children so a cloned child can hang under its cloned parent.
                    let mut ordered = ids.clone();
                    ordered.sort_by_key(|id| state.scene.depth(*id));
                    let mut remap: std::collections::HashMap<NodeId, NodeId> = std::collections::HashMap::new();
                    for (k, id) in ordered.iter().enumerate() {
                        let Some(src) = state.scene.node(*id) else { continue };
                        let mut node = src.clone();
                        node.id = NodeId(0);
                        node.name = match (&name, i, k) {
                            (Some(n), 1, 0) if !n.is_empty() => n.clone(),
                            (Some(n), _, _) if !n.is_empty() => sim.next_name(base_of(n)),
                            _ => sim.next_name(base_of(&src.name)),
                        };
                        node.instance_of = if kind == CloneKind::Copy { None } else { Some(src.instance_of.unwrap_or(*id)) };
                        if let Some(p) = node.parent.and_then(|p| remap.get(&p)) {
                            node.parent = Some(*p);
                        }
                        let new_id = sim.add_node(node.clone());
                        node.id = new_id;
                        remap.insert(*id, new_id);
                        new_ids.push(new_id);
                        changes.push(Change::AddNode(node));
                    }
                    commit(state, "Clone", Change::Group(changes))?;
                    state.selection = new_ids.iter().copied().collect();
                    state.sub_selection.clear();
                    for _ in 0..i {
                        if let Some(d) = offset {
                            execute(state, C::MoveSelection { delta: d, system: None })?;
                        }
                        if let Some(r) = rotate_deg {
                            for (axis, deg) in [(Axis::X, r[0]), (Axis::Y, r[1]), (Axis::Z, r[2])] {
                                if deg != 0.0 {
                                    execute(state, C::RotateSelection { axis, degrees: deg, system: None })?;
                                }
                            }
                        }
                        if let Some(f) = scale {
                            execute(state, C::ScaleSelection { factors: f, system: None })?;
                        }
                    }
                    all_new.extend(new_ids);
                }
                Ok(())
            })();
            state.undo.end();
            r?;
            state.selection = all_new.into_iter().collect();
            state.refresh_center_mode();
            Ok(selection_json(state))
        }
        C::TransformTypeIn { absolute, values } => transform_type_in(state, absolute, Vec3::from(values)),
        C::TransformTypeInDialogToggle => {
            state.dialog = if state.dialog == Some(Dlg::TransformTypeIn) { None } else { Some(Dlg::TransformTypeIn) };
            Ok(json!({ "open": state.dialog.is_some() }))
        }
        C::SpinnerSnapToggle => toggle(&mut state.prefs.spinner_snap, "spinner_snap"),
        C::KeepCreatingToggle => toggle(&mut state.prefs.keep_creating, "keep_creating"),
        C::ShowRibbonToggle => toggle(&mut state.show_ribbon, "show_ribbon"),
        C::EditModifierStack | C::AddModifier { .. } | C::DeleteModifier { .. } | C::MoveModifier { .. } | C::CopyModifier { .. } | C::ToggleModifier { .. } | C::ToggleModifierInViewport { .. }
        | C::RenameModifier { .. } | C::SelectStackEntry { .. } | C::SetModifierParameter { .. } | C::ModifierSubobjectLevel { .. } | C::ShowEndResultToggle | C::PinStackToggle
        | C::MakeUnique | C::InstanceModifier { .. } | C::CollapseTo { .. } | C::CollapseAll | C::ConfigureModifierSets { .. } | C::ModifierSetButtonsToggle
        | C::PickModifierNode { .. } | C::SwitchToControlPointLevel | C::SwitchToLatticeLevel | C::SwitchToSetVolumeLevel | C::SwitchToTopLevel | C::SelectControlPoints { .. }
        | C::ResetLattice | C::ConformToShape | C::SetNumberOfPoints { .. } | C::VertexPaintBrushToggle | C::VertexPaintStroke { .. } => crate::stack::execute(state, cmd),
        C::AttachSpline | C::AttachMultipleSpline | C::BreakVerticesSpline | C::ChamferVertexSpline | C::CloseSpline | C::ConnectSpline | C::CreateLineSpline | C::CycleVerticesSpline
        | C::DeleteSegmentSpline | C::DeleteSplineSpline | C::DeleteVertexSpline | C::DetachSegmentSpline | C::DetachSplineSpline | C::DivideSpline | C::ExplodeSpline | C::FilletSpline
        | C::FuseVerticesSpline | C::HideSpline | C::UnhideAllSpline | C::InsertSelectionSpline | C::MakeFirstSpline | C::MirrorBothSpline | C::MirrorHorizontallySpline | C::MirrorVerticallySpline
        | C::OutlineSpline | C::RefineSpline | C::RefineConnectSpline | C::ReverseSpline | C::SubtractSpline | C::WeldVerticesSpline | C::SplineRefine { .. } | C::SplineRefineAtVertex { .. } | C::SplineRefineReply { .. } | C::SplineFilletMode { .. } | C::SplineFillet { .. }
        | C::SplineChamfer { .. } | C::SetVertexType { .. } | C::SetKnotHandle { .. } | C::SplineOutline { .. } | C::SplineBoolean { .. } | C::SplineMirror { .. } | C::SplineDivide { .. }
        | C::SplineWeld { .. } | C::SplineFuse | C::SplineConnect { .. } | C::SplineAttach { .. } | C::SplineDetach { .. } | C::SplineExplode { .. } | C::SplineSettings { .. }
        | C::CreateShapeFromEdges { .. } | C::ExtrudeAlongSpline { .. } | C::BindSpline | C::UnbindSpline | C::TrimSpline | C::ExtendSpline | C::CrossInsertSpline
        | C::SplineTrim { .. } | C::SplineExtend { .. } | C::SplineCrossInsert { .. } | C::SplineBind { .. } | C::SplineCrossSection { .. } | C::SplineTangentCopy { .. }
        | C::SplineTangentPaste { .. } | C::FilletChamferApply { .. } | C::TrimExtendPick { .. } => crate::spline::execute(state, cmd),
        C::SplineIkControlCreateHelpers => crate::stack::spline_ik_create_helpers(state),
        C::HsdsSubdivide { faces } => crate::stack::hsds_subdivide(state, faces),
        C::HsdsAdaptiveSubdivide { max_level, max_angle } => crate::stack::hsds_adaptive(state, max_level, max_angle),
        C::HsdsSelectLevel { level } => crate::stack::hsds_select_level(state, level),
        C::HsdsDisplay { level } => crate::stack::hsds_display(state, level),
        C::PointCacheRecord { index, path, start, end } => crate::stack::point_cache_record(state, index as usize, &path, start, end),
        C::PointCacheLoad { index, path } => crate::stack::point_cache_load(state, index as usize, Some(&path)),
        C::PointCacheUnload { index } => crate::stack::point_cache_load(state, index as usize, None),
        C::EditSoftSelection => toggle(&mut state.soft_edit, "soft_edit"),
        C::SelectAncestor => {
            let ids: Vec<u64> = state.selection.iter().filter_map(|id| state.scene.node(*id).and_then(|n| n.parent)).map(|p| p.0).collect();
            if ids.is_empty() {
                return Ok(selection_json(state));
            }
            execute(state, C::SelectNodes { ids, op: SelectOp::Replace })
        }
        C::SelectChild => {
            // Page Down: all immediate children.
            let ids: Vec<u64> = state.selection.iter().flat_map(|id| state.scene.children(*id)).map(|c| c.0).collect();
            if ids.is_empty() {
                return Ok(selection_json(state));
            }
            execute(state, C::SelectNodes { ids, op: SelectOp::Replace })
        }
        C::SelectChildren => {
            let ids: Vec<u64> = state.selection.iter().flat_map(|id| state.scene.descendants(*id)).map(|c| c.0).collect();
            if ids.is_empty() {
                return Ok(selection_json(state));
            }
            execute(state, C::SelectNodes { ids, op: SelectOp::Replace })
        }
        C::SelectSiblingNext | C::SelectSiblingPrevious => {
            let next = matches!(cmd, C::SelectSiblingNext);
            let Some(&current) = state.selection.iter().next() else { return Ok(selection_json(state)) };
            let sibs = siblings(&state.scene, current);
            let eligible = |id: &NodeId| *id == current || state.scene.node(*id).is_some_and(|n| state.is_selectable(n));
            let sibs: Vec<NodeId> = sibs.into_iter().filter(eligible).collect();
            let Some(i) = sibs.iter().position(|s| *s == current) else { return Ok(selection_json(state)) };
            if sibs.len() < 2 {
                return Ok(selection_json(state));
            }
            let j = if next { (i + 1) % sibs.len() } else { (i + sibs.len() - 1) % sibs.len() };
            execute(state, C::SelectNodes { ids: vec![sibs[j].0], op: SelectOp::Replace })
        }
        C::LinkInheritanceSelected { move_axes, rotate_axes, scale_axes } => {
            let ids = selected_ids(state);
            if ids.is_empty() {
                return Err("select the objects first".into());
            }
            props_change(state, "Link Inheritance", ids, |p, _| {
                if let Some(m) = move_axes { p.inherit.move_axes = m; }
                if let Some(r) = rotate_axes { p.inherit.rotate_axes = r; }
                if let Some(s) = scale_axes { p.inherit.scale_axes = s; }
            })?;
            Ok(json!({}))
        }
        C::FreezeTransform | C::FreezePosition | C::FreezeRotation => {
            let (pos, rot) = (!matches!(cmd, C::FreezeRotation), !matches!(cmd, C::FreezePosition));
            freeze_transform(state, pos, rot, cmd.name())
        }
        C::TransformToZero | C::PositionToZero | C::RotationToZero => {
            let (pos, rot) = (!matches!(cmd, C::RotationToZero), !matches!(cmd, C::PositionToZero));
            transform_to_zero(state, pos, rot, cmd.name())
        }
        C::SelectInstances => {
            // The instance group: every node sharing the selected node's source.
            let ids: Vec<NodeId> = state.selection.iter().copied().collect();
            if ids.len() != 1 {
                return Err("Select Instances needs exactly one selected object".into());
            }
            let root = |mut n: NodeId| {
                while let Some(src) = state.scene.node(n).and_then(|x| x.instance_of) {
                    n = src;
                }
                n
            };
            let group = root(ids[0]);
            let ids: Vec<u64> = state.scene.nodes().filter(|n| root(n.id) == group).map(|n| n.id.0).collect();
            if ids.len() < 2 {
                return Err("the selected object has no instances".into());
            }
            execute(state, C::SelectNodes { ids, op: SelectOp::Replace })
        }
        C::SelectByColorPick => {
            state.pick_mode = Some(PickMode::SelectByColor);
            Ok(json!({}))
        }
        C::SelectByLayer { layers } => {
            let Some(layers) = layers else {
                state.dialog = Some(Dlg::SelectByLayer);
                return Ok(json!({ "dialog": "SelectByLayer" }));
            };
            for l in &layers {
                state.scene.layer(*l).ok_or_else(|| format!("no layer {l}"))?;
            }
            let ids: Vec<u64> = state.scene.nodes().filter(|n| layers.contains(&n.layer) && state.is_selectable(n)).map(|n| n.id.0).collect();
            execute(state, C::SelectNodes { ids, op: SelectOp::Replace })
        }
        C::SelectSimilar => {
            // Similar: the same object type on the same layer with the same (or no) material.
            let kinds: BTreeSet<(&'static str, u32, Option<loom_scene::material::MaterialId>)> = state.selection.iter().filter_map(|id| state.scene.node(*id)).map(|n| (n.object.base_name(), n.layer, n.material)).collect();
            let ids: Vec<u64> = state.scene.nodes().filter(|n| kinds.contains(&(n.object.base_name(), n.layer, n.material))).map(|n| n.id.0).collect();
            execute(state, C::SelectNodes { ids, op: SelectOp::Replace })
        }
        C::Group { .. } | C::Ungroup | C::GroupOpen | C::GroupClose | C::GroupAttach | C::GroupDetach | C::GroupExplode | C::LayerManager
        | C::CreateLayer { .. } | C::DeleteLayer { .. } | C::RenameLayer { .. } | C::SetCurrentLayer { .. } | C::SetLayerHidden { .. } | C::SetLayerFrozen { .. }
        | C::MoveToLayer { .. } | C::AddSelectionToCurrentLayer | C::SelectLayerObjects { .. } | C::SetCurrentLayerToSelection | C::SetLayerColor { .. }
        | C::SetLayerRenderable { .. } | C::SetAllLayersHidden { .. } | C::SetAllLayersFrozen { .. } | C::SetColorByLayer { .. }
        | C::SetDisplayByLayer { .. } | C::SetRenderByLayer { .. } | C::LayerProperties { .. } | C::SetLayerDisplayProperty { .. } | C::SetLayerRenderProperty { .. } => crate::groups::execute(state, cmd),
        C::NewSceneKeep { .. } | C::SaveSelected { .. } | C::ArchiveFile { .. } | C::SummaryInfo | C::FileProperties | C::SetFileProperty { .. } | C::SetCustomFileProperty { .. } => crate::scene_file::execute(state, cmd),
        C::ShowLayersToolbarToggle => toggle(&mut state.show_layers_toolbar, "show_layers_toolbar"),

        C::SelectObject => set_tool(state, Tool::Select),
        C::SelectAndMove => set_tool(state, Tool::Move),
        C::SelectAndRotate => set_tool(state, Tool::Rotate),
        C::SelectAndScale => set_tool(state, Tool::Scale),
        C::ScaleCycle => {
            if state.active_tool == Tool::Scale {
                state.scale_kind = state.scale_kind.next();
            }
            set_tool(state, Tool::Scale)?;
            Ok(json!({ "tool": state.scale_kind.label() }))
        }
        C::ScaleType { kind } => {
            state.scale_kind = kind;
            set_tool(state, Tool::Scale)?;
            Ok(json!({ "tool": kind.label() }))
        }
        C::SubObjectLevelCycle if state.spline_selected() => {
            state.selection_mode = match state.selection_mode {
                SelectionMode::Object => SelectionMode::Vertex,
                SelectionMode::Vertex => SelectionMode::Edge,
                SelectionMode::Edge | SelectionMode::Border => SelectionMode::Face,
                _ => SelectionMode::Object,
            };
            state.sub_selection.clear();
            Ok(json!({ "mode": state.level_label(state.selection_mode) }))
        }
        C::SubObjectLevelCycle => {
            state.selection_mode = match state.selection_mode {
                SelectionMode::Object | SelectionMode::Element => SelectionMode::Vertex,
                SelectionMode::Vertex => SelectionMode::Edge,
                SelectionMode::Edge => SelectionMode::Border,
                SelectionMode::Border => SelectionMode::Face,
                SelectionMode::Face => SelectionMode::Element,
            };
            state.sub_selection.clear();
            Ok(json!({ "mode": state.selection_mode.label() }))
        }
        C::SubObjectSelectionToggle => {
            state.selection_mode = match state.selection_mode {
                SelectionMode::Object => SelectionMode::Vertex,
                _ => SelectionMode::Object,
            };
            state.sub_selection.clear();
            Ok(json!({ "mode": state.selection_mode.label() }))
        }
        C::SelectionLockToggle => {
            state.selection_locked = !state.selection_locked;
            Ok(json!({ "locked": state.selection_locked }))
        }
        C::SelectionMode { mode } => {
            state.selection_mode = mode;
            state.sub_selection.clear();
            state.mesh_patch.polygon_level = false;
            state.mesh_patch.handle_level = false;
            // The level lives in one place for the user: an active Edit Poly /
            // Poly Select / Mesh Select entry shows the same level in the stack.
            let ids: Vec<NodeId> = state.selection.iter().copied().collect();
            for id in ids {
                let Some(n) = state.scene.node_mut(id) else { continue };
                let Some(i) = n.active_entry else { continue };
                let Some(slot) = n.modifiers.get_mut(i) else { continue };
                let level = match (&slot.modifier, mode) {
                    (_, SelectionMode::Object) => 0,
                    (loom_scene::Modifier::EditPoly { .. } | loom_scene::Modifier::PolySelect { .. }, m) => match m {
                        SelectionMode::Vertex => 1,
                        SelectionMode::Edge => 2,
                        SelectionMode::Border => 3,
                        SelectionMode::Face => 4,
                        _ => 5,
                    },
                    // Edit Patch: Vertex, Handle, Edge, Patch, Element.
                    (loom_scene::Modifier::EditPatch { .. }, m) => match m {
                        SelectionMode::Vertex => 1,
                        SelectionMode::Edge | SelectionMode::Border => 3,
                        SelectionMode::Face => 4,
                        _ => 5,
                    },
                    // Mesh Select: Vertex, Edge, Face, Polygon, Element; polygons keep Face or Polygon.
                    (loom_scene::Modifier::MeshSelect { .. } | loom_scene::Modifier::EditMesh { .. }, m) => match m {
                        SelectionMode::Vertex => 1,
                        SelectionMode::Edge | SelectionMode::Border => 2,
                        SelectionMode::Face if slot.sub_level == 3 => 3,
                        SelectionMode::Face => 4,
                        _ => 5,
                    },
                    _ => continue,
                };
                slot.sub_level = level;
            }
            Ok(json!({ "mode": mode.label() }))
        }

        C::TopView => set_view(state, ViewKind::Top),
        C::BottomView => set_view(state, ViewKind::Bottom),
        C::FrontView => set_view(state, ViewKind::Front),
        C::BackView => set_view(state, ViewKind::Back),
        C::LeftView => set_view(state, ViewKind::Left),
        C::RightView => set_view(state, ViewKind::Right),
        C::PerspectiveUserView => set_view(state, ViewKind::Perspective),
        C::OrthographicUserView => set_view(state, ViewKind::Orthographic),
        C::ZoomExtents => {
            let bounds = state.scene_bounds();
            frame(state, bounds)
        }
        C::ZoomExtentsAll => {
            let bounds = state.scene_bounds();
            frame_all(state, bounds)
        }
        C::ZoomExtentsSelected => {
            // At the Skin's Envelope level: the selected vertices, else the current envelope.
            let bounds = crate::skin::focus_bounds(state).or_else(|| state.selection_bounds()).or_else(|| state.scene_bounds());
            frame(state, bounds)
        }
        C::ZoomViewportIn => zoom(state, 0.8),
        C::ZoomViewportOut => zoom(state, 1.25),
        C::ZoomIn2X => zoom(state, 0.5),
        C::ZoomOut2X => zoom(state, 2.0),
        C::HideGridsToggle => toggle(&mut state.active_mut().show_grid, "grid"),
        C::ShowStatisticsToggle => toggle(&mut state.show_statistics, "statistics"),
        C::ViewEdgedFacesToggle => toggle(&mut state.active_mut().edged_faces, "edged_faces"),
        C::WireframeSmoothHighlightsToggle => {
            let vp = state.active_mut();
            vp.shading = if vp.shading == Shading::Wireframe { Shading::SmoothHighlights } else { Shading::Wireframe };
            Ok(json!({ "shading": vp.shading.label() }))
        }
        C::MaximizeViewportToggle => toggle(&mut state.maximized, "maximized"),
        C::DisableViewport => toggle(&mut state.active_mut().disabled, "disabled"),
        C::RedrawAllViews => {
            state.mesh_dirty = true;
            Ok(json!({}))
        }
        C::HomeView => {
            // The ViewCube Home view: a Perspective view, the default one or Set Current View as Home.
            state.record_view();
            let home = state.home_view;
            let vp = state.active_mut();
            vp.kind = ViewKind::Perspective;
            match home {
                Some(h) => {
                    vp.camera = h.camera();
                    vp.fov_y_radians = h.fov_y_radians;
                }
                None => vp.camera = Viewport::new(ViewKind::Perspective).camera,
            }
            Ok(camera_json(state))
        }
        C::SetCurrentViewAsHome => {
            if matches!(state.active().kind, ViewKind::Camera(_)) {
                return Err("the Home view is set from a non-camera view".into());
            }
            state.home_view = Some(crate::state::SavedView::of(state.active()));
            Ok(json!({}))
        }
        C::ViewCubeToggle => toggle(&mut state.viewcube.show, "viewcube"),
        C::SetViewCubeOptions { show, all_views, size, inactive_opacity, compass } => {
            let v = &mut state.viewcube;
            if let Some(b) = show { v.show = b; }
            if let Some(b) = all_views { v.all_views = b; }
            if let Some(n) = size { v.size = n.min(3) as u8; }
            if let Some(o) = inactive_opacity { v.inactive_opacity = (o / 100.0).clamp(0.0, 1.0); }
            if let Some(b) = compass { v.compass = b; }
            Ok(json!({ "show": v.show, "all_views": v.all_views, "size": v.size, "inactive_opacity": v.inactive_opacity * 100.0, "compass": v.compass }))
        }
        C::SaveActiveView => {
            let vp = state.active();
            if matches!(vp.kind, ViewKind::Camera(_)) {
                return Err("camera views are not saved".into());
            }
            let mut saved = crate::state::SavedView::of(vp);
            saved.shading = Some(vp.shading);
            let label = vp.kind.label().to_owned();
            state.saved_views.insert(label.clone(), saved);
            Ok(json!({ "saved": label }))
        }
        C::RestoreActiveView => {
            let label = state.active().kind.label();
            let Some(saved) = state.saved_views.get(label).copied() else {
                return Err(format!("no saved {label} view"));
            };
            state.record_view();
            let vp = state.active_mut();
            vp.camera = saved.camera();
            vp.fov_y_radians = saved.fov_y_radians;
            if let Some(sh) = saved.shading {
                vp.shading = sh;
            }
            Ok(camera_json(state))
        }
        C::UpdateDuringSpinnerDragToggle => toggle(&mut state.update_during_spinner_drag, "update_during_spinner_drag"),
        C::ShadeSelectedToggle => {
            state.mesh_dirty = true;
            toggle(&mut state.shade_selected, "shade_selected")
        }
        C::DollyCameraMode => nav_toggle(state, NavMode::DollyCamera),
        C::DollyTargetMode => nav_toggle(state, NavMode::DollyTarget),
        C::DollyCameraTargetMode => nav_toggle(state, NavMode::DollyBoth),
        C::PerspectiveMode => nav_toggle(state, NavMode::Perspective),
        C::RollCameraMode => nav_toggle(state, NavMode::RollCamera),
        C::TruckCameraMode => nav_toggle(state, NavMode::TruckCamera),
        C::OrbitCameraMode => nav_toggle(state, NavMode::OrbitCamera),
        C::PanCameraMode => nav_toggle(state, NavMode::PanCamera),
        C::LightHotspotMode => nav_toggle(state, NavMode::LightHotspot),
        C::LightFalloffMode => nav_toggle(state, NavMode::LightFalloff),
        C::TimeConfiguration => {
            state.dialog = Some(Dlg::TimeConfiguration);
            Ok(json!({}))
        }
        C::SetFrame { frame } => set_frame(state, frame),
        C::SetTimeConfiguration { start, end, fps } => {
            let (mut s0, mut s1) = state.frame_range;
            if let Some(a) = start { s0 = a; }
            if let Some(b) = end { s1 = b; }
            if s1 <= s0 {
                return Err("end time must be after start time".into());
            }
            if let Some(f) = fps {
                if !(f.is_finite() && f > 0.0) {
                    return Err("fps must be positive".into());
                }
                state.fps = f;
            }
            state.frame_range = (s0, s1);
            state.current_frame = state.current_frame.clamp(s0, s1);
            Ok(json!({ "start": s0, "end": s1, "fps": state.fps, "frame": state.current_frame }))
        }
        C::SetFov { degrees } => {
            state.record_view_as("Field-of-View");
            state.active_mut().fov_y_radians = degrees.clamp(1.0, 175.0).to_radians();
            Ok(json!({ "fov": state.active().fov_y_radians.to_degrees() }))
        }
        C::AbsoluteOffsetToggle => toggle(&mut state.offset_mode, "offset_mode"),
        C::OrbitSelectedMode => nav_toggle(state, NavMode::OrbitSelected),
        C::OrbitSubObjectMode => nav_toggle(state, NavMode::OrbitSubObject),
        C::ZoomAll => nav_toggle(state, NavMode::ZoomAll),
        C::ZoomExtentsAllSelected => {
            let bounds = state.selection_bounds().or_else(|| state.scene_bounds());
            frame_all(state, bounds)
        }
        C::ZoomMode => nav_toggle(state, NavMode::Zoom),
        C::PanViewport { point: Some(p) } => {
            // The view slides (no turn, no zoom) until `p` is at its centre.
            state.record_view_as("Pan");
            let cam = &mut state.active_mut().camera;
            let to_eye = cam.basis().2;
            let p = Vec3::from(p);
            cam.target = p - to_eye * to_eye.dot(p - cam.target);
            Ok(camera_json(state))
        }
        C::PanView | C::PanViewport { point: None } => nav_toggle(state, NavMode::Pan),
        C::OrbitViewMode => nav_toggle(state, NavMode::Orbit),
        C::ZoomRegionMode => nav_toggle(state, NavMode::ZoomRegion),
        C::FieldOfViewMode => nav_toggle(state, NavMode::Fov),
        C::ViewportSafeframesToggle => toggle(&mut state.active_mut().safe_frames, "safe_frames"),
        C::ViewportClipping => toggle(&mut state.active_mut().clipping, "clipping"),
        C::SetViewportClipping { index, near, far } => {
            let i = index.map(|i| i as usize).unwrap_or(state.active_viewport);
            let vp = state.viewports.get_mut(i).ok_or_else(|| format!("no viewport {i}"))?;
            let near = near.max(0.0);
            vp.clip = (near, far.max(near + 1e-3));
            vp.clipping = true;
            Ok(json!({ "clip": [vp.clip.0, vp.clip.1] }))
        }
        C::ShowMainToolbarToggle => toggle(&mut state.show_main_toolbar, "main_toolbar"),
        C::LockUserInterfaceToggle => toggle(&mut state.ui_locked, "ui_locked"),
        C::ExpertModeToggle => toggle(&mut state.expert_mode, "expert_mode"),
        C::DisplayAsSeeThroughToggle => {
            let ids = selected_ids(state);
            if ids.is_empty() {
                return Ok(json!({}));
            }
            let on = !ids.iter().all(|id| state.scene.node(*id).is_some_and(|n| n.display.see_through));
            props_change(state, "See-Through", ids, |p, _| p.display.see_through = on)?;
            Ok(json!({ "see_through": on }))
        }
        C::HideCamerasToggle => {
            let r = toggle(&mut state.hide_category.cameras, "hide_cameras");
            prune_selection_hidden(state);
            r
        }
        C::HideGeometryToggle => {
            let r = toggle(&mut state.hide_category.geometry, "hide_geometry");
            prune_selection_hidden(state);
            r
        }
        C::DisplayBones => {
            state.display.hide_bones = false;
            state.display_bones_only = true;
            state.mesh_dirty = true;
            Ok(json!({ "display_bones_only": true }))
        }
        C::DisplayBonesAndObjects => {
            state.display.hide_bones = false;
            state.display_bones_only = false;
            state.mesh_dirty = true;
            Ok(json!({ "display_bones_only": false }))
        }
        C::HideHelpersToggle => {
            let r = toggle(&mut state.hide_category.helpers, "hide_helpers");
            prune_selection_hidden(state);
            r
        }
        C::HideLightsToggle => {
            let r = toggle(&mut state.hide_category.lights, "hide_lights");
            prune_selection_hidden(state);
            r
        }
        C::HideShapesToggle => {
            let r = toggle(&mut state.hide_category.shapes, "hide_shapes");
            prune_selection_hidden(state);
            r
        }
        C::ShowFloatingDialogs => toggle(&mut state.hide_floating_dialogs, "hide_floating_dialogs"),
        C::ViewportLightingToggle => toggle(&mut state.active_mut().scene_lights, "scene_lights"),
        C::ShadeSelectedFacesToggle => toggle(&mut state.shade_selected_faces, "shade_selected_faces"),
        C::SelectionBracketToggle => toggle(&mut state.selection_brackets, "selection_brackets"),
        C::BackgroundLockToggle => {
            let on = !state.active().background_locked;
            set_background_lock(state.active_mut(), on);
            Ok(json!({ "background_locked": on }))
        }
        C::ResetBackgroundTransform => {
            let vp = state.active_mut();
            if vp.background_locked {
                set_background_lock(vp, true);
            }
            Ok(json!({}))
        }
        C::UpdateBackgroundImage => {
            state.active_mut().background_version += 1;
            Ok(json!({}))
        }
        C::ViewportBackground => {
            state.dialog = Some(Dialog::ViewportBackground);
            Ok(json!({}))
        }
        C::SetViewportBackground { path, aspect, lock, display, all_views } => {
            let aspect = match aspect {
                Some(a) => Some(crate::viewport::BackgroundAspect::from_label(&a).ok_or_else(|| format!("unknown aspect ratio `{a}` (Match Viewport, Match Bitmap, Match Rendering Output)"))?),
                None => None,
            };
            let active = state.active_viewport;
            for (i, vp) in state.viewports.iter_mut().enumerate() {
                if i != active && all_views != Some(true) {
                    continue;
                }
                if let Some(p) = &path {
                    vp.background = (!p.is_empty()).then(|| p.clone());
                    vp.background_version += 1;
                    vp.background_display = true;
                }
                if let Some(a) = aspect {
                    vp.background_aspect = a;
                }
                if let Some(d) = display {
                    vp.background_display = d;
                }
                if let Some(l) = lock {
                    if l != vp.background_locked {
                        set_background_lock(vp, l);
                    }
                }
            }
            Ok(json!({}))
        }
        C::SelectByName => {
            // The dialog follows the active pick mode, as Select Parent / Attach To Group.
            let purpose = match state.pick_mode {
                Some(PickMode::Link) => DialogPurpose::LinkParent,
                Some(PickMode::GroupAttach) => DialogPurpose::GroupAttach,
                _ => DialogPurpose::Select,
            };
            open_by_name(state, purpose)
        }
        C::ConfigureViewports { panel } => {
            state.dialog = Some(Dialog::ViewportConfiguration);
            state.config_panel = panel;
            Ok(json!({}))
        }
        C::CloseDialog => {
            state.dialog = None;
            Ok(json!({}))
        }
        C::IsolateSelection => {
            if state.isolation.is_some() {
                return exit_isolation(state);
            }
            let keep: BTreeSet<NodeId> = state.selection.iter().copied().collect();
            if keep.is_empty() {
                return Ok(json!({}));
            }
            let ids: Vec<NodeId> = state
                .scene
                .nodes()
                .filter(|n| !n.hidden && !keep.contains(&n.id))
                .map(|n| n.id)
                .collect();
            set_flags(state, "Isolate Selection", ids.clone(), |_, f| (true, f))?;
            state.isolation = Some(ids);
            Ok(json!({ "isolated": keep.len() }))
        }
        C::ExitIsolation => exit_isolation(state),
        C::CameraView => {
            let cam = state
                .selection
                .iter()
                .copied()
                .find(|id| matches!(state.scene.node(*id).map(|n| &n.object), Some(ObjectKind::Camera { .. })))
                .or_else(|| {
                    let cams: Vec<NodeId> = state
                        .scene
                        .nodes()
                        .filter(|n| matches!(n.object, ObjectKind::Camera { .. }))
                        .map(|n| n.id)
                        .collect();
                    (cams.len() == 1).then(|| cams[0])
                });
            match cam {
                Some(id) => set_view(state, ViewKind::Camera(id)),
                None if state.scene.nodes().filter(|n| matches!(n.object, ObjectKind::Camera { .. })).count() > 1 => {
                    // Several cameras and none selected: the Select Camera list.
                    state.dialog = Some(Dialog::SelectCamera);
                    Ok(json!({ "camera": Value::Null, "dialog": "Select Camera" }))
                }
                None => Ok(json!({ "camera": Value::Null })),
            }
        }
        C::SpotDirectionalLightView => {
            let aims = |id: &NodeId| matches!(state.scene.node(*id).map(|n| &n.object), Some(ObjectKind::Light(l)) if l.light_type.aims());
            let light = state.selection.iter().copied().find(|id| aims(id)).or_else(|| {
                let lights: Vec<NodeId> = state.scene.nodes().map(|n| n.id).filter(|id| aims(id)).collect();
                (lights.len() == 1).then(|| lights[0])
            });
            match light {
                Some(id) => set_view(state, ViewKind::Camera(id)),
                None => Ok(json!({ "light": Value::Null })),
            }
        }
        C::ViewportDefaultLights { count } => {
            state.active_mut().default_lights = count.clamp(1, 2);
            Ok(json!({ "default_lights": count.clamp(1, 2) }))
        }
        C::ViewportLayout { layout } => {
            state.layout = layout;
            let kinds = default_kinds(layout);
            let mut vps: Vec<Viewport> = Vec::with_capacity(kinds.len());
            for (i, kind) in kinds.into_iter().enumerate() {
                // Keep an existing viewport of the same kind if there is one.
                match state.viewports.iter().position(|v| v.kind == kind) {
                    Some(j) => vps.push(state.viewports[j].clone()),
                    None => {
                        let _ = i;
                        vps.push(Viewport::new(kind));
                    }
                }
            }
            state.viewports = vps;
            state.active_viewport = state.viewports.len() - 1;
            state.maximized = false;
            Ok(json!({ "viewports": state.viewports.len() }))
        }
        C::ActiveViewport { index } => {
            let i = index as usize;
            if i >= state.viewports.len() {
                return Err(format!("no viewport {i}"));
            }
            state.active_viewport = i;
            Ok(json!({ "active": i }))
        }
        C::ViewportView { index, kind } => {
            let i = index as usize;
            if i >= state.viewports.len() {
                return Err(format!("no viewport {i}"));
            }
            let prev = state.active_viewport;
            state.active_viewport = i;
            let r = set_view(state, kind);
            state.active_viewport = prev;
            r
        }
        C::ViewportShading { shading, edged_faces } => {
            let vp = state.active_mut();
            vp.shading = shading;
            if let Some(e) = edged_faces {
                vp.edged_faces = e;
            }
            Ok(json!({ "shading": shading.label(), "edged_faces": vp.edged_faces }))
        }
        C::SetSplits { x, y } => {
            if let Some(x) = x {
                state.splits.set_x(x);
            }
            if let Some(y) = y {
                state.splits.set_y(y);
            }
            Ok(json!({ "x": state.splits.x, "y": state.splits.y }))
        }
        C::Align => {
            if state.selection.is_empty() {
                return Err("select the object(s) to align first".into());
            }
            state.pick_mode = Some(PickMode::AlignTarget);
            Ok(json!({}))
        }
        C::QuickAlign => {
            if state.selection.is_empty() {
                return Err("select the object(s) to align first".into());
            }
            state.pick_mode = Some(PickMode::QuickAlignTarget);
            Ok(json!({}))
        }
        C::NormalAlign => {
            if state.selection.is_empty() {
                return Err("select the source object first".into());
            }
            state.normal_align_source = None;
            state.pick_mode = Some(PickMode::NormalAlignSource);
            Ok(json!({}))
        }
        C::AlignCamera => {
            let has_camera = state.selection.iter().any(|id| state.scene.node(*id).is_some_and(|n| matches!(n.object, ObjectKind::Camera { .. })));
            if !has_camera {
                return Err("select a camera first".into());
            }
            state.pick_mode = Some(PickMode::AlignCamera);
            Ok(json!({}))
        }
        C::AlignToView => {
            if state.selection.is_empty() {
                return Err("select the object(s) to align first".into());
            }
            state.dialog = Some(Dlg::AlignToView);
            Ok(json!({}))
        }
        C::AlignToViewAxis { axis, flip } => {
            let cam = &state.active().camera;
            let mut dir = (cam.eye_position() - cam.target).normalize_or_zero();
            if flip {
                dir = -dir;
            }
            let a = axis_unit(axis);
            transform_selection_each(state, "Align to View", &|st, id| match id {
                Some(i) => {
                    let w = st.scene.world_matrix(i);
                    let p = w.w_axis.truncate();
                    let cur = w.transform_vector3(a).normalize_or_zero();
                    let q = Quat::from_rotation_arc(cur, dir);
                    (Mat4::from_translation(p) * Mat4::from_quat(q) * Mat4::from_translation(-p), euler_hint(st, id, q.to_axis_angle().0 * q.to_axis_angle().1.to_degrees()))
                }
                None => (Mat4::IDENTITY, Vec3::ZERO),
            })
        }
        C::AlignObjects { target, x, y, z, current, target_pt, orientation, match_scale } => {
            let target = NodeId(target);
            state.scene.node(target).ok_or_else(|| missing(target))?;
            let ids: Vec<NodeId> = state.selection.iter().copied().filter(|i| *i != target).collect();
            if ids.is_empty() {
                return Ok(json!({}));
            }
            let vp = state.active_viewport;
            let sys = match state.coord_system {
                CoordSystem::View => CoordSystem::World,
                s => s,
            };
            let axes = crate::transform::frame_for_system(state, vp, ids.first().copied(), sys).axes;
            state.undo.begin("Align");
            let r: Result<(), String> = (|| {
                // Position: per enabled axis, move the selection's point onto the target's.
                let cur = align_point(state, &ids, current, axes);
                let tgt = align_point(state, &[target], target_pt, axes);
                let mut d = tgt - cur;
                if !x { d.x = 0.0; }
                if !y { d.y = 0.0; }
                if !z { d.z = 0.0; }
                if d.length_squared() > 0.0 {
                    let w = axes * d;
                    transform_selection_each(state, "Align", &|_, _| (Mat4::from_translation(w), Vec3::ZERO))?;
                }
                if orientation.iter().any(|o| *o) {
                    let (_, tq, _) = state.scene.world_matrix(target).to_scale_rotation_translation();
                    let t_axes = Mat3::from_quat(tq);
                    transform_selection_each(state, "Align", &|st, id| match id {
                        Some(i) => {
                            let w = st.scene.world_matrix(i);
                            let p = w.w_axis.truncate();
                            let (_, cq, _) = w.to_scale_rotation_translation();
                            let q = if orientation.iter().all(|o| *o) {
                                tq * cq.inverse()
                            } else {
                                let mut q = Quat::IDENTITY;
                                for (k, on) in orientation.iter().enumerate() {
                                    if *on {
                                        let cur = (q * cq) * axis_unit([Axis::X, Axis::Y, Axis::Z][k]);
                                        q = Quat::from_rotation_arc(cur.normalize_or_zero(), t_axes.col(k)) * q;
                                    }
                                }
                                q
                            };
                            (Mat4::from_translation(p) * Mat4::from_quat(q) * Mat4::from_translation(-p), euler_hint(st, id, q.to_axis_angle().0 * q.to_axis_angle().1.to_degrees()))
                        }
                        None => (Mat4::IDENTITY, Vec3::ZERO),
                    })?;
                }
                if match_scale.iter().any(|m| *m) {
                    let ts = state.scene.node(target).ok_or_else(|| missing(target))?.transform.scale.get();
                    for id in &ids {
                        let mut sc = state.scene.node(*id).ok_or_else(|| missing(*id))?.transform.scale.get();
                        for k in 0..3 {
                            if match_scale[k] {
                                sc[k] = ts[k];
                            }
                        }
                        execute(state, Command::SetTransform { id: id.0, position: None, rotation_deg: None, scale: Some(sc.to_array()) })?;
                    }
                }
                Ok(())
            })();
            state.undo.end();
            r?;
            Ok(json!({}))
        }
        C::NormalAlignFaces { source_point, source_normal, target_point, target_normal, offset, angle_deg, flip } => {
            let sp = Vec3::from(source_point);
            let sn = Vec3::from(source_normal).normalize_or_zero();
            let tp = Vec3::from(target_point);
            let tn = Vec3::from(target_normal).normalize_or_zero();
            let want = if flip { tn } else { -tn };
            let r = Quat::from_rotation_arc(sn, want);
            let spin = Quat::from_axis_angle(tn, angle_deg.to_radians());
            // Offset in the target normal's frame (X/Y tangent, Z along the normal).
            let helper = if tn.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
            let tx = helper.cross(tn).normalize_or_zero();
            let ty = tn.cross(tx);
            let dest = tp + tx * offset[0] + ty * offset[1] + tn * offset[2];
            let m = Mat4::from_translation(dest) * Mat4::from_quat(spin * r) * Mat4::from_translation(-sp);
            let q = spin * r;
            transform_selection_each(state, "Normal Align", &|st, id| (m, euler_hint(st, id, q.to_axis_angle().0 * q.to_axis_angle().1.to_degrees())))
        }
        C::AlignCameraTo { point, normal } => {
            let p = Vec3::from(point);
            let n = Vec3::from(normal).normalize_or_zero();
            let cams: Vec<NodeId> = state.selection.iter().copied().filter(|id| state.scene.node(*id).is_some_and(|n| matches!(n.object, ObjectKind::Camera { .. }))).collect();
            if cams.is_empty() {
                return Err("select a camera first".into());
            }
            state.undo.begin("Align Camera");
            let r: Result<(), String> = (|| {
                for id in cams {
                    let w = state.scene.world_matrix(id);
                    let eye = w.w_axis.truncate();
                    let d = eye.distance(p).max(1e-3);
                    let new_eye = p + n * d;
                    // Cameras look down local -Z: local Z = normal, up as close to world Z as possible.
                    let up = if n.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
                    let xa = up.cross(n).normalize_or_zero();
                    let ya = n.cross(xa);
                    let (sc, _, _) = w.to_scale_rotation_translation();
                    let world = Mat4::from_scale_rotation_translation(sc, Quat::from_mat3(&Mat3::from_cols(xa, ya, n)), new_eye);
                    let local = state.scene.parent_frame(id).inverse() * world;
                    let t = Transform::from_matrix(local);
                    execute(state, Command::SetTransform { id: id.0, position: Some(t.position.get().to_array()), rotation_deg: Some(t.rotation.get().to_array()), scale: None })?;
                }
                Ok(())
            })();
            state.undo.end();
            r?;
            Ok(json!({}))
        }
        C::PickSurface { id, point, normal } => {
            let id = NodeId(id);
            state.scene.node(id).ok_or_else(|| missing(id))?;
            match state.pick_mode {
                Some(PickMode::NormalAlignSource) => {
                    state.normal_align_source = Some((Vec3::from(point), Vec3::from(normal)));
                    state.pick_mode = Some(PickMode::NormalAlignTarget);
                    Ok(json!({ "stage": "target" }))
                }
                Some(PickMode::NormalAlignTarget) => {
                    let (sp, sn) = state.normal_align_source.take().ok_or("no source pick")?;
                    state.pick_mode = None;
                    let picks = NormalAlignPicks { source_point: sp, source_normal: sn, target_point: Vec3::from(point), target_normal: Vec3::from(normal) };
                    execute(state, Command::NormalAlignFaces { source_point: sp.to_array(), source_normal: sn.to_array(), target_point: point, target_normal: normal, offset: [0.0; 3], angle_deg: 0.0, flip: false })?;
                    state.normal_align = Some(picks);
                    state.dialog = Some(Dlg::NormalAlign);
                    Ok(json!({ "stage": "done" }))
                }
                Some(PickMode::AlignCamera) => {
                    state.pick_mode = None;
                    execute(state, Command::AlignCameraTo { point, normal })
                }
                Some(PickMode::WorkingPivotSurface) => {
                    // On the surface, Z along its normal (or the view with Align To View).
                    state.pick_mode = None;
                    state.working_pivot.origin = Vec3::from(point);
                    state.working_pivot.rotation = if state.working_pivot_align_to_view {
                        Quat::from_mat3(&crate::transform::view_axes(state, state.active_viewport)).normalize()
                    } else {
                        Quat::from_rotation_arc(Vec3::Z, Vec3::from(normal).normalize_or(Vec3::Z))
                    };
                    Ok(json!({ "origin": point }))
                }
                Some(PickMode::UvwMapNormal) => {
                    state.pick_mode = None;
                    execute(state, Command::UvwMapAlignTo { point, normal })
                }
                Some(PickMode::UvwMapRegionA) => {
                    state.pick_point_a = Some((id, 0, Vec3::from(point)));
                    state.pick_mode = Some(PickMode::UvwMapRegionB);
                    Ok(json!({ "stage": "b" }))
                }
                Some(PickMode::UvwMapRegionB) => {
                    let Some((_, _, a)) = state.pick_point_a.take() else { return Err("no first corner".into()) };
                    state.pick_mode = None;
                    execute(state, Command::UvwMapRegionFit { a: a.to_array(), b: point })
                }
                _ => Err("no surface pick mode active".into()),
            }
        }
        C::Mirror => {
            if state.selection.is_empty() {
                return Err("select the object(s) to mirror first".into());
            }
            state.dialog = Some(Dlg::Mirror);
            Ok(json!({}))
        }
        C::MirrorObjects { axis, offset, clone } => {
            if state.selection.is_empty() {
                return Ok(json!({}));
            }
            let vp = state.active_viewport;
            let single = state.selection.iter().next().copied().filter(|_| state.selection.len() == 1);
            let frame = frame_for(state, vp, single);
            let flips = axis.flips();
            let s = Vec3::new(if flips[0] { -1.0 } else { 1.0 }, if flips[1] { -1.0 } else { 1.0 }, if flips[2] { -1.0 } else { 1.0 });
            let basis = Mat4::from_mat3(frame.axes);
            let mut shift = Vec3::ZERO;
            for (k, f) in flips.iter().enumerate() {
                if *f {
                    shift -= frame.axes.col(k);
                }
            }
            let shift = shift.normalize_or_zero() * offset;
            let m = Mat4::from_translation(frame.origin + shift) * basis * Mat4::from_scale(s) * basis.inverse() * Mat4::from_translation(-frame.origin);
            state.undo.begin("Mirror");
            let r: Result<(), String> = (|| {
                if let Some(kind) = clone {
                    execute(state, Command::CloneOptions { kind, copies: 1, name: None, offset: None, rotate_deg: None, scale: None })?;
                }
                transform_selection_each(state, "Mirror", &|_, _| (m, Vec3::ZERO))?;
                Ok(())
            })();
            state.undo.end();
            r?;
            Ok(selection_json(state))
        }
        C::Array => {
            if state.selection.is_empty() {
                return Err("select the object(s) to array first".into());
            }
            state.dialog = Some(Dlg::Array);
            Ok(json!({}))
        }
        C::ArrayObjects { mv, rotate, scale, totals, re_orient, uniform, kind, count, row_2d, row_3d } => {
            if count.iter().any(|c| *c == 0) {
                return Err("Array counts must be at least 1".into());
            }
            if scale.iter().any(|v| *v <= 0.0) {
                return Err("Array scale must be positive (percent)".into());
            }
            let ids: Vec<NodeId> = state.selection.iter().copied().collect();
            if ids.is_empty() {
                return Err("select the object(s) to array first".into());
            }
            let total = count[0] as usize * count[1] as usize * count[2] as usize;
            if total == 1 {
                return Ok(json!({ "created": 0 }));
            }
            let vp = state.active_viewport;
            let single = ids.first().copied().filter(|_| ids.len() == 1);
            let frame = frame_for(state, vp, single);
            // Totals divide by the count (Totals Move 120 with Count 3 spaces members 40 apart).
            let n1 = count[0].max(1) as f32;
            let inc_move = if totals[0] { Vec3::from(mv) / n1 } else { Vec3::from(mv) };
            let inc_rot = if totals[1] { Vec3::from(rotate) / n1 } else { Vec3::from(rotate) };
            let mut sc = Vec3::from(scale) / 100.0;
            if uniform {
                sc = Vec3::splat(sc.x);
            }
            let inc_scale = if totals[2] { Vec3::new(sc.x.powf(1.0 / n1), sc.y.powf(1.0 / n1), sc.z.powf(1.0 / n1)) } else { sc };
            let axes = frame.axes;
            let basis = Mat4::from_mat3(axes);
            let c = frame.origin;
            let step_rot = Quat::from_euler(glam::EulerRot::XYZ, inc_rot.x.to_radians(), inc_rot.y.to_radians(), inc_rot.z.to_radians());
            let step_rot_world = Quat::from_mat3(&(axes * Mat3::from_quat(step_rot) * axes.transpose()));
            state.undo.begin("Array");
            let mut created = Vec::new();
            let mut last = Vec::new();
            let r: Result<(), String> = (|| {
                for k in 0..count[2] {
                    for j in 0..count[1] {
                        for i in 0..count[0] {
                            if i == 0 && j == 0 && k == 0 {
                                continue;
                            }
                            execute(state, Command::SelectNodes { ids: ids.iter().map(|n| n.0).collect(), op: SelectOp::Replace })?;
                            execute(state, Command::CloneOptions { kind, copies: 1, name: None, offset: None, rotate_deg: None, scale: None })?;
                            let row = axes * (Vec3::from(row_2d) * j as f32 + Vec3::from(row_3d) * k as f32);
                            let fi = i as f32;
                            let scale_i = Vec3::new(inc_scale.x.powf(fi), inc_scale.y.powf(fi), inc_scale.z.powf(fi));
                            let rot_i = Quat::from_axis_angle(step_rot_world.to_axis_angle().0, step_rot_world.to_axis_angle().1 * fi);
                            let step = Mat4::from_translation(c + axes * (inc_move * fi)) * Mat4::from_quat(rot_i) * basis * Mat4::from_scale(scale_i) * basis.inverse() * Mat4::from_translation(-c);
                            let full = Mat4::from_translation(row) * step;
                            let rot_vec = axes * (inc_rot * fi);
                            transform_selection_each(state, "Array", &|st, id| match (re_orient, id) {
                                (false, Some(n)) => {
                                    // Keep the orientation: scale about the pivot, then move the pivot
                                    // where the full step would take it.
                                    let p = st.scene.world_matrix(n).w_axis.truncate();
                                    let scale_here = Mat4::from_translation(p) * basis * Mat4::from_scale(scale_i) * basis.inverse() * Mat4::from_translation(-p);
                                    (Mat4::from_translation(full.transform_point3(p) - p) * scale_here, Vec3::ZERO)
                                }
                                _ => (full, euler_hint(st, id, rot_vec)),
                            })?;
                            created.extend(state.selection.iter().copied());
                            last = state.selection.iter().copied().collect::<Vec<_>>();
                        }
                    }
                }
                Ok(())
            })();
            state.undo.end();
            r?;
            // The selection moves to the last copy set: the same array again continues it.
            state.selection = last.into_iter().collect();
            state.refresh_center_mode();
            Ok(json!({ "created": created.len() }))
        }
        C::SpacingTool => {
            if state.selection.is_empty() {
                return Err("select the object(s) to space first".into());
            }
            state.dialog = Some(Dlg::SpacingTool);
            Ok(json!({}))
        }
        C::SpacingPickPath => {
            state.pick_mode = if state.pick_mode == Some(PickMode::SpacingPath) { None } else { Some(PickMode::SpacingPath) };
            Ok(json!({}))
        }
        C::SpaceAlongPath { path, start, end, distribution, count, spacing, start_offset, end_offset, context, follow, kind } => {
            space_along_path(state, path.map(NodeId), Vec3::from(start), Vec3::from(end), distribution, count, spacing, start_offset, end_offset, context, follow, kind)
        }
        C::SpacingPickPoints => {
            state.spacing_path = None;
            state.spacing_points = [None, None];
            state.pick_mode = Some(PickMode::SpacingPointA);
            Ok(json!({}))
        }
        C::PickPoint { point } => match state.pick_mode {
            Some(PickMode::SpacingPointA) => {
                state.spacing_points[0] = Some(Vec3::from(point));
                state.pick_mode = Some(PickMode::SpacingPointB);
                Ok(json!({ "stage": "B" }))
            }
            Some(PickMode::SpacingPointB) => {
                state.spacing_points[1] = Some(Vec3::from(point));
                state.pick_mode = None;
                Ok(json!({ "stage": "done" }))
            }
            Some(PickMode::SplineTrim) => crate::spline::execute(state, C::SplineTrim { point }),
            Some(PickMode::SplineExtend) => crate::spline::execute(state, C::SplineExtend { point }),
            Some(PickMode::SplineCrossInsert) => {
                let t = state.spline_settings.cross_threshold;
                crate::spline::execute(state, C::SplineCrossInsert { threshold: t, point: Some(point) })
            }
            Some(PickMode::SplineBind) => crate::spline::bind_at(state, Vec3::from(point)),
            Some(PickMode::TrimExtendModifier) => crate::spline::trim_extend_click(state, point),
            Some(PickMode::MeasureDistanceA | PickMode::MeasureDistanceB) => crate::helpers::measure_pick(state, Vec3::from(point)),
            Some(PickMode::BipedFootstep) => crate::biped_footsteps::execute(state, C::BipedAppendFootstep { position: Some(point), left: None }),
            Some(PickMode::WorkingPivotView) => {
                state.pick_mode = None;
                state.working_pivot.origin = Vec3::from(point);
                if state.working_pivot_align_to_view {
                    state.working_pivot.rotation = Quat::from_mat3(&crate::transform::view_axes(state, state.active_viewport)).normalize();
                }
                Ok(json!({ "origin": point }))
            }
            _ => Err("no point pick mode active".into()),
        },
        C::SpaceObjects { start, end, count, spacing, start_offset, end_offset, context, follow, kind } => {
            if count == 0 {
                return Err("count must be at least 1".into());
            }
            let ids: Vec<NodeId> = state.selection.iter().copied().collect();
            if ids.is_empty() {
                return Err("select the object(s) to space first".into());
            }
            let (a, b) = (Vec3::from(start), Vec3::from(end));
            let dir = (b - a).normalize_or_zero();
            let a = a + dir * start_offset;
            let b = b - dir * end_offset;
            let span = (b - a).length();
            // Extent of the selection along the path for the Edges context.
            let extent = if context == SpacingContext::Edges {
                let mut lo = f32::INFINITY;
                let mut hi = f32::NEG_INFINITY;
                for id in &ids {
                    if let Some((mn, mx)) = state.object_bounds(*id) {
                        for i in 0..8 {
                            let p = Vec3::new(if i & 1 == 0 { mn.x } else { mx.x }, if i & 2 == 0 { mn.y } else { mx.y }, if i & 4 == 0 { mn.z } else { mx.z });
                            let d = p.dot(dir);
                            lo = lo.min(d);
                            hi = hi.max(d);
                        }
                    }
                }
                if lo.is_finite() { hi - lo } else { 0.0 }
            } else {
                0.0
            };
            let step = match spacing {
                Some(sp) => sp,
                None if count > 1 => (span - extent) / (count - 1) as f32,
                None => 0.0,
            };
            let first = a + dir * (extent * 0.5);
            let pivot0 = if ids.len() == 1 { state.scene.world_matrix(ids[0]).w_axis.truncate() } else { state.selection_pivot() };
            let follow_rot = if follow {
                let (_, q, _) = state.scene.world_matrix(ids[0]).to_scale_rotation_translation();
                Quat::from_rotation_arc(q * Vec3::X, dir) 
            } else {
                Quat::IDENTITY
            };
            state.undo.begin("Spacing Tool");
            let mut created = Vec::new();
            let r: Result<(), String> = (|| {
                for i in 0..count {
                    execute(state, Command::SelectNodes { ids: ids.iter().map(|n| n.0).collect(), op: SelectOp::Replace })?;
                    execute(state, Command::CloneOptions { kind, copies: 1, name: None, offset: None, rotate_deg: None, scale: None })?;
                    let target = first + dir * (step * i as f32);
                    let m = Mat4::from_translation(target) * Mat4::from_quat(follow_rot) * Mat4::from_translation(-pivot0);
                    let hint = follow_rot.to_axis_angle();
                    transform_selection_each(state, "Spacing Tool", &|st, id| (m, euler_hint(st, id, hint.0 * hint.1.to_degrees())))?;
                    created.extend(state.selection.iter().copied());
                }
                Ok(())
            })();
            state.undo.end();
            r?;
            state.selection = created.iter().copied().collect();
            state.refresh_center_mode();
            Ok(json!({ "created": created.len() }))
        }
        C::ObjectProperties => {
            if state.selection.is_empty() {
                return Err("select an object first".into());
            }
            state.dialog = Some(Dlg::ObjectProperties);
            Ok(json!({}))
        }
        C::Preferences => {
            state.dialog = Some(Dlg::Preferences);
            Ok(json!({}))
        }
        C::PaintSelectionSizeUp | C::PaintSelectionSizeDown => {
            let step = if matches!(cmd, C::PaintSelectionSizeUp) { 1.0 } else { -1.0 };
            state.prefs.paint_brush_size = (state.prefs.paint_brush_size + step).clamp(1.0, 500.0);
            Ok(json!({ "size": state.prefs.paint_brush_size }))
        }
        C::SetPreference { name, value } => {
            state.prefs.set(&name, value)?;
            if name.starts_with("Viewport Texture "){state.mesh_dirty=true;}
            state.undo.set_limit(state.prefs.undo_levels);
            if name == "Reference Coordinate System Constant" && state.prefs.ref_coord_constant {
                state.coord_by_tool = [state.coord_system; 4];
            }
            Ok(json!({ "name": name, "value": state.prefs.get(&name) }))
        }
        C::UnitsSetup => {
            state.dialog = Some(Dlg::UnitsSetup);
            Ok(json!({}))
        }
        C::SetUnits { system_unit, system_scale, display, custom_name, custom_value, custom_unit } => {
            use crate::state::{DisplayUnits, SystemUnit};
            if let Some(u) = system_unit {
                state.units.system_unit = SystemUnit::parse(&u).ok_or_else(|| format!("unknown system unit {u:?}"))?;
            }
            if let Some(sc) = system_scale {
                state.units.system_scale = sc.max(1e-6);
            }
            if let Some(d) = display {
                state.units.display = match d.as_str() {
                    "Generic" => DisplayUnits::Generic,
                    "Millimeters" => DisplayUnits::Millimeters,
                    "Centimeters" => DisplayUnits::Centimeters,
                    "Meters" => DisplayUnits::Meters,
                    "Kilometers" => DisplayUnits::Kilometers,
                    "Decimal Inches" => DisplayUnits::DecimalInches,
                    "Decimal Feet" => DisplayUnits::DecimalFeet,
                    "Feet w/ Decimal Inches" => DisplayUnits::FeetDecimalInches,
                    "Custom" => {
                        let (name, value, unit) = match &state.units.display {
                            DisplayUnits::Custom { name, value, unit } => (name.clone(), *value, *unit),
                            _ => ("FL".to_owned(), 1.0, SystemUnit::Inches),
                        };
                        let unit = match custom_unit.as_deref() {
                            Some(u) => SystemUnit::parse(u).ok_or_else(|| format!("unknown custom unit {u:?}"))?,
                            None => unit,
                        };
                        DisplayUnits::Custom { name: custom_name.clone().unwrap_or(name), value: custom_value.unwrap_or(value).max(1e-6), unit }
                    }
                    other => return Err(format!("unknown display unit {other:?}")),
                };
            } else if let DisplayUnits::Custom { name, value, unit } = &mut state.units.display {
                if let Some(n) = custom_name { *name = n; }
                if let Some(v) = custom_value { *value = v.max(1e-6); }
                if let Some(u) = custom_unit { *unit = SystemUnit::parse(&u).ok_or_else(|| format!("unknown custom unit {u:?}"))?; }
            }
            state.default_units = state.units.clone();
            Ok(json!({ "display": state.units.display.label(), "example": state.format_units(1.0) }))
        }
        C::CustomizeUserInterface => {
            state.dialog = Some(Dlg::CustomizeUI);
            Ok(json!({}))
        }
        C::AssignShortcut { group, action, chord } => {
            let c = crate::keymap::parse_chord(&chord)?;
            let previous = state.custom_keys.assign(&group, &action, c)?;
            Ok(json!({ "group": group, "action": action, "chord": c.chart_text(), "previous": previous }))
        }
        C::RemoveShortcut { group, action } => {
            let removed = state.custom_keys.remove(&group, &action)?;
            Ok(json!({ "removed": removed }))
        }
        C::ResetShortcuts => {
            state.custom_keys.reset();
            Ok(json!({}))
        }
        C::SaveShortcuts { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::SaveShortcuts);
                return Ok(json!({ "dialog": "SaveShortcuts" }));
            };
            write_text(&path, &crate::keymap::shortcuts_to_file_text(&state.custom_keys))?;
            back_to_keyboard_panel(state);
            Ok(json!({ "path": path }))
        }
        C::LoadShortcuts { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::LoadShortcuts);
                return Ok(json!({ "dialog": "LoadShortcuts" }));
            };
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            let rows = crate::keymap::shortcuts_from_file_text(&text)?;
            state.custom_keys.set_rows(&rows)?;
            back_to_keyboard_panel(state);
            Ok(json!({ "path": path, "groups": rows.len() }))
        }
        C::SaveCustomUiScheme { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::SaveCustomUiScheme);
                return Ok(json!({ "dialog": "SaveCustomUiScheme" }));
            };
            write_text(&path, &crate::settings::scheme_to_text(state))?;
            Ok(json!({ "path": path }))
        }
        C::LoadCustomUiScheme { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dlg::LoadCustomUiScheme);
                return Ok(json!({ "dialog": "LoadCustomUiScheme" }));
            };
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            crate::settings::apply_scheme_text(state, &text)?;
            Ok(json!({ "path": path }))
        }
        C::RevertToStartupUi => {
            let ui = state.startup_ui;
            ui.apply(state);
            Ok(json!({}))
        }
        C::ShowCommandPanelToggle => toggle(&mut state.show_command_panel, "show_command_panel"),
        C::SelectAndManipulate => toggle(&mut state.manipulate, "manipulate"),
        C::Manipulate { id, name, delta } => {
            let nid = NodeId(id);
            let node = state.scene.node(nid).ok_or_else(|| missing(nid))?;
            let current = node.object.parameters().into_iter().find(|(n, _)| *n == name).ok_or_else(|| format!("no parameter {name:?}"))?.1;
            let loom_scene::ParamValue::Float(f) = current else {
                return Err(format!("{name} is not a float parameter"));
            };
            execute(state, Command::SetParameter { id, name, value: (f as f64 + delta).max(0.0) })
        }
        C::CycleSelectionMethod => {
            use crate::state::SelectionRegion as R;
            let all = R::ALL;
            let i = all.iter().position(|r| *r == state.selection_region).unwrap_or(0);
            state.selection_region = all[(i + 1) % all.len()];
            state.region_points_reset = true;
            Ok(json!({ "region": format!("{:?}", state.selection_region) }))
        }
        C::VertexLevel | C::EdgeLevel | C::BorderLevel | C::FaceLevel | C::ElementLevel | C::ObjectLevel
        | C::GrowSelection | C::ShrinkSelection | C::SelectEdgeRing | C::SelectEdgeLoop
        | C::SelectEdgeRingUp | C::SelectEdgeRingDown | C::SelectEdgeRingUpAdd | C::SelectEdgeRingDownAdd
        | C::SelectEdgeRingUpSubtract | C::SelectEdgeRingDownSubtract | C::SelectEdgeLoopUp | C::SelectEdgeLoopDown
        | C::SelectEdgeLoopUpAdd | C::SelectEdgeLoopDownAdd | C::SelectEdgeLoopUpSubtract | C::SelectEdgeLoopDownSubtract
        | C::SelectByVertex | C::IgnoreBackfacingInSelections | C::PreviewSelectMode | C::Hide
        | C::AffectBackfacing | C::ConvertSelection { .. } | C::NamedSubSelectionCopy { .. } | C::NamedSubSelectionPaste { .. }
        | C::UseSoftSelection | C::LockSoftSelection | C::EditSoftSelectionMode | C::PaintSoftSelection | C::ShadedFaceToggle
        | C::SoftSelectionSettings { .. } | C::PaintSoftSelectionBlur | C::SoftSelectionRevert | C::PaintSoftSelectionStroke { .. }
        | C::Connect | C::Remove | C::Break | C::SplitEdges | C::TargetWeldMode | C::WeldSettings | C::ChamferMode | C::ChamferSettings
        | C::ConnectEdgeSettings | C::RemoveIsolatedVertices | C::RemoveUnusedMapVertices | C::InsertVertexMode | C::Cap | C::BridgeSettings
        | C::ExtrudeMode | C::ExtrudeSettings | C::Turn | C::EditTriangulationMode | C::Retriangulate | C::Create | C::RepeatLastOperation
        | C::Weld { .. } | C::Chamfer { .. } | C::ConnectEdges { .. } | C::ConnectVertices | C::Bridge { .. } | C::Extrude { .. }
        | C::PickSub { .. } | C::InsertVertexOnEdge { .. } | C::InsertVertexInFace { .. } | C::CreateVertex { .. }
        | C::InsetMode | C::InsetSettings | C::BevelMode | C::BevelSettings | C::OutlineMode | C::OutlineSettings | C::HingeFromEdgeMode | C::HingeFromEdgeSettings
        | C::ExtrudeAlongSplineMode | C::ExtrudeAlongSplineSettings | C::FlipNormals | C::MakePlanar | C::ViewAlign | C::AlignToGrid
        | C::Inset { .. } | C::Bevel { .. } | C::Outline { .. } | C::HingeFromEdge { .. } | C::PickHinge | C::MakePlanarAxis { .. }
        | C::Attach | C::AttachList | C::Detach | C::SlicePlaneMode | C::Slice | C::ResetSlicePlane | C::QuickSliceMode | C::Cut
        | C::MeshSmooth | C::MeshSmoothSettings | C::Tessellate | C::TessellateSettings | C::ConstrainToNone | C::ConstrainToEdges | C::ConstrainToFaces | C::ConstrainToNormals
        | C::AttachNode { .. } | C::DetachSelection { .. } | C::SetSlicePlane { .. } | C::SliceSplit { .. } | C::QuickSlice { .. } | C::PickFacePoint { .. }
        | C::MSmooth { .. } | C::TessellateFaces { .. } | C::Relax { .. } | C::RelaxSettings | C::FullInteractivity { .. } | C::PreserveUVs { .. } | C::DeleteIsolatedVertices
        | C::AutoSmooth | C::PaintDeformPushPull | C::PaintDeformRelax | C::SetMaterialId { .. } | C::SelectByMaterialId { .. } | C::SetSmoothingGroups { .. } | C::ClearSmoothingGroups
        | C::SelectBySmoothingGroup { .. } | C::AutoSmoothThreshold { .. } | C::SetVertexColor { .. } | C::SetVertexIllumination { .. } | C::SetVertexAlpha { .. } | C::SelectByVertexColor { .. }
        | C::SubdivisionSurface { .. } | C::PaintDeformRevert | C::PaintDeformCommit | C::PaintDeformCancel | C::PaintDeformSettings { .. } | C::PaintDeformStroke { .. }
        | C::CleanRemove | C::SwiftLoopMode | C::SwiftLoop { .. } | C::LoopMode | C::SelectByHalf { .. } | C::BranchesMode | C::Branch { .. } | C::BranchSettings { .. } | C::BridgeSpecificEdges { .. } | C::PickBridgeEdge { .. } => {
            crate::poly::execute(state, cmd)
        }
        C::SnapsToggle => toggle(&mut state.snaps_on, "snaps"),
        C::AngleSnapToggle => toggle(&mut state.angle_snap_on, "angle_snap"),
        C::PercentSnapToggle => toggle(&mut state.percent_snap_on, "percent_snap"),
        C::CycleActiveSnapType => {
            state.snaps.mode = state.snaps.mode.next();
            Ok(json!({ "mode": state.snaps.mode.label() }))
        }
        C::CycleSnapHit => {
            state.snaps.hit_index = state.snaps.hit_index.wrapping_add(1);
            Ok(json!({ "hit_index": state.snaps.hit_index }))
        }
        C::EnableAxisConstraintsInSnapsToggle => toggle(&mut state.snaps.use_axis_constraints, "use_axis_constraints"),
        C::SnapToFrozenObjectsToggle => toggle(&mut state.snaps.snap_to_frozen, "snap_to_frozen"),
        C::GridAndSnapSettings => {
            state.dialog = if state.dialog == Some(Dlg::GridAndSnapSettings) { None } else { Some(Dlg::GridAndSnapSettings) };
            Ok(json!({ "open": state.dialog.is_some() }))
        }
        C::SnapTarget { name, on } => {
            state.snaps.targets.set(&name, on)?;
            Ok(json!({ "target": name, "on": on }))
        }
        C::SnapOverride { name } => {
            if let Some(n) = &name {
                if n != "None" && n != "Last" {
                    crate::snap::SnapTargets::none().set(n, true)?;
                    state.snaps.last_override = Some(n.clone());
                }
            }
            state.snaps.override_target = match name.as_deref() {
                Some("Last") => state.snaps.last_override.clone(),
                _ => name,
            };
            Ok(json!({ "override": state.snaps.override_target }))
        }
        C::SnapSettings { mode, marker_display, marker_size, preview_radius_px, radius_px, angle_deg, percent, use_axis_constraints, show_rubber_band } => {
            let s = &mut state.snaps;
            if let Some(v) = mode { s.mode = v; }
            if let Some(v) = marker_display { s.marker_display = v; }
            if let Some(v) = marker_size { s.marker_size = v.max(1.0); }
            // Each radius pushes the other: preview below snap lowers the snap
            // radius, snap above preview raises the preview radius.
            if let Some(v) = preview_radius_px {
                s.preview_radius_px = v.max(1.0);
                s.radius_px = s.radius_px.min(s.preview_radius_px);
            }
            if let Some(v) = radius_px {
                s.radius_px = v.max(1.0);
                s.preview_radius_px = s.preview_radius_px.max(s.radius_px);
            }
            if let Some(v) = angle_deg { s.angle_deg = v.max(0.0); }
            if let Some(v) = percent { s.percent = v.max(0.0); }
            if let Some(v) = use_axis_constraints { s.use_axis_constraints = v; }
            if let Some(v) = show_rubber_band { s.show_rubber_band = v; }
            Ok(json!({}))
        }
        C::SnapMarkerColor { rgb } => {
            state.snaps.marker_color = rgb;
            Ok(json!({}))
        }
        C::UserGridSettings { activate_when_created, autogrid_object_space } => {
            if let Some(v) = activate_when_created {
                state.user_grids.activate_when_created = v;
            }
            if let Some(v) = autogrid_object_space {
                state.user_grids.autogrid_object_space = v;
            }
            Ok(json!({}))
        }
        C::HomeGrid { spacing, major_every, perspective_extent, inhibit_subdivision, inhibit_resize, dynamic_update_all } => {
            let g = &mut state.home_grid;
            if let Some(v) = spacing { g.spacing = v.max(0.001); }
            if let Some(v) = major_every { g.major_every = v.max(2); }
            if let Some(v) = perspective_extent { g.perspective_extent = v.max(1); }
            if let Some(v) = inhibit_subdivision { g.inhibit_subdivision = v; }
            if let Some(v) = inhibit_resize { g.inhibit_resize = v; }
            if let Some(v) = dynamic_update_all { g.dynamic_update_all = v; }
            state.grid_dirty = true;
            Ok(json!({ "spacing": g.spacing }))
        }
        C::GoToStartFrame => set_frame(state, state.frame_range.0),
        C::GoToEndFrame => set_frame(state, state.frame_range.1),

        C::HideSelection => {
            let ids: Vec<NodeId> = state.selection.iter().copied().collect();
            set_flags(state, "Hide Selection", ids, |h, f| (h.max(true), f))?;
            clear_selection(state);
            Ok(json!({}))
        }
        C::HideUnselected if state.selection_mode != SelectionMode::Object => crate::poly::hide_unselected(state),
        C::UnhideAll if state.selection_mode != SelectionMode::Object => crate::poly::unhide_all(state),
        C::HideUnselected => {
            let ids: Vec<NodeId> = state
                .scene
                .nodes()
                .filter(|n| !state.selection.contains(&n.id))
                .map(|n| n.id)
                .collect();
            set_flags(state, "Hide Unselected", ids, |_, f| (true, f))?;
            Ok(json!({}))
        }
        C::UnhideAll => {
            let ids: Vec<NodeId> = state.scene.nodes().filter(|n| n.hidden).map(|n| n.id).collect();
            set_flags(state, "Unhide All", ids, |_, f| (false, f))?;
            Ok(json!({}))
        }
        C::FreezeSelection => {
            let ids: Vec<NodeId> = state.selection.iter().copied().collect();
            set_flags(state, "Freeze Selection", ids, |h, _| (h, true))?;
            clear_selection(state);
            Ok(json!({}))
        }
        C::FreezeUnselected => {
            let ids: Vec<NodeId> = state.scene.nodes().filter(|n| !state.selection.contains(&n.id)).map(|n| n.id).collect();
            set_flags(state, "Freeze Unselected", ids, |h, _| (h, true))?;
            Ok(json!({}))
        }
        C::UnfreezeAll => {
            let ids: Vec<NodeId> = state.scene.nodes().filter(|n| n.frozen).map(|n| n.id).collect();
            set_flags(state, "Unfreeze All", ids, |h, _| (h, false))?;
            Ok(json!({}))
        }

        C::CreatePrimitive { kind, position, size } => {
            let size = Vec3::from(size);
            let r = size.x.max(size.y) * 0.5;
            // Objects with a base sit on the construction plane; centred
            // kinds are lifted by their radius so they rest on it too.
            let (object, z_offset) = match kind {
                PrimitiveKind::Box => (ObjectKind::box_object(size.y, size.x, size.z), 0.0),
                PrimitiveKind::Cone => (ObjectKind::cone_object(r, r * 0.5, size.z), 0.0),
                PrimitiveKind::Sphere => (ObjectKind::sphere_object(r), r),
                PrimitiveKind::GeoSphere => (ObjectKind::geosphere_object(r), r),
                PrimitiveKind::Cylinder => (ObjectKind::cylinder_object(r, size.z), 0.0),
                PrimitiveKind::Tube => (ObjectKind::tube_object(r, r * 0.6, size.z), 0.0),
                PrimitiveKind::Torus => (ObjectKind::torus_object(r * 0.75, r * 0.25), r * 0.25),
                PrimitiveKind::Pyramid => (ObjectKind::pyramid_object(size.x, size.y, size.z), 0.0),
                PrimitiveKind::Teapot => (ObjectKind::teapot_object(r), 0.0),
                PrimitiveKind::Plane => (ObjectKind::plane_object(size.y, size.x), 0.0),
            };
            let mut node = Node::new(&state.scene.next_name(object.base_name()), object);
            node.wire_color = state.display.new_object_color(state.scene.next_id());
            node.transform.position.set(Vec3::from(position) + Vec3::Z * z_offset);
            let name = node.name.clone();
            let id = NodeId(state.scene.next_id());
            commit(state, &format!("Create {name}"), Change::AddNode(node))?;
            state.selection = BTreeSet::from([id]);
            state.sub_selection.clear();
            state.selection_mode = SelectionMode::Object;
            state.active_tool = Tool::Select;
            Ok(json!({ "id": id.0, "name": name }))
        }
        C::CreateShape { kind, position, size } => {
            let size = Vec3::from(size);
            let r = size.x.max(size.y) * 0.5;
            let object = match kind {
                ShapeKind::Rectangle => ObjectKind::rectangle_object(size.y, size.x),
                ShapeKind::Circle => ObjectKind::circle_object(r),
                ShapeKind::Ellipse => ObjectKind::ellipse_object(size.y, size.x),
                ShapeKind::Arc => ObjectKind::arc_object(r),
                ShapeKind::Donut => ObjectKind::donut_object(r, r * 0.5),
                ShapeKind::NGon => ObjectKind::ngon_object(r),
                ShapeKind::Star => ObjectKind::star_object(r, r * 0.5),
                ShapeKind::Helix => ObjectKind::helix_object(r, r, size.z),
            };
            let id = add_object(state, object, Vec3::from(position))?;
            Ok(json!({ "id": id.0, "name": state.scene.node(id).map(|n| n.name.clone()) }))
        }
        C::ConvertToEditableSpline => {
            let ids: Vec<NodeId> = state.selection.iter().copied().filter(|id| state.scene.node(*id).is_some_and(|n| n.object.is_shape() && n.object.spline().is_none())).collect();
            if ids.is_empty() {
                return Err("select a shape first".into());
            }
            state.undo.begin("Convert to Editable Spline");
            for id in &ids {
                if let Err(e) = detach_instance(state, *id) {
                    state.undo.end();
                    return Err(e);
                }
            }
            let mut changes = Vec::new();
            for id in ids {
                let node = state.scene.node(id).ok_or_else(|| missing(id))?;
                let shape = node.object.shape().ok_or("not a shape")?;
                changes.push(Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(node.object.with_shape(shape)) });
            }
            let r = commit(state, "Convert to Editable Spline", Change::Group(changes));
            state.undo.end();
            r?;
            Ok(json!({}))
        }
        C::LineStart { point } => {
            let shape = loom_shape::Shape::from_splines(vec![loom_shape::Spline::new(vec![loom_shape::Knot::corner(Vec3::ZERO)], false)]);
            let id = add_object(state, ObjectKind::line_object(shape), Vec3::from(point))?;
            Ok(json!({ "id": id.0, "name": state.scene.node(id).map(|n| n.name.clone()) }))
        }
        C::LineAddKnot { node, point, smooth } => edit_line(state, NodeId(node), "Line Add Knot", |spline, local| {
            spline.knots.push(if smooth { loom_shape::Knot::smooth(local) } else { loom_shape::Knot::corner(local) });
            Ok(())
        }, Some(Vec3::from(point))),
        C::LineSetHandle { node, point } => edit_line(state, NodeId(node), "Line Set Handle", |spline, local| {
            let Some(k) = spline.knots.last_mut() else { return Err("the line has no knot".into()) };
            k.kind = loom_shape::KnotKind::Bezier;
            k.out_vec = local - k.pos;
            k.in_vec = -k.out_vec;
            Ok(())
        }, Some(Vec3::from(point))),
        C::LineSmoothLastKnot { node } => edit_line(state, NodeId(node), "Line Smooth Last Knot", |spline, _| {
            let Some(k) = spline.knots.last_mut() else { return Err("the line has no knot".into()) };
            k.kind = loom_shape::KnotKind::Smooth;
            Ok(())
        }, None),
        C::LineRemoveLastKnot { node } => edit_line(state, NodeId(node), "Line Remove Last Knot", |spline, _| {
            if spline.knots.len() <= 1 {
                return Err("the line keeps its first knot".into());
            }
            spline.knots.pop();
            Ok(())
        }, None),
        C::LineEnd { node } => crate::spline::line_end(state, NodeId(node)),
        C::LineCloseAsk { node, point, smooth } => {
            state.scene.node(NodeId(node)).ok_or_else(|| missing(NodeId(node)))?;
            state.line_close_pending = Some((NodeId(node), point, smooth));
            state.dialog = Some(Dlg::CloseSpline);
            Ok(json!({ "dialog": "CloseSpline" }))
        }
        C::LineCloseReply { close } => {
            let (node, point, smooth) = state.line_close_pending.take().ok_or("no Close spline prompt is open")?;
            if state.dialog == Some(Dlg::CloseSpline) {
                state.dialog = None;
            }
            if close {
                execute(state, C::LineClose { node: node.0 })
            } else {
                execute(state, C::LineAddKnot { node: node.0, point, smooth })
            }
        }
        C::LineClose { node } => edit_line(state, NodeId(node), "Line Close", |spline, _| {
            if spline.knots.len() < 3 {
                return Err("a spline needs three knots to close".into());
            }
            spline.closed = true;
            Ok(())
        }, None),
        C::CreateBoolean { operand_b, operation, move_operand } => compound_boolean(state, NodeId(operand_b), operation, move_operand),
        C::PickOperandB => toggle_pick_mode(state, PickMode::BooleanOperandB),
        C::CreateProBoolean => {
            let id = state.selection.iter().next().copied().ok_or("select the base object first")?;
            let node = state.scene.node(id).ok_or_else(|| missing(id))?;
            if node.object.is_shape() || matches!(node.object, ObjectKind::Camera { .. } | ObjectKind::Light(_) | ObjectKind::Group { .. }) {
                return Err("ProBoolean needs a mesh object".into());
            }
            let base = state.scene.eval_mesh(id).value.mesh.clone();
            let mut changes = vec![Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(ObjectKind::proboolean_object(base)) }];
            if !node.modifiers.is_empty() {
                changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: Vec::new() });
            }
            commit(state, "ProBoolean", Change::Group(changes))?;
            Ok(json!({ "id": id.0 }))
        }
        C::ProBooleanPickOperand => toggle_pick_mode(state, PickMode::ProBooleanOperand),
        C::ProBooleanAddOperand { node, operation, move_operand } => {
            let id = state.selection.iter().next().copied().ok_or("select the ProBoolean first")?;
            let other = NodeId(node);
            if other == id {
                return Err("pick another object".into());
            }
            let base_node = state.scene.node(id).ok_or_else(|| missing(id))?;
            let ObjectKind::ProBoolean { base, operands, .. } = &base_node.object else { return Err("the selection is not a ProBoolean".into()) };
            let onode = state.scene.node(other).ok_or_else(|| missing(other))?;
            let mut operand = state.scene.eval_mesh(other).value.mesh.clone();
            let to_local = state.scene.object_matrix(id).inverse() * state.scene.object_matrix(other);
            edit_ops::transform_mesh(&mut operand, to_local)?;
            let mut ops = operands.clone();
            ops.push((operation.min(5), operand));
            let mut changes = vec![Change::ReplaceObject { id, before: Box::new(base_node.object.clone()), after: Box::new(ObjectKind::ProBoolean { base: base.clone(), operands: ops, cache: Default::default() }) }];
            if move_operand {
                let _ = onode;
                changes.push(Change::RemoveNode(other));
            }
            commit(state, "ProBoolean Operand", Change::Group(changes))?;
            state.selection = BTreeSet::from([id]);
            Ok(json!({ "operands": state.scene.node(id).map(|n| match &n.object { ObjectKind::ProBoolean { operands, .. } => operands.len(), _ => 0 }) }))
        }
        C::CreateLoft { path } => {
            // Get Path: the selected shape is the cross-section at 0 %, the picked shape the path.
            let id = state.selection.iter().next().copied().ok_or("select the cross-section shape first")?;
            let path_id = NodeId(path);
            if path_id == id {
                return Err("pick another shape as the path".into());
            }
            let node = state.scene.node(id).ok_or_else(|| missing(id))?;
            let section = node.object.shape().ok_or("Loft needs a shape as its cross-section")?;
            let pnode = state.scene.node(path_id).ok_or_else(|| missing(path_id))?;
            let mut path_shape = pnode.object.shape().ok_or("the path must be a shape")?;
            let to_local = state.scene.object_matrix(id).inverse() * state.scene.object_matrix(path_id);
            for sp in &mut path_shape.splines {
                for k in &mut sp.knots {
                    k.pos = to_local.transform_point3(k.pos);
                    k.in_vec = to_local.transform_vector3(k.in_vec);
                    k.out_vec = to_local.transform_vector3(k.out_vec);
                }
            }
            let mut changes = vec![Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(ObjectKind::loft_object(path_shape, vec![(0.0, section)])) }];
            if !node.modifiers.is_empty() {
                changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: Vec::new() });
            }
            commit(state, "Loft", Change::Group(changes))?;
            state.pick_mode = None;
            Ok(json!({ "id": id.0 }))
        }
        C::LoftGetPathMode => toggle_pick_mode(state, PickMode::LoftGetPath),
        C::LoftGetShapeMode => toggle_pick_mode(state, PickMode::LoftGetShape),
        C::LoftAddShape { shape, percent } => {
            let id = state.selection.iter().next().copied().ok_or("select the Loft first")?;
            let sid = NodeId(shape);
            let node = state.scene.node(id).ok_or_else(|| missing(id))?;
            let snode = state.scene.node(sid).ok_or_else(|| missing(sid))?;
            let section = snode.object.shape().ok_or("Get Shape: pick a shape")?;
            let after = match &node.object {
                ObjectKind::Loft { path, shapes, shape_steps, path_steps, cap_start, cap_end, contour, flip_normals, .. } => {
                    let mut shapes = shapes.clone();
                    shapes.retain(|(p, _)| (p - percent).abs() > 1e-3);
                    shapes.push((percent.clamp(0.0, 100.0), section));
                    ObjectKind::Loft { path: path.clone(), shapes, shape_steps: shape_steps.clone(), path_steps: path_steps.clone(), cap_start: cap_start.clone(), cap_end: cap_end.clone(), contour: contour.clone(), flip_normals: flip_normals.clone(), cache: Default::default() }
                }
                other if other.shape().is_some() => {
                    // Get Shape with a path selected: the path becomes the Loft with the picked section.
                    ObjectKind::loft_object(other.shape().unwrap(), vec![(percent.clamp(0.0, 100.0), section)])
                }
                _ => return Err("the selection is not a Loft or a path shape".into()),
            };
            commit(state, "Get Shape", Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(after) })?;
            state.pick_mode = None;
            Ok(json!({ "id": id.0 }))
        }
        C::CompoundSettings { name, value } => {
            let c = &mut state.compound_settings;
            match name.as_str() {
                "Boolean Operation" => c.boolean_op = (value.max(0.0) as u32).min(7),
                "Move Operand" => c.move_operand = value != 0.0,
                "ProBoolean Operation" => c.proboolean_op = (value.max(0.0) as u32).min(5),
                "Loft Path" => c.loft_path_percent = (value as f32).clamp(0.0, 100.0),
                other => return Err(format!("unknown compound setting {other:?}")),
            }
            Ok(json!({}))
        }
        c @ (C::CreateCamera { .. } | C::CreateCameraAt { .. } | C::CameraType { .. } | C::CreateCameraFromView | C::NavigateCamera { .. }) => crate::camera::execute(state, c),
        c @ (C::CreateLight { .. } | C::LightType { .. } | C::SetLightExclusion { .. } | C::SetLightProjectorMap { .. } | C::LightLister | C::PhotometricTemplate { .. } | C::CreateSunlight { .. } | C::SunlightParameters { .. } | C::AddDefaultLightsToScene { .. } | C::SelectTarget) => crate::light::execute(state, c),
        C::SetParameter { id, name, value } => {
            let id = NodeId(id);
            let node = state.scene.node(id).ok_or_else(|| missing(id))?;
            if let ObjectKind::Biped(b) = &node.object {
                // Structure parameters rebuild the parts.
                let mut after = (**b).clone();
                after.set(&name, value)?;
                return crate::biped::rebuild(state, id, after, &name);
            }
            // Target Distance of a target camera or light moves the target
            // along the line of sight.
            let live_target = match &node.object {
                ObjectKind::Camera { target, .. } => *target,
                ObjectKind::Light(l) => l.target,
                _ => None,
            }
            .filter(|t| state.scene.node(*t).is_some());
            if let (Some(t), "Target Distance") = (live_target, name.as_str()) {
                let world = state.scene.world_matrix(id);
                let at = world.w_axis.truncate() - world.z_axis.truncate().normalize_or_zero() * (value as f32).max(1e-3);
                let tnode = state.scene.node(t).ok_or_else(|| missing(t))?;
                let parent = tnode.parent.map(|p| state.scene.world_matrix(p)).unwrap_or(glam::Mat4::IDENTITY);
                let mut after = tnode.transform.clone();
                after.position.set(parent.inverse().transform_point3(at));
                commit(state, "Target Distance", Change::SetTransform { id: t, before: tnode.transform.clone(), after })?;
                return Ok(json!({}));
            }
            let root = node.instance_of.unwrap_or(id);
            let group: Vec<NodeId> = state
                .scene
                .nodes()
                .filter(|n| n.id == root || n.instance_of == Some(root))
                .map(|n| n.id)
                .collect();
            let mut changes = Vec::new();
            for id in group {
                let before = state.scene.node(id).ok_or_else(|| missing(id))?.object.clone();
                let mut after = before.clone();
                after.set_parameter(&name, value)?;
                changes.push(Change::ReplaceObject { id, before: Box::new(before), after: Box::new(after) });
            }
            commit(state, &name, Change::Group(changes))?;
            Ok(json!({}))
        }
        C::ConvertToEditablePoly => {
            let ids: Vec<NodeId> = state.selection.iter().copied().collect();
            state.undo.begin("Convert to Editable Poly");
            for id in &ids {
                if let Err(e) = detach_instance(state, *id) {
                    state.undo.end();
                    return Err(e);
                }
            }
            let mut changes = Vec::new();
            for id in ids {
                let node = state.scene.node(id).ok_or_else(|| missing(id))?;
                if matches!(node.object, ObjectKind::EditablePoly(_)) && node.modifiers.is_empty() {
                    continue;
                }
                // With a stack the whole stack collapses into the result (Convert To).
                let mesh = if node.modifiers.is_empty() { node.object.mesh().into_owned() } else { state.scene.eval_mesh(id).value.mesh.clone() };
                // An Editable Mesh's triangles merge back into their polygons.
                let mesh = if mesh.has_invisible_edges() { loom_mesh::tri_mesh::to_poly(&mesh)? } else { mesh };
                let mut poly = match &node.object {
                    ObjectKind::EditablePoly(_) => node.object.with_mesh(mesh),
                    _ => ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(mesh)),
                };
                if let ObjectKind::EditablePoly(p) = &mut poly {
                    if !node.modifiers.is_empty() {
                        p.subdiv.use_nurms.set(false);
                    }
                }
                changes.push(Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(poly) });
                if !node.modifiers.is_empty() {
                    changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: Vec::new() });
                }
            }
            let r = if changes.is_empty() { Ok(()) } else { commit(state, "Convert to Editable Poly", Change::Group(changes)) };
            state.undo.end();
            r?;
            Ok(json!({}))
        }
        C::SelectionFilter { filter } => {
            state.selection_filter = filter;
            Ok(json!({ "filter": filter.label() }))
        }
        C::WindowCrossingSelectionToggle => toggle(&mut state.window_selection, "window"),
        C::SelectionRegion { shape } => {
            state.selection_region = shape;
            Ok(json!({ "region": shape.label() }))
        }
        C::ReferenceCoordinateSystem { system: CoordSystem::Pick } => {
            // Pick: the next clicked object's axes become the system (Pick Hit).
            state.pick_mode = Some(PickMode::CoordSystem);
            Ok(json!({ "pick": true }))
        }
        C::ReferenceCoordinateSystem { system } => {
            state.coord_system = system;
            if state.prefs.ref_coord_constant {
                state.coord_by_tool = [system; 4];
            } else {
                state.coord_by_tool[tool_index(state.active_tool)] = system;
            }
            Ok(json!({ "system": system.label() }))
        }
        C::UseCenter { mode } => {
            state.center_mode = mode;
            if state.selection.len() > 1 {
                state.center_multi = mode;
            } else {
                state.center_single = mode;
            }
            Ok(json!({ "center": mode.label() }))
        }
        C::KeyboardShortcutOverrideToggle => toggle(&mut state.keyboard_override, "keyboard_override"),
        C::SelectAndLink => {
            state.pick_mode = Some(PickMode::Link);
            state.active_tool = Tool::Select;
            Ok(json!({}))
        }
        C::UnlinkSelection => {
            let ids: Vec<NodeId> = state.selection.iter().copied().collect();
            let mut changes = Vec::new();
            for c in ids {
                let node = state.scene.node(c).ok_or_else(|| missing(c))?;
                if node.parent.is_none() {
                    continue;
                }
                let t_before = node.transform.clone();
                let mut sim = state.scene.clone();
                sim.set_parent(c, None);
                let t_after = sim.node(c).ok_or_else(|| missing(c))?.transform.clone();
                changes.push(Change::SetTransform { id: c, before: t_before, after: t_after });
                changes.push(Change::SetParent { id: c, before: node.parent, after: None });
            }
            if !changes.is_empty() {
                commit_static(state, "Unlink", Change::Group(changes))?;
            }
            Ok(json!({}))
        }
        C::NamedSelectionSet { name } => {
            if name.trim().is_empty() {
                return Err("set name required".into());
            }
            state.named_sets.insert(name.clone(), state.selection.iter().copied().collect());
            Ok(json!({ "set": name, "count": state.selection.len() }))
        }
        C::SelectNamedSet { name } => {
            let ids = state.named_sets.get(&name).cloned().ok_or_else(|| format!("no named set `{name}`"))?;
            state.selection = ids.into_iter().filter(|id| state.scene.node(*id).is_some()).collect();
            state.sub_selection.clear();
            Ok(selection_json(state))
        }
        C::DeleteNamedSet { name } => {
            state.named_sets.remove(&name).ok_or_else(|| format!("no named set `{name}`"))?;
            Ok(json!({}))
        }
        C::EditNamedSelectionSets => {
            state.dialog = Some(Dlg::NamedSelectionSets);
            Ok(json!({}))
        }
        C::HideNodes { ids } => {
            set_flags(state, "Hide", ids.into_iter().map(NodeId).collect(), |_, f| (true, f))?;
            prune_selection_hidden(state);
            Ok(json!({}))
        }
        C::UnhideNodes { ids } => {
            set_flags(state, "Unhide", ids.into_iter().map(NodeId).collect(), |_, f| (false, f))?;
            Ok(json!({}))
        }
        C::FreezeNodes { ids } => {
            set_flags(state, "Freeze", ids.into_iter().map(NodeId).collect(), |h, _| (h, true))?;
            prune_selection_hidden(state);
            Ok(json!({}))
        }
        C::UnfreezeNodes { ids } => {
            set_flags(state, "Unfreeze", ids.into_iter().map(NodeId).collect(), |h, _| (h, false))?;
            Ok(json!({}))
        }
        C::AffectPivotOnly => {
            state.affect_pivot_only = !state.affect_pivot_only;
            if state.affect_pivot_only {
                state.affect_object_only = false;
                state.affect_hierarchy_only = false;
            }
            Ok(json!({ "affect_pivot_only": state.affect_pivot_only }))
        }
        C::AffectObjectOnly => {
            state.affect_object_only = !state.affect_object_only;
            if state.affect_object_only {
                state.affect_pivot_only = false;
                state.affect_hierarchy_only = false;
            }
            Ok(json!({ "affect_object_only": state.affect_object_only }))
        }
        C::AffectHierarchyOnly => {
            state.affect_hierarchy_only = !state.affect_hierarchy_only;
            if state.affect_hierarchy_only {
                state.affect_pivot_only = false;
                state.affect_object_only = false;
            }
            Ok(json!({ "affect_hierarchy_only": state.affect_hierarchy_only }))
        }
        C::DontAffectChildren => toggle(&mut state.dont_affect_children, "dont_affect_children"),
        C::EditWorkingPivot => {
            if state.edit_working_pivot {
                // Ending the edit restores the tool and the managers it started with.
                state.edit_working_pivot = false;
                if let Some(saved) = state.working_pivot_saved.0.take() {
                    restore_managers(state, saved);
                }
            } else {
                state.working_pivot_saved.0 = Some(save_managers(state));
                state.edit_working_pivot = true;
                state.coord_by_tool = [CoordSystem::Local; 4];
                state.coord_system = CoordSystem::Local;
            }
            Ok(json!({ "edit_working_pivot": state.edit_working_pivot }))
        }
        C::UseWorkingPivot => {
            if state.use_working_pivot {
                state.use_working_pivot = false;
                if let Some(saved) = state.working_pivot_saved.1.take() {
                    let tool = state.active_tool;
                    restore_managers(state, SavedManagers { tool, ..saved });
                }
            } else {
                // Every transform uses the Working system about the working pivot.
                state.working_pivot_saved.1 = Some(save_managers(state));
                state.use_working_pivot = true;
                let tc = CenterMode::TransformCoordinate;
                state.coord_by_tool = [CoordSystem::Working; 4];
                state.coord_system = CoordSystem::Working;
                state.center_by_tool = [(tc, tc); 4];
                (state.center_single, state.center_multi) = (tc, tc);
                state.refresh_center_mode();
            }
            Ok(json!({ "use_working_pivot": state.use_working_pivot }))
        }
        C::WorkingPivotAlignToView => {
            let axes = crate::transform::view_axes(state, state.active_viewport);
            state.working_pivot.rotation = Quat::from_mat3(&axes).normalize();
            Ok(json!({}))
        }
        C::WorkingPivotReset => {
            let origin = if state.selection_mode != SelectionMode::Object && !state.sub_selection.is_empty() {
                // Sub-objects: the average of the selected vertices.
                let pts: Vec<Vec3> = selected_vertices(state)
                    .into_iter()
                    .flat_map(|(n, vs)| {
                        let m = state.scene.object_matrix(n);
                        let mesh = state.scene.node(n).map(|nd| nd.cage_mesh().into_owned());
                        vs.into_iter().filter_map(move |v| mesh.as_ref()?.vertex_position(v).map(|p| m.transform_point3(p)))
                    })
                    .collect();
                if pts.is_empty() {
                    return Ok(json!({}));
                }
                pts.iter().sum::<Vec3>() / pts.len() as f32
            } else {
                // The pivot of the last-selected object (the highest id here).
                match state.selection.iter().next_back() {
                    Some(id) => state.scene.world_matrix(*id).w_axis.truncate(),
                    None => return Ok(json!({})),
                }
            };
            state.working_pivot.origin = origin;
            Ok(json!({ "origin": origin.to_array() }))
        }
        C::PlacePivotToView => toggle_pick_mode(state, PickMode::WorkingPivotView),
        C::PlacePivotToSurface => toggle_pick_mode(state, PickMode::WorkingPivotSurface),
        C::PlacePivotAlignToViewToggle => toggle(&mut state.working_pivot_align_to_view, "align_to_view"),
        C::SetWorkingPivot { position, rotation_deg } => {
            if let Some(p) = position {
                state.working_pivot.origin = Vec3::from(p);
            }
            if let Some(r) = rotation_deg {
                state.working_pivot.rotation = Quat::from_euler(glam::EulerRot::XYZ, r[0].to_radians(), r[1].to_radians(), r[2].to_radians());
            }
            Ok(json!({ "origin": state.working_pivot.origin.to_array() }))
        }
        C::AlignPivotToWorld if state.affect_object_only => {
            // Affect Object Only: the object turns to the world; the pivot stays.
            object_offset_change(state, "Align to World", |state, id, off| {
                let (_, wq, _) = state.scene.world_matrix(id).to_scale_rotation_translation();
                let (s, _, t) = off.to_scale_rotation_translation();
                Mat4::from_scale_rotation_translation(s, wq.inverse(), t)
            })
        }
        C::CenterObjectToPivot => object_offset_change(state, "Center to Pivot", |state, id, off| {
            // Shift the offset so the bounds centre lands on the pivot.
            let Some((min, max)) = state.object_bounds(id) else { return off };
            let centre_local = state.scene.world_matrix(id).inverse().transform_point3((min + max) * 0.5);
            Mat4::from_translation(-centre_local) * off
        }),
        C::AlignObjectToPivot => object_offset_change(state, "Align to Pivot", |_, _, off| {
            let (s, _, t) = off.to_scale_rotation_translation();
            Mat4::from_scale_rotation_translation(s, Quat::IDENTITY, t)
        }),
        C::AlignPivotToObject => pivot_world_change(state, "Align to Object", |state, id| {
            // The pivot turns to the object's own axes.
            let (_, oq, _) = state.scene.object_matrix(id).to_scale_rotation_translation();
            let (ws, _, wt) = state.scene.world_matrix(id).to_scale_rotation_translation();
            (Mat4::from_scale_rotation_translation(ws, oq, wt), None)
        }),
        C::ResetTransform => pivot_world_change(state, "Reset Transform", |state, id| {
            // The local axes turn to the world.
            let (ws, _, wt) = state.scene.world_matrix(id).to_scale_rotation_translation();
            (Mat4::from_scale_rotation_translation(ws, Quat::IDENTITY, wt), None)
        }),
        C::ResetScale => pivot_world_change(state, "Reset Scale", |state, id| {
            // The node scale becomes 100 %; the object offset takes it.
            let (_, wq, wt) = state.scene.world_matrix(id).to_scale_rotation_translation();
            (Mat4::from_rotation_translation(wq, wt), Some(Vec3::ONE))
        }),
        C::CenterPivotToObject => {
            let ids = selected_ids(state);
            let mut changes = Vec::new();
            for id in ids {
                let Some((min, max)) = state.object_bounds(id) else { continue };
                let node = state.scene.node(id).ok_or_else(|| missing(id))?;
                let centre = (min + max) * 0.5;
                let parent_world = state.scene.parent_frame(id);
                let mut after = node.transform.clone();
                after.position.set(parent_world.inverse().transform_point3(centre));
                let new_world = parent_world * after.matrix();
                let mut props = node.props();
                props.object_offset = Transform::from_matrix(new_world.inverse() * state.scene.object_matrix(id));
                changes.push(Change::SetNodeProps { id, before: Box::new(node.props()), after: Box::new(props) });
                changes.extend(keep_children(state, id, new_world));
                changes.push(Change::SetTransform { id, before: node.transform.clone(), after });
            }
            if !changes.is_empty() {
                commit(state, "Center to Object", Change::Group(changes))?;
            }
            Ok(json!({}))
        }
        C::AlignPivotToWorld => {
            let ids = selected_ids(state);
            let mut changes = Vec::new();
            for id in ids {
                let node = state.scene.node(id).ok_or_else(|| missing(id))?;
                let parent_world = state.scene.parent_frame(id);
                let world = state.scene.world_matrix(id);
                let pos = world.w_axis.truncate();
                let after = Transform::from_matrix(parent_world.inverse() * Mat4::from_translation(pos));
                let new_world = parent_world * after.matrix();
                let mut props = node.props();
                props.object_offset = Transform::from_matrix(new_world.inverse() * state.scene.object_matrix(id));
                changes.push(Change::SetNodeProps { id, before: Box::new(node.props()), after: Box::new(props) });
                changes.extend(keep_children(state, id, new_world));
                changes.push(Change::SetTransform { id, before: node.transform.clone(), after });
            }
            if !changes.is_empty() {
                commit(state, "Align to World", Change::Group(changes))?;
            }
            Ok(json!({}))
        }
        C::ResetPivot => {
            let ids = selected_ids(state);
            let mut changes = Vec::new();
            for id in ids {
                let node = state.scene.node(id).ok_or_else(|| missing(id))?;
                // Pivot returns to the object origin: node TM takes the offset.
                let parent_world = state.scene.parent_frame(id);
                let after = Transform::from_matrix(parent_world.inverse() * state.scene.object_matrix(id));
                let new_world = parent_world * after.matrix();
                let mut props = node.props();
                props.object_offset = Transform::default();
                changes.push(Change::SetNodeProps { id, before: Box::new(node.props()), after: Box::new(props) });
                changes.extend(keep_children(state, id, new_world));
                changes.push(Change::SetTransform { id, before: node.transform.clone(), after });
            }
            if !changes.is_empty() {
                commit(state, "Reset Pivot", Change::Group(changes))?;
            }
            Ok(json!({}))
        }
        C::SetPivot { id, position, rotation_deg } => {
            let id = NodeId(id);
            let node = state.scene.node(id).ok_or_else(|| missing(id))?;
            let parent_world = state.scene.parent_frame(id);
            let mut after = node.transform.clone();
            if let Some(p) = position {
                after.position.set(Vec3::from(p));
            }
            if let Some(r) = rotation_deg {
                after.rotation.set(Vec3::from(r));
            }
            let new_world = parent_world * after.matrix();
            let mut props = node.props();
            props.object_offset = Transform::from_matrix(new_world.inverse() * state.scene.object_matrix(id));
            let before_t = node.transform.clone();
            let before_p = node.props();
            let mut changes = vec![Change::SetNodeProps { id, before: Box::new(before_p), after: Box::new(props) }];
            changes.extend(keep_children(state, id, new_world));
            changes.push(Change::SetTransform { id, before: before_t, after });
            commit(state, "Set Pivot", Change::Group(changes))?;
            Ok(json!({}))
        }
        C::SetNodeLocks { id, move_axes, rotate_axes, scale_axes } => {
            props_change(state, "Locks", vec![NodeId(id)], |p, _| {
                if let Some(m) = move_axes { p.locks.move_axes = m; }
                if let Some(r) = rotate_axes { p.locks.rotate_axes = r; }
                if let Some(s) = scale_axes { p.locks.scale_axes = s; }
            })?;
            Ok(json!({}))
        }
        C::SetNodeInherit { id, move_axes, rotate_axes, scale_axes } => {
            props_change(state, "Inherit", vec![NodeId(id)], |p, _| {
                if let Some(m) = move_axes { p.inherit.move_axes = m; }
                if let Some(r) = rotate_axes { p.inherit.rotate_axes = r; }
                if let Some(s) = scale_axes { p.inherit.scale_axes = s; }
            })?;
            Ok(json!({}))
        }
        C::SetSelectionDisplayProperty { name, value } => {
            if loom_scene::DisplayProps::default().get(&name).is_none() {
                return Err(format!("no display property `{name}`"));
            }
            let ids = selected_ids(state);
            props_change(state, &name, ids, |p, _| {
                p.display.set(&name, value);
            })?;
            Ok(json!({}))
        }
        C::SetDisplayProperty { id, name, value } => {
            if loom_scene::DisplayProps::default().get(&name).is_none() {
                return Err(format!("no display property `{name}`"));
            }
            props_change(state, &name, vec![NodeId(id)], |p, _| {
                p.display.set(&name, value);
            })?;
            Ok(json!({}))
        }
        C::SetRenderProperty { name, value } => {
            if loom_scene::RenderProps::default().get(&name).is_none() {
                return Err(format!("no rendering control `{name}`"));
            }
            props_change(state, &name, selected_ids(state), |p, _| {
                p.render.set(&name, value);
            })?;
            Ok(json!({}))
        }
        C::SetObjectVisibility { value } => {
            let mut changes = Vec::new();
            for id in selected_ids(state) {
                let n = state.scene.node(id).ok_or_else(|| missing(id))?;
                let mut after = n.visibility.clone().unwrap_or_else(|| loom_scene::Param::new(1.0));
                after.set(value.clamp(0.0, 1.0));
                if Some(&after) != n.visibility.as_ref() {
                    changes.push(Change::SetVisibility { id, before: n.visibility.clone(), after: Some(after) });
                }
            }
            if !changes.is_empty() {
                commit(state, "Visibility", Change::Group(changes))?;
            }
            Ok(json!({}))
        }
        C::SetUserProperties { id, text } => {
            props_change(state, "User Defined", vec![NodeId(id)], |p, _| {
                p.user_props = text.clone();
            })?;
            Ok(json!({}))
        }
        C::HideByHit => {
            state.pick_mode = Some(PickMode::HideByHit);
            Ok(json!({}))
        }
        C::FreezeByHit => {
            state.pick_mode = Some(PickMode::FreezeByHit);
            Ok(json!({}))
        }
        C::UnfreezeByHit => {
            state.pick_mode = Some(PickMode::UnfreezeByHit);
            Ok(json!({}))
        }
        C::ExitPickMode => {
            // Cut: the first right-click ends the current cut, the second leaves Cut.
            if state.pick_mode == Some(PickMode::CutB) {
                state.pick_mode = Some(PickMode::CutA);
                state.pick_point_a = None;
                return Ok(json!({ "pick": "CutA" }));
            }
            state.pick_mode = None;
            state.refine_chain = None;
            state.pick_source = None;
            if !state.create_verts.is_empty() {
                return crate::poly_edit::finish_create(state);
            }
            Ok(json!({}))
        }
        C::PickHit { id } => {
            let id = NodeId(id);
            state.scene.node(id).ok_or_else(|| missing(id))?;
            match state.pick_mode {
                Some(PickMode::AttachObject) => {
                    return crate::poly_edit::execute(state, C::AttachNode { id: id.0 });
                }
                Some(PickMode::SplineRefine | PickMode::SplineInsert) => return Err("Refine / Insert pick a segment, not an object".into()),
                Some(PickMode::SplineFillet | PickMode::SplineChamfer) => return Err("Fillet / Chamfer drag on a selected vertex, not an object".into()),
                Some(PickMode::UnwrapSplineMap) => {
                    let mapping = state.uv_editor.spline_map_mapping;
                    let normalize = state.unwrap_entry().and_then(|(n, i)| match &state.scene.node(n)?.modifiers[i].modifier {
                        loom_scene::Modifier::UnwrapUVW(u) => Some(u.normalize_map.get()),
                        _ => None,
                    }).unwrap_or(true);
                    return crate::unwrap::execute(state, C::UnwrapSplineMap { spline: id.0, mapping, normalize });
                }
                Some(PickMode::SplineTrim | PickMode::SplineExtend | PickMode::SplineCrossInsert | PickMode::SplineBind | PickMode::TrimExtendModifier) => return Err("this spline tool picks a point on the spline, not an object".into()),
                Some(PickMode::BooleanOperandB) => {
                    let c = state.compound_settings;
                    return execute(state, C::CreateBoolean { operand_b: id.0, operation: c.boolean_op, move_operand: c.move_operand });
                }
                Some(PickMode::ProBooleanOperand) => {
                    let c = state.compound_settings;
                    return execute(state, C::ProBooleanAddOperand { node: id.0, operation: c.proboolean_op, move_operand: c.move_operand });
                }
                Some(PickMode::LoftGetPath) => return execute(state, C::CreateLoft { path: id.0 }),
                Some(PickMode::LoftGetShape) => {
                    let pct = state.compound_settings.loft_path_percent;
                    return execute(state, C::LoftAddShape { shape: id.0, percent: pct });
                }
                Some(PickMode::SplineAttach) => {
                    let reorient = state.spline_settings.attach_reorient;
                    return crate::spline::execute(state, C::SplineAttach { node: id.0, reorient });
                }
                Some(PickMode::ExtrudeAlongSpline) => {
                    if state.dialog == Some(Dlg::PolySettings(crate::PolySettingsKind::ExtrudeAlongSpline)) {
                        // The open dialog keeps the path; its preview runs the extrusion.
                        state.scene.node(id).and_then(|n| n.edit_shape()).ok_or("the picked object is not a shape")?;
                        state.spline_settings.extrude_path = Some(id);
                        state.pick_mode = None;
                        return Ok(json!({ "path": id.0 }));
                    }
                    let ss = state.spline_settings;
                    return crate::spline::execute(state, C::ExtrudeAlongSpline { spline: id.0, segments: ss.extrude_segments, taper: ss.extrude_taper, taper_curve: ss.extrude_taper_curve, twist: ss.extrude_twist, rotation: ss.extrude_rotation, align: ss.extrude_align });
                }
                Some(PickMode::GroupAttach) => {
                    return crate::groups::attach_to_group(state, id);
                }
                Some(PickMode::UvwMapAcquire) => {
                    state.pick_mode = None;
                    return crate::stack::execute(state, C::UvwMapAcquire { node: id.0 });
                }
                Some(PickMode::UvwMapNormal | PickMode::UvwMapRegionA | PickMode::UvwMapRegionB | PickMode::BipedFootstep) => return Err("pick a surface point, not an object".into()),
                Some(PickMode::SeamPointA | PickMode::SeamPointB) => return Err("Point To Point Seam picks a vertex, not an object".into()),
                Some(PickMode::TvTargetWeldSource | PickMode::TvTargetWeldTarget) => return Err("Target Weld picks a texture vertex in the Edit UVWs window".into()),
                // Pick Bone / Add keep picking until right-click or the button again.
                Some(PickMode::SkinMorphBone) => return crate::morph::execute(state, C::SkinMorphAddBones { ids: vec![id.0] }),
                Some(PickMode::SkinWrapControl) => return crate::morph::execute(state, C::SkinWrapAddBase { id: id.0 }),
                Some(PickMode::SkinMorphExternal) => {
                    state.pick_mode = None;
                    let morph = state.skin_morph_current.ok_or("no morph is highlighted")? as u32;
                    return crate::morph::execute(state, C::SkinMorphSetExternal { morph, node: Some(id.0) });
                }
                Some(PickMode::ModifierNode) => {
                    let Some(index) = state.selection.iter().next().and_then(|n| state.scene.node(*n)).and_then(|n| n.active_entry) else { return Err("no active stack entry".into()) };
                    state.pick_mode = None;
                    return crate::stack::execute(state, C::PickModifierNode { index: index as u32, node: id.0 });
                }
                Some(PickMode::Constraint(kind)) => {
                    state.pick_mode = None;
                    return crate::motion::pick_constraint(state, kind, id);
                }
                Some(PickMode::BipedXtraParent) => {
                    state.pick_mode = None;
                    let (root, name, reorient) = state.biped_xtra_pick.take().ok_or("no Xtra to pick a parent for")?;
                    return crate::biped::set_xtra(state, root, &name, None, None, Some(id.0), reorient);
                }
                Some(PickMode::PickMaterial) => {
                    state.pick_mode = None;
                    return crate::material::execute(state, C::GetMaterialFromObject { node: Some(id.0) });
                }
                Some(PickMode::HideByHit) => set_flags(state, "Hide by Hit", vec![id], |_, f| (true, f))?,
                Some(PickMode::FreezeByHit) => set_flags(state, "Freeze by Hit", vec![id], |h, _| (h, true))?,
                Some(PickMode::UnfreezeByHit) => set_flags(state, "Unfreeze by Hit", vec![id], |h, _| (h, false))?,
                Some(PickMode::Link) => {
                    let children: Vec<NodeId> = state.selection.iter().copied().filter(|c| *c != id).collect();
                    let mut changes = Vec::new();
                    for c in children {
                        let node = state.scene.node(c).ok_or_else(|| missing(c))?;
                        if state.scene.is_descendant(id, c) {
                            continue; // would make a cycle
                        }
                        let t_before = node.transform.clone();
                        let mut sim = state.scene.clone();
                        sim.set_parent(c, Some(id));
                        let t_after = sim.node(c).ok_or_else(|| missing(c))?.transform.clone();
                        changes.push(Change::SetTransform { id: c, before: t_before, after: t_after });
                        changes.push(Change::SetParent { id: c, before: node.parent, after: Some(id) });
                    }
                    if !changes.is_empty() {
                        commit_static(state, "Link", Change::Group(changes))?;
                    }
                    return Ok(json!({ "parent": id.0 }));
                }
                Some(PickMode::SelectByColor) => {
                    state.pick_mode = None;
                    let rgb = state.scene.node(id).ok_or_else(|| missing(id))?.wire_color;
                    return execute(state, C::SelectByColor { rgb });
                }
                Some(PickMode::CoordSystem) => {
                    state.pick_mode = None;
                    return execute(state, C::PickCoordinateSystem { id: id.0 });
                }
                Some(PickMode::AlignTarget) => {
                    state.pick_mode = None;
                    state.align_target = Some(id);
                    state.dialog = Some(Dlg::Align);
                    return Ok(json!({ "target": id.0 }));
                }
                Some(PickMode::QuickAlignTarget) => {
                    state.pick_mode = None;
                    let from = if state.selection.len() == 1 {
                        state.scene.world_matrix(*state.selection.iter().next().unwrap()).w_axis.truncate()
                    } else {
                        state.selection_pivot()
                    };
                    let to = state.scene.world_matrix(id).w_axis.truncate();
                    return transform_selection_each(state, "Quick Align", &|_, _| (Mat4::from_translation(to - from), Vec3::ZERO));
                }
                Some(PickMode::NormalAlignSource | PickMode::NormalAlignTarget | PickMode::AlignCamera | PickMode::WorkingPivotSurface) => {
                    return Err("this pick mode needs a surface pick (Pick Surface)".into());
                }
                Some(PickMode::SpacingPointA | PickMode::SpacingPointB | PickMode::CreateVertex | PickMode::WorkingPivotView | PickMode::MeasureDistanceA | PickMode::MeasureDistanceB) => {
                    return Err("this pick mode needs a point pick (Pick Point)".into());
                }
                Some(PickMode::CloneAlignDestination) => return crate::extras::pick_destination(state, id),
                Some(PickMode::SpacingPath) => {
                    state.scene.node(id).and_then(|n| n.object.shape()).ok_or("pick a shape for the path")?;
                    state.pick_mode = None;
                    state.spacing_path = Some(id);
                    state.spacing_points = [None, None];
                    return Ok(json!({ "path": id.0 }));
                }
                Some(PickMode::PlaceHighlight) => return Err("Place Highlight drags over a surface".into()),
                Some(PickMode::ProtractorObject(which)) => {
                    state.pick_mode = None;
                    let protractor = state.selection.iter().copied().find(|p| state.scene.node(*p).is_some_and(|n| matches!(n.object, ObjectKind::Protractor { .. }))).ok_or("select the protractor first")?;
                    return crate::helpers::set_protractor_object(state, protractor, which as u32, Some(id));
                }
                Some(PickMode::TargetWeldSource | PickMode::TargetWeldTarget | PickMode::InsertVertex | PickMode::EditTriangulationA | PickMode::EditTriangulationB | PickMode::CreatePolygon | PickMode::HingeEdge | PickMode::QuickSliceA | PickMode::QuickSliceB | PickMode::CutA | PickMode::CutB | PickMode::SwiftLoop | PickMode::BridgeEdge1 | PickMode::BridgeEdge2) => {
                    return Err("this pick mode needs a sub-object pick (Pick Sub)".into());
                }
                None => return Err("no pick mode active".into()),
            }
            state.selection.remove(&id);
            Ok(json!({}))
        }
        C::HideByName => open_by_name(state, DialogPurpose::Hide),
        C::UnhideByName => open_by_name(state, DialogPurpose::Unhide),
        C::FreezeByName => open_by_name(state, DialogPurpose::Freeze),
        C::UnfreezeByName => open_by_name(state, DialogPurpose::Unfreeze),
        C::ResetXForm => {
            let ids = selected_ids(state);
            let mut changes = Vec::new();
            for id in ids {
                let node = state.scene.node(id).ok_or_else(|| missing(id))?;
                if matches!(node.object, ObjectKind::Camera { .. } | ObjectKind::Light(_) | ObjectKind::Group { .. }) {
                    continue;
                }
                // As the reference: rotation and scale (and the pivot offset) go into
                // a new XForm modifier on top of the stack; collapsing bakes them.
                let t = &node.transform;
                let held = Mat4::from_scale_rotation_translation(t.scale.get(), t.rotation_quat(), Vec3::ZERO) * node.object_offset.matrix();
                let mut xform = Modifier::by_name("XForm").ok_or("no XForm modifier")?;
                if let Some(g) = xform.gizmo_mut() {
                    *g = Transform::from_matrix(held);
                }
                let mut stack = node.modifiers.clone();
                let at = stack.iter().position(|m| m.modifier.is_wsm()).unwrap_or(stack.len());
                stack.insert(at, loom_scene::ModifierSlot::new(xform));
                let mut after_t = t.clone();
                after_t.rotation.set(Vec3::ZERO);
                after_t.scale.set(Vec3::ONE);
                let mut props = node.props();
                props.object_offset = Transform::default();
                let new_world = state.scene.parent_frame(id) * after_t.matrix();
                changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: stack });
                changes.push(Change::SetNodeProps { id, before: Box::new(node.props()), after: Box::new(props) });
                changes.extend(keep_children(state, id, new_world));
                changes.push(Change::SetTransform { id, before: t.clone(), after: after_t });
            }
            if !changes.is_empty() {
                commit(state, "Reset XForm", Change::Group(changes))?;
            }
            Ok(json!({}))
        }
        C::RescaleWorldUnits { factor } => {
            if !(factor.is_finite() && factor > 0.0) {
                return Err("factor must be positive".into());
            }
            let mut changes = Vec::new();
            let ids: Vec<NodeId> = state.scene.nodes().map(|n| n.id).collect();
            for id in ids {
                let node = state.scene.node(id).ok_or_else(|| missing(id))?;
                let mut after_t = node.transform.clone();
                after_t.position.set(node.transform.position.get() * factor);
                let mut props = node.props();
                props.object_offset.position.set(node.object_offset.position.get() * factor);
                let mut mesh = node.object.mesh().into_owned();
                edit_ops::transform_mesh(&mut mesh, Mat4::from_scale(Vec3::splat(factor)))?;
                let after_obj = match &node.object {
                    ObjectKind::EditablePoly(_) => node.object.with_mesh(mesh),
                    other => {
                        let mut o = other.clone();
                        for (name, v) in other.parameters() {
                            if let loom_scene::ParamValue::Float(f) = v {
                                if !matches!(name, "Slice From" | "Slice To" | "Rotation" | "Twist" | "Hemisphere" | "FOV") {
                                    let _ = o.set_parameter(name, (f * factor) as f64);
                                }
                            }
                        }
                        o
                    }
                };
                changes.push(Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(after_obj) });
                changes.push(Change::SetNodeProps { id, before: Box::new(node.props()), after: Box::new(props) });
                changes.push(Change::SetTransform { id, before: node.transform.clone(), after: after_t });
            }
            if !changes.is_empty() {
                commit(state, "Rescale World Units", Change::Group(changes))?;
            }
            Ok(json!({ "factor": factor }))
        }
        C::Measure => {
            let ids = selected_ids(state);
            Ok(crate::helpers::measure(state, &ids))
        }
        C::MeasureNodes { ids } => {
            let ids: Vec<NodeId> = ids.into_iter().map(NodeId).collect();
            Ok(crate::helpers::measure(state, &ids))
        }
        C::MeasureCreateCenterPoint => {
            let ids = selected_ids(state);
            let r = crate::helpers::measure(state, &ids);
            let c = r["center_of_mass"].as_array().ok_or("select an object to measure")?;
            let at = [0, 1, 2].map(|i| c[i].as_f64().unwrap_or(0.0) as f32);
            execute(state, C::CreateHelper { kind: "point".into(), position: at, size: 20.0 })
        }
        C::SetWireColor { id, rgb } => {
            let id = NodeId(id);
            let before = state.scene.node(id).ok_or_else(|| missing(id))?.wire_color;
            commit(state, "Object Color", Change::SetWireColor { id, before, after: rgb })?;
            Ok(json!({}))
        }
        C::SetTransform { id, position, rotation_deg, scale } => {
            let id = NodeId(id);
            let before = state.scene.node(id).ok_or_else(|| missing(id))?.transform.clone();
            let mut after = before.clone();
            if let Some(p) = position {
                after.position.set(Vec3::from(p));
            }
            if let Some(r) = rotation_deg {
                after.rotation.set(Vec3::from(r));
            }
            if let Some(s) = scale {
                after.scale.set(Vec3::from(s));
            }
            commit(state, "Transform", Change::SetTransform { id, before, after })?;
            Ok(json!({}))
        }
        // Motion > Trajectories > Sub-Object Keys: the Move tool moves the selected path keys.
        C::MoveSelection { delta, .. } if state.trajectory_keys && !state.trajectory_selected.is_empty() => crate::motion::execute(state, C::TrajectoryMoveKeys { delta }),
        C::MoveSelection { delta, system } => {
            let saved = with_system(state, system);
            gimbal_as_parent(state);
            let vp = state.active_viewport;
            let r = transform_selection_each(state, "Move", &|st, id| (frame_for(st, vp, id).move_matrix(Vec3::from(delta)), Vec3::ZERO));
            state.coord_system = saved;
            r
        }
        C::RotateSelection { axis, degrees, system } => {
            let saved = with_system(state, system);
            let vp = state.active_viewport;
            let a = match axis {
                Axis::X => Vec3::X,
                Axis::Y => Vec3::Y,
                Axis::Z => Vec3::Z,
            };
            let centre = animate_pivot(state);
            let r = transform_selection_each(state, "Rotate", &|st, id| {
                let f = frame_for(st, vp, id);
                // Gimbal: the rotation is exactly one Euler track.
                let hint = if st.coord_system == CoordSystem::Gimbal { a * degrees } else { euler_hint(st, id, f.axes * a * degrees) };
                (f.rotate_matrix(a, degrees.to_radians()), hint)
            });
            state.center_mode = centre;
            state.coord_system = saved;
            r
        }
        C::ScaleSelection { factors, system } => {
            let saved = with_system(state, system);
            gimbal_as_parent(state);
            let vp = state.active_viewport;
            let centre = animate_pivot(state);
            let r = transform_selection_each(state, "Scale", &|st, id| (frame_for(st, vp, id).scale_matrix(Vec3::from(factors)), Vec3::ZERO));
            state.center_mode = centre;
            state.coord_system = saved;
            r
        }
        C::RotateSelectionFree { quat } => {
            let q = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]).normalize();
            let vp = state.active_viewport;
            let centre = animate_pivot(state);
            let r = transform_selection_each(state, "Rotate", &|st, id| {
                let (ax, ang) = q.to_axis_angle();
                (frame_for(st, vp, id).rotate_quat_matrix(q), euler_hint(st, id, ax * ang.to_degrees()))
            });
            state.center_mode = centre;
            r
        }
        C::TransformGizmoXConstraint => {
            state.axis_constraint = AxisConstraint::X;
            Ok(json!({ "constraint": "X" }))
        }
        C::TransformGizmoYConstraint => {
            state.axis_constraint = AxisConstraint::Y;
            Ok(json!({ "constraint": "Y" }))
        }
        C::TransformGizmoZConstraint => {
            state.axis_constraint = AxisConstraint::Z;
            Ok(json!({ "constraint": "Z" }))
        }
        C::TransformGizmoToggle => {
            state.prefs.gizmos.enabled = !state.prefs.gizmos.enabled;
            Ok(json!({ "gizmos": state.prefs.gizmos.enabled }))
        }
        C::TransformGizmoSizeUp => {
            state.prefs.gizmos.size = (state.prefs.gizmos.size + 2.0).min(100.0);
            Ok(json!({ "size": state.prefs.gizmos.size }))
        }
        C::TransformGizmoSizeDown => {
            state.prefs.gizmos.size = (state.prefs.gizmos.size - 2.0).max(1.0);
            Ok(json!({ "size": state.prefs.gizmos.size }))
        }
        C::TransformGizmoPlaneConstraintCycle => {
            state.axis_constraint = match state.axis_constraint {
                AxisConstraint::XY => AxisConstraint::YZ,
                AxisConstraint::YZ => AxisConstraint::ZX,
                _ => AxisConstraint::XY,
            };
            Ok(json!({ "constraint": format!("{:?}", state.axis_constraint) }))
        }
        C::PickCoordinateSystem { id } => {
            let id = NodeId(id);
            state.scene.node(id).ok_or_else(|| missing(id))?;
            state.pick_coord_node = Some(id);
            // The list keeps the four most recently picked objects.
            state.pick_coord_recent.retain(|r| *r != id);
            state.pick_coord_recent.insert(0, id);
            state.pick_coord_recent.truncate(4);
            state.coord_system = CoordSystem::Pick;
            if state.prefs.ref_coord_constant {
                state.coord_by_tool = [CoordSystem::Pick; 4];
            } else {
                state.coord_by_tool[tool_index(state.active_tool)] = CoordSystem::Pick;
            }
            Ok(json!({ "system": "Pick", "node": id.0 }))
        }

        C::SelectNodes { ids, op } => {
            // Edit Working Pivot locks the selection to the working pivot.
            if state.selection_locked || state.edit_working_pivot {
                return Ok(json!({ "locked": true }));
            }
            let ids: BTreeSet<NodeId> = ids
                .into_iter()
                .map(NodeId)
                .filter(|id| state.scene.node(*id).is_some_and(|n| state.is_selectable(n)))
                .map(|id| state.selection_target(id))
                .collect();
            match op {
                SelectOp::Replace => state.selection = ids,
                SelectOp::Add => state.selection.extend(ids),
                SelectOp::Subtract => state.selection.retain(|id| !ids.contains(id)),
            }
            state.sub_selection.retain_nodes(|n| state.selection.contains(&n));
            state.refresh_center_mode();
            Ok(selection_json(state))
        }
        C::SelectSub { node, ids, op } => {
            let node = resolve_node(state, node)?;
            let mode = state.selection_mode;
            if mode == SelectionMode::Object {
                return Err("not in a sub-object level".into());
            }
            if !state.selection.contains(&node) {
                return Err(format!("node {} is not selected; a sub-object level edits only the selected objects", node.0).into());
            }
            if state.scene.node(node).is_some_and(|n| n.edit_spline().is_some()) {
                return crate::spline::select_sub(state, node, &ids, op);
            }
            if state.scene.node(node).is_some_and(|n| n.edit_patch().is_some()) {
                return crate::patch_edit::select_sub(state, node, &ids, op);
            }
            if op == SelectOp::Replace {
                state.sub_selection.clear();
            }
            let mesh = state.scene.node(node).ok_or_else(|| missing(node))?.cage_mesh().into_owned();
            let loop_mode = state.loop_mode;
            let clicked = ids.clone();
            for raw in ids {
                let polygon = state.scene.node(node).and_then(|n| crate::mesh_edit::polygon_pick(state, n, &mesh, FaceId(raw)));
                let expanded: Vec<SubObjectId> = match mode {
                    // Editable Mesh > Polygon: the triangles of the clicked polygon.
                    SelectionMode::Face if polygon.is_some() => polygon.unwrap_or_default().into_iter().map(SubObjectId::Face).collect(),
                    SelectionMode::Element => element_of(&mesh, FaceId(raw)).into_iter().map(SubObjectId::Face).collect(),
                    // Loop Mode: an edge brings its whole loop (Alt takes the loop away).
                    SelectionMode::Edge if loop_mode => loom_mesh::select_ops::edge_loop(&mesh, &[HalfEdgeId(raw)]).into_iter().map(SubObjectId::Edge).collect(),
                    SelectionMode::Border => match mesh.canonical_edge_id(HalfEdgeId(raw)) {
                        Some(e) if is_border_edge(&mesh, e) => vec![SubObjectId::Edge(e)],
                        _ => vec![],
                    },
                    _ => vec![sub_id(mode, raw)],
                };
                for id in expanded {
                    if op == SelectOp::Subtract {
                        state.sub_selection.remove(node, id);
                    } else if !is_hidden(&mesh, id) {
                        state.sub_selection.insert(node, id);
                    }
                }
            }
            if loop_mode && op != SelectOp::Subtract {
                loop_mode_select(state, node, &mesh, mode, &clicked);
            }
            Ok(selection_json(state))
        }
        C::TestRoom => {
            let mut changes: Vec<Change> = state.scene.nodes().map(|n| Change::RemoveNode(n.id)).collect();
            for (i, mesh) in loom_mesh::test_room(2.5, 3.0).into_iter().enumerate() {
                let name = if i == 0 { "Floor001".to_owned() } else { format!("Wall{i:03}") };
                changes.push(Change::AddNode(Node::new(&name, ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(mesh)))));
            }
            commit(state, "Test Room", Change::Group(changes))?;
            clear_selection(state);
            state.record_view();
            let cam = &mut state.active_mut().camera;
            cam.target = Vec3::new(0.0, 0.0, 1.5);
            cam.distance = 12.0;
            cam.pitch = 1.1;
            cam.yaw = std::f32::consts::FRAC_PI_4;
            Ok(json!({ "nodes": state.scene.len() }))
        }
        C::SelectFacesFacing { node, direction, tolerance } => {
            let node = resolve_node(state, node)?;
            let dir = Vec3::from(direction).normalize_or_zero();
            let world = state.scene.world_matrix(node);
            let mesh = state.scene.node(node).ok_or_else(|| missing(node))?.cage_mesh();
            let faces: Vec<FaceId> = (0..mesh.face_count() as u32)
                .map(FaceId)
                .filter(|&f| {
                    face_normal(&mesh, f)
                        .map(|n| world.transform_vector3(n).normalize_or_zero().dot(dir) >= tolerance)
                        .unwrap_or(false)
                })
                .collect();
            drop(mesh);
            state.selection_mode = SelectionMode::Face;
            state.sub_selection.clear();
            for f in faces {
                state.sub_selection.insert(node, SubObjectId::Face(f));
            }
            Ok(selection_json(state))
        }
        C::EdgeRing => grow_edges(state, loom_mesh::select_ops::edge_ring),
        C::EdgeLoop => grow_edges(state, loom_mesh::select_ops::edge_loop),
        C::ExtrudeFaces { distance } => {
            let faces = selected_faces(state);
            if faces.is_empty() {
                return Ok(json!({}));
            }
            let mut new_sel = Vec::new();
            edit_meshes(state, "Extrude", faces, |mesh, node, ids| {
                for f in ids {
                    let cap = edit_ops::extrude_face(mesh, f, distance)?;
                    new_sel.push((node, SubObjectId::Face(cap)));
                }
                Ok(())
            })?;
            state.sub_selection.clear();
            for (n, id) in new_sel {
                state.sub_selection.insert(n, id);
            }
            Ok(selection_json(state))
        }
        C::DissolveEdges => {
            let edges = selected_edges(state);
            if edges.is_empty() {
                return Ok(json!({}));
            }
            edit_meshes(state, "Dissolve Edges", edges, |mesh, _, ids| {
                edit_ops::dissolve_edges(mesh, &ids)
            })?;
            state.sub_selection.clear();
            Ok(json!({}))
        }
        C::Collapse => {
            state.last_poly_op = Some(C::Collapse);
            let verts = selected_vertices(state);
            if verts.is_empty() {
                return Ok(json!({}));
            }
            edit_meshes(state, "Collapse", verts, |mesh, _, ids| loom_mesh::topo_ops::collapse_vertices(mesh, &ids).map(|_| ()))?;
            state.sub_selection.clear();
            Ok(json!({}))
        }
        C::ConnectEdgeRing => {
            let edges = selected_edges(state);
            if edges.is_empty() {
                return Ok(json!({}));
            }
            let mut new_sel = Vec::new();
            edit_meshes(state, "Connect", edges, |mesh, node, ids| {
                for e in edit_ops::connect_edge_ring(mesh, &ids)? {
                    new_sel.push((node, SubObjectId::Edge(e)));
                }
                Ok(())
            })?;
            state.sub_selection.clear();
            state.selection_mode = SelectionMode::Edge;
            for (n, id) in new_sel {
                state.sub_selection.insert(n, id);
            }
            Ok(selection_json(state))
        }
        C::Rename { id, name } => {
            let id = NodeId(id);
            let node = state.scene.node(id).ok_or_else(|| missing(id))?;
            let before = node.name.clone();
            // A target camera or light renames its "<name>.Target" with it.
            let target = match &node.object {
                ObjectKind::Camera { target, .. } => *target,
                ObjectKind::Light(l) => l.target,
                _ => None,
            };
            let mut changes = Vec::new();
            if let Some(t) = target.and_then(|t| state.scene.node(t)) {
                if t.name.ends_with(".Target") {
                    changes.push(Change::SetName { id: t.id, before: t.name.clone(), after: format!("{name}.Target") });
                }
            }
            changes.insert(0, Change::SetName { id, before, after: name });
            commit(state, "Rename", Change::Group(changes))?;
            Ok(json!({}))
        }
        C::SetParent { id, parent } => {
            let id = NodeId(id);
            let before = state.scene.node(id).ok_or_else(|| missing(id))?.parent;
            let after = parent.map(NodeId);
            if let Some(p) = after {
                state.scene.node(p).ok_or_else(|| missing(p))?;
            }
            // SetParent keeps world transform by rewriting the local one, so
            // record the transform too for an exact undo.
            let t_before = state.scene.node(id).ok_or_else(|| missing(id))?.transform.clone();
            let mut sim = state.scene.clone();
            sim.set_parent(id, after);
            let t_after = sim.node(id).ok_or_else(|| missing(id))?.transform.clone();
            commit_static(
                state,
                "Link",
                Change::Group(vec![
                    Change::SetTransform { id, before: t_before, after: t_after },
                    Change::SetParent { id, before, after },
                ]),
            )?;
            Ok(json!({}))
        }
        C::Orbit { yaw, pitch, distance, target, roll } => {
            state.record_view_as("Orbit");
            let cam = &mut state.active_mut().camera;
            if let Some(r) = roll {
                cam.roll = loom_input::camera_control::wrap_angle(r.to_radians());
            }
            if let Some(y) = yaw {
                cam.yaw = y.to_radians();
            }
            if let Some(p) = pitch {
                cam.pitch = p.to_radians().clamp(-1.55, 1.55);
            }
            if let Some(d) = distance {
                cam.distance = d.max(0.05);
            }
            if let Some(t) = target {
                cam.target = Vec3::from(t);
            }
            Ok(camera_json(state))
        }
        C::Hotkey { chord } => {
            let parsed = crate::keymap::parse_chord(&chord)?;
            let name = state.hotkey(parsed).ok_or_else(|| format!("`{chord}` is not bound"))?;
            let r = execute_json(state, &json!({ "cmd": name }));
            if r["ok"] == Value::Bool(false) {
                return Err(r["error"].as_str().unwrap_or("hotkey failed").to_owned());
            }
            Ok(json!({ "action": name, "result": r }))
        }
        C::Overlays { edges, vertices } => {
            if let Some(e) = edges {
                state.active_mut().edged_faces = e;
            }
            if let Some(v) = vertices {
                state.show_vertices = v;
            }
            Ok(json!({ "edges": state.active().edged_faces, "vertices": state.show_vertices }))
        }
        c @ (C::CreateBiped { .. } | C::SetBipedStructure { .. } | C::BipedFigureMode { .. } | C::BipedRootName { .. } | C::BipedCreateXtra { .. } | C::BipedDeleteXtra { .. } | C::BipedCreateOppositeXtra { .. } | C::BipedSetXtra { .. } | C::BipedPickXtraParent { .. } | C::BipedSaveFile { .. } | C::BipedLoadFile { .. }) => crate::biped::execute(state, c),
        c @ (C::BipedFootstepMode { .. } | C::BipedFootstepCreation { .. } | C::BipedCreateMultipleFootsteps { .. } | C::BipedAppendFootstep { .. } | C::BipedActivateFootsteps | C::BipedDeactivateFootsteps | C::BipedDeleteFootsteps) => crate::biped_footsteps::execute(state, c),
        c @ (C::CreateLoomRig { .. } | C::SetLoomRigMapping { .. } | C::SetLoomRigLimb { .. } | C::RemoveLoomRig { .. } | C::CreateBoneChain { .. } | C::BoneTools | C::BoneOptions | C::BoneEditModeToggle | C::RefineBone { .. } | C::MirrorBones { .. } | C::ReassignRoot { .. } | C::DeleteBone { .. } | C::ConnectBones { .. } | C::CreateEndBone { .. } | C::SetBoneFins { .. } | C::CreateHelper { .. } | C::Dummy | C::HistoryIndependentIkSolver | C::IkLimbSolver | C::SplineIkSolver | C::BonesIkChain | C::IkSolverToggle | C::IkChainFkSnap | C::IkChainIkSnap | C::IkChainSnapAction | C::IkTerminatorToggle | C::CreateIkChain { .. } | C::SetIkChainParameter { .. } | C::PickSplineIkShape { .. }) => crate::rig::execute(state, c),
        c @ (C::CreateTape { .. } | C::HelperReadout { .. } | C::ProtractorPickObject { .. } | C::SetProtractorObject { .. } | C::ActivateGridObject | C::ActivateHomeGrid | C::MeasureDistance | C::AlignGridToView | C::NudgeGridUp | C::NudgeGridDown) => crate::helpers::execute(state, c),
        c @ (C::SnapshotDialog | C::Snapshot { .. } | C::CloneAndAlignDialog | C::CloneAndAlignPick | C::CloneAndAlignSetDestinations { .. } | C::CloneAndAlign { .. } | C::PlaceHighlight | C::PlaceHighlightAt { .. }) => crate::extras::execute(state, c),
        // Every remaining variant is an Unwrap UVW group command.
        other => crate::unwrap::execute(state, other),
    }
}

/// Runs a JSON `{"cmd": name, "args": {...}}` body and always returns a
/// JSON object with `ok`.
pub fn execute_json(state: &mut AppState, body: &Value) -> Value {
    // Unit commands reject an `args` value, but callers (MCP) often send
    // `{}` or null; strip those so `{"cmd":"State","args":{}}` works.
    let mut body = body.clone();
    if let Some(obj) = body.as_object_mut() {
        let name = obj.get("cmd").and_then(Value::as_str).unwrap_or("");
        let info = crate::command::registry().iter().find(|i| i.name == name);
        let unit = info.is_some_and(|i| i.args_schema["properties"].as_object().is_some_and(|p| p.is_empty()));
        let empty = matches!(obj.get("args"), Some(Value::Null) | None)
            || obj.get("args").and_then(Value::as_object).is_some_and(|a| a.is_empty());
        if unit && empty {
            obj.remove("args");
        } else if !unit && empty {
            obj.insert("args".into(), json!({}));
        }
    }
    let cmd: Command = match serde_json::from_value(body) {
        Ok(c) => c,
        Err(e) => return json!({ "ok": false, "error": format!("bad command: {e}") }),
    };
    match execute(state, cmd) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("ok".into(), Value::Bool(true));
                v
            } else {
                json!({ "ok": true, "result": v })
            }
        }
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

// ---------------------------------------------------------------- helpers

/// The change that replaces the poly the user edits: the active Edit Poly
/// entry's data, else the base object.
pub(crate) fn replace_poly_change(node: &Node, poly: loom_scene::PolyObject) -> Change {
    match node.editing_entry() {
        Some(a) => {
            let mut after = node.modifiers.clone();
            after[a].modifier.set_edit_poly(poly);
            Change::SetStack { id: node.id, before: node.modifiers.clone(), after }
        }
        None => {
            let after = match &node.object {
                // An Editable Mesh stays triangles (cut again, edges keep their visibility).
                ObjectKind::EditableMesh(old) => {
                    let mesh = loom_mesh::tri_mesh::retriangulate_like(&old.mesh, poly.mesh.clone()).unwrap_or_else(|_| poly.mesh.clone());
                    ObjectKind::EditableMesh(poly.with_mesh(mesh))
                }
                _ => ObjectKind::EditablePoly(poly),
            };
            Change::ReplaceObject { id: node.id, before: Box::new(node.object.clone()), after: Box::new(after) }
        }
    }
}

/// The object with an edited cage mesh: an Editable Spline takes the vertex
/// positions back as its knots, anything else becomes / stays a poly.
pub(crate) fn object_with_cage(before: &ObjectKind, mesh: Mesh) -> ObjectKind {
    match before.shape() {
        Some(mut shape) if before.spline().is_some() => {
            let pts: Vec<Vec3> = mesh.vertices().iter().map(|v| v.position).collect();
            shape.set_knot_positions(&pts);
            shape.apply_binds();
            before.with_shape(shape)
        }
        _ => before.with_mesh(mesh),
    }
}

/// The change for an edited cage mesh of `node`: Edit Spline / Edit Poly
/// entries take it in place, else the object is replaced.
pub(crate) fn cage_change(node: &Node, mesh: Mesh) -> Change {
    if let Some(p) = node.edit_patch() {
        return crate::patch_edit::replace_patch_change(node, p.with_patch(p.patch.with_cage(&mesh)));
    }
    if let Some(so) = node.edit_spline().filter(|_| node.editing_spline_entry().is_some()) {
        let mut shape = so.shape.clone();
        let pts: Vec<Vec3> = mesh.vertices().iter().map(|v| v.position).collect();
        shape.set_knot_positions(&pts);
        shape.apply_binds();
        return replace_shape_change(node, shape);
    }
    if let Some(p) = node.edit_poly().filter(|_| node.editing_entry().is_some()) {
        return replace_poly_change(node, p.with_mesh(mesh));
    }
    Change::ReplaceObject { id: node.id, before: Box::new(node.object.clone()), after: Box::new(object_with_cage(&node.object, mesh)) }
}

/// Adds a new object node at a world position, selected, as the Create
/// commands do; returns its id.
pub(crate) fn add_object(state: &mut AppState, object: ObjectKind, position: Vec3) -> Result<NodeId, String> {
    let mut node = Node::new(&state.scene.next_name(object.base_name()), object);
    node.wire_color = state.display.new_object_color(state.scene.next_id());
    node.transform.position.set(position);
    let name = node.name.clone();
    let id = NodeId(state.scene.next_id());
    commit(state, &format!("Create {name}"), Change::AddNode(node))?;
    state.selection = BTreeSet::from([id]);
    state.sub_selection.clear();
    state.selection_mode = SelectionMode::Object;
    state.active_tool = Tool::Select;
    Ok(id)
}

/// Edits the first spline of a Line being drawn; `point` (world) arrives in object space.
fn edit_line(state: &mut AppState, id: NodeId, label: &str, f: impl FnOnce(&mut loom_shape::Spline, Vec3) -> Result<(), String>, point: Option<Vec3>) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let Some(so) = node.object.spline() else { return Err("not a Line".into()) };
    let local = point.map(|p| state.scene.object_matrix(id).inverse().transform_point3(p)).unwrap_or_default();
    let mut shape = so.shape.clone();
    let spline = shape.splines.last_mut().ok_or("the line has no spline")?;
    f(spline, local)?;
    let knots = spline.knots.len();
    let change = Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(node.object.with_shape(shape)) };
    commit(state, label, change)?;
    Ok(json!({ "knots": knots }))
}

fn toggle_pick_mode(state: &mut AppState, mode: PickMode) -> Result<Value, String> {
    state.pick_mode = if state.pick_mode == Some(mode) { None } else { Some(mode) };
    Ok(json!({ "pick": state.pick_mode.map(|m| format!("{m:?}")) }))
}

/// Boolean: the selected object A and the picked B (in A's space) become a
/// Boolean compound; A's stack collapses into operand A.
fn compound_boolean(state: &mut AppState, b: NodeId, operation: u32, move_operand: bool) -> Result<Value, String> {
    let id = state.selection.iter().next().copied().ok_or("select operand A first")?;
    if b == id {
        return Err("pick another object as operand B".into());
    }
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let bnode = state.scene.node(b).ok_or_else(|| missing(b))?;
    if node.object.is_shape() || bnode.object.is_shape() || matches!(node.object, ObjectKind::Camera { .. } | ObjectKind::Light(_) | ObjectKind::Group { .. }) {
        return Err("Boolean needs two mesh objects".into());
    }
    let a_mesh = state.scene.eval_mesh(id).value.mesh.clone();
    let mut b_mesh = state.scene.eval_mesh(b).value.mesh.clone();
    let to_local = state.scene.object_matrix(id).inverse() * state.scene.object_matrix(b);
    edit_ops::transform_mesh(&mut b_mesh, to_local)?;
    let mut changes = vec![Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(ObjectKind::boolean_object(a_mesh, b_mesh, operation)) }];
    if !node.modifiers.is_empty() {
        changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: Vec::new() });
    }
    if move_operand {
        changes.push(Change::RemoveNode(b));
    }
    commit(state, "Boolean", Change::Group(changes))?;
    state.selection = BTreeSet::from([id]);
    state.pick_mode = None;
    Ok(json!({ "id": id.0, "error": state.scene.node(id).and_then(|n| n.object.compound_error()) }))
}

/// The change that replaces the shape the user edits: the active Edit
/// Spline entry's data, else the object (which becomes an Editable Spline).
pub(crate) fn replace_shape_change(node: &Node, shape: loom_shape::Shape) -> Change {
    match node.editing_spline_entry() {
        Some(a) => {
            let mut after = node.modifiers.clone();
            if let Some(so) = after[a].modifier.edit_spline_mut() {
                so.shape = shape;
            }
            Change::SetStack { id: node.id, before: node.modifiers.clone(), after }
        }
        None => Change::ReplaceObject { id: node.id, before: Box::new(node.object.clone()), after: Box::new(node.object.with_shape(shape)) },
    }
}

pub(crate) fn commit_change(state: &mut AppState, label: &str, change: Change) -> Result<(), String> {
    commit(state, label, change)
}

pub(crate) fn commit(state: &mut AppState, label: &str, change: Change) -> Result<(), String> {
    crate::xref::guard(&state.scene, &change)?;
    let change = share_with_instances(&state.scene, change);
    state.undo.commit(label, change, &mut state.scene)?;
    state.mesh_dirty = true;
    Ok(())
}

/// Instances share one object: a replaced object (a sub-object edit, a
/// parameter, an attach) is replaced on every member of the node's instance
/// group that the change does not already cover.
fn share_with_instances(scene: &loom_scene::Scene, change: Change) -> Change {
    fn replaced<'a>(c: &'a Change, out: &mut Vec<(NodeId, &'a ObjectKind)>, removed: &mut BTreeSet<NodeId>) {
        match c {
            Change::ReplaceObject { id, after, .. } => out.push((*id, &**after)),
            Change::RemoveNode(id) => {
                removed.insert(*id);
            }
            Change::Group(cs) => cs.iter().for_each(|c| replaced(c, out, removed)),
            _ => {}
        }
    }
    let mut objects = Vec::new();
    let mut done = BTreeSet::new();
    replaced(&change, &mut objects, &mut done);
    if objects.is_empty() {
        return change;
    }
    done.extend(objects.iter().map(|(id, _)| *id));
    let mut extra = Vec::new();
    for (id, after) in objects {
        let Some(node) = scene.node(id) else { continue };
        let root = node.instance_of.unwrap_or(id);
        for member in scene.nodes().filter(|n| n.id == root || n.instance_of == Some(root)) {
            if done.insert(member.id) {
                extra.push(Change::ReplaceObject { id: member.id, before: Box::new(member.object.clone()), after: Box::new(after.clone()) });
            }
        }
    }
    if extra.is_empty() {
        return change;
    }
    extra.insert(0, change);
    Change::Group(extra)
}

/// Commits a structural edit (link, unlink, group) that only re-expresses a
/// node's transform in a new frame: never keyed, even under Auto Key; an
/// animated node's keys are offset instead.
pub(crate) fn commit_static(state: &mut AppState, label: &str, change: Change) -> Result<(), String> {
    let animate = loom_scene::anim::animate();
    loom_scene::anim::set_animate(false);
    let r = commit(state, label, change);
    loom_scene::anim::set_animate(animate);
    r
}

/// Spline Select: set the stored selection of the selected node's Spline Select entry.
fn spline_select_set(state: &mut AppState, level: u32, ids: Vec<u32>, op: SelectOp) -> Result<Value, String> {
    if !(1..=3).contains(&level) {
        return Err("level is 1 Vertex, 2 Segment or 3 Spline".into());
    }
    let (id, index) = state
        .selection
        .iter()
        .find_map(|id| {
            let n = state.scene.node(*id)?;
            let holds = |m: &Modifier| matches!(m, Modifier::SplineSelect { .. } | Modifier::FilletChamfer(_));
            let i = n.active_entry.filter(|a| n.modifiers.get(*a).is_some_and(|s| holds(&s.modifier))).or_else(|| n.modifiers.iter().rposition(|s| holds(&s.modifier)))?;
            Some((*id, i))
        })
        .ok_or("select an object with a Spline Select modifier")?;
    let n = state.scene.node(id).unwrap();
    let knots = match index {
        0 => n.object.shape(),
        i => state.scene.evaluated(id, Some(i - 1)).value.shape.clone(),
    }
    .map(|s| s.knot_count())
    .ok_or("Spline Select needs a shape below it")?;
    let before = n.modifiers.clone();
    let mut after = before.clone();
    if let Modifier::FilletChamfer(d) = &mut after[index].modifier {
        if level != 1 {
            return Err("Fillet/Chamfer selects vertices (level 1)".into());
        }
        let mut set: BTreeSet<u32> = d.ids.iter().copied().collect();
        match op {
            SelectOp::Replace => set = ids.iter().copied().collect(),
            SelectOp::Add => set.extend(ids.iter().copied()),
            SelectOp::Subtract => {
                for i in &ids {
                    set.remove(i);
                }
            }
        }
        d.ids = set.into_iter().collect();
    }
    if let Modifier::SplineSelect { level: l, ids: cur, knots: k } = &mut after[index].modifier {
        let mut set: BTreeSet<u32> = if *l == level { cur.iter().copied().collect() } else { BTreeSet::new() };
        match op {
            SelectOp::Replace => set = ids.into_iter().collect(),
            SelectOp::Add => set.extend(ids),
            SelectOp::Subtract => {
                for i in ids {
                    set.remove(&i);
                }
            }
        }
        *l = level;
        *cur = set.into_iter().collect();
        *k = knots;
    }
    after[index].sub_level = level;
    let stored = match &after[index].modifier {
        Modifier::SplineSelect { ids, .. } => ids.clone(),
        Modifier::FilletChamfer(d) => d.ids.clone(),
        _ => Vec::new(),
    };
    commit(state, "Spline Select", Change::SetStack { id, before, after })?;
    // The viewport selection mirrors the stored one (sync_selection reads it back).
    state.selection_mode = match level {
        1 => SelectionMode::Vertex,
        2 => SelectionMode::Edge,
        _ => SelectionMode::Face,
    };
    state.sub_selection.clear();
    for raw in stored {
        state.sub_selection.insert(id, crate::poly::sub(state.selection_mode, raw));
    }
    if let Some(n) = state.scene.node_mut(id) {
        n.active_entry = Some(index);
    }
    Ok(json!({ "level": level }))
}

/// Commits controllers exactly as given (no Auto Key merge onto the node's).
pub(crate) fn commit_exact(state: &mut AppState, label: &str, change: Change) -> Result<(), String> {
    state.undo.commit_exact(label, change, &mut state.scene)?;
    state.mesh_dirty = true;
    Ok(())
}

fn missing(id: NodeId) -> String {
    format!("no node {}", id.0)
}

/// Replaces the scene and settings with a loaded document (Open / Fetch):
/// not undoable.
pub(crate) fn load_document(state: &mut AppState, doc: crate::file::Document, file: Option<std::path::PathBuf>) {
    state.scene = doc.scene;
    // Node versions restart with a loaded scene: cached track bar keys belong to the old one.
    state.key_time_cache = Default::default();
    state.undo = loom_scene::UndoStack::new(state.undo.limit());
    state.autosave.last_version = None;
    clear_selection(state);
    doc.settings.apply_to(state);
    prune_camera_views(state);
    state.pick_mode = None;
    state.dialog = None;
    state.file = file;
    state.mesh_dirty = true;
}

/// Export by extension: `.fbx` → FBX, anything else → OBJ.
fn export_any(state: &AppState, ids: &[NodeId], path: &std::path::Path) -> Result<Value, String> {
    if crate::fbx_io::is_fbx(path) {
        crate::fbx_io::export(state, ids, path)
    } else if crate::geom_io::is_3ds(path) {
        crate::geom_io::export_3ds(state, ids, path)
    } else if crate::geom_io::is_stl(path) {
        crate::geom_io::export_stl(state, ids, path)
    } else if crate::geom_io::is_dxf(path) {
        crate::geom_io::export_dxf(state, ids, path)
    } else {
        crate::obj_io::export(state, ids, path)
    }
}

/// `Box` → `Box001`, `Box002`… (trailing digits replaced), the first not in `taken`.
pub(crate) fn unique_name(name: &str, taken: &std::collections::BTreeSet<String>) -> String {
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let base = if base.is_empty() { name } else { base };
    (1..).map(|n| format!("{base}{n:03}")).find(|c| !taken.contains(c)).expect("a free name exists")
}

fn save_to(state: &AppState, path: &std::path::Path) -> Result<(), String> {
    let doc = crate::file::Document {
        format: crate::file::FORMAT,
        app: format!("3dloom {}", env!("CARGO_PKG_VERSION")),
        scene: state.scene.clone(),
        settings: crate::file::SceneSettings::from_state(state),
    };
    crate::file::write(path, &doc)
}

/// The New / Reset / Open / Exit behind the Save Changes prompt.
pub fn file_action(state: &mut AppState, action: crate::viewport::PendingFileAction) -> Result<Value, String> {
    use crate::viewport::PendingFileAction as A;
    use Command as C;
    match action {
        A::New => execute(state, C::NewScene),
        A::Reset => {
            // After the save prompt, the reference asks once more.
            state.dialog = Some(crate::viewport::Dialog::ConfirmReset);
            Ok(json!({ "dialog": "ConfirmReset" }))
        }
        A::NewKeeping(keep) => execute(state, C::NewSceneKeep { keep: keep.arg().into() }),
        A::Open => execute(state, C::OpenFile { path: None }),
        A::OpenRecent(i) => {
            let path = state.recent_files.get(i).ok_or_else(|| format!("no recent file {}", i + 1))?.to_string_lossy().into_owned();
            execute(state, C::OpenFile { path: Some(path) })
        }
        A::Exit => {
            state.exit_requested = true;
            Ok(json!({ "exit": true }))
        }
    }
}

/// After a successful save: the action the Save Changes prompt deferred.
fn run_pending(state: &mut AppState) -> Result<(), String> {
    if let Some(action) = state.pending_file_action.take() {
        file_action(state, action)?;
    }
    Ok(())
}

/// Save Incremental: `a.loom` → `a01.loom`, `a01.loom` → `a02.loom` (first free number).
fn incremental_path(current: &std::path::Path) -> std::path::PathBuf {
    let stem = current.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = current.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "loom".into());
    let base = {
        let trimmed = stem.trim_end_matches(|c: char| c.is_ascii_digit());
        if trimmed.is_empty() { stem.clone() } else { trimmed.to_owned() }
    };
    let dir = current.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    (1..)
        .map(|n| dir.join(format!("{base}{n:02}.{ext}")))
        .find(|p| !p.exists())
        .expect("a free number exists")
}

fn clear_selection(state: &mut AppState) {
    state.selection.clear();
    state.sub_selection.clear();
}

/// Drops selection entries whose nodes no longer exist (after undo/redo).
fn prune_selection(state: &mut AppState) {
    let scene = &state.scene;
    state.selection.retain(|id| scene.node(*id).is_some());
    // A step without a recorded selection (an interactive drag) keeps the
    // sub-object ids that still name an element of their mesh.
    crate::sel_undo::keep_valid(state);
    prune_camera_views(state);
}

fn tool_index(tool: Tool) -> usize {
    match tool {
        Tool::Select => 0,
        Tool::Move => 1,
        Tool::Rotate => 2,
        Tool::Scale => 3,
    }
}

fn set_tool(state: &mut AppState, tool: Tool) -> Result<Value, String> {
    let old = tool_index(state.active_tool);
    state.active_tool = tool;
    // The transform managers (coordinate system, centre, axis constraint)
    // are remembered per transform unless Preferences > Reference
    // Coordinate System > Constant is on.
    if !state.prefs.ref_coord_constant {
        state.center_by_tool[old] = (state.center_single, state.center_multi);
        let level = state.constraint_level_sub as usize;
        state.constraint_by_tool[level][old] = state.axis_constraint;
        let i = tool_index(tool);
        state.coord_system = state.coord_by_tool[i];
        (state.center_single, state.center_multi) = state.center_by_tool[i];
        state.axis_constraint = state.constraint_by_tool[level][i];
        state.refresh_center_mode();
    }
    Ok(json!({ "tool": tool.label() }))
}

/// The Keyboard panel's Save / Load file dialog returns to the panel.
fn back_to_keyboard_panel(state: &mut AppState) {
    if matches!(state.dialog, Some(Dlg::SaveShortcuts | Dlg::LoadShortcuts)) {
        state.dialog = Some(Dlg::CustomizeUI);
    }
}

/// Writes a settings-style text file, making its folder first.
fn write_text(path: &str, text: &str) -> Result<(), String> {
    let path = std::path::Path::new(path);
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn toggle(flag: &mut bool, name: &str) -> Result<Value, String> {
    *flag = !*flag;
    Ok(json!({ name: *flag }))
}

pub(crate) fn set_frame(state: &mut AppState, frame: i32) -> Result<Value, String> {
    let frame = frame.clamp(state.frame_range.0, state.frame_range.1);
    // Set Key mode: a pose not keyed yet is dropped when the frame changes.
    if state.set_key_mode && frame != state.current_frame {
        crate::anim_cmds::discard_pose(state);
    }
    // The viewports show the scene at the new frame.
    if frame != state.current_frame {
        state.mesh_dirty = true;
    }
    state.current_frame = frame;
    state.scene.set_time(frame as f32);
    if state.set_key_mode {
        crate::anim_cmds::start_pose(state);
    }
    Ok(json!({ "frame": state.current_frame }))
}

fn set_view(state: &mut AppState, kind: ViewKind) -> Result<Value, String> {
    state.record_view();
    let vp = state.active_mut();
    vp.kind = kind;
    if let Some((yaw, pitch)) = kind.angles() {
        vp.camera.yaw = yaw;
        vp.camera.pitch = pitch;
        vp.camera.roll = 0.0;
    }
    Ok(camera_json(state))
}

pub(crate) fn frame(state: &mut AppState, bounds: Option<(Vec3, Vec3)>) -> Result<Value, String> {
    let Some((min, max)) = bounds else {
        return Ok(camera_json(state));
    };
    state.record_view_as("Zoom Extents");
    let fov = state.active().fov_y_radians;
    let radius = (max - min).length().max(0.25) * 0.5;
    let cam = &mut state.active_mut().camera;
    cam.target = (min + max) * 0.5;
    cam.distance = (radius / (fov * 0.5).tan()).max(0.5) * 1.6;
    Ok(camera_json(state))
}

/// Zoom Extents All: frame the bounds in every viewport.
fn frame_all(state: &mut AppState, bounds: Option<(Vec3, Vec3)>) -> Result<Value, String> {
    let active = state.active_viewport;
    for i in 0..state.viewports.len() {
        state.active_viewport = i;
        frame(state, bounds)?;
    }
    state.active_viewport = active;
    Ok(camera_json(state))
}

fn zoom(state: &mut AppState, factor: f32) -> Result<Value, String> {
    state.record_view_as("Zoom");
    let cam = &mut state.active_mut().camera;
    cam.distance = (cam.distance * factor).max(0.05);
    Ok(camera_json(state))
}

/// Lock Zoom/Pan on (anchored to the viewport's current view) or off.
fn set_background_lock(vp: &mut Viewport, on: bool) {
    vp.background_locked = on;
    vp.background_anchor = on.then_some((vp.camera.target, vp.camera.distance));
}

fn nav_toggle(state: &mut AppState, mode: NavMode) -> Result<Value, String> {
    state.nav_mode = if state.nav_mode == mode { NavMode::None } else { mode };
    Ok(json!({ "nav_mode": format!("{:?}", state.nav_mode) }))
}

fn exit_isolation(state: &mut AppState) -> Result<Value, String> {
    let Some(ids) = state.isolation.take() else {
        return Ok(json!({}));
    };
    set_flags(state, "Exit Isolation", ids, |_, f| (false, f))?;
    Ok(json!({}))
}

fn set_flags(
    state: &mut AppState,
    label: &str,
    ids: Vec<NodeId>,
    f: impl Fn(bool, bool) -> (bool, bool),
) -> Result<(), String> {
    let mut changes = Vec::new();
    for id in ids {
        let Some(n) = state.scene.node(id) else { continue };
        let before = (n.hidden, n.frozen);
        let after = f(n.hidden, n.frozen);
        if before != after {
            changes.push(Change::SetFlags { id, before, after });
        }
    }
    if changes.is_empty() {
        return Ok(());
    }
    commit(state, label, Change::Group(changes))
}

/// The ids plus every descendant of the group heads among them (closed
/// groups act as one object for Delete / Clone).
fn with_group_members(state: &AppState, ids: Vec<NodeId>) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = Vec::new();
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
        if state.scene.node(id).is_some_and(|n| matches!(n.object, ObjectKind::Group { .. })) {
            for d in state.scene.descendants(id) {
                if !out.contains(&d) {
                    out.push(d);
                }
            }
        }
    }
    out
}

/// Spacing Tool > Apply along a picked shape (every spline) or the line
/// between two picked points, by Distribution mode.
#[allow(clippy::too_many_arguments)]
fn space_along_path(state: &mut AppState, path: Option<NodeId>, start: Vec3, end: Vec3, mode: u32, count: u32, spacing: f32, start_offset: f32, end_offset: f32, context: SpacingContext, follow: bool, kind: CloneKind) -> Result<Value, String> {
    use crate::spacing::{distances, straight, PathWalk, DISTRIBUTIONS};
    if mode as usize >= DISTRIBUTIONS.len() {
        return Err(format!("distribution is 0..{}", DISTRIBUTIONS.len() - 1));
    }
    let ids: Vec<NodeId> = state.selection.iter().copied().filter(|id| Some(*id) != path).collect();
    if ids.is_empty() {
        return Err("select the object(s) to space first".into());
    }
    let walks: Vec<PathWalk> = match path {
        Some(p) => {
            let node = state.scene.node(p).ok_or_else(|| missing(p))?;
            let shape = node.object.shape().ok_or("the path is not a shape")?.transformed(state.scene.object_matrix(p));
            shape
                .splines
                .iter()
                .map(|sp| {
                    // Fine steps: the tangents (Follow) come from the polyline.
                    let mut pts = sp.polyline(shape.steps.max(16), false, false);
                    if sp.closed {
                        if let Some(f) = pts.first().copied() {
                            pts.push(f);
                        }
                    }
                    PathWalk::new(pts)
                })
                .collect()
        }
        None => vec![PathWalk::new(vec![start, end])],
    };
    // Edges: the selection's size along the path's overall direction.
    let extent_along = |dir: Vec3| -> f32 {
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for id in &ids {
            if let Some((mn, mx)) = state.object_bounds(*id) {
                for i in 0..8 {
                    let p = Vec3::new(if i & 1 == 0 { mn.x } else { mx.x }, if i & 2 == 0 { mn.y } else { mx.y }, if i & 4 == 0 { mn.z } else { mx.z });
                    lo = lo.min(p.dot(dir));
                    hi = hi.max(p.dot(dir));
                }
            }
        }
        if lo.is_finite() { hi - lo } else { 0.0 }
    };
    let mut targets: Vec<(Vec3, Vec3)> = Vec::new();
    for w in &walks {
        let (a, _) = w.sample(0.0);
        let (b, _) = w.sample(w.length());
        let dir = (b - a).normalize_or(Vec3::X);
        let extent = if context == SpacingContext::Edges { extent_along(dir) } else { 0.0 };
        for d in distances(mode, w.length(), count, spacing, start_offset, end_offset, extent) {
            targets.push(if straight(mode) { (a + dir * d, dir) } else { w.sample(d) });
        }
    }
    let pivot0 = if ids.len() == 1 { state.scene.world_matrix(ids[0]).w_axis.truncate() } else { state.selection_pivot() };
    let (_, q0, _) = state.scene.world_matrix(ids[0]).to_scale_rotation_translation();
    state.undo.begin("Spacing Tool");
    let mut created = Vec::new();
    let r: Result<(), String> = (|| {
        for (target, tangent) in &targets {
            execute(state, Command::SelectNodes { ids: ids.iter().map(|n| n.0).collect(), op: SelectOp::Replace })?;
            execute(state, Command::CloneOptions { kind, copies: 1, name: None, offset: None, rotate_deg: None, scale: None })?;
            let rot = if follow { Quat::from_rotation_arc((q0 * Vec3::X).normalize_or(Vec3::X), *tangent) } else { Quat::IDENTITY };
            let m = Mat4::from_translation(*target) * Mat4::from_quat(rot) * Mat4::from_translation(-pivot0);
            let hint = rot.to_axis_angle();
            transform_selection_each(state, "Spacing Tool", &|st, id| (m, euler_hint(st, id, hint.0 * hint.1.to_degrees())))?;
            created.extend(state.selection.iter().copied());
        }
        Ok(())
    })();
    state.undo.end();
    r?;
    state.selection = created.iter().copied().collect();
    state.refresh_center_mode();
    Ok(json!({ "created": created.len() }))
}

fn base_of(name: &str) -> &str {
    name.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// Loop Mode at Vertex / Polygon level: a clicked vertex or polygon next to
/// an already selected one brings the loop the two of them run along.
fn loop_mode_select(state: &mut AppState, node: NodeId, mesh: &Mesh, mode: SelectionMode, clicked: &[u32]) {
    use loom_mesh::select_ops::{edge_loop, edge_ring, vertex_edges};
    let mut add: Vec<SubObjectId> = Vec::new();
    for &raw in clicked {
        match mode {
            SelectionMode::Vertex => {
                let v = VertexId(raw);
                for e in vertex_edges(mesh, v) {
                    let Some((a, b)) = mesh.edge_vertices(e) else { continue };
                    let other = if a == v { b } else { a };
                    if !state.sub_selection.contains(node, SubObjectId::Vertex(other)) {
                        continue;
                    }
                    for l in edge_loop(mesh, &[e]) {
                        if let Some((x, y)) = mesh.edge_vertices(l) {
                            add.extend([SubObjectId::Vertex(x), SubObjectId::Vertex(y)]);
                        }
                    }
                }
            }
            SelectionMode::Face => {
                let hes = mesh.half_edges();
                for c in mesh.face_corners(FaceId(raw)).unwrap_or_default() {
                    let Some(g) = hes[hes[c.0 as usize].twin.0 as usize].face else { continue };
                    if !state.sub_selection.contains(node, SubObjectId::Face(g)) {
                        continue;
                    }
                    // The polygons between the ring edges of the shared edge.
                    for r in edge_ring(mesh, &[c]) {
                        let h = &hes[r.0 as usize];
                        add.extend(h.face.into_iter().chain(hes[h.twin.0 as usize].face).map(SubObjectId::Face));
                    }
                }
            }
            _ => {}
        }
    }
    for id in add {
        if !is_hidden(mesh, id) {
            state.sub_selection.insert(node, id);
        }
    }
}

/// Delete. Deleting polygons that would leave vertices with no polygon asks
/// first (`delete_isolated` None opens the prompt); the answer comes back as
/// Delete Polygons.
fn delete(state: &mut AppState, delete_isolated: Option<bool>) -> Result<Value, String> {
    if state.selection_mode != SelectionMode::Object && state.sub_selection.is_empty() {
        return Ok(json!({}));
    }
    if state.selection_mode == SelectionMode::Object {
        let ids: Vec<NodeId> = with_group_members(state, state.selection.iter().copied().collect());
        // Deleting either end of a tape deletes the tape.
        let ids = crate::helpers::with_tape_partners(state, ids);
        if ids.is_empty() {
            return Ok(json!({}));
        }
        // Children first so a removed head never reparents members that go too.
        let mut ids = ids;
        ids.sort_by_key(|id| std::cmp::Reverse(state.scene.depth(*id)));
        commit(
            state,
            "Delete",
            Change::Group(ids.into_iter().map(Change::RemoveNode).collect()),
        )?;
        clear_selection(state);
        prune_camera_views(state);
        return Ok(json!({}));
    }
    if state.spline_selected() {
        return crate::spline::delete_selection(state);
    }
    if state.patch_selected() {
        return crate::patch_edit::delete_selection(state);
    }
    let mut faces: BTreeMap<NodeId, Vec<FaceId>> = BTreeMap::new();
    let mut edges: BTreeMap<NodeId, Vec<HalfEdgeId>> = BTreeMap::new();
    let mut verts: BTreeMap<NodeId, Vec<VertexId>> = BTreeMap::new();
    for (n, id) in state.sub_selection.iter() {
        match id {
            SubObjectId::Face(f) => faces.entry(n).or_default().push(f),
            SubObjectId::Edge(e) => edges.entry(n).or_default().push(e),
            SubObjectId::Vertex(v) => verts.entry(n).or_default().push(v),
        }
    }
    let nodes: BTreeSet<NodeId> = faces.keys().chain(edges.keys()).chain(verts.keys()).copied().collect();
    if delete_isolated.is_none() && edges.is_empty() && verts.is_empty() {
        let leaves_isolated = faces.iter().any(|(n, fs)| {
            let Some(node) = state.scene.node(*n) else { return false };
            let mesh = node.cage_mesh();
            let set: BTreeSet<FaceId> = fs.iter().copied().collect();
            fs.iter().flat_map(|f| mesh.face_vertex_ids(*f).unwrap_or_default()).any(|v| mesh.faces_around_vertex(v).iter().all(|g| set.contains(g)))
        });
        if leaves_isolated {
            state.dialog = Some(Dialog::DeleteIsolatedVertices);
            return Ok(json!({ "ask": "Delete isolated vertices?" }));
        }
    }
    let mut changes = Vec::new();
    for n in nodes {
        let node = state.scene.node(n).ok_or_else(|| missing(n))?;
        let before = node.object.clone();
        let mut mesh = node.cage_mesh().into_owned();
        if let Some(f) = faces.get(&n) {
            if delete_isolated == Some(false) {
                edit_ops::delete_faces_keep_vertices(&mut mesh, f)?;
            } else {
                edit_ops::delete_faces(&mut mesh, f)?;
            }
        }
        if let Some(e) = edges.get(&n) {
            edit_ops::delete_edges(&mut mesh, e)?;
        }
        if let Some(v) = verts.get(&n) {
            edit_ops::delete_vertices(&mut mesh, v)?;
        }
        changes.push(if node.edits_tri_mesh() {
            cage_change(node, mesh)
        } else {
            Change::ReplaceObject {
                id: n,
                before: Box::new(before.clone()),
                after: Box::new(before.with_mesh(mesh)),
            }
        });
    }
    commit(state, "Delete", Change::Group(changes))?;
    state.sub_selection.clear();
    Ok(json!({}))
}

/// Reference system the Type-In uses: View and Screen fall back to World
/// (Screen only for absolute values, which is what the status bar shows).
fn type_in_system(state: &AppState, absolute: bool) -> CoordSystem {
    match state.coord_system {
        CoordSystem::View => CoordSystem::World,
        CoordSystem::Screen if absolute => CoordSystem::World,
        s => s,
    }
}

/// Current Transform Type-In absolute values for the active tool.
pub fn type_in_values(state: &AppState) -> Vec3 {
    let first = state.selection.iter().next().copied();
    if state.slice_plane_mode {
        // Slice Plane mode: the fields show the plane (world position / Euler XYZ).
        let sp = &state.slice_plane;
        return match state.active_tool {
            Tool::Rotate => {
                let (x, y, z) = sp.rotation.to_euler(glam::EulerRot::XYZ);
                Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
            }
            Tool::Scale => Vec3::splat(100.0),
            _ => sp.origin,
        };
    }
    match state.active_tool {
        Tool::Rotate => first.and_then(|id| state.scene.node(id)).map(|n| {
            let (_, q, _) = state.scene.world_matrix(n.id).to_scale_rotation_translation();
            let (x, y, z) = q.to_euler(glam::EulerRot::XYZ);
            let w = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
            // A single node reports its own accumulating Euler values.
            if state.selection.len() == 1 && n.parent.is_none() { n.euler_deg() } else { w }
        }).unwrap_or(Vec3::ZERO),
        Tool::Scale => first.and_then(|id| state.scene.node(id)).map(|n| n.transform.scale.get() * 100.0).unwrap_or(Vec3::splat(100.0)),
        _ => {
            let vp = state.active_viewport;
            let origin = if state.selection.len() == 1 { frame_for(state, vp, first).origin } else { state.selection_pivot() };
            let axes = crate::transform::frame_for_system(state, vp, first, type_in_system(state, true)).axes;
            axes.transpose() * origin
        }
    }
}

fn transform_type_in(state: &mut AppState, absolute: bool, values: Vec3) -> Result<Value, String> {
    if state.selection.is_empty() {
        return Ok(json!({}));
    }
    let system = Some(type_in_system(state, absolute));
    if state.slice_plane_mode && absolute && matches!(state.active_tool, Tool::Rotate | Tool::Move | Tool::Select) {
        // Slice Plane mode: absolute values place the plane (world), never the object.
        let (position, rotation_deg) = if state.active_tool == Tool::Rotate { (None, Some(values.to_array())) } else { (Some(values.to_array()), None) };
        execute(state, Command::SetSlicePlane { position, rotation_deg, size: None })?;
        return Ok(json!({ "values": type_in_values(state).to_array() }));
    }
    match state.active_tool {
        Tool::Rotate => {
            let delta = if absolute { values - type_in_values(state) } else { values };
            state.undo.begin("Rotate");
            let r: Result<(), String> = (|| {
                for (axis, deg) in [(Axis::X, delta.x), (Axis::Y, delta.y), (Axis::Z, delta.z)] {
                    if deg.abs() > 1e-6 {
                        execute(state, Command::RotateSelection { axis, degrees: deg, system })?;
                    }
                }
                if absolute && system == Some(CoordSystem::World) {
                    // The orbit above positions the selection; the orientation is the
                    // typed Euler exactly (world Euler, converted to each parent's space).
                    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
                    let target = Quat::from_euler(glam::EulerRot::XYZ, values.x.to_radians(), values.y.to_radians(), values.z.to_radians());
                    for id in ids {
                        let (_, pq, _) = state.scene.edit_parent_frame(id).to_scale_rotation_translation();
                        let local = pq.inverse() * target;
                        let rot = if state.scene.node(id).is_some_and(|n| n.parent.is_none()) {
                            values
                        } else {
                            let (x, y, z) = local.to_euler(glam::EulerRot::XYZ);
                            Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
                        };
                        execute(state, Command::SetTransform { id: id.0, position: None, rotation_deg: Some(rot.to_array()), scale: None })?;
                    }
                }
                Ok(())
            })();
            state.undo.end();
            r?;
        }
        Tool::Scale => {
            if absolute {
                // Absolute scaling always uses the Local system: set each node's scale.
                let ids: Vec<NodeId> = state.selection.iter().copied().collect();
                state.undo.begin("Scale");
                let r: Result<(), String> = (|| {
                    for id in ids {
                        execute(state, Command::SetTransform { id: id.0, position: None, rotation_deg: None, scale: Some((values / 100.0).to_array()) })?;
                    }
                    Ok(())
                })();
                state.undo.end();
                r?;
            } else {
                execute(state, Command::ScaleSelection { factors: (values / 100.0).to_array(), system })?;
            }
        }
        _ => {
            let delta = if absolute { values - type_in_values(state) } else { values };
            if delta.length_squared() > 0.0 {
                execute(state, Command::MoveSelection { delta: delta.to_array(), system })?;
            }
        }
    }
    Ok(json!({ "values": type_in_values(state).to_array() }))
}

fn axis_unit(a: Axis) -> Vec3 {
    match a {
        Axis::X => Vec3::X,
        Axis::Y => Vec3::Y,
        Axis::Z => Vec3::Z,
    }
}

/// Align dialog point of a set of nodes, in `axes` coordinates: bounding
/// box minimum / centre / maximum of all their vertices, or the pivot
/// (single node) / selection centre.
fn align_point(state: &AppState, ids: &[NodeId], which: AlignPoint, axes: Mat3) -> Vec3 {
    let inv = axes.transpose();
    if which == AlignPoint::PivotPoint {
        let p = if ids.len() == 1 { state.scene.world_matrix(ids[0]).w_axis.truncate() } else {
            let sum: Vec3 = ids.iter().map(|i| state.scene.world_matrix(*i).w_axis.truncate()).sum();
            sum / ids.len().max(1) as f32
        };
        return inv * p;
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for id in ids {
        let Some(node) = state.scene.node(*id) else { continue };
        let world = state.scene.object_matrix(*id);
        for v in node.cage_mesh().vertices() {
            let p = inv * world.transform_point3(v.position);
            min = min.min(p);
            max = max.max(p);
        }
    }
    if !min.is_finite() {
        return inv * state.selection_pivot();
    }
    match which {
        AlignPoint::Minimum => min,
        AlignPoint::Maximum => max,
        _ => (min + max) * 0.5,
    }
}

/// Takes `id` out of its instance group before its object is replaced by
/// something the group does not share (Convert to Editable Poly, mesh edits).
/// A root hands the group to its first remaining member.
pub(crate) fn detach_instance(state: &mut AppState, id: NodeId) -> Result<(), String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    if let Some(root) = node.instance_of {
        let _ = root;
        return props_change(state, "Make Unique", vec![id], |p, _| p.instance_of = None);
    }
    let members: Vec<NodeId> = state.scene.nodes().filter(|n| n.instance_of == Some(id)).map(|n| n.id).collect();
    if members.is_empty() {
        return Ok(());
    }
    let new_root = members[0];
    props_change(state, "Make Unique", members, move |p, n| p.instance_of = if n.id == new_root { None } else { Some(new_root) })
}

/// Euler-degree change to expect on a node's Euler XYZ from a world
/// rotation vector (axis × degrees), used to pick the 360° wrap. Only
/// rotations about a parent-space X, the local Z, or (with X level) a
/// parent-space Y change a single Euler component; anything else gets no
/// hint and takes the nearest wrap.
fn euler_hint(state: &AppState, id: Option<NodeId>, world_rot_deg: Vec3) -> Vec3 {
    let Some(i) = id else { return Vec3::ZERO };
    let deg = world_rot_deg.length();
    if deg < 1e-6 {
        return Vec3::ZERO;
    }
    let axis = world_rot_deg / deg;
    let (_, pq, _) = state.scene.edit_parent_frame(i).to_scale_rotation_translation();
    let (_, wq, _) = state.scene.world_matrix(i).to_scale_rotation_translation();
    let in_parent = pq.inverse() * axis;
    let in_local = wq.inverse() * axis;
    let signed = |c: f32| if c > 0.0 { deg } else { -deg };
    if in_parent.x.abs() > 0.999 {
        Vec3::new(signed(in_parent.x), 0.0, 0.0)
    } else if in_local.z.abs() > 0.999 {
        Vec3::new(0.0, 0.0, signed(in_local.z))
    } else if in_parent.y.abs() > 0.999 && state.scene.node(i).is_some_and(|n| n.euler_deg().x.abs() < 1e-3) {
        Vec3::new(0.0, signed(in_parent.y), 0.0)
    } else {
        Vec3::ZERO
    }
}

fn save_managers(state: &AppState) -> SavedManagers {
    SavedManagers { coord_by_tool: state.coord_by_tool, center_by_tool: state.center_by_tool, center: (state.center_single, state.center_multi), tool: state.active_tool }
}

fn restore_managers(state: &mut AppState, s: SavedManagers) {
    state.coord_by_tool = s.coord_by_tool;
    state.center_by_tool = s.center_by_tool;
    (state.center_single, state.center_multi) = s.center;
    state.active_tool = s.tool;
    state.coord_system = state.coord_by_tool[tool_index(s.tool)];
    state.refresh_center_mode();
}

/// Gimbal moves and scales in the Parent system.
fn gimbal_as_parent(state: &mut AppState) {
    if state.coord_system == CoordSystem::Gimbal {
        state.coord_system = CoordSystem::Parent;
    }
}

/// With Auto Key on, Rotate and Scale use each object's pivot (Local Center
/// During Animate); returns the centre for the caller to restore.
fn animate_pivot(state: &mut AppState) -> crate::state::CenterMode {
    let saved = state.center_mode;
    if crate::transform::animate_forces_pivot(state) {
        state.center_mode = crate::state::CenterMode::PivotPoint;
    }
    saved
}

/// Temporarily applies an explicit reference coordinate system; returns the
/// previous one for the caller to restore.
fn with_system(state: &mut AppState, system: Option<CoordSystem>) -> CoordSystem {
    let saved = state.coord_system;
    if let Some(sys) = system {
        state.coord_system = sys;
    }
    saved
}

/// Transforms the selection with a per-node world matrix (`id` is None for
/// a sub-object selection), as one undo entry.
/// Per-node world matrix and Euler-degree hint for `transform_selection_each`.
type MatrixFor<'a> = &'a dyn Fn(&AppState, Option<NodeId>) -> (Mat4, Vec3);

fn transform_selection_each(state: &mut AppState, label: &str, matrix_for: MatrixFor) -> Result<Value, String> {
    // Skin > Edit Envelopes: the transform tools move envelope end points / lattice points only.
    if state.skin_active() && matches!(label, "Move" | "Rotate" | "Scale") {
        let (m, _) = matrix_for(state, None);
        if let Some(r) = crate::skin::transform(state, label, m) {
            return r;
        }
    }
    if let Some((id, _)) = crate::normals::normal_level(state) {
        let (m, _) = matrix_for(state, Some(id));
        if let Some(r) = crate::normals::transform(state, label, m) {
            return r;
        }
    }
    // Skin Morph Points: the tools shape the morph (Edit mode, or on the fly
    // with Beginner Mode off); Skin Wrap Control Vertices: nothing moves.
    if let Some((id, _)) = crate::morph::points_level(state) {
        let (m, _) = matrix_for(state, Some(id));
        return crate::morph::transform_points(state, label, m);
    }
    if crate::morph::control_vertex_level(state).is_some() {
        return Ok(json!({}));
    }
    let mut changes = Vec::new();
    // A modifier gizmo level (XForm Gizmo / Center): the tools move the gizmo.
    let gizmo_targets: Vec<(NodeId, usize, u32)> = state
        .selection
        .iter()
        .filter_map(|id| {
            let n = state.scene.node(*id)?;
            let a = n.active_entry?;
            let slot = n.modifiers.get(a)?;
            (slot.sub_level > 0 && slot.modifier.gizmo().is_some()).then_some((*id, a, slot.sub_level))
        })
        .collect();
    if !gizmo_targets.is_empty() {
        for (id, a, level) in gizmo_targets {
            let (m, _) = matrix_for(state, Some(id));
            let node = state.scene.node(id).ok_or_else(|| missing(id))?;
            let world = state.scene.object_matrix(id);
            let local_m = world.inverse() * m * world;
            let mut stack = node.modifiers.clone();
            if let Modifier::FFD { points, rest, gizmo, .. } = &mut stack[a].modifier {
                if level != 2 {
                    // Control Points / Set Volume: the selected points move in gizmo space;
                    // Set Volume moves their rest positions too (no deformation).
                    let g = gizmo.matrix();
                    let gm = g.inverse() * local_m * g;
                    let n = points.len();
                    for i in state.ffd_selection.iter().map(|i| *i as usize).filter(|i| *i < n) {
                        points[i] = gm.transform_point3(points[i]);
                        if level == 3 {
                            rest[i] = local_m.transform_point3(rest[i]);
                        }
                    }
                    changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: stack });
                    continue;
                }
                // Lattice: the whole gizmo moves, points along with it.
                *gizmo = Transform::from_matrix_near(local_m * gizmo.matrix(), gizmo, Vec3::ZERO);
                changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: stack });
                continue;
            }
            if level == 2 {
                if let Some(d) = stack[a].modifier.deformer_mut() {
                    // Center: only the center point moves; the gizmo stays.
                    let g = d.gizmo.matrix();
                    let c = g.inverse().transform_point3(local_m.transform_point3(g.transform_point3(d.center.get())));
                    d.center.set(c);
                    changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: stack });
                    continue;
                }
            }
            if let Some(gizmo) = stack[a].modifier.gizmo_mut() {
                let cur = gizmo.matrix();
                let next = if level == 2 {
                    // Center: translate only.
                    Mat4::from_translation(local_m.transform_point3(Vec3::ZERO)) * cur
                } else {
                    local_m * cur
                };
                *gizmo = Transform::from_matrix_near(next, gizmo, Vec3::ZERO);
            }
            changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: stack });
        }
        commit(state, label, Change::Group(changes))?;
        return Ok(json!({}));
    }
    if state.edit_working_pivot {
        // Edit Working Pivot: the transform tools move the working pivot.
        let (m, _) = matrix_for(state, None);
        let (_, rot, _) = m.to_scale_rotation_translation();
        let wp = &mut state.working_pivot;
        wp.origin = m.transform_point3(wp.origin);
        wp.rotation = (rot * wp.rotation).normalize();
        return Ok(json!({ "working_pivot": wp.origin.to_array() }));
    }
    if state.slice_plane_mode {
        // Slice Plane mode: the transform tools move the plane, not the mesh.
        let (m, _) = matrix_for(state, None);
        let (scale, rot, _) = m.to_scale_rotation_translation();
        let sp = &mut state.slice_plane;
        sp.origin = m.transform_point3(sp.origin);
        sp.rotation = (rot * sp.rotation).normalize();
        sp.size = (sp.size * scale.max_element()).max(0.01);
        return Ok(json!({ "slice_plane": sp.origin.to_array() }));
    }
    if state.selection_mode != SelectionMode::Object && state.sub_selection.is_empty() {
        return Ok(json!({}));
    }
    if state.selection_mode != SelectionMode::Object {
        let verts = selected_vertices(state);
        let constraint = match state.poly_constraint {
            crate::state::PolyConstraint::None => loom_mesh::geom_ops::Constraint::None,
            crate::state::PolyConstraint::Edges => loom_mesh::geom_ops::Constraint::Edges,
            crate::state::PolyConstraint::Faces => loom_mesh::geom_ops::Constraint::Faces,
            crate::state::PolyConstraint::Normals => loom_mesh::geom_ops::Constraint::Normals,
        };
        let (m, _) = matrix_for(state, None);
        let mut sim = Vec::new();
        for (n, ids) in verts {
            let weights = state.soft_weights(n);
            let node = state.scene.node(n).ok_or_else(|| missing(n))?;
            let world = state.scene.object_matrix(n);
            let local_m = world.inverse() * m * world;
            let before = node.object.clone();
            let mut mesh = node.cage_mesh().into_owned();
            let soft = (node.edit_poly().is_some_and(|p| p.soft.on) || node.edit_spline().is_some_and(|s| s.soft.on)) && weights.len() == mesh.vertex_count();
            if soft {
                // Soft Selection: every vertex moves by its weight of the full transform.
                let targets: Vec<(VertexId, glam::Vec3)> = (0..mesh.vertex_count())
                    .filter_map(|i| {
                        let v = VertexId(i as u32);
                        let w = if ids.contains(&v) { 1.0 } else { weights[i] };
                        (w > 0.0).then(|| {
                            let p = mesh.vertex_position(v).unwrap_or_default();
                            (v, p + (local_m.transform_point3(p) - p) * w)
                        })
                    })
                    .collect();
                let before_mesh = mesh.clone();
                for (v, target) in targets {
                    let p = mesh.vertex_position(v).unwrap_or_default();
                    let d = loom_mesh::geom_ops::constrain_delta(&before_mesh, v, target - p, constraint);
                    edit_ops::translate_vertices(&mut mesh, &[v], d)?;
                }
            } else if constraint == loom_mesh::geom_ops::Constraint::None {
                edit_ops::transform_vertices(&mut mesh, &ids, local_m)?;
            } else {
                let before_mesh = mesh.clone();
                for v in &ids {
                    let p = mesh.vertex_position(*v).unwrap_or_default();
                    let d = loom_mesh::geom_ops::constrain_delta(&before_mesh, *v, local_m.transform_point3(p) - p, constraint);
                    edit_ops::translate_vertices(&mut mesh, &[*v], d)?;
                }
            }
            sim.push((n, before, mesh));
        }
        for (n, _before, mesh) in sim {
            let node = state.scene.node(n).ok_or_else(|| missing(n))?;
            changes.push(cage_change(node, mesh));
        }
    } else {
        // A node whose ancestor is also selected moves with it; transforming
        // both would move it twice (so it is skipped).
        let selected = state.selection.clone();
        let has_selected_ancestor = |id: NodeId| {
            let mut cur = state.scene.node(id).and_then(|n| n.parent);
            while let Some(p) = cur {
                if selected.contains(&p) {
                    return true;
                }
                cur = state.scene.node(p).and_then(|n| n.parent);
            }
            false
        };
        for id in state.selection.iter().copied().filter(|id| !has_selected_ancestor(*id)) {
            let (m, hint) = matrix_for(state, Some(id));
            let node = state.scene.node(id).ok_or_else(|| missing(id))?;
            let parent_world = state.scene.edit_parent_frame(id);
            let world = state.scene.world_matrix(id);
            if state.affect_pivot_only && label == "Scale" {
                // A Scale transform has no effect on the pivot.
                continue;
            }
            if state.affect_hierarchy_only {
                // Rotate/Scale swing and stretch the links: each child's
                // position follows the transform, nothing turns or resizes.
                if label == "Rotate" || label == "Scale" {
                    for c in state.scene.children(id) {
                        let Some(child) = state.scene.node(c) else { continue };
                        let wc = state.scene.world_matrix(c);
                        let (cs, cq, ct) = wc.to_scale_rotation_translation();
                        let target = Mat4::from_scale_rotation_translation(cs, cq, m.transform_point3(ct));
                        let after = Transform::from_matrix_near(state.scene.parent_frame(c).inverse() * target, &child.transform, Vec3::ZERO);
                        changes.push(Change::SetTransform { id: c, before: child.transform.clone(), after });
                    }
                }
                continue;
            }
            if state.affect_object_only {
                // Move the mesh relative to the pivot; the pivot stays.
                let local_m = world.inverse() * m * world;
                let mut after = node.props();
                after.object_offset = Transform::from_matrix(local_m * node.object_offset.matrix());
                changes.push(Change::SetNodeProps { id, before: Box::new(node.props()), after: Box::new(after) });
                continue;
            }
            let new_local = parent_world.inverse() * m * world;
            let after = apply_locks(&node.transform, Transform::from_matrix_near(new_local, &node.transform, hint), node.locks);
            if state.affect_pivot_only {
                // Pivot moves; the object offset compensates so the mesh stays,
                // and the children stay too.
                let new_world = parent_world * after.matrix();
                let mesh_world = state.scene.object_matrix(id);
                let mut props = node.props();
                props.object_offset = Transform::from_matrix(new_world.inverse() * mesh_world);
                changes.push(Change::SetNodeProps { id, before: Box::new(node.props()), after: Box::new(props) });
                changes.extend(keep_children(state, id, new_world));
            } else if state.dont_affect_children {
                // Adjust Transform > Don't Affect Children.
                changes.extend(keep_children(state, id, parent_world * after.matrix()));
            }
            changes.push(Change::SetTransform {
                id,
                before: node.transform.clone(),
                after,
            });
        }
    }
    if state.bone_edit_mode {
        crate::rig::bone_edit_adjust(state, &mut changes);
    }
    if changes.is_empty() {
        return Ok(json!({}));
    }
    commit(state, label, Change::Group(changes))?;
    Ok(json!({}))
}

/// Hierarchy > Pivot alignment acting on the object (Affect Object Only):
/// `f` maps each selected node's object offset matrix to the new one.
fn object_offset_change(state: &mut AppState, label: &str, f: impl Fn(&AppState, NodeId, Mat4) -> Mat4) -> Result<Value, String> {
    let ids = selected_ids(state);
    let mut changes = Vec::new();
    for id in ids {
        let node = state.scene.node(id).ok_or_else(|| missing(id))?;
        let mut props = node.props();
        props.object_offset = Transform::from_matrix(f(state, id, node.object_offset.matrix()));
        changes.push(Change::SetNodeProps { id, before: Box::new(node.props()), after: Box::new(props) });
    }
    if !changes.is_empty() {
        commit(state, label, Change::Group(changes))?;
    }
    Ok(json!({}))
}

/// Moves each selected node's pivot to the world matrix `f` gives (and, if
/// set, the local scale), with the object offset compensating so the mesh
/// stays, and the children kept in place.
fn pivot_world_change(state: &mut AppState, label: &str, f: impl Fn(&AppState, NodeId) -> (Mat4, Option<Vec3>)) -> Result<Value, String> {
    let ids = selected_ids(state);
    let mut changes = Vec::new();
    for id in ids {
        let node = state.scene.node(id).ok_or_else(|| missing(id))?;
        let parent_world = state.scene.parent_frame(id);
        let (target, scale) = f(state, id);
        let mut after = Transform::from_matrix_near(parent_world.inverse() * target, &node.transform, Vec3::ZERO);
        if let Some(sc) = scale {
            after.scale.set(sc);
        }
        let new_world = parent_world * after.matrix();
        let mut props = node.props();
        props.object_offset = Transform::from_matrix(new_world.inverse() * state.scene.object_matrix(id));
        changes.push(Change::SetNodeProps { id, before: Box::new(node.props()), after: Box::new(props) });
        changes.extend(keep_children(state, id, new_world));
        changes.push(Change::SetTransform { id, before: node.transform.clone(), after });
    }
    if !changes.is_empty() {
        commit(state, label, Change::Group(changes))?;
    }
    Ok(json!({}))
}

/// Changes that keep every direct child where it is in the world after
/// `id`'s world matrix becomes `new_world` (pivot edits never move children).
fn keep_children(state: &AppState, id: NodeId, new_world: Mat4) -> Vec<Change> {
    let mut changes = Vec::new();
    for c in state.scene.children(id) {
        let Some(child) = state.scene.node(c) else { continue };
        let world_c = state.scene.world_matrix(c);
        let frame = loom_scene::Scene::frame_from(new_world, child.inherit);
        let after = Transform::from_matrix_near(frame.inverse() * world_c, &child.transform, Vec3::ZERO);
        changes.push(Change::SetTransform { id: c, before: child.transform.clone(), after });
    }
    changes
}

/// Keeps locked components of `before` (Link Info > Locks).
fn apply_locks(before: &Transform, mut after: Transform, locks: loom_scene::Axes3) -> Transform {
    if locks == loom_scene::Axes3::NONE {
        return after;
    }
    let (bp, ap) = (before.position.get(), after.position.get());
    after.position.set(Vec3::new(
        if locks.move_axes[0] { bp.x } else { ap.x },
        if locks.move_axes[1] { bp.y } else { ap.y },
        if locks.move_axes[2] { bp.z } else { ap.z },
    ));
    let (bs, as_) = (before.scale.get(), after.scale.get());
    after.scale.set(Vec3::new(
        if locks.scale_axes[0] { bs.x } else { as_.x },
        if locks.scale_axes[1] { bs.y } else { as_.y },
        if locks.scale_axes[2] { bs.z } else { as_.z },
    ));
    if locks.rotate_axes.iter().any(|l| *l) {
        let (b, a) = (before.rotation.get(), after.rotation.get());
        after.rotation.set(Vec3::new(
            if locks.rotate_axes[0] { b.x } else { a.x },
            if locks.rotate_axes[1] { b.y } else { a.y },
            if locks.rotate_axes[2] { b.z } else { a.z },
        ));
    }
    after
}

/// Hidden or frozen objects drop out of the selection.
pub(crate) fn prune_selection_hidden(state: &mut AppState) {
    let keep: BTreeSet<NodeId> = state
        .selection
        .iter()
        .copied()
        .filter(|id| state.scene.node(*id).is_some_and(|n| state.is_visible(n) && !n.frozen))
        .collect();
    state.selection = keep;
    state.sub_selection.retain_nodes(|id| state.selection.contains(&id));
}

/// Viewports looking through a camera that no longer exists fall back to Perspective.
fn prune_camera_views(state: &mut AppState) {
    for vp in &mut state.viewports {
        if let ViewKind::Camera(id) = vp.kind {
            if state.scene.node(id).is_none() {
                vp.kind = ViewKind::Perspective;
            }
        }
    }
}

fn open_by_name(state: &mut AppState, purpose: DialogPurpose) -> Result<Value, String> {
    state.dialog_purpose = purpose;
    state.dialog = Some(Dialog::SelectFromScene);
    Ok(json!({}))
}

fn resolve_node(state: &AppState, node: Option<u64>) -> Result<NodeId, String> {
    match node {
        Some(id) => {
            let id = NodeId(id);
            state.scene.node(id).map(|_| id).ok_or_else(|| missing(id))
        }
        None => state
            .selection
            .iter()
            .next()
            .copied()
            .ok_or_else(|| "no node given and nothing selected".to_owned()),
    }
}

pub(crate) fn sub_id(mode: SelectionMode, raw: u32) -> SubObjectId {
    match mode {
        SelectionMode::Vertex => SubObjectId::Vertex(VertexId(raw)),
        SelectionMode::Edge | SelectionMode::Border => SubObjectId::Edge(HalfEdgeId(raw)),
        _ => SubObjectId::Face(FaceId(raw)),
    }
}

fn props_change(state: &mut AppState, label: &str, ids: Vec<NodeId>, f: impl Fn(&mut NodeProps, &loom_scene::Node)) -> Result<(), String> {
    let mut changes = Vec::new();
    for id in ids {
        let node = state.scene.node(id).ok_or_else(|| missing(id))?;
        let before = node.props();
        let mut after = before.clone();
        f(&mut after, node);
        if after != before {
            changes.push(Change::SetNodeProps { id, before: Box::new(before), after: Box::new(after) });
        }
    }
    if changes.is_empty() {
        return Ok(());
    }
    commit(state, label, Change::Group(changes))
}

/// Siblings in the reference's sense: the objects as many generations below
/// the selected object's nearest branching ancestor (one with several
/// children, or the scene root) as it is, in creation order. In an uneven
/// hierarchy A can be B's sibling without B being A's.
fn siblings(scene: &loom_scene::Scene, id: NodeId) -> Vec<NodeId> {
    let kids = |p: Option<NodeId>| -> Vec<NodeId> {
        match p {
            Some(p) => scene.children(p),
            None => scene.nodes().filter(|n| n.parent.is_none()).map(|n| n.id).collect(),
        }
    };
    let mut cur = id;
    let mut depth = 1;
    let branch = loop {
        let parent = scene.node(cur).and_then(|n| n.parent);
        if parent.is_none() || kids(parent).len() > 1 {
            break parent;
        }
        cur = parent.expect("a parent");
        depth += 1;
    };
    let mut level = kids(branch);
    for _ in 1..depth {
        level = level.into_iter().flat_map(|n| scene.children(n)).collect();
    }
    level.sort_by_key(|n| n.0);
    level
}

/// A List controller holding `frozen` under a zero layer that edits go to
/// (the zero layer is first: a List sets its first weighted entry).
fn frozen_list(frozen: glam::Vec3, zero_name: &str, frozen_name: &str) -> loom_scene::Param<glam::Vec3> {
    use loom_scene::param::{Controller, ListItem};
    loom_scene::Param {
        controller: Controller::List(vec![
            ListItem { name: zero_name.into(), weight: 100.0, controller: Controller::Constant(glam::Vec3::ZERO) },
            ListItem { name: frozen_name.into(), weight: 100.0, controller: Controller::Constant(frozen) },
        ]),
    }
}

/// Freeze Transform / Position / Rotation on the selection: the current
/// value becomes the frozen layer, the active layer starts at zero.
fn freeze_transform(state: &mut AppState, pos: bool, rot: bool, label: &str) -> Result<Value, String> {
    let ids = selected_ids(state);
    if ids.is_empty() {
        return Err("select the objects first".into());
    }
    let mut changes = Vec::new();
    for id in ids {
        let node = state.scene.node(id).ok_or_else(|| missing(id))?;
        let mut t = node.transform.clone();
        if pos {
            t.position = frozen_list(node.transform.position.get(), "Zero Pos XYZ", "Frozen Position");
        }
        if rot {
            t.rotation = frozen_list(node.transform.rotation.get(), "Zero Euler XYZ", "Frozen Rotation");
        }
        changes.push(Change::SetTransform { id, before: node.transform.clone(), after: t });
    }
    commit_exact(state, label, Change::Group(changes))?;
    Ok(json!({}))
}

/// Transform / Position / Rotation To Zero: the active layer back to zero
/// (the frozen pose); a component never frozen goes to zero itself.
fn transform_to_zero(state: &mut AppState, pos: bool, rot: bool, label: &str) -> Result<Value, String> {
    use loom_scene::param::Controller;
    let ids = selected_ids(state);
    if ids.is_empty() {
        return Err("select the objects first".into());
    }
    let zero = |p: &loom_scene::Param<glam::Vec3>| -> loom_scene::Param<glam::Vec3> {
        let mut p = p.clone();
        match &mut p.controller {
            Controller::List(items) if !items.is_empty() => items[0].controller = Controller::Constant(glam::Vec3::ZERO),
            _ => p = loom_scene::Param::new(glam::Vec3::ZERO),
        }
        p
    };
    let mut changes = Vec::new();
    for id in ids {
        let node = state.scene.node(id).ok_or_else(|| missing(id))?;
        let mut t = node.transform.clone();
        if pos {
            t.position = zero(&t.position);
        }
        if rot {
            t.rotation = zero(&t.rotation);
        }
        changes.push(Change::SetTransform { id, before: node.transform.clone(), after: t });
    }
    commit_exact(state, label, Change::Group(changes))?;
    Ok(json!({}))
}

fn selected_ids(state: &AppState) -> Vec<NodeId> {
    state.selection.iter().copied().collect()
}

/// Raw sub-object ids per node at the current level.
pub fn selected_ids_raw(state: &AppState) -> Vec<(NodeId, Vec<u32>)> {
    let mode = state.selection_mode;
    state
        .sub_selection
        .by_node()
        .map(|(n, ids)| (n, crate::poly::raw_ids(mode, &ids.iter().copied().collect::<Vec<_>>())))
        .collect()
}

/// Replaces the sub-selection from raw ids per node (hidden ones skipped;
/// Element and Border expand / filter as Select Sub does).
pub fn set_sub_selection(state: &mut AppState, mode: SelectionMode, per_node: Vec<(NodeId, Vec<u32>)>) {
    state.sub_selection.clear();
    for (n, ids) in per_node {
        let Some(node) = state.scene.node(n) else { continue };
        let mesh = node.cage_mesh().into_owned();
        for raw in ids {
            let expanded: Vec<SubObjectId> = match mode {
                SelectionMode::Element => element_of(&mesh, FaceId(raw)).into_iter().map(SubObjectId::Face).collect(),
                SelectionMode::Border => match mesh.canonical_edge_id(HalfEdgeId(raw)) {
                    Some(e) if is_border_edge(&mesh, e) => vec![SubObjectId::Edge(e)],
                    _ => vec![],
                },
                _ => vec![sub_id(mode, raw)],
            };
            for id in expanded {
                if !is_hidden(&mesh, id) {
                    state.sub_selection.insert(n, id);
                }
            }
        }
    }
}

/// A sub-object is hidden when its vertex / face (or any face of an edge) is hidden.
pub fn is_hidden(mesh: &Mesh, id: SubObjectId) -> bool {
    match id {
        SubObjectId::Vertex(v) => mesh.vertices().get(v.0 as usize).is_some_and(|x| x.hidden),
        SubObjectId::Face(f) => mesh.faces().get(f.0 as usize).is_some_and(|x| x.hidden),
        SubObjectId::Edge(e) => {
            let Some(he) = mesh.half_edges().get(e.0 as usize) else { return false };
            let twin = &mesh.half_edges()[he.twin.0 as usize];
            let faces = [he.face, twin.face];
            let visible = faces.iter().flatten().any(|f| !mesh.faces()[f.0 as usize].hidden);
            let any = faces.iter().any(|f| f.is_some());
            any && !visible
        }
    }
}

fn all_sub_ids(mesh: &Mesh, mode: SelectionMode) -> Vec<SubObjectId> {
    all_sub_ids_raw(mesh, mode).into_iter().filter(|id| !is_hidden(mesh, *id)).collect()
}

fn all_sub_ids_raw(mesh: &Mesh, mode: SelectionMode) -> Vec<SubObjectId> {
    match mode {
        SelectionMode::Vertex => (0..mesh.vertex_count() as u32)
            .map(|v| SubObjectId::Vertex(VertexId(v)))
            .collect(),
        SelectionMode::Edge => (0..mesh.half_edges().len() as u32)
            .filter_map(|e| mesh.canonical_edge_id(HalfEdgeId(e)))
            .map(SubObjectId::Edge)
            .collect(),
        SelectionMode::Face | SelectionMode::Element => (0..mesh.face_count() as u32)
            .map(|f| SubObjectId::Face(FaceId(f)))
            .collect(),
        SelectionMode::Border => (0..mesh.half_edges().len() as u32)
            .filter_map(|e| mesh.canonical_edge_id(HalfEdgeId(e)))
            .filter(|e| is_border_edge(mesh, *e))
            .map(SubObjectId::Edge)
            .collect(),
        SelectionMode::Object => Vec::new(),
    }
}

/// An edge is a border when one of its half-edges has no face.
pub fn is_border_edge(mesh: &Mesh, e: HalfEdgeId) -> bool {
    let Some(he) = mesh.half_edges().get(e.0 as usize) else { return false };
    let twin = &mesh.half_edges()[he.twin.0 as usize];
    he.face.is_none() || twin.face.is_none()
}

/// All faces connected to `seed` through shared edges (an Element).
pub fn element_of(mesh: &Mesh, seed: FaceId) -> Vec<FaceId> {
    let mut seen: BTreeSet<FaceId> = BTreeSet::new();
    let mut stack = vec![seed];
    while let Some(f) = stack.pop() {
        if f.0 as usize >= mesh.face_count() || !seen.insert(f) {
            continue;
        }
        let start = mesh.faces()[f.0 as usize].he;
        let mut he = start;
        loop {
            let h = &mesh.half_edges()[he.0 as usize];
            if let Some(nf) = mesh.half_edges()[h.twin.0 as usize].face {
                if !seen.contains(&nf) {
                    stack.push(nf);
                }
            }
            he = h.next;
            if he == start {
                break;
            }
        }
    }
    seen.into_iter().collect()
}

/// Vertices covered by a set of sub-object ids (edges and faces expand).
pub fn vertices_of(mesh: &Mesh, ids: &BTreeSet<SubObjectId>) -> BTreeSet<VertexId> {
    let mut out = BTreeSet::new();
    for id in ids {
        match id {
            SubObjectId::Vertex(v) => {
                out.insert(*v);
            }
            SubObjectId::Edge(e) => {
                if let Some((a, b)) = mesh.edge_vertices(*e) {
                    out.insert(a);
                    out.insert(b);
                }
            }
            SubObjectId::Face(f) => {
                if let Some(vs) = mesh.face_vertex_ids(*f) {
                    out.extend(vs);
                }
            }
        }
    }
    out
}

/// Vertices moved by a sub-object transform: the selected vertices, or the
/// vertices of the selected edges / faces, per node.
pub fn selected_vertices(state: &AppState) -> Vec<(NodeId, Vec<VertexId>)> {
    state
        .sub_selection
        .by_node()
        .filter_map(|(n, ids)| {
            let node = state.scene.node(n)?;
            if let Some(shape) = node.edit_shape().filter(|_| node.edit_spline().is_some()) {
                // Spline levels: segments and splines resolve to their knots.
                let vs: Vec<VertexId> = crate::spline::selected_knots(state, n, &shape).into_iter().map(|k| VertexId(k as u32)).collect();
                return (!vs.is_empty()).then_some((n, vs));
            }
            let mesh = node.cage_mesh();
            let vs: Vec<VertexId> = vertices_of(&mesh, ids).into_iter().filter(|v| (v.0 as usize) < mesh.vertex_count()).collect();
            (!vs.is_empty()).then_some((n, vs))
        })
        .collect()
}

pub(crate) fn selected_edges(state: &AppState) -> Vec<(NodeId, Vec<HalfEdgeId>)> {
    state
        .sub_selection
        .by_node()
        .filter_map(|(n, ids)| {
            let mesh = state.scene.node(n)?.cage_mesh();
            let es: Vec<HalfEdgeId> = ids
                .iter()
                .filter_map(|id| match id {
                    SubObjectId::Edge(e) if mesh.canonical_edge_id(*e) == Some(*e) => Some(*e),
                    _ => None,
                })
                .collect();
            (!es.is_empty()).then_some((n, es))
        })
        .collect()
}

pub(crate) fn selected_faces(state: &AppState) -> Vec<(NodeId, Vec<FaceId>)> {
    state
        .sub_selection
        .by_node()
        .filter_map(|(n, ids)| {
            let faces = state.scene.node(n)?.cage_mesh().face_count();
            let fs: Vec<FaceId> = ids
                .iter()
                .filter_map(|id| match id {
                    SubObjectId::Face(f) if (f.0 as usize) < faces => Some(*f),
                    _ => None,
                })
                .collect();
            (!fs.is_empty()).then_some((n, fs))
        })
        .collect()
}

/// Applies a mesh edit to each node's mesh as one undo step. Parametric
/// objects become Editable Poly first, as "Convert To" would.
pub(crate) fn edit_meshes<T>(
    state: &mut AppState,
    label: &str,
    targets: Vec<(NodeId, Vec<T>)>,
    mut op: impl FnMut(&mut Mesh, NodeId, Vec<T>) -> Result<(), String>,
) -> Result<(), String> {
    let mut changes = Vec::new();
    for (n, ids) in targets {
        let node = state.scene.node(n).ok_or_else(|| missing(n))?;
        if node.edit_patch().is_some() {
            return Err(format!("{label}: an Editable Patch is edited with its own Geometry tools"));
        }
        let before = node.object.clone();
        let mut mesh = node.cage_mesh().into_owned();
        op(&mut mesh, n, ids)?;
        changes.push(match node.edit_poly().filter(|_| node.editing_entry().is_some()) {
            Some(p) => replace_poly_change(node, p.with_mesh(mesh)),
            None => Change::ReplaceObject { id: n, before: Box::new(before.clone()), after: Box::new(before.with_mesh(mesh)) },
        });
    }
    if changes.is_empty() {
        return Ok(());
    }
    commit(state, label, Change::Group(changes))
}

fn grow_edges(
    state: &mut AppState,
    f: fn(&Mesh, &[HalfEdgeId]) -> Vec<HalfEdgeId>,
) -> Result<Value, String> {
    let edges = selected_edges(state);
    let mut items = Vec::new();
    for (n, seeds) in edges {
        let Some(node) = state.scene.node(n) else { continue };
        for e in f(&node.cage_mesh(), &seeds) {
            items.push((n, SubObjectId::Edge(e)));
        }
    }
    for (n, id) in items {
        state.sub_selection.insert(n, id);
    }
    Ok(selection_json(state))
}

fn face_normal(mesh: &Mesh, id: FaceId) -> Option<Vec3> {
    let pts = mesh.face_positions(id)?;
    if pts.len() < 3 {
        return None;
    }
    let mut n = Vec3::ZERO;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    Some(n.normalize_or_zero())
}

pub(crate) fn selection_json(state: &AppState) -> Value {
    json!({
        "center": state.center_mode.label(),
        "selected": state.selection.iter().map(|id| id.0).collect::<Vec<_>>(),
        "sub_selected": state.sub_selection.len(),
        "mode": state.selection_mode.label(),
    })
}

fn camera_json(state: &AppState) -> Value {
    let vp = state.active();
    let c = &vp.camera;
    json!({
        "viewport": state.active_viewport,
        "view": state.view_label(state.active_viewport),
        "fov": vp.fov_y_radians.to_degrees(),
        "shading": vp.shading.label(),
        "yaw_deg": c.yaw.to_degrees(),
        "pitch_deg": c.pitch.to_degrees(),
        "roll_deg": c.roll.to_degrees(),
        "distance": c.distance,
        "target": [c.target.x, c.target.y, c.target.z],
    })
}

fn snapshot(state: &AppState) -> Value {
    let nodes: Vec<Value> = state
        .scene
        .nodes()
        .map(|n| {
            let mesh = n.cage_mesh();
            let (min, max) = mesh.aabb();
            let p = n.transform.position.get();
            let r = n.transform.rotation.get();
            let sc = n.transform.scale.get();
            json!({
                "id": n.id.0,
                "name": n.name,
                "kind": n.object.base_name(),
                "parent": n.parent.map(|p| p.0),
                "position": [p.x, p.y, p.z],
                "rotation_deg": [r.x, r.y, r.z],
                "scale": [sc.x, sc.y, sc.z],
                "instance_of": n.instance_of.map(|p| p.0),
                "stack": n.modifiers.iter().map(|s| json!({ "name": s.name, "modifier": s.modifier.name(), "enabled": s.enabled, "sub_level": s.sub_level })).collect::<Vec<_>>(),
                "active_entry": n.active_entry,
                "ffd_selection": state.ffd_selection.iter().collect::<Vec<_>>(),
                "maps": state.scene.eval_mesh(n.id).value.mesh.texture_maps().map(|(c, _)| *c).collect::<Vec<_>>(),
                "hidden": n.hidden,
                "frozen": n.frozen,
                "vertices": mesh.vertex_count(),
                "edges": mesh.edge_count(),
                "faces": mesh.face_count(),
                "aabb_local": [[min.x, min.y, min.z], [max.x, max.y, max.z]],
                "controllers": crate::controllers::transform_controllers_json(n),
            })
        })
        .collect();
    json!({
        "nodes": nodes,
        "unwrap": crate::unwrap::status(state),
        "skin": crate::skin::status(state),
        "morpher": crate::morph::morpher_status(state),
        "skin_morph": crate::morph::skin_morph_status(state),
        "skin_wrap": crate::morph::skin_wrap_status(state),
        "skin_wrap_patch": crate::morph::skin_wrap_patch_status(state),
        "file": state.file.as_ref().map(|p| p.to_string_lossy().into_owned()),
        "dirty": state.dirty(),
        "animation_clips": state.scene.animation_clips,
        "ik_chains": state.scene.ik_chains.iter().map(|c| json!({ "id": c.id, "name": c.name, "solver": c.solver.label(), "start": c.start.0, "end": c.end.0, "goal": c.goal.0, "enabled": c.enabled, "swivel": c.swivel_deg, "spline": c.spline.map(|s| s.0) })).collect::<Vec<_>>(),
        "title": state.title(),
        "selection": selection_json(state),
        "dialog": state.dialog.map(|d| format!("{d:?}")),
        "material_editor": state.material_editor_open,
        "rendered_frame": crate::render::state_json(state),
        "pick_mode": state.pick_mode.map(|p| format!("{p:?}")),
        "poly_drag": state.poly_drag.map(|p| format!("{p:?}")),
        "poly_settings": serde_json::to_value(state.poly_settings).unwrap_or(Value::Null),
        "tool": state.active_tool.label(),
        "camera": camera_json(state),
        "layout": format!("{:?}", state.layout),
        "viewports": state.viewports.iter().map(|v| json!({ "view": v.kind.label(), "shading": v.shading.label(), "edged_faces": v.edged_faces })).collect::<Vec<_>>(),
        "show_vertices": state.show_vertices,
        "nav_mode": format!("{:?}", state.nav_mode),
        "maximized": state.maximized,
        "frame": state.current_frame,
        "animation": crate::anim_cmds::state_json(state),
        "edit_normals": crate::normals::state_json(state),
        "hsds": crate::stack::hsds_json(state),
        "vertex_paint": crate::vertex_paint::state_json(state),
        "undo_labels": state.undo.labels(),
    })
}

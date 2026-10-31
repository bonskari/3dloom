//! Render To Texture: the dialog's settings, Render (Bake Textures), Unwrap
//! Only, Projection Mapping and the baked (Shell) materials. The geometry
//! elements (Normals, Height, Alpha) are baked here on the CPU; the lit
//! ones need the GPU renderer, so they become a request the UI works
//! through right after (as a Render does).

use crate::command::Command as C;
use crate::execute::commit_change;
use crate::AppState;
use glam::Mat3;
use loom_bake::{Bvh, NormalOptions, Projection, Triangle};
use loom_scene::bake::{BakeElement, BakeObjects, BakeSettings, ElementKind, NormalSpace, ProjectionData, AUTO_UNWRAP_NAME};
use loom_scene::material::{default_amount, next_material_id, Map, MaterialId, MaterialLibrary, MaterialKind, Shell, ShellView, StandardMaterial};
use loom_scene::{Change, Modifier, ModifierSlot, Node, NodeId};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One lit element of one object, for the UI's renderer.
#[derive(Debug, Clone, PartialEq)]
pub struct BakeJob {
    pub node: NodeId,
    pub element: BakeElement,
    /// The file to write.
    pub path: String,
    /// The map channel to bake into.
    pub channel: u32,
    /// Edge padding in texels.
    pub padding: u32,
}

/// The lit elements of a Render To Texture, waiting for the GPU renderer.
#[derive(Debug, Clone, PartialEq)]
pub struct BakeRequest {
    pub jobs: Vec<BakeJob>,
    /// Show each map in the Rendered Frame Window.
    pub rendered_frame_window: bool,
}

/// Largest map side in texels.
pub const MAX_MAP_SIZE: u32 = 8192;
/// Output > preset resolution buttons.
pub const SIZE_PRESETS: [u32; 5] = [128, 256, 512, 768, 1024];

pub fn handles(c: &C) -> bool {
    matches!(
        c,
        C::RenderToTexture
            | C::RenderToTextureSettings { .. }
            | C::ProjectionOptions { .. }
            | C::AddTextureElement { .. }
            | C::DeleteTextureElement { .. }
            | C::TextureElementSettings { .. }
            | C::BakeTextures
            | C::UnwrapOnly
            | C::ClearUnwrappers
            | C::PickProjectionTarget { .. }
            | C::UpdateBakedMaterials
            | C::ClearShellMaterials
    )
}

/// The folder baked maps go to: the dialog's, else `images` in the data directory.
pub fn output_dir(s: &BakeSettings) -> PathBuf {
    if s.output_dir.trim().is_empty() {
        crate::autosave::loom_home().join("images")
    } else {
        PathBuf::from(s.output_dir.trim())
    }
}

/// Objects that bake: geometry, not helpers, cameras, lights or bones.
pub fn is_geometry(n: &Node) -> bool {
    n.object.has_render_geometry() && !n.object.is_bone()
}

fn frozen(state: &AppState, n: &Node) -> bool {
    n.frozen || state.scene.layer(n.layer).is_some_and(|l| l.frozen)
}

fn has_auto_unwrap(n: &Node, channel: u32) -> bool {
    n.modifiers.iter().any(|m| m.name == AUTO_UNWRAP_NAME && matches!(&m.modifier, Modifier::UnwrapUVW(u) if u.channel.get() == channel))
}

/// Objects to Bake: the selected geometry (Individual, All Selected), or
/// every visible unfrozen object with the mapping to bake into (All Prepared).
pub fn bake_objects(state: &AppState, s: &BakeSettings) -> Vec<NodeId> {
    match s.objects {
        BakeObjects::AllPrepared => state
            .scene
            .nodes()
            .filter(|n| is_geometry(n) && state.is_visible(n) && !frozen(state, n))
            .filter(|n| if s.auto_unwrap { has_auto_unwrap(n, s.auto_channel) } else { state.scene.eval_mesh(n.id).value.mesh.map(s.existing_channel).is_some() })
            .map(|n| n.id)
            .collect(),
        BakeObjects::Individual | BakeObjects::AllSelected => state.selection.iter().copied().filter(|id| state.scene.node(*id).is_some_and(is_geometry)).collect(),
    }
}

fn targets(state: &AppState, s: &BakeSettings) -> Result<Vec<NodeId>, String> {
    let ids = bake_objects(state, s);
    if ids.is_empty() {
        return Err(match s.objects {
            BakeObjects::AllPrepared => "no prepared objects to bake (visible, unfrozen, with the mapping to bake into)".into(),
            _ => "select the objects to bake".into(),
        });
    }
    Ok(ids)
}

/// One undo step for the dialog's values.
fn set_bake(state: &mut AppState, label: &str, f: impl FnOnce(&mut BakeSettings) -> Result<(), String>) -> Result<Value, String> {
    let mut after = state.scene.render.clone();
    f(&mut after.bake)?;
    if after != state.scene.render {
        let before = Box::new(state.scene.render.clone());
        commit_change(state, label, Change::SetRenderSettings { before, after: Box::new(after) })?;
    }
    Ok(serde_json::to_value(&state.scene.render.bake).unwrap_or(Value::Null))
}

/// The stack change that gives node `id` an Automatic Flatten UVs entry on
/// the automatic channel, flattened with the Automatic Mapping values
/// (None: it has one already).
fn auto_unwrap_change(state: &AppState, id: NodeId, s: &BakeSettings) -> Result<Option<Change>, String> {
    let node = state.scene.node(id).ok_or("missing node")?;
    let ch = s.auto_channel;
    if has_auto_unwrap(node, ch) {
        return Ok(None);
    }
    let mut stack = node.modifiers.clone();
    // On top of the object-space entries (world-space ones stay above).
    let at = stack.iter().position(|m| m.modifier.is_wsm()).unwrap_or(stack.len());
    let input = match at.checked_sub(1) {
        Some(prev) => state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
        None => node.object.display_output(),
    };
    if input.face_count() == 0 {
        return Err(format!("{} has no faces to unwrap", node.name));
    }
    let mut u = loom_scene::UnwrapUvw::default();
    u.channel.set(ch);
    u.init(&input);
    let mut working = u.working_mesh(&input);
    let faces: Vec<loom_mesh::FaceId> = (0..working.face_count() as u32).map(loom_mesh::FaceId).collect();
    loom_mesh::unwrap::flatten(&mut working, ch, &faces, s.threshold_angle, s.spacing, true, s.rotate_clusters, s.fill_holes, loom_mesh::unwrap::FlattenBy::Angle);
    u.take_edits(&working);
    let mut slot = ModifierSlot::new(Modifier::UnwrapUVW(Box::new(u)));
    slot.name = AUTO_UNWRAP_NAME.to_owned();
    stack.insert(at, slot);
    Ok(Some(Change::SetStack { id, before: node.modifiers.clone(), after: stack }))
}

fn unwrap_objects(state: &mut AppState, ids: &[NodeId], s: &BakeSettings, label: &str) -> Result<usize, String> {
    let mut changes = Vec::new();
    for id in ids {
        if let Some(c) = auto_unwrap_change(state, *id, s)? {
            changes.push(c);
        }
    }
    let n = changes.len();
    if n > 0 {
        commit_change(state, label, Change::Group(changes))?;
    }
    Ok(n)
}

/// A node's evaluated mesh in world space.
pub fn world_mesh(state: &AppState, id: NodeId) -> loom_mesh::Mesh {
    let mut m = state.scene.eval_mesh(id).value.mesh.clone();
    let _ = loom_mesh::edit_ops::transform_mesh(&mut m, state.scene.object_matrix(id));
    m
}

/// The node's first enabled Projection modifier.
fn projection_of(node: &Node) -> Option<&ProjectionData> {
    node.modifiers.iter().filter(|m| m.enabled).find_map(|m| match &m.modifier {
        Modifier::Projection(d) => Some(&**d),
        _ => None,
    })
}

fn to_u8(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Bakes a geometry element (Normals, Height, Alpha) of `low` into an image
/// with edge padding; the heights the projection found come back too.
#[allow(clippy::too_many_arguments)]
fn bake_geometry(e: &BakeElement, low: &[Triangle], high: Option<(&Bvh, f32)>, s: &BakeSettings, basis: Mat3, opacity: impl Fn(u16) -> f32, padding: u32) -> (image::RgbaImage, Option<(f32, f32)>) {
    let raster = loom_bake::rasterize(low, e.width, e.height);
    let o = &s.options;
    let proj = high.map(|(bvh, push)| Projection {
        high: bvh,
        use_cage: o.use_cage,
        push,
        offset: o.offset,
        closest: o.closest,
        match_material_id: o.match_material_id,
        include_working: o.include_working_model,
        ray_miss_check: o.ray_miss_check,
        miss_color: to_u8(o.ray_miss_color),
    });
    let bg = to_u8(e.background);
    let (mut img, range) = match e.kind {
        ElementKind::Normals => {
            let space = match o.space {
                NormalSpace::Tangent => loom_bake::NormalSpace::Tangent,
                NormalSpace::World => loom_bake::NormalSpace::World,
                NormalSpace::Screen => loom_bake::NormalSpace::Screen,
                NormalSpace::LocalXyz => loom_bake::NormalSpace::Local,
            };
            let opts = NormalOptions { space, red_flip: o.red_flip, green_flip: o.green_flip, basis };
            loom_bake::bake_normals(low, &raster, proj.as_ref(), &opts, e.height_in_alpha.then_some((o.min_height, o.max_height)), bg)
        }
        ElementKind::Height => loom_bake::bake_height(low, &raster, proj.as_ref(), o.min_height, o.max_height, bg),
        _ => (loom_bake::bake_alpha(low, &raster, opacity, bg), None),
    };
    let mut covered = raster.coverage();
    loom_bake::dilate(&mut img, &mut covered, padding);
    (img, range)
}

/// What the elements put into object `name`'s baked material: (Target Map
/// Slot, file, as a Normal Bump map).
fn slot_maps(elements: &[BakeElement], dir: &Path, name: &str) -> Vec<(String, String, bool)> {
    elements.iter().filter(|e| e.enabled && !e.target_slot.is_empty()).map(|e| (e.target_slot.clone(), e.path(dir, name).to_string_lossy().into_owned(), e.kind == ElementKind::Normals && e.normal_bump)).collect()
}

/// Puts bitmaps into material `id`'s slots (every sub-material of a
/// Multi/Sub-Object material that is a Standard one). A Diffuse map shows in viewports.
fn put_maps(lib: &mut MaterialLibrary, id: MaterialId, maps: &[(String, String, bool)], depth: u32) -> Result<(), String> {
    let Some(m) = lib.get(&id) else { return Ok(()) };
    match &m.kind {
        MaterialKind::MultiSub(list) if depth < 8 => {
            let subs: Vec<MaterialId> = list.iter().filter_map(|s| s.material).collect();
            for s in subs {
                put_maps(lib, s, maps, depth + 1)?;
            }
            Ok(())
        }
        MaterialKind::Standard => {
            let m = lib.get_mut(&id).expect("checked");
            for (slot, path, normal_bump) in maps {
                let map = if *normal_bump {
                    // Output into Normal Bump: the normals in a Normal Bump map's Normal component.
                    let mut n = Map::new(loom_scene::material::MapType::NormalBump, default_amount(slot));
                    n.sub_maps[0] = Some(Box::new(Map::bitmap(path.clone(), 100.0)));
                    n
                } else {
                    Map::bitmap(path.clone(), default_amount(slot))
                };
                *m.map_slot_mut(slot)? = Some(map);
                if slot == "Diffuse Color" {
                    m.show_map_in_viewport = true;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// The material changes that put the baked maps of `ids` into their
/// materials: the source material (Output Into Source) or the baked
/// material of a Shell (Save Source; a Shell is made when there is none).
fn baked_materials(state: &AppState, ids: &[NodeId], elements: &[BakeElement], s: &BakeSettings, into_source: bool) -> Result<Vec<Change>, String> {
    let dir = output_dir(s);
    let before = Box::new((state.scene.materials.clone(), state.scene.material_editor.clone()));
    let mut lib = before.0.clone();
    let mut assigns = Vec::new();
    for id in ids {
        let node = state.scene.node(*id).ok_or("missing node")?;
        let maps = slot_maps(elements, &dir, &node.name);
        if maps.is_empty() {
            continue;
        }
        let shell = node.material.and_then(|m| match &lib.get(&m)?.kind {
            MaterialKind::Shell(sh) => Some((m, (**sh).clone())),
            _ => None,
        });
        if into_source {
            let source = match (&shell, node.material) {
                (Some((_, sh)), _) => sh.materials[0],
                (None, m) => m,
            };
            let source = match source {
                Some(m) => m,
                None => {
                    let m = next_material_id(&lib);
                    lib.insert(m, StandardMaterial::new(format!("{} Baked", node.name)));
                    assigns.push(Change::SetMaterial { id: *id, before: node.material, after: Some(m) });
                    m
                }
            };
            put_maps(&mut lib, source, &maps, 0)?;
            continue;
        }
        match shell {
            Some((shell_id, sh)) => {
                let baked = match sh.materials[1] {
                    Some(b) => b,
                    None => {
                        let b = next_material_id(&lib);
                        lib.insert(b, StandardMaterial::new(format!("{} Baked", node.name)));
                        b
                    }
                };
                put_maps(&mut lib, baked, &maps, 0)?;
                if let Some(MaterialKind::Shell(sh)) = lib.get_mut(&shell_id).map(|m| &mut m.kind) {
                    sh.materials[1] = Some(baked);
                }
            }
            None => {
                let original = node.material;
                let baked = next_material_id(&lib);
                let mut b = match original.and_then(|o| lib.get(&o)).filter(|_| !s.create_new_baked) {
                    // Duplicate Source To Baked: a copy of the source material.
                    Some(src) => src.clone(),
                    None => StandardMaterial::new(""),
                };
                b.name = format!("{} Baked", node.name);
                lib.insert(baked, b);
                put_maps(&mut lib, baked, &maps, 0)?;
                let shell_id = next_material_id(&lib);
                let mut m = StandardMaterial::new(format!("{} Shell", node.name));
                m.kind = MaterialKind::Shell(Box::new(Shell { materials: [original, Some(baked)], viewport: s.views_baked as u8, render: s.render_baked as u8 }));
                lib.insert(shell_id, m);
                assigns.push(Change::SetMaterial { id: *id, before: original, after: Some(shell_id) });
            }
        }
    }
    if lib == before.0 && assigns.is_empty() {
        return Ok(Vec::new());
    }
    let after = Box::new((lib, before.1.clone()));
    let mut changes = vec![Change::SetMaterials { before, after }];
    changes.extend(assigns);
    Ok(changes)
}

/// Original / Baked: the objects' Shell materials show `views` / `render`.
fn shell_choice(state: &AppState, ids: &[NodeId], views: Option<bool>, render: Option<bool>) -> Option<Change> {
    let before = Box::new((state.scene.materials.clone(), state.scene.material_editor.clone()));
    let mut lib = before.0.clone();
    for id in ids {
        let Some(m) = state.scene.node(*id).and_then(|n| n.material) else { continue };
        if let Some(MaterialKind::Shell(sh)) = lib.get_mut(&m).map(|m| &mut m.kind) {
            if let Some(v) = views {
                sh.viewport = v as u8;
            }
            if let Some(r) = render {
                sh.render = r as u8;
            }
        }
    }
    (lib != before.0).then(|| Change::SetMaterials { after: Box::new((lib, before.1.clone())), before })
}

fn element_mut(s: &mut BakeSettings, index: u32) -> Result<&mut BakeElement, String> {
    let n = s.elements.len();
    s.elements.get_mut(index as usize).ok_or_else(|| format!("no texture element {index} ({n} in the list)"))
}

/// Render: bakes every enabled element of every object to bake.
fn bake(state: &mut AppState) -> Result<Value, String> {
    if state.pending_bake.is_some() {
        return Err("a Render To Texture is still being rendered".into());
    }
    let s = state.scene.render.bake.clone();
    let elements: Vec<BakeElement> = s.elements.iter().filter(|e| e.enabled).cloned().collect();
    if elements.is_empty() {
        return Err("no texture elements to render: Add elements on the Output rollout".into());
    }
    let ids = targets(state, &s)?;
    let dir = output_dir(&s);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    state.undo.begin("Render To Texture");
    let r = (|| -> Result<Value, String> {
        if s.auto_unwrap {
            unwrap_objects(state, &ids, &s, "Render To Texture")?;
        }
        let ch = s.channel();
        let mut written = Vec::new();
        let mut skipped = Vec::new();
        let mut jobs = Vec::new();
        let mut heights: Option<(f32, f32)> = None;
        for id in &ids {
            let node = state.scene.node(*id).ok_or("missing node")?.clone();
            let world = world_mesh(state, *id);
            if world.map(ch).is_none() {
                return Err(format!("{} has no map channel {ch}: turn on Use Automatic Unwrap or map that channel", node.name));
            }
            let low = loom_bake::triangles(&world, ch);
            let high = match (s.projection, projection_of(&node)) {
                (false, _) => None,
                (true, None) => return Err(format!("{} has no Projection modifier: Pick the high-resolution object first", node.name)),
                (true, Some(p)) => {
                    let mut tris = Vec::new();
                    for t in &p.targets {
                        if state.scene.node(*t).is_some() {
                            tris.extend(loom_bake::triangles(&world_mesh(state, *t), 1));
                        }
                    }
                    if tris.is_empty() {
                        return Err(format!("{}'s Projection modifier has no Reference Geometry", node.name));
                    }
                    Some((Bvh::new(tris), p.push.get()))
                }
            };
            let basis = match s.options.space {
                NormalSpace::LocalXyz => {
                    let m = Mat3::from_mat4(state.scene.object_matrix(*id));
                    Mat3::from_cols(m.x_axis.normalize_or_zero(), m.y_axis.normalize_or_zero(), m.z_axis.normalize_or_zero()).transpose()
                }
                NormalSpace::Screen => {
                    let (right, up, eye) = state.active().camera.basis();
                    Mat3::from_cols(right, up, eye).transpose()
                }
                _ => Mat3::IDENTITY,
            };
            let surfaces = state.scene.node_surfaces_for(*id, ShellView::Original);
            let opacity = |mid: u16| surfaces.as_ref().map_or(1.0, |s| s.for_face(mid).opacity);
            for e in &elements {
                let path = e.path(&dir, &node.name);
                let path_s = path.to_string_lossy().into_owned();
                if s.skip_existing && path.exists() {
                    skipped.push(path_s);
                    continue;
                }
                if e.width == 0 || e.height == 0 || e.width > MAX_MAP_SIZE || e.height > MAX_MAP_SIZE {
                    return Err(format!("{}: map size {}x{} (1 to {MAX_MAP_SIZE})", e.name, e.width, e.height));
                }
                if e.kind.lit() {
                    jobs.push(BakeJob { node: *id, element: e.clone(), path: path_s, channel: ch, padding: s.padding });
                    continue;
                }
                let (img, range) = bake_geometry(e, &low, high.as_ref().map(|(b, p)| (b, *p)), &s, basis, opacity, s.padding);
                if let Some((a, b)) = range {
                    heights = Some(heights.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
                }
                crate::render::save_image(&img, &path_s)?;
                if s.rendered_frame_window {
                    state.rendered.title = format!("{} {}", node.name, e.name);
                    state.rendered.set_image(Some(Arc::new(img)));
                    state.rendered.open = true;
                }
                written.push(path_s);
            }
        }
        if let Some((lo, hi)) = heights {
            set_bake(state, "Render To Texture", |b| {
                b.options.buffer_min = lo;
                b.options.buffer_max = hi;
                Ok(())
            })?;
        }
        if !s.render_to_files_only {
            let changes = baked_materials(state, &ids, &elements, &s, s.output_into_source)?;
            if !changes.is_empty() {
                commit_change(state, "Render To Texture", Change::Group(changes))?;
            }
        }
        let pending = jobs.len();
        if pending > 0 {
            state.pending_bake = Some(BakeRequest { jobs, rendered_frame_window: s.rendered_frame_window });
        }
        // Files on disk changed: maps read them again.
        state.bitmap_generation += 1;
        state.status_prompt = format!("Render To Texture: {} map(s) written{}", written.len(), if pending > 0 { format!(", {pending} rendering") } else { String::new() });
        Ok(json!({ "objects": ids.iter().map(|n| n.0).collect::<Vec<_>>(), "written": written, "skipped": skipped, "pending": pending }))
    })();
    state.undo.end();
    r
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::RenderToTexture => {
            state.render_to_texture_open = !state.render_to_texture_open;
            Ok(json!({ "open": state.render_to_texture_open }))
        }
        C::RenderToTextureSettings { output_dir, skip_existing, rendered_frame_window, objects, enable, padding, projection, auto_unwrap, channel, output_into_source, create_new_baked, render_to_files_only, keep_baked, views_baked, render_baked, rotate_clusters, threshold_angle, fill_holes, spacing } => {
            let objects = match objects {
                Some(o) => Some(BakeObjects::from_label(&o).ok_or_else(|| format!("no Objects to Bake choice \"{o}\" (Individual, All Selected, All Prepared)"))?),
                None => None,
            };
            if let Some(c) = channel {
                if !(1..=99).contains(&c) {
                    return Err(format!("map channel {c}: 1 to 99"));
                }
            }
            if threshold_angle.is_some_and(|a| !(0.0..=180.0).contains(&a)) || spacing.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
                return Err("Threshold Angle 0-180, Spacing 0-1".into());
            }
            let ids = bake_objects(state, &state.scene.render.bake);
            state.undo.begin("Render To Texture");
            let r = (|| {
                let v = set_bake(state, "Render To Texture", |b| {
                    if let Some(v) = output_dir {
                        b.output_dir = v;
                    }
                    macro_rules! set {
                        ($($f:ident),*) => { $( if let Some(v) = $f { b.$f = v; } )* };
                    }
                    set!(skip_existing, rendered_frame_window, objects, enable, padding, projection, auto_unwrap, output_into_source, create_new_baked, render_to_files_only, keep_baked, views_baked, render_baked, rotate_clusters, threshold_angle, fill_holes, spacing);
                    if let Some(c) = channel {
                        if b.auto_unwrap {
                            b.auto_channel = c;
                        } else {
                            b.existing_channel = c;
                        }
                    }
                    Ok(())
                })?;
                if views_baked.is_some() || render_baked.is_some() {
                    if let Some(c) = shell_choice(state, &ids, views_baked, render_baked) {
                        commit_change(state, "Render To Texture", c)?;
                    }
                }
                Ok(v)
            })();
            state.undo.end();
            r
        }
        C::ProjectionOptions { use_cage, offset, closest, match_material_id, include_working_model, ray_miss_check, ray_miss_color, normal_space, red_flip, green_flip, min_height, max_height } => {
            let space = match normal_space {
                Some(n) => Some(NormalSpace::from_label(&n).ok_or_else(|| format!("no Normal Map Space \"{n}\" (World, Screen, Local XYZ, Tangent)"))?),
                None => None,
            };
            set_bake(state, "Projection Options", |b| {
                let o = &mut b.options;
                macro_rules! set {
                    ($($f:ident),*) => { $( if let Some(v) = $f { o.$f = v; } )* };
                }
                set!(use_cage, offset, closest, match_material_id, include_working_model, ray_miss_check, ray_miss_color, red_flip, green_flip, min_height, max_height);
                if let Some(v) = space {
                    o.space = v;
                }
                Ok(())
            })
        }
        C::AddTextureElement { element } => {
            let kind = ElementKind::from_label(&element).ok_or_else(|| format!("no texture element \"{element}\" ({})", ElementKind::ALL.map(|k| k.label()).join(", ")))?;
            set_bake(state, "Add Texture Element", |b| {
                b.elements.push(BakeElement::new(kind));
                Ok(())
            })
        }
        C::DeleteTextureElement { index } => set_bake(state, "Delete Texture Element", |b| {
            element_mut(b, index)?;
            b.elements.remove(index as usize);
            Ok(())
        }),
        C::TextureElementSettings { index, enabled, name, file, format, width, height, target_slot, background, component, on } => {
            if let Some(slot) = target_slot.as_deref().filter(|s| !s.is_empty()) {
                StandardMaterial::new("").map_slot_mut(slot)?;
            }
            for v in [width, height].into_iter().flatten() {
                if v == 0 || v > MAX_MAP_SIZE {
                    return Err(format!("map size {v}: 1 to {MAX_MAP_SIZE}"));
                }
            }
            if let Some(f) = format.as_deref() {
                if !crate::render::IMAGE_FORMATS.contains(&f.to_ascii_lowercase().as_str()) {
                    return Err(format!("file type {f}: {}", crate::render::IMAGE_FORMATS.join(", ")));
                }
            }
            set_bake(state, "Texture Element", |b| {
                let e = element_mut(b, index)?;
                if let Some(v) = enabled {
                    e.enabled = v;
                }
                if let Some(v) = name {
                    e.name = v;
                }
                if let Some(v) = file {
                    e.file = v;
                }
                if let Some(v) = format {
                    e.format = v.to_ascii_lowercase();
                }
                if let Some(v) = width {
                    e.width = v;
                }
                if let Some(v) = height {
                    e.height = v;
                }
                if let Some(v) = target_slot {
                    e.target_slot = v;
                }
                if let Some(v) = background {
                    e.background = v;
                }
                match (component, on) {
                    (Some(c), Some(v)) => {
                        if !e.kind.components().contains(&c.as_str()) {
                            return Err(format!("{} has no component \"{c}\" ({})", e.kind.label(), e.kind.components().join(", ")));
                        }
                        e.set_component(&c, v)?;
                    }
                    (None, None) => {}
                    _ => return Err("`component` and `on` go together".into()),
                }
                Ok(())
            })
        }
        C::BakeTextures => bake(state),
        C::UnwrapOnly => {
            let s = state.scene.render.bake.clone();
            let ids = targets(state, &s)?;
            let n = unwrap_objects(state, &ids, &s, "Unwrap Only")?;
            Ok(json!({ "unwrapped": n, "channel": s.auto_channel }))
        }
        C::ClearUnwrappers => {
            let s = state.scene.render.bake.clone();
            let ids = targets(state, &s)?;
            let mut changes = Vec::new();
            for id in &ids {
                let node = state.scene.node(*id).ok_or("missing node")?;
                let after: Vec<ModifierSlot> = node.modifiers.iter().filter(|m| !(m.name == AUTO_UNWRAP_NAME && matches!(m.modifier, Modifier::UnwrapUVW(_)))).cloned().collect();
                if after.len() != node.modifiers.len() {
                    changes.push(Change::SetStack { id: *id, before: node.modifiers.clone(), after });
                }
            }
            let n = changes.len();
            if n > 0 {
                commit_change(state, "Clear Unwrappers", Change::Group(changes))?;
                for id in &ids {
                    if let Some(node) = state.scene.node_mut(*id) {
                        node.active_entry = node.active_entry.filter(|a| *a < node.modifiers.len());
                    }
                }
            }
            Ok(json!({ "cleared": n }))
        }
        C::PickProjectionTarget { target } => {
            let target = NodeId(target);
            let t = state.scene.node(target).ok_or_else(|| format!("no node {}", target.0))?;
            if !is_geometry(t) {
                return Err(format!("{} is not geometry", t.name));
            }
            let s = state.scene.render.bake.clone();
            let ids: Vec<NodeId> = targets(state, &s)?.into_iter().filter(|id| *id != target).collect();
            if ids.is_empty() {
                return Err("select the low-resolution objects, then pick the high-resolution one".into());
            }
            let (tlo, thi) = world_mesh(state, target).aabb();
            let mut changes = Vec::new();
            for id in &ids {
                let node = state.scene.node(*id).ok_or("missing node")?;
                let mut stack = node.modifiers.clone();
                match stack.iter_mut().find_map(|m| match &mut m.modifier {
                    Modifier::Projection(d) => Some(d),
                    _ => None,
                }) {
                    Some(d) => {
                        if !d.targets.contains(&target) {
                            d.targets.push(target);
                        }
                    }
                    None => {
                        // The cage starts pushed out past the high-resolution
                        // object's bounds, plus a twentieth of the object's size.
                        let (lo, hi) = world_mesh(state, *id).aabb();
                        let out = (tlo - lo).abs().max((thi - hi).abs()).max_element();
                        let size = ((hi - lo) * 0.5).length();
                        let mut d = ProjectionData::default();
                        d.targets.push(target);
                        d.push.set(out + size * 0.05);
                        let at = stack.iter().position(|m| m.modifier.is_wsm()).unwrap_or(stack.len());
                        stack.insert(at, ModifierSlot::new(Modifier::Projection(Box::new(d))));
                    }
                }
                changes.push(Change::SetStack { id: *id, before: node.modifiers.clone(), after: stack });
            }
            commit_change(state, "Pick Projection Target", Change::Group(changes))?;
            Ok(json!({ "objects": ids.iter().map(|n| n.0).collect::<Vec<_>>(), "target": target.0 }))
        }
        C::UpdateBakedMaterials => {
            let s = state.scene.render.bake.clone();
            let ids = targets(state, &s)?;
            let elements: Vec<BakeElement> = s.elements.iter().filter(|e| e.enabled).cloned().collect();
            let changes = baked_materials(state, &ids, &elements, &s, false)?;
            let n = changes.len();
            if n > 0 {
                commit_change(state, "Update Baked Materials", Change::Group(changes))?;
            }
            Ok(json!({ "updated": n > 0 }))
        }
        C::ClearShellMaterials => {
            let s = state.scene.render.bake.clone();
            let ids = targets(state, &s)?;
            let mut changes = Vec::new();
            for id in &ids {
                let node = state.scene.node(*id).ok_or("missing node")?;
                let Some(m) = node.material else { continue };
                if let Some(MaterialKind::Shell(sh)) = state.scene.materials.get(&m).map(|m| &m.kind) {
                    changes.push(Change::SetMaterial { id: *id, before: Some(m), after: sh.materials[s.keep_baked as usize] });
                }
            }
            let n = changes.len();
            if n > 0 {
                commit_change(state, "Clear Shell Materials", Change::Group(changes))?;
            }
            Ok(json!({ "cleared": n }))
        }
        _ => unreachable!("routed by handles()"),
    }
}


use eframe::egui;
use egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor};
use glam::{Mat4, Vec2, Vec3};
use loom_input::{InputEvent, Modifiers, MouseButton};
use loom_mesh::{FaceId, HalfEdgeId, Mesh, VertexId};
use loom_render::{MeshBuffer, ViewportRenderer};

use crate::gizmo::{self, DragState, Projection};
use loom_commands::{Axis, Command, SelectOp, SelectionMode, SubObjectId, Tool};
use loom_scene::NodeId;
use crate::LoomApp;

// ---------------------------------------------------------------------------
// Resource stored permanently in egui-wgpu's CallbackResources TypeMap
// ---------------------------------------------------------------------------

/// Persistent GPU-side state shared across all frames.
pub struct ViewportResources {
    pub renderer: ViewportRenderer,
    pub mesh_buffers: Vec<MeshBuffer>,
    /// Built on the first frame (needs the queue) and whenever bitmaps change.
    pub textures: Option<loom_render::TextureSet>,
    /// GPU bitmaps by path: uploaded once, reused across scene rebuilds.
    pub bitmap_cache: std::collections::HashMap<String, std::sync::Arc<wgpu::BindGroup>>,
    pub base_textures: Option<(std::sync::Arc<wgpu::BindGroup>, std::sync::Arc<wgpu::BindGroup>)>,
}

/// Called once during `LoomApp::new` to install `ViewportResources` into the
/// egui-wgpu resource map.
pub fn init_render_resources(wgpu: &egui_wgpu::RenderState) {
    let renderer = ViewportRenderer::new(&wgpu.device, wgpu.target_format);
    wgpu.renderer
        .write()
        .callback_resources
        .insert(ViewportResources {
            renderer,
            mesh_buffers: Vec::new(),
            textures: None,
            bitmap_cache: std::collections::HashMap::new(),
            base_textures: None,
        });
}

/// What a click or marquee hit, by index into `LoomApp::world_meshes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pick {
    Object(usize),
    SubObject { mesh: usize, id: SubObjectId },
}

/// Applies picks with modifiers: Ctrl adds, Alt subtracts, plain
/// replaces. An empty plain click deselects.
fn apply_picks(app: &mut LoomApp, picks: Vec<Pick>, mods: Modifiers) {
    let op = if mods.ctrl { SelectOp::Add } else if mods.alt { SelectOp::Subtract } else { SelectOp::Replace };
    if picks.is_empty() {
        if op == SelectOp::Replace {
            let _ = app.dispatch(Command::SelectNone);
        }
        return;
    }
    let world: Vec<NodeId> = app.world_meshes().iter().map(|(id, _)| *id).collect();
    let mut objects: Vec<u64> = Vec::new();
    let mut subs: std::collections::BTreeMap<u64, Vec<u32>> = Default::default();
    for p in picks {
        match p {
            Pick::Object(i) => objects.push(world[i].0),
            Pick::SubObject { mesh, id } => {
                let raw = match id {
                    SubObjectId::Vertex(v) => v.0,
                    SubObjectId::Edge(e) => e.0,
                    SubObjectId::Face(f) => f.0,
                };
                subs.entry(world[mesh].0).or_default().push(raw);
            }
        }
    }
    if !objects.is_empty() {
        let _ = app.dispatch(Command::SelectNodes { ids: objects, op });
    }
    // By Angle: a clicked polygon brings its neighbours within the angle.
    if let Some(angle) = app.by_angle.filter(|_| app.state.selection_mode.is_face_level()) {
        let cos_t = angle.to_radians().cos();
        for (node, ids) in subs.iter_mut() {
            let Some((_, mesh)) = app.world_meshes().iter().find(|(n, _)| n.0 == *node) else { continue };
            let mut queue: Vec<u32> = ids.clone();
            let mut seen: std::collections::BTreeSet<u32> = ids.iter().copied().collect();
            while let Some(f) = queue.pop() {
                let n0 = mesh.face_normal(FaceId(f));
                for v in mesh.face_vertex_ids(FaceId(f)).unwrap_or_default() {
                    for nb in mesh.faces_around_vertex(v) {
                        if seen.contains(&nb.0) {
                            continue;
                        }
                        if mesh.face_normal(nb).dot(n0) >= cos_t {
                            seen.insert(nb.0);
                            queue.push(nb.0);
                        }
                    }
                }
            }
            *ids = seen.into_iter().collect();
        }
    }
    let mut first = true;
    for (node, ids) in subs {
        let op = if first { op } else if op == SelectOp::Replace { SelectOp::Add } else { op };
        first = false;
        let _ = app.dispatch(Command::SelectSub { node: Some(node), ids, op });
    }
}

// ---------------------------------------------------------------------------
// Per-frame paint callback
// ---------------------------------------------------------------------------

/// Data captured from LoomApp each frame and passed into the callback.
struct ViewportCallback {
    slot: usize,
    view_proj: Mat4,
    camera_pos: Vec3,
    upload: crate::PendingUpload,
    shading: loom_render::ViewShading,
    selected: Vec<usize>,
    show_edges: bool,
    shaded: bool,
    grid: Option<loom_render::GridPlane>,
    orthographic: bool,
    grid_params: loom_render::GridParams,
}

impl CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let res: &mut ViewportResources = callback_resources.get_mut().unwrap();
        res.renderer.set_grid_params(device, self.grid_params);

        let pending = self.upload.lock().ok().and_then(|mut slot| slot.take());
        if let Some((meshes, styles, bitmaps)) = pending {
            res.mesh_buffers = meshes
                .iter()
                .enumerate()
                .map(|(i, m)| MeshBuffer::from_mesh(device, m, styles.get(i).copied().unwrap_or_default()))
                .collect();
            // The clear and checker textures are made once; bitmaps once per path.
            let (none, checker) = res.base_textures.get_or_insert_with(|| res.renderer.base_textures(device, queue)).clone();
            let mut groups = Vec::with_capacity(bitmaps.len());
            for (path, bm) in &bitmaps {
                let bg = res.bitmap_cache.entry(path.clone()).or_insert_with(|| res.renderer.bitmap_texture(device, queue, bm, path)).clone();
                groups.push(bg);
            }
            res.bitmap_cache.retain(|p, _| bitmaps.iter().any(|(q, _)| q == p));
            res.textures = Some(loom_render::TextureSet { none, checker, bitmaps: groups });
        }
        if res.textures.is_none() {
            let (none, checker) = res.base_textures.get_or_insert_with(|| res.renderer.base_textures(device, queue)).clone();
            res.textures = Some(loom_render::TextureSet { none, checker, bitmaps: Vec::new() });
        }
        res.renderer
            .update_view(queue, self.slot, self.view_proj, self.camera_pos, self.shading);

        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::epaint::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        if let Some(res) = callback_resources.get::<ViewportResources>() {
            let Some(textures) = res.textures.as_ref() else { return };
            let refs: Vec<&MeshBuffer> = res.mesh_buffers.iter().collect();
            res.renderer.record_into_pass(
                pass,
                self.slot,
                &refs,
                &self.selected,
                self.show_edges,
                self.shaded,
                self.grid,
                self.orthographic,
                textures,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// show(): called every frame from LoomApp::update
// ---------------------------------------------------------------------------

/// Draws one viewport (index into `app.state.viewports`) in `rect` and
/// handles its input. Any button press inside the rect activates it first;
/// navigation and tools then act on the active viewport.
pub fn show_one(ui: &mut egui::Ui, app: &mut LoomApp, index: usize, rect: egui::Rect) {
    let ctx = ui.ctx().clone();
    {
            let response = ui.interact(rect, ui.id().with(("viewport", index)), egui::Sense::click_and_drag());
            let painter = ui.painter().with_clip_rect(rect);
            let ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(rect));
            if response.hovered() && ctx.input(|i| i.pointer.any_pressed()) && app.state.active_viewport != index {
                let _ = app.dispatch(Command::ActiveViewport { index: index as u32 });
            }
            let is_active = app.state.active_viewport == index;
            let disabled = app.state.viewports[index].disabled && !is_active;
            let aspect = (rect.width() / rect.height().max(1.0)).max(0.01);
            let camera = app.camera_for(index);
            let view = camera.view_matrix();
            let proj = camera.projection_matrix(aspect);
            let view_proj = proj * view;
            let projection = Projection::new(view, proj, rect);
            // Labels: [+] [Top] [Wireframe], each opening its menu.
            // Perspective views get a vertical gradient; axis views stay flat.
            // Painted first: everything else, the labels included, goes over it.
            if app.state.viewports[index].background.is_none() && !app.state.viewports[index].is_orthographic() {
                draw_background_gradient(&painter, rect);
            }
            let over_label = draw_labels(ui, app, index, rect);

            if app.state.region_points_reset {
                app.state.region_points_reset = false;
                app.region_points.clear();
            }
            // A drag whose release happened outside the window: close it so its
            // undo transaction does not absorb later edits.
            {
                let ptr = ctx.input(|i| i.pointer.clone());
                let lost = !ptr.primary_down() && !ptr.button_released(egui::PointerButton::Primary);
                if lost && app.transform_drag.is_some() {
                    app.end_transform_drag();
                    app.shift_sources.clear();
                }
                if lost && app.manip_drag.is_some() {
                    app.manip_drag = None;
                    app.state.undo.end();
                }
                // A camera viewport control drag released outside the window.
                if app.camera_nav_open && !ptr.any_down() && !ptr.any_released() {
                    app.state.undo.end();
                    app.camera_nav_open = false;
                }
            }
            // --- Mouse input ---
            let mods = ctx.input(|i| Modifiers {
                alt: i.modifiers.alt,
                ctrl: i.modifiers.ctrl,
                shift: i.modifiers.shift,
            });
            let pointer = ctx.input(|i| i.pointer.clone());
            // --- ViewCube: a click acts on release; a drag from the cube orbits the view ---
            let cube_settings = app.state.viewcube;
            let gizmo_rect = crate::viewport_gizmo::rect(rect, cube_settings.size);
            let cube_shown = crate::viewport_gizmo::shows_in(is_active, app.state.viewports[index].kind, &cube_settings);
            // Only while nothing else is being dragged across it.
            let cube_free = !app.camera_controller.is_dragging() && app.transform_drag.is_none() && app.drag_select_start.is_none() && app.manip_drag.is_none();
            let over_cube = cube_shown && cube_free && response.hovered() && pointer.hover_pos().is_some_and(|p| gizmo_rect.contains(p));
            if over_cube && pointer.button_pressed(egui::PointerButton::Primary) {
                app.cube_press = pointer.interact_pos().map(|p| (p, false));
            }
            if over_cube && pointer.button_pressed(egui::PointerButton::Secondary) {
                app.cube_menu = pointer.interact_pos().map(|p| (p, index));
            }
            let cube_busy = over_cube || (is_active && app.cube_press.is_some());
            if is_active {
                if let Some((start, mut moved)) = app.cube_press {
                    if pointer.button_down(egui::PointerButton::Primary) {
                        if !moved && pointer.interact_pos().is_some_and(|p| p.distance(start) > 3.0) {
                            moved = true;
                            app.view_animation = None;
                            app.state.record_view();
                            // Dragging an axis view makes it an Orthographic (User) view.
                            if app.state.viewports[index].kind.angles().is_some() {
                                let _ = app.dispatch(Command::ViewportView { index: index as u32, kind: loom_commands::ViewKind::Orthographic });
                            }
                        }
                        if moved {
                            let d = pointer.delta();
                            app.state.viewports[index].camera.orbit(d.x * 0.006, d.y * 0.006);
                            ctx.request_repaint();
                        }
                        app.cube_press = Some((start, moved));
                    } else {
                        app.cube_press = None;
                        if !moved {
                            let cam = app.state.viewports[index].camera.clone();
                            if let Some(hit) = crate::viewport_gizmo::hit_test(start, gizmo_rect, &cam, !app.state.viewports[index].is_orthographic()) {
                                cube_click(app, index, hit);
                                ctx.request_repaint();
                            }
                        }
                    }
                }
            }
            // Motion > Trajectories > Sub-Object Keys: a click on a path key selects it.
            let mut trajectory_click = false;
            if app.state.trajectory_keys && is_active && !cube_busy && response.clicked() {
                if let Some(p) = pointer.interact_pos() {
                    if let Some(f) = trajectory_hit(app, view_proj, rect, p) {
                        let op = if ctx.input(|i| i.modifiers.ctrl) { SelectOp::Add } else { SelectOp::Replace };
                        let _ = app.dispatch(Command::TrajectorySelectKeys { frames: vec![f], op });
                        trajectory_click = true;
                    }
                }
            }
            if let Some(p) = pointer.interact_pos().filter(|_| !trajectory_click) {
                if !cube_busy && is_active {
                    let pos = Vec2::new(p.x, p.y);

                    // Paint Soft Selection brush: LMB paints weights on the selected poly.
                    if app.state.soft_paint.is_some() && app.state.selection_mode != SelectionMode::Object {
                        if let Some(target) = app.state.selection.iter().next().copied() {
                            let down = pointer.button_down(egui::PointerButton::Primary) && (response.hovered() || app.soft_painting);
                            if pointer.button_pressed(egui::PointerButton::Primary) && response.hovered() {
                                app.soft_painting = true;
                            }
                            if down && app.soft_painting && (pointer.is_moving() || pointer.button_pressed(egui::PointerButton::Primary)) {
                                if let Some((_, point, _)) = surface_hit_node(app, p, rect, view, proj) {
                                    let b = app.soft_brush;
                                    let _ = app.dispatch(Command::PaintSoftSelectionStroke { node: target.0, points: vec![point.to_array()], value: b.value, size: b.size, strength: b.strength });
                                }
                                ctx.request_repaint();
                            }
                            if pointer.button_released(egui::PointerButton::Primary) {
                                app.soft_painting = false;
                            }
                            if let Some(hp) = pointer.hover_pos().filter(|hp| rect.contains(*hp)) {
                                // Brush circle: size in world units at the surface under the cursor.
                                if let Some((_, point, _)) = surface_hit_node(app, hp, rect, view, proj) {
                                    let upp = projection.units_per_pixel(point);
                                    painter.circle_stroke(hp, app.soft_brush.size / upp.max(1e-6), egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 200, 80)));
                                }
                            }
                            if down || app.soft_painting {
                                // The brush owns the button: no selection, no gizmo.
                                app.drag_select_start = None;
                            }
                        }
                    }
                    // Edit Soft Selection: dragging sets Falloff (Ctrl: Pinch, Shift: Bubble).
                    if app.state.soft_edit && app.state.selection_mode != SelectionMode::Object {
                        let soft = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.edit_poly().map(|p| p.soft.clone()).or_else(|| n.edit_spline().map(|s| s.soft.clone())));
                        if let Some(soft) = soft {
                            if pointer.button_down(egui::PointerButton::Primary) && response.hovered() {
                                let dx = pointer.delta().x;
                                if dx != 0.0 {
                                    let m = ctx.input(|i| i.modifiers);
                                    let (mut f, mut pi, mut bu) = (None, None, None);
                                    if m.ctrl {
                                        pi = Some(soft.pinch.get() + dx * 0.01);
                                    } else if m.shift {
                                        bu = Some(soft.bubble.get() + dx * 0.01);
                                    } else {
                                        let at = app.state.selection.iter().next().map(|id| app.state.scene.object_matrix(*id).w_axis.truncate()).unwrap_or_default();
                                        let upp = projection.units_per_pixel(at);
                                        f = Some((soft.falloff.get() + dx * upp).max(0.0));
                                    }
                                    let _ = app.dispatch(Command::SoftSelectionSettings { falloff: f, pinch: pi, bubble: bu, edge_distance: None, affect_backfacing: None });
                                    ctx.request_repaint();
                                }
                                app.drag_select_start = None;
                            }
                        }
                    }
                    // Extrude / Chamfer mode: the drag sets the value with a live preview.
                    if crate::poly_modes::drag(app, &ctx, &pointer, response.hovered(), p, &projection) {
                        app.drag_select_start = None;
                    }
                    // Editable Spline: dragging a Bezier handle square.
                    if crate::spline_modes::handle_drag(app, &ctx, &pointer, response.hovered(), p, &projection) {
                        app.drag_select_start = None;
                    }
                    // VertexPaint brush: LMB paints the active VertexPaint entry's colour.
                    if app.state.vertex_paint_brush {
                        if let Some((target, brush)) = app.state.selection.iter().next().and_then(|id| {
                            let n = app.state.scene.node(*id)?;
                            match &n.modifiers.get(n.active_entry?)?.modifier {
                                loom_scene::Modifier::VertexPaint { brush_size, opacity, color, .. } => Some((*id, (brush_size.get(), opacity.get(), [color[0].get(), color[1].get(), color[2].get()]))),
                                _ => None,
                            }
                        }) {
                            let down = pointer.button_down(egui::PointerButton::Primary) && (response.hovered() || app.deform_painting);
                            if pointer.button_pressed(egui::PointerButton::Primary) && response.hovered() {
                                app.deform_painting = true;
                                app.poly_dragging = true;
                                app.state.undo.begin("Vertex Paint");
                            }
                            if down && app.deform_painting && (pointer.is_moving() || pointer.button_pressed(egui::PointerButton::Primary)) {
                                if let Some((_, point, _)) = surface_hit_node(app, p, rect, view, proj) {
                                    let local = app.state.scene.object_matrix(target).inverse().transform_point3(point);
                                    let rgb = [(brush.2[0] * 255.0) as u8, (brush.2[1] * 255.0) as u8, (brush.2[2] * 255.0) as u8];
                                    let _ = app.dispatch(Command::VertexPaintStroke { node: target.0, points: vec![local.to_array()], rgb, size: brush.0, opacity: brush.1 });
                                }
                                ctx.request_repaint();
                            }
                            if pointer.button_released(egui::PointerButton::Primary) && app.deform_painting {
                                app.deform_painting = false;
                                app.poly_dragging = false;
                                app.state.undo.end();
                            }
                            if let Some(hp) = pointer.hover_pos().filter(|hp| rect.contains(*hp)) {
                                if let Some((_, point, _)) = surface_hit_node(app, hp, rect, view, proj) {
                                    let upp = projection.units_per_pixel(point);
                                    painter.circle_stroke(hp, brush.0 / upp.max(1e-6), egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 120, 200)));
                                }
                            }
                            if down || app.deform_painting {
                                app.drag_select_start = None;
                            }
                        }
                    }
                    // Skin > Paint Weights brush.
                    crate::poly_modes::skin_paint(app, &ctx, &painter, &pointer, response.hovered(), p, rect, view, proj, &projection);
                    // Paint Deformation brush: LMB paints on the selected poly (Ctrl inverts, Shift relaxes).
                    if let Some(mode) = app.state.paint_deform {
                        if let Some(target) = app.state.selection.iter().next().copied() {
                            let down = pointer.button_down(egui::PointerButton::Primary) && (response.hovered() || app.deform_painting);
                            if pointer.button_pressed(egui::PointerButton::Primary) && response.hovered() {
                                app.deform_painting = true;
                                app.poly_dragging = true;
                                // One stroke = one undo entry.
                                app.state.undo.begin("Paint Deform");
                            }
                            if down && app.deform_painting && (pointer.is_moving() || pointer.button_pressed(egui::PointerButton::Primary)) {
                                if let Some((_, point, _)) = surface_hit_node(app, p, rect, view, proj) {
                                    let mods = ctx.input(|i| i.modifiers);
                                    let mode = if mods.shift { loom_commands::PaintMode::Relax } else { mode };
                                    let _ = app.dispatch(Command::PaintDeformStroke { node: target.0, points: vec![point.to_array()], mode, invert: mods.ctrl });
                                }
                                ctx.request_repaint();
                            }
                            if pointer.button_released(egui::PointerButton::Primary) && app.deform_painting {
                                app.deform_painting = false;
                                app.poly_dragging = false;
                                app.state.undo.end();
                            }
                            if let Some(hp) = pointer.hover_pos().filter(|hp| rect.contains(*hp)) {
                                if let Some((_, point, _)) = surface_hit_node(app, hp, rect, view, proj) {
                                    let size = app.state.scene.node(target).and_then(|n| n.edit_poly().map(|p| p.paint.brush_size)).unwrap_or(20.0);
                                    let upp = projection.units_per_pixel(point);
                                    painter.circle_stroke(hp, size / upp.max(1e-6), egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(80, 200, 255)));
                                }
                            }
                            if down || app.deform_painting {
                                app.drag_select_start = None;
                            }
                        }
                    }
                    // Select and Manipulate: radius / height handles of primitives.
                    if app.state.manipulate && pointer.button_pressed(egui::PointerButton::Primary) && response.hovered() && app.transform_drag.is_none() {
                        if let Some(m) = manipulators(app).into_iter().find(|m| projection.screen(m.handle).is_some_and(|sp| sp.distance(p) <= 8.0)) {
                            app.manip_drag = Some(crate::ManipDrag { id: m.id, name: m.name, last: p, origin: m.origin, dir: m.dir });
                            app.state.undo.begin(m.name);
                        }
                    }
                    if pointer.button_released(egui::PointerButton::Primary) && app.manip_drag.is_some() {
                        app.manip_drag = None;
                        app.state.undo.end();
                    }
                    if let Some(md) = app.manip_drag.clone() {
                        if pointer.is_moving() {
                            // Change by the pointer movement along the handle direction, in world units.
                            if let (Some(a), Some(b)) = (projection.hit_plane(md.last, md.origin, projection.toward_viewer()), projection.hit_plane(p, md.origin, projection.toward_viewer())) {
                                let delta = (b - a).dot(md.dir);
                                if delta.abs() > 0.0 {
                                    let _ = app.dispatch(Command::Manipulate { id: md.id.0, name: md.name.to_owned(), delta: delta as f64 });
                                }
                            }
                            if let Some(md) = app.manip_drag.as_mut() {
                                md.last = p;
                            }
                            ctx.request_repaint();
                        }
                    }
                    if pointer.button_pressed(egui::PointerButton::Primary)
                        && response.hovered()
                        && app.manip_drag.is_none()
                        && app.state.active_tool != Tool::Select
                        && app.has_selection()
                    {
                        let layout = gizmo::layout_for(app, index, rect.height());
                        if let Some(handle) = gizmo::hit(app, &layout, &projection, p) {
                            let mut drag = DragState::new(handle, p);
                            drag.shift = mods.shift;
                            app.begin_transform_drag(drag);
                            if mods.shift {
                                app.shift_sources = app.state.selection.iter().copied().collect();
                                // Shift+drag: the original stays, a Copy follows the pointer.
                                let _ = app.dispatch(Command::CloneOptions { kind: loom_commands::CloneKind::Copy, copies: 1, name: None, offset: None, rotate_deg: None, scale: None });
                            }
                            set_constraint_from_handle(app, handle);
                        }
                    }
                    if pointer.button_released(egui::PointerButton::Primary) {
                        if let Some(drag) = app.transform_drag {
                            app.end_transform_drag();
                            app.consume_snap_override();
                            if drag.shift {
                                app.shift_clone = Some(crate::ShiftClone { tool: app.state.active_tool, sources: std::mem::take(&mut app.shift_sources), offset: drag.offset, rotate_deg: drag.rotate_deg, quat: drag.quat, scale: drag.factors, undo_len: app.state.undo.len() });
                                let _ = app.dispatch(Command::Clone);
                            }
                        }
                    }
                    if let Some(mut drag) = app.transform_drag {
                        if pointer.is_moving() {
                            let layout = gizmo::layout_for(app, index, rect.height());
                            if drag.snap_start.is_none() {
                                drag.snap_start = Some(layout.origin);
                            }
                            gizmo::drag(app, &mut drag, &layout, &projection, p, index);
                            app.transform_drag = Some(drag);
                            ctx.request_repaint();
                        }
                    }

                    // Left mouse button:
                    //   Alt+LMB → click subtracts from the selection, drag orbits like Alt+MMB.
                    //   LMB without Alt → click-pick on release if drag was minimal,
                    //                     marquee area-select otherwise.
                    if let Some(start) = app.alt_click {
                        if pointer.button_down(egui::PointerButton::Primary) && start.distance(p) >= 4.0 {
                            app.alt_click = None;
                            if !mods.ctrl && app.state.viewports[index].kind.angles().is_some() {
                                let _ = app.dispatch(Command::ViewportView { index: index as u32, kind: loom_commands::ViewKind::Orthographic });
                            }
                            app.state.record_view();
                            nav_event(app, index, rect, InputEvent::MouseDown { button: MouseButton::Left, pos: Vec2::new(start.x, start.y), mods });
                        }
                    }
                    if app.transform_drag.is_none()
                        && app.manip_drag.is_none()
                        && pointer.button_pressed(egui::PointerButton::Primary)
                        && response.hovered()
                        && mods.alt
                        && app.state.nav_mode == loom_input::NavMode::None
                        && app.create.is_none()
                    {
                        app.alt_click = Some(p);
                    } else if app.transform_drag.is_none()
                        && app.manip_drag.is_none()
                        && pointer.button_pressed(egui::PointerButton::Primary)
                        && response.hovered()
                    {
                        if app.state.nav_mode != loom_input::NavMode::None {
                            app.state.record_view();
                        }
                        // Orbit Selected / SubObject: orbit around the selection,
                        // keeping the eye where it is.
                        if matches!(app.state.nav_mode, loom_input::NavMode::Orbit | loom_input::NavMode::OrbitSelected | loom_input::NavMode::OrbitSubObject)
                            && app.state.viewports[index].kind.angles().is_some()
                        {
                            let _ = app.dispatch(Command::ViewportView { index: index as u32, kind: loom_commands::ViewKind::Orthographic });
                        }
                        if matches!(app.state.nav_mode, loom_input::NavMode::OrbitSelected | loom_input::NavMode::OrbitSubObject) && app.has_selection() {
                            let pivot = app.selection_pivot();
                            let cam = &mut app.state.active_mut().camera;
                            let eye = cam.eye_position();
                            cam.target = pivot;
                            cam.distance = eye.distance(pivot).max(0.05);
                        }
                        nav_event(app, index, rect, InputEvent::MouseDown {
                                button: MouseButton::Left,
                                pos,
                                mods,
                            });
                        let nav = app.state.nav_mode;
                        if nav != loom_input::NavMode::None && nav != loom_input::NavMode::ZoomRegion {
                            // Modal navigation tool: the camera controller owns the drag.
                        } else if nav == loom_input::NavMode::ZoomRegion {
                            app.drag_select_start = Some(p);
                            app.region_points.clear();
                        } else if !mods.alt {
                            if let Some(session) = app.create.as_mut() {
                                // Click during a height/radius step confirms it.
                                let mut session = session.clone();
                                if session.on_click(app, p) {
                                    app.create = None;
                                } else {
                                    app.create = Some(session);
                                }
                            } else if app.create_kind == Some(crate::CreateKind::Bones) {
                                // Bones tool: each click is a joint; the right-click builds the chain.
                                if let Some((mut anchor, _basis)) = creation_anchor(app, index, p, rect, view, proj) {
                                    if let Some(hit) = snap_at(app, index, &projection, p, None, false) {
                                        anchor = hit.point;
                                        app.consume_snap_override();
                                    }
                                    app.bone_points.push(anchor);
                                }
                            } else if let Some(crate::CreateKind::Helper(kind)) = app.create_kind {
                                if let Some((mut anchor, _basis)) = creation_anchor(app, index, p, rect, view, proj) {
                                    if let Some(hit) = snap_at(app, index, &projection, p, None, false) {
                                        anchor = hit.point;
                                        app.consume_snap_override();
                                    }
                                    let _ = app.dispatch(Command::CreateHelper { kind: kind.api_name().into(), position: anchor.to_array(), size: kind.default_size() });
                                }
                            } else if app.create_kind == Some(crate::CreateKind::Biped) {
                                // Biped: the press puts the feet down, the drag sets the height.
                                if let Some((mut anchor, basis)) = creation_anchor(app, index, p, rect, view, proj) {
                                    if let Some(hit) = snap_at(app, index, &projection, p, None, false) {
                                        anchor = hit.point;
                                        app.consume_snap_override();
                                    }
                                    let upp = units_per_pixel(app, index, rect);
                                    app.biped_create = crate::create_tool::BipedSession::begin(app, anchor, basis.2, p, upp);
                                }
                            } else if let Some(crate::CreateKind::Camera { target }) = app.create_kind {
                                if let Some((mut anchor, basis)) = creation_anchor(app, index, p, rect, view, proj) {
                                    if let Some(hit) = snap_at(app, index, &projection, p, None, false) {
                                        anchor = hit.point;
                                        app.consume_snap_override();
                                    }
                                    if target {
                                        app.camera_create = crate::create_tool::CameraSession::begin(app, anchor, basis.2);
                                    } else {
                                        crate::create_tool::CameraSession::place_free(app, anchor, basis.2);
                                    }
                                }
                            } else if app.create_kind == Some(crate::CreateKind::Line) {
                                // Line tool: every click is a knot on the construction plane.
                                if let Some((mut anchor, basis)) = creation_anchor(app, index, p, rect, view, proj) {
                                    if let Some(hit) = snap_at(app, index, &projection, p, None, false) {
                                        anchor = hit.point;
                                        app.consume_snap_override();
                                    }
                                    match app.line.take() {
                                        None => app.line = crate::create_tool::LineSession::begin(app, anchor, basis.2),
                                        Some(mut line) => {
                                            let first = line.first_knot_screen(app, &projection);
                                            if !line.on_press(app, anchor, p, first) {
                                                app.line = Some(line);
                                            }
                                        }
                                    }
                                }
                            } else if let Some(kind) = app.create_kind {
                                if let Some((mut anchor, basis)) = creation_anchor(app, index, p, rect, view, proj) {
                                    if let Some(hit) = snap_at(app, index, &projection, p, None, false) {
                                        anchor = hit.point;
                                        app.consume_snap_override();
                                    }
                                    let upp = units_per_pixel(app, index, rect);
                                    app.create = crate::create_tool::CreateSession::begin(app, kind, anchor, basis, p, upp);
                                }
                            } else if app.state.selection_region == loom_commands::SelectionRegion::Fence && app.state.pick_mode.is_none() {
                                // Fence: each click adds a corner; clicking near the first corner closes it.
                                let close = app.region_points.len() >= 3 && app.region_points[0].distance(p) < 8.0;
                                if close {
                                    let region = drag_region(app, p, p);
                                    let items = region_select(app, &region, rect, view_proj);
                                    apply_picks(app, items, mods);
                                    app.region_points.clear();
                                } else {
                                    app.region_points.push(p);
                                }
                            } else if !app.poly_dragging {
                                app.drag_select_start = Some(p);
                                app.region_points.clear();
                                app.region_points.push(p);
                            }
                        }
                    }
                    if pointer.button_released(egui::PointerButton::Primary) {
                        nav_event(app, index, rect, InputEvent::MouseUp {
                                button: MouseButton::Left,
                                pos,
                                mods,
                            });
                        if let Some(line) = app.line.as_mut() {
                            line.on_release();
                        }
                        if let Some(session) = app.camera_create.take() {
                            session.finish(app);
                        }
                        if let Some(session) = app.biped_create.take() {
                            let hit = plane_hit(p, rect, view, proj, session.normal);
                            session.finish(app, hit, p);
                        }
                        if app.alt_click.take().is_some() {
                            // Alt+click removes: Skin vertices at Edit Envelopes, else the picked items.
                            if !(app.state.pick_mode.is_none() && crate::poly_modes::skin_pick(app, &projection, None, p, mods)) {
                                let picked = picks_at(app, p, rect, view, proj, view_proj);
                                apply_picks(app, picked, mods);
                            }
                        } else if app.transform_drag.is_none() {
                            if let Some(mut session) = app.create.take() {
                                if !session.on_release(app, p) {
                                    app.create = Some(session);
                                }
                            } else if let Some(start) = app.drag_select_start.filter(|_| app.state.nav_mode == loom_input::NavMode::ZoomRegion) {
                                app.drag_select_start = None;
                                let region = egui::Rect::from_two_pos(start, p);
                                if region.width() > 4.0 && region.height() > 4.0 {
                                    let centre_hit = ground_hit(region.center(), rect, view, proj);
                                    let factor = (region.height() / rect.height()).max(region.width() / rect.width()).clamp(0.01, 1.0);
                                    let cam = &mut app.state.active_mut().camera;
                                    if let Some(c) = centre_hit {
                                        cam.target = c;
                                    }
                                    cam.distance = (cam.distance * factor).max(0.05);
                                }
                            } else if let Some(start) = app.drag_select_start.take() {
                                let drag_px = (start - p).length();
                                if drag_px < 4.0 {
                                    if matches!(app.state.pick_mode, Some(loom_commands::PickMode::SpacingPointA | loom_commands::PickMode::SpacingPointB)) {
                                        // Spacing Tool > Pick Points: the snap point, else the construction plane.
                                        let point = snap_at(app, index, &projection, p, None, false).map(|h| h.point).or_else(|| creation_anchor(app, index, p, rect, view, proj).map(|(a, _)| a));
                                        if let Some(pt) = point {
                                            let _ = app.dispatch(Command::PickPoint { point: pt.to_array() });
                                        }
                                    } else if matches!(app.state.pick_mode, Some(loom_commands::PickMode::SeamPointA | loom_commands::PickMode::SeamPointB)) {
                                        // Point To Point Seam: the nearest vertex of the Unwrap object.
                                        if let Some(Pick::SubObject { mesh, id: SubObjectId::Vertex(v) }) = pick_vertex(app, p, rect, view_proj) {
                                            let node = app.world_meshes()[mesh].0;
                                            let _ = app.dispatch(Command::SeamPoint { node: node.0, vertex: v.0 });
                                        }
                                    } else if app.state.pick_mode.is_none() && app.state.uv_editor.edit_seams && app.state.unwrap_entry().is_some() {
                                        // Edit Seams: a click on an edge toggles it.
                                        if let Some(Pick::SubObject { mesh, id: SubObjectId::Edge(e) }) = pick_edge(app, p, rect, view_proj) {
                                            let node = app.world_meshes()[mesh].0;
                                            let _ = app.dispatch(Command::ToggleSeamEdge { node: node.0, edge: e.0 });
                                        }
                                    } else if matches!(app.state.pick_mode, Some(loom_commands::PickMode::NormalAlignSource | loom_commands::PickMode::NormalAlignTarget | loom_commands::PickMode::AlignCamera | loom_commands::PickMode::UvwMapNormal | loom_commands::PickMode::UvwMapRegionA | loom_commands::PickMode::UvwMapRegionB)) {
                                        if let Some((id, point, normal)) = surface_hit_node(app, p, rect, view, proj) {
                                            let _ = app.dispatch(Command::PickSurface { id: id.0, point: point.to_array(), normal: normal.to_array() });
                                        }
                                    } else if crate::spline_modes::is_spline_pick(app.state.pick_mode) {
                                        crate::spline_modes::click(app, p, rect, view_proj);
                                    } else if crate::poly_modes::is_poly_pick(app.state.pick_mode) {
                                        crate::poly_modes::click(app, index, p, rect, view, proj);
                                    } else if app.state.pick_mode.is_none() && crate::poly_modes::skin_pick(app, &projection, None, p, mods) {
                                    } else if app.state.pick_mode.is_none() && crate::poly_modes::ffd_pick(app, &projection, None, p, mods) {
                                    } else if app.state.pick_mode.is_some() {
                                        if let Some(Pick::Object(i)) = pick_object(app, p, rect, view, proj) {
                                            let id = app.world_meshes()[i].0;
                                            let _ = app.dispatch(Command::PickHit { id: id.0 });
                                        }
                                    } else {
                                        let double = pointer.button_double_clicked(egui::PointerButton::Primary);
                                        if !(double && double_click(app, p, rect, view, proj, view_proj, mods)) {
                                            let mut picked = picks_at(app, p, rect, view, proj, view_proj);
                                            cycle_object_pick(app, p, rect, view, proj, &mut picked, double);
                                            apply_picks(app, picked, mods);
                                        }
                                    }
                                } else {
                                    let region = drag_region(app, start, p);
                                    if !crate::poly_modes::skin_pick(app, &projection, Some(&region), p, mods) && !crate::poly_modes::ffd_pick(app, &projection, Some(&region), p, mods) {
                                        // Auto Window/Crossing by Direction: left-to-right = Window, right-to-left = Crossing.
                                        let window = if app.state.prefs.auto_window_crossing { p.x >= start.x } else { app.state.window_selection };
                                        let items = region_select_mode(app, &region, rect, view_proj, window);
                                        apply_picks(app, items, mods);
                                    }
                                }
                                app.region_points.clear();
                            }
                        }
                    }
                    if pointer.is_moving()
                        && app.drag_select_start.is_some()
                        && matches!(app.state.selection_region, loom_commands::SelectionRegion::Lasso | loom_commands::SelectionRegion::Paint)
                        && app.region_points.last().is_none_or(|l| l.distance(p) > 3.0)
                    {
                        app.region_points.push(p);
                    }
                    if pointer.is_moving() && pointer.button_down(egui::PointerButton::Primary) {
                        if let Some(mut line) = app.line.take() {
                            let hit = plane_hit(p, rect, view, proj, line.normal);
                            line.on_drag(app, hit, p);
                            app.line = Some(line);
                            ctx.request_repaint();
                        }
                    }
                    if pointer.is_moving() {
                        if let Some(mut session) = app.biped_create.take() {
                            let hit = plane_hit(p, rect, view, proj, session.normal);
                            session.on_drag(app, hit, p);
                            app.biped_create = Some(session);
                            ctx.request_repaint();
                        }
                        if let Some(session) = app.camera_create.clone() {
                            let mut hit = plane_hit(p, rect, view, proj, session.normal);
                            if let Some(snap) = snap_at(app, index, &projection, p, None, false) {
                                hit = Some(snap.point);
                            }
                            if let Some(at) = hit {
                                session.on_drag(app, at);
                            }
                            ctx.request_repaint();
                        }
                        if let Some(mut session) = app.create.take() {
                            let mut hit = plane_hit(p, rect, view, proj, session.normal());
                            if let Some(snap) = snap_at(app, index, &projection, p, Some(session.anchor), false) {
                                hit = Some(snap.point);
                            }
                            session.on_move(app, hit, p);
                            app.create = Some(session);
                            ctx.request_repaint();
                        }
                    }
                    // Right click cancels a creation in progress or a by-Hit mode;
                    // otherwise it opens the quad menu (on the click).
                    if pointer.button_pressed(egui::PointerButton::Secondary) && response.hovered() && !over_label && !over_cube {
                        if let Some(session) = app.create.take() {
                            session.cancel(app);
                        } else if app.create_kind.is_some() && app.bone_points.is_empty() && app.line.is_none() {
                            // A right-click with nothing being dragged out ends the create mode.
                            app.create_kind = None;
                        } else if let Some(line) = app.line.take() {
                            line.finish(app);
                        } else if !app.bone_points.is_empty() {
                            let points: Vec<[f32; 3]> = app.bone_points.drain(..).map(|p| p.to_array()).collect();
                            if points.len() >= 2 {
                                let _ = app.dispatch(Command::CreateBoneChain { points, name: None });
                            }
                        } else if loom_commands::skin::painting(&app.state) {
                            // Right-click ends Paint Weights.
                            let _ = app.dispatch(Command::SkinPaintWeightsToggle);
                        } else if app.state.pick_mode.is_some() {
                            let _ = app.dispatch(Command::ExitPickMode);
                        } else if app.state.nav_mode != loom_input::NavMode::None {
                            app.state.nav_mode = loom_input::NavMode::None;
                        } else if !app.region_points.is_empty() && app.drag_select_start.is_none() {
                            app.region_points.clear();
                        } else {
                            // A right-click elsewhere while one is open reopens it there.
                            let mut open = crate::quad_menu::Open::new(p, index);
                            open.kind = if mods.shift { crate::quad_menu::QuadKind::Snaps } else if mods.ctrl { crate::quad_menu::QuadKind::Create } else if mods.alt { crate::quad_menu::QuadKind::Animation } else { crate::quad_menu::QuadKind::Default };
                            app.quad_menu = Some(open);
                        }
                    }
                    // Middle mouse button = pan; Alt+MMB orbits, which turns an
                    // axis view into an Orthographic (User) view.
                    if pointer.button_pressed(egui::PointerButton::Middle) && response.hovered() {
                        if mods.alt && !mods.ctrl && app.state.viewports[index].kind.angles().is_some() {
                            let _ = app.dispatch(Command::ViewportView { index: index as u32, kind: loom_commands::ViewKind::Orthographic });
                        }
                        app.state.record_view();
                        nav_event(app, index, rect, InputEvent::MouseDown {
                                button: MouseButton::Middle,
                                pos,
                                mods,
                            });
                    }
                    if pointer.button_released(egui::PointerButton::Middle) {
                        nav_event(app, index, rect, InputEvent::MouseUp {
                                button: MouseButton::Middle,
                                pos,
                                mods,
                            });
                    }
                    // MouseMove fires every frame the pointer moves; the controller's internal
                    // `dragging` state decides whether to act on it.
                    if pointer.is_moving() {
                        let before = app.state.active().camera.distance;
                        nav_event(app, index, rect, InputEvent::MouseMove { pos, mods });
                        let after = app.state.active().camera.distance;
                        if app.camera_controller.is_dragging() && (after - before).abs() > 0.0 {
                            match app.state.nav_mode {
                                loom_input::NavMode::Fov => {
                                    // FOV tool: the drag changes the field of view, not the distance.
                                    let vp = app.state.active_mut();
                                    vp.camera.distance = before;
                                    let deg = (vp.fov_y_radians.to_degrees() * (after / before)).clamp(1.0, 175.0);
                                    vp.fov_y_radians = deg.to_radians();
                                }
                                loom_input::NavMode::ZoomAll => {
                                    let factor = after / before;
                                    let active = app.state.active_viewport;
                                    // Shift leaves the Perspective views out.
                                    for (i, vp) in app.state.viewports.iter_mut().enumerate() {
                                        if i != active && !(mods.shift && vp.kind == loom_commands::ViewKind::Perspective) {
                                            vp.camera.distance = (vp.camera.distance * factor).max(0.05);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            let scroll = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 && response.hovered() && !disabled {
                let delta = scroll / 50.0;
                nav_event(app, index, rect, InputEvent::Scroll {
                        delta_y: delta,
                        pos: Vec2::ZERO,
                        mods,
                    });
            }

            // --- Build callback and enqueue ---
            let camera_pos = app.state.viewports[index].camera.eye_position();
            let selected: Vec<usize> = app
                .world_meshes()
                .iter()
                .enumerate()
                .filter(|(_, (id, _))| app.state.selection.contains(id))
                .map(|(i, _)| i)
                .collect();

            let (wire, grid_plane, edged_faces, show_grid, view_shading) = {
                let vp = &app.state.viewports[index];
                let grid_plane = match vp.kind {
                    loom_commands::ViewKind::Front | loom_commands::ViewKind::Back => loom_render::GridPlane::XZ,
                    loom_commands::ViewKind::Left | loom_commands::ViewKind::Right => loom_render::GridPlane::YZ,
                    _ => loom_render::GridPlane::XY,
                };
                let lighting = match vp.shading {
                    loom_commands::Shading::Flat => 1,
                    _ if vp.scene_lights => 2,
                    _ => 0,
                };
                // Hidden Line: faces in the viewport colour hide the lines behind them.
                let override_color = (vp.shading == loom_commands::Shading::HiddenLine).then_some([0.0806, 0.0806, 0.0806]);
                let view_shading = loom_render::ViewShading { lighting, override_color };
                let edges = vp.edged_faces || vp.shading == loom_commands::Shading::HiddenLine;
                (vp.shading == loom_commands::Shading::Wireframe, grid_plane, edges, vp.show_grid, view_shading)
            };
            // Viewport background image: stretched to the viewport, or, with
            // Lock Zoom/Pan, pinned to a 10-unit square on the construction plane.
            if let Some(path) = app.state.viewports[index].background.clone() {
                let version = app.state.viewports[index].background_version;
                let locked = app.state.viewports[index].background_locked;
                if let Some(tex) = app.background_texture(&ctx, &path, version) {
                    let target = if locked {
                        let corners = [Vec3::new(-5.0, -5.0, 0.0), Vec3::new(5.0, 5.0, 0.0)];
                        match (project_world_to_screen(corners[0], view_proj, rect), project_world_to_screen(corners[1], view_proj, rect)) {
                            (Some(a), Some(b)) => egui::Rect::from_two_pos(a, b),
                            _ => rect,
                        }
                    } else {
                        rect
                    };
                    painter.image(tex.id(), target, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                }
            }
            // Realistic: Bevy's image is the 3D view (no grid); without a GPU
            // (the headless app) the viewport falls back to the shaded view.
            let realistic = app.state.viewports[index].shading == loom_commands::Shading::Realistic && app.realistic_available();
            if realistic {
                let ppp = ctx.pixels_per_point();
                let size = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
                match app.realistic_texture(index, size, view, proj) {
                    Some(tex) => {
                        painter.image(tex, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                    }
                    None => ctx.request_repaint(),
                }
            }
            let cb = (!realistic).then(|| egui_wgpu::Callback::new_paint_callback(
                rect,
                ViewportCallback {
                    slot: index,
                    view_proj,
                    camera_pos,
                    upload: app.pending_upload.clone(),
                    shading: view_shading,
                    selected,
                    show_edges: edged_faces || wire || app.state.selection_mode.is_edge_level(),
                    shaded: !wire,
                    grid: show_grid.then_some(grid_plane),
                    orthographic: app.state.viewports[index].is_orthographic(),
                    grid_params: {
                        let g = &app.state.home_grid;
                        let cells = (g.perspective_extent.max(1) * g.major_every.max(1)) as i32;
                        loom_render::GridParams { spacing: g.spacing, major_every: g.major_every, cells, ortho_cells: cells * 10 }
                    },
                },
            ));
            if let Some(cb) = cb {
                painter.add(cb);
            }

            let hover_pos = ctx.input(|i| i.pointer.hover_pos());
            draw_shapes(&painter, app, view_proj, rect);
            crate::camera_overlay::draw(&painter, app, index, view_proj, rect);
            crate::spline_modes::draw(&painter, app, &projection);
            draw_mode_overlays(&painter, app, view_proj, rect);
            draw_soft_selection(&painter, app, view_proj, rect);
            draw_subobject_overlay(&painter, app, view_proj, rect);
            // Preview Selection: the sub-object under the cursor glows yellow.
            if is_active && app.state.preview_selection != loom_commands::PreviewSelect::Off && app.state.selection_mode != SelectionMode::Object && app.transform_drag.is_none() {
                if let Some(hp) = hover_pos.filter(|hp| rect.contains(*hp)) {
                    if let Some(Pick::SubObject { mesh, id }) = pick_at(app, hp, rect, view, proj, view_proj) {
                        if let Some((_, m)) = app.world_meshes().get(mesh) {
                            draw_sub_object(&painter, m, id, view_proj, rect, egui::Color32::YELLOW);
                        }
                    }
                }
            }
            // Show Cage: the base mesh edges over the NURMS result.
            if app.state.selection_mode != SelectionMode::Object {
                let cage = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 160, 40));
                for id in app.state.selection.iter() {
                    let show = app.state.scene.node(*id).and_then(|n| n.edit_poly().map(|p| p.subdiv.use_nurms.get() && p.subdiv.show_cage)).unwrap_or(false);
                    if !show {
                        continue;
                    }
                    if let Some((_, m)) = app.world_meshes().iter().find(|(n, _)| n == id) {
                        for (i, _) in m.half_edges().iter().enumerate() {
                            let e = HalfEdgeId(i as u32);
                            if m.canonical_edge_id(e) == Some(e) {
                                draw_edge_overlay(&painter, m, e, view_proj, rect, cage);
                            }
                        }
                    }
                }
            }
            if app.state.selection_brackets && app.state.selection_mode == SelectionMode::Object {
                draw_selection_brackets(&painter, app, view_proj, rect);
                // Bones tool preview: the joints clicked so far.
                if !app.bone_points.is_empty() {
                    let stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(230, 200, 90));
                    let pts: Vec<Option<egui::Pos2>> = app.bone_points.iter().map(|w| project_world_to_screen(*w, view_proj, rect)).collect();
                    for w in pts.windows(2) {
                        if let (Some(a), Some(b)) = (w[0], w[1]) {
                            painter.line_segment([a, b], stroke);
                        }
                    }
                    for p in pts.iter().flatten() {
                        painter.circle_filled(*p, 3.0, egui::Color32::from_rgb(230, 200, 90));
                    }
                }
            }
            if app.state.viewports[index].safe_frames {
                draw_safe_frames(&painter, rect);
            }
            if app.state.show_trajectories || app.motion_trajectories || app.state.trajectory_keys {
                draw_trajectories(&painter, app, view_proj, rect);
            }
            if is_active && app.has_selection() {
                let layout = gizmo::layout_for(app, index, rect.height());
                let hovered = hover_pos.and_then(|p| gizmo::hit(app, &layout, &projection, p));
                gizmo::draw(&painter, app, &layout, &projection, hovered, app.transform_drag.as_ref());
            }
            if is_active {
                crate::poly_modes::draw(&painter, app, &projection, hover_pos.filter(|h| rect.contains(*h)));
            }
            if app.state.manipulate && is_active {
                for m in manipulators(app) {
                    draw_manipulator(&painter, &projection, &m, hover_pos, app.manip_drag.as_ref().is_some_and(|d| d.id == m.id && d.name == m.name));
                }
            }
            if app.state.prefs.show_cross_hair && is_active {
                if let Some(hp) = hover_pos.filter(|hp| rect.contains(*hp)) {
                    let c = egui::Color32::from_gray(200);
                    painter.line_segment([egui::pos2(rect.left(), hp.y), egui::pos2(rect.right(), hp.y)], egui::Stroke::new(1.0_f32, c));
                    painter.line_segment([egui::pos2(hp.x, rect.top()), egui::pos2(hp.x, rect.bottom())], egui::Stroke::new(1.0_f32, c));
                }
            }
            // Snap marker: while creating, or with Move active (dragging or hovering).
            if is_active && app.state.snaps_on && rect.contains(hover_pos.unwrap_or(egui::pos2(-1.0, -1.0))) {
                let hp = hover_pos.unwrap();
                let dragging = app.transform_drag.is_some() && app.state.active_tool == Tool::Move;
                let creating = app.create.is_some() || app.create_kind.is_some();
                if dragging || creating || app.state.active_tool == Tool::Move {
                    let start = app.transform_drag.as_ref().and_then(|d| d.snap_start).or_else(|| app.create.as_ref().map(|c| c.anchor));
                    if let Some(hit) = snap_preview(app, index, &projection, hp, start, dragging) {
                        let from = if dragging || app.create.is_some() { start.and_then(|w| projection.screen(w)) } else { None };
                        draw_snap_marker(&painter, app, &hit, from);
                    }
                }
            }
            if cube_shown {
                let cam = app.state.viewports[index].camera.clone();
                let perspective = !app.state.viewports[index].is_orthographic();
                let hovered = (over_cube || (is_active && app.cube_press.is_some())).then(|| hover_pos.and_then(|p| crate::viewport_gizmo::hit_test(p, gizmo_rect, &cam, perspective)));
                crate::viewport_gizmo::draw(&painter, gizmo_rect, &cam, perspective, hovered, &cube_settings);
            }
            // The Orbit trackball: a yellow circle with handles at its quadrant points.
            let orbit_mode = matches!(app.state.nav_mode, loom_input::NavMode::Orbit | loom_input::NavMode::OrbitSelected | loom_input::NavMode::OrbitSubObject);
            if is_active && orbit_mode && !matches!(app.state.viewports[index].kind, loom_commands::ViewKind::Camera(_)) {
                let (c, r) = (rect.center(), trackball_radius(rect));
                let yellow = egui::Color32::from_rgb(230, 210, 40);
                painter.circle_stroke(c, r, egui::Stroke::new(1.5_f32, yellow));
                for d in [egui::vec2(-r, 0.0), egui::vec2(r, 0.0), egui::vec2(0.0, -r), egui::vec2(0.0, r)] {
                    painter.rect_stroke(egui::Rect::from_center_size(c + d, egui::vec2(7.0, 7.0)), 0.0, egui::Stroke::new(1.5_f32, yellow), egui::StrokeKind::Middle);
                }
            }
            if app.state.viewports[index].clipping {
                clip_slider(ui, &painter, app, index, rect);
            }

            // --- Marquee overlay (drawn after the 3D scene) ---
            if let Some(start) = app.drag_select_start.filter(|_| is_active) {
                if let Some(end) = pointer.interact_pos() {
                    draw_region(&painter, &drag_region(app, start, end));
                }
                ctx.request_repaint(); // keep redrawing while dragging
            } else if is_active && app.state.selection_region == loom_commands::SelectionRegion::Fence && !app.region_points.is_empty() {
                let mut pts = app.region_points.clone();
                if let Some(h) = pointer.hover_pos() {
                    pts.push(h);
                }
                painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(180))));
            }
            // The ViewCube's right-click menu.
            if let Some((at, _)) = app.cube_menu.filter(|(_, i)| *i == index) {
                let entries = crate::label_menus::viewcube_entries(app, index);
                let area = egui::Area::new(egui::Id::new("viewcube_menu")).fixed_pos(at).order(egui::Order::Foreground).show(&ctx, |ui| {
                    egui::Frame::menu(ui.style()).show(ui, |ui| {
                        ui.set_min_width(190.0);
                        crate::label_menus::render(ui, app, index, &entries);
                    });
                });
                // Picking an item or clicking elsewhere closes it.
                let outside = pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p));
                if pointer.button_released(egui::PointerButton::Primary) || (outside && pointer.any_pressed()) {
                    app.cube_menu = None;
                }
            }
            let _ = ui;
    }
}

/// One navigation event for viewport `index`. Views move their own orbit;
/// through a camera the camera viewport controls move the camera node
/// instead (Truck for a pan, Orbit / Pan Camera, Dolly, Perspective, Roll,
/// FOV), each drag one scene undo step. A press in an Orbit mode uses the
/// trackball (`rect`) and Angle Snap.
fn nav_event(app: &mut LoomApp, index: usize, rect: egui::Rect, ev: InputEvent) -> bool {
    use loom_input::NavMode as N;
    if let InputEvent::MouseDown { .. } = ev {
        let orbit_mode = matches!(app.state.nav_mode, N::Orbit | N::OrbitSelected | N::OrbitSubObject);
        let c = rect.center();
        app.camera_controller.set_trackball(orbit_mode.then(|| (Vec2::new(c.x, c.y), trackball_radius(rect))));
        let snap = app.state.angle_snap_on.then(|| app.state.gizmos.orbit_snap_angle.to_radians());
        app.camera_controller.set_orbit_snap(snap);
    }
    let camera = match app.state.viewports[index].kind {
        loom_commands::ViewKind::Camera(id) => Some(id),
        _ => None,
    };
    let Some(before) = camera.and_then(|id| loom_commands::camera::orbit_of(&app.state.scene, id)) else {
        return app.camera_controller.handle(ev, &mut app.state.viewports[index].camera);
    };
    if app.state.active_viewport != index {
        let _ = app.dispatch(Command::ActiveViewport { index: index as u32 });
    }
    let was = app.camera_controller.is_dragging();
    let mut after = before.clone();
    let changed = app.camera_controller.handle(ev, &mut after);
    let now = app.camera_controller.is_dragging();
    if now && !was && !app.camera_nav_open {
        app.state.undo.begin("Camera Navigation");
        app.camera_nav_open = true;
    }
    if changed {
        let mode = app.state.nav_mode;
        let (right, up, _) = before.basis();
        let shift = after.target - before.target;
        let factor = after.distance / before.distance.max(1e-6);
        let deg = |a: f32, b: f32| loom_input::camera_control::wrap_angle(b - a).to_degrees();
        let op: Option<(&str, f32, Option<f32>)> = if (factor - 1.0).abs() > 1e-6 {
            Some(match mode {
                N::Fov => {
                    let fov = match app.state.scene.node(camera.unwrap()).map(|n| &n.object) {
                        Some(loom_scene::ObjectKind::Camera { fov, .. }) => fov.get(),
                        _ => 45.0,
                    };
                    ("fov", (fov * factor).clamp(1.0, 175.0), None)
                }
                N::DollyTarget => ("dolly_target", 1.0 / factor, None),
                N::DollyBoth => ("dolly_both", factor, None),
                N::Perspective => ("perspective", factor, None),
                _ => ("dolly", factor, None),
            })
        } else if (after.roll - before.roll).abs() > 1e-6 {
            Some(("roll", -deg(before.roll, after.roll), None))
        } else if (after.yaw - before.yaw).abs() > 1e-6 || (after.pitch - before.pitch).abs() > 1e-6 {
            let (dyaw, dpitch) = (deg(before.yaw, after.yaw), deg(before.pitch, after.pitch));
            Some(if mode == N::PanCamera { ("pan", -dyaw, Some(dpitch)) } else { ("orbit", dyaw, Some(dpitch)) })
        } else if shift.length_squared() > 0.0 {
            Some(("truck", shift.dot(right), Some(shift.dot(up))))
        } else {
            None
        };
        if let Some((op, x, y)) = op {
            let _ = app.dispatch(Command::NavigateCamera { op: op.into(), x, y });
        }
    }
    if !now && app.camera_nav_open {
        app.state.undo.end();
        app.camera_nav_open = false;
    }
    changed
}

/// The world point under screen point `p` on the active view's focus plane
/// (through its centre, facing the eye); None off the active viewport.
pub(crate) fn view_point_under(app: &LoomApp, p: egui::Pos2) -> Option<Vec3> {
    let i = app.state.active_viewport;
    let rect = *app.viewport_rects.get(i)?;
    if !rect.contains(p) || matches!(app.state.viewports[i].kind, loom_commands::ViewKind::Camera(_)) {
        return None;
    }
    let camera = app.camera_for(i);
    let (view, proj) = (camera.view_matrix(), camera.projection_matrix(rect.width() / rect.height().max(1.0)));
    let local = Vec2::new(p.x - rect.left(), p.y - rect.top());
    let (origin, dir) = loom_render::pick::screen_to_world_ray(local, Vec2::new(rect.width(), rect.height()), view, proj);
    let cam = &app.state.viewports[i].camera;
    let normal = cam.basis().2;
    let denom = dir.dot(normal);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (cam.target - origin).dot(normal) / denom;
    Some(origin + dir * t)
}

/// The Orbit trackball's radius in a viewport.
fn trackball_radius(rect: egui::Rect) -> f32 {
    rect.width().min(rect.height()) * 0.375
}

/// Animates the active view from its angles before `change` to those after (ViewCube transitions).
fn animate_view(app: &mut LoomApp, change: impl FnOnce(&mut LoomApp)) {
    let before = app.state.active().camera.clone();
    change(app);
    let after = app.state.active().camera.clone();
    let cam = &mut app.state.active_mut().camera;
    (cam.yaw, cam.pitch, cam.roll) = (before.yaw, before.pitch, before.roll);
    app.start_view_animation(after.yaw, after.pitch, after.roll);
}

/// A click on the ViewCube: a face sets that axis view (the face already
/// shown keeps it), an edge or corner an angled view in the current
/// projection, Home the Home view; in a face view the roll arrows turn it
/// 90 degrees and the triangles go to the adjacent face.
fn cube_click(app: &mut LoomApp, index: usize, hit: crate::viewport_gizmo::Hit) {
    use crate::viewport_gizmo::{self as cube, Hit};
    if app.state.active_viewport != index {
        let _ = app.dispatch(Command::ActiveViewport { index: index as u32 });
    }
    let cam = app.state.viewports[index].camera.clone();
    let idx = index as u32;
    let orbit = |yaw: Option<f32>, pitch: Option<f32>, roll: f32| Command::Orbit { yaw: yaw.map(f32::to_degrees), pitch: pitch.map(f32::to_degrees), distance: None, target: None, roll: Some(roll.to_degrees()) };
    match hit {
        Hit::Home => animate_view(app, |app| {
            let _ = app.dispatch(Command::HomeView);
        }),
        Hit::Spot(dir) => match cube::face_of(dir) {
            Some(face) => {
                let kind = cube::view_kind(face);
                if app.state.viewports[index].kind == kind && cam.roll.abs() < 1e-4 {
                    return;
                }
                animate_view(app, |app| {
                    let _ = app.dispatch(Command::ViewportView { index: idx, kind });
                });
            }
            None => {
                let (yaw, pitch) = cube::spot_angles(dir);
                let kind = if app.state.viewports[index].is_orthographic() { loom_commands::ViewKind::Orthographic } else { loom_commands::ViewKind::Perspective };
                animate_view(app, |app| {
                    if app.state.viewports[index].kind != kind {
                        let _ = app.dispatch(Command::ViewportView { index: idx, kind });
                    }
                    let _ = app.dispatch(orbit(Some(yaw), Some(pitch), 0.0));
                });
            }
        },
        Hit::RollLeft | Hit::RollRight => {
            // Left turns the view counterclockwise, right clockwise.
            let turn = if hit == Hit::RollLeft { -std::f32::consts::FRAC_PI_2 } else { std::f32::consts::FRAC_PI_2 };
            animate_view(app, |app| {
                let _ = app.dispatch(orbit(None, None, cam.roll + turn));
            });
        }
        Hit::Adjacent(side) => {
            let (eye, up) = cube::adjacent(&cam, side);
            if let Some(face) = cube::face_of(eye.round().as_ivec3()) {
                let kind = cube::view_kind(face);
                let (yaw, pitch) = kind.angles().expect("axis views have fixed angles");
                let roll = cube::roll_for(yaw, pitch, up);
                animate_view(app, |app| {
                    let _ = app.dispatch(Command::ViewportView { index: idx, kind });
                    let _ = app.dispatch(orbit(None, None, roll));
                });
            }
        }
    }
}

/// The three viewport label menus (General, POV, Shading).
/// Returns true when the pointer is over a label (its right-click is the label's menu, not the quad menu).
fn draw_labels(ui: &mut egui::Ui, app: &mut LoomApp, index: usize, rect: egui::Rect) -> bool {
    let mut over = false;
    // The labels sit over the 3D image (drawn later in the viewport's own
    // layer): their own background layer, painted after the panel.
    let painter = &ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("viewport_labels"))).with_clip_rect(rect);
    let (kind, shading, disabled) = {
        let vp = &app.state.viewports[index];
        (app.state.view_label(index), vp.shading.label(), vp.disabled)
    };
    let font = egui::FontId::proportional(13.0);
    let mut x = rect.min.x + 6.0;
    let y = rect.min.y + 4.0;
    let items: [(String, u8); 3] = [("[ + ]".to_owned(), 0), (format!("[ {kind} ]"), 1), (format!("[ {shading} ]"), 2)];
    for (text, which) in items {
        let galley = painter.layout_no_wrap(text, font.clone(), egui::Color32::from_gray(225));
        let size = galley.size();
        let r = egui::Rect::from_min_size(egui::pos2(x, y), size);
        let entries = match which {
            0 => crate::label_menus::general_entries(app, index),
            1 => crate::label_menus::pov_entries(app, index),
            _ => crate::label_menus::shading_entries(app, index),
        };
        // A real menu (submenus open to the side, picking an item or clicking
        // elsewhere closes it); its own text is invisible, the label above is drawn on top.
        let text = galley.text().to_owned();
        let button = egui::Button::new(egui::RichText::new(text).font(font.clone()).color(egui::Color32::TRANSPARENT)).frame(false);
        let resp = ui
            .put(r, |ui: &mut egui::Ui| {
                egui::menu::menu_custom_button(ui, button, |ui| {
                    ui.set_min_width(170.0);
                    crate::label_menus::render(ui, app, index, &entries);
                })
                .response
            })
            .context_menu(|ui| {
                ui.set_min_width(170.0);
                crate::label_menus::render(ui, app, index, &entries);
            });
        let hovered = resp.is_some_and(|r| r.response.hovered()) || ui.rect_contains_pointer(r);
        over |= hovered;
        let color = if hovered { egui::Color32::WHITE } else { egui::Color32::from_gray(225) };
        painter.galley(r.min, galley, color);
        x += size.x + 10.0;
    }
    if disabled {
        painter.text(egui::pos2(x, y), egui::Align2::LEFT_TOP, "<Disabled>", font, egui::Color32::from_gray(225));
    }
    over
}

/// World units per screen pixel at the view's focus distance.
fn units_per_pixel(app: &LoomApp, index: usize, rect: egui::Rect) -> f32 {
    let vp = &app.state.viewports[index];
    let height = if vp.is_orthographic() { vp.camera.distance * 0.8284 } else { 2.0 * vp.camera.distance * (vp.fov_y_radians * 0.5).tan() };
    (height / rect.height().max(1.0)).max(1e-4)
}

/// Pointer on the construction plane: the z = 0 plane in Top/Perspective
/// views, the view's own plane (through the origin) in the axis views.
fn ground_hit(p: egui::Pos2, viewport_rect: egui::Rect, view: Mat4, proj: Mat4) -> Option<Vec3> {
    plane_hit(p, viewport_rect, view, proj, Vec3::Z)
}

fn plane_hit(p: egui::Pos2, viewport_rect: egui::Rect, view: Mat4, proj: Mat4, normal: Vec3) -> Option<Vec3> {
    let local = Vec2::new(p.x - viewport_rect.left(), p.y - viewport_rect.top());
    let size = Vec2::new(viewport_rect.width(), viewport_rect.height());
    let (origin, dir) = loom_render::pick::screen_to_world_ray(local, size, view, proj);
    let denom = dir.dot(normal);
    if denom.abs() <= 1e-6 {
        return None;
    }
    let t = -origin.dot(normal) / denom;
    (t >= 0.0).then_some(origin + dir * t)
}

/// Where a creation starts: the view's construction plane, or with AutoGrid
/// the surface under the cursor (point and face normal).
pub(crate) fn creation_anchor(app: &LoomApp, index: usize, p: egui::Pos2, rect: egui::Rect, view: Mat4, proj: Mat4) -> Option<(Vec3, (Vec3, Vec3, Vec3))> {
    let kind = app.state.viewports[index].kind;
    let (u, v, n) = kind.construction_plane();
    if app.autogrid {
        if let Some((point, normal)) = surface_hit(app, p, rect, view, proj) {
            let n = normal.normalize_or_zero();
            let helper = if n.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
            let u = helper.cross(n).normalize_or_zero();
            let v = n.cross(u);
            return Some((point, (u, v, n)));
        }
    }
    plane_hit(p, rect, view, proj, n).map(|pt| (pt, (u, v, n)))
}

/// Nearest mesh surface under the pointer, as (point, face normal).
fn surface_hit(app: &LoomApp, p: egui::Pos2, rect: egui::Rect, view: Mat4, proj: Mat4) -> Option<(Vec3, Vec3)> {
    surface_hit_node(app, p, rect, view, proj).map(|(_, pt, n)| (pt, n))
}

/// Nearest mesh surface under the pointer with its node: (node, point, face normal).
pub(crate) fn surface_hit_node(app: &LoomApp, p: egui::Pos2, rect: egui::Rect, view: Mat4, proj: Mat4) -> Option<(loom_scene::NodeId, Vec3, Vec3)> {
    let local = Vec2::new(p.x - rect.left(), p.y - rect.top());
    let size = Vec2::new(rect.width(), rect.height());
    let (origin, dir) = loom_render::pick::screen_to_world_ray(local, size, view, proj);
    let mut best: Option<(f32, Vec3, loom_scene::NodeId)> = None;
    for (id, mesh) in app.world_meshes() {
        for face_i in 0..mesh.face_count() {
            let Some(points) = mesh.face_positions(FaceId(face_i as u32)) else { continue };
            if points.len() < 3 {
                continue;
            }
            let normal = (points[1] - points[0]).cross(points[2] - points[0]);
            for i in 1..points.len() - 1 {
                if let Some(t) = ray_triangle(origin, dir, points[0], points[i], points[i + 1]) {
                    if best.is_none_or(|(bt, _, _)| t < bt) {
                        best = Some((t, normal, *id));
                    }
                }
            }
        }
    }
    best.map(|(t, n, id)| (id, origin + dir * t, (if n.dot(dir) > 0.0 { -n } else { n }).normalize_or_zero()))
}

/// Click picks: with By Vertex on at an edge / polygon level, a click on a
/// vertex selects every edge / polygon that uses it.
fn picks_at(app: &LoomApp, p: egui::Pos2, rect: egui::Rect, view: Mat4, proj: Mat4, view_proj: Mat4) -> Vec<Pick> {
    let mode = app.state.selection_mode;
    if app.state.select_by_vertex && mode != SelectionMode::Object && mode != SelectionMode::Vertex {
        if let Some(Pick::SubObject { mesh, id: SubObjectId::Vertex(v) }) = pick_vertex(app, p, rect, view_proj) {
            let m = &app.world_meshes()[mesh].1;
            let level = match mode {
                SelectionMode::Edge | SelectionMode::Border => loom_mesh::select_ops::SubLevel::Edge,
                _ => loom_mesh::select_ops::SubLevel::Face,
            };
            let ids = loom_mesh::select_ops::convert_selection(m, loom_mesh::select_ops::SubLevel::Vertex, &[v.0], level, false);
            return ids.into_iter().map(|raw| Pick::SubObject { mesh, id: match mode { SelectionMode::Edge | SelectionMode::Border => SubObjectId::Edge(HalfEdgeId(raw)), _ => SubObjectId::Face(FaceId(raw)) } }).collect();
        }
        return Vec::new();
    }
    pick_at(app, p, rect, view, proj, view_proj).into_iter().collect()
}

fn pick_at(
    app: &LoomApp,
    p: egui::Pos2,
    rect: egui::Rect,
    view: Mat4,
    proj: Mat4,
    view_proj: Mat4,
) -> Option<Pick> {
    if app.state.spline_selected() && app.state.selection_mode != SelectionMode::Object && app.state.selection_mode != SelectionMode::Vertex {
        return crate::spline_modes::pick_sub(app, p, rect, view_proj);
    }
    match app.state.selection_mode {
        SelectionMode::Object => pick_object(app, p, rect, view, proj),
        SelectionMode::Vertex => pick_vertex(app, p, rect, view_proj),
        SelectionMode::Edge | SelectionMode::Border => pick_edge(app, p, rect, view_proj),
        SelectionMode::Face | SelectionMode::Element => pick_face(app, p, rect, view, proj),
    }
}

fn pick_object(
    app: &LoomApp,
    p: egui::Pos2,
    rect: egui::Rect,
    view: Mat4,
    proj: Mat4,
) -> Option<Pick> {
    object_hits(app, p, rect, view, proj).first().map(|&i| Pick::Object(i))
}

/// Objects under `p` in pick order, as indices into `world_meshes`: a shape
/// whose outline is within reach, else every object whose drawn surface the
/// pointer ray hits (in a Wireframe view: whose drawn edges pass within 4 px),
/// solid ones front to back and then See-Through ones, so what a See-Through
/// object encloses is picked through it.
fn object_hits(app: &LoomApp, p: egui::Pos2, rect: egui::Rect, view: Mat4, proj: Mat4) -> Vec<usize> {
    let view_proj = proj * view;
    if let Some(i) = pick_shape(app, p, rect, view_proj) {
        return vec![i];
    }
    let local = p - rect.min;
    let (origin, dir) = loom_render::pick::screen_to_world_ray(Vec2::new(local.x, local.y), Vec2::new(rect.width(), rect.height()), view, proj);
    let wire = app.state.active().shading == loom_commands::Shading::Wireframe;
    let mut hits: Vec<(bool, f32, usize)> = Vec::new();
    for (i, (id, _)) in app.world_meshes().iter().enumerate() {
        let mesh = app.drawn_mesh(i);
        if mesh.face_count() == 0 {
            continue;
        }
        let t = if wire { edge_hit(mesh, p, rect, view_proj, origin, dir) } else { loom_render::pick::ray_mesh(origin, dir, mesh) };
        if let Some(t) = t {
            hits.push((app.state.scene.node(*id).is_some_and(crate::draws_see_through), t, i));
        }
    }
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    hits.into_iter().map(|(_, _, i)| i).collect()
}

/// The nearest point (as a distance along the ray) of a mesh edge drawn
/// within 4 px of `p`.
fn edge_hit(mesh: &Mesh, p: egui::Pos2, rect: egui::Rect, view_proj: Mat4, origin: Vec3, dir: Vec3) -> Option<f32> {
    let mut best: Option<f32> = None;
    for he_i in 0..mesh.half_edges().len() {
        let e = HalfEdgeId(he_i as u32);
        if mesh.canonical_edge_id(e) != Some(e) {
            continue;
        }
        let Some((va, vb)) = mesh.edge_vertices(e) else { continue };
        let (Some(wa), Some(wb)) = (mesh.vertex_position(va), mesh.vertex_position(vb)) else { continue };
        let (Some(a), Some(b)) = (project_world_to_screen(wa, view_proj, rect), project_world_to_screen(wb, view_proj, rect)) else { continue };
        let ab = b - a;
        let s = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
        if (a + ab * s).distance(p) <= 4.0 {
            let t = (wa + (wb - wa) * s - origin).dot(dir);
            if best.is_none_or(|bt| t < bt) {
                best = Some(t);
            }
        }
    }
    best
}

/// A repeat click within 2 px of the last object click picks the next object
/// under it (and comes round again); a double-click never steps.
fn cycle_object_pick(app: &mut LoomApp, p: egui::Pos2, rect: egui::Rect, view: Mat4, proj: Mat4, picked: &mut Vec<Pick>, double: bool) {
    if !matches!(picked.as_slice(), [Pick::Object(_)]) {
        app.pick_cycle = None;
        return;
    }
    let hits = object_hits(app, p, rect, view, proj);
    let ids: Vec<NodeId> = hits.iter().map(|&i| app.world_meshes()[i].0).collect();
    let k = match &app.pick_cycle {
        Some((q, prev, k)) if !double && q.distance(p) <= 2.0 && *prev == ids => (k + 1) % ids.len(),
        _ => 0,
    };
    *picked = vec![Pick::Object(hits[k])];
    app.pick_cycle = Some((p, ids, k));
}

/// Viewport double-clicks: on an object, it and all its descendants are
/// selected; on an Editable Poly / Edit Poly edge, its loop.
fn double_click(app: &mut LoomApp, p: egui::Pos2, rect: egui::Rect, view: Mat4, proj: Mat4, view_proj: Mat4, mods: Modifiers) -> bool {
    app.pick_cycle = None;
    match app.state.selection_mode {
        SelectionMode::Object => {
            let Some(Pick::Object(i)) = pick_object(app, p, rect, view, proj) else { return false };
            let id = app.world_meshes()[i].0;
            let ids: Vec<u64> = std::iter::once(id).chain(app.state.scene.descendants(id)).map(|n| n.0).collect();
            let op = if mods.ctrl { SelectOp::Add } else { SelectOp::Replace };
            let _ = app.dispatch(Command::SelectNodes { ids, op });
            true
        }
        SelectionMode::Edge if !app.state.spline_selected() => {
            let Some(Pick::SubObject { mesh, id: SubObjectId::Edge(e) }) = pick_edge(app, p, rect, view_proj) else { return false };
            let edges = loom_mesh::select_ops::edge_loop(&app.world_meshes()[mesh].1, &[e]);
            apply_picks(app, edges.into_iter().map(|e| Pick::SubObject { mesh, id: SubObjectId::Edge(e) }).collect(), mods);
            true
        }
        _ => false,
    }
}

pub(crate) fn pick_vertex(
    app: &LoomApp,
    p: egui::Pos2,
    rect: egui::Rect,
    view_proj: Mat4,
) -> Option<Pick> {
    let view_dir = view_direction(app, app.state.active_viewport);
    let mut best: Option<(usize, VertexId, f32)> = None;
    for (mesh_i, (node_id, mesh)) in app.world_meshes().iter().enumerate() {
        if !app.state.selection.contains(node_id) {
            continue;
        }
        for (vertex_i, vertex) in mesh.vertices().iter().enumerate() {
            if !sub_pickable(app, mesh, SubObjectId::Vertex(VertexId(vertex_i as u32)), view_dir) {
                continue;
            }
            if let Some(sp) = project_world_to_screen(vertex.position, view_proj, rect) {
                let d = sp.distance(p);
                if d <= 10.0 && best.is_none_or(|(_, _, bd)| d < bd) {
                    best = Some((mesh_i, VertexId(vertex_i as u32), d));
                }
            }
        }
    }
    best.map(|(mesh, vertex, _)| Pick::SubObject {
        mesh,
        id: SubObjectId::Vertex(vertex),
    })
}

pub(crate) fn pick_edge(
    app: &LoomApp,
    p: egui::Pos2,
    rect: egui::Rect,
    view_proj: Mat4,
) -> Option<Pick> {
    let view_dir = view_direction(app, app.state.active_viewport);
    let mut best: Option<(usize, HalfEdgeId, f32)> = None;
    for (mesh_i, (node_id, mesh)) in app.world_meshes().iter().enumerate() {
        if !app.state.selection.contains(node_id) {
            continue;
        }
        for (he_i, _) in mesh.half_edges().iter().enumerate() {
            let he_id = HalfEdgeId(he_i as u32);
            if mesh.canonical_edge_id(he_id) != Some(he_id) || !sub_pickable(app, mesh, SubObjectId::Edge(he_id), view_dir) {
                continue;
            }
            let Some((a_id, b_id)) = mesh.edge_vertices(he_id) else {
                continue;
            };
            let Some(a) = mesh.vertex_position(a_id) else {
                continue;
            };
            let Some(b) = mesh.vertex_position(b_id) else {
                continue;
            };
            let Some(sa) = project_world_to_screen(a, view_proj, rect) else {
                continue;
            };
            let Some(sb) = project_world_to_screen(b, view_proj, rect) else {
                continue;
            };
            let d = distance_to_segment(p, sa, sb);
            if d <= 8.0 && best.is_none_or(|(_, _, bd)| d < bd) {
                best = Some((mesh_i, he_id, d));
            }
        }
    }
    best.map(|(mesh, edge, _)| Pick::SubObject {
        mesh,
        id: SubObjectId::Edge(edge),
    })
}

pub(crate) fn pick_face(
    app: &LoomApp,
    p: egui::Pos2,
    rect: egui::Rect,
    view: Mat4,
    proj: Mat4,
) -> Option<Pick> {
    let local = p - rect.min;
    let viewport_size = Vec2::new(rect.width(), rect.height());
    let (origin, dir) = loom_render::pick::screen_to_world_ray(
        Vec2::new(local.x, local.y),
        viewport_size,
        view,
        proj,
    );
    let view_dir = view_direction(app, app.state.active_viewport);
    let mut best: Option<(usize, FaceId, f32)> = None;
    for (mesh_i, (node_id, mesh)) in app.world_meshes().iter().enumerate() {
        if !app.state.selection.contains(node_id) {
            continue;
        }
        for face_i in 0..mesh.face_count() {
            let face_id = FaceId(face_i as u32);
            if !sub_pickable(app, mesh, SubObjectId::Face(face_id), view_dir) {
                continue;
            }
            let Some(points) = mesh.face_positions(face_id) else {
                continue;
            };
            if points.len() < 3 {
                continue;
            }
            for i in 1..points.len() - 1 {
                if let Some(t) = ray_triangle(origin, dir, points[0], points[i], points[i + 1]) {
                    if best.is_none_or(|(_, _, bt)| t < bt) {
                        best = Some((mesh_i, face_id, t));
                    }
                }
            }
        }
    }
    best.map(|(mesh, face, _)| Pick::SubObject {
        mesh,
        id: SubObjectId::Face(face),
    })
}

/// Screen points that represent a sub-object for region selection.
fn candidate_points(mesh: &Mesh, id: SubObjectId, view_proj: Mat4, rect: egui::Rect) -> Vec<egui::Pos2> {
    let project = |p: Vec3| project_world_to_screen(p, view_proj, rect);
    match id {
        SubObjectId::Vertex(v) => mesh.vertex_position(v).and_then(project).into_iter().collect(),
        SubObjectId::Edge(e) => mesh
            .edge_vertices(e)
            .map(|(a, b)| [a, b].into_iter().filter_map(|v| mesh.vertex_position(v)).filter_map(project).collect())
            .unwrap_or_default(),
        SubObjectId::Face(f) => mesh.face_positions(f).map(|pts| pts.into_iter().filter_map(project).collect()).unwrap_or_default(),
    }
}

/// Object region selection by the drawn geometry: Window needs every vertex
/// inside the region; Crossing any part of the object: a vertex, a point
/// along an edge, or a face under the region's centre.
fn object_in_region(mesh: &Mesh, region: &crate::region::Region, view_proj: Mat4, rect: egui::Rect, window: bool) -> bool {
    let project = |p: Vec3| project_world_to_screen(p, view_proj, rect);
    let points: Vec<egui::Pos2> = mesh.vertices().iter().filter_map(|v| project(v.position)).collect();
    if window || points.is_empty() {
        return region.selects(&points, window);
    }
    if points.iter().any(|p| region.contains(*p)) {
        return true;
    }
    for he_i in 0..mesh.half_edges().len() {
        let e = HalfEdgeId(he_i as u32);
        if mesh.canonical_edge_id(e) != Some(e) {
            continue;
        }
        let Some((a, b)) = mesh.edge_vertices(e) else { continue };
        let (Some(a), Some(b)) = (mesh.vertex_position(a).and_then(project), mesh.vertex_position(b).and_then(project)) else { continue };
        let steps = ((b - a).length() / 4.0).ceil().clamp(1.0, 256.0) as usize;
        if (1..steps).any(|k| region.contains(a + (b - a) * (k as f32 / steps as f32))) {
            return true;
        }
    }
    let centre = region.bounding_rect().center() - rect.min;
    let (origin, dir) = loom_render::pick::screen_to_world_ray(Vec2::new(centre.x, centre.y), Vec2::new(rect.width(), rect.height()), Mat4::IDENTITY, view_proj);
    region.contains(region.bounding_rect().center()) && loom_render::pick::ray_mesh(origin, dir, mesh).is_some()
}

/// Region selection with Window (fully enclosed) or Crossing (touched) semantics.
fn region_select(app: &LoomApp, region: &crate::region::Region, viewport_rect: egui::Rect, view_proj: Mat4) -> Vec<Pick> {
    region_select_mode(app, region, viewport_rect, view_proj, app.state.window_selection)
}

/// Sub-objects that cannot be picked: hidden ones, and with Ignore
/// Backfacing those whose normal points away from the viewer.
fn sub_pickable(app: &LoomApp, mesh: &Mesh, id: SubObjectId, view_dir: Vec3) -> bool {
    if loom_commands::is_hidden(mesh, id) {
        return false;
    }
    if !app.state.ignore_backfacing {
        return true;
    }
    let facing = |f: FaceId| mesh.face_normal(f).dot(view_dir) < 0.0;
    match id {
        SubObjectId::Face(f) => facing(f),
        SubObjectId::Vertex(v) => {
            let fs = mesh.faces_around_vertex(v);
            fs.is_empty() || fs.iter().any(|f| facing(*f))
        }
        SubObjectId::Edge(e) => {
            let he = &mesh.half_edges()[e.0 as usize];
            let twin = &mesh.half_edges()[he.twin.0 as usize];
            let fs: Vec<FaceId> = he.face.into_iter().chain(twin.face).collect();
            fs.is_empty() || fs.iter().any(|f| facing(*f))
        }
    }
}

/// Unit view direction (from the eye into the scene) of a viewport camera.
fn view_direction(app: &LoomApp, index: usize) -> Vec3 {
    let cam = &app.state.viewports[index.min(app.state.viewports.len() - 1)].camera;
    (cam.target - cam.eye_position()).normalize_or_zero()
}

fn region_select_mode(app: &LoomApp, region: &crate::region::Region, viewport_rect: egui::Rect, view_proj: Mat4, window: bool) -> Vec<Pick> {
    let view_dir = view_direction(app, app.state.active_viewport);
    let mut items = Vec::new();
    if app.state.spline_selected() && matches!(app.state.selection_mode, SelectionMode::Edge | SelectionMode::Border | SelectionMode::Face | SelectionMode::Element) {
        return crate::spline_modes::region_subs(app, region, viewport_rect, view_proj, window);
    }
    for (mesh_i, (node_id, mesh)) in app.world_meshes().iter().enumerate() {
        match app.state.selection_mode {
            SelectionMode::Object => {
                let selectable = app.state.scene.node(*node_id).is_some_and(|n| !n.frozen && app.state.selection_filter.accepts(n));
                if selectable && object_in_region(app.drawn_mesh(mesh_i), region, view_proj, viewport_rect, window) {
                    items.push(Pick::Object(mesh_i));
                }
            }
            // A sub-object level edits only the selected objects.
            _ if !app.state.selection.contains(node_id) => {}
            SelectionMode::Vertex => {
                for vertex_i in 0..mesh.vertex_count() {
                    let id = SubObjectId::Vertex(VertexId(vertex_i as u32));
                    if sub_pickable(app, mesh, id, view_dir) && region.selects(&candidate_points(mesh, id, view_proj, viewport_rect), window) {
                        items.push(Pick::SubObject { mesh: mesh_i, id });
                    }
                }
            }
            SelectionMode::Edge | SelectionMode::Border => {
                for he_i in 0..mesh.half_edges().len() {
                    let he_id = HalfEdgeId(he_i as u32);
                    if mesh.canonical_edge_id(he_id) != Some(he_id) {
                        continue;
                    }
                    let id = SubObjectId::Edge(he_id);
                    if sub_pickable(app, mesh, id, view_dir) && region.selects(&candidate_points(mesh, id, view_proj, viewport_rect), window) {
                        items.push(Pick::SubObject { mesh: mesh_i, id });
                    }
                }
            }
            SelectionMode::Face | SelectionMode::Element => {
                for face_i in 0..mesh.face_count() {
                    let id = SubObjectId::Face(FaceId(face_i as u32));
                    if sub_pickable(app, mesh, id, view_dir) && region.selects(&candidate_points(mesh, id, view_proj, viewport_rect), window) {
                        items.push(Pick::SubObject { mesh: mesh_i, id });
                    }
                }
            }
        }
    }
    items
}

/// The region a drag from `start` to `end` describes in the active mode.
fn drag_region(app: &LoomApp, start: egui::Pos2, end: egui::Pos2) -> crate::region::Region {
    use crate::region::Region;
    match app.state.selection_region {
        loom_commands::SelectionRegion::Rectangular => Region::Rect(egui::Rect::from_two_pos(start, end)),
        loom_commands::SelectionRegion::Circular => Region::Circle { center: start, radius: start.distance(end) },
        loom_commands::SelectionRegion::Lasso => Region::Polygon(app.region_points.clone()),
        loom_commands::SelectionRegion::Paint => Region::Paint { path: app.region_points.clone(), radius: app.state.prefs.paint_brush_size * 0.5 },
        loom_commands::SelectionRegion::Fence => Region::Polygon(app.region_points.clone()),
    }
}

fn draw_region(painter: &egui::Painter, region: &crate::region::Region) {
    use crate::region::Region;
    let fill = egui::Color32::from_white_alpha(28);
    let stroke = egui::Stroke::new(1.0, egui::Color32::from_white_alpha(180));
    match region {
        Region::Rect(r) => {
            painter.rect_filled(*r, 0.0, fill);
            painter.rect_stroke(*r, 0.0, stroke, egui::StrokeKind::Middle);
        }
        Region::Circle { center, radius } => {
            painter.circle_filled(*center, *radius, fill);
            painter.circle_stroke(*center, *radius, stroke);
        }
        Region::Polygon(pts) => {
            if pts.len() >= 2 {
                painter.add(egui::Shape::closed_line(pts.clone(), stroke));
            }
        }
        Region::Paint { path, radius } => {
            for p in path {
                painter.circle_filled(*p, *radius, egui::Color32::from_white_alpha(12));
            }
            if let Some(last) = path.last() {
                painter.circle_stroke(*last, *radius, stroke);
            }
        }
    }
}

/// Shapes as polylines: white, selected orange; the knots of a Line being drawn.
fn draw_shapes(painter: &egui::Painter, app: &LoomApp, view_proj: Mat4, viewport_rect: egui::Rect) {
    for n in app.state.scene.nodes() {
        if !app.state.is_visible(n) {
            continue;
        }
        let Some(base) = n.edit_shape() else { continue };
        let selected = app.state.selection.contains(&n.id);
        let editing = selected && app.state.selection_mode != SelectionMode::Object && n.active_entry.is_none_or(|a| n.editing_spline_entry() == Some(a));
        // A shape turned into a mesh by its stack shows the mesh, not the outline,
        // unless its splines are being edited.
        if !n.modifiers.is_empty() && !editing && app.state.scene.eval_mesh(n.id).value.mesh.face_count() > 0 {
            continue;
        }
        // Otherwise the outline is the stack's shape (Fillet/Chamfer, Trim/Extend, …),
        // or with Show End Result off the active entry's.
        let shape = if n.modifiers.is_empty() || editing {
            base
        } else {
            let v = match n.active_entry.filter(|_| !app.state.show_end_result) {
                Some(a) => app.state.scene.evaluated(n.id, Some(a)),
                None => app.state.scene.eval_mesh(n.id),
            };
            v.value.shape.clone().unwrap_or(base)
        };
        let world = app.state.scene.object_matrix(n.id);
        let color = if selected { egui::Color32::from_rgb(255, 150, 40) } else if n.frozen { egui::Color32::from_gray(110) } else { egui::Color32::from_gray(235) };
        let stroke = egui::Stroke::new(1.5_f32, color);
        for (si, pl) in shape.polylines().iter().enumerate() {
            let pts: Vec<Option<egui::Pos2>> = pl.iter().map(|p| project_world_to_screen(world.transform_point3(*p), view_proj, viewport_rect)).collect();
            let closed = shape.splines.get(si).is_some_and(|s| s.closed);
            let segs = if closed { pts.len() } else { pts.len().saturating_sub(1) };
            for i in 0..segs {
                if let (Some(a), Some(b)) = (pts[i], pts[(i + 1) % pts.len()]) {
                    painter.line_segment([a, b], stroke);
                }
            }
        }
        if app.line.as_ref().is_some_and(|l| l.node == n.id) {
            for s in &shape.splines {
                for k in &s.knots {
                    if let Some(sp) = project_world_to_screen(world.transform_point3(k.pos), view_proj, viewport_rect) {
                        painter.rect_filled(egui::Rect::from_center_size(sp, egui::vec2(6.0, 6.0)), 0.0, egui::Color32::WHITE);
                    }
                }
            }
        }
    }
}

/// The shape node whose outline passes within 6 px of `p`, as an index into `world_meshes`.
fn pick_shape(app: &LoomApp, p: egui::Pos2, rect: egui::Rect, view_proj: Mat4) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, (id, _)) in app.world_meshes().iter().enumerate() {
        let Some(n) = app.state.scene.node(*id) else { continue };
        let Some(shape) = n.object.shape() else { continue };
        let world = app.state.scene.object_matrix(*id);
        for (si, pl) in shape.polylines().iter().enumerate() {
            let pts: Vec<Option<egui::Pos2>> = pl.iter().map(|q| project_world_to_screen(world.transform_point3(*q), view_proj, rect)).collect();
            let closed = shape.splines.get(si).is_some_and(|s| s.closed);
            let segs = if closed { pts.len() } else { pts.len().saturating_sub(1) };
            for k in 0..segs {
                let (Some(a), Some(b)) = (pts[k], pts[(k + 1) % pts.len()]) else { continue };
                let ab = b - a;
                let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
                let d = (a + ab * t).distance(p);
                if d <= 6.0 && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((i, d));
                }
            }
        }
    }
    best.map(|(i, _)| i)
}

fn draw_mode_overlays(
    painter: &egui::Painter,
    app: &LoomApp,
    view_proj: Mat4,
    viewport_rect: egui::Rect,
) {
    let vertex_level = app.state.selection_mode == SelectionMode::Vertex;
    {
        let color = egui::Color32::from_gray(210);
        for (id, mesh) in app.world_meshes() {
            let ticks = app.state.scene.node(*id).is_some_and(|n| n.display.vertex_ticks);
            // Vertex level shows only the edited (selected) objects' vertices.
            let edited = vertex_level && app.state.selection.contains(id);
            if !app.state.show_vertices && !edited && !ticks {
                continue;
            }
            for vertex in mesh.vertices() {
                if let Some(sp) = project_world_to_screen(vertex.position, view_proj, viewport_rect)
                {
                    painter.circle_filled(sp, 3.0, color);
                    painter.circle_stroke(sp, 4.0, egui::Stroke::new(1.0, egui::Color32::BLACK));
                }
            }
        }
    }
}

fn draw_subobject_overlay(
    painter: &egui::Painter,
    app: &LoomApp,
    view_proj: Mat4,
    viewport_rect: egui::Rect,
) {
    let orange = egui::Color32::from_rgb(255, 150, 40);
    let stroke = egui::Stroke::new(2.0, orange);
    for (node, id) in app.state.sub_selection.iter() {
        let Some((_, mesh)) = app.world_meshes().iter().find(|(n, _)| *n == node) else {
            continue;
        };
        match id {
            SubObjectId::Vertex(vertex_id) => {
                if let Some(pos) = mesh.vertex_position(vertex_id) {
                    if let Some(sp) = project_world_to_screen(pos, view_proj, viewport_rect) {
                        painter.circle_filled(sp, 5.0, orange);
                        painter.circle_stroke(
                            sp,
                            6.0,
                            egui::Stroke::new(1.0, egui::Color32::BLACK),
                        );
                    }
                }
            }
            SubObjectId::Edge(edge_id) => {
                draw_edge_overlay(painter, mesh, edge_id, view_proj, viewport_rect, stroke)
            }
            SubObjectId::Face(face_id) => {
                if let Some(ids) = mesh.face_vertex_ids(face_id) {
                    // F2 Shade Selected Faces: red tint over the polygon.
                    if app.state.shade_selected_faces {
                        let pts: Vec<egui::Pos2> = ids
                            .iter()
                            .filter_map(|v| mesh.vertex_position(*v))
                            .filter_map(|p| project_world_to_screen(p, view_proj, viewport_rect))
                            .collect();
                        if pts.len() >= 3 {
                            painter.add(egui::Shape::convex_polygon(pts, egui::Color32::from_rgba_unmultiplied(255, 0, 0, 90), egui::Stroke::NONE));
                        }
                    }
                    for i in 0..ids.len() {
                        let a = ids[i];
                        let b = ids[(i + 1) % ids.len()];
                        draw_segment_overlay(painter, mesh, a, b, view_proj, viewport_rect, stroke);
                    }
                }
            }
        }
    }
}

/// J: white corner brackets around each selected object's screen bounds.
/// Snap resolution for the pointer in a viewport: the construction plane of
/// the view, the selection excluded while a transform drag moves it.
pub fn snap_at(app: &LoomApp, index: usize, projection: &Projection, pointer: egui::Pos2, start: Option<Vec3>, exclude_selection: bool) -> Option<loom_commands::snap::SnapHit> {
    snap_within(app, index, projection, pointer, start, exclude_selection, app.state.snaps.radius_px)
}

/// `snap_at` with the preview radius: marker shown, no snap yet.
pub fn snap_preview(app: &LoomApp, index: usize, projection: &Projection, pointer: egui::Pos2, start: Option<Vec3>, exclude_selection: bool) -> Option<loom_commands::snap::SnapHit> {
    snap_within(app, index, projection, pointer, start, exclude_selection, app.state.snaps.preview_radius_px.max(app.state.snaps.radius_px))
}

fn snap_within(app: &LoomApp, index: usize, projection: &Projection, pointer: egui::Pos2, start: Option<Vec3>, exclude_selection: bool, radius_px: f32) -> Option<loom_commands::snap::SnapHit> {
    if !app.state.snaps_on {
        return None;
    }
    let (u, v, n) = app.state.viewports[index].kind.construction_plane();
    // Moving objects: skip them and their descendants. Moving sub-objects:
    // skip only the vertices that move, so a vertex can snap to its own mesh.
    let sub_object = app.state.selection_mode != SelectionMode::Object;
    let exclude: Vec<loom_scene::NodeId> = if exclude_selection && !sub_object {
        app.state.scene.nodes().filter(|nd| app.state.selection.contains(&nd.id) || app.state.selection.iter().any(|s| app.state.scene.is_descendant(nd.id, *s))).map(|nd| nd.id).collect()
    } else {
        Vec::new()
    };
    let exclude_vertices: Vec<(loom_scene::NodeId, u32)> = if exclude_selection && sub_object {
        loom_commands::selected_vertices(&app.state).into_iter().flat_map(|(id, vs)| vs.into_iter().map(move |v| (id, v.0))).collect()
    } else {
        Vec::new()
    };
    let project = |w: Vec3| projection.screen(w).map(|p| Vec2::new(p.x, p.y));
    let q = loom_commands::snap::SnapQuery {
        pointer: Vec2::new(pointer.x, pointer.y),
        ray: projection.ray(pointer),
        project: &project,
        plane: (Vec3::ZERO, u, v, n),
        start,
        exclude: &exclude,
        exclude_vertices: &exclude_vertices,
    };
    loom_commands::snap::resolve_within(&app.state, &q, radius_px)
}

/// Snap marker (shape per snap type) and the rubber band from the
/// operation's start point.
fn draw_snap_marker(painter: &egui::Painter, app: &LoomApp, hit: &loom_commands::snap::SnapHit, rubber_from: Option<egui::Pos2>) {
    use loom_commands::snap::SnapKind as K;
    let s = &app.state.snaps;
    if !s.marker_display {
        return;
    }
    let c = egui::pos2(hit.screen.x, hit.screen.y);
    let r = (s.marker_size * 0.5).max(3.0);
    let color = egui::Color32::from_rgb(0, 220, 255);
    let stroke = egui::Stroke::new(1.5, color);
    match hit.kind {
        K::GridPoint => {
            painter.line_segment([egui::pos2(c.x - r, c.y), egui::pos2(c.x + r, c.y)], stroke);
            painter.line_segment([egui::pos2(c.x, c.y - r), egui::pos2(c.x, c.y + r)], stroke);
        }
        K::GridLine => {
            painter.line_segment([egui::pos2(c.x - r, c.y - r), egui::pos2(c.x + r, c.y + r)], stroke);
            painter.line_segment([egui::pos2(c.x - r, c.y + r), egui::pos2(c.x + r, c.y - r)], stroke);
        }
        K::Vertex | K::Endpoint | K::BoundingBox => {
            painter.rect_stroke(egui::Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0)), 0.0, stroke, egui::StrokeKind::Middle);
        }
        K::Pivot => {
            painter.circle_stroke(c, r, stroke);
            painter.circle_filled(c, 2.0, color);
        }
        K::Midpoint | K::CenterFace => {
            let pts = vec![egui::pos2(c.x, c.y - r), egui::pos2(c.x + r, c.y + r), egui::pos2(c.x - r, c.y + r)];
            painter.add(egui::Shape::closed_line(pts, stroke));
        }
        K::EdgeSegment | K::Perpendicular => {
            let pts = vec![egui::pos2(c.x, c.y - r), egui::pos2(c.x + r, c.y), egui::pos2(c.x, c.y + r), egui::pos2(c.x - r, c.y)];
            painter.add(egui::Shape::closed_line(pts, stroke));
        }
        K::Face => {
            painter.rect_stroke(egui::Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0)), 0.0, stroke, egui::StrokeKind::Middle);
            painter.line_segment([egui::pos2(c.x - r, c.y - r), egui::pos2(c.x + r, c.y + r)], stroke);
            painter.line_segment([egui::pos2(c.x - r, c.y + r), egui::pos2(c.x + r, c.y - r)], stroke);
        }
    }
    painter.text(egui::pos2(c.x + r + 4.0, c.y - r - 2.0), egui::Align2::LEFT_BOTTOM, hit.kind.label(), egui::FontId::proportional(10.0), color);
    if let (Some(from), true) = (rubber_from, s.show_rubber_band) {
        painter.line_segment([from, c], egui::Stroke::new(1.0_f32, egui::Color32::from_gray(230)));
    }
}

/// A built-in manipulator of a selected primitive (Select and Manipulate).
pub struct Manipulator {
    pub id: loom_scene::NodeId,
    pub name: &'static str,
    /// World point the handle is drawn at.
    pub handle: Vec3,
    /// World origin the handle measures from and the unit direction it moves along.
    pub origin: Vec3,
    pub dir: Vec3,
    /// Ring radius (world) for radius manipulators, None for height.
    pub ring: Option<f32>,
}

/// Radius / height manipulators of the selected primitives: radius
/// parameters get a ring in the object's XY plane with a handle on local
/// +X, Box / Cylinder / Cone / Tube / Pyramid heights a handle on top.
pub fn manipulators(app: &LoomApp) -> Vec<Manipulator> {
    let mut out = Vec::new();
    for id in app.state.selection.iter().copied() {
        let Some(node) = app.state.scene.node(id) else { continue };
        let world = app.state.scene.object_matrix(id);
        let origin = world.w_axis.truncate();
        let xa = world.x_axis.truncate().normalize_or_zero();
        let za = world.z_axis.truncate().normalize_or_zero();
        let mut height = None;
        for (name, value) in node.object.parameters() {
            let loom_scene::ParamValue::Float(v) = value else { continue };
            match name {
                "Radius" | "Radius 1" | "Radius 2" => {
                    let r = world.transform_vector3(Vec3::X * v).length();
                    out.push(Manipulator { id, name, handle: origin + xa * r, origin, dir: xa, ring: Some(r) });
                }
                "Height" => height = Some(v),
                _ => {}
            }
        }
        if let Some(h) = height {
            let top = world.transform_point3(Vec3::Z * h);
            out.push(Manipulator { id, name: "Height", handle: top, origin, dir: za, ring: None });
        }
    }
    out
}

fn draw_manipulator(painter: &egui::Painter, p: &Projection, m: &Manipulator, hover: Option<egui::Pos2>, dragging: bool) {
    let Some(hp) = p.screen(m.handle) else { return };
    let hot = dragging || hover.is_some_and(|h| h.distance(hp) <= 8.0);
    let color = if hot { egui::Color32::from_rgb(255, 0, 0) } else { egui::Color32::from_rgb(0, 200, 90) };
    if let Some(r) = m.ring {
        let n = m.dir.cross(p.toward_viewer()).length_squared();
        let _ = n;
        let up = {
            let helper = if m.dir.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
            let v = m.dir.cross(helper).normalize_or_zero();
            v
        };
        let pts: Vec<egui::Pos2> = (0..=48)
            .filter_map(|i| {
                let t = i as f32 / 48.0 * std::f32::consts::TAU;
                p.screen(m.origin + (m.dir * t.cos() + up * t.sin()) * r)
            })
            .collect();
        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0_f32, color.gamma_multiply(0.7))));
    } else if let Some(o) = p.screen(m.origin) {
        painter.line_segment([o, hp], egui::Stroke::new(1.0_f32, color.gamma_multiply(0.7)));
    }
    painter.rect_filled(egui::Rect::from_center_size(hp, egui::vec2(8.0, 8.0)), 1.0, color);
    if hot {
        painter.text(egui::pos2(hp.x + 8.0, hp.y - 8.0), egui::Align2::LEFT_BOTTOM, m.name, egui::FontId::proportional(11.0), color);
    }
}

/// Picking a gizmo handle also sets the Axis Constraints toolbar.
fn set_constraint_from_handle(app: &mut LoomApp, handle: gizmo::Handle) {
    use loom_commands::AxisConstraint as C;
    let c = match handle {
        gizmo::Handle::Axis(Axis::X) => C::X,
        gizmo::Handle::Axis(Axis::Y) => C::Y,
        gizmo::Handle::Axis(Axis::Z) => C::Z,
        gizmo::Handle::Plane(gizmo::Plane::XY) => C::XY,
        gizmo::Handle::Plane(gizmo::Plane::YZ) => C::YZ,
        gizmo::Handle::Plane(gizmo::Plane::ZX) => C::ZX,
        gizmo::Handle::Uniform | gizmo::Handle::Free => C::XYZ,
        gizmo::Handle::CenterBox | gizmo::Handle::Screen => return,
    };
    app.state.axis_constraint = c;
}

/// World boxes the selection brackets go around: each selected object's drawn
/// result (a skinned mesh as posed, not its bind-pose cage); a group head, which
/// has no mesh, gets its members' bounds.
pub(crate) fn selection_bracket_boxes(app: &LoomApp) -> Vec<(Vec3, Vec3)> {
    let bounds = |i: usize| app.world_display().get(i).filter(|m| m.vertex_count() > 0).map(|m| m.aabb());
    let drawn = |id: NodeId| app.world_meshes().iter().position(|(n, _)| *n == id).and_then(bounds);
    let mut boxes: Vec<(Vec3, Vec3)> = Vec::new();
    for id in app.state.selection.iter() {
        let Some(n) = app.state.scene.node(*id) else { continue };
        if matches!(n.object, loom_scene::ObjectKind::Group { .. }) {
            let members = app.state.scene.descendants(*id).into_iter().filter_map(drawn);
            if let Some(b) = members.reduce(|a, b| (a.0.min(b.0), a.1.max(b.1))) {
                boxes.push(b);
            }
        }
    }
    for (i, (id, _)) in app.world_meshes().iter().enumerate() {
        if !app.state.selection.contains(id) {
            continue;
        }
        if let Some(b) = bounds(i) {
            boxes.push(b);
        }
    }
    boxes
}

fn draw_selection_brackets(painter: &egui::Painter, app: &LoomApp, view_proj: Mat4, viewport_rect: egui::Rect) {
    let stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    for (mn, mx) in selection_bracket_boxes(app) {
        let mut bounds: Option<egui::Rect> = None;
        for i in 0..8 {
            let c = Vec3::new(if i & 1 == 0 { mn.x } else { mx.x }, if i & 2 == 0 { mn.y } else { mx.y }, if i & 4 == 0 { mn.z } else { mx.z });
            if let Some(sp) = project_world_to_screen(c, view_proj, viewport_rect) {
                bounds = Some(bounds.map_or(egui::Rect::from_min_max(sp, sp), |b| b.union(egui::Rect::from_min_max(sp, sp))));
            }
        }
        let Some(b) = bounds else { continue };
        let b = b.expand(4.0);
        let l = (b.width().min(b.height()) * 0.25).clamp(4.0, 12.0);
        for (corner, dx, dy) in [(b.left_top(), 1.0, 1.0), (b.right_top(), -1.0, 1.0), (b.left_bottom(), 1.0, -1.0), (b.right_bottom(), -1.0, -1.0)] {
            painter.line_segment([corner, corner + egui::vec2(dx * l, 0.0)], stroke);
            painter.line_segment([corner, corner + egui::vec2(0.0, dy * l)], stroke);
        }
    }
}

/// Safe frames: the render live area (default 4:3 output) with the action-
/// safe (90 %) and title-safe (80 %) regions.
/// The selection's motion paths: a line through every frame, white frame
/// ticks, red key boxes (white when selected), the current frame in yellow.
fn draw_trajectories(painter: &egui::Painter, app: &LoomApp, view_proj: glam::Mat4, rect: egui::Rect) {
    let frame = app.state.current_frame;
    for id in &app.state.selection {
        let Some((pts, keys)) = loom_commands::motion::trajectory(&app.state, *id) else { continue };
        let screen: Vec<Option<egui::Pos2>> = pts.iter().map(|(_, p)| project_world_to_screen(*p, view_proj, rect)).collect();
        for w in screen.windows(2) {
            if let (Some(a), Some(b)) = (w[0], w[1]) {
                painter.line_segment([a, b], egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(220, 220, 220)));
            }
        }
        for ((f, _), s) in pts.iter().zip(&screen) {
            let Some(s) = s else { continue };
            if keys.contains(f) {
                let sel = app.state.trajectory_keys && app.state.trajectory_selected.contains(f);
                let r = egui::Rect::from_center_size(*s, egui::vec2(7.0, 7.0));
                painter.rect_filled(r, 0.0, if sel { egui::Color32::WHITE } else { egui::Color32::from_rgb(220, 40, 40) });
            } else {
                painter.circle_filled(*s, 1.5, egui::Color32::WHITE);
            }
            if *f == frame {
                painter.circle_stroke(*s, 5.0, egui::Stroke::new(1.5_f32, egui::Color32::YELLOW));
            }
        }
    }
}

/// The trajectory key frame under a screen point, if any.
fn trajectory_hit(app: &LoomApp, view_proj: glam::Mat4, rect: egui::Rect, p: egui::Pos2) -> Option<i32> {
    for id in &app.state.selection {
        let Some((pts, keys)) = loom_commands::motion::trajectory(&app.state, *id) else { continue };
        for (f, w) in &pts {
            if keys.contains(f) && project_world_to_screen(*w, view_proj, rect).is_some_and(|s| s.distance(p) < 6.0) {
                return Some(*f);
            }
        }
    }
    None
}

fn draw_safe_frames(painter: &egui::Painter, rect: egui::Rect) {
    let aspect = 4.0 / 3.0;
    let (w, h) = if rect.width() / rect.height() > aspect { (rect.height() * aspect, rect.height()) } else { (rect.width(), rect.width() / aspect) };
    let live = egui::Rect::from_center_size(rect.center(), egui::vec2(w, h));
    painter.rect_stroke(live, 0.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 230, 0)), egui::StrokeKind::Middle);
    painter.rect_stroke(live.shrink2(egui::vec2(w * 0.05, h * 0.05)), 0.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(90, 200, 255)), egui::StrokeKind::Middle);
    painter.rect_stroke(live.shrink2(egui::vec2(w * 0.10, h * 0.10)), 0.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 150, 60)), egui::StrokeKind::Middle);
}

/// The soft selection gradient: red (1) → orange → yellow → blue (0).
fn soft_color(w: f32) -> egui::Color32 {
    let w = w.clamp(0.0, 1.0);
    let lerp = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    let c = if w > 0.66 { lerp([255.0, 160.0, 0.0], [255.0, 0.0, 0.0], (w - 0.66) / 0.34) } else if w > 0.33 { lerp([255.0, 255.0, 0.0], [255.0, 160.0, 0.0], (w - 0.33) / 0.33) } else { lerp([60.0, 90.0, 255.0], [255.0, 255.0, 0.0], w / 0.33) };
    egui::Color32::from_rgb(c[0] as u8, c[1] as u8, c[2] as u8)
}

/// Soft Selection weights: vertex dots (and faces with Shaded Face Toggle)
/// coloured by weight.
fn draw_soft_selection(painter: &egui::Painter, app: &LoomApp, view_proj: Mat4, rect: egui::Rect) {
    if app.state.selection_mode == SelectionMode::Object {
        return;
    }
    for (id, mesh) in app.world_meshes() {
        let on = app.state.scene.node(*id).and_then(|n| n.edit_poly()).is_some_and(|p| p.soft.on);
        if !on {
            continue;
        }
        let w = app.state.soft_weights(*id);
        if w.len() != mesh.vertex_count() {
            continue;
        }
        if app.state.soft_shaded_faces {
            for (fi, _) in mesh.faces().iter().enumerate() {
                let Some(ids) = mesh.face_vertex_ids(FaceId(fi as u32)) else { continue };
                let avg = ids.iter().map(|v| w[v.0 as usize]).sum::<f32>() / ids.len().max(1) as f32;
                if avg <= 0.0 {
                    continue;
                }
                let pts: Vec<egui::Pos2> = ids.iter().filter_map(|v| mesh.vertex_position(*v)).filter_map(|p| project_world_to_screen(p, view_proj, rect)).collect();
                if pts.len() >= 3 {
                    painter.add(egui::Shape::convex_polygon(pts, soft_color(avg).gamma_multiply(0.45), egui::Stroke::NONE));
                }
            }
        }
        for (i, v) in mesh.vertices().iter().enumerate() {
            if w[i] <= 0.0 || w[i] >= 1.0 {
                continue;
            }
            if let Some(sp) = project_world_to_screen(v.position, view_proj, rect) {
                painter.circle_filled(sp, 4.0, soft_color(w[i]));
            }
        }
    }
}

/// One sub-object outline / dot in a colour.
fn draw_sub_object(painter: &egui::Painter, mesh: &Mesh, id: SubObjectId, view_proj: Mat4, rect: egui::Rect, color: egui::Color32) {
    let stroke = egui::Stroke::new(2.0, color);
    match id {
        SubObjectId::Vertex(v) => {
            if let Some(sp) = mesh.vertex_position(v).and_then(|p| project_world_to_screen(p, view_proj, rect)) {
                painter.circle_filled(sp, 5.0, color);
            }
        }
        SubObjectId::Edge(e) => draw_edge_overlay(painter, mesh, e, view_proj, rect, stroke),
        SubObjectId::Face(f) => {
            if let Some(ids) = mesh.face_vertex_ids(f) {
                for i in 0..ids.len() {
                    draw_segment_overlay(painter, mesh, ids[i], ids[(i + 1) % ids.len()], view_proj, rect, stroke);
                }
            }
        }
    }
}

fn draw_edge_overlay(
    painter: &egui::Painter,
    mesh: &Mesh,
    edge_id: HalfEdgeId,
    view_proj: Mat4,
    viewport_rect: egui::Rect,
    stroke: egui::Stroke,
) {
    if let Some((a, b)) = mesh.edge_vertices(edge_id) {
        draw_segment_overlay(painter, mesh, a, b, view_proj, viewport_rect, stroke);
    }
}

fn draw_segment_overlay(
    painter: &egui::Painter,
    mesh: &Mesh,
    a: VertexId,
    b: VertexId,
    view_proj: Mat4,
    viewport_rect: egui::Rect,
    stroke: egui::Stroke,
) {
    let Some(a) = mesh.vertex_position(a) else {
        return;
    };
    let Some(b) = mesh.vertex_position(b) else {
        return;
    };
    let Some(sa) = project_world_to_screen(a, view_proj, viewport_rect) else {
        return;
    };
    let Some(sb) = project_world_to_screen(b, view_proj, viewport_rect) else {
        return;
    };
    painter.line_segment([sa, sb], stroke);
}

/// Project a world-space point through view*proj into egui screen pixels.
/// Returns `None` if the point is behind the camera (w <= 0).
pub(crate) fn project_world_to_screen(
    world: Vec3,
    view_proj: Mat4,
    viewport_rect: egui::Rect,
) -> Option<egui::Pos2> {
    let clip = view_proj * glam::Vec4::new(world.x, world.y, world.z, 1.0);
    if clip.w <= 1e-6 {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    let sx = viewport_rect.min.x + (ndc_x + 1.0) * 0.5 * viewport_rect.width();
    let sy = viewport_rect.min.y + (1.0 - ndc_y) * 0.5 * viewport_rect.height();
    Some(egui::Pos2::new(sx, sy))
}

fn distance_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let p = Vec2::new(p.x, p.y);
    let a = Vec2::new(a.x, a.y);
    let b = Vec2::new(b.x, b.y);
    let ab = b - a;
    let denom = ab.length_squared();
    if denom <= f32::EPSILON {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / denom).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let eps = 1e-6;
    let edge1 = b - a;
    let edge2 = c - a;
    let h = dir.cross(edge2);
    let det = edge1.dot(h);
    if det.abs() < eps {
        return None;
    }
    let inv_det = 1.0 / det;
    let s = origin - a;
    let u = inv_det * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(edge1);
    let v = inv_det * dir.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = inv_det * edge2.dot(q);
    (t > eps).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_segment_distance_hits_segment_body() {
        let p = egui::Pos2::new(5.0, 3.0);
        let a = egui::Pos2::new(0.0, 0.0);
        let b = egui::Pos2::new(10.0, 0.0);
        assert!((distance_to_segment(p, a, b) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn ray_triangle_hits_front_face() {
        let t = ray_triangle(
            Vec3::new(0.25, 0.25, 1.0),
            -Vec3::Z,
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
        );
        assert!(t.is_some_and(|t| (t - 1.0).abs() < 1e-5));
    }
}

/// Perspective background: dark at the top, where the viewport labels sit,
/// lighter toward the bottom.
const GRADIENT_TOP: egui::Color32 = egui::Color32::from_rgb(38, 38, 38);
const GRADIENT_BOTTOM: egui::Color32 = egui::Color32::from_rgb(96, 96, 96);

fn draw_background_gradient(painter: &egui::Painter, rect: egui::Rect) {
    let mut mesh = egui::Mesh::default();
    for (pos, color) in [(rect.left_top(), GRADIENT_TOP), (rect.right_top(), GRADIENT_TOP), (rect.right_bottom(), GRADIENT_BOTTOM), (rect.left_bottom(), GRADIENT_BOTTOM)] {
        mesh.colored_vertex(pos, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Viewport Clipping's slider on the viewport's right edge: the top arrow is
/// the far plane, the bottom one the near plane, over the eye-to-scene range.
fn clip_slider(ui: &mut egui::Ui, painter: &egui::Painter, app: &mut LoomApp, index: usize, rect: egui::Rect) {
    let track = egui::Rect::from_min_max(egui::pos2(rect.right() - 16.0, rect.top() + 140.0), egui::pos2(rect.right() - 6.0, rect.bottom() - 16.0));
    if track.height() < 40.0 {
        return;
    }
    let vp = &app.state.viewports[index];
    let eye = app.camera_for(index).orbit.eye_position();
    // Range: from the eye to the far side of the scene (or a bit past the target).
    let reach = app.state.scene_bounds().map(|(lo, hi)| {
        (0..8).map(|i| Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z }).distance(eye)).fold(0.0_f32, f32::max)
    });
    let range = reach.unwrap_or(0.0).max(vp.camera.distance * 2.0).max(vp.clip.0 + 1.0);
    let y_of = |d: f32| track.bottom() - (d / range).clamp(0.0, 1.0) * track.height();
    let d_of = |y: f32| ((track.bottom() - y) / track.height()).clamp(0.0, 1.0) * range;
    let (near, far) = vp.clip;
    painter.rect_filled(egui::Rect::from_center_size(track.center(), egui::vec2(2.0, track.height())), 0.0, egui::Color32::from_gray(150));
    let arrow = |y: f32, up: bool, hot: bool| {
        let (x0, x1) = (track.left(), track.right());
        let tip = if up { y - 6.0 } else { y + 6.0 };
        let c = if hot { egui::Color32::from_rgb(255, 210, 60) } else { egui::Color32::from_gray(220) };
        painter.add(egui::Shape::convex_polygon(vec![egui::pos2(x0, y), egui::pos2(x1, y), egui::pos2((x0 + x1) * 0.5, tip)], c, egui::Stroke::new(1.0_f32, egui::Color32::BLACK)));
    };
    let mut changed = None;
    for (which, d) in [(1usize, far), (0usize, near)] {
        let y = y_of(d.min(range));
        let hit = egui::Rect::from_center_size(egui::pos2(track.center().x, y), egui::vec2(18.0, 14.0));
        let r = ui.interact(hit, ui.id().with(("clip_slider", index, which)), egui::Sense::drag());
        if r.dragged() {
            if let Some(p) = r.interact_pointer_pos() {
                changed = Some((which, d_of(p.y)));
            }
        }
        arrow(y, which == 1, r.hovered() || r.dragged());
    }
    if let Some((which, d)) = changed {
        let (n, f) = if which == 0 { (d.min(far - 1e-3), far) } else { (near, d.max(near + 1e-3)) };
        let _ = app.dispatch(Command::SetViewportClipping { index: Some(index as u32), near: n, far: f });
    }
}

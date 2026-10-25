//! A small curve graph (Skin Morph's soft selection and Custom Falloff):
//! a Move / Insert toolbar, points dragged in the graph, a right-click on a
//! point deletes it. The curve runs straight between its points.

use eframe::egui;

const POINT: f32 = 6.0;

/// Draws the graph of `points` (x and y in 0..1) and returns the edited
/// points when an edit finishes (a drag released, an inserted or deleted point).
pub fn show(ui: &mut egui::Ui, id_salt: &str, points: &[[f32; 2]]) -> Option<Vec<[f32; 2]>> {
    let id = ui.id().with(("curve_graph", id_salt));
    let insert_id = id.with("insert");
    let mut insert: bool = ui.ctx().data(|d| d.get_temp(insert_id)).unwrap_or(false);
    ui.horizontal(|ui| {
        if ui.selectable_label(!insert, "Move").on_hover_text("Drag points in the graph").clicked() {
            insert = false;
        }
        if ui.selectable_label(insert, "Insert").on_hover_text("Click in the graph to add a point").clicked() {
            insert = true;
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(insert_id, insert));
    let width = ui.available_width().clamp(120.0, 220.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 90.0), egui::Sense::click_and_drag());
    let inner = rect.shrink(4.0);
    let to_screen = |p: [f32; 2]| egui::pos2(inner.left() + p[0] * inner.width(), inner.bottom() - p[1].clamp(-0.1, 1.1) * inner.height());
    let to_graph = |s: egui::Pos2| [((s.x - inner.left()) / inner.width()).clamp(0.0, 1.0), ((inner.bottom() - s.y) / inner.height()).clamp(0.0, 1.0)];
    // The points being dragged live in memory until the release.
    let drag_id = id.with("drag");
    let mut dragging: Option<(usize, Vec<[f32; 2]>)> = ui.ctx().data(|d| d.get_temp(drag_id));
    let shown: Vec<[f32; 2]> = dragging.as_ref().map(|d| d.1.clone()).unwrap_or_else(|| points.to_vec());
    let nearest = |pos: egui::Pos2| shown.iter().enumerate().map(|(i, p)| (i, to_screen(*p).distance(pos))).filter(|(_, d)| *d <= POINT + 2.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i);
    let mut result = None;
    if response.drag_started() && !insert {
        if let Some(i) = response.interact_pointer_pos().and_then(nearest) {
            dragging = Some((i, shown.clone()));
        }
    }
    if let (Some((i, pts)), Some(pos)) = (dragging.as_mut(), response.interact_pointer_pos()) {
        let mut g = to_graph(pos);
        let last = pts.len() - 1;
        // The ends stay at x 0 and 1; inner points stay between their neighbours.
        g[0] = if *i == 0 { 0.0 } else if *i == last { 1.0 } else { g[0].clamp(pts[*i - 1][0], pts[*i + 1][0]) };
        pts[*i] = g;
    }
    if response.drag_stopped() {
        if let Some((_, pts)) = dragging.take() {
            result = Some(pts);
        }
    }
    if response.clicked() && insert {
        if let Some(pos) = response.interact_pointer_pos() {
            let mut pts = shown.clone();
            pts.push(to_graph(pos));
            pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
            result = Some(pts);
        }
    }
    if response.secondary_clicked() {
        if let Some(i) = response.interact_pointer_pos().and_then(nearest) {
            if i != 0 && i != shown.len() - 1 {
                let mut pts = shown.clone();
                pts.remove(i);
                result = Some(pts);
            }
        }
    }
    ui.ctx().data_mut(|d| match &dragging {
        Some(v) => d.insert_temp(drag_id, v.clone()),
        None => d.remove::<(usize, Vec<[f32; 2]>)>(drag_id),
    });
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let grid = egui::Stroke::new(1.0_f32, visuals.widgets.noninteractive.bg_stroke.color);
    for k in 1..4 {
        let t = k as f32 / 4.0;
        painter.line_segment([to_screen([t, 0.0]), to_screen([t, 1.0])], grid);
        painter.line_segment([to_screen([0.0, t]), to_screen([1.0, t])], grid);
    }
    let curve: Vec<egui::Pos2> = shown.iter().map(|p| to_screen(*p)).collect();
    painter.add(egui::Shape::line(curve.clone(), egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(230, 230, 230))));
    for (i, c) in curve.iter().enumerate() {
        let active = dragging.as_ref().is_some_and(|d| d.0 == i);
        painter.rect_filled(egui::Rect::from_center_size(*c, egui::vec2(POINT, POINT)), 0.0, if active { egui::Color32::from_rgb(255, 255, 255) } else { egui::Color32::from_rgb(240, 200, 60) });
    }
    if dragging.is_some() {
        ui.ctx().request_repaint();
    }
    result
}

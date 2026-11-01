//! xView (Views menu / [+] menu): mesh checks on the selected Editable Poly
//! and Editable Mesh objects, shown in the viewports as highlighted
//! sub-objects with the test's name and result.

use crate::state::AppState;
use glam::Vec3;
use loom_mesh::Mesh;
use loom_scene::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum XViewTest {
    FaceOrientation,
    OverlappingFaces,
    OpenEdges,
    MultipleEdges,
    IsolatedVertices,
    OverlappingVertices,
    MissingUvw,
    FlippedUvwFaces,
}

impl XViewTest {
    pub const ALL: [XViewTest; 8] = [
        XViewTest::FaceOrientation, XViewTest::OverlappingFaces, XViewTest::OpenEdges, XViewTest::MultipleEdges,
        XViewTest::IsolatedVertices, XViewTest::OverlappingVertices, XViewTest::MissingUvw, XViewTest::FlippedUvwFaces,
    ];

    pub fn label(self) -> &'static str {
        match self {
            XViewTest::FaceOrientation => "Face Orientation",
            XViewTest::OverlappingFaces => "Overlapping Faces",
            XViewTest::OpenEdges => "Open Edges",
            XViewTest::MultipleEdges => "Multiple Edges",
            XViewTest::IsolatedVertices => "Isolated Vertices",
            XViewTest::OverlappingVertices => "Overlapping Vertices",
            XViewTest::MissingUvw => "Missing UVW Coordinates",
            XViewTest::FlippedUvwFaces => "Flipped UVW Faces",
        }
    }

    pub fn parse(s: &str) -> Option<XViewTest> {
        Self::ALL.into_iter().find(|t| t.label().eq_ignore_ascii_case(s.trim()))
    }

    /// The tests with a Tolerance (Configure).
    pub fn configurable(self) -> bool {
        matches!(self, XViewTest::OverlappingFaces | XViewTest::OverlappingVertices)
    }
}

/// What a test flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum XViewKind {
    Vertices,
    Edges,
    Faces,
}

/// xView settings and the active test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XView {
    pub test: Option<XViewTest>,
    pub tolerance: f32,
    pub see_through: bool,
    pub auto_update: bool,
    pub display_on_top: bool,
}

impl Default for XView {
    fn default() -> Self {
        Self { test: None, tolerance: 0.001, see_through: false, auto_update: true, display_on_top: false }
    }
}

/// One object's flagged sub-objects (vertex, half-edge or face indices).
#[derive(Debug, Clone, PartialEq)]
pub struct XViewHits {
    pub node: NodeId,
    pub kind: XViewKind,
    pub items: Vec<u32>,
}

/// The kind of sub-object a test flags.
pub fn kind_of(test: XViewTest) -> XViewKind {
    match test {
        XViewTest::OpenEdges | XViewTest::MultipleEdges => XViewKind::Edges,
        XViewTest::IsolatedVertices | XViewTest::OverlappingVertices => XViewKind::Vertices,
        _ => XViewKind::Faces,
    }
}

/// Runs `test` on one mesh (object space; the tolerance is in its units).
pub fn check(mesh: &Mesh, test: XViewTest, tolerance: f32, eye: Option<Vec3>) -> Vec<u32> {
    let hes = mesh.half_edges();
    match test {
        XViewTest::OpenEdges => (0..hes.len())
            .filter(|i| hes[*i].face.is_some() && hes[hes[*i].twin.0 as usize].face.is_none())
            .map(|i| i as u32)
            .collect(),
        // A half-edge structure holds each edge between at most two faces.
        XViewTest::MultipleEdges => Vec::new(),
        XViewTest::IsolatedVertices => mesh.vertices().iter().enumerate().filter(|(_, v)| v.he.is_none()).map(|(i, _)| i as u32).collect(),
        XViewTest::OverlappingVertices => {
            let vs = mesh.vertices();
            let mut out = Vec::new();
            for i in 0..vs.len() {
                if vs.iter().enumerate().any(|(j, w)| j != i && (w.position - vs[i].position).length() <= tolerance) {
                    out.push(i as u32);
                }
            }
            out
        }
        XViewTest::OverlappingFaces => {
            let faces: Vec<(Vec3, Vec3, Vec<Vec3>)> = (0..mesh.face_count())
                .filter_map(|f| {
                    let pts = mesh.face_positions(loom_mesh::FaceId(f as u32))?;
                    let n = mesh.face_normal(loom_mesh::FaceId(f as u32));
                    Some((n, pts[0], pts))
                })
                .collect();
            let mut out = Vec::new();
            for (i, (n, p0, pts)) in faces.iter().enumerate() {
                // Another face coplanar with this one and with a vertex of it inside this face.
                let hit = faces.iter().enumerate().any(|(j, (m, q0, qs))| {
                    j != i && n.cross(*m).length() < 1e-4 && (*q0 - *p0).dot(*n).abs() <= tolerance && qs.iter().any(|q| inside(pts, *n, *q) && !pts.iter().any(|p| (*p - *q).length() <= tolerance))
                });
                if hit {
                    out.push(i as u32);
                }
            }
            out
        }
        // Faces whose back is toward the viewer.
        XViewTest::FaceOrientation => match eye {
            Some(e) => (0..mesh.face_count())
                .filter(|f| {
                    let id = loom_mesh::FaceId(*f as u32);
                    mesh.face_positions(id).is_some_and(|p| mesh.face_normal(id).dot(e - p[0]) < 0.0)
                })
                .map(|f| f as u32)
                .collect(),
            None => Vec::new(),
        },
        XViewTest::MissingUvw => match mesh.map(1) {
            None => (0..mesh.face_count() as u32).collect(),
            Some(_) => (0..mesh.face_count())
                .filter(|f| mesh.face_corners(loom_mesh::FaceId(*f as u32)).is_some_and(|cs| cs.iter().any(|c| mesh.corner_uv(1, *c).is_none())))
                .map(|f| f as u32)
                .collect(),
        },
        XViewTest::FlippedUvwFaces => (0..mesh.face_count())
            .filter(|f| {
                let Some(cs) = mesh.face_corners(loom_mesh::FaceId(*f as u32)) else { return false };
                let uv: Vec<Vec3> = cs.iter().filter_map(|c| mesh.corner_uv(1, *c)).collect();
                if uv.len() != cs.len() || uv.len() < 3 {
                    return false;
                }
                // Signed area in UV: negative winding is flipped.
                let area: f32 = (0..uv.len()).map(|k| uv[k].x * uv[(k + 1) % uv.len()].y - uv[(k + 1) % uv.len()].x * uv[k].y).sum();
                area < 0.0
            })
            .map(|f| f as u32)
            .collect(),
    }
}

/// Whether `q` lies inside the convex-or-not polygon `pts` (normal `n`), by winding.
fn inside(pts: &[Vec3], n: Vec3, q: Vec3) -> bool {
    let mut angle = 0.0;
    for k in 0..pts.len() {
        let (a, b) = (pts[k] - q, pts[(k + 1) % pts.len()] - q);
        if a.length() < 1e-6 || b.length() < 1e-6 {
            return false;
        }
        angle += a.cross(b).dot(n).atan2(a.dot(b));
    }
    angle.abs() > std::f32::consts::PI
}

/// The active test on every selected Editable Poly / Mesh object (others are
/// unsupported), with the message the viewport shows.
pub fn results(state: &AppState, eye: Option<Vec3>) -> (Vec<XViewHits>, String) {
    let Some(test) = state.xview.test else { return (Vec::new(), String::new()) };
    if state.selection.is_empty() {
        return (Vec::new(), "No selection".into());
    }
    let mut hits = Vec::new();
    for id in &state.selection {
        let Some(n) = state.scene.node(*id) else { continue };
        if !matches!(n.object, loom_scene::ObjectKind::EditablePoly(_) | loom_scene::ObjectKind::EditableMesh(_)) {
            continue;
        }
        let mesh = n.cage_mesh();
        let local_eye = eye.map(|e| state.scene.object_matrix(*id).inverse().transform_point3(e));
        hits.push(XViewHits { node: *id, kind: kind_of(test), items: check(&mesh, test, state.xview.tolerance, local_eye) });
    }
    if hits.is_empty() {
        return (hits, "Unsupported object type".into());
    }
    let count: usize = hits.iter().map(|h| h.items.len()).sum();
    let what = match kind_of(test) {
        XViewKind::Vertices => "Vertices",
        XViewKind::Edges => "Edges",
        XViewKind::Faces => "Faces",
    };
    (hits, format!("{count} {what}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_edges_isolated_and_overlapping_vertices() {
        // A plane's four border edges are open; a box has none.
        let plane = loom_mesh::standard_primitives::plane_segmented(10.0, 10.0, 1, 1);
        assert_eq!(check(&plane, XViewTest::OpenEdges, 0.001, None).len(), 4);
        let bx = loom_mesh::box_primitive(Vec3::splat(5.0));
        assert!(check(&bx, XViewTest::OpenEdges, 0.001, None).is_empty());
        assert!(check(&bx, XViewTest::IsolatedVertices, 0.001, None).is_empty());
        assert!(check(&bx, XViewTest::OverlappingVertices, 0.001, None).is_empty());
        assert_eq!(check(&bx, XViewTest::OverlappingVertices, 100.0, None).len(), 8, "every corner within 100 of another");
        // Seen from straight above the centre, every face but the top turns its back.
        assert_eq!(check(&bx, XViewTest::FaceOrientation, 0.001, Some(Vec3::new(0.0, 0.0, 100.0))).len(), 5, "the four sides (seen from inside their planes) and the bottom");
    }
}

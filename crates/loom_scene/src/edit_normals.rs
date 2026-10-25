//! Edit Normals modifier: normals as groups of face corners. By default a
//! group is the corners of one vertex that share a smoothing group (what
//! the viewport shades); Break gives each corner its own normal, Unify
//! merges the selected normals at a vertex, Make Explicit / Set fix a
//! direction, Reset restores the default. The output writes every corner's
//! normal into the mesh's explicit-normals channel.

use glam::Vec3;
use loom_mesh::half_edge::MapChannel;
use loom_mesh::{FaceId, HalfEdgeId, Mesh, NORMALS_CHANNEL};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Corners outside any face (boundary half-edges) belong to no group.
pub const NO_GROUP: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditNormalsData {
    /// (vertices, faces, half-edges) of the input the edits were made on; zero before any edit.
    pub signature: (u32, u32, u32),
    /// Normal group per input half-edge (empty: the default grouping).
    pub groups: Vec<u32>,
    /// Fixed directions by group.
    pub explicit: BTreeMap<u32, Vec3>,
    pub selection: BTreeSet<u32>,
    /// Select By: 0 Normal, 1 Vertex, 2 Edge, 3 Face.
    pub select_by: u32,
    pub ignore_backfacing: bool,
    pub display_length: f32,
    /// Average > Use Threshold and its distance.
    pub use_threshold: bool,
    pub threshold: f32,
    /// Copy Normal's value.
    pub clipboard: Option<Vec3>,
    /// Connectivity hash of the input the edits were made on (0 in files
    /// written before it existed: then only the counts are compared).
    pub topology: u64,
}

impl Default for EditNormalsData {
    fn default() -> Self {
        Self { signature: (0, 0, 0), groups: Vec::new(), explicit: BTreeMap::new(), selection: BTreeSet::new(), select_by: 0, ignore_backfacing: false, display_length: 10.0, use_threshold: false, threshold: 0.1, clipboard: None, topology: 0 }
    }
}

/// Which vertex starts each corner and which face holds it: equal counts with
/// other connectivity (a turned edge, a reordered face) give another hash.
fn topology(m: &Mesh) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for he in m.half_edges() {
        (he.origin.0, he.next.0, he.face.map(|f| f.0)).hash(&mut h);
    }
    h.finish().max(1)
}

fn signature(m: &Mesh) -> (u32, u32, u32) {
    (m.vertex_count() as u32, m.face_count() as u32, m.half_edge_count() as u32)
}

/// The default grouping: corners of a vertex joined when their faces share a smoothing group bit.
pub fn default_groups(m: &Mesh) -> Vec<u32> {
    let he = m.half_edges();
    let mut groups = vec![NO_GROUP; he.len()];
    let mut by_vertex: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, h) in he.iter().enumerate() {
        if h.face.is_some() {
            by_vertex.entry(h.origin.0).or_default().push(i);
        }
    }
    let mut next = 0u32;
    for corners in by_vertex.values() {
        let sg = |c: usize| he[c].face.map(|f| m.faces()[f.0 as usize].smoothing_groups).unwrap_or(0);
        let mut parent: Vec<usize> = (0..corners.len()).collect();
        fn find(p: &mut [usize], i: usize) -> usize {
            let mut r = i;
            while p[r] != r {
                r = p[r];
            }
            p[i] = r;
            r
        }
        for a in 0..corners.len() {
            for b in a + 1..corners.len() {
                if sg(corners[a]) & sg(corners[b]) != 0 {
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    parent[ra] = rb;
                }
            }
        }
        let mut ids: BTreeMap<usize, u32> = BTreeMap::new();
        for k in 0..corners.len() {
            let r = find(&mut parent, k);
            let id = *ids.entry(r).or_insert_with(|| {
                next += 1;
                next - 1
            });
            groups[corners[k]] = id;
        }
    }
    groups
}

fn face_normal(m: &Mesh, he: usize) -> Vec3 {
    m.half_edges()[he].face.map(|f| m.face_normal(f)).unwrap_or(Vec3::ZERO)
}

impl EditNormalsData {
    /// Whether the edits still fit `input` (a topology change below drops them).
    pub fn fits(&self, input: &Mesh) -> bool {
        self.signature == (0, 0, 0) || (self.signature == signature(input) && (self.topology == 0 || self.topology == topology(input)))
    }

    pub fn has_edits(&self) -> bool {
        self.signature != (0, 0, 0)
    }

    /// The grouping in effect for `input`.
    pub fn groups_for(&self, input: &Mesh) -> Vec<u32> {
        if self.fits(input) && self.groups.len() == input.half_edge_count() {
            self.groups.clone()
        } else {
            default_groups(input)
        }
    }

    /// Every distinct normal group used by a face corner.
    pub fn normal_ids(&self, input: &Mesh) -> BTreeSet<u32> {
        self.groups_for(input).into_iter().filter(|g| *g != NO_GROUP).collect()
    }

    /// A group's normal: its fixed direction, else the average of its corners' face normals.
    pub fn normal(&self, input: &Mesh, groups: &[u32], g: u32) -> Vec3 {
        if let Some(d) = self.explicit.get(&g) {
            return d.normalize_or_zero();
        }
        let mut faces: BTreeSet<u32> = BTreeSet::new();
        let mut n = Vec3::ZERO;
        for (i, gg) in groups.iter().enumerate() {
            if *gg == g {
                if let Some(f) = input.half_edges()[i].face {
                    if faces.insert(f.0) {
                        n += face_normal(input, i);
                    }
                }
            }
        }
        n.normalize_or_zero()
    }

    /// Every group's normal in one pass over the corners: fixed directions,
    /// else the mean of the group's distinct faces' normals.
    pub fn normals(&self, input: &Mesh, groups: &[u32]) -> std::collections::HashMap<u32, Vec3> {
        let mut sum: std::collections::HashMap<u32, Vec3> = std::collections::HashMap::new();
        let mut seen: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
        for (i, g) in groups.iter().enumerate() {
            if *g == NO_GROUP || self.explicit.contains_key(g) {
                continue;
            }
            if let Some(f) = input.half_edges().get(i).and_then(|h| h.face) {
                if seen.insert((*g, f.0)) {
                    *sum.entry(*g).or_default() += face_normal(input, i);
                }
            }
        }
        let mut out: std::collections::HashMap<u32, Vec3> = sum.into_iter().map(|(g, n)| (g, n.normalize_or_zero())).collect();
        for (g, d) in &self.explicit {
            out.insert(*g, d.normalize_or_zero());
        }
        out
    }

    /// The modifier's output: the input with every corner's normal written, when edited.
    pub fn apply(&self, mut mesh: Mesh) -> (Mesh, Option<String>) {
        if !self.has_edits() {
            return (mesh, None);
        }
        if !self.fits(&mesh) {
            return (mesh, Some("Edit Normals: the topology below changed; its edits were dropped".into()));
        }
        let groups = self.groups_for(&mesh);
        let all = self.normals(&mesh, &groups);
        let mut verts = vec![Vec3::ZERO];
        let mut tv_of: BTreeMap<u32, u32> = BTreeMap::new();
        let mut corners = vec![0u32; mesh.half_edge_count()];
        for (i, g) in groups.iter().enumerate() {
            if *g == NO_GROUP {
                continue;
            }
            let tv = *tv_of.entry(*g).or_insert_with(|| {
                verts.push(all.get(g).copied().unwrap_or(Vec3::ZERO));
                (verts.len() - 1) as u32
            });
            corners[i] = tv;
        }
        mesh.set_map(NORMALS_CHANNEL, MapChannel { verts, corners });
        (mesh, None)
    }

    /// Starts editing against `input`: freezes the current grouping.
    fn begin(&mut self, input: &Mesh) -> Vec<u32> {
        let g = self.groups_for(input);
        if !self.fits(input) {
            self.explicit.clear();
            self.selection.clear();
        }
        self.signature = signature(input);
        self.topology = topology(input);
        self.groups = g.clone();
        g
    }

    /// When the input changed under stale edits, starts over on it now (the
    /// edits were already dropped from the output), so a selection made next
    /// names the new grouping and the following operation keeps it.
    pub fn rebase(&mut self, input: &Mesh) {
        if !self.fits(input) {
            self.begin(input);
        }
    }

    /// The groups an id list names under Select By.
    pub fn resolve(&self, input: &Mesh, ids: &[u32], by: u32) -> BTreeSet<u32> {
        let groups = self.groups_for(input);
        let he = input.half_edges();
        let mut out = BTreeSet::new();
        for (i, g) in groups.iter().enumerate() {
            if *g == NO_GROUP {
                continue;
            }
            let h = &he[i];
            let hit = match by {
                0 => ids.contains(g),
                1 => ids.contains(&h.origin.0),
                2 => {
                    // Edge ids: canonical half-edge ids; a corner belongs to the edges at its vertex.
                    let prev = h.prev.0 as usize;
                    let edges = [input.canonical_edge_id(HalfEdgeId(i as u32)), input.canonical_edge_id(HalfEdgeId(prev as u32))];
                    edges.iter().flatten().any(|e| ids.contains(&e.0))
                }
                _ => h.face.is_some_and(|f: FaceId| ids.contains(&f.0)),
            };
            if hit {
                out.insert(*g);
            }
        }
        out
    }

    /// Break: every corner of the selected normals gets its own normal.
    pub fn break_selected(&mut self, input: &Mesh) {
        let mut groups = self.begin(input);
        let mut next = groups.iter().filter(|g| **g != NO_GROUP).max().map(|m| m + 1).unwrap_or(0);
        let mut new_sel = BTreeSet::new();
        let mut seen: BTreeSet<u32> = BTreeSet::new();
        for g in groups.iter_mut() {
            if *g == NO_GROUP || !self.selection.contains(g) {
                continue;
            }
            let old = *g;
            if seen.insert(old) {
                // The first corner keeps the id.
                new_sel.insert(old);
                continue;
            }
            if let Some(d) = self.explicit.get(&old).copied() {
                self.explicit.insert(next, d);
            }
            *g = next;
            new_sel.insert(next);
            next += 1;
        }
        self.groups = groups;
        self.selection = new_sel;
    }

    /// Unify: the selected normals at each vertex become one (computed, not fixed).
    pub fn unify_selected(&mut self, input: &Mesh) {
        let mut groups = self.begin(input);
        let he = input.half_edges();
        let mut target: BTreeMap<u32, u32> = BTreeMap::new();
        for (i, g) in groups.iter_mut().enumerate() {
            if *g == NO_GROUP || !self.selection.contains(g) {
                continue;
            }
            let t = *target.entry(he[i].origin.0).or_insert(*g);
            self.explicit.remove(g);
            *g = t;
        }
        self.explicit.retain(|g, _| !target.values().any(|t| t == g));
        self.groups = groups;
        self.selection = target.values().copied().collect();
    }

    /// Make Explicit: the selected normals keep their current direction.
    pub fn make_explicit(&mut self, input: &Mesh) {
        let groups = self.begin(input);
        let all = self.normals(input, &groups);
        for g in self.selection.clone() {
            self.explicit.insert(g, all.get(&g).copied().unwrap_or(Vec3::ZERO));
        }
    }

    /// Specify: the selected normals take their own faces' average and stay
    /// that way (they no longer follow smoothing groups).
    pub fn specify(&mut self, input: &Mesh) {
        self.begin(input);
        for g in self.selection.clone() {
            self.explicit.remove(&g);
        }
    }

    pub fn set_direction(&mut self, input: &Mesh, dir: Vec3) {
        self.begin(input);
        let d = dir.normalize_or_zero();
        if d == Vec3::ZERO {
            return;
        }
        for g in self.selection.clone() {
            self.explicit.insert(g, d);
        }
    }

    pub fn rotate(&mut self, input: &Mesh, axis: Vec3, degrees: f32) {
        self.rotate_by(input, glam::Quat::from_axis_angle(axis.normalize_or_zero(), degrees.to_radians()));
    }

    /// Turns the selected normals by `q` (object space); they become explicit.
    pub fn rotate_by(&mut self, input: &Mesh, q: glam::Quat) {
        let groups = self.begin(input);
        let all = self.normals(input, &groups);
        for g in self.selection.clone() {
            let n = all.get(&g).copied().unwrap_or(Vec3::ZERO);
            self.explicit.insert(g, (q * n).normalize_or_zero());
        }
    }

    /// Reset: the corners of the selected normals go back to the default grouping and direction.
    pub fn reset(&mut self, input: &Mesh) {
        let mut groups = self.begin(input);
        let def = default_groups(input);
        let mut next = groups.iter().chain(def.iter()).filter(|g| **g != NO_GROUP).max().map(|m| m + 1).unwrap_or(0);
        let mut fresh: BTreeMap<u32, u32> = BTreeMap::new();
        let mut new_sel = BTreeSet::new();
        for (i, g) in groups.iter_mut().enumerate() {
            if *g == NO_GROUP || !self.selection.contains(g) {
                continue;
            }
            self.explicit.remove(g);
            let id = *fresh.entry(def[i]).or_insert_with(|| {
                next += 1;
                next - 1
            });
            *g = id;
            new_sel.insert(id);
        }
        self.groups = groups;
        self.selection = new_sel;
    }

    /// Average: the selected normals all take their mean (or, with Use
    /// Threshold, the mean of the selected normals within the threshold of
    /// each other); they stay separate normals.
    pub fn average(&mut self, input: &Mesh) {
        let groups = self.begin(input);
        let sel: Vec<u32> = self.selection.iter().copied().collect();
        // Each group's position: its first corner's vertex.
        let mut first: std::collections::HashMap<u32, Vec3> = std::collections::HashMap::new();
        for (i, g) in groups.iter().enumerate() {
            first.entry(*g).or_insert_with(|| input.vertices()[input.half_edges()[i].origin.0 as usize].position);
        }
        let pos = |g: u32| -> Vec3 { first.get(&g).copied().unwrap_or(Vec3::ZERO) };
        let all = self.normals(input, &groups);
        let normals: Vec<Vec3> = sel.iter().map(|g| all.get(g).copied().unwrap_or(Vec3::ZERO)).collect();
        for (k, g) in sel.iter().enumerate() {
            let p = pos(*g);
            let mean: Vec3 = sel
                .iter()
                .enumerate()
                .filter(|(j, h)| !self.use_threshold || *j == k || pos(**h).distance(p) <= self.threshold)
                .map(|(j, _)| normals[j])
                .sum::<Vec3>()
                .normalize_or_zero();
            self.explicit.insert(*g, mean);
        }
    }

    pub fn copy(&mut self, input: &Mesh) -> Option<Vec3> {
        let groups = self.groups_for(input);
        let g = *self.selection.iter().next()?;
        let n = self.normal(input, &groups, g);
        self.clipboard = Some(n);
        Some(n)
    }

    pub fn paste(&mut self, input: &Mesh) -> bool {
        let Some(n) = self.clipboard else { return false };
        self.set_direction(input, n);
        true
    }
}

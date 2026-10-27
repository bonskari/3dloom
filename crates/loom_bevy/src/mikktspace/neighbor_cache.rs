//! Cache only proven current-attribute welding and corrected manifold
//! adjacency after checking their complete current dependency sets.
use super::generated::STriInfo;

#[derive(PartialEq, Eq)]
struct Guard {
    classes: Vec<usize>,
    flags: Vec<(i32, i32)>,
}
struct Entry { guard: Guard, neighbors: Vec<[i32; 3]> }

#[derive(Default)]
pub(crate) struct NeighborCache {
    pub(crate) weld_validation: super::weld_validation::WeldValidation,
    entry: Option<Entry>,
    pending: Option<Guard>,
    #[cfg(test)]
    pub(crate) hits: usize,
    #[cfg(test)]
    pub(crate) misses: usize,
}

impl NeighborCache {
    pub(super) fn restore(&mut self, triangles: &mut [STriInfo], indices: &[i32]) -> bool {
        let _scope = loom_render::profile::Scope::new("mikk.neighbor_guard");
        let mut first = vec![usize::MAX; indices.iter().copied().max().unwrap_or(-1).saturating_add(1) as usize];
        let mut next = 0;
        let classes = indices.iter().map(|id| {
            let slot = &mut first[*id as usize];
            if *slot == usize::MAX { *slot = next; next += 1; }
            *slot
        }).collect();
        let current = Guard { classes, flags: triangles.iter().map(|t| (t.iOrgFaceNumber, t.iFlag)).collect() };
        if let Some(entry) = self.entry.as_ref().filter(|e| e.guard == current) {
            for (triangle, neighbors) in triangles.iter_mut().zip(&entry.neighbors) { triangle.FaceNeighbors = *neighbors; }
            #[cfg(test)] { self.hits += 1; }
            true
        } else {
            #[cfg(test)] { self.misses += 1; }
            self.pending = Some(current);
            false
        }
    }

    pub(super) fn store(&mut self, triangles: &[STriInfo]) {
        let Some(current) = self.pending.take() else { return; };
        let mut counts = std::collections::HashMap::new();
        for triangle in current.classes.chunks_exact(3) {
            for edge in 0..3 {
                let (a, b) = (triangle[edge], triangle[(edge + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_insert(0usize) += 1;
            }
        }
        // Multiple possible opposite faces need the complete fresh builder's
        // pairing order. They never enter the structural cache.
        self.entry = if counts.values().any(|count| *count > 2) { None } else {
            Some(Entry { guard: current, neighbors: triangles.iter().map(|t| t.FaceNeighbors).collect() })
        };
    }
}

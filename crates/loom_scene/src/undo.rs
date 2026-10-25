use crate::change::Change;
use crate::scene::Scene;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ENTRY_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct Entry {
    id: u64,
    label: String,
    change: Change,
}

/// Change-level undo/redo. Each entry stores the inverse of what was
/// applied, so undo is cheap even for large scenes.
#[derive(Debug, Default)]
pub struct UndoStack {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    limit: usize,
    /// An open interactive transaction (a gizmo drag): inverses collect
    /// here, exempt from the limit, and land as one entry on `end`.
    open: Option<(String, Vec<Change>)>,
    /// Nested `begin` calls while a transaction is open; `end` closes it
    /// only when the outermost one ends.
    depth: usize,
    /// Bumped on every commit, end, undo and redo: a stable identity for
    /// "the history changed since", unlike `len()` at the limit.
    version: u64,
    /// `undo.len()` when the scene was last saved; `None` once that point
    /// is unreachable (a commit after undoing below it, or trimmed away).
    saved: Option<usize>,
    /// Entries dropped off the bottom at the limit (`position` stays monotonic).
    trimmed: u64,
}

impl UndoStack {
    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            open: None,
            depth: 0,
            version: 0,
            saved: Some(0),
            trimmed: 0,
        }
    }

    /// Marks the current history position as the saved scene.
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.undo.len());
    }

    /// Fetched or merged content: no history position matches a file.
    pub fn mark_unsaved(&mut self) {
        self.saved = None;
    }

    /// True while an interactive transaction (a drag) is collecting steps.
    pub fn in_transaction(&self) -> bool {
        self.open.is_some()
    }

    /// True while the scene matches what was last saved (undoing back to
    /// the saved position counts as clean).
    pub fn is_saved(&self) -> bool {
        self.saved == Some(self.undo.len()) && self.open.as_ref().is_none_or(|(_, s)| s.is_empty())
    }

    /// A new entry on top: a saved position below the top is now unreachable.
    fn push_entry(&mut self, entry: (String, Change)) {
        if self.saved.is_some_and(|s| s > self.undo.len()) {
            self.saved = None;
        }
        self.undo.push(Entry { id: NEXT_ENTRY_ID.fetch_add(1, Ordering::Relaxed), label: entry.0, change: entry.1 });
        if self.undo.len() > self.limit {
            self.undo.remove(0);
            self.trimmed += 1;
            self.saved = match self.saved {
                Some(0) | None => None,
                Some(s) => Some(s - 1),
            };
        }
        self.redo.clear();
    }

    /// Preferences > Scene Undo Levels: trims the stack when lowered.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
        while self.undo.len() > self.limit {
            self.undo.remove(0);
            self.saved = match self.saved {
                Some(0) | None => None,
                Some(s) => Some(s - 1),
            };
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Applies `change`; on success records its inverse and clears redo.
    /// On error the scene is unchanged and nothing is recorded.
    pub fn commit(&mut self, label: &str, change: Change, scene: &mut Scene) -> Result<(), String> {
        let change = change.merged_transforms(scene);
        self.commit_exact(label, change, scene)
    }

    /// `commit` without the transform merge: the change's controllers are
    /// written as given (Assign Controller, key edits, Track View).
    pub fn commit_exact(&mut self, label: &str, change: Change, scene: &mut Scene) -> Result<(), String> {
        let inverse = change.inverse(scene);
        change.apply(scene)?;
        self.version += 1;
        if let Some((_, steps)) = self.open.as_mut() {
            steps.push(inverse);
            return Ok(());
        }
        self.push_entry((label.to_owned(), inverse));
        Ok(())
    }

    /// An entry that changes nothing in the scene (a selection change, whose
    /// state the caller keeps beside the stack): it takes its place in the
    /// undo order and clears redo, but does not make a saved scene unsaved.
    pub fn push_marker(&mut self, label: &str) {
        let at_saved = self.saved == Some(self.undo.len());
        self.version += 1;
        self.push_entry((label.to_owned(), Change::Group(Vec::new())));
        if at_saved {
            self.saved = Some(self.undo.len());
        }
    }

    /// Starts an interactive transaction; commits until `end` become one
    /// undo entry.
    pub fn begin(&mut self, label: &str) {
        if self.open.is_some() {
            self.depth += 1;
            return;
        }
        self.open = Some((label.to_owned(), Vec::new()));
    }

    /// Closes the open transaction, if any, into a single entry.
    pub fn end(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
            return;
        }
        let Some((label, mut steps)) = self.open.take() else { return };
        if steps.is_empty() {
            return;
        }
        // Inverses are collected oldest-first; undoing runs newest-first.
        steps.reverse();
        self.push_entry((label, Change::Group(steps)));
    }

    pub fn undo(&mut self, scene: &mut Scene) -> Option<String> {
        // A drag whose release was lost must not absorb later commits.
        self.depth = 0;
        self.end();
        let Entry { id, label, change: inverse } = self.undo.pop()?;
        self.version += 1;
        let redo = inverse.inverse(scene);
        if let Err(e) = inverse.apply(scene) {
            // Keep the entry so the history is not silently lost.
            self.undo.push(Entry { id, label: format!("{label} (failed: {e})"), change: inverse });
            return None;
        }
        self.redo.push(Entry { id, label: label.clone(), change: redo });
        Some(label)
    }

    pub fn redo(&mut self, scene: &mut Scene) -> Option<String> {
        self.depth = 0;
        self.end();
        let Entry { id, label, change } = self.redo.pop()?;
        self.version += 1;
        let inverse = change.inverse(scene);
        if let Err(e) = change.apply(scene) {
            self.redo.push(Entry { id, label: format!("{label} (failed: {e})"), change });
            return None;
        }
        self.undo.push(Entry { id, label: label.clone(), change: inverse });
        Some(label)
    }

    /// Stable identity of a history step, retained through undo/redo and
    /// never reused by a branch or a fresh stack. Sidecar state uses this,
    /// rather than a history position that can name a different step later.
    pub fn undo_entry_id(&self) -> Option<u64> {
        self.undo.last().map(|e| e.id)
    }

    pub fn redo_entry_id(&self) -> Option<u64> {
        self.redo.last().map(|e| e.id)
    }

    pub fn contains_entry(&self, id: u64) -> bool {
        self.undo.iter().chain(&self.redo).any(|e| e.id == id)
    }

    pub fn len(&self) -> usize {
        self.undo.len()
    }

    /// The history position counted from the first entry ever made: the
    /// Set Key buffer undoes back to it.
    pub fn position(&self) -> u64 {
        self.trimmed + self.undo.len() as u64
    }

    /// Changes with every commit / undo / redo (a live preview keys on it).
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    /// Labels of undoable operations, most recent last.
    pub fn labels(&self) -> Vec<String> {
        self.undo.iter().map(|e| e.label.clone()).collect()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::Change;
    use crate::node::{Node, ObjectKind, Transform};
    use crate::scene::Scene;
    use crate::NodeId;
    use glam::Vec3;

    fn add_box(stack: &mut UndoStack, scene: &mut Scene) -> NodeId {
        let node = Node::new("Box001", ObjectKind::box_object(1.0, 1.0, 1.0));
        stack
            .commit("Create Box", Change::AddNode(node), scene)
            .unwrap();
        scene.nodes().next().unwrap().id
    }

    #[test]
    fn commit_then_undo_restores_scene() {
        let (mut stack, mut scene) = (UndoStack::new(100), Scene::new());
        add_box(&mut stack, &mut scene);
        assert_eq!(scene.len(), 1);
        assert_eq!(stack.undo(&mut scene).as_deref(), Some("Create Box"));
        assert_eq!(scene.len(), 0);
    }

    #[test]
    fn redo_reapplies_and_new_commit_clears_redo() {
        let (mut stack, mut scene) = (UndoStack::new(100), Scene::new());
        let id = add_box(&mut stack, &mut scene);
        stack.undo(&mut scene);
        assert_eq!(stack.redo(&mut scene).as_deref(), Some("Create Box"));
        assert_eq!(scene.node(id).map(|n| n.name.as_str()), Some("Box001"));
        stack.undo(&mut scene);
        add_box(&mut stack, &mut scene);
        assert_eq!(stack.redo(&mut scene), None);
    }

    #[test]
    fn failed_group_leaves_scene_unchanged() {
        let (mut stack, mut scene) = (UndoStack::new(100), Scene::new());
        let before = serde_json::to_string(&scene).unwrap();
        let group = Change::Group(vec![
            Change::AddNode(Node::new("A", ObjectKind::box_object(1.0, 1.0, 1.0))),
            Change::SetTransform {
                id: NodeId(999),
                before: Transform::default(),
                after: Transform::default(),
            },
        ]);
        assert!(stack.commit("bad", group, &mut scene).is_err());
        assert_eq!(serde_json::to_string(&scene).unwrap(), before);
        assert_eq!(stack.undo(&mut scene), None);
    }

    #[test]
    fn set_transform_undo_restores_position() {
        let (mut stack, mut scene) = (UndoStack::new(100), Scene::new());
        let id = add_box(&mut stack, &mut scene);
        let before = scene.node(id).unwrap().transform.clone();
        let mut after = before.clone();
        after.position.set(Vec3::new(5.0, 0.0, 0.0));
        stack
            .commit("Move", Change::SetTransform { id, before, after }, &mut scene)
            .unwrap();
        assert_eq!(scene.node(id).unwrap().transform.position.get().x, 5.0);
        stack.undo(&mut scene);
        assert_eq!(scene.node(id).unwrap().transform.position.get().x, 0.0);
    }

    #[test]
    fn transaction_collapses_drag_steps_into_one_and_ignores_limit() {
        let (mut stack, mut scene) = (UndoStack::new(2), Scene::new());
        let id = add_box(&mut stack, &mut scene);
        stack.begin("Move");
        for i in 1..=150 {
            let before = scene.node(id).unwrap().transform.clone();
            let mut after = before.clone();
            after.position.set(Vec3::new(i as f32, 0.0, 0.0));
            stack.commit("step", Change::SetTransform { id, before, after }, &mut scene).unwrap();
        }
        stack.end();
        assert_eq!(stack.labels(), vec!["Create Box", "Move"]);
        stack.undo(&mut scene);
        assert_eq!(scene.node(id).unwrap().transform.position.get().x, 0.0);
        stack.redo(&mut scene);
        assert_eq!(scene.node(id).unwrap().transform.position.get().x, 150.0);
    }

    #[test]
    fn remove_node_inverse_restores_children() {
        let (mut stack, mut scene) = (UndoStack::new(100), Scene::new());
        let p = add_box(&mut stack, &mut scene);
        let mut child = Node::new("C", ObjectKind::box_object(1.0, 1.0, 1.0));
        child.transform.position.set(Vec3::new(2.0, 0.0, 0.0));
        stack.commit("Create C", Change::AddNode(child), &mut scene).unwrap();
        let c = scene.nodes().find(|n| n.name == "C").unwrap().id;
        scene.node_mut(p).unwrap().transform.position.set(Vec3::new(5.0, 0.0, 0.0));
        stack.commit("Link", Change::SetParent { id: c, before: None, after: Some(p) }, &mut scene).unwrap();
        assert_eq!(scene.node(c).unwrap().transform.position.get().x, 2.0, "SetParent::apply sets only the field");
        stack.commit("Delete", Change::RemoveNode(p), &mut scene).unwrap();
        assert_eq!(scene.node(c).unwrap().parent, None);
        assert_eq!(scene.node(c).unwrap().transform.position.get().x, 7.0);
        stack.undo(&mut scene);
        assert_eq!(scene.node(c).unwrap().parent, Some(p));
        assert_eq!(scene.node(c).unwrap().transform.position.get().x, 2.0);
    }

    #[test]
    fn limit_drops_oldest() {
        let (mut stack, mut scene) = (UndoStack::new(2), Scene::new());
        for _ in 0..3 {
            add_box(&mut stack, &mut scene);
        }
        assert_eq!(stack.labels().len(), 2);
        assert!(stack.undo(&mut scene).is_some());
        assert!(stack.undo(&mut scene).is_some());
        assert_eq!(stack.undo(&mut scene), None);
        assert_eq!(scene.len(), 1);
    }

    #[test]
    fn nested_begin_joins_the_outer_transaction() {
        let mut stack = UndoStack::new(10);
        let mut scene = Scene::new();
        stack.begin("Move");
        add_box(&mut stack, &mut scene);
        stack.begin("Clone");
        add_box(&mut stack, &mut scene);
        stack.end();
        add_box(&mut stack, &mut scene);
        stack.end();
        assert_eq!(scene.nodes().count(), 3);
        assert_eq!(stack.labels(), vec!["Move"]);
        stack.undo(&mut scene).unwrap();
        assert_eq!(scene.nodes().count(), 0);
    }
    #[test]
    fn entry_identity_survives_redo_but_never_branches_resets_or_eviction() {
        let mut stack = UndoStack::new(3);
        let mut scene = Scene::new();
        add_box(&mut stack, &mut scene);
        let first = stack.undo_entry_id().unwrap();
        stack.undo(&mut scene).unwrap();
        assert_eq!(stack.redo_entry_id(), Some(first));
        stack.redo(&mut scene).unwrap();
        assert_eq!(stack.undo_entry_id(), Some(first));
        stack.undo(&mut scene).unwrap();
        add_box(&mut stack, &mut scene);
        let branch = stack.undo_entry_id().unwrap();
        assert_ne!(branch, first);
        assert!(!stack.contains_entry(first));
        add_box(&mut stack, &mut scene);
        let latest = stack.undo_entry_id().unwrap();
        stack.set_limit(1);
        assert!(!stack.contains_entry(branch));
        assert_eq!(stack.undo_entry_id(), Some(latest));
        stack.undo(&mut scene).unwrap();
        assert_eq!(stack.redo_entry_id(), Some(latest));
        stack = UndoStack::new(1);
        add_box(&mut stack, &mut scene);
        assert_ne!(stack.undo_entry_id(), Some(latest));
    }

}

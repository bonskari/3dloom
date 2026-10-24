use crate::node::{Node, NodeId, Transform};
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A layer of the Layer Manager; layer 0 is "0 (default)".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: u32,
    pub name: String,
    pub hidden: bool,
    pub frozen: bool,
    /// The layer colour (objects whose colour is By Layer show it).
    #[serde(default = "default_layer_color")]
    pub color: [u8; 3],
    /// Render: off keeps the layer's objects out of renders.
    #[serde(default = "default_true")]
    pub renderable: bool,
}

fn default_layer_color() -> [u8; 3] {
    crate::OBJECT_COLORS[0]
}

fn default_true() -> bool {
    true
}

impl Layer {
    /// A new visible, unfrozen, renderable layer with a palette colour picked by its id.
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self { id, name: name.into(), hidden: false, frozen: false, color: crate::OBJECT_COLORS[id as usize % crate::OBJECT_COLORS.len()], renderable: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    nodes: BTreeMap<NodeId, Node>,
    next_id: u64,
    #[serde(default = "default_layers")]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub current_layer: u32,
    /// Hierarchy > IK chains.
    #[serde(default)]
    pub ik_chains: Vec<crate::ik::IkChain>,
    /// Track View note tracks, track sets and per-track flags.
    #[serde(default)]
    pub track_data: crate::tracks::TrackData,
    /// Every material in the scene file: assigned ones and the Material Editor's slots.
    #[serde(default)]
    pub materials: crate::material::MaterialLibrary,
    /// The Material Editor's sample slots (saved with the scene, as Max does).
    #[serde(default)]
    pub material_editor: crate::material::MaterialEditorSlots,
    /// Solved local matrices of the chains' bones, for the scene version they were solved at.
    #[serde(skip)]
    ik_cache: std::cell::RefCell<(u64, Option<f32>, std::collections::HashMap<NodeId, Mat4>)>,
    #[serde(skip)]
    ik_solving: std::cell::Cell<bool>,
    /// Nodes whose stack is being evaluated right now (cycle guard for eval lookups).
    #[serde(skip)]
    eval_in_progress: std::cell::RefCell<std::collections::HashSet<NodeId>>,
    /// Evaluated stacks keyed by (node, up-to entry) with the versions they were built from.
    #[serde(skip)]
    /// The frame is set when an animated parameter was read while building the result.
    eval_cache: std::cell::RefCell<std::collections::HashMap<(NodeId, Option<usize>), (u64, Option<f32>, std::sync::Arc<crate::modifier::Evaluated>)>>,
    /// Counts stack evaluations (tests pin the caching).
    #[serde(skip)]
    pub eval_count: std::cell::Cell<usize>,
    #[serde(skip)]
    version_counter: std::cell::Cell<u64>,
    /// The frame the scene evaluates at (the time slider).
    #[serde(skip)]
    time: std::cell::Cell<f32>,
    /// Nodes whose constraints are being solved (cycle guard).
    #[serde(skip)]
    constraint_busy: std::cell::RefCell<std::collections::HashSet<NodeId>>,
    /// Nodes whose Link Constraint frame is being solved (cycle guard).
    #[serde(skip)]
    link_busy: std::cell::RefCell<std::collections::HashSet<NodeId>>,
}

fn default_layers() -> Vec<Layer> {
    vec![Layer::new(0, "0 (default)")]
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    /// The material Material Editor slot `slot` shows: its library material,
    /// or the slot's untouched default.
    pub fn slot_material(&self, slot: usize) -> crate::material::StandardMaterial {
        self.material_editor
            .slots
            .get(slot)
            .copied()
            .flatten()
            .and_then(|id| self.materials.get(&id).cloned())
            .unwrap_or_else(|| crate::material::StandardMaterial::slot_default(slot))
    }

    /// The material a node renders with, if one is assigned.
    pub fn node_material(&self, id: NodeId) -> Option<&crate::material::StandardMaterial> {
        self.node(id).and_then(|n| n.material).and_then(|m| self.materials.get(&m))
    }

    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            next_id: 1,
            layers: default_layers(),
            current_layer: 0,
            eval_cache: Default::default(),
            eval_count: std::cell::Cell::new(0),
            version_counter: std::cell::Cell::new(1),
            ik_chains: Vec::new(),
            track_data: Default::default(),
            materials: Default::default(),
            material_editor: Default::default(),
            ik_cache: Default::default(),
            ik_solving: std::cell::Cell::new(false),
            eval_in_progress: Default::default(),
            time: std::cell::Cell::new(0.0),
            constraint_busy: Default::default(),
            link_busy: Default::default(),
        }
    }

    /// The frame the scene evaluates at.
    pub fn time(&self) -> f32 {
        self.time.get()
    }

    /// Moves the scene to frame `t`; parameters read on this thread follow.
    pub fn set_time(&self, t: f32) {
        self.time.set(t);
        crate::anim::set_time(t);
    }

    /// Runs `f` with the scene at frame `t` (trajectories, baking), then restores the frame.
    pub fn at_time<R>(&self, t: f32, f: impl FnOnce() -> R) -> R {
        let t0 = self.time.get();
        self.set_time(t);
        let r = f();
        self.set_time(t0);
        r
    }

    /// Makes the thread's ambient time the scene's before reading parameters.
    fn sync_time(&self) {
        let t = self.time.get();
        if crate::anim::time() != t {
            crate::anim::set_time(t);
        }
    }

    /// Marks the scene changed without touching a node (IK chain edits).
    pub fn bump_version(&self) {
        self.next_version();
    }

    fn next_version(&self) -> u64 {
        let v = self.version_counter.get() + 1;
        self.version_counter.set(v);
        v
    }

    /// Marks a node's evaluated result stale.
    pub fn touch(&mut self, id: NodeId) {
        let v = self.next_version();
        if let Some(n) = self.nodes.get_mut(&id) {
            n.version = v;
            n.geom_version = v;
        }
    }

    /// The cache key of a node: its version plus the versions of the nodes
    /// its instanced modifiers point at.
    fn eval_key(&self, id: NodeId) -> u64 {
        self.eval_key_depth(id, 0)
    }

    /// A referenced node's whole evaluation key folds in (a Skin Wrap base
    /// that is itself skinned), to a bounded depth.
    fn eval_key_depth(&self, id: NodeId, depth: u32) -> u64 {
        let Some(n) = self.nodes.get(&id) else { return 0 };
        let mut key = n.geom_version.wrapping_mul(1_000_003);
        // The node's own transform (and its ancestors') matters to modifiers that follow
        // another node: Linked XForm, and Skin / Skin Morph / Skin Wrap, whose result is
        // world-space geometry brought back into this node's frame.
        if n.modifiers.iter().any(|s| !s.modifier.referenced_nodes().is_empty() || s.modifier.uses_own_transform()) {
            key = key.wrapping_add(n.version.wrapping_mul(31));
            let mut cur = n.parent;
            let mut k = 37u64;
            for t in n.constraints.nodes() {
                key = key.wrapping_add(self.transform_key(t, 0).wrapping_mul(41));
            }
            while let Some(p) = cur {
                let Some(pn) = self.nodes.get(&p) else { break };
                key = key.wrapping_add(pn.version.wrapping_mul(k));
                for t in pn.constraints.nodes() {
                    key = key.wrapping_add(self.transform_key(t, 0).wrapping_mul(k + 1));
                }
                cur = pn.parent;
                k += 2;
            }
        }
        for (i, slot) in n.modifiers.iter().enumerate() {
            if let Some((src, _)) = slot.instance_of {
                key = key.wrapping_add(self.nodes.get(&src).map(|s| s.geom_version).unwrap_or(0).wrapping_mul(7 + i as u64));
            }
            // Linked XForm / Preserve / Skin bones / Skin Wrap bases follow other nodes: their transform chains too.
            for p in slot.modifier.referenced_nodes() {
                if depth < 6 && p != id {
                    key = key.wrapping_add(self.eval_key_depth(p, depth + 1).wrapping_mul(11 + depth as u64));
                }
                let mut cur = Some(p);
                let mut depth = 0u64;
                while let Some(c) = cur {
                    let Some(cn) = self.nodes.get(&c) else { break };
                    key = key.wrapping_add(cn.version.wrapping_mul(13 + i as u64 + depth)).wrapping_add(cn.geom_version.wrapping_mul(17 + depth));
                    // A constrained bone moves with its targets.
                    for t in cn.constraints.nodes() {
                        key = key.wrapping_add(self.transform_key(t, 0).wrapping_mul(19 + depth));
                    }
                    cur = cn.parent;
                    depth += 1;
                }
            }
            // IK moves bones without versioning them: any change re-evaluates a skinned mesh.
            if !self.ik_chains.is_empty() && matches!(slot.modifier, crate::modifier::Modifier::Skin(_)) {
                key = key.wrapping_add(self.version_counter.get().wrapping_mul(29));
            }
        }
        key
    }

    /// Folds the versions a node's world matrix depends on: itself, its
    /// ancestors and (to a bounded depth) their constraint targets.
    fn transform_key(&self, id: NodeId, depth: u32) -> u64 {
        let mut key = 0u64;
        let mut cur = Some(id);
        let mut k = 3u64;
        while let Some(c) = cur {
            let Some(cn) = self.nodes.get(&c) else { break };
            key = key.wrapping_add(cn.version.wrapping_mul(k)).wrapping_add(cn.geom_version.wrapping_mul(k + 1));
            if depth < 4 {
                for t in cn.constraints.nodes() {
                    if t != id {
                        key = key.wrapping_add(self.transform_key(t, depth + 1).wrapping_mul(k + 2));
                    }
                }
            }
            cur = cn.parent;
            k += 4;
        }
        key
    }

    /// The modifier a slot resolves to (its own, or the instanced source's).
    pub fn resolved_modifier(&self, id: NodeId, index: usize) -> Option<crate::modifier::Modifier> {
        let slot = self.nodes.get(&id)?.modifiers.get(index)?;
        if let Some((src, si)) = slot.instance_of {
            if let Some(s) = self.nodes.get(&src).and_then(|n| n.modifiers.get(si)) {
                // Parameters are shared; the gizmo / lattice belongs to this node (a per-node modifier context).
                let mut m = s.modifier.clone();
                if let (Some(g), Some(dst)) = (slot.modifier.gizmo(), m.gizmo_mut()) {
                    *dst = g.clone();
                }
                if let (crate::modifier::Modifier::FFD { points, rest, lo, hi, .. }, crate::modifier::Modifier::FFD { points: dp, rest: dr, lo: dlo, hi: dhi, .. }) = (&slot.modifier, &mut m) {
                    if points.len() == dp.len() {
                        *dp = points.clone();
                        *dr = rest.clone();
                        *dlo = *lo;
                        *dhi = *hi;
                    }
                }
                return Some(m);
            }
        }
        Some(slot.modifier.clone())
    }

    /// Evaluations run so far (tests watch the cache with it).
    pub fn eval_count(&self) -> usize {
        self.eval_count.get()
    }

    /// Mutable node access for transform-only edits: bumps the transform
    /// version but not the geometry version, so cached stack results stay.
    pub fn transform_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        let v = self.next_version();
        let n = self.nodes.get_mut(&id)?;
        n.version = v;
        Some(n)
    }

    /// Evaluates a node's stack up to and including entry `upto` (None = all),
    /// cached until the node (or an instanced source) changes.
    pub fn evaluated(&self, id: NodeId, upto: Option<usize>) -> std::sync::Arc<crate::modifier::Evaluated> {
        use crate::modifier::{EvalCtx, Evaluated, StackValue};
        self.sync_time();
        let key = self.eval_key(id);
        let now = self.time.get();
        if let Some((k, at, e)) = self.eval_cache.borrow().get(&(id, upto)) {
            if *k == key && at.is_none_or(|t| t == now) {
                if at.is_some() {
                    // Tell an enclosing build that it depends on time too.
                    crate::anim::note_animated_read();
                }
                return e.clone();
            }
        }
        let reads = crate::anim::animated_reads();
        self.eval_count.set(self.eval_count.get() + 1);
        let node = self.nodes.get(&id);
        let object_world = self.object_matrix(id);
        let lookup = |other: NodeId| -> Option<(glam::Mat4, loom_mesh::Mesh)> {
            let n = self.nodes.get(&other)?;
            Some((self.world_matrix(other), n.object.mesh().into_owned()))
        };
        let shape_lookup = |other: NodeId| -> Option<loom_shape::Shape> { self.nodes.get(&other)?.object.shape() };
        let world_lookup = |other: NodeId| -> Option<glam::Mat4> { self.nodes.contains_key(&other).then(|| self.world_matrix(other)) };
        let eval_lookup = |other: NodeId| -> Option<(glam::Mat4, loom_mesh::Mesh)> {
            if !self.nodes.contains_key(&other) || self.eval_in_progress.borrow().contains(&other) {
                return None;
            }
            let e = self.evaluated(other, None);
            Some((self.object_matrix(other), e.value.mesh.clone()))
        };
        let eval_shape_lookup = |other: NodeId| -> Option<loom_shape::Shape> {
            if !self.nodes.contains_key(&other) || self.eval_in_progress.borrow().contains(&other) {
                return None;
            }
            self.evaluated(other, None).value.shape.clone()
        };
        self.eval_in_progress.borrow_mut().insert(id);
        let camera_fov = |other: NodeId| -> Option<f32> {
            match &self.nodes.get(&other)?.object {
                crate::node::ObjectKind::Camera { fov, .. } => Some(fov.get()),
                _ => None,
            }
        };
        let self_world_at = |t: f32| -> glam::Mat4 { self.at_time(t, || self.object_matrix(id)) };
        let ctx = EvalCtx { object_world, object_inverse: object_world.inverse(), lookup: Some(&lookup), shape_lookup: Some(&shape_lookup), eval_shape_lookup: Some(&eval_shape_lookup), world_lookup: Some(&world_lookup), eval_lookup: Some(&eval_lookup), self_world_at: Some(&self_world_at), camera_fov: Some(&camera_fov) };
        let mut value = StackValue { mesh: node.map(|n| n.object.display_output()).unwrap_or_default(), selection: None, shape: node.and_then(|n| n.object.shape()), shape_selection: None, warnings: Vec::new() };
        let mut warnings = Vec::new();
        if let Some(n) = node {
            // An Editable Poly at a sub-object level passes its selection up.
            if let crate::node::ObjectKind::EditablePoly(p) = &n.object {
                if let Some(sel) = &p.passes_up {
                    value.selection = Some(sel.clone());
                }
            }
            let last = upto.map(|u| u + 1).unwrap_or(n.modifiers.len()).min(n.modifiers.len());
            for i in 0..last {
                let slot = &n.modifiers[i];
                if !slot.enabled || !slot.enabled_in_viewport {
                    continue;
                }
                let modifier = self.resolved_modifier(id, i).unwrap_or_else(|| slot.modifier.clone());
                if let Some(stored) = modifier.stored_selection() {
                    if !stored.ids.is_empty() && (stored.vertices != value.mesh.vertex_count() || stored.faces != value.mesh.face_count()) {
                        warnings.push(format!("Topology changed below {}: its sub-object selection was discarded", slot.name));
                        // Evaluate with an empty selection.
                        let mut m = modifier.clone();
                        if let Some(s) = m.stored_selection_mut() {
                            s.ids.clear();
                        }
                        match m.apply(value.clone(), &ctx) {
                            Ok(v) => value = v,
                            Err(e) => warnings.push(format!("{}: {e}", slot.name)),
                        }
                        continue;
                    }
                }
                let keeps_shape = modifier.passes_shape();
                let counts_before = (value.mesh.vertex_count(), value.mesh.face_count());
                match modifier.apply(value.clone(), &ctx) {
                    Ok(v) => value = v,
                    Err(e) => warnings.push(format!("{}: {e}", slot.name)),
                }
                warnings.append(&mut value.warnings);
                // A selection from below cannot survive a topology change (its ids no longer mean anything).
                if value.selection.is_some() && (value.mesh.vertex_count(), value.mesh.face_count()) != counts_before {
                    value.selection = None;
                }
                // Only Edit Spline passes a shape on; anything else leaves a mesh.
                if !keeps_shape {
                    value.shape = None;
                    value.shape_selection = None;
                }
            }
        }
        let e = std::sync::Arc::new(Evaluated { value, warnings });
        let at = (crate::anim::animated_reads() != reads).then_some(now);
        self.eval_cache.borrow_mut().insert((id, upto), (key, at, e.clone()));
        self.eval_in_progress.borrow_mut().remove(&id);
        e
    }

    /// The mesh the viewports show for a node: the whole stack's output.
    pub fn eval_mesh(&self, id: NodeId) -> std::sync::Arc<crate::modifier::Evaluated> {
        self.evaluated(id, None)
    }

    pub fn layer(&self, id: u32) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn layer_mut(&mut self, id: u32) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// A fresh layer id (ids are never reused).
    /// The colour a node shows in the viewports: its layer's when its colour is By Layer.
    pub fn display_color(&self, node: &Node) -> [u8; 3] {
        match self.layer(node.layer) {
            Some(l) if node.display.color_by_layer => l.color,
            _ => node.wire_color,
        }
    }

    /// Whether renders include the node (its layer's Render flag).
    pub fn is_renderable(&self, node: &Node) -> bool {
        self.layer(node.layer).is_none_or(|l| l.renderable)
    }

    pub fn next_layer_id(&self) -> u32 {
        self.layers.iter().map(|l| l.id + 1).max().unwrap_or(1)
    }

    /// The closed group a node belongs to, outermost first: selecting any
    /// member of a closed group selects that group's head.
    pub fn closed_group_of(&self, id: NodeId) -> Option<NodeId> {
        let mut out = None;
        let mut cur = self.node(id).and_then(|n| n.parent);
        while let Some(p) = cur {
            let node = self.node(p)?;
            if let crate::node::ObjectKind::Group { open } = node.object {
                if !open {
                    out = Some(p);
                }
            }
            cur = node.parent;
        }
        out
    }

    /// Every descendant of a node, depth first.
    /// Parent, grandparent, … up to the root.
    pub fn ancestors(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = self.nodes.get(&id).and_then(|n| n.parent);
        while let Some(p) = cur {
            out.push(p);
            cur = self.nodes.get(&p).and_then(|n| n.parent);
        }
        out
    }

    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = self.children(id);
        while let Some(c) = stack.pop() {
            out.push(c);
            stack.extend(self.children(c));
        }
        out
    }

    /// Adds the node, assigning a fresh id when `node.id == NodeId(0)`.
    /// A node carrying an explicit id (undo re-adding it) keeps that id.
    pub fn add_node(&mut self, mut node: Node) -> NodeId {
        if node.id == NodeId(0) {
            node.id = NodeId(self.next_id);
            // New objects land on the current layer (undo re-adds keep theirs).
            if node.layer == 0 {
                node.layer = self.current_layer;
            }
        }
        self.next_id = self.next_id.max(node.id.0 + 1);
        if node.parent.is_some_and(|p| !self.nodes.contains_key(&p)) {
            node.parent = None;
        }
        let id = node.id;
        node.version = self.next_version();
        self.nodes.insert(id, node);
        id
    }

    /// Removes a node. Its children are reparented to its parent and keep
    /// their world transform.
    pub fn remove_node(&mut self, id: NodeId) -> Option<Node> {
        let grandparent = self.nodes.get(&id)?.parent;
        for child in self.children(id) {
            self.set_parent(child, grandparent);
        }
        self.nodes.remove(&id)
    }

    /// Id the next `add_node` will assign to an id-less node.
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    pub(crate) fn restore_next_id(&mut self, next_id: u64) {
        self.next_id = next_id;
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        let v = self.next_version();
        let n = self.nodes.get_mut(&id)?;
        n.version = v;
        n.geom_version = v;
        Some(n)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        self.nodes
            .values()
            .filter(|n| n.parent == Some(id))
            .map(|n| n.id)
            .collect()
    }

    /// Node (pivot) world matrix: the inherit-filtered parent frame times
    /// the local transform (IK-solved for a bone in an enabled chain).
    pub fn world_matrix(&self, id: NodeId) -> Mat4 {
        if !self.nodes.contains_key(&id) {
            return Mat4::IDENTITY;
        }
        self.sync_time();
        self.frame_of(id) * self.local_matrix(id)
    }

    /// The local matrix as the user set it, IK ignored (FK).
    pub fn fk_local_matrix(&self, id: NodeId) -> Mat4 {
        self.nodes.get(&id).map(|n| n.transform.matrix()).unwrap_or(Mat4::IDENTITY)
    }

    /// World matrix with every IK chain ignored.
    pub fn fk_world_matrix(&self, id: NodeId) -> Mat4 {
        let Some(node) = self.nodes.get(&id) else { return Mat4::IDENTITY };
        self.sync_time();
        let frame = match node.parent {
            Some(p) => Self::frame_from(self.fk_world_matrix(p), node.inherit),
            None => Mat4::IDENTITY,
        };
        frame * node.transform.matrix()
    }

    /// The local matrix: the IK solution for a bone of an enabled chain, else FK.
    pub fn local_matrix(&self, id: NodeId) -> Mat4 {
        if !self.ik_chains.is_empty() && !self.ik_solving.get() {
            self.ensure_ik();
        }
        let cache = self.ik_cache.borrow();
        if let Some(m) = cache.2.get(&id) {
            if cache.1.is_some() {
                crate::anim::note_animated_read();
            }
            return *m;
        }
        drop(cache);
        self.constrained_local(id)
    }

    /// The frame a node's local transform sits in: the Link Constraint's
    /// active target frame, else the parent frame.
    pub fn frame_of(&self, id: NodeId) -> Mat4 {
        self.link_frame(id).unwrap_or_else(|| self.parent_frame(id))
    }

    fn link_frame(&self, id: NodeId) -> Option<Mat4> {
        let l = self.nodes.get(&id)?.constraints.link.as_ref()?;
        let t = self.time.get();
        let i = l.links.iter().rposition(|(f, _)| *f <= t + 1e-4)?;
        // Its own guard: a node with a Link and a Position / Rotation constraint is in
        // `constraint_busy` while its link frame is asked for.
        if !self.link_busy.borrow_mut().insert(id) {
            return None;
        }
        let m = self.link_frame_at(id, l, i, t);
        self.link_busy.borrow_mut().remove(&id);
        Some(m)
    }

    /// F_i(t) = T_i(t) · O_i, with O_i stored when the link was added; without a
    /// stored offset O_i = T_i(f_i)⁻¹ · F_(i-1)(f_i), continuous at every switch.
    fn link_frame_at(&self, id: NodeId, l: &crate::constraint::LinkConstraint, i: usize, t: f32) -> Mat4 {
        let (fi, target) = l.links[i];
        let tw = |time: f32| target.filter(|n| self.nodes.contains_key(n)).map(|n| self.at_time(time, || self.world_matrix(n))).unwrap_or(Mat4::IDENTITY);
        if l.offsets.len() == l.links.len() {
            return tw(t) * l.offsets[i];
        }
        let prev = self.at_time(fi, || if i == 0 { self.parent_frame(id) } else { self.link_frame_at(id, l, i - 1, fi) });
        tw(t) * tw(fi).inverse() * prev
    }

    /// The frame a new link to `target` at frame `at` must hold so the node
    /// does not jump there: the target's world at `at`, inverted, times the
    /// node's frame at `at` before the link.
    pub fn link_offset(&self, id: NodeId, target: Option<NodeId>, at: f32) -> Mat4 {
        self.at_time(at, || {
            let t = target.filter(|n| self.nodes.contains_key(n)).map(|n| self.world_matrix(n)).unwrap_or(Mat4::IDENTITY);
            t.inverse() * self.frame_of(id)
        })
    }

    /// The local matrix with Position / Rotation constraints applied.
    fn constrained_local(&self, id: NodeId) -> Mat4 {
        use crate::constraint::{aim, blend_rotations, path_point, weights, PositionConstraint, RotationConstraint};
        let Some(n) = self.nodes.get(&id) else { return Mat4::IDENTITY };
        let base = n.transform.matrix();
        let c = &n.constraints;
        if c.position.is_none() && c.rotation.is_none() {
            return base;
        }
        if !self.constraint_busy.borrow_mut().insert(id) {
            return base;
        }
        let frame = self.frame_of(id);
        let (_, frot, _) = frame.to_scale_rotation_translation();
        let (s, r, p) = base.to_scale_rotation_translation();
        let mut wpos = frame.transform_point3(p);
        let mut wrot = frot * r;
        let pos_of = |n: NodeId| self.world_matrix(n).w_axis.truncate();
        match &c.position {
            Some(PositionConstraint::Path(pc)) => {
                let w = weights(&pc.targets);
                let mut u = pc.percent.get() / 100.0;
                if pc.loop_path {
                    u = u.rem_euclid(1.0);
                }
                let (mut acc, mut tan, mut any) = (Vec3::ZERO, Vec3::ZERO, false);
                let mut normal = Vec3::ZERO;
                for (t, w) in pc.targets.iter().zip(&w) {
                    let Some(shape) = self.nodes.get(&t.node).and_then(|n| n.object.shape()) else { continue };
                    let om = self.object_matrix(t.node);
                    let Some((pt, tg)) = path_point(&shape, u, pc.constant_velocity) else { continue };
                    acc += om.transform_point3(pt) * *w;
                    tan += om.transform_vector3(tg) * *w;
                    // Curvature by the change of the tangent around u (Smoothness widens the window).
                    let h = 0.01 + pc.smoothness.clamp(0.0, 1.0) * 0.05;
                    if let (Some((_, a)), Some((_, b))) = (path_point(&shape, (u - h).max(0.0), pc.constant_velocity), path_point(&shape, (u + h).min(1.0), pc.constant_velocity)) {
                        normal += om.transform_vector3(b - a) * *w;
                    }
                    any = true;
                }
                if any {
                    wpos = acc + if pc.relative { pc.offset } else { Vec3::ZERO };
                    if pc.follow && tan.length_squared() > 1e-12 {
                        let up = if pc.allow_upside_down && normal.length_squared() > 1e-10 { -normal.normalize() } else { Vec3::Z };
                        let mut q = aim(tan, pc.axis, pc.flip, up);
                        if pc.bank {
                            // Roll into the turn: Bank Amount × the signed turn of the tangent.
                            let turn = Vec3::Z.cross(tan.normalize()).dot(normal);
                            let roll = (-pc.bank_amount * turn * std::f32::consts::PI).clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
                            q = Quat::from_axis_angle(tan.normalize(), roll) * q;
                        }
                        wrot = q;
                    }
                }
            }
            Some(PositionConstraint::Position { targets, keep_offset, offset }) => {
                let live: Vec<&crate::constraint::Target> = targets.iter().filter(|t| self.nodes.contains_key(&t.node)).collect();
                if !live.is_empty() {
                    let owned: Vec<crate::constraint::Target> = live.iter().map(|t| (*t).clone()).collect();
                    let w = weights(&owned);
                    wpos = owned.iter().zip(&w).map(|(t, w)| pos_of(t.node) * *w).sum::<Vec3>() + if *keep_offset { *offset } else { Vec3::ZERO };
                }
            }
            Some(PositionConstraint::Attachment { target, face, a, b, align }) => {
                if self.nodes.contains_key(target) && !self.eval_in_progress.borrow().contains(target) {
                    let e = self.evaluated(*target, None);
                    let om = self.object_matrix(*target);
                    if let Some(vs) = e.value.mesh.face_vertex_ids(loom_mesh::FaceId(*face)) {
                        let ps: Vec<Vec3> = vs.iter().take(3).filter_map(|v| e.value.mesh.vertex_position(*v)).map(|p| om.transform_point3(p)).collect();
                        if ps.len() == 3 {
                            wpos = ps[0] + (ps[1] - ps[0]) * *a + (ps[2] - ps[0]) * *b;
                            if *align {
                                let nrm = (ps[1] - ps[0]).cross(ps[2] - ps[0]).normalize_or_zero();
                                let x = (ps[1] - ps[0]).normalize_or_zero();
                                if nrm != Vec3::ZERO && x != Vec3::ZERO {
                                    wrot = Quat::from_mat3(&glam::Mat3::from_cols(x, nrm.cross(x), nrm)).normalize();
                                }
                            }
                        }
                    }
                }
            }
            None => {}
        }
        match &c.rotation {
            Some(RotationConstraint::LookAt { targets, axis, flip, upnode, keep_offset, offset }) => {
                let owned: Vec<crate::constraint::Target> = targets.iter().filter(|t| self.nodes.contains_key(&t.node)).cloned().collect();
                if !owned.is_empty() {
                    let w = weights(&owned);
                    let at: Vec3 = owned.iter().zip(&w).map(|(t, w)| pos_of(t.node) * *w).sum();
                    let up = upnode.filter(|u| self.nodes.contains_key(u)).map(|u| self.world_matrix(u).z_axis.truncate()).unwrap_or(Vec3::Z);
                    wrot = aim(at - wpos, *axis, *flip, up) * if *keep_offset { *offset } else { Quat::IDENTITY };
                }
            }
            Some(RotationConstraint::Orientation { targets, keep_offset, offset }) => {
                let owned: Vec<crate::constraint::Target> = targets.iter().filter(|t| self.nodes.contains_key(&t.node)).cloned().collect();
                if !owned.is_empty() {
                    let w = weights(&owned);
                    let rots: Vec<(Quat, f32)> = owned.iter().zip(&w).map(|(t, w)| (self.world_matrix(t.node).to_scale_rotation_translation().1, *w)).collect();
                    wrot = blend_rotations(&rots) * if *keep_offset { *offset } else { Quat::IDENTITY };
                }
            }
            None => {}
        }
        self.constraint_busy.borrow_mut().remove(&id);
        let inv = frame.inverse();
        Mat4::from_scale_rotation_translation(s, (frot.inverse() * wrot).normalize(), inv.transform_point3(wpos))
    }

    /// The solved local rotation of a bone, when a chain drives it.
    pub fn ik_rotation(&self, id: NodeId) -> Option<Mat4> {
        if !self.ik_chains.is_empty() && !self.ik_solving.get() {
            self.ensure_ik();
        }
        self.ik_cache.borrow().2.get(&id).copied()
    }

    /// The bones of a chain from `start` down to `end` (empty when `end` does not descend from `start`).
    pub fn ik_chain_bones(&self, chain: &crate::ik::IkChain) -> Vec<NodeId> {
        let mut path = vec![chain.end];
        let mut cur = chain.end;
        while cur != chain.start {
            let Some(p) = self.nodes.get(&cur).and_then(|n| n.parent) else { return Vec::new() };
            path.push(p);
            cur = p;
        }
        path.reverse();
        path
    }

    /// The chain a bone or goal belongs to.
    pub fn ik_chain_of(&self, id: NodeId) -> Option<&crate::ik::IkChain> {
        self.ik_chains.iter().find(|c| c.goal == id || self.ik_chain_bones(c).contains(&id))
    }

    fn ensure_ik(&self) {
        self.sync_time();
        let v = self.version_counter.get();
        let now = self.time.get();
        {
            let c = self.ik_cache.borrow();
            if c.0 == v && c.1.is_none_or(|t| t == now) {
                return;
            }
        }
        let reads = crate::anim::animated_reads();
        self.ik_solving.set(true);
        self.ik_cache.borrow_mut().2.clear();
        // Parents first: a chain hanging under another chain solves against its solved pose.
        let mut order: Vec<&crate::ik::IkChain> = self.ik_chains.iter().filter(|c| c.enabled).collect();
        order.sort_by_key(|c| self.depth(c.start));
        for chain in order {
            // The end bone marks the end joint (the goal sits at its base): it is not solved.
            let ids = self.ik_chain_bones(chain);
            if ids.len() < 2 || !self.nodes.contains_key(&chain.goal) {
                continue;
            }
            let bones: Vec<crate::ik::IkBone> = ids[..ids.len() - 1]
                .iter()
                .enumerate()
                .map(|(k, id)| {
                    let n = &self.nodes[id];
                    let tip = self.nodes[&ids[k + 1]].transform.position.get();
                    crate::ik::IkBone { id: *id, parent_frame: self.parent_frame(*id), position: n.transform.position.get(), rotation: n.transform.rotation_quat(), tip }
                })
                .collect();
            let goal = self.world_matrix(chain.goal).transform_point3(Vec3::ZERO);
            let spline = chain.spline.and_then(|s| self.nodes.get(&s).and_then(|n| n.object.shape().map(|sh| sh.transformed(self.world_matrix(s)))));
            let solved = crate::ik::solve(chain, &bones, goal, spline.as_ref());
            // Spline IK: the root bone's base sits on the shape's first knot.
            let root_pos = spline.as_ref().and_then(crate::ik::spline_start).map(|p| bones[0].parent_frame.inverse().transform_point3(p));
            let mut cache = self.ik_cache.borrow_mut();
            for (i, (id, q)) in solved.into_iter().enumerate() {
                let n = &self.nodes[&id];
                let pos = if i == 0 { root_pos.unwrap_or_else(|| n.transform.position.get()) } else { n.transform.position.get() };
                cache.2.insert(id, Mat4::from_scale_rotation_translation(n.transform.scale.get(), q, pos));
            }
        }
        let at = (crate::anim::animated_reads() != reads).then_some(now);
        let mut c = self.ik_cache.borrow_mut();
        c.0 = v;
        c.1 = at;
        drop(c);
        self.ik_solving.set(false);
    }

    /// The frame a node's local transform is relative to: its parent's world
    /// matrix with the components it does not inherit (Link Info > Inherit)
    /// dropped. Identity for root nodes.
    pub fn parent_frame(&self, id: NodeId) -> Mat4 {
        let Some(node) = self.nodes.get(&id) else {
            return Mat4::IDENTITY;
        };
        match node.parent {
            Some(p) => Self::frame_from(self.world_matrix(p), node.inherit),
            None => Mat4::IDENTITY,
        }
    }

    /// Applies inherit flags to a would-be parent world matrix.
    pub fn frame_from(parent_world: Mat4, inherit: crate::node::Axes3) -> Mat4 {
        if inherit == crate::node::Axes3::ALL {
            return parent_world;
        }
        let (scale, rot, pos) = parent_world.to_scale_rotation_translation();
        let (ex, ey, ez) = rot.to_euler(glam::EulerRot::XYZ);
        let pick = |keep: [bool; 3], v: [f32; 3], def: f32| {
            glam::Vec3::new(
                if keep[0] { v[0] } else { def },
                if keep[1] { v[1] } else { def },
                if keep[2] { v[2] } else { def },
            )
        };
        let pos = pick(inherit.move_axes, pos.to_array(), 0.0);
        let scale = pick(inherit.scale_axes, scale.to_array(), 1.0);
        let e = pick(inherit.rotate_axes, [ex, ey, ez], 0.0);
        let rot = glam::Quat::from_euler(glam::EulerRot::XYZ, e.x, e.y, e.z);
        Mat4::from_scale_rotation_translation(scale, rot, pos)
    }

    /// World matrix of the node's mesh (pivot world × object offset).
    pub fn object_matrix(&self, id: NodeId) -> Mat4 {
        let Some(node) = self.nodes.get(&id) else {
            return Mat4::IDENTITY;
        };
        self.world_matrix(id) * node.object_offset.matrix()
    }

    /// Reparents `id`, adjusting its local transform so the world
    /// transform is unchanged. Refuses to create a cycle.
    pub fn set_parent(&mut self, id: NodeId, parent: Option<NodeId>) {
        if !self.nodes.contains_key(&id) {
            return;
        }
        if let Some(p) = parent {
            if p == id || !self.nodes.contains_key(&p) || self.is_descendant(p, id) {
                return;
            }
        }
        let world = self.world_matrix(id);
        let inherit = self.nodes.get(&id).map(|n| n.inherit).unwrap_or(crate::node::Axes3::ALL);
        let frame = parent.map_or(Mat4::IDENTITY, |p| Self::frame_from(self.world_matrix(p), inherit));
        let local = frame.inverse() * world;
        let node = self.nodes.get_mut(&id).expect("checked above");
        node.parent = parent;
        node.transform = Transform::from_matrix_near(local, &node.transform, glam::Vec3::ZERO);
    }

    /// Number of ancestors above `id` (0 for a root).
    pub fn depth(&self, id: NodeId) -> usize {
        let mut d = 0;
        let mut cur = self.node(id).and_then(|n| n.parent);
        while let Some(p) = cur {
            d += 1;
            cur = self.node(p).and_then(|n| n.parent);
            if d > 10_000 {
                break;
            }
        }
        d
    }

    pub fn is_descendant(&self, node: NodeId, ancestor: NodeId) -> bool {
        let mut cur = self.nodes.get(&node).and_then(|n| n.parent);
        while let Some(p) = cur {
            if p == ancestor {
                return true;
            }
            cur = self.nodes.get(&p).and_then(|n| n.parent);
        }
        false
    }

    /// Next free numbered name: "Box001", "Box002", ...
    pub fn next_name(&self, base: &str) -> String {
        let mut n = 1;
        loop {
            let candidate = format!("{base}{n:03}");
            if !self.nodes.values().any(|node| node.name == candidate) {
                return candidate;
            }
            n += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{Node, ObjectKind, Transform};
    use crate::param::Param;
    use glam::{Mat4, Quat, Vec3};

    fn box_node(name: &str, pos: Vec3) -> Node {
        let mut n = Node::new(name, ObjectKind::box_object(1.0, 1.0, 1.0));
        n.transform.position.set(pos);
        n
    }

    #[test]
    fn add_node_assigns_increasing_ids_never_reused() {
        let mut s = Scene::new();
        let a = s.add_node(box_node("A", Vec3::ZERO));
        let b = s.add_node(box_node("B", Vec3::ZERO));
        assert!(b.0 > a.0);
        s.remove_node(b);
        let c = s.add_node(box_node("C", Vec3::ZERO));
        assert!(c.0 > b.0, "ids must never be reused");
    }

    #[test]
    fn next_name_follows_numbering_convention() {
        let mut s = Scene::new();
        assert_eq!(s.next_name("Box"), "Box001");
        let name = s.next_name("Box");
        s.add_node(box_node(&name, Vec3::ZERO));
        assert_eq!(s.next_name("Box"), "Box002");
    }

    #[test]
    fn remove_parent_keeps_children_world_transform() {
        let mut s = Scene::new();
        let g = s.add_node(box_node("G", Vec3::new(1.0, 0.0, 0.0)));
        let p = s.add_node(box_node("P", Vec3::new(0.0, 2.0, 0.0)));
        let c = s.add_node(box_node("C", Vec3::new(0.0, 0.0, 3.0)));
        s.set_parent(p, Some(g));
        s.set_parent(c, Some(p));
        let before = s.world_matrix(c);
        s.remove_node(p);
        assert_eq!(s.node(c).unwrap().parent, Some(g));
        assert!(before.abs_diff_eq(s.world_matrix(c), 1e-5));
    }

    #[test]
    fn set_parent_keeps_world_transform() {
        let mut s = Scene::new();
        let p = s.add_node(box_node("P", Vec3::new(5.0, 0.0, 0.0)));
        let c = s.add_node(box_node("C", Vec3::new(0.0, 1.0, 0.0)));
        let before = s.world_matrix(c);
        s.set_parent(c, Some(p));
        assert!(before.abs_diff_eq(s.world_matrix(c), 1e-5));
        assert!(s
            .node(c)
            .unwrap()
            .transform
            .position
            .get()
            .abs_diff_eq(Vec3::new(-5.0, 1.0, 0.0), 1e-5));
    }

    #[test]
    fn parametric_box_mesh_matches_primitive() {
        let obj = ObjectKind::box_object(2.0, 3.0, 4.0);
        let (min, max) = obj.mesh().aabb();
        // Length runs along Y, Width along X.
        assert!(min.abs_diff_eq(Vec3::new(-1.5, -1.0, 0.0), 1e-5), "{min}");
        assert!(max.abs_diff_eq(Vec3::new(1.5, 1.0, 4.0), 1e-5), "{max}");
    }

    #[test]
    fn transform_matrix_roundtrip() {
        let mut t = Transform::default();
        t.position.set(Vec3::new(1.0, 2.0, 3.0));
        t.set_rotation_quat(Quat::from_rotation_z(0.5));
        t.scale.set(Vec3::new(2.0, 2.0, 2.0));
        let back = Transform::from_matrix(t.matrix());
        assert!(back.matrix().abs_diff_eq(t.matrix(), 1e-5));
        let _ = Mat4::IDENTITY;
        let _: Param<f32> = Param::new(1.0);
    }
}

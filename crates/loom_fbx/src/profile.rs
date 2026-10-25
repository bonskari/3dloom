//! Rest-pose import of a MetaHuman composite onto its existing Body skeleton.
use std::collections::BTreeMap;

use glam::{EulerRot, Mat4, Quat};

use crate::{FbxDocument, FbxKind};

fn facial(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("facial_")
}

fn worlds(doc: &FbxDocument) -> Result<Vec<Mat4>, String> {
    let mut world = Vec::with_capacity(doc.nodes.len());
    for (i, node) in doc.nodes.iter().enumerate() {
        if node.parent.is_some_and(|p| p >= i) {
            return Err(format!("{}: parent must precede child", node.name));
        }
        let r = node.rotation_deg * std::f32::consts::PI / 180.0;
        let local = Mat4::from_scale_rotation_translation(node.scale, Quat::from_euler(EulerRot::XYZ, r.x, r.y, r.z), node.translation);
        let matrix = node.parent.map_or(local, |p| world[p] * local);
        if !matrix.is_finite() || matrix.determinant().abs() < 1e-12 {
            return Err(format!("{}: noninvertible reference transform", node.name));
        }
        world.push(matrix);
    }
    Ok(world)
}

fn under(doc: &FbxDocument, mut node: usize, ancestor: usize) -> bool {
    loop {
        if node == ancestor { return true; }
        match doc.nodes[node].parent {
            Some(parent) => node = parent,
            None => return false,
        }
    }
}

/// Return a rest-only composite with one canonical Body bone graph.
///
/// The `Body` component must have one direct bone child named `root` (case
/// insensitive), and unique bone names below it. Other components' bones map
/// by name; FACIAL bones map through their nearest non-FACIAL bone ancestor.
/// Skin entries and weights remain separate, including entries that now share
/// a target. Their mesh-to-bone matrices compensate for the changed reference
/// frames, preserving the neutral weighted surface without re-skinning.
///
/// Mesh geometry, component groups, local mesh transforms and `mesh_bind` are
/// unchanged. The source animation take is deliberately omitted. Active Face
/// morphs, unknown bone semantics and
/// attachments whose parent-frame change would move them are rejected rather
/// than silently changing their appearance. Zero-weight Face morphs are removed.
/// The caller retains the original document/FBX; no source asset is modified.
pub fn body_only_profile(source: &FbxDocument) -> Result<FbxDocument, String> {
    let world = worlds(source)?;
    let roots: Vec<usize> = source.nodes.iter().enumerate().filter_map(|(i, n)| {
        (matches!(n.kind, FbxKind::Bone { .. }) && n.name.eq_ignore_ascii_case("root")
            && n.parent.is_some_and(|p| source.nodes[p].name == "Body" && !matches!(source.nodes[p].kind, FbxKind::Bone { .. }))).then_some(i)
    }).collect();
    if roots.len() != 1 {
        return Err("expected one Body component with a root bone".into());
    }
    let root = roots[0];
    let mut canonical = BTreeMap::new();
    let mut kept = vec![true; source.nodes.len()];
    for (i, n) in source.nodes.iter().enumerate() {
        if !matches!(n.kind, FbxKind::Bone { .. }) { continue; }
        kept[i] = under(source, i, root);
        if kept[i] && (facial(&n.name) || canonical.insert(n.name.to_ascii_lowercase(), i).is_some()) {
            return Err(format!("invalid or duplicate canonical Body bone: {}", n.name));
        }
    }
    let mut target: Vec<usize> = (0..source.nodes.len()).collect();
    for (i, n) in source.nodes.iter().enumerate() {
        if kept[i] { continue; }
        let mut ancestor = i;
        while facial(&source.nodes[ancestor].name) {
            ancestor = source.nodes[ancestor].parent.ok_or_else(|| format!("{}: FACIAL bone has no retained ancestor", n.name))?;
        }
        target[i] = *canonical.get(&source.nodes[ancestor].name.to_ascii_lowercase())
            .ok_or_else(|| format!("{}: no canonical Body bone for {}", n.name, source.nodes[ancestor].name))?;
    }
    let mut indices = vec![None; source.nodes.len()];
    let mut result = source.clone();
    result.nodes.clear();
    result.anim = None;
    for (i, n) in source.nodes.iter().enumerate() {
        if !kept[i] { continue; }
        indices[i] = Some(result.nodes.len());
        let mut node = n.clone();
        if let Some(parent) = n.parent {
            if !kept[parent] && !world[parent].abs_diff_eq(world[target[parent]], 1e-5) {
                return Err(format!("{}: attachment would change reference frame", n.name));
            }
        }
        let in_face = source.nodes.iter().enumerate().any(|(a, n)| n.name.eq_ignore_ascii_case("Face") && under(source, i, a));
        if in_face {
            if n.blend_shapes.iter().any(|m| !m.weight.is_finite() || m.weight != 0.0) {
                return Err(format!("{}: active Face morph cannot be discarded", n.name));
            }
            node.blend_shapes.clear();
        }
        if let Some(skin) = &mut node.skin {
            if let FbxKind::Mesh { mesh, .. } = &node.kind {
                if skin.weights.len() != mesh.vertex_count() {
                    return Err(format!("{}: skin vertex count mismatch", n.name));
                }
            }
            for weights in &skin.weights {
                if weights.iter().any(|(b, w)| *b as usize >= skin.bones.len() || !w.is_finite() || *w < 0.0)
                    || (!weights.is_empty() && (weights.iter().map(|(_, w)| *w).sum::<f32>() - 1.0).abs() > 1e-3) {
                    return Err(format!("{}: invalid skin weights", n.name));
                }
            }
            for (bone, geometry_to_bone) in &mut skin.bones {
                if *bone >= source.nodes.len() || !matches!(source.nodes[*bone].kind, FbxKind::Bone { .. }) || !geometry_to_bone.is_finite() {
                    return Err(format!("{}: invalid skin bone reference", n.name));
                }
                let mapped = target[*bone];
                if mapped != *bone {
                    *geometry_to_bone = world[mapped].inverse() * world[*bone] * *geometry_to_bone;
                }
                *bone = mapped;
            }
        }
        result.nodes.push(node);
    }
    let remap = |i: usize| -> Result<usize, String> {
        target.get(i).and_then(|t| indices[*t]).ok_or_else(|| format!("invalid document node reference: {i}"))
    };
    for node in &mut result.nodes {
        node.parent = node.parent.map(&remap).transpose()?;
        if let Some(skin) = &mut node.skin {
            for (bone, _) in &mut skin.bones { *bone = remap(*bone)?; }
        }
    }
    for (a, b) in &mut result.instances { *a = remap(*a)?; *b = remap(*b)?; }
    for (_, nodes) in &mut result.selection_sets {
        for node in nodes { *node = remap(*node)?; }
    }
    for layer in &mut result.layers {
        for node in &mut layer.nodes { *node = remap(*node)?; }
    }
    for (node, _, _) in &mut result.subdivision { *node = remap(*node)?; }
    // Reparenting metadata/attachments must still leave the document in its
    // promised parent-before-child order.
    worlds(&result)?;
    Ok(result)
}

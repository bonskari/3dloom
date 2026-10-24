//! Phase 9 subdivision and caches: HSDS (hierarchical subdivision of
//! chosen polygons) and Point Cache / Point Cache (WSM) playing back .pc2
//! files.

use crate::modifier::{EvalCtx, StackValue};
use crate::param::Param;
use glam::Vec3;
use loom_mesh::pc2::Pc2;
use loom_mesh::{FaceId, Mesh};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// One HSDS level: the polygons of the mesh at that level that split, and
/// that mesh's counts (a topology change below drops the levels).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HsdsLevel {
    pub faces: Vec<u32>,
    pub vertex_count: usize,
    pub face_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HsdsData {
    pub levels: Vec<HsdsLevel>,
    pub smooth_result: bool,
    /// The finest level's mesh, the cage picked at the sub-object levels
    /// (rebuilt by the commands; not saved).
    #[serde(skip)]
    pub cage: Mesh,
}

/// The mesh after every level, or the input and a warning when the input no
/// longer has the topology the levels were made on.
pub fn hsds_mesh(d: &HsdsData, input: &Mesh) -> Result<(Mesh, Option<String>), String> {
    let s = if d.smooth_result { 1.0 } else { 0.0 };
    let mut m = input.clone();
    for (i, level) in d.levels.iter().enumerate() {
        if m.vertex_count() != level.vertex_count || m.face_count() != level.face_count {
            return Ok((input.clone(), Some(format!("HSDS: the topology below changed at level {}; its {} subdivision level(s) were dropped", i + 1, d.levels.len()))));
        }
        let faces: Vec<FaceId> = level.faces.iter().copied().filter(|f| (*f as usize) < m.face_count()).map(FaceId).collect();
        m = loom_mesh::geom_ops::msmooth_faces(&m, Some(&faces), s, false, false)?;
    }
    Ok((m, None))
}

pub fn hsds(d: &HsdsData, mut v: StackValue) -> Result<StackValue, String> {
    let (m, warning) = hsds_mesh(d, &v.mesh)?;
    if let Some(w) = warning {
        v.warnings.push(w);
        return Ok(v);
    }
    v.mesh = m;
    v.selection = None;
    Ok(v)
}

/// Adaptive Subdivision: up to `max_level` new levels, each splitting the
/// polygons that have an edge whose faces meet at more than `max_angle`
/// degrees.
pub fn hsds_adaptive(d: &mut HsdsData, input: &Mesh, max_level: u32, max_angle: f32) -> Result<(), String> {
    let (mut m, warning) = hsds_mesh(d, input)?;
    if let Some(w) = warning {
        return Err(w);
    }
    let cos_limit = max_angle.to_radians().cos();
    for _ in 0..max_level {
        let mut sharp = std::collections::BTreeSet::new();
        for (i, he) in m.half_edges().iter().enumerate() {
            let (Some(f), Some(g)) = (he.face, m.half_edges()[he.twin.0 as usize].face) else { continue };
            if (i as u32) < he.twin.0 && m.face_normal(f).dot(m.face_normal(g)) < cos_limit {
                sharp.insert(f.0);
                sharp.insert(g.0);
            }
        }
        if sharp.is_empty() {
            break;
        }
        d.levels.push(HsdsLevel { faces: sharp.into_iter().collect(), vertex_count: m.vertex_count(), face_count: m.face_count() });
        let level = d.levels.last().unwrap();
        let faces: Vec<FaceId> = level.faces.iter().map(|f| FaceId(*f)).collect();
        m = loom_mesh::geom_ops::msmooth_faces(&m, Some(&faces), if d.smooth_result { 1.0 } else { 0.0 }, false, false)?;
    }
    d.cage = m;
    Ok(())
}

/// Point Cache / Point Cache (WSM): plays a .pc2 file back onto the mesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PointCacheData {
    pub path: String,
    pub strength: Param<f32>,
    /// Relative Offset: the recorded motion (from the first sample) is added
    /// to the input, scaled by Strength; off: the input moves toward the cache.
    pub relative: bool,
    /// Playback Type: 0 Original Range, 1 Custom Start (`start_frame`).
    pub playback: u32,
    pub start_frame: f32,
    /// The WSM: the file holds world positions.
    pub world: bool,
    #[serde(skip)]
    pub data: Pc2Cache,
}

/// Which version of a file a read came from: its path, size and modification time.
type FileStamp = (String, u64, Option<std::time::SystemTime>);

fn stamp(path: &str) -> Option<FileStamp> {
    let m = std::fs::metadata(path).ok()?;
    Some((path.to_string(), m.len(), m.modified().ok()))
}

/// The last successful read of a Point Cache file, keyed by the file's
/// stamp. A failed read is never kept: the next evaluation tries again, and
/// a file rewritten on disk is read again. Not part of the modifier's value
/// (equality ignores it; a copy starts with the same loaded data).
#[derive(Default)]
pub struct Pc2Cache(Mutex<Option<(FileStamp, Arc<Pc2>)>>);

impl Clone for Pc2Cache {
    fn clone(&self) -> Self {
        Self(Mutex::new(self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()))
    }
}

impl PartialEq for Pc2Cache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for Pc2Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pc2Cache")
    }
}

impl Default for PointCacheData {
    fn default() -> Self {
        Self { path: String::new(), strength: Param::new(1.0), relative: false, playback: 0, start_frame: 0.0, world: false, data: Pc2Cache::default() }
    }
}

impl PointCacheData {
    /// The file's data: from the cache while the file's size and time are
    /// unchanged, else read again (None when it cannot be read; nothing is
    /// remembered of the failure).
    pub fn cache(&self) -> Option<Arc<Pc2>> {
        if self.path.is_empty() {
            return None;
        }
        let key = stamp(&self.path)?;
        let mut slot = self.data.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((k, pc)) = slot.as_ref() {
            if *k == key {
                return Some(pc.clone());
            }
        }
        let pc = Arc::new(std::fs::File::open(&self.path).ok().and_then(|f| Pc2::read(std::io::BufReader::new(f)).ok())?);
        *slot = Some((key, pc.clone()));
        Some(pc)
    }

    /// Loads `path` now, so a bad file is refused at once.
    pub fn load(&mut self, path: &str) -> Result<(), String> {
        let key = stamp(path);
        let f = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let pc = Pc2::read(std::io::BufReader::new(f)).map_err(|e| format!("{path}: {e}"))?;
        self.path = path.to_string();
        self.data = Pc2Cache(Mutex::new(key.map(|k| (k, Arc::new(pc)))));
        Ok(())
    }
}

pub fn point_cache(d: &PointCacheData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    if d.path.is_empty() {
        return Ok(v);
    }
    // The file is looked at again on every frame: one that appears or
    // changes on disk plays without a Load.
    crate::anim::note_animated_read();
    let Some(pc) = d.cache() else {
        v.warnings.push(format!("Point Cache: cannot read {}", d.path));
        return Ok(v);
    };
    let n = v.mesh.vertex_count();
    if pc.points() != n {
        v.warnings.push(format!("Point Cache: the file has {} vertices, the mesh {n}; the mesh passes through", pc.points()));
        return Ok(v);
    }
    let now = crate::anim::time();
    let t = if d.playback == 1 { now - d.start_frame + pc.start } else { now };
    let Some(at) = pc.at(t) else { return Ok(v) };
    let to_local = |p: Vec3| if d.world { ctx.object_inverse.transform_point3(p) } else { p };
    let first = &pc.samples[0];
    let k = d.strength.get();
    for (i, vert) in v.mesh.vertices_mut().iter_mut().enumerate() {
        let cur = vert.position;
        vert.position = if d.relative { cur + (to_local(at[i]) - to_local(first[i])) * k } else { cur.lerp(to_local(at[i]), k) };
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &std::path::Path, points: usize) {
        let pc = Pc2 { start: 0.0, rate: 1.0, samples: vec![vec![Vec3::ZERO; points]; 2] };
        pc.write(std::io::BufWriter::new(std::fs::File::create(path).unwrap())).unwrap();
    }

    #[test]
    fn a_failed_read_is_not_kept() {
        let dir = std::env::temp_dir().join(format!("loom-pc-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("later.pc2");
        let _ = std::fs::remove_file(&path);
        let d = PointCacheData { path: path.to_string_lossy().into(), ..Default::default() };
        assert!(d.cache().is_none(), "no file yet");
        write(&path, 3);
        assert_eq!(d.cache().map(|p| p.points()), Some(3), "read once the file appears");
        // A rewritten file is read again; an unchanged one comes from the cache.
        write(&path, 5);
        assert_eq!(d.cache().map(|p| p.points()), Some(5));
        let a = d.cache().unwrap();
        assert!(Arc::ptr_eq(&a, &d.cache().unwrap()), "cached while the file is unchanged");
        // A copy of the modifier keeps the loaded file.
        assert!(Arc::ptr_eq(&a, &d.clone().cache().unwrap()));
        std::fs::remove_file(&path).unwrap();
        assert!(d.cache().is_none(), "the file is gone");
    }
}

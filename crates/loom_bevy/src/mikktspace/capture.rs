//! Test-only preparation guards; never read to produce renderer output.
use super::generated::STriInfo;
use std::cell::{Cell, RefCell};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub(super) indices: Vec<i32>,
    pub(super) flags: Vec<(i32, i32)>,
    pub(super) neighbors: Vec<[i32; 3]>,
    pub(super) active_count: usize,
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static LAST: RefCell<Option<Snapshot>> = const { RefCell::new(None) };
}

pub(super) fn enable() { ENABLED.with(|v| v.set(true)); LAST.with(|v| *v.borrow_mut() = None); }
pub(super) fn take() -> Snapshot { LAST.with(|v| v.borrow_mut().take().expect("preparation capture")) }
pub(super) fn record(indices: &[i32], triangles: &[STriInfo], active_count: usize) {
    if ENABLED.with(Cell::get) {
        LAST.with(|v| *v.borrow_mut() = Some(Snapshot { indices: indices.to_vec(), flags: triangles.iter().map(|tri| (tri.iOrgFaceNumber, tri.iFlag)).collect(), neighbors: triangles.iter().map(|tri| tri.FaceNeighbors).collect(), active_count }));
    }
}

/// Actual ordered inputs to one invocation of the CPU subgroup evaluator.
#[derive(Clone, Debug)]
pub(crate) struct GroupFace {
    pub normal: [f32; 3],
    pub prev: [f32; 3],
    pub position: [f32; 3],
    pub next: [f32; 3],
    pub os: [f32; 3],
    pub ot: [f32; 3],
    pub mag_s: f32,
    pub mag_t: f32,
    pub skip: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupFixture {
    pub faces: Vec<GroupFace>,
    pub expected_s: [f32; 3],
    pub expected_t: [f32; 3],
    pub expected_mag_s: f32,
    pub expected_mag_t: f32,
}

thread_local! {
    static GROUPS: RefCell<Option<Vec<GroupFixture>>> = const { RefCell::new(None) };
}

/// Run `generate_serial` on the same thread between enable and take. Workers
/// deliberately do not inherit this bounded, test-only recording state.
pub(crate) fn enable_group_capture() { GROUPS.with(|v| *v.borrow_mut() = Some(Vec::new())); }
pub(crate) fn take_group_captures() -> Vec<GroupFixture> { GROUPS.with(|v| v.borrow_mut().take().unwrap_or_default()) }
pub(super) fn wants_group() -> bool { GROUPS.with(|v| v.borrow().as_ref().is_some_and(|groups| groups.len() < 256)) }
pub(super) fn record_group(fixture: GroupFixture) {
    GROUPS.with(|v| {
        if let Some(groups) = v.borrow_mut().as_mut() {
            if groups.len() < 256 { groups.push(fixture); }
        }
    });
}


#[derive(Clone, Debug)]
pub(crate) struct PreparationTriangle {
    pub original_face: i32,
    pub flags: i32,
    /// Prepared-triangle indices; only active triangles have valid neighbors.
    pub neighbors: [i32; 3],
}

#[derive(Clone, Debug)]
pub(crate) struct PreparationFixture {
    pub indices: Vec<i32>,
    pub triangles: Vec<PreparationTriangle>,
    pub active_count: usize,
}

pub(super) fn finish_preparation() -> PreparationFixture {
    ENABLED.with(|v| v.set(false));
    let snapshot = take();
    PreparationFixture {
        indices: snapshot.indices,
        triangles: snapshot.flags.into_iter().zip(snapshot.neighbors).map(|((original_face, flags), neighbors)| PreparationTriangle { original_face, flags, neighbors }).collect(),
        active_count: snapshot.active_count,
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparationGroup {
    pub members: Vec<i32>,
    pub representative: i32,
    pub orientation: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupPreparationFixture {
    /// Flags here are AFTER Build4RuleGroups, including Any orientation changes.
    pub preparation: PreparationFixture,
    pub assigned_groups: Vec<[i32; 3]>,
    pub groups: Vec<PreparationGroup>,
    /// Current InitTriInfo derivatives in prepared triangle order; no output tangents.
    pub derivatives: Vec<[[f32; 4]; 2]>,
}

thread_local! {
    static CAPTURE_PREPARED_GROUPS: Cell<bool> = const { Cell::new(false) };
    static PREPARED_GROUPS: RefCell<Option<GroupPreparationFixture>> = const { RefCell::new(None) };
}
pub(super) fn enable_prepared_groups() {
    CAPTURE_PREPARED_GROUPS.with(|v|v.set(true));
    PREPARED_GROUPS.with(|v|*v.borrow_mut()=None);
}
pub(super) fn take_prepared_groups() -> GroupPreparationFixture {
    CAPTURE_PREPARED_GROUPS.with(|v|v.set(false));
    PREPARED_GROUPS.with(|v|v.borrow_mut().take().expect("actual prepared group capture"))
}
pub(super) fn record_prepared_groups(indices: &[i32], triangles: &[STriInfo], groups: &[super::generated::SGroup], active_count: usize) {
    if !CAPTURE_PREPARED_GROUPS.with(Cell::get) {return;}
    let assigned_groups=triangles.iter().map(|t|t.AssignedGroup.map(|g| {
        if g.is_null() {-1} else {
            let index=unsafe {g.cast_const().offset_from(groups.as_ptr())};
            assert!(index>=0&&(index as usize)<groups.len());
            index as i32
        }
    })).collect();
    let groups=groups.iter().map(|g|PreparationGroup {
        members:unsafe {std::slice::from_raw_parts(g.pFaceIndices,g.iNrFaces as usize)}.to_vec(),
        representative:g.iVertexRepresentative,orientation:g.bOrientPreserving,
    }).collect();
    let fixture=GroupPreparationFixture {
        preparation:PreparationFixture {
            indices:indices.to_vec(),
            triangles:triangles.iter().map(|t|PreparationTriangle{original_face:t.iOrgFaceNumber,flags:t.iFlag,neighbors:t.FaceNeighbors}).collect(),
            active_count,
        },assigned_groups,groups,
        derivatives: triangles.iter().map(|t| [[t.vOs.x, t.vOs.y, t.vOs.z, t.fMagS], [t.vOt.x, t.vOt.y, t.vOt.z, t.fMagT]]).collect(),
    };
    PREPARED_GROUPS.with(|v|*v.borrow_mut()=Some(fixture));
}

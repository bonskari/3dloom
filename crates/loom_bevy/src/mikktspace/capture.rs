//! Test-only preparation guards; never read to produce renderer output.
use super::generated::STriInfo;
use std::cell::{Cell, RefCell};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub(super) indices: Vec<i32>,
    pub(super) flags: Vec<(i32, i32)>,
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static LAST: RefCell<Option<Snapshot>> = const { RefCell::new(None) };
}

pub(super) fn enable() { ENABLED.with(|v| v.set(true)); }
pub(super) fn take() -> Snapshot { LAST.with(|v| v.borrow_mut().take().expect("preparation capture")) }
pub(super) fn record(indices: &[i32], triangles: &[STriInfo]) {
    if ENABLED.with(Cell::get) {
        LAST.with(|v| *v.borrow_mut() = Some(Snapshot { indices: indices.to_vec(), flags: triangles.iter().map(|tri| (tri.iOrgFaceNumber, tri.iFlag)).collect() }));
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

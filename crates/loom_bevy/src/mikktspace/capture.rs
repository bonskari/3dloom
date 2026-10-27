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

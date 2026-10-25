//! Parameter blocks. The old ParamBlock keeps 0x0002 entries { 0x0003
//! index, 0x0100 float / 0x0101 int / 0x0102 three floats / 0x0104 bool };
//! the ParamBlock2 keeps 0x000E entries: u16 id, u16 type, ten bytes of
//! flags, a marker byte, then the value (for a table type, 0x0800 set, a
//! u32 count and per element a marker byte and the value). Types as seen in
//! the sample files: 0 float, 1 int, 2 colour (three floats), 3 point (three
//! floats), 4 bool, 5 angle (radians), 6 percent fraction, 7 world units.

use crate::chunk::Chunk;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PVal {
    F(f32),
    I(i32),
    V([f32; 3]),
}

impl PVal {
    pub fn f(self) -> Option<f32> {
        match self {
            PVal::F(v) => Some(v),
            PVal::I(v) => Some(v as f32),
            PVal::V(_) => None,
        }
    }
    pub fn v(self) -> Option<[f32; 3]> {
        match self {
            PVal::V(v) => Some(v),
            _ => None,
        }
    }
}

/// A ParamBlock2 table of ints or bools (type | 0x0800): the elements, or None
/// when the entry is something else.
pub fn int_table(pb: &Chunk, id: u16) -> Option<Vec<i32>> {
    let e = pb.kids().iter().find(|e| e.id == 0x000e && e.data.len() >= 18 && u16::from_le_bytes([e.data[0], e.data[1]]) == id)?;
    let ty = u16::from_le_bytes([e.data[2], e.data[3]]);
    if ty & 0x0800 == 0 || !matches!(ty & 0x7ff, 1 | 4) {
        return None;
    }
    let n = u32::from_le_bytes(e.data[14..18].try_into().unwrap()) as usize;
    let body = &e.data[18..];
    if body.len() != n.checked_mul(5)? {
        return None;
    }
    Some(body.chunks_exact(5).map(|c| i32::from_le_bytes([c[1], c[2], c[3], c[4]])).collect())
}

/// Values by parameter index (ParamBlock) or id (ParamBlock2). Animated
/// parameters (a controller instead of a value) are left out.
pub fn read(pb: &Chunk) -> BTreeMap<u32, PVal> {
    let mut out = BTreeMap::new();
    for p in pb.kids() {
        match p.id {
            0x0002 => {
                let Some(idx) = p.child(0x0003).and_then(|c| c.u32s().first().copied()) else { continue };
                let val = p.kids().iter().find_map(|c| match c.id {
                    0x0100 => c.f32s().first().map(|v| PVal::F(*v)),
                    0x0101 | 0x0104 => c.i32s().first().map(|v| PVal::I(*v)),
                    0x0102 => {
                        let v = c.f32s();
                        (v.len() >= 3).then(|| PVal::V([v[0], v[1], v[2]]))
                    }
                    _ => None,
                });
                if let Some(v) = val {
                    out.insert(idx, v);
                }
            }
            0x000e if p.data.len() >= 19 => {
                let id = u16::from_le_bytes([p.data[0], p.data[1]]) as u32;
                let ty = u16::from_le_bytes([p.data[2], p.data[3]]);
                let v = &p.data[15..];
                let f = |o: usize| f32::from_le_bytes(v[o..o + 4].try_into().unwrap());
                let val = match ty {
                    0 | 5 | 6 | 7 if v.len() == 4 => PVal::F(f(0)),
                    1 | 4 if v.len() == 4 => PVal::I(i32::from_le_bytes(v[..4].try_into().unwrap())),
                    2 | 3 if v.len() == 12 => PVal::V([f(0), f(4), f(8)]),
                    _ => continue,
                };
                out.insert(id, val);
            }
            _ => {}
        }
    }
    out
}

/// The ParamBlock2's sub-chunk 0x0003 > 0x1260 > 0x0002: an asset id (16 bytes).
pub fn asset_ids(pb: &Chunk) -> Vec<[u8; 16]> {
    let mut out = Vec::new();
    for c in pb.kids().iter().filter(|c| c.id == 0x0003) {
        for a in c.kids().iter().filter(|a| a.id == 0x1260) {
            if let Some(g) = a.child(0x0002).and_then(|g| <[u8; 16]>::try_from(g.data.as_slice()).ok()) {
                out.push(g);
            }
        }
    }
    out
}

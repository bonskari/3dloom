//! FBX 7.4 binary encoding: a tree of records, each with typed properties
//! and nested records, 32-bit offsets, arrays uncompressed (encoding 0).

#[derive(Debug, Clone)]
pub enum Prop {
    I16(i16),
    Bool(bool),
    I32(i32),
    F32(f32),
    F64(f64),
    I64(i64),
    Str(String),
    Raw(Vec<u8>),
    ArrF32(Vec<f32>),
    ArrF64(Vec<f64>),
    ArrI32(Vec<i32>),
    ArrI64(Vec<i64>),
    ArrBool(Vec<bool>),
}

#[derive(Debug, Clone, Default)]
pub struct Record {
    pub name: String,
    pub props: Vec<Prop>,
    pub children: Vec<Record>,
}

impl Record {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), props: Vec::new(), children: Vec::new() }
    }
    pub fn with(name: &str, props: Vec<Prop>) -> Self {
        Self { name: name.into(), props, children: Vec::new() }
    }
    pub fn child(mut self, r: Record) -> Self {
        self.children.push(r);
        self
    }
    pub fn leaf(name: &str, props: Vec<Prop>) -> Self {
        Self::with(name, props)
    }
}

pub const MAGIC: &[u8] = b"Kaydara FBX Binary  \0\x1a\0";
pub const VERSION: u32 = 7400;
/// The 13-byte sentinel that closes a record's nested list (7.4: 3 × u32 + u8).
const NULL_RECORD: [u8; 13] = [0; 13];

/// A whole file: magic, version, top-level records, closing sentinel, footer.
pub fn encode(top: &[Record]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 << 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    for r in top {
        write_record(&mut out, r);
    }
    out.extend_from_slice(&NULL_RECORD);
    footer(&mut out);
    out
}

fn write_record(out: &mut Vec<u8>, r: &Record) {
    let start = out.len();
    out.extend_from_slice(&[0u8; 12]); // end_offset, num_props, prop_list_len
    out.push(r.name.len() as u8);
    out.extend_from_slice(r.name.as_bytes());
    let props_start = out.len();
    for p in &r.props {
        write_prop(out, p);
    }
    let prop_list_len = (out.len() - props_start) as u32;
    if !r.children.is_empty() {
        for c in &r.children {
            write_record(out, c);
        }
        out.extend_from_slice(&NULL_RECORD);
    }
    let end = out.len() as u32;
    out[start..start + 4].copy_from_slice(&end.to_le_bytes());
    out[start + 4..start + 8].copy_from_slice(&(r.props.len() as u32).to_le_bytes());
    out[start + 8..start + 12].copy_from_slice(&prop_list_len.to_le_bytes());
}

fn write_array<T, F: Fn(&T, &mut Vec<u8>)>(out: &mut Vec<u8>, code: u8, items: &[T], put: F) {
    out.push(code);
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // encoding: raw
    let len_pos = out.len();
    out.extend_from_slice(&[0u8; 4]);
    let data_start = out.len();
    for it in items {
        put(it, out);
    }
    let n = (out.len() - data_start) as u32;
    out[len_pos..len_pos + 4].copy_from_slice(&n.to_le_bytes());
}

fn write_prop(out: &mut Vec<u8>, p: &Prop) {
    match p {
        Prop::I16(v) => {
            out.push(b'Y');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::Bool(v) => {
            out.push(b'C');
            out.push(*v as u8);
        }
        Prop::I32(v) => {
            out.push(b'I');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::F32(v) => {
            out.push(b'F');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::F64(v) => {
            out.push(b'D');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::I64(v) => {
            out.push(b'L');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::Str(s) => {
            out.push(b'S');
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
        Prop::Raw(b) => {
            out.push(b'R');
            out.extend_from_slice(&(b.len() as u32).to_le_bytes());
            out.extend_from_slice(b);
        }
        Prop::ArrF32(v) => write_array(out, b'f', v, |x, o| o.extend_from_slice(&x.to_le_bytes())),
        Prop::ArrF64(v) => write_array(out, b'd', v, |x, o| o.extend_from_slice(&x.to_le_bytes())),
        Prop::ArrI32(v) => write_array(out, b'i', v, |x, o| o.extend_from_slice(&x.to_le_bytes())),
        Prop::ArrI64(v) => write_array(out, b'l', v, |x, o| o.extend_from_slice(&x.to_le_bytes())),
        Prop::ArrBool(v) => write_array(out, b'b', v, |x, o| o.push(*x as u8)),
    }
}

/// The `FileId` / `CreationTime` ("1970-01-01 10:00:00:000") pair Blender
/// and Assimp write: the SDK ties its id check to the timestamp, and this
/// pair is the known-good one.
pub const FILE_ID: [u8; 16] = [0x28, 0xb3, 0x2a, 0xeb, 0xb6, 0x24, 0xcc, 0xc2, 0xbf, 0xc8, 0xb0, 0x2a, 0xa9, 0x2b, 0xfc, 0xf1];
const FOOTER_ID: [u8; 16] = [0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e];
const FOOTER_TAIL: [u8; 16] = [0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b];

/// Footer as Blender writes it: id, 1–16 zero bytes to a 16-byte
/// boundary, the version, 120 zero bytes, the tail.
fn footer(out: &mut Vec<u8>) {
    out.extend_from_slice(&FOOTER_ID);
    let mut pad = (16 - out.len() % 16) % 16;
    if pad == 0 {
        pad = 16;
    }
    out.extend(std::iter::repeat_n(0u8, pad));
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&[0u8; 120]);
    out.extend_from_slice(&FOOTER_TAIL);
}

/// `P: name, type, label, flags, values…` inside a `Properties70`.
pub fn p(name: &str, ty: &str, label: &str, flags: &str, values: Vec<Prop>) -> Record {
    let mut props = vec![Prop::Str(name.into()), Prop::Str(ty.into()), Prop::Str(label.into()), Prop::Str(flags.into())];
    props.extend(values);
    Record::with("P", props)
}

/// `"Name\0\x01Class"` as FBX object names are written.
pub fn object_name(name: &str, class: &str) -> Prop {
    Prop::Str(format!("{name}\0\u{1}{class}"))
}

/// A record read back from a binary file: only the properties the reader
/// needs (integers, strings, i32 arrays); everything else is `Other`.
#[derive(Debug, Clone)]
pub enum RawProp {
    Int(i64),
    Str(String),
    I32Arr(Vec<i32>),
    Other,
}

#[derive(Debug, Clone, Default)]
pub struct RawRecord {
    pub name: String,
    pub props: Vec<RawProp>,
    pub children: Vec<RawRecord>,
}

fn rd_u32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn rd_u64(b: &[u8], at: usize) -> Option<u64> {
    b.get(at..at + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap()))
}

/// Parses the top-level records of a binary FBX (7.0–7.7; 7.5+ uses
/// 64-bit record headers). Returns `None` for ASCII or truncated data.
pub fn parse(bytes: &[u8]) -> Option<Vec<RawRecord>> {
    if !bytes.starts_with(MAGIC) {
        return None;
    }
    let version = rd_u32(bytes, 23)?;
    let wide = version >= 7500;
    let mut at = 27;
    let mut out = Vec::new();
    while let Some((rec, next)) = parse_record(bytes, at, wide, false)? {
        out.push(rec);
        at = next;
    }
    Some(out)
}

/// One record at `at`; `Ok(None)` is the null sentinel.
#[allow(clippy::type_complexity)]
fn parse_record(b: &[u8], at: usize, wide: bool, keep_arrays: bool) -> Option<Option<(RawRecord, usize)>> {
    let (end, nprops, plen, hdr) = if wide {
        (rd_u64(b, at)? as usize, rd_u64(b, at + 8)? as usize, rd_u64(b, at + 16)? as usize, 24)
    } else {
        (rd_u32(b, at)? as usize, rd_u32(b, at + 4)? as usize, rd_u32(b, at + 8)? as usize, 12)
    };
    let name_len = *b.get(at + hdr)? as usize;
    if end == 0 {
        return Some(None);
    }
    if end <= at || end > b.len() {
        return None;
    }
    let name = String::from_utf8_lossy(b.get(at + hdr + 1..at + hdr + 1 + name_len)?).into_owned();
    let mut p = at + hdr + 1 + name_len;
    let props_end = p.checked_add(plen)?;
    if props_end > end || nprops > plen {
        return None; // every property takes at least two bytes
    }
    // Arrays are decoded only where the reader looks (Smoothing); the rest is skipped.
    let keep = keep_arrays || name == "LayerElementSmoothing";
    let mut props = Vec::with_capacity(nprops);
    for _ in 0..nprops {
        let (prop, next) = parse_prop(b, p, keep && name == "Smoothing")?;
        props.push(prop);
        p = next;
    }
    let mut children = Vec::new();
    let mut c = props_end;
    while c < end {
        match parse_record(b, c, wide, keep)? {
            Some((rec, next)) => {
                children.push(rec);
                c = next;
            }
            None => break,
        }
    }
    Some(Some((RawRecord { name, props, children }, end)))
}

fn parse_prop(b: &[u8], at: usize, decode_arrays: bool) -> Option<(RawProp, usize)> {
    let code = *b.get(at)?;
    let p = at + 1;
    Some(match code {
        b'Y' => (RawProp::Int(i16::from_le_bytes([*b.get(p)?, *b.get(p + 1)?]) as i64), p + 2),
        b'C' => (RawProp::Int(*b.get(p)? as i64), p + 1),
        b'I' => (RawProp::Int(rd_u32(b, p)? as i32 as i64), p + 4),
        b'F' => (RawProp::Other, p + 4),
        b'D' => (RawProp::Other, p + 8),
        b'L' => (RawProp::Int(rd_u64(b, p)? as i64), p + 8),
        b'S' | b'R' => {
            let n = rd_u32(b, p)? as usize;
            let s = b.get(p + 4..p + 4 + n)?;
            (if code == b'S' { RawProp::Str(String::from_utf8_lossy(s).into_owned()) } else { RawProp::Other }, p + 4 + n)
        }
        b'f' | b'd' | b'l' | b'i' | b'b' => {
            let n = rd_u32(b, p)? as usize;
            let encoding = rd_u32(b, p + 4)?;
            let len = rd_u32(b, p + 8)? as usize;
            let data = b.get(p + 12..p.checked_add(12 + len)?)?;
            let prop = if code == b'i' && decode_arrays {
                let raw: std::borrow::Cow<[u8]> = if encoding == 1 { miniz_oxide::inflate::decompress_to_vec_zlib(data).ok()?.into() } else { data.into() };
                if raw.len() == n * 4 {
                    RawProp::I32Arr(raw.as_chunks::<4>().0.iter().map(|c| i32::from_le_bytes(*c)).collect())
                } else {
                    RawProp::Other
                }
            } else {
                RawProp::Other
            };
            (prop, p + 12 + len)
        }
        _ => return None,
    })
}

/// `Smoothing` arrays stored ByPolygon per geometry id, straight from the
/// file: ufbx reduces them to one bool per face, but exporters write the
/// smoothing-group bits there.
pub fn smoothing_by_geometry(bytes: &[u8]) -> std::collections::HashMap<i64, Vec<u32>> {
    let mut out = std::collections::HashMap::new();
    let Some(top) = parse(bytes) else { return out };
    let Some(objects) = top.iter().find(|r| r.name == "Objects") else { return out };
    for g in objects.children.iter().filter(|r| r.name == "Geometry") {
        let Some(RawProp::Int(id)) = g.props.first() else { continue };
        for layer in g.children.iter().filter(|r| r.name == "LayerElementSmoothing") {
            let by_polygon = layer.children.iter().any(|c| c.name == "MappingInformationType" && matches!(c.props.first(), Some(RawProp::Str(s)) if s == "ByPolygon"));
            if !by_polygon {
                continue;
            }
            if let Some(RawProp::I32Arr(v)) = layer.children.iter().find(|c| c.name == "Smoothing").and_then(|c| c.props.first()) {
                out.insert(*id, v.iter().map(|x| *x as u32).collect());
            }
        }
    }
    out
}

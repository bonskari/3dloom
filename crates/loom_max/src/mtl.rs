//! Materials: Standard (name, the shader's colours and highlights, opacity,
//! map files) and Multi/Sub-Object. As the sample files store them:
//!
//! - Standard: 0x4000 > 0x4001 the name; references 1 Texmaps, 2 the shader
//!   (Blinn / Phong / Metal / Anisotropic …, whose first ParamBlock2 holds
//!   0 ambient, 1 diffuse, 2 specular; Blinn, Phong and Metal also 9
//!   Specular Level, 10 Glossiness (fractions) and 11 Soften), 4 the
//!   extended ParamBlock2 (1 opacity, a fraction).
//! - Texmaps: per map channel two references, the amount controller and the
//!   map (channel order Ambient, Diffuse, Specular, Specular Level?, …).
//! - Bitmap: its first ParamBlock2 carries the file's asset id, which the
//!   FileAssetMetaData2 stream names.
//! - Multi/Sub-Object: references 1… the sub-materials; ParamBlock2 entry 1
//!   the On/Off table and 3 the material id table (0-based).

use crate::params::{self, PVal};
use crate::Scene;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct MaxStandard {
    pub name: String,
    /// 0–1 swatch colours.
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
    /// The UI's numbers (Specular Level %, Glossiness 0–100, Soften 0–1, Opacity %),
    /// None when the shader does not keep them where a Blinn does.
    pub specular_level: Option<f32>,
    pub glossiness: Option<f32>,
    pub soften: Option<f32>,
    pub opacity: Option<f32>,
    /// Map channel (0 Ambient, 1 Diffuse, 2 Specular, …, 8 Bump) → bitmap file.
    pub maps: Vec<(usize, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaxSub {
    /// The material id faces use (1-based, as Loom counts them).
    pub id: u16,
    pub material: Option<usize>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MaxMaterial {
    Standard(MaxStandard),
    MultiSub { name: String, subs: Vec<MaxSub> },
}

impl MaxMaterial {
    pub fn name(&self) -> &str {
        match self {
            MaxMaterial::Standard(s) => &s.name,
            MaxMaterial::MultiSub { name, .. } => name,
        }
    }
}

const STANDARD: (u32, u32, u32) = (0x2, 0, 0xc00);
const MULTI_SUB: (u32, u32, u32) = (0x200, 0, 0xc00);
const BITMAP: (u32, u32, u32) = (0x240, 0, 0xc10);
const PARAM_BLOCK2: (u32, u32, u32) = (0x82, 0, 0x82);
/// Shaders whose ParamBlock2 keeps Specular Level / Glossiness / Soften at 9 / 10 / 11.
const BLINN: (u32, u32, u32) = (0x38, 0, 0x10b0);
const PHONG: (u32, u32, u32) = (0x37, 0, 0x10b0);
const METAL: (u32, u32, u32) = (0x39, 0, 0x10b0);

/// File names by asset id (FileAssetMetaData2: per record a 16-byte id, a
/// u32-counted UTF-16 type name and a u32-counted UTF-16 path, both counts
/// including the terminating zero).
pub fn assets(b: &[u8]) -> BTreeMap<[u8; 16], String> {
    let mut out = BTreeMap::new();
    let mut o = 0usize;
    let rd = |o: &mut usize| -> Option<String> {
        let n = u32::from_le_bytes(b.get(*o..*o + 4)?.try_into().ok()?) as usize;
        let s = b.get(*o + 4..*o + 4 + n.checked_mul(2)?)?;
        *o += 4 + n * 2;
        let u: Vec<u16> = s.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|c| *c != 0).collect();
        Some(String::from_utf16_lossy(&u))
    };
    while o + 16 <= b.len() {
        let id: [u8; 16] = b[o..o + 16].try_into().unwrap();
        o += 16;
        let (Some(_kind), Some(path)) = (rd(&mut o), rd(&mut o)) else { break };
        out.insert(id, path);
    }
    out
}

pub struct Reader<'a> {
    pub scene: &'a Scene,
    pub assets: &'a BTreeMap<[u8; 16], String>,
    pub out: Vec<MaxMaterial>,
    by_object: BTreeMap<usize, usize>,
    pub warnings: Vec<String>,
}

impl<'a> Reader<'a> {
    pub fn new(scene: &'a Scene, assets: &'a BTreeMap<[u8; 16], String>) -> Self {
        Self { scene, assets, out: Vec::new(), by_object: BTreeMap::new(), warnings: Vec::new() }
    }

    /// The material at scene object `obj`, read once; None for a type not read.
    pub fn material(&mut self, obj: usize, depth: u32) -> Option<usize> {
        if let Some(i) = self.by_object.get(&obj) {
            return Some(*i);
        }
        if depth > 8 {
            return None;
        }
        let s = self.scene;
        let name = s.chunk(obj, 0x4000).and_then(|c| c.child(0x4001)).map(|c| c.utf16()).unwrap_or_default();
        let m = if s.is(obj, STANDARD) {
            MaxMaterial::Standard(self.standard(obj, name))
        } else if s.is(obj, MULTI_SUB) {
            let r = s.refs(obj);
            let pb = r.first().copied().flatten().filter(|p| s.is(*p, PARAM_BLOCK2)).and_then(|p| s.objects.get(p));
            let ids = pb.and_then(|pb| params::int_table(pb, 3));
            let on = pb.and_then(|pb| params::int_table(pb, 1));
            let count = r.len().saturating_sub(1);
            // Reserve the slot first so a material that names itself cannot loop.
            let at = self.out.len();
            self.out.push(MaxMaterial::MultiSub { name: name.clone(), subs: Vec::new() });
            self.by_object.insert(obj, at);
            let mut subs = Vec::with_capacity(count);
            for k in 0..count {
                let id = match ids.as_ref().and_then(|t| t.get(k)) {
                    Some(v) if (0..u16::MAX as i32).contains(v) => *v as u16 + 1,
                    _ => k as u16 + 1,
                };
                let material = r[k + 1].and_then(|o| self.material(o, depth + 1));
                if r[k + 1].is_some() && material.is_none() {
                    self.warnings.push(format!("{name}: sub-material {id} ({}) is not imported", s.name(r[k + 1].unwrap())));
                }
                subs.push(MaxSub { id, material, enabled: !matches!(on.as_ref().and_then(|t| t.get(k)), Some(0)) });
            }
            self.out[at] = MaxMaterial::MultiSub { name, subs };
            return Some(at);
        } else {
            return None;
        };
        self.out.push(m);
        self.by_object.insert(obj, self.out.len() - 1);
        Some(self.out.len() - 1)
    }

    fn standard(&mut self, obj: usize, name: String) -> MaxStandard {
        let s = self.scene;
        let r = s.refs(obj);
        let grey = [150.0 / 255.0; 3];
        let mut m = MaxStandard { name: name.clone(), ambient: grey, diffuse: grey, specular: [229.0 / 255.0; 3], specular_level: None, glossiness: None, soften: None, opacity: None, maps: Vec::new() };
        let shader = r.get(2).copied().flatten();
        let spb = shader.and_then(|sh| s.refs(sh).into_iter().flatten().find(|p| s.is(*p, PARAM_BLOCK2))).and_then(|p| s.objects.get(p)).map(params::read).unwrap_or_default();
        let col = |i: u32| spb.get(&i).and_then(|v| v.v());
        match (col(0), col(1), col(2)) {
            (Some(a), Some(d), Some(sp)) => {
                m.ambient = a;
                m.diffuse = d;
                m.specular = sp;
            }
            _ => self.warnings.push(format!("{name}: shader {} colours not read; default grey used", shader.map(|o| s.name(o)).unwrap_or_else(|| "missing".into()))),
        }
        if shader.is_some_and(|o| s.is(o, BLINN) || s.is(o, PHONG) || s.is(o, METAL)) {
            let f = |i: u32| spb.get(&i).and_then(|v: &PVal| v.f());
            m.specular_level = f(9).map(|v| v * 100.0);
            m.glossiness = f(10).map(|v| v * 100.0);
            m.soften = f(11);
        } else if let Some(o) = shader {
            self.warnings.push(format!("{name}: {} highlights are not imported (only Blinn / Phong / Metal)", s.name(o)));
        }
        if let Some(ext) = r.get(4).copied().flatten().filter(|p| s.is(*p, PARAM_BLOCK2)).and_then(|p| s.objects.get(p)) {
            m.opacity = params::read(ext).get(&1).and_then(|v| v.f()).map(|v| v * 100.0);
        }
        if let Some(tm) = r.get(1).copied().flatten() {
            for (slot, map) in s.refs(tm).into_iter().enumerate().filter(|(k, _)| k % 2 == 1) {
                let Some(map) = map else { continue };
                let ch = slot / 2;
                if !s.is(map, BITMAP) {
                    self.warnings.push(format!("{name}: map channel {ch} ({}) is not imported", s.name(map)));
                    continue;
                }
                let path = s.refs(map).into_iter().flatten().filter(|p| s.is(*p, PARAM_BLOCK2)).filter_map(|p| s.objects.get(p)).flat_map(params::asset_ids).find_map(|g| self.assets.get(&g).cloned());
                match path {
                    Some(p) => m.maps.push((ch, p)),
                    None => self.warnings.push(format!("{name}: the bitmap of map channel {ch} names no file")),
                }
            }
        }
        m
    }
}

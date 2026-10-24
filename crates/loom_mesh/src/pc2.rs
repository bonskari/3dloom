//! Point Cache 2 (.pc2) files: a per-frame vertex position cache.
//!
//! Layout (little endian): the 12-byte signature `POINTCACHE2\0`, i32
//! version (1), i32 point count, f32 start frame, f32 sample rate (frames
//! per sample), i32 sample count, then sample count × point count × 3 f32.

use glam::Vec3;
use std::io::{Read, Write};

pub const SIGNATURE: &[u8; 12] = b"POINTCACHE2\0";

#[derive(Debug, Clone, PartialEq)]
pub struct Pc2 {
    pub start: f32,
    pub rate: f32,
    pub samples: Vec<Vec<Vec3>>,
}

impl Pc2 {
    pub fn points(&self) -> usize {
        self.samples.first().map_or(0, |s| s.len())
    }

    /// Positions at frame `t`: linear between samples, held at the ends.
    pub fn at(&self, t: f32) -> Option<Vec<Vec3>> {
        let n = self.samples.len();
        if n == 0 {
            return None;
        }
        let f = ((t - self.start) / self.rate.max(1e-6)).clamp(0.0, (n - 1) as f32);
        let i = (f.floor() as usize).min(n - 1);
        let j = (i + 1).min(n - 1);
        let k = f - i as f32;
        Some(self.samples[i].iter().zip(&self.samples[j]).map(|(a, b)| a.lerp(*b, k)).collect())
    }

    pub fn write(&self, mut w: impl Write) -> std::io::Result<()> {
        w.write_all(SIGNATURE)?;
        w.write_all(&1i32.to_le_bytes())?;
        w.write_all(&(self.points() as i32).to_le_bytes())?;
        w.write_all(&self.start.to_le_bytes())?;
        w.write_all(&self.rate.to_le_bytes())?;
        w.write_all(&(self.samples.len() as i32).to_le_bytes())?;
        for s in &self.samples {
            for p in s {
                for c in p.to_array() {
                    w.write_all(&c.to_le_bytes())?;
                }
            }
        }
        Ok(())
    }

    pub fn read(mut r: impl Read) -> Result<Self, String> {
        let mut bytes = Vec::new();
        r.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() < 32 || &bytes[..12] != SIGNATURE {
            return Err("not a Point Cache 2 (.pc2) file".into());
        }
        let i32_at = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let f32_at = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let (points, start, rate, count) = (i32_at(16), f32_at(20), f32_at(24), i32_at(28));
        if points < 0 || count < 0 {
            return Err("corrupt .pc2 header".into());
        }
        let (points, count) = (points as usize, count as usize);
        if points == 0 && count > 0 {
            return Err("corrupt .pc2 header: samples of no points".into());
        }
        let need = points.checked_mul(count).and_then(|n| n.checked_mul(12)).and_then(|n| n.checked_add(32)).ok_or("corrupt .pc2 header")?;
        if bytes.len() < need {
            return Err(format!(".pc2 file is truncated: {} bytes, the header needs {need}", bytes.len()));
        }
        let mut samples = Vec::with_capacity(count);
        let mut o = 32;
        for _ in 0..count {
            let mut s = Vec::with_capacity(points);
            for _ in 0..points {
                s.push(Vec3::new(f32_at(o), f32_at(o + 4), f32_at(o + 8)));
                o += 12;
            }
            samples.push(s);
        }
        Ok(Self { start, rate, samples })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_claiming_more_samples_than_the_file_holds_is_refused() {
        let mut b = SIGNATURE.to_vec();
        for v in [1i32, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&0f32.to_le_bytes());
        b.extend_from_slice(&1f32.to_le_bytes());
        b.extend_from_slice(&i32::MAX.to_le_bytes());
        assert!(Pc2::read(&b[..]).is_err(), "no points but two billion samples");
        let pc = Pc2 { start: 0.0, rate: 1.0, samples: vec![vec![Vec3::ONE; 3]; 2] };
        let mut ok = Vec::new();
        pc.write(&mut ok).unwrap();
        assert_eq!(Pc2::read(&ok[..]).unwrap(), pc);
        assert!(Pc2::read(&ok[..ok.len() - 1]).is_err(), "truncated");
    }
}

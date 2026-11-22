//! Photometric web files (IESNA LM-63, `.ies`): a luminaire's luminous
//! intensity by direction, read for a photometric light's Photometric Web
//! distribution.
//!
//! Type C photometry: vertical angles from the nadir (0°, straight down the
//! light's direction) to the zenith (180°); horizontal angles turn about
//! that axis from the light's local +X (0°) towards +Y (90°).

use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Web {
    /// Vertical angles in degrees, ascending.
    pub vertical: Vec<f32>,
    /// Horizontal angles in degrees, ascending.
    pub horizontal: Vec<f32>,
    /// Candela, one row of `vertical.len()` values per horizontal angle
    /// (the file's multiplier and ballast factor applied).
    pub candela: Vec<f32>,
    /// The largest value of `candela`.
    pub peak: f32,
    /// ∫ intensity / peak over the sphere (sr): lumens = peak candela × this.
    pub solid_angle: f32,
    /// The luminous opening's width, length and height in metres (negative: round).
    pub size: [f32; 3],
}

impl Web {
    /// Reads an LM-63 (1986, 1991, 1995, 2002) file.
    pub fn parse(text: &str) -> Result<Web, String> {
        let mut lines = text.lines();
        let tilt = loop {
            let l = lines.next().ok_or("no TILT= line: not a photometric web file")?;
            if let Some(t) = l.trim().strip_prefix("TILT=") {
                break t.trim().to_string();
            }
        };
        let rest: Vec<&str> = lines.collect();
        let mut nums = rest.iter().flat_map(|l| l.split(|c: char| c.is_whitespace() || c == ',')).filter(|s| !s.is_empty()).map(|s| s.parse::<f32>().map_err(|_| format!("not a number: {s:?}")));
        let mut next = || nums.next().unwrap_or(Err("the file ends early".into()));
        if tilt == "INCLUDE" {
            // Lamp-to-luminaire geometry, then the tilt angles and factors: a
            // lamp tilted in the fixture, which no renderer applies.
            next()?;
            let n = next()? as usize;
            for _ in 0..2 * n {
                next()?;
            }
        }
        let _lamps = next()?;
        let _lumens_per_lamp = next()?;
        let multiplier = next()?;
        let nv = next()? as usize;
        let nh = next()? as usize;
        let kind = next()? as u32;
        let units = next()? as u32;
        let mut size = [next()?, next()?, next()?];
        if units == 1 {
            // Feet.
            for s in &mut size {
                *s *= 0.3048;
            }
        }
        let ballast = next()?;
        let _ballast_lamp = next()?;
        let _watts = next()?;
        if kind != 1 {
            return Err(format!("photometric type {} (only Type C webs are read)", ["?", "C", "B", "A"].get(kind as usize).unwrap_or(&"?")));
        }
        if nv == 0 || nh == 0 || nv > 10_000 || nh > 10_000 {
            return Err(format!("{nv} vertical and {nh} horizontal angles"));
        }
        let vertical = (0..nv).map(|_| next()).collect::<Result<Vec<_>, _>>()?;
        let horizontal = (0..nh).map(|_| next()).collect::<Result<Vec<_>, _>>()?;
        let scale = multiplier * if ballast > 0.0 { ballast } else { 1.0 };
        let candela = (0..nv * nh).map(|_| next().map(|c| (c * scale).max(0.0))).collect::<Result<Vec<_>, _>>()?;
        if vertical.windows(2).any(|w| w[1] < w[0]) || horizontal.windows(2).any(|w| w[1] < w[0]) {
            return Err("the angles are not in ascending order".into());
        }
        let peak = candela.iter().copied().fold(0.0, f32::max);
        if peak <= 0.0 {
            return Err("the web gives no light".into());
        }
        let mut web = Web { vertical, horizontal, candela, peak, solid_angle: 0.0, size };
        web.solid_angle = web.integrate();
        Ok(web)
    }

    /// Candela at vertical angle `v` and horizontal angle `h` (degrees).
    pub fn candela_at(&self, v: f32, h: f32) -> f32 {
        let (first, last) = (self.horizontal[0], *self.horizontal.last().expect("at least one"));
        let mut h = h.rem_euclid(360.0);
        // The file's symmetry: one plane, a quadrant, a half, or the half from 90° to 270°.
        if self.horizontal.len() == 1 || last <= first {
            h = first;
        } else if first == 0.0 && last == 90.0 {
            if h > 180.0 {
                h = 360.0 - h;
            }
            if h > 90.0 {
                h = 180.0 - h;
            }
        } else if first == 0.0 && last == 180.0 {
            if h > 180.0 {
                h = 360.0 - h;
            }
        } else if first == 90.0 && last == 270.0 && !(90.0..=270.0).contains(&h) {
            h = (180.0 - h).rem_euclid(360.0);
        }
        let nv = self.vertical.len();
        let row = |i: usize| &self.candela[i * nv..(i + 1) * nv];
        let along = |r: &[f32]| interp(&self.vertical, r, v).unwrap_or(0.0);
        match bracket(&self.horizontal, h) {
            None => 0.0,
            Some((i, j, t)) => along(row(i)) * (1.0 - t) + along(row(j)) * t,
        }
    }

    /// Intensity towards `dir` (in the web's frame, light going down -Z) as
    /// a fraction of the peak.
    pub fn relative(&self, dir: Vec3) -> f32 {
        let d = dir.normalize_or(Vec3::NEG_Z);
        let v = (-d.z).clamp(-1.0, 1.0).acos().to_degrees();
        let h = d.y.atan2(d.x).to_degrees();
        self.candela_at(v, h) / self.peak
    }

    fn integrate(&self) -> f32 {
        let (nt, np) = (180, 72);
        let (dt, dp) = (std::f32::consts::PI / nt as f32, std::f32::consts::TAU / np as f32);
        let mut sum = 0.0;
        for i in 0..nt {
            let t = (i as f32 + 0.5) * dt;
            for j in 0..np {
                let ph = (j as f32 + 0.5) * dp;
                sum += self.candela_at(t.to_degrees(), ph.to_degrees()) * t.sin();
            }
        }
        sum * dt * dp / self.peak
    }

    /// The beam and field angles (full cone, degrees) about the nadir: where
    /// the intensity in the 0° plane falls under 50% and 10% of the peak.
    pub fn beam_field(&self) -> (f32, f32) {
        let mut beam = None;
        for i in 0..=360 {
            let v = i as f32 * 0.5;
            let c = self.candela_at(v, 0.0) / self.peak;
            if beam.is_none() && c < 0.5 {
                beam = Some(v);
            }
            if c < 0.1 {
                return (2.0 * beam.unwrap_or(v), 2.0 * v);
            }
        }
        (2.0 * beam.unwrap_or(180.0), 360.0)
    }
}

/// `xs` around `x`: (lower index, upper index, fraction); None outside.
fn bracket(xs: &[f32], x: f32) -> Option<(usize, usize, f32)> {
    let (first, last) = (xs[0], xs[xs.len() - 1]);
    if xs.len() == 1 {
        return (x == first).then_some((0, 0, 0.0));
    }
    if x < first - 1e-4 || x > last + 1e-4 {
        return None;
    }
    let j = xs.partition_point(|a| *a <= x).clamp(1, xs.len() - 1);
    let i = j - 1;
    let span = xs[j] - xs[i];
    let t = if span > 0.0 { ((x - xs[i]) / span).clamp(0.0, 1.0) } else { 0.0 };
    Some((i, j, t))
}

fn interp(xs: &[f32], ys: &[f32], x: f32) -> Option<f32> {
    if xs.len() == 1 {
        return Some(ys[0]);
    }
    bracket(xs, x).map(|(i, j, t)| ys[i] * (1.0 - t) + ys[j] * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A downlight: rotationally symmetric, 1000 cd at the nadir, half at
    /// 30°, none from 60° up (vertical angles 0–90, one horizontal plane).
    pub const DOWNLIGHT: &str = "IESNA:LM-63-2002\n[TEST] downlight\n[MANUFAC] test\nTILT=NONE\n1 -1 1.0 4 1 1 2 0.1 0.1 0.0\n1.0 1.0 20\n0 30 60 90\n0\n1000 500 0 0\n";

    #[test]
    fn a_symmetric_downlight_reads_and_interpolates() {
        let w = Web::parse(DOWNLIGHT).unwrap();
        assert_eq!((w.vertical.len(), w.horizontal.len(), w.peak), (4, 1, 1000.0));
        assert_eq!(w.candela_at(15.0, 123.0), 750.0, "any plane: the web is symmetric about its axis");
        assert_eq!(w.candela_at(120.0, 0.0), 0.0, "past the last vertical angle: no light");
        assert!((w.relative(Vec3::NEG_Z) - 1.0).abs() < 1e-6, "the nadir is the light's direction");
        assert_eq!(w.relative(Vec3::Z), 0.0);
        let (beam, field) = w.beam_field();
        assert!((beam - 60.0).abs() <= 1.0 && (field - 108.0).abs() <= 1.0, "50% at 30° off the axis, 10% at 54°: {beam} {field}");
        // ∫ over the cap: 2π ∫ f(θ) sinθ dθ with f linear 1→0.5 (0–30°), 0.5→0 (30–60°).
        assert!((w.solid_angle - 1.087).abs() < 0.01, "{}", w.solid_angle);
        assert_eq!(w.size, [0.1, 0.1, 0.0], "metres");
    }

    #[test]
    fn quadrant_symmetry_and_tilt_include_and_errors() {
        // Two planes 0° and 90° (quadrant symmetric), the 90° plane half as bright.
        let text = "IESNA91\nTILT=INCLUDE\n1\n2\n0 90\n1 1\n1 1000 2.0 2 2 1 1 1 1 0\n1 1 50\n0 90\n0 90\n100 100\n50 50\n";
        let w = Web::parse(text).unwrap();
        assert_eq!(w.peak, 200.0, "the candela multiplier applies");
        assert_eq!(w.candela_at(0.0, 0.0), 200.0);
        assert_eq!(w.candela_at(0.0, 90.0), 100.0);
        assert_eq!(w.candela_at(0.0, 270.0), 100.0, "mirrored into the first quadrant");
        assert_eq!(w.candela_at(0.0, 180.0), 200.0);
        assert!((w.size[0] - 0.3048).abs() < 1e-6, "feet");
        assert!(Web::parse("no tilt here").is_err());
        assert!(Web::parse("TILT=NONE\n1 -1 1 2 1 2 2 0 0 0 1 1 1 0 90 0 5 5").unwrap_err().contains("type B"));
        assert!(Web::parse("TILT=NONE\n1 -1 1 2 1 1 2 0 0 0 1 1 1 0 90 0 5").unwrap_err().contains("ends early"));
    }
}

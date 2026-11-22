//! The Audio controller: a sound file's amplitude drives a parameter
//! between its Base and Target values.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};

/// A decoded PCM WAV file: samples per channel in -1..1.
#[derive(Debug)]
pub struct Wave {
    pub rate: u32,
    pub channels: Vec<Vec<f32>>,
    /// The largest sample magnitude per channel.
    pub peak: Vec<f32>,
}

/// Reads an uncompressed PCM (8, 16, 24 or 32-bit integer, or 32-bit
/// float) WAV file.
pub fn read_wav(bytes: &[u8]) -> Result<Wave, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let (mut fmt, mut data) = (None, None);
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let len = u32_at(i + 4) as usize;
        let body = i + 8..(i + 8 + len).min(bytes.len());
        match &bytes[i..i + 4] {
            b"fmt " if len >= 16 => fmt = Some((u16_at(i + 8), u16_at(i + 10), u32_at(i + 12), u16_at(i + 22))),
            b"data" => data = Some(body),
            _ => {}
        }
        i += 8 + len + (len & 1);
    }
    let (format, channels, rate, bits) = fmt.ok_or("no fmt chunk")?;
    let data = &bytes[data.ok_or("no data chunk")?];
    let channels = channels.max(1) as usize;
    let width = (bits as usize).div_ceil(8);
    if !matches!((format, bits), (1, 8 | 16 | 24 | 32) | (3, 32) | (0xFFFE, 8 | 16 | 24 | 32)) {
        return Err(format!("unsupported WAV encoding (format {format}, {bits} bits)"));
    }
    let sample = |s: &[u8]| -> f32 {
        match (format, bits) {
            (3, _) => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
            (_, 8) => (s[0] as f32 - 128.0) / 128.0,
            (_, 16) => i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0,
            (_, 24) => (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32 / 8_388_608.0,
            _ => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0,
        }
    };
    let mut out = vec![Vec::new(); channels];
    for frame in data.chunks_exact(width * channels) {
        for (c, ch) in out.iter_mut().enumerate() {
            ch.push(sample(&frame[c * width..(c + 1) * width]));
        }
    }
    let peak = out.iter().map(|ch| ch.iter().fold(0.0f32, |m, s| m.max(s.abs()))).collect();
    Ok(Wave { rate, channels: out, peak })
}

/// The decoded file, read once on first use and shared by clones.
#[derive(Debug, Clone, Default)]
pub struct WaveCache(Arc<OnceLock<Option<Wave>>>);

impl PartialEq for WaveCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// Which channel drives the output (stereo files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioChannel {
    #[default]
    Left,
    Right,
    /// The greater of the two channels.
    Mix,
}

/// Audio controller: Base at amplitude 0, Target at the maximum amplitude.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioCtrl {
    /// The sound file (empty: none chosen).
    pub path: String,
    /// Absolute Value: the maximum is the file's loudest sample (off: full scale).
    pub absolute: bool,
    /// Amplitudes below this fraction drop to 0.
    pub threshold: f32,
    /// Samples averaged per reading.
    pub oversampling: u32,
    pub base: Vec<f32>,
    pub target: Vec<f32>,
    pub channel: AudioChannel,
    #[serde(skip)]
    pub wave: WaveCache,
}

impl AudioCtrl {
    /// No sound yet, Threshold 0, Oversampling 1, Left channel; Base and
    /// Target as given.
    pub fn new(base: Vec<f32>, target: Vec<f32>) -> Self {
        Self { path: String::new(), absolute: false, threshold: 0.0, oversampling: 1, base, target, channel: AudioChannel::Left, wave: WaveCache::default() }
    }

    pub fn wave(&self) -> Option<&Wave> {
        self.wave
            .0
            .get_or_init(|| {
                if self.path.is_empty() {
                    return None;
                }
                std::fs::read(&self.path).ok().and_then(|b| read_wav(&b).ok())
            })
            .as_ref()
    }

    /// The amplitude (0..1) at frame `t`: the file starts at frame 0.
    pub fn amplitude(&self, t: f32) -> f32 {
        let Some(w) = self.wave() else { return 0.0 };
        let fps = crate::anim::segment().2.max(1e-6);
        let start = (t / fps * w.rate as f32).floor();
        if start < 0.0 {
            return 0.0;
        }
        let n = self.oversampling.max(1) as usize;
        let read = |c: usize| -> Option<f32> {
            let ch = w.channels.get(c)?;
            let s = start as usize;
            if s >= ch.len() {
                return Some(0.0);
            }
            let e = (s + n).min(ch.len());
            let avg = ch[s..e].iter().map(|x| x.abs()).sum::<f32>() / (e - s) as f32;
            let max = if self.absolute { w.peak[c] } else { 1.0 };
            Some(if max > 0.0 { (avg / max).min(1.0) } else { 0.0 })
        };
        let right = if w.channels.len() > 1 { 1 } else { 0 };
        let a = match self.channel {
            AudioChannel::Left => read(0),
            AudioChannel::Right => read(right),
            AudioChannel::Mix => read(0).zip(read(right)).map(|(l, r)| l.max(r)),
        }
        .unwrap_or(0.0);
        if a < self.threshold {
            0.0
        } else {
            a
        }
    }

    pub fn eval(&self, t: f32, dims: usize) -> Vec<f32> {
        let a = self.amplitude(t);
        (0..dims)
            .map(|i| {
                let b = self.base.get(i).copied().unwrap_or(0.0);
                let g = self.target.get(i).copied().unwrap_or(b);
                b + (g - b) * a
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit stereo WAV at 30 samples a second: left `l`, right `r`.
    pub fn wav16(l: &[i16], r: &[i16]) -> Vec<u8> {
        let mut data = Vec::new();
        for (a, b) in l.iter().zip(r) {
            data.extend(a.to_le_bytes());
            data.extend(b.to_le_bytes());
        }
        let mut f = b"RIFF".to_vec();
        f.extend((36 + data.len() as u32).to_le_bytes());
        f.extend(b"WAVEfmt ");
        f.extend(16u32.to_le_bytes());
        f.extend(1u16.to_le_bytes());
        f.extend(2u16.to_le_bytes());
        f.extend(30u32.to_le_bytes());
        f.extend((30u32 * 4).to_le_bytes());
        f.extend(4u16.to_le_bytes());
        f.extend(16u16.to_le_bytes());
        f.extend(b"data");
        f.extend((data.len() as u32).to_le_bytes());
        f.extend(data);
        f
    }

    #[test]
    fn amplitude_drives_base_to_target() {
        let w = read_wav(&wav16(&[0, 16384, -32768, 8192], &[0, 0, 0, 32767])).unwrap();
        assert_eq!(w.channels.len(), 2);
        assert_eq!(w.peak[0], 1.0);
        let mut a = AudioCtrl::new(vec![0.0], vec![10.0]);
        let _ = a.wave.0.set(Some(w));
        crate::anim::set_segment(0.0, 100.0, 30.0);
        assert_eq!(a.eval(1.0, 1), vec![5.0], "half amplitude: halfway");
        assert_eq!(a.eval(2.0, 1), vec![10.0]);
        a.channel = AudioChannel::Mix;
        assert_eq!(a.eval(3.0, 1)[0], 32767.0 / 32768.0 * 10.0, "Mix takes the louder channel");
        a.channel = AudioChannel::Left;
        a.threshold = 0.3;
        assert_eq!(a.eval(3.0, 1), vec![0.0], "below Threshold: 0");
        a.threshold = 0.0;
        a.oversampling = 2;
        assert_eq!(a.eval(1.0, 1), vec![7.5], "Oversampling averages");
        assert_eq!(a.eval(40.0, 1), vec![0.0], "past the end: silence");
    }
}

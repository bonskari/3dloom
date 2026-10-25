//! Time Configuration > Time Display (frames, SMPTE, FRAME:TICKS,
//! MM:SS:TICKS) and Key Steps (which keys Key Mode steps between). Saved
//! with the scene.

use crate::command::Command as C;
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Ticks per second.
pub const TICKS: i64 = 4800;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimeDisplay {
    #[default]
    Frames,
    #[serde(rename = "SMPTE")]
    Smpte,
    FrameTicks,
    MmSsTicks,
}

impl TimeDisplay {
    pub const ALL: [TimeDisplay; 4] = [TimeDisplay::Frames, TimeDisplay::Smpte, TimeDisplay::FrameTicks, TimeDisplay::MmSsTicks];

    pub fn label(self) -> &'static str {
        match self {
            TimeDisplay::Frames => "Frames",
            TimeDisplay::Smpte => "SMPTE",
            TimeDisplay::FrameTicks => "FRAME:TICKS",
            TimeDisplay::MmSsTicks => "MM:SS:TICKS",
        }
    }
}

/// Key Steps: Key Mode steps between the track bar's keys (Use TrackBar),
/// or between the transform keys picked here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeySteps {
    pub use_track_bar: bool,
    pub selected_only: bool,
    /// The keys of the active transform tool (Move: position, Rotate: rotation, Scale: scale).
    pub use_current_transform: bool,
    pub position: bool,
    pub rotation: bool,
    pub scale: bool,
}

impl Default for KeySteps {
    fn default() -> Self {
        Self { use_track_bar: true, selected_only: true, use_current_transform: true, position: true, rotation: true, scale: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TimeConfig {
    pub display: TimeDisplay,
    pub key_steps: KeySteps,
}

fn whole_fps(fps: f32) -> i64 {
    (fps.round() as i64).max(1)
}

/// `frame` as the Time Display shows it.
pub fn format(frame: i32, fps: f32, mode: TimeDisplay) -> String {
    let sign = if frame < 0 { "-" } else { "" };
    let f = (frame as i64).abs();
    let ticks = (f as f64 * TICKS as f64 / fps.max(1e-3) as f64).round() as i64;
    match mode {
        TimeDisplay::Frames => frame.to_string(),
        TimeDisplay::Smpte => {
            let r = whole_fps(fps);
            format!("{sign}{}:{}:{}", f / r / 60, f / r % 60, f % r)
        }
        TimeDisplay::FrameTicks => format!("{sign}{f}:0"),
        TimeDisplay::MmSsTicks => format!("{sign}{}:{}:{}", ticks / TICKS / 60, ticks / TICKS % 60, ticks % TICKS),
    }
}

/// A typed time (in the Time Display's format, or plain frames) as a frame.
pub fn parse(text: &str, fps: f32, mode: TimeDisplay) -> Option<f64> {
    let t = text.trim();
    let (neg, t) = t.strip_prefix('-').map(|r| (true, r)).unwrap_or((false, t));
    let parts: Vec<f64> = t.split(':').map(|p| p.trim().parse::<f64>().ok()).collect::<Option<_>>()?;
    let fps = fps.max(1e-3) as f64;
    let ticks_per_frame = TICKS as f64 / fps;
    let v = match (mode, parts.as_slice()) {
        (_, [f]) => *f,
        (TimeDisplay::FrameTicks, [f, tk]) => f + tk / ticks_per_frame,
        (TimeDisplay::Smpte, [s, f]) => s * whole_fps(fps as f32) as f64 + f,
        (TimeDisplay::Smpte, [m, s, f]) => (m * 60.0 + s) * whole_fps(fps as f32) as f64 + f,
        (TimeDisplay::MmSsTicks, [s, tk]) => s * fps + tk / ticks_per_frame,
        (TimeDisplay::MmSsTicks, [m, s, tk]) => (m * 60.0 + s) * fps + tk / ticks_per_frame,
        _ => return None,
    };
    Some(if neg { -v } else { v })
}

pub fn handles(c: &C) -> bool {
    matches!(c, C::SetTimeDisplay { .. } | C::SetKeySteps { .. })
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::SetTimeDisplay { mode } => {
            state.time_config.display = mode;
            Ok(json!({ "display": mode }))
        }
        C::SetKeySteps { use_track_bar, selected_only, use_current_transform, position, rotation, scale } => {
            let k = &mut state.time_config.key_steps;
            for (field, v) in [(&mut k.use_track_bar, use_track_bar), (&mut k.selected_only, selected_only), (&mut k.use_current_transform, use_current_transform), (&mut k.position, position), (&mut k.rotation, rotation), (&mut k.scale, scale)] {
                if let Some(v) = v {
                    *field = v;
                }
            }
            Ok(json!(*k))
        }
        other => Err(format!("{} is not a Time Configuration command", other.name())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_parses_each_time_display() {
        assert_eq!(format(75, 30.0, TimeDisplay::Frames), "75");
        assert_eq!(format(75, 30.0, TimeDisplay::Smpte), "0:2:15");
        assert_eq!(format(1830, 30.0, TimeDisplay::Smpte), "1:1:0");
        assert_eq!(format(15, 30.0, TimeDisplay::FrameTicks), "15:0");
        assert_eq!(format(45, 30.0, TimeDisplay::MmSsTicks), "0:1:2400");
        assert_eq!(format(-5, 30.0, TimeDisplay::Smpte), "-0:0:5");
        for mode in TimeDisplay::ALL {
            for f in [0, 1, 29, 30, 75, 1830] {
                assert_eq!(parse(&format(f, 30.0, mode), 30.0, mode), Some(f as f64), "{mode:?} {f}");
            }
        }
        assert_eq!(parse("40", 30.0, TimeDisplay::Smpte), Some(40.0), "plain frames always work");
        assert_eq!(parse("1:x", 30.0, TimeDisplay::Smpte), None);
    }
}

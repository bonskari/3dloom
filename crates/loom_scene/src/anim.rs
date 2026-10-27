//! Animation time and keyed float tracks.
//!
//! The ambient evaluation time and the Auto Key flag live in thread-local
//! state: the scene
//! sets the time before it evaluates, a command sets both before it edits,
//! and every `Param` reads them.

use serde::{Deserialize, Serialize};
use std::cell::Cell;

thread_local! {
    static TIME: Cell<f32> = const { Cell::new(0.0) };
    static ANIMATE: Cell<bool> = const { Cell::new(false) };
    static READS: Cell<u64> = const { Cell::new(0) };
    static TANGENTS: Cell<(TangentType, TangentType)> = const { Cell::new((TangentType::Auto, TangentType::Auto)) };
    static RENDERING: Cell<bool> = const { Cell::new(false) };
}

/// Whether the scene evaluates for a render (Skin's Fast Updates shows a
/// rigid preview in the viewports only).
pub fn rendering() -> bool {
    RENDERING.with(|c| c.get())
}

/// Runs `f` evaluating for a render.
pub fn with_rendering<R>(f: impl FnOnce() -> R) -> R {
    with_rendering_state(true, f)
}

/// Preserves an executor thread's render mode around a scoped evaluation.
pub(crate) fn with_rendering_state<R>(rendering: bool, f: impl FnOnce() -> R) -> R {
    let was = RENDERING.with(|c| c.replace(rendering));
    let r = f();
    RENDERING.with(|c| c.set(was));
    r
}

/// The frame parameters evaluate at.
pub fn time() -> f32 {
    TIME.with(|c| c.get())
}

pub fn set_time(t: f32) {
    TIME.with(|c| c.set(t));
}

/// Auto Key: whether `Param::set` creates keys.
pub fn animate() -> bool {
    ANIMATE.with(|c| c.get())
}

pub fn set_animate(on: bool) {
    ANIMATE.with(|c| c.set(on));
}

/// Default In / Out tangent for new keys.
pub fn default_tangents() -> (TangentType, TangentType) {
    TANGENTS.with(|c| c.get())
}

pub fn set_default_tangents(t: (TangentType, TangentType)) {
    TANGENTS.with(|c| c.set(t));
}

/// Runs `f` at `time` with Auto Key `animate`, restoring both afterwards.
pub fn with<R>(time: f32, animate: bool, f: impl FnOnce() -> R) -> R {
    let (t0, a0) = (self::time(), self::animate());
    set_time(time);
    set_animate(animate);
    let r = f();
    set_time(t0);
    set_animate(a0);
    r
}

/// How many animated parameters have been read on this thread: a cache
/// compares it before and after a build to know whether the result
/// depends on time.
pub fn animated_reads() -> u64 {
    READS.with(|c| c.get())
}

pub fn note_animated_read() {
    READS.with(|c| c.set(c.get().wrapping_add(1)));
}

/// Bezier key tangent types (Default In/Out Tangents).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TangentType {
    #[default]
    Auto,
    Smooth,
    Linear,
    Step,
    Fast,
    Slow,
    Custom,
}

/// Parameter Curve Out-of-Range Types.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Ort {
    #[default]
    Constant,
    Cycle,
    Loop,
    PingPong,
    Linear,
    RelativeRepeat,
}

/// Interpolation of a track: Bezier, TCB or Linear controller.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Interp {
    #[default]
    Bezier,
    Tcb,
    Linear,
}

/// One key. Tangent slopes are in value per frame and used when the type
/// is Custom; tension, continuity, bias and ease are 0–50 TCB values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Key {
    pub time: f32,
    pub value: f32,
    pub in_type: TangentType,
    pub out_type: TangentType,
    pub in_tan: f32,
    pub out_tan: f32,
    pub tension: f32,
    pub continuity: f32,
    pub bias: f32,
    pub ease_to: f32,
    pub ease_from: f32,
    pub selected: bool,
    /// Break Tangents: the In and Out handles move independently.
    pub broken: bool,
    /// Key Info > Advanced > Constant Velocity: the segment after this key
    /// runs at an even speed.
    pub constant_velocity: bool,
}

impl Default for Key {
    fn default() -> Self {
        Self { time: 0.0, value: 0.0, in_type: TangentType::Auto, out_type: TangentType::Auto, in_tan: 0.0, out_tan: 0.0, tension: 25.0, continuity: 25.0, bias: 25.0, ease_to: 0.0, ease_from: 0.0, selected: false, broken: false, constant_velocity: false }
    }
}

/// A float curve: sorted keys plus the out-of-range types at both ends.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct FloatTrack {
    pub interp: Interp,
    pub keys: Vec<Key>,
    pub before: Ort,
    pub after: Ort,
}

const SAME_TIME: f32 = 1e-4;

#[derive(Clone, Copy, PartialEq)]
enum Side {
    In,
    Out,
}

impl FloatTrack {
    pub fn new(interp: Interp) -> Self {
        Self { interp, ..Default::default() }
    }

    pub fn key_at(&self, t: f32) -> Option<usize> {
        self.keys.iter().position(|k| (k.time - t).abs() < SAME_TIME)
    }

    /// Sets the key at `t` (keeping its tangents) or inserts a new one with
    /// the given In / Out tangent types.
    pub fn set_key(&mut self, t: f32, v: f32, tangents: (TangentType, TangentType)) -> usize {
        if let Some(i) = self.key_at(t) {
            self.keys[i].value = v;
            return i;
        }
        let i = self.keys.partition_point(|k| k.time < t);
        self.keys.insert(i, Key { time: t, value: v, in_type: tangents.0, out_type: tangents.1, ..Default::default() });
        i
    }

    pub fn offset(&mut self, dv: f32) {
        for k in &mut self.keys {
            k.value += dv;
        }
    }

    /// Keeps keys sorted by time after they were moved.
    pub fn sort(&mut self) {
        self.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
    }

    /// The value at `t`, or None for a track without keys.
    pub fn eval(&self, t: f32) -> Option<f32> {
        let n = self.keys.len();
        if n == 0 {
            return None;
        }
        let (first, last) = (self.keys[0].time, self.keys[n - 1].time);
        let period = last - first;
        if n == 1 || period <= 0.0 {
            return Some(self.keys[0].value);
        }
        let (ort, edge) = if t < first {
            (self.before, Side::In)
        } else if t > last {
            (self.after, Side::Out)
        } else {
            return Some(self.eval_in(t));
        };
        let (v0, v1) = (self.keys[0].value, self.keys[n - 1].value);
        let cycles = ((t - first) / period).floor();
        let local = (t - first).rem_euclid(period);
        Some(match ort {
            Ort::Constant => if edge == Side::In { v0 } else { v1 },
            Ort::Cycle | Ort::Loop => self.eval_in(first + local),
            Ort::PingPong => {
                if (cycles as i64).rem_euclid(2) == 1 { self.eval_in(last - local) } else { self.eval_in(first + local) }
            }
            Ort::RelativeRepeat => self.eval_in(first + local) + cycles * (v1 - v0),
            Ort::Linear => {
                if edge == Side::In { v0 + self.end_slope(0, Side::Out) * (t - first) } else { v1 + self.end_slope(n - 1, Side::In) * (t - last) }
            }
        })
    }

    fn end_slope(&self, i: usize, side: Side) -> f32 {
        let k = &self.keys[i];
        let ty = if side == Side::In { k.in_type } else { k.out_type };
        match self.interp {
            Interp::Linear => self.linear_slope(i, side),
            Interp::Tcb => self.tcb_slope(i, side),
            Interp::Bezier if ty == TangentType::Step => 0.0,
            Interp::Bezier => self.bezier_slope(i, side),
        }
    }

    /// The neighbour key before (-1) or after (+1) key `i` as (time, value);
    /// Loop wraps around so a matching first and last key loop smoothly.
    fn neighbour(&self, i: usize, dir: i32) -> Option<(f32, f32)> {
        let n = self.keys.len();
        let period = self.keys[n - 1].time - self.keys[0].time;
        if dir < 0 {
            if i > 0 {
                return Some((self.keys[i - 1].time, self.keys[i - 1].value));
            }
            (self.before == Ort::Loop && n > 2).then(|| (self.keys[n - 2].time - period, self.keys[n - 2].value))
        } else {
            if i + 1 < n {
                return Some((self.keys[i + 1].time, self.keys[i + 1].value));
            }
            (self.after == Ort::Loop && n > 2).then(|| (self.keys[1].time + period, self.keys[1].value))
        }
    }

    fn deltas(&self, i: usize) -> (Option<f32>, Option<f32>) {
        let k = &self.keys[i];
        let prev = self.neighbour(i, -1).map(|(t, v)| (k.value - v) / (k.time - t));
        let next = self.neighbour(i, 1).map(|(t, v)| (v - k.value) / (t - k.time));
        (prev, next)
    }

    fn linear_slope(&self, i: usize, side: Side) -> f32 {
        let (p, n) = self.deltas(i);
        if side == Side::In { p.or(n) } else { n.or(p) }.unwrap_or(0.0)
    }

    fn bezier_slope(&self, i: usize, side: Side) -> f32 {
        let k = &self.keys[i];
        let ty = if side == Side::In { k.in_type } else { k.out_type };
        let (p, n) = self.deltas(i);
        let smooth = || match (self.neighbour(i, -1), self.neighbour(i, 1)) {
            (Some((tp, vp)), Some((tn, vn))) => (vn - vp) / (tn - tp),
            _ => p.or(n).unwrap_or(0.0),
        };
        match ty {
            TangentType::Custom => if side == Side::In { k.in_tan } else { k.out_tan },
            TangentType::Step => 0.0,
            TangentType::Linear => self.linear_slope(i, side),
            TangentType::Fast => 2.0 * self.linear_slope(i, side),
            TangentType::Slow => 0.25 * self.linear_slope(i, side),
            TangentType::Smooth => smooth(),
            TangentType::Auto => match (p, n) {
                (Some(p), Some(n)) if p * n > 0.0 => {
                    // Catmull-Rom slope, clamped so the curve never overshoots (Fritsch-Carlson).
                    let m = smooth();
                    m.clamp(-3.0 * p.abs().min(n.abs()), 3.0 * p.abs().min(n.abs()))
                }
                _ => 0.0,
            },
        }
    }

    fn tcb_slope(&self, i: usize, side: Side) -> f32 {
        let k = &self.keys[i];
        let (p, n) = self.deltas(i);
        let (d0, d1) = match (p, n) {
            (Some(p), Some(n)) => (p, n),
            (Some(p), None) => (p, p),
            (None, Some(n)) => (n, n),
            (None, None) => (0.0, 0.0),
        };
        let (t, c, b) = ((k.tension - 25.0) / 25.0, (k.continuity - 25.0) / 25.0, (k.bias - 25.0) / 25.0);
        if side == Side::Out {
            (1.0 - t) * (1.0 + c) * (1.0 + b) / 2.0 * d0 + (1.0 - t) * (1.0 - c) * (1.0 - b) / 2.0 * d1
        } else {
            (1.0 - t) * (1.0 - c) * (1.0 + b) / 2.0 * d0 + (1.0 - t) * (1.0 + c) * (1.0 - b) / 2.0 * d1
        }
    }

    fn eval_in(&self, t: f32) -> f32 {
        let n = self.keys.len();
        if t >= self.keys[n - 1].time {
            return self.keys[n - 1].value;
        }
        let i = self.keys.partition_point(|k| k.time <= t).saturating_sub(1);
        let (a, b) = (&self.keys[i], &self.keys[i + 1]);
        let dt = b.time - a.time;
        let s = (t - a.time) / dt;
        if a.constant_velocity {
            return a.value + (b.value - a.value) * s;
        }
        match self.interp {
            Interp::Linear => a.value + (b.value - a.value) * s,
            Interp::Bezier => {
                if a.out_type == TangentType::Step || b.in_type == TangentType::Step {
                    return a.value;
                }
                hermite(a.value, self.bezier_slope(i, Side::Out) * dt, b.value, self.bezier_slope(i + 1, Side::In) * dt, s)
            }
            Interp::Tcb => {
                let s = ease(s, a.ease_from / 50.0, b.ease_to / 50.0);
                hermite(a.value, self.tcb_slope(i, Side::Out) * dt, b.value, self.tcb_slope(i + 1, Side::In) * dt, s)
            }
        }
    }
}

fn hermite(p0: f32, m0: f32, p1: f32, m1: f32, s: f32) -> f32 {
    let (s2, s3) = (s * s, s * s * s);
    (2.0 * s3 - 3.0 * s2 + 1.0) * p0 + (s3 - 2.0 * s2 + s) * m0 + (-2.0 * s3 + 3.0 * s2) * p1 + (s3 - s2) * m1
}

/// TCB Ease From / Ease To: slows time near the keys (0 = none, 1 = full).
fn ease(s: f32, from: f32, to: f32) -> f32 {
    if from <= 0.0 && to <= 0.0 {
        return s;
    }
    hermite(0.0, 1.0 - from.clamp(0.0, 1.0), 1.0, 1.0 - to.clamp(0.0, 1.0), s)
}

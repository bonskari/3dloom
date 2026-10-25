//! Spacing Tool: the Distribution modes (where along a path the objects go)
//! and walking a spline path by arc length.

use glam::Vec3;

/// The Distribution drop-down, in list order.
pub const DISTRIBUTIONS: [&str; 19] = [
    "Free Center",
    "Divide Evenly, Objects at Ends",
    "Centered, Specify Spacing",
    "End Offset",
    "End Offset, Divide Evenly",
    "End Offset, Specify Spacing",
    "Start Offset",
    "Start Offset, Divide Evenly",
    "Start Offset, Specify Spacing",
    "Specify Offsets and Spacing",
    "Specify Offsets, Divide Evenly",
    "Space from End, Unbounded",
    "Space from End, Specify Number",
    "Space from End, Specify Spacing",
    "Space from Start, Unbounded",
    "Space from Start, Specify Number",
    "Space from Start, Specify Spacing",
    "Specify Spacing, Matching Offsets",
    "Divide Evenly, No Objects at Ends",
];

/// The Distribution the dialog starts with.
pub const DEFAULT_DISTRIBUTION: u32 = 1;

/// Which of Count, Spacing, Start Offset and End Offset a mode reads (the
/// others are unavailable in the dialog).
pub fn uses(mode: u32) -> [bool; 4] {
    match mode {
        0 => [true, true, false, false],
        1 => [true, false, false, false],
        2 => [false, true, false, false],
        3 => [true, true, false, true],
        4 => [true, false, false, true],
        5 => [false, true, false, true],
        6 => [true, true, true, false],
        7 => [true, false, true, false],
        8 => [false, true, true, false],
        9 => [false, true, true, true],
        10 => [true, false, true, true],
        11 => [true, true, false, false],
        12 => [true, false, false, false],
        13 => [false, true, false, false],
        14 => [true, true, false, false],
        15 => [true, false, false, false],
        16 => [false, true, false, false],
        17 => [false, true, false, false],
        _ => [true, false, false, false],
    }
}

/// Free Center lays its objects on the straight line from the path's start
/// toward its end, ignoring the path's curve.
pub fn straight(mode: u32) -> bool {
    mode == 0
}

/// Most objects a "fit as many as possible" mode places.
const MAX_FIT: usize = 10_000;

/// Distances along a path of `length` where the object centres go.
/// `extent` is the objects' size along the path in the Edges context (0 for
/// Centers): spacing then runs between facing edges, and objects at an end
/// stay inside the path.
pub fn distances(mode: u32, length: f32, count: u32, spacing: f32, start_offset: f32, end_offset: f32, extent: f32) -> Vec<f32> {
    let n = count.max(1) as usize;
    let half = extent * 0.5;
    let step = spacing + extent;
    // `n` objects from `a` to `b` (one: at `single`).
    let divide = |a: f32, b: f32, single: f32| -> Vec<f32> {
        if n == 1 {
            return vec![single];
        }
        (0..n).map(|i| a + (b - a) * i as f32 / (n - 1) as f32).collect()
    };
    // As many as fit from `a` toward `b` (`dir` ±1) one step apart.
    let fit = |a: f32, b: f32, dir: f32| -> Vec<f32> {
        if step <= 1e-6 {
            return vec![a];
        }
        let room = (b - a) * dir;
        if room < -1e-4 {
            return Vec::new();
        }
        let m = ((room + 1e-4) / step).floor() as usize + 1;
        (0..m.min(MAX_FIT)).map(|i| a + dir * step * i as f32).collect()
    };
    let (lo, hi) = (half, length - half);
    match mode {
        0 => (0..n).map(|i| half + step * i as f32).collect(),
        1 => divide(lo, hi, length * 0.5),
        2 => {
            let pts = fit(lo, hi, 1.0);
            let used = pts.last().copied().unwrap_or(lo) - lo;
            let shift = (hi - lo - used) * 0.5;
            pts.into_iter().map(|p| p + shift).collect()
        }
        3 => (0..n).map(|i| length - end_offset - half - step * i as f32).collect(),
        4 => divide(lo, length - end_offset - half, length - end_offset - half),
        5 => fit(length - end_offset - half, lo, -1.0),
        6 => (0..n).map(|i| start_offset + half + step * i as f32).collect(),
        7 => divide(start_offset + half, hi, start_offset + half),
        8 => fit(start_offset + half, hi, 1.0),
        9 => fit(start_offset + half, length - end_offset - half, 1.0),
        10 => divide(start_offset + half, length - end_offset - half, (start_offset + length - end_offset) * 0.5),
        11 => (0..n).map(|i| length - step * (i + 1) as f32).collect(),
        12 => {
            let d = length / n as f32;
            (0..n).map(|i| length - d * (i + 1) as f32).collect()
        }
        13 => fit(length - step, 0.0, -1.0),
        14 => (0..n).map(|i| step * (i + 1) as f32).collect(),
        15 => {
            let d = length / n as f32;
            (0..n).map(|i| d * (i + 1) as f32).collect()
        }
        16 => fit(step, length, 1.0),
        17 => fit(step, length - step, 1.0),
        _ => {
            let d = length / (n + 1) as f32;
            (0..n).map(|i| d * (i + 1) as f32).collect()
        }
    }
}

/// A polyline walked by arc length; distances past either end continue
/// along the end segment's direction.
pub struct PathWalk {
    points: Vec<Vec3>,
    /// Arc length at each point.
    at: Vec<f32>,
}

impl PathWalk {
    pub fn new(points: Vec<Vec3>) -> Self {
        let mut at = Vec::with_capacity(points.len());
        let mut s = 0.0;
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                s += p.distance(points[i - 1]);
            }
            at.push(s);
        }
        Self { points, at }
    }

    pub fn length(&self) -> f32 {
        self.at.last().copied().unwrap_or(0.0)
    }

    /// The point and unit tangent at distance `s`.
    pub fn sample(&self, s: f32) -> (Vec3, Vec3) {
        match self.points.len() {
            0 => return (Vec3::ZERO, Vec3::X),
            1 => return (self.points[0], Vec3::X),
            _ => {}
        }
        let last = self.points.len() - 1;
        let i = self.at.partition_point(|a| *a <= s).clamp(1, last);
        let (a, b) = (self.points[i - 1], self.points[i]);
        let seg = (self.at[i] - self.at[i - 1]).max(1e-9);
        let t = (s - self.at[i - 1]) / seg;
        (a + (b - a) * t, (b - a).normalize_or(Vec3::X))
    }
}

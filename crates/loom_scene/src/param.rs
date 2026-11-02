use crate::anim::{self, FloatTrack, Interp, TangentType};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// The value types parameters hold, as float components a track can key.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ParamType {
    F32,
    U32,
    Bool,
    Vec3,
}

pub trait Animatable: Clone + PartialEq {
    fn comps(&self) -> Vec<f32>;
    fn from_comps(c: &[f32]) -> Self;
    fn dims() -> usize;
    fn ty() -> ParamType;
    /// Integer and on/off parameters hold their value between keys.
    fn stepped() -> bool {
        false
    }
}

impl Animatable for f32 {
    fn comps(&self) -> Vec<f32> {
        vec![*self]
    }
    fn from_comps(c: &[f32]) -> Self {
        c[0]
    }
    fn dims() -> usize {
        1
    }
    fn ty() -> ParamType {
        ParamType::F32
    }
}

impl Animatable for u32 {
    fn comps(&self) -> Vec<f32> {
        vec![*self as f32]
    }
    fn from_comps(c: &[f32]) -> Self {
        c[0].round().max(0.0) as u32
    }
    fn dims() -> usize {
        1
    }
    fn ty() -> ParamType {
        ParamType::U32
    }
    fn stepped() -> bool {
        true
    }
}

impl Animatable for bool {
    fn comps(&self) -> Vec<f32> {
        vec![if *self { 1.0 } else { 0.0 }]
    }
    fn from_comps(c: &[f32]) -> Self {
        c[0] > 0.5
    }
    fn dims() -> usize {
        1
    }
    fn ty() -> ParamType {
        ParamType::Bool
    }
    fn stepped() -> bool {
        true
    }
}

impl Animatable for Vec3 {
    fn comps(&self) -> Vec<f32> {
        self.to_array().to_vec()
    }
    fn from_comps(c: &[f32]) -> Self {
        Vec3::new(c[0], c[1], c[2])
    }
    fn dims() -> usize {
        3
    }
    fn ty() -> ParamType {
        ParamType::Vec3
    }
}

/// One float track per component (Position XYZ / Euler XYZ style); `rest`
/// is the value of a component that has no keys.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KeyedTracks {
    pub ty: ParamType,
    pub tracks: Vec<FloatTrack>,
    pub rest: Vec<f32>,
    /// One key carries every component (Bezier / TCB / Linear Position,
    /// Rotation, Scale) instead of a track per axis (Position XYZ, Euler XYZ).
    #[serde(default)]
    pub linked: bool,
    /// Smooth Rotation: linked keys whose curves are always smooth (Auto
    /// tangents, no key properties to adjust).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub smooth: bool,
}

impl KeyedTracks {
    pub fn from_rest(ty: ParamType, rest: Vec<f32>) -> Self {
        Self { ty, tracks: rest.iter().map(|_| FloatTrack::new(Interp::Bezier)).collect(), rest, linked: false, smooth: false }
    }

    pub fn comps_at(&self, t: f32) -> Vec<f32> {
        self.tracks.iter().zip(&self.rest).map(|(tr, r)| tr.eval_or_rest(t, *r)).collect()
    }

    pub fn has_keys(&self) -> bool {
        self.tracks.iter().any(|t| !t.keys.is_empty() || t.ease_curve.is_some() || t.multiplier_curve.is_some() || t.limit_enabled)
    }
}

/// Noise controller: fractal gradient noise per component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NoiseCtrl {
    pub seed: u32,
    pub frequency: f32,
    pub fractal: bool,
    pub roughness: f32,
    pub ramp_in: f32,
    pub ramp_out: f32,
    /// The frame Ramp Out ends at (the controller's range end).
    pub end: f32,
    /// Peak-to-peak strength per component.
    pub strength: Vec<f32>,
    /// >0 per component: the noise stays between 0 and the strength.
    pub positive: Vec<bool>,
    /// The value the noise varies around, per component (empty: zero). A
    /// Noise Scale holds the scale it was assigned over, so the object
    /// keeps its size instead of shrinking to the bare noise.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub center: Vec<f32>,
}

impl Default for NoiseCtrl {
    fn default() -> Self {
        Self::new(0)
    }
}

impl NoiseCtrl {
    /// Defaults: Frequency 0.5, Strength 50, no fractal, no ramps.
    pub fn new(seed: u32) -> Self {
        Self { seed, frequency: 0.5, fractal: false, roughness: 0.0, ramp_in: 0.0, ramp_out: 0.0, end: 100.0, strength: vec![50.0], positive: Vec::new(), center: Vec::new() }
    }

    pub fn eval(&self, t: f32, dims: usize) -> Vec<f32> {
        let mut ramp = 1.0;
        if self.ramp_in > 0.0 {
            ramp *= smoothstep((t / self.ramp_in).clamp(0.0, 1.0));
        }
        if self.ramp_out > 0.0 {
            ramp *= smoothstep(((self.end - t) / self.ramp_out).clamp(0.0, 1.0));
        }
        (0..dims)
            .map(|i| {
                let strength = self.strength.get(i).or(self.strength.last()).copied().unwrap_or(0.0);
                let seed = self.seed.wrapping_mul(0x9E37_79B9).wrapping_add((i as u32).wrapping_mul(0x85EB_CA6B));
                let x = t * self.frequency * 0.1;
                let n = if self.fractal {
                    let gain = 0.5 + 0.5 * self.roughness.clamp(0.0, 1.0);
                    let (mut sum, mut amp, mut norm, mut f) = (0.0, 1.0, 0.0, 1.0);
                    for o in 0..5u32 {
                        sum += amp * gradient_noise(x * f, seed.wrapping_add(o * 7919));
                        norm += amp;
                        amp *= gain;
                        f *= 2.0;
                    }
                    sum / norm
                } else {
                    gradient_noise(x, seed)
                };
                let n = n.clamp(-1.0, 1.0);
                let n = if self.positive.get(i).copied().unwrap_or(false) { n * 0.5 + 0.5 } else { n * 0.5 };
                self.center.get(i).copied().unwrap_or(0.0) + n * strength * ramp
            })
            .collect()
    }
}

fn smoothstep(s: f32) -> f32 {
    s * s * (3.0 - 2.0 * s)
}

fn hash(i: i32, seed: u32) -> f32 {
    let mut h = (i as u32) ^ seed;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// 1D gradient (Perlin) noise scaled to about -1..1.
fn gradient_noise(x: f32, seed: u32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let (g0, g1) = (hash(i as i32, seed), hash(i as i32 + 1, seed));
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    2.0 * (g0 * f + (g1 * (f - 1.0) - g0 * f) * u)
}

/// Another object whose motion acts as a spring on a Spring controller.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpringLink {
    /// The object's Position track (its controller copied and kept current).
    pub source: crate::expr::ExprSource,
    pub tension: f32,
    pub dampening: f32,
}

/// Spring controller: secondary mass / spring dynamics on top of the
/// original motion (`inner`), simulated from Start Frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpringCtrl<T> {
    pub inner: Controller<T>,
    pub mass: f32,
    pub drag: f32,
    /// Self Influence: the spring to the original motion.
    pub tension: f32,
    pub dampening: f32,
    #[serde(default)]
    pub springs: Vec<SpringLink>,
    pub start: f32,
    pub iterations: u32,
    /// X / Y / Z Effect, percent.
    pub effect: [f32; 3],
}

/// Spring force per unit of Tension, and damping per unit of Dampening x
/// Tension and of Drag (forces in mass units per frame squared).
const SPRING_K: f32 = 60.0;
const SPRING_C: f32 = 30.0;
const DRAG_C: f32 = 15.0;

impl<T: Animatable> SpringCtrl<T> {
    /// Wraps `inner` with the reference defaults: Drag 1, Start Frame 0,
    /// Iterations 2, Effect 100 (Mass 300, Tension 2, Dampening 0.5).
    pub fn new(inner: Controller<T>) -> Self {
        Self { inner, mass: 300.0, drag: 1.0, tension: 2.0, dampening: 0.5, springs: Vec::new(), start: 0.0, iterations: 2, effect: [100.0; 3] }
    }

    /// The simulated value at frame `t`: the spring-held point, its offset
    /// from the original motion scaled by the axis Effect.
    pub fn eval(&self, t: f32) -> Vec<f32> {
        let goal = |x: f32| self.inner.comps_at(x);
        let g = goal(t);
        if t <= self.start || self.mass <= 0.0 {
            return g;
        }
        let dims = g.len();
        let others: Vec<(&SpringLink, Vec<f32>)> = self.springs.iter().filter_map(|l| l.source.copy.as_ref().map(|c| (l, c.comps_at(self.start)))).collect();
        // Stiffer springs take smaller steps so the solution stays stable.
        let stiff = self.tension.abs() + others.iter().map(|(l, _)| l.tension.abs()).sum::<f32>();
        let omega = (stiff * SPRING_K / self.mass).sqrt();
        let n = (self.iterations + 1).max((omega / 0.5).ceil() as u32).min(64);
        let dt = 1.0 / n as f32;
        let mut p = goal(self.start);
        // Each other spring rests at the object's start offset from it.
        let rest: Vec<Vec<f32>> = others.iter().map(|(_, o)| p.iter().zip(o).map(|(a, b)| a - b).collect()).collect();
        let mut v: Vec<f32> = goal(self.start + dt).iter().zip(&p).map(|(a, b)| (a - b) / dt).collect();
        let (mut tau, mut prev_goal) = (self.start, p.clone());
        let steps = ((t - self.start) / dt).ceil() as usize;
        for s in 0..steps {
            let next = (self.start + (s + 1) as f32 * dt).min(t);
            let h = next - tau;
            let gl = goal(next);
            let mut acc = vec![0.0f32; dims];
            for i in 0..dims {
                let gv = (gl[i] - prev_goal[i]) / h.max(1e-6);
                let mut f = self.tension * SPRING_K * (gl[i] - p[i]) - self.dampening * self.tension * SPRING_C * (v[i] - gv) - self.drag * DRAG_C * v[i];
                for (k, (l, _)) in others.iter().enumerate() {
                    let c = l.source.copy.as_ref().unwrap();
                    let (o1, o0) = (c.comps_at(next), c.comps_at(tau));
                    let ov = (o1[i] - o0[i]) / h.max(1e-6);
                    f += l.tension * SPRING_K * (o1[i] + rest[k][i] - p[i]) - l.dampening * l.tension * SPRING_C * (v[i] - ov);
                }
                acc[i] = f / self.mass;
            }
            for i in 0..dims {
                v[i] += acc[i] * h;
                p[i] += v[i] * h;
            }
            tau = next;
            prev_goal = gl;
        }
        (0..dims).map(|i| g[i] + (p[i] - g[i]) * self.effect.get(i).copied().unwrap_or(100.0) / 100.0).collect()
    }
}

/// One entry of a List controller; `weight` is a percentage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListItem<T> {
    pub name: String,
    pub weight: f32,
    pub controller: Controller<T>,
}

/// How a parameter's value is produced over time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Controller<T> {
    Constant(T),
    Keys(KeyedTracks),
    Noise(NoiseCtrl),
    List(Vec<ListItem<T>>),
    Expression(Box<crate::expr::ExprCtrl>),
    Spring(Box<SpringCtrl<T>>),
    Audio(Box<crate::audio::AudioCtrl>),
    /// A Ring Array box's position: its place on the ring.
    Ring(Box<RingCtrl>),
}

/// Ring Array: box `index` of `number` on a ring around the centre dummy
/// (the box's parent), lifted along a sine wave of Amplitude and Cycles
/// shifted by Phase (in whole waves).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RingCtrl {
    pub index: u32,
    pub number: u32,
    pub radius: Param<f32>,
    pub amplitude: Param<f32>,
    pub cycles: Param<f32>,
    pub phase: Param<f32>,
}

impl RingCtrl {
    pub fn eval(&self, t: f32) -> Vec<f32> {
        let a = std::f32::consts::TAU * self.index as f32 / self.number.max(1) as f32;
        let r = self.radius.get_at(t);
        let z = self.amplitude.get_at(t) * (self.cycles.get_at(t) * a + std::f32::consts::TAU * self.phase.get_at(t)).sin();
        vec![r * a.cos(), r * a.sin(), z]
    }

    pub fn is_animated(&self) -> bool {
        [&self.radius, &self.amplitude, &self.cycles, &self.phase].iter().any(|p| p.is_animated())
    }
}

impl<T: Animatable> Controller<T> {
    pub fn comps_at(&self, t: f32) -> Vec<f32> {
        match self {
            Controller::Constant(v) => v.comps(),
            Controller::Keys(k) => k.comps_at(t),
            Controller::Noise(n) => n.eval(t, T::dims()),
            Controller::Expression(e) => e.eval(t, T::dims()),
            Controller::Spring(s) => s.eval(t),
            Controller::Audio(a) => a.eval(t, T::dims()),
            Controller::Ring(r) => {
                let mut c = r.eval(t);
                c.resize(T::dims(), 0.0);
                c
            }
            Controller::List(items) => {
                let mut sum = vec![0.0; T::dims()];
                for it in items {
                    for (s, c) in sum.iter_mut().zip(it.controller.comps_at(t)) {
                        *s += it.weight / 100.0 * c;
                    }
                }
                sum
            }
        }
    }

    pub fn is_animated(&self) -> bool {
        match self {
            Controller::Constant(_) => false,
            Controller::Keys(k) => k.has_keys(),
            Controller::Noise(_) | Controller::Expression(_) | Controller::Audio(_) => true,
            Controller::Spring(s) => s.inner.is_animated() || !s.springs.is_empty(),
            Controller::Ring(r) => r.is_animated(),
            Controller::List(items) => items.iter().any(|i| i.controller.is_animated()),
        }
    }

    /// Sets the components at time `t`: keys them under Auto Key, otherwise
    /// edits the key at `t` or offsets the component's keys.
    fn set_comps(&mut self, nc: &[f32], t: f32, animate: bool) {
        let tangents = anim::default_tangents();
        let stepped = T::stepped();
        let tan = if stepped { (TangentType::Step, TangentType::Step) } else { tangents };
        match self {
            Controller::Constant(old) => {
                let oc = old.comps();
                if animate && t.abs() > 1e-4 && oc != nc {
                    let mut k = KeyedTracks::from_rest(T::ty(), oc.clone());
                    for (i, tr) in k.tracks.iter_mut().enumerate() {
                        if oc[i] != nc[i] {
                            tr.set_key(0.0, oc[i], tan);
                            tr.set_key(t, nc[i], tan);
                        }
                    }
                    *self = Controller::Keys(k);
                } else {
                    *old = T::from_comps(nc);
                }
            }
            Controller::Keys(k) => {
                let cur = k.comps_at(t);
                let tan = if k.smooth { (TangentType::Auto, TangentType::Auto) } else { tan };
                // A linked controller keys every component together.
                if k.linked && animate && t.abs() > 1e-4 && cur != nc {
                    for (i, tr) in k.tracks.iter_mut().enumerate() {
                        if tr.keys.is_empty() {
                            tr.set_key(0.0, k.rest[i], tan);
                        }
                        tr.set_key(t, nc[i], tan);
                    }
                    return;
                }
                for i in 0..k.tracks.len() {
                    if cur[i] == nc[i] {
                        continue;
                    }
                    let tr = &mut k.tracks[i];
                    if tr.keys.is_empty() {
                        if animate && t.abs() > 1e-4 {
                            tr.set_key(0.0, k.rest[i], tan);
                            tr.set_key(t, nc[i], tan);
                        } else {
                            k.rest[i] = nc[i];
                        }
                    } else if animate {
                        tr.set_key(t, nc[i], tan);
                    } else if let Some(j) = tr.key_at(t) {
                        tr.keys[j].value = nc[i];
                    } else {
                        tr.offset(nc[i] - cur[i]);
                    }
                }
            }
            // Noise, Expression and Audio output is not settable.
            // A Ring Array box keeps its place on the ring.
            Controller::Noise(_) | Controller::Expression(_) | Controller::Audio(_) | Controller::Ring(_) => {}
            // Moving a spring-driven object moves its original motion.
            Controller::Spring(s) => s.inner.set_comps(nc, t, animate),
            // A List sets its first weighted entry so the sum becomes the value.
            Controller::List(items) => {
                let mut others = vec![0.0; nc.len()];
                for it in items.iter().skip(1) {
                    for (o, c) in others.iter_mut().zip(it.controller.comps_at(t)) {
                        *o += it.weight / 100.0 * c;
                    }
                }
                if let Some(first) = items.first_mut().filter(|f| f.weight != 0.0) {
                    let w = first.weight / 100.0;
                    let target: Vec<f32> = nc.iter().zip(&others).map(|(v, o)| (v - o) / w).collect();
                    first.controller.set_comps(&target, t, animate);
                }
            }
        }
    }

    /// Keys component `only` (or every component) at `t` with its current value.
    fn key_comps(&mut self, t: f32, only: Option<usize>) {
        let tangents = if T::stepped() { (TangentType::Step, TangentType::Step) } else { anim::default_tangents() };
        if let Controller::Constant(v) = self {
            *self = Controller::Keys(KeyedTracks::from_rest(T::ty(), v.comps()));
        }
        match self {
            Controller::Keys(k) => {
                let cur = k.comps_at(t);
                let tangents = if k.smooth { (TangentType::Auto, TangentType::Auto) } else { tangents };
                for (i, (tr, v)) in k.tracks.iter_mut().zip(cur).enumerate() {
                    if only.is_none_or(|o| o == i) {
                        tr.set_key(t, v, tangents);
                    }
                }
            }
            Controller::List(items) => {
                if let Some(first) = items.first_mut() {
                    first.controller.key_comps(t, only);
                }
            }
            Controller::Spring(s) => s.inner.key_comps(t, only),
            Controller::Constant(_) | Controller::Noise(_) | Controller::Expression(_) | Controller::Audio(_) | Controller::Ring(_) => {}
        }
    }
}

/// An animatable parameter, as every spinner is.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Param<T> {
    pub controller: Controller<T>,
}

impl<T: Animatable> Param<T> {
    pub fn new(value: T) -> Self {
        Self { controller: Controller::Constant(value) }
    }

    /// The value at the ambient time (`anim::time()`).
    pub fn get(&self) -> T {
        self.get_at(anim::time())
    }

    pub fn get_at(&self, t: f32) -> T {
        if let Controller::Constant(v) = &self.controller {
            return v.clone();
        }
        if self.controller.is_animated() {
            anim::note_animated_read();
        }
        T::from_comps(&self.controller.comps_at(t))
    }

    /// Sets the value at the ambient time under the ambient Auto Key state.
    pub fn set(&mut self, value: T) {
        // A constant that is not about to be keyed keeps the exact value (the
        // f32 components cannot hold every u32, e.g. smoothing group bit 32).
        if let Controller::Constant(old) = &mut self.controller {
            if !(anim::animate() && anim::time().abs() > 1e-4 && old.comps() != value.comps()) {
                *old = value;
                return;
            }
        }
        let nc = value.comps();
        self.controller.set_comps(&nc, anim::time(), anim::animate());
    }

    pub fn is_animated(&self) -> bool {
        self.controller.is_animated()
    }

    pub fn keys_mut(&mut self) -> Option<&mut KeyedTracks> {
        match &mut self.controller {
            Controller::Keys(k) => Some(k),
            _ => None,
        }
    }

    pub fn keys(&self) -> Option<&KeyedTracks> {
        match &self.controller {
            Controller::Keys(k) => Some(k),
            _ => None,
        }
    }

    /// Set Keys: keys every component at the ambient time.
    pub fn key_now(&mut self) {
        self.controller.key_comps(anim::time(), None);
    }

    /// Key Position X and friends: keys one component at the ambient time.
    pub fn key_comp(&mut self, i: usize) {
        self.controller.key_comps(anim::time(), Some(i));
    }

    /// The value as float components at `t` (noting an animated read).
    pub fn comps_at(&self, t: f32) -> Vec<f32> {
        self.get_at(t).comps()
    }

    /// `set` from float components.
    pub fn set_comps(&mut self, nc: &[f32]) {
        self.controller.set_comps(nc, anim::time(), anim::animate());
    }

    /// Keyed tracks to edit directly (Track View, track bar): a constant
    /// becomes keyed tracks with no keys yet. None for Noise and List.
    pub fn ensure_keys(&mut self) -> Option<&mut KeyedTracks> {
        if let Controller::Constant(v) = &self.controller {
            self.controller = Controller::Keys(KeyedTracks::from_rest(T::ty(), v.comps()));
        }
        self.keys_mut()
    }
}

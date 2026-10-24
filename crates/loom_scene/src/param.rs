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
}

impl KeyedTracks {
    pub fn from_rest(ty: ParamType, rest: Vec<f32>) -> Self {
        Self { ty, tracks: rest.iter().map(|_| FloatTrack::new(Interp::Bezier)).collect(), rest, linked: false }
    }

    pub fn comps_at(&self, t: f32) -> Vec<f32> {
        self.tracks.iter().zip(&self.rest).map(|(tr, r)| tr.eval(t).unwrap_or(*r)).collect()
    }

    pub fn has_keys(&self) -> bool {
        self.tracks.iter().any(|t| !t.keys.is_empty())
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
}

impl Default for NoiseCtrl {
    fn default() -> Self {
        Self::new(0)
    }
}

impl NoiseCtrl {
    /// Defaults: Frequency 0.5, Strength 50, no fractal, no ramps.
    pub fn new(seed: u32) -> Self {
        Self { seed, frequency: 0.5, fractal: false, roughness: 0.0, ramp_in: 0.0, ramp_out: 0.0, end: 100.0, strength: vec![50.0] }
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
                n.clamp(-1.0, 1.0) * strength * 0.5 * ramp
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
}

impl<T: Animatable> Controller<T> {
    pub fn comps_at(&self, t: f32) -> Vec<f32> {
        match self {
            Controller::Constant(v) => v.comps(),
            Controller::Keys(k) => k.comps_at(t),
            Controller::Noise(n) => n.eval(t, T::dims()),
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
            Controller::Noise(_) => true,
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
            // Noise output is not settable.
            Controller::Noise(_) => {}
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
            Controller::Constant(_) | Controller::Noise(_) => {}
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

//! Scene lights in Bevy: each [`LightSample`] (the renderer-independent
//! evaluation of a light proxy in the scene) becomes a Bevy PointLight,
//! SpotLight or DirectionalLight entity, kept in sync incrementally by node
//! id; skylights and Ambient Only lights add up into the AmbientLight.
//!
//! Units: a standard light of multiplier 1 lights a white surface it faces
//! to full white. Bevy is physical (lux at the default exposure), so the
//! multiplier maps through [`LUX_PER_MULTIPLIER`]. Bevy lights always fall
//! off with the inverse square: Inverse Square decay matches the reference
//! from the decay start on; None and Inverse match it at the distance from
//! the light to the scene's centre. Far attenuation End is the light's range.

use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::math::Vec3;
use bevy::pbr::{DirectionalLight, PointLight, SpotLight};
use bevy::transform::components::Transform;
use loom_scene::light::{Emitter, LightSample};

/// Illuminance (lux) that renders like multiplier 1 at Bevy's default
/// exposure (EV100 9.7): Lambert divides by π, the exposure by 1.2 · 2^9.7.
pub const LUX_PER_MULTIPLIER: f32 = std::f32::consts::PI * 1.2 * TWO_POW_EV100;
/// 2^9.7, Bevy's default EV100.
const TWO_POW_EV100: f32 = 831.746_5;
/// What Bevy's default exposure (EV100 9.7) scales light by.
pub const EXPOSURE: f32 = 1.0 / (1.2 * TWO_POW_EV100);
/// AmbientLight brightness (cd/m²) of an ambient multiplier 1.
pub const AMBIENT_PER_MULTIPLIER: f32 = 1.2 * TWO_POW_EV100;

/// Whether the Bevy renderer uses a light parameter (by its rollout name, or
/// a colour's prefix). The Modify panel disables the others.
pub fn supports(param: &str) -> bool {
    !matches!(
        param,
        "Near Use" | "Rectangle" | "Aspect" | "Contrast" | "Soften Diffuse Edge" | "Affect Diffuse" | "Affect Specular" | "Projector" | "Shadow Color" | "Shadow Density" | "Sample Range" | "Use Scene Environment" | "Rays per Sample" | "Ray Bias"
    )
}

/// The kind of Bevy light a sample becomes (a change respawns the entity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Point,
    Spot,
    Directional,
}

pub(crate) fn kind(s: &LightSample) -> Option<Kind> {
    match s.emitter {
        Emitter::Point => Some(Kind::Point),
        Emitter::Spot { falloff, .. } if falloff < 179.0 => Some(Kind::Spot),
        Emitter::Spot { .. } => Some(Kind::Point),
        Emitter::Directional => Some(Kind::Directional),
        Emitter::Ambient => None,
    }
}

/// Linear colour and its brightness (the largest channel; negative lights clamp to 0).
fn split(c: glam::Vec3) -> (Color, f32) {
    let c = c.max(glam::Vec3::ZERO);
    let b = c.max_element();
    if b <= 0.0 {
        return (Color::BLACK, 0.0);
    }
    let n = c / b;
    (Color::linear_rgb(n.x, n.y, n.z), b)
}

/// Luminous power (lumens) of a point or spot light so that it matches the
/// reference at its reference distance; `centre_distance` is the distance
/// from the light to the scene's centre.
pub fn lumens(s: &LightSample, centre_distance: f32) -> f32 {
    let (_, b) = split(s.color);
    let r0 = s.decay_start.max(1e-3);
    let d_ref = if s.decay == 2 { r0 } else { centre_distance.max(1e-2) };
    let decay = if s.decay > 0 && d_ref > r0 { (r0 / d_ref).powi(s.decay as i32) } else { 1.0 };
    let lux = LUX_PER_MULTIPLIER * b * decay;
    4.0 * std::f32::consts::PI * lux * d_ref * d_ref
}

/// Range of a point or spot light: Far End, or far enough that the light
/// reaches past the scene.
pub fn range(s: &LightSample, centre_distance: f32, scene_radius: f32) -> f32 {
    match s.far {
        Some((_, end)) => end.max(1e-2),
        None => ((centre_distance + scene_radius) * 4.0).max(s.decay_start * 4.0).max(10.0),
    }
}

pub(crate) fn transform(s: &LightSample) -> Transform {
    let up = if s.direction.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
    Transform::from_translation(Vec3::from(s.position.to_array())).looking_to(Vec3::from(s.direction.to_array()), up)
}

pub(crate) fn point(s: &LightSample, centre_distance: f32, scene_radius: f32) -> PointLight {
    let (color, _) = split(s.color);
    PointLight {
        color,
        intensity: lumens(s, centre_distance),
        range: range(s, centre_distance, scene_radius),
        radius: s.source_radius,
        shadows_enabled: s.shadows,
        shadow_depth_bias: PointLight::DEFAULT_SHADOW_DEPTH_BIAS * s.map_bias,
        shadow_normal_bias: s.shadow_normal_bias,
        ..Default::default()
    }
}

pub(crate) fn spot(s: &LightSample, centre_distance: f32, scene_radius: f32) -> SpotLight {
    let (color, _) = split(s.color);
    let Emitter::Spot { hotspot, falloff } = s.emitter else { unreachable!("spot sample") };
    let outer = (falloff * 0.5).to_radians().min(std::f32::consts::FRAC_PI_2 - 1e-3);
    SpotLight {
        color,
        intensity: lumens(s, centre_distance),
        range: range(s, centre_distance, scene_radius),
        radius: s.source_radius,
        shadows_enabled: s.shadows,
        shadow_depth_bias: SpotLight::DEFAULT_SHADOW_DEPTH_BIAS * s.map_bias,
        shadow_normal_bias: s.shadow_normal_bias,
        inner_angle: (hotspot * 0.5).to_radians().min(outer),
        outer_angle: outer,
        ..Default::default()
    }
}

pub(crate) fn directional(s: &LightSample) -> DirectionalLight {
    let (color, b) = split(s.color);
    DirectionalLight {
        color,
        illuminance: LUX_PER_MULTIPLIER * b,
        shadows_enabled: s.shadows,
        shadow_depth_bias: DirectionalLight::DEFAULT_SHADOW_DEPTH_BIAS * s.map_bias,
        shadow_normal_bias: s.shadow_normal_bias,
        ..Default::default()
    }
}

/// The ambient colour and brightness of skylights and Ambient Only lights.
pub(crate) fn ambient(samples: &[LightSample]) -> (Color, f32) {
    let sum: glam::Vec3 = samples.iter().filter(|s| s.emitter == Emitter::Ambient).map(|s| s.color).sum();
    let (color, b) = split(sum);
    (color, AMBIENT_PER_MULTIPLIER * b)
}

/// One synced scene light.
pub(crate) struct Synced {
    pub entity: Entity,
    pub kind: Kind,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LightSample {
        let mut s = loom_scene::light::default_lights(1, glam::Mat4::IDENTITY)[0].clone();
        s.emitter = Emitter::Point;
        s
    }

    #[test]
    fn inverse_square_matches_the_reference_past_the_decay_start() {
        let mut s = sample();
        s.decay = 2;
        s.decay_start = 10.0;
        // Bevy: E(d) = lm / (4π d²); the reference: K (r0 / d)².
        let lm = lumens(&s, 500.0);
        let e = |d: f32| lm / (4.0 * std::f32::consts::PI * d * d);
        for d in [10.0, 25.0, 80.0] {
            let want = LUX_PER_MULTIPLIER * s.falloff(d, 1.0);
            assert!((e(d) - want).abs() / want < 1e-4, "{d}: {} vs {want}", e(d));
        }
    }

    #[test]
    fn no_decay_matches_at_the_scene_centre() {
        let s = sample();
        let lm = lumens(&s, 40.0);
        let e = lm / (4.0 * std::f32::consts::PI * 40.0 * 40.0);
        assert!((e - LUX_PER_MULTIPLIER).abs() / LUX_PER_MULTIPLIER < 1e-4);
    }

    #[test]
    fn renderer_ignores_what_it_cannot_draw() {
        assert!(supports("Multiplier") && supports("Hotspot") && supports("Far Use") && supports("Map Bias"));
        assert!(!supports("Rectangle") && !supports("Contrast") && !supports("Near Use"));
    }
}

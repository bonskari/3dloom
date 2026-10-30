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
/// AmbientLight brightness (cd/m²) of an ambient multiplier 1.
pub const AMBIENT_PER_MULTIPLIER: f32 = 1.2 * TWO_POW_EV100;

/// Whether the Bevy renderer uses a light parameter (by its rollout name, or
/// a colour's prefix). The Modify panel disables the others.
pub fn supports(param: &str) -> bool {
    !matches!(
        param,
        "Near Use" | "Rectangle" | "Aspect" | "Contrast" | "Soften Diffuse Edge" | "Affect Diffuse" | "Affect Specular" | "Projector" | "Sample Range" | "Use Scene Environment" | "Rays per Sample" | "Ray Bias" | "Render Shape"
            | "Shadow Map On" | "Atmosphere Shadows" | "Atmosphere Opacity" | "Atmosphere Color Amount" | "2-Sided Shadows" | "Max Quadtree Depth" | "Adv RT Mode"
            | "Shadow Integrity" | "Shadow Quality" | "Shadow Spread" | "Shadow Bias" | "Jitter Amount"
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

/// Shadow depth bias: Absolute Map Bias is in scene units; otherwise Map
/// Bias scales the renderer's default (1 = the default bias).
fn depth_bias(s: &LightSample, default: f32) -> f32 {
    if s.absolute_bias { s.map_bias.max(0.0) } else { default * s.map_bias }
}

/// A light whose shadows let some light through (shadow colour, density)
/// renders as two lights: the part shadows stop, with shadows, and the part
/// they let through, without. Lit points get both (the whole light);
/// shadowed points only the second. None when shadows stop all of it.
pub fn shadow_split(s: &LightSample) -> Option<(LightSample, LightSample)> {
    let t = s.shadow_transmission.clamp(glam::Vec3::ZERO, glam::Vec3::ONE);
    if !s.shadows || t.max_element() <= 1e-4 {
        return None;
    }
    let mut stopped = s.clone();
    stopped.color = s.color * (glam::Vec3::ONE - t);
    let mut through = s.clone();
    through.color = s.color * t;
    through.shadows = false;
    Some((stopped, through))
}

pub(crate) fn point(s: &LightSample, centre_distance: f32, scene_radius: f32) -> PointLight {
    let (color, _) = split(s.color);
    PointLight {
        color,
        intensity: lumens(s, centre_distance),
        range: range(s, centre_distance, scene_radius),
        radius: s.source_radius,
        shadows_enabled: s.shadows,
        // Soft shadows from the light's size (Area Shadows, a photometric shape, Source Radius).
        soft_shadows_enabled: s.source_radius > 0.0,
        shadow_depth_bias: depth_bias(s, PointLight::DEFAULT_SHADOW_DEPTH_BIAS),
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
        soft_shadows_enabled: s.source_radius > 0.0,
        shadow_depth_bias: depth_bias(s, SpotLight::DEFAULT_SHADOW_DEPTH_BIAS),
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
        // A directional light's softness: the angle a source of the light's
        // radius spans seen from its target distance.
        soft_shadow_size: (s.source_radius > 0.0).then(|| 2.0 * (s.source_radius / s.target_distance.max(1e-3)).atan()),
        shadow_depth_bias: depth_bias(s, DirectionalLight::DEFAULT_SHADOW_DEPTH_BIAS),
        shadow_normal_bias: s.shadow_normal_bias,
        ..Default::default()
    }
}

/// Moves a light to point `d` (on the unit disk) of its Source Radius disk,
/// the disk facing the scene: a spot or directional light across its beam,
/// an omni across the line to `centre`. A directional light keeps aiming
/// at its target, so its rays turn by up to Source Radius / target distance.
pub(crate) fn move_over_source(s: &mut LightSample, d: [f32; 2], centre: Vec3) {
    let axis = match s.emitter {
        Emitter::Point => (centre - Vec3::from(s.position.to_array())).normalize_or(Vec3::NEG_Z),
        _ => Vec3::from(s.direction.to_array()).normalize_or(Vec3::NEG_Z),
    };
    let u = axis.any_orthonormal_vector();
    let v = axis.cross(u);
    let offset = (u * d[0] + v * d[1]) * s.source_radius;
    let target = s.position + s.direction * s.target_distance;
    s.position += glam::Vec3::from(offset.to_array());
    if s.emitter == Emitter::Directional {
        s.direction = (target - s.position).normalize_or(s.direction);
    }
}

/// A point light in a world `scale` times the scene's size: the same light
/// at every scaled point (power by scale², range, radius and bias by scale).
pub(crate) fn scaled_point(mut l: PointLight, scale: f32) -> PointLight {
    l.intensity *= scale * scale;
    l.range *= scale;
    l.radius *= scale;
    l.shadow_depth_bias *= scale;
    l
}

pub(crate) fn scaled_spot(mut l: SpotLight, scale: f32) -> SpotLight {
    l.intensity *= scale * scale;
    l.range *= scale;
    l.radius *= scale;
    l.shadow_depth_bias *= scale;
    l
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
    /// The unshadowed part of a light whose shadows let light through.
    pub fill: Option<Entity>,
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
    fn shadow_colour_and_density_split_the_light() {
        let mut s = sample();
        s.shadows = true;
        s.color = glam::Vec3::splat(2.0);
        s.shadow_transmission = glam::Vec3::ZERO;
        assert!(shadow_split(&s).is_none(), "black shadows at density 1 stop the whole light");
        s.shadow_transmission = glam::Vec3::new(0.5, 0.0, 0.0);
        let (stopped, through) = shadow_split(&s).unwrap();
        assert_eq!(stopped.color + through.color, s.color, "lit points get the whole light");
        assert_eq!(through.color, glam::Vec3::new(1.0, 0.0, 0.0), "shadowed points get the transmitted part");
        assert!(stopped.shadows && !through.shadows);
        s.absolute_bias = true;
        s.map_bias = 0.3;
        assert_eq!(depth_bias(&s, 0.02), 0.3);
        s.absolute_bias = false;
        assert!((depth_bias(&s, 0.02) - 0.006).abs() < 1e-6);
    }

    #[test]
    fn renderer_ignores_what_it_cannot_draw() {
        assert!(supports("Multiplier") && supports("Hotspot") && supports("Far Use") && supports("Map Bias"));
        assert!(!supports("Rectangle") && !supports("Contrast") && !supports("Near Use"));
    }
}

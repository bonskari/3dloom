//! Keyed, noise and list controllers on `Param` (phase 8).

use glam::Vec3;
use loom_scene::anim::{self, FloatTrack, Interp, Key, Ort, TangentType};
use loom_scene::param::{Controller, ListItem, NoiseCtrl};
use loom_scene::Param;

fn track(interp: Interp, keys: &[(f32, f32)], tangent: TangentType) -> FloatTrack {
    let mut t = FloatTrack::new(interp);
    for (time, v) in keys {
        t.set_key(*time, *v, (tangent, tangent));
    }
    t
}

#[test]
fn bezier_auto_passes_through_keys_and_is_smooth() {
    let t = track(Interp::Bezier, &[(0.0, 0.0), (10.0, 10.0), (20.0, 0.0)], TangentType::Auto);
    assert!((t.eval(0.0).unwrap()).abs() < 1e-5);
    assert!((t.eval(10.0).unwrap() - 10.0).abs() < 1e-5);
    assert!((t.eval(20.0).unwrap()).abs() < 1e-5);
    // Auto is flat at the peak: never overshoots it, and it is continuous across the key.
    for i in 0..200 {
        let v = t.eval(i as f32 * 0.1).unwrap();
        assert!(v <= 10.0 + 1e-4, "overshoot {v} at {}", i as f32 * 0.1);
    }
    let a = t.eval(9.99).unwrap();
    let b = t.eval(10.01).unwrap();
    assert!((a - b).abs() < 0.01);
    // Two-key Auto eases in and out: slow at the ends, half way in the middle.
    let e = track(Interp::Bezier, &[(0.0, 0.0), (10.0, 10.0)], TangentType::Auto);
    assert!((e.eval(5.0).unwrap() - 5.0).abs() < 1e-4);
    assert!(e.eval(1.0).unwrap() < 1.0, "ease in: {}", e.eval(1.0).unwrap());
}

#[test]
fn step_and_linear_tangents() {
    let s = track(Interp::Bezier, &[(0.0, 0.0), (10.0, 10.0)], TangentType::Step);
    assert_eq!(s.eval(9.9).unwrap(), 0.0);
    assert_eq!(s.eval(10.0).unwrap(), 10.0);
    let l = track(Interp::Bezier, &[(0.0, 0.0), (10.0, 10.0), (20.0, 30.0)], TangentType::Linear);
    assert!((l.eval(5.0).unwrap() - 5.0).abs() < 1e-4);
    assert!((l.eval(15.0).unwrap() - 20.0).abs() < 1e-4);
    let lin = track(Interp::Linear, &[(0.0, 0.0), (10.0, 10.0)], TangentType::Auto);
    assert!((lin.eval(2.5).unwrap() - 2.5).abs() < 1e-5);
    // Custom slopes are used as given.
    let mut c = track(Interp::Bezier, &[(0.0, 0.0), (10.0, 0.0)], TangentType::Custom);
    c.keys[0].out_tan = 1.0;
    c.keys[1].in_tan = 1.0;
    assert!(c.eval(0.5).unwrap() > 0.3, "{}", c.eval(0.5).unwrap());
    // Fast and Slow differ from each other.
    let f = track(Interp::Bezier, &[(0.0, 0.0), (10.0, 10.0), (20.0, 30.0)], TangentType::Fast);
    let sl = track(Interp::Bezier, &[(0.0, 0.0), (10.0, 10.0), (20.0, 30.0)], TangentType::Slow);
    assert!((f.eval(9.0).unwrap() - sl.eval(9.0).unwrap()).abs() > 0.05);
}

#[test]
fn tcb_passes_through_keys() {
    let mut t = track(Interp::Tcb, &[(0.0, 0.0), (10.0, 10.0), (20.0, 0.0), (30.0, 5.0)], TangentType::Auto);
    for (time, v) in [(0.0, 0.0), (10.0, 10.0), (20.0, 0.0), (30.0, 5.0)] {
        assert!((t.eval(time).unwrap() - v).abs() < 1e-4);
    }
    let mid = t.eval(15.0).unwrap();
    t.keys[2].tension = 50.0;
    assert!((t.eval(15.0).unwrap() - mid).abs() > 1e-3, "tension changes the curve");
    t.keys[1].ease_from = 50.0;
    assert!((t.eval(10.0).unwrap() - 10.0).abs() < 1e-4);
}

#[test]
fn out_of_range_types_extend_the_curve() {
    let base = track(Interp::Linear, &[(0.0, 0.0), (10.0, 10.0)], TangentType::Linear);
    let at = |before: Ort, after: Ort, time: f32| {
        let mut t = base.clone();
        t.before = before;
        t.after = after;
        t.eval(time).unwrap()
    };
    assert!((at(Ort::Constant, Ort::Constant, 25.0) - 10.0).abs() < 1e-4);
    assert!((at(Ort::Constant, Ort::Constant, -5.0) - 0.0).abs() < 1e-4);
    assert!((at(Ort::Cycle, Ort::Cycle, 13.0) - 3.0).abs() < 1e-4);
    assert!((at(Ort::Cycle, Ort::Cycle, -3.0) - 7.0).abs() < 1e-4);
    assert!((at(Ort::Loop, Ort::Loop, 13.0) - 3.0).abs() < 1e-4);
    assert!((at(Ort::PingPong, Ort::PingPong, 13.0) - 7.0).abs() < 1e-4);
    assert!((at(Ort::PingPong, Ort::PingPong, -3.0) - 3.0).abs() < 1e-4);
    assert!((at(Ort::Linear, Ort::Linear, 15.0) - 15.0).abs() < 1e-4);
    assert!((at(Ort::Linear, Ort::Linear, -5.0) + 5.0).abs() < 1e-4);
    assert!((at(Ort::RelativeRepeat, Ort::RelativeRepeat, 13.0) - 13.0).abs() < 1e-4);
    assert!((at(Ort::RelativeRepeat, Ort::RelativeRepeat, -3.0) + 3.0).abs() < 1e-4);
    // A single key holds whatever the type.
    for ort in [Ort::Cycle, Ort::Loop, Ort::PingPong, Ort::Linear, Ort::RelativeRepeat] {
        let mut one = track(Interp::Bezier, &[(5.0, 2.0)], TangentType::Auto);
        one.before = ort;
        one.after = ort;
        assert_eq!(one.eval(-20.0), Some(2.0));
        assert_eq!(one.eval(40.0), Some(2.0));
    }
    assert_eq!(FloatTrack::new(Interp::Bezier).eval(1.0), None);
}

#[test]
fn auto_key_creates_keys_and_off_offsets() {
    let mut p = Param::new(Vec3::new(1.0, 2.0, 3.0));
    // Auto Key at frame 0 changes the value without a key.
    anim::with(0.0, true, || p.set(Vec3::new(1.0, 2.0, 4.0)));
    assert!(!p.is_animated());
    anim::with(10.0, true, || p.set(Vec3::new(5.0, 2.0, 4.0)));
    assert!(p.is_animated());
    assert!((p.get_at(0.0) - Vec3::new(1.0, 2.0, 4.0)).length() < 1e-4);
    assert!((p.get_at(10.0) - Vec3::new(5.0, 2.0, 4.0)).length() < 1e-4);
    assert!((p.get_at(5.0).x - 3.0).abs() < 1e-3, "{:?}", p.get_at(5.0));
    // Only X was keyed.
    let k = p.keys_mut().unwrap();
    assert_eq!(k.tracks[0].keys.len(), 2);
    assert!(k.tracks[1].keys.is_empty() && k.tracks[2].keys.is_empty());
    // Auto Key off, at a key: edits that key.
    anim::with(10.0, false, || p.set(Vec3::new(7.0, 2.0, 4.0)));
    assert!((p.get_at(10.0).x - 7.0).abs() < 1e-4);
    assert!((p.get_at(0.0).x - 1.0).abs() < 1e-4);
    // Auto Key off, between keys: offsets all keys of the changed component.
    anim::with(5.0, false, || {
        let now = p.get();
        p.set(now + Vec3::new(2.0, 0.0, 1.0));
    });
    assert!((p.get_at(0.0).x - 3.0).abs() < 1e-4);
    assert!((p.get_at(10.0).x - 9.0).abs() < 1e-4);
    assert!((p.get_at(10.0).z - 5.0).abs() < 1e-4, "an unkeyed component just changes");
    // get() reads the ambient time.
    anim::with(10.0, false, || assert!((p.get().x - 9.0).abs() < 1e-4));
    // Booleans step.
    let mut b = Param::new(false);
    anim::with(10.0, true, || b.set(true));
    assert!(!b.get_at(9.0));
    assert!(b.get_at(10.0));
    // Set Keys keys every component at the ambient time.
    let mut q = Param::new(2.0f32);
    anim::with(4.0, false, || q.key_now());
    assert_eq!(q.keys_mut().unwrap().tracks[0].keys.len(), 1);
}

#[test]
fn noise_is_deterministic_and_bounded() {
    let mut n = NoiseCtrl::new(1);
    n.strength = vec![10.0];
    let p: Param<f32> = Param { controller: Controller::Noise(n.clone()) };
    let q: Param<f32> = Param { controller: Controller::Noise(n) };
    let mut distinct = 0;
    let mut last = p.get_at(0.0);
    for i in 0..100 {
        let v = p.get_at(i as f32);
        assert_eq!(v, q.get_at(i as f32));
        assert!(v.abs() <= 5.0 + 1e-4, "{v}");
        if (v - last).abs() > 1e-3 {
            distinct += 1;
        }
        last = v;
    }
    assert!(distinct > 50, "noise moves: {distinct}");
}

#[test]
fn list_sums_weighted_controllers() {
    let mut keyed = Param::new(0.0f32);
    anim::with(10.0, true, || keyed.set(10.0));
    let p: Param<f32> = Param {
        controller: Controller::List(vec![
            ListItem { name: "Bezier Float".into(), weight: 100.0, controller: keyed.controller.clone() },
            ListItem { name: "Bezier Float".into(), weight: 50.0, controller: Controller::Constant(4.0) },
        ]),
    };
    assert!((p.get_at(10.0) - 12.0).abs() < 1e-4);
    assert!(p.is_animated());
}

#[test]
fn old_constant_params_deserialise() {
    let p: Param<f32> = serde_json::from_str(r#"{"controller":{"Constant":5.0}}"#).unwrap();
    assert_eq!(p.get(), 5.0);
    let mut k = Param::new(1.0f32);
    anim::with(10.0, true, || k.set(3.0));
    let back: Param<f32> = serde_json::from_str(&serde_json::to_string(&k).unwrap()).unwrap();
    assert_eq!(back, k);
    let _ = Key::default();
}

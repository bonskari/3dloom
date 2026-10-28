use super::*;
use std::time::{Duration, Instant};

fn same(actual: &crate::Evaluated, expected: &crate::Evaluated) {
    assert_eq!(actual.warnings, expected.warnings);
    let (a, b) = (&actual.value, &expected.value);
    assert_eq!(a.mesh, b.mesh);
    assert_eq!(a.selection, b.selection);
    assert_eq!(a.shape_selection, b.shape_selection);
    assert_eq!(a.warnings, b.warnings);
    assert_eq!(a.patch_steps, b.patch_steps);
    assert_eq!(format!("{:?}", a.shape), format!("{:?}", b.shape));
    assert_eq!(format!("{:?}", a.patch), format!("{:?}", b.patch));
    for (a, b) in a.mesh.vertices().iter().zip(b.mesh.vertices()) {
        assert_eq!(a.position.to_array().map(f32::to_bits), b.position.to_array().map(f32::to_bits));
        assert_eq!(a.color.map(f32::to_bits), b.color.map(f32::to_bits));
        assert_eq!(a.illum.map(f32::to_bits), b.illum.map(f32::to_bits));
        assert_eq!(a.alpha.to_bits(), b.alpha.to_bits());
    }
    for (channel, a) in a.mesh.maps() {
        let b = b.mesh.map(*channel).unwrap();
        for (a, b) in a.verts.iter().zip(&b.verts) {
            assert_eq!(a.to_array().map(f32::to_bits), b.to_array().map(f32::to_bits));
        }
    }
}

#[test]
fn actual_parallel_skin_input_317() {
    let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(std::env::var("LOOM_PERF_SCENE").unwrap()).unwrap()).unwrap();
    let baseline: Scene = serde_json::from_value(value["scene"].clone()).unwrap();
    drop(value);
    let candidate = baseline.clone();
    let serial = baseline.clone();
    let ids: Vec<_> = candidate.nodes().filter(|n| n.modifiers.iter().any(|m| matches!(m.modifier, crate::Modifier::Skin(_)))).map(|n| n.id).collect();
    assert_eq!(ids.len(), 5);
    let original: Vec<_> = ids.iter().map(|id| {
        let n = candidate.node(*id).unwrap();
        (n.cage_mesh().into_owned(), serde_json::to_vec(&n.modifiers).unwrap())
    }).collect();
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut old_total = Duration::ZERO;
    let mut new_total = Duration::ZERO;
    let mut old_prepare = Duration::ZERO;
    let mut new_prepare = Duration::ZERO;
    let mut poses = 0;
    for (index, frame) in (0..=157).map(|i| i as f32).chain([39.25]).chain((0..=157).map(|i| i as f32 + 0.5)).enumerate() {
        baseline.set_time(frame); candidate.set_time(frame); serial.set_time(frame);
        let mut old = || {
            let t = Instant::now();
            let result = baseline.evaluated_batch_baseline(&ids, |jobs| {
                old_prepare += t.elapsed();
                assert_eq!(jobs.len(), 5);
                pool.scope(|scope| { for job in jobs { scope.spawn(async move { job.run() }); } })
            });
            old_total += t.elapsed(); result
        };
        let mut new = || {
            let t = Instant::now();
            let result = candidate.evaluated_batch(&ids, |jobs| {
                new_prepare += t.elapsed();
                assert_eq!(jobs.len(), 5);
                pool.scope(|scope| { for job in jobs { scope.spawn(async move { job.run() }); } })
            });
            new_total += t.elapsed(); result
        };
        let (a, b) = if index % 2 == 0 { (old(), new()) } else { let b = new(); (old(), b) };
        for (i, ((a, b), id)) in a.iter().zip(&b).zip(&ids).enumerate() {
            same(b, a); same(b, &serial.evaluated(*id, None));
            assert_eq!(candidate.node(*id).unwrap().cage_mesh().as_ref(), &original[i].0);
        }
        poses += 1;
    }
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(serde_json::to_vec(&candidate.node(*id).unwrap().modifiers).unwrap(), original[i].1);
    }
    assert_eq!(poses, 317);
    let ms = |d: Duration| d.as_secs_f64() * 1000.0 / poses as f64;
    eprintln!("PAIRED poses={poses} skins=5 baseline_ms={:.6} candidate_ms={:.6} baseline_prepare_ms={:.6} candidate_prepare_ms={:.6}", ms(old_total), ms(new_total), ms(old_prepare), ms(new_prepare));
}

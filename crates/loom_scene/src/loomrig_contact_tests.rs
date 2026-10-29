use crate::loomrig_contact::{ContactKey, ContactReference, ContactSpace, ContactTrack};
use glam::{Quat, Vec3};
use std::collections::BTreeMap;

fn reference(space: ContactSpace) -> ContactReference {
    ContactReference {
        space,
        position: Vec3::new(1., 2., 3.),
        rotation: Quat::IDENTITY,
        pole_position: Vec3::Z,
    }
}
fn key(time: f32, reference: u64, blend: f32) -> ContactKey {
    ContactKey {
        time,
        reference,
        blend,
    }
}
fn track() -> ContactTrack {
    ContactTrack::new(
        BTreeMap::from([
            (7, reference(ContactSpace::World)),
            (9, reference(ContactSpace::Body)),
        ]),
        vec![key(10., 7, 1.), key(20., 7, 0.75), key(30., 9, 0.)],
    )
    .unwrap()
}

#[test]
fn contact_sampling_preserves_references_and_registers_time_dependence() {
    let empty = ContactTrack::default();
    let reads = crate::anim::animated_reads();
    assert!(empty.active_key(100.).unwrap().is_none());
    assert_eq!(crate::anim::animated_reads(), reads);
    let track = track();
    for (time, expected) in [
        (0., None),
        (10., Some(10.)),
        (19.5, Some(10.)),
        (30., Some(30.)),
        (20., Some(20.)),
        (-2., None),
        (100., Some(30.)),
    ] {
        let reads = crate::anim::animated_reads();
        assert_eq!(track.active_key(time).unwrap().map(|k| k.time), expected);
        assert!(crate::anim::animated_reads() > reads);
    }
    assert!(track.active_key(f32::NAN).is_err());
    assert_eq!(track.keys()[0].reference, track.keys()[1].reference);
    assert_eq!(
        serde_json::from_str::<ContactTrack>(&serde_json::to_string(&track).unwrap()).unwrap(),
        track
    );
}

#[test]
fn contact_edits_and_collisions_are_atomic_and_keep_shared_references() {
    let mut track = track();
    let original = track.clone();
    for result in [
        track.move_key(10., 20.),
        track.move_key(10., f32::INFINITY),
        track.insert_key(key(15., 99, 1.)),
        track.update_key(key(20., 7, 2.)),
    ] {
        assert!(result.is_err());
    }
    assert_eq!(track, original);
    let mut edited = reference(ContactSpace::World);
    edited.position.x = 4.;
    track.update_reference(7, edited.clone()).unwrap();
    assert_eq!(track.references()[&track.keys()[0].reference], edited);
    assert_eq!(track.references()[&track.keys()[1].reference], edited);
    track.move_key(20., 35.).unwrap();
    assert_eq!(
        track.keys().iter().map(|k| k.time).collect::<Vec<_>>(),
        vec![10., 30., 35.]
    );
    track.remove_key(10.).unwrap();
    assert!(track.references().contains_key(&7));
    track.remove_key(35.).unwrap();
    assert!(!track.references().contains_key(&7));
    assert!(track.references().contains_key(&9));
    track.update_key(key(30., 9, 0.5)).unwrap();
    assert_eq!(track.keys()[0].blend, 0.5);
    track.remove_key(30.).unwrap();
    assert!(track.is_empty());
}

#[test]
fn contact_invalid_data_is_rejected_by_construction_and_deserialization() {
    let refs = BTreeMap::from([(1, reference(ContactSpace::World))]);
    for keys in [
        vec![key(f32::NAN, 1, 1.)],
        vec![key(0., 1, f32::NAN)],
        vec![key(0., 1, -0.1)],
        vec![key(0., 2, 1.)],
        vec![key(0., 1, 1.), key(-0., 1, 0.)],
        vec![key(2., 1, 1.), key(1., 1, 1.)],
    ] {
        assert!(ContactTrack::new(refs.clone(), keys).is_err());
    }
    for rotation in [
        Quat::from_xyzw(0., 0., 0., 0.),
        Quat::from_xyzw(0., 0., 0., 2.),
        Quat::from_xyzw(f32::NAN, 0., 0., 1.),
    ] {
        let mut bad = reference(ContactSpace::Body);
        bad.rotation = rotation;
        assert!(ContactTrack::new(BTreeMap::from([(1, bad)]), vec![key(0., 1, 1.)]).is_err());
    }
    let mut bad = reference(ContactSpace::World);
    bad.position.z = f32::INFINITY;
    assert!(ContactTrack::new(BTreeMap::from([(1, bad)]), vec![key(0., 1, 1.)]).is_err());
    let mut value = serde_json::to_value(track()).unwrap();
    value["keys"][1]["time"] = value["keys"][0]["time"].clone();
    assert!(serde_json::from_value::<ContactTrack>(value).is_err());
    let mut value = serde_json::to_value(track()).unwrap();
    value["keys"][0]["reference"] = serde_json::json!(99);
    assert!(serde_json::from_value::<ContactTrack>(value).is_err());
}

#[test]
fn contact_limb_legacy_serialization_omits_empty_track_and_roundtrips_authored_data() {
    use crate::loomrig::{LimbMapping, RigLimb};
    let mut limb = RigLimb {
        mapping: LimbMapping {
            name: "arm".into(),
            start: crate::NodeId(1),
            middle: crate::NodeId(2),
            end: crate::NodeId(3),
        },
        goal: crate::NodeId(4),
        pole: crate::NodeId(5),
        goal_rest: Vec3::ZERO,
        pole_rest: Vec3::Y,
        solver: crate::ik::IkSolver::Limb,
        blend: crate::Param::new(0.),
        pose_offsets: vec![],
        contacts: ContactTrack::default(),
    };
    let legacy = serde_json::to_value(&limb).unwrap();
    assert!(legacy.get("contacts").is_none());
    assert_eq!(
        serde_json::from_value::<RigLimb>(legacy.clone()).unwrap(),
        limb
    );
    limb.contacts = track();
    let authored = serde_json::to_value(&limb).unwrap();
    assert!(authored.get("contacts").is_some());
    assert_eq!(serde_json::from_value::<RigLimb>(authored).unwrap(), limb);
    limb.contacts = ContactTrack::default();
    assert_eq!(serde_json::to_value(&limb).unwrap(), legacy);
}

#[test]
fn contact_reference_insertion_and_invalid_edits_preserve_authored_data() {
    let mut track = ContactTrack::default();
    track
        .insert_reference(7, reference(ContactSpace::World))
        .unwrap();
    assert!(!track.is_empty());
    let original = track.clone();
    assert!(track
        .insert_reference(7, reference(ContactSpace::Body))
        .is_err());
    assert!(track
        .update_reference(99, reference(ContactSpace::Body))
        .is_err());
    let mut bad = reference(ContactSpace::World);
    bad.position.x = f32::NAN;
    assert!(track.update_reference(7, bad.clone()).is_err());
    assert!(track.insert_reference(8, bad).is_err());
    assert_eq!(track, original);
    track.insert_key(key(2., 7, 1.)).unwrap();
    track.insert_key(key(1., 7, 0.)).unwrap();
    assert_eq!(
        track.keys().iter().map(|k| k.time).collect::<Vec<_>>(),
        vec![1., 2.]
    );
    let original = track.clone();
    assert!(track.insert_key(key(1., 7, 0.5)).is_err());
    assert!(track.update_key(key(3., 7, 0.)).is_err());
    assert!(track.move_key(3., 4.).is_err());
    assert!(track.remove_key(f32::NAN).is_err());
    track.move_key(1., 1.).unwrap();
    assert_eq!(track, original);
}

#[test]
fn contact_sampling_resolves_affine_body_frames_and_rejects_singular_frames() {
    use glam::Mat4;
    let track = track();
    let frame = Mat4::from_cols_array(&[
        -2., 0., 0., 0., 0.4, 3., 0., 0., 0., 0.3, 1., 0., 4., 5., 6., 1.,
    ]);
    let sample = track.sample(30., frame).unwrap().unwrap();
    assert!(sample
        .position
        .abs_diff_eq(frame.transform_point3(Vec3::new(1., 2., 3.)), 1e-5));
    assert!(sample.rotation.is_finite());
    assert!((sample.rotation.length_squared() - 1.).abs() < 1e-5);
    assert!(track
        .sample(30., Mat4::from_scale(Vec3::new(0., 1., 1.)))
        .is_err());
    assert!(track
        .sample(30., Mat4::from_cols_array(&[f32::NAN; 16]))
        .is_err());
    // A world-space contact does not depend on an unrelated singular body frame.
    assert!(track.sample(10., Mat4::ZERO).unwrap().is_some());
    let mut bad = reference(ContactSpace::World);
    bad.pole_position.x = f32::INFINITY;
    assert!(ContactTrack::new([(7, bad)].into(), vec![key(0., 7, 1.)]).is_err());
}

#[test]
fn contact_interpolation_handles_all_valid_finite_key_times() {
    let mut a = reference(ContactSpace::World);
    let mut b = a.clone();
    a.position.x = -f32::MAX;
    b.position.x = f32::MAX;
    let track = ContactTrack::new(
        [(1, a), (2, b)].into(),
        vec![key(-f32::MAX, 1, 0.), key(f32::MAX, 2, 1.)],
    )
    .unwrap();
    let sample = track.sample(0., glam::Mat4::IDENTITY).unwrap().unwrap();
    assert_eq!(sample.blend, 0.5);
    assert!(sample.position.is_finite());
    assert_eq!(sample.position.x, 0.);
}

#[test]
fn contact_capture_roundtrips_world_pose_under_affine_body_frames() {
    use glam::Mat4;
    let rotation = Quat::from_euler(glam::EulerRot::XYZ, 0.3, -0.5, 0.7);
    let end = Mat4::from_scale_rotation_translation(
        Vec3::new(2., 3., 1.),
        rotation,
        Vec3::new(4., 5., 6.),
    );
    let pole = Vec3::new(-4., 12., 15.);
    for sign in [1., -1.] {
        let frame = Mat4::from_scale_rotation_translation(
            Vec3::new(sign * 2., 3., 1.),
            Quat::from_rotation_z(0.4),
            Vec3::new(2., 1., 3.),
        ) * Mat4::from_rotation_y(0.3);
        for space in [ContactSpace::World, ContactSpace::Body] {
            let captured = ContactReference::capture(space, frame, end, pole).unwrap();
            let track = ContactTrack::new([(7, captured)].into(), vec![key(0., 7, 1.)]).unwrap();
            let sample = track.sample(0., frame).unwrap().unwrap();
            assert!(sample.position.abs_diff_eq(end.w_axis.truncate(), 1e-4));
            assert!(sample.pole_position.abs_diff_eq(pole, 1e-4));
            assert!(
                sample.rotation.abs_diff_eq(rotation, 1e-4)
                    || sample.rotation.abs_diff_eq(-rotation, 1e-4),
                "{sign}: {:?} != {rotation:?}",
                sample.rotation
            );
        }
    }
    assert!(ContactReference::capture(ContactSpace::Body, Mat4::ZERO, end, pole).is_err());
}

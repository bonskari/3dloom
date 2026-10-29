use glam::{Mat4, Quat, Vec3};
use loom_scene::ik::{IkBone, IkChain, IkSolver};
use loom_scene::NodeId;

#[test]
fn contact_limb_solution_keeps_roll_continuous_without_changing_analytic_joint_positions() {
    let bones = vec![
        IkBone {
            id: NodeId(1),
            parent_frame: Mat4::IDENTITY,
            position: Vec3::ZERO,
            rotation: Quat::from_rotation_z(0.4) * Quat::from_rotation_x(0.8),
            tip: Vec3::X * 10.,
        },
        IkBone {
            id: NodeId(2),
            parent_frame: Mat4::IDENTITY,
            position: Vec3::X * 10.,
            rotation: Quat::from_rotation_z(-0.8) * Quat::from_rotation_x(-0.3),
            tip: Vec3::X * 10.,
        },
    ];
    let chain = IkChain {
        id: 1,
        name: "contact".into(),
        solver: IkSolver::Limb,
        start: NodeId(1),
        end: NodeId(3),
        goal: NodeId(4),
        enabled: true,
        swivel_deg: 0.,
        threshold: 0.0001,
        iterations: 100,
        spline: None,
        ik_for_fk: false,
    };
    let joint_positions = |q0: Quat, q1: Quat| {
        let a = Mat4::from_rotation_translation(q0, Vec3::ZERO);
        let b = a * Mat4::from_rotation_translation(q1, Vec3::X * 10.);
        [b.w_axis.truncate(), b.transform_point3(Vec3::X * 10.)]
    };
    let original = joint_positions(bones[0].rotation, bones[1].rotation);
    for delta in [Vec3::new(0.001, 0.002, 0.001), Vec3::new(-1., 2., 0.5)] {
        let goal = original[1] + delta;
        let legacy = loom_scene::ik::solve(&chain, &bones, goal, None);
        let contact = loom_scene::ik::solve_preserving_roll(&chain, &bones, goal);
        let a = joint_positions(legacy[0].1, legacy[1].1);
        let b = joint_positions(contact[0].1, contact[1].1);
        for (a, b) in a.into_iter().zip(b) {
            assert!(a.distance(b) < 0.001, "{a:?} != {b:?}");
        }
        if delta.length() < 0.01 {
            assert!(contact[0].1.angle_between(bones[0].rotation) < 0.01);
            assert!(contact[1].1.angle_between(bones[1].rotation) < 0.01);
        }
    }
}

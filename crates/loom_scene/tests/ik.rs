//! IK solvers: HI (CCD + swivel), IK Limb (two-bone analytic), Spline IK.

use glam::{Mat4, Quat, Vec3};
use loom_scene::ik::{solve, IkBone, IkChain, IkSolver};
use loom_scene::NodeId;

fn bones(lengths: &[f32]) -> Vec<IkBone> {
    let mut out = Vec::new();
    let mut prev_len = 0.0;
    for (i, l) in lengths.iter().enumerate() {
        out.push(IkBone { id: NodeId(i as u64 + 1), parent_frame: Mat4::IDENTITY, position: Vec3::X * prev_len, rotation: Quat::IDENTITY, tip: Vec3::X * *l });
        prev_len = *l;
    }
    out
}

fn chain(solver: IkSolver, n: usize) -> IkChain {
    IkChain { id: 1, name: "IK Chain001".into(), solver, start: NodeId(1), end: NodeId(n as u64), goal: NodeId(99), enabled: true, swivel_deg: 0.0, threshold: 0.1, iterations: 50, spline: None, ik_for_fk: false }
}

fn tip(bones: &[IkBone], rots: &[(NodeId, Quat)]) -> Vec3 {
    let mut m = Mat4::IDENTITY;
    for (i, b) in bones.iter().enumerate() {
        m = m * Mat4::from_translation(b.position) * Mat4::from_quat(rots[i].1);
        if i + 1 == bones.len() {
            m *= Mat4::from_translation(b.tip);
        }
    }
    m.transform_point3(Vec3::ZERO)
}

#[test]
fn hi_solver_reaches_a_goal_in_range() {
    let b = bones(&[10.0, 10.0, 10.0]);
    let rots = solve(&chain(IkSolver::HI, 3), &b, Vec3::new(15.0, 10.0, 0.0), None);
    assert_eq!(rots.len(), 3);
    let t = tip(&b, &rots);
    assert!((t - Vec3::new(15.0, 10.0, 0.0)).length() < 0.1, "{t:?}");
}

#[test]
fn hi_solver_reaches_toward_an_unreachable_goal() {
    let b = bones(&[10.0, 10.0]);
    let rots = solve(&chain(IkSolver::HI, 2), &b, Vec3::new(0.0, 100.0, 0.0), None);
    let t = tip(&b, &rots);
    assert!(t.is_finite());
    assert!((t - Vec3::new(0.0, 20.0, 0.0)).length() < 0.1, "straight toward the goal: {t:?}");
}

#[test]
fn limb_solver_is_exact_and_bends_in_the_swivel_plane() {
    let b = bones(&[10.0, 10.0]);
    let mut c = chain(IkSolver::Limb, 2);
    let rots = solve(&c, &b, Vec3::new(10.0, 10.0, 0.0), None);
    assert!((tip(&b, &rots) - Vec3::new(10.0, 10.0, 0.0)).length() < 1e-3, "{:?}", tip(&b, &rots));
    // The elbow lies in the XY plane at swivel 0 and leaves it at swivel 90.
    let elbow = |rots: &[(NodeId, Quat)]| Mat4::from_quat(rots[0].1).transform_point3(Vec3::X * 10.0);
    assert!(elbow(&rots).z.abs() < 1e-3, "{:?}", elbow(&rots));
    c.swivel_deg = 90.0;
    let rots = solve(&c, &b, Vec3::new(10.0, 10.0, 0.0), None);
    assert!(elbow(&rots).z.abs() > 1.0, "{:?}", elbow(&rots));
    assert!((tip(&b, &rots) - Vec3::new(10.0, 10.0, 0.0)).length() < 1e-3);
}

#[test]
fn spline_ik_lays_bones_along_the_shape() {
    let b = bones(&[10.0, 10.0, 10.0]);
    let shape = loom_shape::Shape::polyline_shape(&[Vec3::ZERO, Vec3::new(0.0, 30.0, 0.0)]);
    let mut c = chain(IkSolver::Spline, 3);
    c.spline = Some(NodeId(5));
    let rots = solve(&c, &b, Vec3::ZERO, Some(&shape));
    let t = tip(&b, &rots);
    assert!((t - Vec3::new(0.0, 30.0, 0.0)).length() < 0.1, "{t:?}");
}

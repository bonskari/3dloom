//! Headless test-only quality audit. CPU Mikk is an explicit oracle on the
//! CURRENT GPU-represented positions and stable GPU normals, never a production
//! hybrid or a claimed GPU tangent implementation.
use super::{precision, GpuPoseGeometry, PackedVertex};
use crate::gpu_skin::{GpuSkinPositions, SkinInput};
use crate::{BevyRenderer, MeshData, ObjectFlags, SceneObject, ViewCamera};
use glam::{Mat4, Vec3};
use std::{collections::HashSet, ops::Range};

fn renderer() -> BevyRenderer {
    let instance = wgpu::Instance::new(&Default::default());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("real GPU required");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
            .unwrap();
    BevyRenderer::new(adapter, device, queue)
}
fn couple_tangents(records: &mut [PackedVertex], ranges: &[Range<usize>], templates: &[MeshData]) {
    for (range, template) in ranges.iter().zip(templates) {
        let slice = &mut records[range.clone()];
        let data = MeshData {
            positions: slice
                .iter()
                .map(|v| v.position[..3].try_into().unwrap())
                .collect(),
            normals: slice
                .iter()
                .map(|v| v.normal[..3].try_into().unwrap())
                .collect(),
            uvs: slice
                .iter()
                .map(|v| v.uv[..2].try_into().unwrap())
                .collect(),
            indices: template.indices.clone(),
        };
        let tangents =
            crate::mikktspace::generate_serial(&data).expect("CURRENT coupled CPU tangent oracle");
        for (vertex, tangent) in slice.iter_mut().zip(tangents) {
            vertex.tangent = tangent;
        }
    }
}
fn write_records(
    renderer: &BevyRenderer,
    id: u64,
    ranges: &[Range<usize>],
    records: &[PackedVertex],
) {
    let object = &renderer.objects[&id];
    assert_eq!(ranges.len(), object.buffers.len());
    for (range, buffer) in ranges.iter().zip(&object.buffers) {
        renderer.queue.write_buffer(
            buffer.buffer(),
            0,
            bytemuck::cast_slice(&records[range.clone()]),
        );
    }
}
fn camera(target: Vec3, radius: f32, offset: Vec3) -> ViewCamera {
    let eye = target + offset.normalize() * radius.max(0.1) * 3.2;
    ViewCamera {
        view: Mat4::look_at_rh(eye, target, Vec3::Z),
        projection: Mat4::perspective_rh(45f32.to_radians(), 1.0, 0.01, 10000.0),
    }
}
fn differences(a: &image::RgbaImage, b: &image::RgbaImage) -> (usize, usize, u8, f64) {
    let (mut changed, mut coverage, mut maximum, mut total) = (0, 0, 0u8, 0u64);
    for (a, b) in a.pixels().zip(b.pixels()) {
        changed += usize::from(a != b);
        coverage += usize::from((a[3] != 0) != (b[3] != 0));
        for c in 0..3 {
            let delta = a[c].abs_diff(b[c]);
            maximum = maximum.max(delta);
            total += u64::from(delta);
        }
    }
    (
        changed,
        coverage,
        maximum,
        total as f64 / (a.width() * a.height() * 3) as f64,
    )
}

#[test]
#[ignore = "actual normal-mapped GPU posed-geometry images require LOOM_PERF_SCENE"]
fn actual_ray_stable_gpu_sg_coupled_tbn_image_audit() {
    run_image_audit(false);
}

#[test]
#[ignore = "actual well-lit normal-mapped front head images require LOOM_PERF_SCENE"]
fn actual_ray_stable_gpu_sg_lit_front_image_audit() {
    run_image_audit(true);
}

fn run_image_audit(lit_front: bool) {
    let size = if lit_front { 512 } else { 256 };
    let path = std::env::var("LOOM_PERF_SCENE").expect("real scene");
    let mut document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene: loom_scene::Scene = serde_json::from_value(document["scene"].take()).unwrap();
    let ids: Vec<_> = scene
        .nodes()
        .filter(|n| {
            n.modifiers
                .iter()
                .any(|m| matches!(m.modifier, loom_scene::Modifier::Skin(_)))
        })
        .map(|n| n.id)
        .collect();
    let dynamic: HashSet<_> = ids.iter().map(|id| id.0).collect();
    let (mut reference, mut candidate) = (renderer(), renderer());
    let directory = std::path::Path::new(if lit_front {
        "/tmp/loom-gpu-pose-lit-audit"
    } else {
        "/tmp/loom-gpu-pose-audit"
    });
    std::fs::create_dir_all(directory).unwrap();
    let mut positive = 0;
    for time in [0.0, 39.25, 149.0] {
        scene.set_time(time);
        let meshes: Vec<_> = ids
            .iter()
            .map(|id| {
                let mut m = scene.eval_mesh(*id).value.mesh.clone();
                loom_mesh::edit_ops::transform_mesh(&mut m, scene.object_matrix(*id)).unwrap();
                m
            })
            .collect();
        let objects: Vec<_> = ids
            .iter()
            .zip(&meshes)
            .map(|(id, mesh)| SceneObject {
                id: id.0,
                mesh,
                transform: Mat4::IDENTITY,
                wire_color: [200, 200, 200],
                surfaces: scene.node_surfaces_for(*id, loom_scene::material::ShellView::Viewport),
                renderable: true,
                flags: ObjectFlags::default(),
            })
            .collect();
        reference.sync_deforming(&objects, &dynamic);
        candidate.sync_deforming(&objects, &dynamic);
        let mut stable_frames = Vec::new();
        for (id, expected) in ids.iter().zip(&meshes) {
            let node = scene.node(*id).unwrap();
            assert_eq!(node.modifiers.len(), 1);
            let loom_scene::Modifier::Skin(data) = &node.modifiers[0].modifier else {
                panic!()
            };
            assert!(data.gizmos.iter().all(|g| !g.enabled));
            let rest = node.object.display_output();
            let world = scene.object_matrix(*id);
            let lookup = |id| scene.node(id).map(|_| scene.world_matrix(id));
            let squash = |id| {
                scene.node(id).is_some_and(|n|matches!(&n.object,loom_scene::ObjectKind::Bone{stretch,..} if stretch.get()==2))
            };
            let context = loom_scene::EvalCtx {
                object_world: world,
                object_inverse: world.inverse(),
                world_lookup: Some(&lookup),
                bone_squashes: Some(&squash),
                ..Default::default()
            };
            let matrices = loom_scene::skin::bone_matrices(data, &context);
            let positions: Vec<_> = rest.vertices().iter().map(|v| v.position).collect();
            let weights: Vec<_> = positions
                .iter()
                .enumerate()
                .map(|(i, p)| data.vertex_weights(i, data.mesh_bind.transform_point3(*p)))
                .collect();
            let back = if data.back_transform {
                world.inverse()
            } else {
                data.mesh_bind.inverse()
            };
            let skin = GpuSkinPositions::new(
                &candidate.device,
                &SkinInput {
                    positions: &positions,
                    weights: &weights,
                    bones: &matrices,
                    mesh_bind: data.mesh_bind,
                    object_world: world,
                    back,
                },
            )
            .unwrap();
            let surfaces = scene.node_surfaces_for(*id, loom_scene::material::ShellView::Viewport);
            let parts: Vec<_> = candidate.objects[&id.0]
                .part_ids
                .iter()
                .map(|id| {
                    (
                        *id,
                        surfaces
                            .as_ref()
                            .is_some_and(|s| s.for_face(id.unwrap_or(0)).faceted),
                    )
                })
                .collect();
            let all = crate::mesh_data(expected);
            let templates: Vec<_> = parts
                .iter()
                .map(|(id, faceted)| crate::convert::mesh_data_part(expected, &all, *id, *faceted))
                .collect();
            let oracle: Vec<_> = templates
                .iter()
                .flat_map(|d| crate::mikktspace::generate_serial(d).unwrap())
                .collect();
            let geometry = GpuPoseGeometry::new_origin_relative(
                &candidate.device,
                skin.output(),
                &rest,
                &parts,
                world,
                &oracle,
            )
            .unwrap();
            let mut encoder = candidate.device.create_command_encoder(&Default::default());
            skin.encode(&mut encoder);
            geometry.encode(&mut encoder);
            candidate.queue.submit([encoder.finish()]);
            let mut records = geometry.readback(&candidate.device, &candidate.queue);
            let world_positions =
                geometry.read_world_positions(&candidate.device, &candidate.queue);
            let truth = precision::normals_f64(&rest, &parts, &world_positions);
            assert!(precision::packed_error(&records, &truth) < 1e-5);
            couple_tangents(&mut records, &geometry.parts, &templates);
            let mut high_precision = records.clone();
            for (record, normal) in high_precision.iter_mut().zip(truth) {
                record.normal = [normal.x, normal.y, normal.z, 0.0];
            }
            couple_tangents(&mut high_precision, &geometry.parts, &templates);
            stable_frames.push((id.0, geometry.parts, records, high_precision));
        }
        let face_index = ids
            .iter()
            .position(|id| scene.node(*id).unwrap().name == "Face001")
            .unwrap();
        let (face_lo, face_hi) = meshes[face_index].aabb();
        let face_center = (face_lo + face_hi) * 0.5;
        let face_radius = (face_hi - face_lo).length() * 0.5;
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for mesh in &meshes {
            let (a, b) = mesh.aabb();
            lo = lo.min(a);
            hi = hi.max(b);
        }
        let body_center = (lo + hi) * 0.5;
        let body_radius = (hi - lo).length() * 0.5;
        let mut cameras: Vec<_> = [
            Vec3::new(-1.0, -1.0, 0.3),
            Vec3::new(1.0, -1.0, 0.3),
            Vec3::new(-1.0, 1.0, 0.3),
            Vec3::new(1.0, 1.0, 0.3),
        ]
        .into_iter()
        .map(|v| camera(face_center, face_radius, v))
        .collect();
        cameras.extend([
            camera(body_center, body_radius, Vec3::new(-1.0, -1.0, 0.6)),
            camera(body_center, body_radius, Vec3::new(1.0, 1.0, 0.6)),
        ]);
        if lit_front {
            cameras = [
                Vec3::new(0.0, 1.0, 0.05),
                Vec3::new(-0.45, 1.0, 0.05),
                Vec3::new(0.45, 1.0, 0.05),
            ]
            .into_iter()
            .map(|direction| camera(face_center, face_radius * 0.76, direction))
            .collect();
        }
        for (view, camera) in cameras.iter().enumerate() {
            if lit_front {
                let direction = camera.view.inverse().transform_point3(Vec3::ZERO) - face_center;
                for renderer in [&mut reference, &mut candidate] {
                    let light = renderer.light.unwrap();
                    *renderer
                        .app
                        .world_mut()
                        .get_mut::<bevy::transform::components::Transform>(light)
                        .unwrap() = bevy::transform::components::Transform::from_translation(
                        bevy::math::Vec3::from(direction),
                    )
                    .looking_at(bevy::math::Vec3::ZERO, bevy::math::Vec3::Z);
                    renderer
                        .app
                        .world_mut()
                        .get_mut::<bevy::pbr::DirectionalLight>(light)
                        .unwrap()
                        .illuminance = 10000.0;
                }
                for vertex in meshes[face_index].vertices() {
                    let clip = camera.projection * camera.view * vertex.position.extend(1.0);
                    let ndc = clip.truncate() / clip.w;
                    assert!(
                        clip.w > 0.0 && ndc.x.abs() < 0.98 && ndc.y.abs() < 0.98,
                        "complete head must fit: {ndc:?}"
                    );
                }
                use bevy::asset::Assets;
                // RENDER_WORLD-only images are intentionally removed from main Assets after extraction.
                if time == 0.0 && view == 0 {
                    let mut textured_normal_parts = 0;
                    for (_, material) in &candidate.objects[&ids[face_index].0].parts {
                        let crate::ObjectMaterial::Dynamic(handle) = material else {
                            panic!("dynamic audit material")
                        };
                        let material = candidate
                            .app
                            .world()
                            .resource::<Assets<crate::dynamic_geometry::DynamicMaterial>>()
                            .get(handle)
                            .unwrap();
                        if material.base.base_color_texture.is_some()
                            && material.base.normal_map_texture.is_some()
                        {
                            let texture = candidate
                                .app
                                .world()
                                .resource::<Assets<bevy::image::Image>>()
                                .get(material.base.normal_map_texture.as_ref().unwrap())
                                .unwrap();
                            let colors: HashSet<_> = texture
                                .data
                                .as_ref()
                                .expect("loaded actual normal map")
                                .chunks_exact(4)
                                .map(|p| [p[0], p[1], p[2]])
                                .collect();
                            assert!(colors.len() > 8, "actual normal map must vary");
                            textured_normal_parts += 1;
                        }
                    }
                    assert!(
                        textured_normal_parts > 0,
                        "head must include actual diffuse and varying normal maps"
                    );
                }
            }
            let old = reference.render_image(size, size, camera).unwrap();
            for (id, ranges, records, _) in &stable_frames {
                write_records(&candidate, *id, ranges, records);
            }
            let new = candidate.render_image(size, size, camera).unwrap();
            for (id, ranges, _, truth) in &stable_frames {
                write_records(&candidate, *id, ranges, truth);
            }
            let high_precision = candidate.render_image(size, size, camera).unwrap();
            assert!(
                old.pixels().filter(|p| p[3] != 0).count() > 100,
                "quality camera must see real geometry"
            );
            let legacy = differences(&old, &new);
            let accuracy = differences(&new, &high_precision);
            eprintln!("GPU_POSE_IMAGE time={time} view={view} legacy_changed={} legacy_coverage={} legacy_max={} legacy_mean={:.6} f64_changed={} f64_coverage={} f64_max={} f64_mean={:.6}",legacy.0,legacy.1,legacy.2,legacy.3,accuracy.0,accuracy.1,accuracy.2,accuracy.3);
            if lit_front {
                assert!(
                    accuracy.1 == 0 && accuracy.2 <= 1,
                    "stable coupled TBN must agree with same-position F64 pixels: {accuracy:?}"
                );
            }
            let key = format!("{time}-{view}");
            old.save(directory.join(format!("{key}-legacy.png")))
                .unwrap();
            new.save(directory.join(format!("{key}-gpu-stable.png")))
                .unwrap();
            high_precision
                .save(directory.join(format!("{key}-f64-oracle.png")))
                .unwrap();
            if time == 39.25 && view == 0 {
                for (id, ranges, records, _) in &stable_frames {
                    let mut wrong = records.clone();
                    for v in &mut wrong {
                        for c in 0..3 {
                            v.tangent[c] = -v.tangent[c];
                        }
                    }
                    write_records(&candidate, *id, ranges, &wrong);
                }
                let control = candidate.render_image(size, size, camera).unwrap();
                positive = differences(&control, &new).0;
                control
                    .save(directory.join("positive-control-flipped-tangents.png"))
                    .unwrap();
                assert!(
                    positive > 100,
                    "normal-map positive control must visibly change the image"
                );
            }
        }
    }
    eprintln!(
        "GPU_POSE_IMAGE_AUDIT views={} positive_control_changed={positive} directory={}",
        if lit_front { 9 } else { 18 },
        directory.display()
    );
}

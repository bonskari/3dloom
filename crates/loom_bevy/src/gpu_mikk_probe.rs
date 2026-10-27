//! Unregistered test-only arithmetic probe. Actual EvalTspace group membership
//! and triangle derivatives come from the CPU oracle; nothing here supplies
//! production renderer tangents or bypasses GPU Mikk preparation.
use super::read_records;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Face {
    normal: [f32; 4],
    prev: [f32; 4],
    position: [f32; 4],
    next: [f32; 4],
    os: [f32; 4],
    ot: [f32; 4],
    probe: [f32; 4],
    flags: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
struct Output {
    s: [f32; 4],
    t: [f32; 4],
}

fn current_cosine(face: &crate::mikktspace::GroupFace) -> f32 {
    let normal = Vec3::from_array(face.normal);
    let mut a = Vec3::from_array(face.prev) - Vec3::from_array(face.position);
    let mut b = Vec3::from_array(face.next) - Vec3::from_array(face.position);
    a = a - normal.dot(a) * normal;
    b = b - normal.dot(b) * normal;
    let nonzero = |v: Vec3| v.to_array().iter().any(|v| v.abs() > f32::MIN_POSITIVE);
    if nonzero(a) {
        a = (1.0 / a.length()) * a;
    }
    if nonzero(b) {
        b = (1.0 / b.length()) * b;
    }
    a.dot(b).clamp(-1.0, 1.0)
}

fn evaluate(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    fixtures: &[crate::mikktspace::GroupFixture],
) -> (Vec<Output>, Vec<[f32; 4]>) {
    assert!(!fixtures.is_empty());
    let mut faces = Vec::new();
    let mut groups = Vec::new();
    let vec4 = |v: [f32; 3], w| [v[0], v[1], v[2], w];
    for fixture in fixtures {
        groups.push([faces.len() as u32, fixture.faces.len() as u32]);
        for face in &fixture.faces {
            let cosine = current_cosine(face);
            faces.push(Face {
                normal: vec4(face.normal, 0.0),
                prev: vec4(face.prev, 0.0),
                position: vec4(face.position, 0.0),
                next: vec4(face.next, 0.0),
                os: vec4(face.os, face.mag_s),
                ot: vec4(face.ot, face.mag_t),
                probe: [cosine, (cosine as f64).acos() as f32, 0.0, 0.0],
                flags: [u32::from(face.skip), 0, 0, 0],
            });
        }
    }
    assert!(!faces.is_empty());
    let storage = |label, bytes: &[u8]| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytes,
            usage: wgpu::BufferUsages::STORAGE,
        })
    };
    let input = storage(
        "actual current EvalTspace faces",
        bytemuck::cast_slice(&faces),
    );
    let groups_buffer = storage(
        "actual current EvalTspace groups",
        bytemuck::cast_slice(&groups),
    );
    let output_buffer = |label, size| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    };
    let output = output_buffer("GPU EvalTspace arithmetic", fixtures.len() as u64 * 32);
    let angles = output_buffer("GPU acos comparison", faces.len() as u64 * 16);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("unwired current-group Mikk math"),
        source: wgpu::ShaderSource::Wgsl(include_str!("gpu_mikk_group.wgsl").into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("unwired current-group Mikk math"),
        layout: None,
        module: &shader,
        entry_point: Some("evaluate"),
        compilation_options: Default::default(),
        cache: None,
    });
    let entries: Vec<_> = [&input, &groups_buffer, &output, &angles]
        .iter()
        .enumerate()
        .map(|(i, b)| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: b.as_entire_binding(),
        })
        .collect();
    let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("actual group oracle inputs"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &entries,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &binding, &[]);
        pass.dispatch_workgroups((fixtures.len() as u32).div_ceil(64), 1, 1);
    }
    queue.submit([encoder.finish()]);
    (
        read_records(device, queue, &output, fixtures.len()),
        read_records(device, queue, &angles, faces.len()),
    )
}

#[test]
#[ignore = "actual CPU EvalTspace group fixtures require LOOM_PERF_SCENE"]
fn actual_current_mikk_groups_measure_gpu_math_and_native_acos() {
    let path = std::env::var("LOOM_PERF_SCENE").expect("real saved scene");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene: loom_scene::Scene = serde_json::from_value(value["scene"].take()).unwrap();
    scene.set_time(39.25);
    let node = scene
        .nodes()
        .find(|n| n.name == "Face001")
        .expect("actual Ray Face001");
    let mut mesh = scene.eval_mesh(node.id).value.mesh.clone();
    loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(node.id)).unwrap();
    let data = crate::mesh_data_by_id(&mesh)
        .into_iter()
        .find(|(id, _)| *id == 0)
        .unwrap()
        .1;
    crate::mikktspace::enable_group_capture();
    crate::mikktspace::generate_serial(&data).unwrap();
    let fixtures = crate::mikktspace::take_group_captures();
    assert!(!fixtures.is_empty());
    let (device, queue) = super::tests::device();
    let (results, angles) = evaluate(&device, &queue, &fixtures);
    let (
        mut changed_groups,
        mut max_vector,
        mut max_relative_magnitude,
        mut angle_changes,
        mut max_same_input_acos,
    ) = (0, 0.0f32, 0.0f32, 0, 0.0f32);
    let mut index = 0;
    for (fixture, result) in fixtures.iter().zip(results) {
        let actual_s = Vec3::from_array(result.s[..3].try_into().unwrap());
        let actual_t = Vec3::from_array(result.t[..3].try_into().unwrap());
        assert!(result.s.iter().chain(&result.t).all(|v| v.is_finite()));
        max_vector = max_vector
            .max(actual_s.distance(Vec3::from_array(fixture.expected_s)))
            .max(actual_t.distance(Vec3::from_array(fixture.expected_t)));
        max_relative_magnitude = max_relative_magnitude
            .max(
                (result.s[3] - fixture.expected_mag_s).abs()
                    / fixture.expected_mag_s.abs().max(1e-20),
            )
            .max(
                (result.t[3] - fixture.expected_mag_t).abs()
                    / fixture.expected_mag_t.abs().max(1e-20),
            );
        if result.s.map(f32::to_bits)
            != [
                fixture.expected_s[0],
                fixture.expected_s[1],
                fixture.expected_s[2],
                fixture.expected_mag_s,
            ]
            .map(f32::to_bits)
            || result.t.map(f32::to_bits)
                != [
                    fixture.expected_t[0],
                    fixture.expected_t[1],
                    fixture.expected_t[2],
                    fixture.expected_mag_t,
                ]
                .map(f32::to_bits)
        {
            changed_groups += 1;
        }
        for face in &fixture.faces {
            if !face.skip {
                let cosine = current_cosine(face);
                let cpu = (cosine as f64).acos() as f32;
                let gpu = angles[index][2];
                if cpu.to_bits() != gpu.to_bits() {
                    angle_changes += 1;
                }
                max_same_input_acos = max_same_input_acos.max((cpu - gpu).abs());
            }
            index += 1;
        }
    }
    eprintln!("GPU_MIKK_GROUP groups={} changed_bits={changed_groups} vector_max={max_vector:e} magnitude_relative_max={max_relative_magnitude:e} native_acos_changed={angle_changes} native_acos_abs_max={max_same_input_acos:e}",fixtures.len());
    assert!(
        max_vector < 1e-4 && max_relative_magnitude < 1e-4,
        "arithmetic probe exceeds explicit diagnostic bounds; not a production quality acceptance"
    );
}

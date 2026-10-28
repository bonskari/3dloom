#[cfg(test)]
mod precision_tests {
    use super::*;
    use crate::MeshData;
    fn device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("real GPU required");
        pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
            .unwrap()
    }
    fn precision_math(
        device: &wgpu::Device,
        vertices: &wgpu::Buffer,
        base: u32,
        data: &MeshData,
        preparation: &GpuPreparation,
        initial: &Prepared,
        groups: &crate::mikktspace::StructuralRecipe,
    ) -> Result<GpuRecipeMath, String> {
        let program = crate::gpu_mikk_precision::PrecisionProgram::new(device)?;
        GpuRecipeMath::with_precision(
            device,
            vertices,
            base,
            data,
            preparation,
            initial,
            groups,
            Some(&program),
        )
    }
    fn fixture() -> MeshData {
        MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, -1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [2.0, 0.0]],
            indices: vec![0, 1, 2, 1, 0, 3, 0, 0, 1],
        }
    }
    fn packed(data: &MeshData) -> Vec<PackedVertex> {
        (0..data.positions.len())
            .map(|i| PackedVertex {
                position: [
                    data.positions[i][0],
                    data.positions[i][1],
                    data.positions[i][2],
                    1.0,
                ],
                normal: [
                    data.normals[i][0],
                    data.normals[i][1],
                    data.normals[i][2],
                    0.0,
                ],
                uv: [data.uvs[i][0], data.uvs[i][1], 0.0, 0.0],
                tangent: [f32::NAN; 4],
            })
            .collect()
    }
    fn submit(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        preparation: &GpuPreparation,
        math: Option<&GpuRecipeMath>,
    ) {
        let mut encoder = device.create_command_encoder(&Default::default());
        preparation.encode(&mut encoder);
        if let Some(math) = math {
            math.encode(&mut encoder);
        }
        queue.submit([encoder.finish()]);
    }
    fn compare(data: &MeshData, actual: &[PackedVertex]) -> f32 {
        let expected = crate::mikktspace::generate_serial(data).unwrap();
        let mut maximum = 0.0f32;
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!(
                a.tangent.iter().all(|v| v.is_finite()),
                "poisoned CPU input tangents must never become output"
            );
            assert_eq!(
                a.tangent[3], e[3],
                "current orientation/last setter at vertex {i}"
            );
            let error = glam::Vec3::from_array(a.tangent[..3].try_into().unwrap())
                .distance(glam::Vec3::from_array(e[..3].try_into().unwrap()));
            maximum = maximum.max(error);
        }
        maximum
    }
    fn precision_fixture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &MeshData,
    ) -> (GpuRecipeMath, crate::mikktspace::StructuralRecipe) {
        let values = packed(data);
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("real synthetic current attributes"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let prep = GpuPreparation::new(device, &input, values.len(), &data.indices).unwrap();
        submit(device, queue, &prep, None);
        let initial = prep.readback(device, queue).unwrap();
        let structure = crate::mikktspace::compile_structure(data);
        eprintln!(
            "FINITE_CASE_PREP gpu_flags={:?} cpu_flags={:?}",
            initial.flags, structure.initial_flags
        );
        let math = precision_math(device, &input, 0, data, &prep, &initial, &structure).unwrap();
        submit(device, queue, &prep, Some(&math));
        (math, structure)
    }
    fn precision_reference_check(
        data: &MeshData,
        structure: &crate::mikktspace::StructuralRecipe,
        actual: &[PackedVertex],
    ) {
        let truth = super::super::mikk_f64::tangents_f64(data, structure);
        assert_eq!(truth.len(), actual.len());
        for (i, (v, t)) in actual.iter().zip(truth).enumerate() {
            assert!(
                v.tangent.iter().all(|x| x.is_finite()),
                "nonfinite output vertex={i}"
            );
            assert_eq!(f64::from(v.tangent[3]), t[3]);
            let error = glam::Vec3::from_array(v.tangent[..3].try_into().unwrap())
                .as_dvec3()
                .distance(glam::DVec3::from_array(t[..3].try_into().unwrap()));
            assert!(error<2e-5,"synthetic precision mismatch vertex={i} error={error:e} actual={:?} expected={t:?}",v.tangent);
        }
    }
    #[test]
    fn finite_large_small_and_near_antiparallel_cases_remain_finite_and_accurate() {
        let (device, queue) = device();
        for scale in [1e-16f32, 1e-8, 1.0, 1e18, 1e20, 1e30] {
            let data = MeshData {
                positions: vec![[0.0, 0.0, 0.0], [scale, 0.0, 0.0], [0.0, scale, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]; 3],
                uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
                indices: vec![0, 1, 2],
            };
            let (math, structure) = precision_fixture(&device, &queue, &data);
            let actual = math.readback(&device, &queue).unwrap();
            precision_reference_check(&data, &structure, &actual);
            eprintln!(
                "FINITE_SCALE_PASS scale={scale:e} risks={:?}",
                math.read_precision_risks(&device, &queue)
            );
        }
        for skew in [1.0f32, 1e3, 1e8, 1e12, 1e15, 2e15, 4e15, 1e16, 1e20] {
            let data = MeshData {
                positions: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, -1.0, 0.0],
                ],
                normals: vec![[0.0, 0.0, 1.0]; 4],
                uvs: vec![[0.0, 0.0], [0.0, 1.0], [1.0, skew], [-1.0, skew]],
                indices: vec![0, 1, 2, 1, 0, 3],
            };
            let (math, structure) = precision_fixture(&device, &queue, &data);
            let actual = math.readback(&device, &queue).unwrap();
            precision_reference_check(&data, &structure, &actual);
            eprintln!(
                "FINITE_ANTIPARALLEL_PASS skew={skew:e} risks={:?} tangent={:?}",
                math.read_precision_risks(&device, &queue),
                actual[0].tangent
            );
        }
    }
    #[test]
    fn final_nonfinite_payload_and_invalid_source_are_guard_rejections() {
        let (device, queue) = device();
        let data = fixture();
        let (math, _) = precision_fixture(&device, &queue, &data);
        math.readback(&device, &queue).unwrap();
        queue.write_buffer(
            &math.output,
            32,
            bytemuck::cast_slice(&[f32::NAN, 0.0, 0.0, 1.0]),
        );
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(math.pipelines.last().unwrap());
            pass.set_bind_group(0, &math.group, &[]);
            pass.dispatch_workgroups(math.counts[0].div_ceil(64), 1, 1);
        }
        queue.submit([encoder.finish()]);
        let guard: Vec<u32> = super::super::read_records(&device, &queue, math.guard_buffer(), 1);
        assert_ne!(guard[0] & 32, 0);
        assert!(math.readback(&device, &queue).is_err());
        eprintln!("FINAL_NONFINITE_REJECTED guard={guard:?}");
        let mut invalid = fixture();
        invalid.normals[0] = [f32::INFINITY, 0.0, 1.0];
        let values = packed(&invalid);
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("invalid source is rejected before recipe"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let prep = GpuPreparation::new(&device, &input, values.len(), &invalid.indices).unwrap();
        submit(&device, &queue, &prep, None);
        let error = prep.readback(&device, &queue).unwrap_err();
        assert!(error.contains("nonfinite current attribute"));
        eprintln!("INVALID_SOURCE_REJECTED_BEFORE_RECIPE {error}");
    }
    #[test]
    fn cone_zero_angle_collinear_case_retains_full_risk_flags() {
        let (device, queue) = device();
        let data = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            indices: vec![0, 1, 2],
        };
        let (math, structure) = precision_fixture(&device, &queue, &data);
        precision_reference_check(&data, &structure, &math.readback(&device, &queue).unwrap());
    }
    #[test]
    fn finite_source_overflow_must_refine_or_reject_before_commit() {
        let (device, queue) = device();
        let data = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1e30, 0.0, 0.0], [0.0, 1e30, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1e10, 0.0], [0.0, 1e10]],
            indices: vec![0, 1, 2],
        };
        assert!(data
            .positions
            .iter()
            .flatten()
            .chain(data.normals.iter().flatten())
            .chain(data.uvs.iter().flatten())
            .all(|v| v.is_finite()));
        let values = packed(&data);
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("finite actual arithmetic overflow case"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let preparation =
            GpuPreparation::new(&device, &input, values.len(), &data.indices).unwrap();
        submit(&device, &queue, &preparation, None);
        let initial = preparation.readback(&device, &queue).unwrap();
        let structure = crate::mikktspace::compile_structure(&data);
        eprintln!(
            "FINITE_OVERFLOW_PREP gpu_flags={:?} cpu_flags={:?}",
            initial.flags, structure.initial_flags
        );
        let math = precision_math(
            &device,
            &input,
            0,
            &data,
            &preparation,
            &initial,
            &structure,
        )
        .unwrap();
        submit(&device, &queue, &preparation, Some(&math));
        let guard: Vec<u32> = super::super::read_records(&device, &queue, math.guard_buffer(), 1);
        let risks = math.read_precision_risks(&device, &queue);

        match math.readback(&device,&queue){Ok(output)=>{let truth=super::super::mikk_f64::tangents_f64(&data,&structure);for(i,v)in output.iter().enumerate(){eprintln!("FINITE_OVERFLOW_RESULT vertex={i} guard={guard:?} risks={risks:?} tangent={:?} truth={:?}",v.tangent,truth[i]);assert!(v.tangent.iter().all(|x|x.is_finite()),"FINITE_OVERFLOW_PUBLISHED_NONFINITE");let error=glam::Vec3::from_array(v.tangent[..3].try_into().unwrap()).as_dvec3().distance(glam::DVec3::from_array(truth[i][..3].try_into().unwrap()));assert!(error<2e-5);}},Err(error)=>panic!("recoverable finite overflow must refine successfully: {error} guard={guard:?} risks={risks:?}")}
    }
    #[test]
    fn gpu_current_recipe_math_preserves_any_orientation_last_owner_and_degenerate_epilogue() {
        let (device, queue) = device();
        let mut data = fixture();
        let mut values = packed(&data);
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("current attrs; CPU tangent fields intentionally poisoned"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let preparation =
            GpuPreparation::new(&device, &input, values.len(), &data.indices).unwrap();
        submit(&device, &queue, &preparation, None);
        let initial = preparation.readback(&device, &queue).unwrap();
        let structure = crate::mikktspace::compile_structure(&data);
        assert!(
            structure
                .triangles
                .iter()
                .any(|t| t.flags & 4 != 0 && t.flags & 8 != 0),
            "fixture must adopt Any orientation globally"
        );
        let math = precision_math(
            &device,
            &input,
            0,
            &data,
            &preparation,
            &initial,
            &structure,
        )
        .unwrap();
        for pose in 0..3 {
            if pose > 0 {
                data.positions[1] = [1.0, 0.2 * pose as f32, 0.1];
                data.positions[2] = [0.2, 1.0, 0.1 * pose as f32];
                values = packed(&data);
                queue.write_buffer(&input, 0, bytemuck::cast_slice(&values));
            }
            submit(&device, &queue, &preparation, Some(&math));
            let actual = math.readback(&device, &queue).unwrap();

            let error = compare(&data, &actual);
            assert!(
                error < 2e-5,
                "CURRENT recipe math max vector error {error:e}"
            );
        }
        let before: Vec<PackedVertex> =
            super::super::read_records(&device, &queue, &math.output, values.len());
        data.normals[0] = [0.0, 1.0, 0.0];
        // A seam may keep the same class partition when its vertex was already
        // unique; force a UV-degenerate flag change as well, a real guard miss.
        data.uvs[3] = [0.5, 0.5];
        values = packed(&data);
        queue.write_buffer(&input, 0, bytemuck::cast_slice(&values));
        submit(&device, &queue, &preparation, Some(&math));
        assert!(math
            .readback(&device, &queue)
            .unwrap_err()
            .contains("guard rejected"));
        let after: Vec<PackedVertex> =
            super::super::read_records(&device, &queue, &math.output, values.len());
        assert_eq!(
            bytemuck::cast_slice::<PackedVertex, u8>(&before),
            bytemuck::cast_slice::<PackedVertex, u8>(&after),
            "guard miss cannot publish candidate geometry"
        );
    }
    #[test]
    fn current_recipe_handles_interspersed_degenerate_face_order() {
        let (device, queue) = device();
        let mut data = fixture();
        data.indices = vec![0, 1, 2, 0, 0, 1, 1, 0, 3];
        let values = packed(&data);
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("real degen-middle synthetic attributes"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let preparation =
            GpuPreparation::new(&device, &input, values.len(), &data.indices).unwrap();
        submit(&device, &queue, &preparation, None);
        let initial = preparation.readback(&device, &queue).unwrap();
        assert_eq!(initial.original_faces, vec![0, 2, 1]);
        let structure = crate::mikktspace::compile_structure(&data);
        let math = precision_math(
            &device,
            &input,
            0,
            &data,
            &preparation,
            &initial,
            &structure,
        )
        .unwrap();
        submit(&device, &queue, &preparation, Some(&math));
        let actual = math.readback(&device, &queue).unwrap();

        assert!(compare(&data, &actual) < 2e-5);
    }
    #[test]
    fn stable_angular_predicates_preserve_opposite_near_opposite_zero_and_any_inputs() {
        let (device, queue) = device();
        let diagonal = glam::Vec3::new(1.0, 2.0, 3.0).normalize();
        let cases = [
            (glam::Vec3::X, -glam::Vec3::X, false),
            (diagonal, -diagonal, false),
            (
                glam::Vec3::X,
                glam::Vec3::new(-1.0, 1e-4, 0.0).normalize(),
                true,
            ),
            (glam::Vec3::ZERO, glam::Vec3::X, true),
            (glam::Vec3::ZERO, glam::Vec3::ZERO, true),
            (diagonal, diagonal, true),
            (glam::Vec3::X, glam::Vec3::Y, true),
        ];
        let input: Vec<[[f32; 4]; 2]> = cases
            .iter()
            .map(|(a, b, _)| [a.extend(0.0).to_array(), b.extend(0.0).to_array()])
            .collect();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("real angular boundary cases"),
            contents: bytemuck::cast_slice(&input),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: cases.len() as u64 * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let source = include_str!("gpu_mikk_precision.wgsl");
        let begin = source.find("fn nonzero").unwrap();
        let end = source.find("fn descriptor").unwrap();
        let shader_source=format!("{}\nstruct Case{{a:vec4<f32>,b:vec4<f32>}}\n@group(0) @binding(0) var<storage,read> cases:array<Case>;\n@group(0) @binding(1) var<storage,read_write> results:array<vec4<f32>>;\n@compute @workgroup_size(64) fn check(@builtin(global_invocation_id) id:vec3<u32>){{if id.x>=arrayLength(&cases){{return;}}let a=cases[id.x].a.xyz;let b=cases[id.x].b.xyz;results[id.x]=vec4<f32>(select(0.0,1.0,angular_member(a,b)),corner_angle(a,b),select(0.0,1.0,dot(a,b)>-1.0),0.0);}}",&source[begin..end]);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("actual recipe angular functions boundary regression"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &module,
            entry_point: Some("check"),
            compilation_options: Default::default(),
            cache: None,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        queue.submit([encoder.finish()]);
        let actual: Vec<[f32; 4]> =
            super::super::read_records(&device, &queue, &output, cases.len());
        for (i, ((a, b, member), result)) in cases.iter().zip(&actual).enumerate() {
            assert_eq!(result[0], f32::from(u8::from(*member)), "case{i}");
            let ideal = if a.length_squared() == 0.0 || b.length_squared() == 0.0 {
                std::f64::consts::FRAC_PI_2
            } else {
                a.as_dvec3()
                    .cross(b.as_dvec3())
                    .length()
                    .atan2(a.as_dvec3().dot(b.as_dvec3()))
            };
            assert!((f64::from(result[1]) - ideal).abs() < 1e-6, "angle case{i}");
        }
        assert_eq!(
            actual[2][2], 0.0,
            "original unit dot predicate must reproduce the nearly opposite false exclusion"
        );
        assert_eq!(actual[2][0], 1.0);
    }
    #[test]
    fn current_recipe_all_any_and_all_position_degenerate_defaults_match_f64() {
        let (device, queue) = device();
        for degenerate in [false, true] {
            let mut data = fixture();
            if degenerate {
                data.positions.fill([0.0; 3]);
            } else {
                data.uvs.fill([0.0; 2]);
            }
            let values = packed(&data);
            let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("actual all-any/all-position-degenerate values"),
                contents: bytemuck::cast_slice(&values),
                usage: wgpu::BufferUsages::STORAGE,
            });
            let preparation =
                GpuPreparation::new(&device, &input, values.len(), &data.indices).unwrap();
            submit(&device, &queue, &preparation, None);
            let initial = preparation.readback(&device, &queue).unwrap();
            let structure = crate::mikktspace::compile_structure(&data);
            assert!(structure.groups.is_empty());
            let math = precision_math(
                &device,
                &input,
                0,
                &data,
                &preparation,
                &initial,
                &structure,
            )
            .unwrap();
            submit(&device, &queue, &preparation, Some(&math));
            let actual = math.readback(&device, &queue).unwrap();

            let truth = super::super::mikk_f64::tangents_f64(&data, &structure);
            assert_eq!(actual.len(), truth.len());
            for (a, t) in actual.iter().zip(truth) {
                assert_eq!(a.tangent.map(f64::from), t);
                assert_eq!(a.tangent, [1.0, 0.0, 0.0, 1.0]);
            }
        }
    }
    fn serial_parallel_parity(device: &wgpu::Device, queue: &wgpu::Queue, data: &MeshData) {
        let values = packed(data);
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("actual shared indexed / duplicated triangle / degenerate source"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let prep = GpuPreparation::new(device, &input, values.len(), &data.indices).unwrap();
        submit(device, queue, &prep, None);
        let initial = prep.readback(device, queue).unwrap();
        let structure = crate::mikktspace::compile_structure(data);
        let recipe =
            Recipe::from_current_structure(data, &initial, &structure, [0; 3], 0, true).unwrap();
        assert_eq!(
            recipe.refine_work_items as usize,
            structure
                .groups
                .iter()
                .map(|g| g.members.len())
                .sum::<usize>()
        );
        let fast =
            Recipe::from_current_structure(data, &initial, &structure, [0; 3], 0, false).unwrap();
        assert_eq!(fast.refine_work_items, 0);
        assert_eq!(fast.params.recipe1[3], 0);
        assert_eq!(
            fast.words,
            &recipe.words[..recipe.params.recipe1[3] as usize]
        );
        let mut same_params = recipe.params;
        same_params.recipe1[3] = 0;
        assert_eq!(
            bytemuck::bytes_of(&fast.params),
            bytemuck::bytes_of(&same_params)
        );

        let parallel_program = crate::gpu_mikk_precision::PrecisionProgram::new(device).unwrap();
        let serial_program = crate::gpu_mikk_precision::PrecisionProgram::serial(device).unwrap();
        let parallel = GpuRecipeMath::with_precision(
            device,
            &input,
            0,
            data,
            &prep,
            &initial,
            &structure,
            Some(&parallel_program),
        )
        .unwrap();
        let serial = GpuRecipeMath::with_precision(
            device,
            &input,
            0,
            data,
            &prep,
            &initial,
            &structure,
            Some(&serial_program),
        )
        .unwrap();
        submit(device, queue, &prep, Some(&serial));
        submit(device, queue, &prep, Some(&parallel));
        let a = serial.readback(device, queue).unwrap();
        let b = parallel.readback(device, queue).unwrap();
        assert_eq!(
            serial.read_precision_risks(device, queue),
            parallel.read_precision_risks(device, queue)
        );
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a),
            bytemuck::cast_slice::<_, u8>(&b),
            "parallel per-output writes differ from serial group loop"
        );
        precision_reference_check(data, &structure, &b);
        eprintln!(
            "PARALLEL_SYNTHETIC_BYTES_PASS triangles={} groups={} unique_writers={} vertices={}",
            data.indices.len() / 3,
            structure.groups.len(),
            recipe.refine_work_items,
            values.len()
        );
    }
    #[test]
    fn parallel_writes_preserve_duplicate_triangles_shared_vertices_any_and_degenerate_cases() {
        let (device, queue) = device();
        let original = fixture();
        serial_parallel_parity(&device, &queue, &original);
        let mut repeated = original.clone();
        repeated.indices.extend_from_slice(&original.indices[..3]);
        serial_parallel_parity(&device, &queue, &repeated);
        repeated.indices.extend_from_slice(&original.indices[..3]);
        serial_parallel_parity(&device, &queue, &repeated);
        let mut any = original.clone();
        any.uvs.fill([0.0; 2]);
        serial_parallel_parity(&device, &queue, &any);
        let mut degen = original.clone();
        degen.positions.fill([0.0; 3]);
        serial_parallel_parity(&device, &queue, &degen);
        let mut interspersed = original.clone();
        interspersed.indices = vec![0, 0, 1, 0, 1, 2, 1, 1, 0, 1, 0, 3];
        serial_parallel_parity(&device, &queue, &interspersed);
        let mut nonmanifold = original.clone();
        nonmanifold.positions.push([0.5, 1.0, 1.0]);
        nonmanifold.normals.push([0.0, 0.0, 1.0]);
        nonmanifold.uvs.push([0.5, 1.0]);
        nonmanifold.indices.extend_from_slice(&[0, 1, 4]);
        serial_parallel_parity(&device, &queue, &nonmanifold);
        for scale in [1e-16f32, 1e-8, 1.0, 1e18, 1e20, 1e30] {
            let data = MeshData {
                positions: vec![[0.0, 0.0, 0.0], [scale, 0.0, 0.0], [0.0, scale, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]; 3],
                uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
                indices: vec![0, 1, 2],
            };
            serial_parallel_parity(&device, &queue, &data);
        }
        for skew in [1.0f32, 1e3, 1e8, 1e12, 1e15, 2e15, 4e15, 1e16, 1e20] {
            let data = MeshData {
                positions: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, -1.0, 0.0],
                ],
                normals: vec![[0.0, 0.0, 1.0]; 4],
                uvs: vec![[0.0, 0.0], [0.0, 1.0], [1.0, skew], [-1.0, skew]],
                indices: vec![0, 1, 2, 1, 0, 3],
            };
            serial_parallel_parity(&device, &queue, &data);
        }
    }
    #[test]
    fn parallel_metadata_rejects_duplicate_writers_without_dropping_inner_members() {
        let (device, queue) = device();
        let data = fixture();
        let values = packed(&data);
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ownership validation input"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let prep = GpuPreparation::new(&device, &input, values.len(), &data.indices).unwrap();
        submit(&device, &queue, &prep, None);
        let initial = prep.readback(&device, &queue).unwrap();
        let structure = crate::mikktspace::compile_structure(&data);
        let mut duplicate = structure.clone();
        let member = duplicate.groups[0].members[0];
        duplicate.groups[0].members.push(member);
        assert!(
            Recipe::from_current_structure(&data, &initial, &duplicate, [0; 3], 0, true)
                .err()
                .unwrap()
                .contains("duplicate spaces writer")
        );
        let mut cross = structure.clone();
        cross.groups.push(cross.groups[0].clone());
        assert!(
            Recipe::from_current_structure(&data, &initial, &cross, [0; 3], 0, true)
                .err()
                .unwrap()
                .contains("duplicate spaces writer")
        );
        assert_eq!(
            structure.groups[0].members,
            crate::mikktspace::compile_structure(&data).groups[0].members,
            "inner membership is never deduplicated/reordered by output flattening"
        );
    }
}

            let part_ids: Vec<_> = {
                let _scope = loom_render::profile::Scope::new("gpu_pose.material_ids");
                if matches!(source.surfaces, Some(Surfaces::PerId(_))) {
                source
                    .mesh
                    .faces()
                    .iter()
                    .filter(|f| !f.hidden)
                    .map(|f| f.material_id)
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .map(Some)
                    .collect()
                } else {
                    vec![None]
                }
            };

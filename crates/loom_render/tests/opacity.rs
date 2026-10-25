//! Actual viewport shader readback: mapped geometry alpha and legacy RGBA decals.
use glam::{Mat4, Vec3};
use loom_render::{Bitmap, MeshBuffer, MeshStyle, TextureSel, ViewShading, ViewportRenderer};

#[test]
fn viewport_opacity_pixels_reveal_background_while_legacy_rgba_keeps_decal_colour() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let Some(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("no GPU adapter: opacity shader test skipped");
        return;
    };
    eprintln!("opacity pixel adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
            .unwrap();
    let renderer = ViewportRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.update_view(
        &queue,
        0,
        Mat4::IDENTITY,
        Vec3::Z * 2.0,
        ViewShading {
            lighting: 1,
            ..Default::default()
        },
    );
    let mut mesh =
        loom_mesh::quad_primitive(Vec3::new(-1.0, -1.0, 0.5), Vec3::X * 2.0, Vec3::Y * 2.0);
    for (i, uv) in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
        .into_iter()
        .enumerate()
    {
        mesh.set_corner_uv(
            1,
            loom_mesh::half_edge::HalfEdgeId(i as u32),
            Vec3::new(uv[0], uv[1], 0.0),
        );
    }
    let bitmap = Bitmap {
        width: 2,
        height: 1,
        rgba: vec![255, 0, 0, 0, 255, 0, 0, 255],
    };
    let textures = renderer.make_texture_set(&device, &queue, &[bitmap]);
    let render = |amount| {
        // Negative amount is the vertex protocol for composed opacity, with no
        // vertex-layout/binding change. A material's constant opacity stays separate.
        let buffer = MeshBuffer::from_mesh(
            &device,
            &mesh,
            MeshStyle {
                color: [0.0, 1.0, 0.0],
                alpha: 0.5,
                unlit: true,
                texture: TextureSel::Bitmap(0),
                texture_amount: amount,
                ..Default::default()
            },
        );
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("opacity test"),
            size: wgpu::Extent3d {
                width: 64,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: texture.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let depth = depth.create_view(&Default::default());
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * 32,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            renderer.record_into_pass(
                &mut pass,
                0,
                &[&buffer],
                &[],
                false,
                true,
                &[],
                None,
                true,
                &textures,
                None,
            );
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(32),
                },
            },
            texture.size(),
        );
        queue.submit([encoder.finish()]);
        let slice = output.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        let _ = device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();
        let data = slice.get_mapped_range();
        let pixel = |x: usize| data[(16 * 64 + x) * 4..(16 * 64 + x) * 4 + 4].to_vec();
        (pixel(16), pixel(48))
    };
    let (clear, solid) = render(-1.0);
    assert!(
        clear[0] < 8 && clear[1] < 8 && clear[2] > 245,
        "opacity zero must reveal blue background: {clear:?}"
    );
    assert!(
        (120..=135).contains(&solid[0]) && solid[1] < 8 && (120..=135).contains(&solid[2]),
        "mapped alpha multiplies material 0.5: {solid:?}"
    );
    let (decal, _) = render(1.0);
    assert!(
        decal[0] < 8 && (120..=135).contains(&decal[1]) && (120..=135).contains(&decal[2]),
        "ordinary RGBA alpha still mixes with green base: {decal:?}"
    );
}

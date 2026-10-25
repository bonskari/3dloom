//! The environment pass in Bevy's 3D render graph: after the main pass
//! (and volumetric fog), before bloom and tonemapping, it draws the
//! environment background, the atmospheres and the Depth of Field blur
//! (`env_pass.wgsl`). A camera takes part through its [`EnvPass`]
//! component; an all-zero uniform leaves the image as it is.
//!
//! In an HDR view (Process Background) the pass runs before tonemapping,
//! so the exposure control processes the background too; otherwise the
//! main pass has tonemapped already and the background keeps its colour.

use bevy::app::{App, Plugin};
use bevy::asset::{weak_handle, Assets, Handle};
use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::QueryItem;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::lifetimeless::Read;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::ecs::world::{FromWorld, World};
use bevy::image::{BevyDefault, Image};
use bevy::math::Vec4;
use bevy::pbr::graph::NodePbr;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, CachedRenderPipelineId, ColorTargetState, ColorWrites, FilterMode, FragmentState, LoadOp, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, Shader, ShaderStages, ShaderType, SpecializedRenderPipeline, SpecializedRenderPipelines, StoreOp, TextureFormat, TextureSampleType,
};
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::view::{ExtractedView, Msaa, ViewDepthTexture, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms};
use bevy::render::{Render, RenderApp, RenderSet};

const SHADER: Handle<Shader> = weak_handle!("6b0f5a9e-3d4c-4a8e-9b7d-1f2e3c4d5a6b");

/// The pass's values for one view (see `env_pass.wgsl`).
#[derive(Component, ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct EnvUniform {
    pub background: Vec4,
    pub map: Vec4,
    pub frame: Vec4,
    pub fog_color: [Vec4; 4],
    pub fog_a: [Vec4; 4],
    pub fog_b: [Vec4; 4],
    pub dof: Vec4,
    pub dof_loss: Vec4,
}

impl EnvUniform {
    fn environment(&self) -> bool {
        self.background.w > 0.5 || self.fog_color.iter().any(|c| c.w > 0.5)
    }

    fn depth_of_field(&self) -> bool {
        self.dof.x > 0.5
    }
}

/// On a camera: the environment pass draws with `uniform` and `map`
/// (Use Map's bitmap; None binds a blank image).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct EnvPass {
    pub uniform: EnvUniform,
    pub map: Option<Handle<Image>>,
}

/// The render world's copy of the map.
#[derive(Component, Clone)]
pub struct EnvMap(Option<Handle<Image>>);

impl ExtractComponent for EnvPass {
    type QueryData = Read<EnvPass>;
    type QueryFilter = ();
    type Out = (EnvUniform, EnvMap);

    fn extract_component(item: QueryItem<'_, Self::QueryData>) -> Option<Self::Out> {
        Some((item.uniform, EnvMap(item.map.clone())))
    }
}

pub struct EnvPassPlugin;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct EnvPassLabel;

impl Plugin for EnvPassPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().resource_mut::<Assets<Shader>>().insert(&SHADER, Shader::from_wgsl(include_str!("env_pass.wgsl"), "loom_bevy/env_pass.wgsl"));
        app.add_plugins((ExtractComponentPlugin::<EnvPass>::default(), UniformComponentPlugin::<EnvUniform>::default()));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<SpecializedRenderPipelines<EnvPipeline>>()
            .add_systems(Render, prepare_pipelines.in_set(RenderSet::Prepare))
            .add_render_graph_node::<ViewNodeRunner<EnvNode>>(Core3d, EnvPassLabel);
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app.init_resource::<EnvPipeline>();
        // After volumetric fog (added in the PBR plugin's finish), before bloom.
        render_app.add_render_graph_edges(Core3d, (NodePbr::VolumetricFog, EnvPassLabel, Node3d::Bloom));
    }
}

#[derive(Resource)]
pub struct EnvPipeline {
    single: BindGroupLayout,
    multisampled: BindGroupLayout,
    map_sampler: Sampler,
}

impl FromWorld for EnvPipeline {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let layout = |label: &'static str, ms: bool| {
            device.create_bind_group_layout(
                Some(label),
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::FRAGMENT,
                    (
                        uniform_buffer::<ViewUniform>(true),
                        if ms { texture_depth_2d_multisampled() } else { texture_depth_2d() },
                        texture_2d(TextureSampleType::Float { filterable: true }),
                        uniform_buffer::<EnvUniform>(true),
                        texture_2d(TextureSampleType::Float { filterable: true }),
                        sampler(SamplerBindingType::Filtering),
                    ),
                ),
            )
        };
        let single = layout("loom environment pass layout", false);
        let multisampled = layout("loom environment pass layout (multisampled)", true);
        let map_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("loom environment map sampler"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            address_mode_u: bevy::render::render_resource::AddressMode::Repeat,
            address_mode_v: bevy::render::render_resource::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self { single, multisampled, map_sampler }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Pass {
    Environment,
    DofHorizontal,
    DofVertical,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnvPipelineKey {
    hdr: bool,
    multisampled: bool,
    pass: Pass,
}

impl SpecializedRenderPipeline for EnvPipeline {
    type Key = EnvPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut shader_defs = Vec::new();
        if key.multisampled {
            shader_defs.push("MULTISAMPLED".into());
        }
        RenderPipelineDescriptor {
            label: Some("loom environment pass".into()),
            layout: vec![if key.multisampled { self.multisampled.clone() } else { self.single.clone() }],
            push_constant_ranges: Vec::new(),
            vertex: fullscreen_shader_vertex_state(),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(FragmentState {
                shader: SHADER,
                shader_defs,
                entry_point: match key.pass {
                    Pass::Environment => "environment",
                    Pass::DofHorizontal => "dof_horizontal",
                    Pass::DofVertical => "dof_vertical",
                }
                .into(),
                targets: vec![Some(ColorTargetState { format: if key.hdr { ViewTarget::TEXTURE_FORMAT_HDR } else { TextureFormat::bevy_default() }, blend: None, write_mask: ColorWrites::ALL })],
            }),
            zero_initialize_workgroup_memory: false,
        }
    }
}

/// The specialised pipelines of a view, in pass order.
#[derive(Component)]
pub struct EnvPipelines(Vec<CachedRenderPipelineId>);

fn prepare_pipelines(mut commands: Commands, cache: Res<PipelineCache>, mut pipelines: ResMut<SpecializedRenderPipelines<EnvPipeline>>, pipeline: Res<EnvPipeline>, views: Query<(Entity, &ExtractedView, &Msaa, &EnvUniform)>) {
    for (entity, view, msaa, uniform) in &views {
        let mut passes = Vec::new();
        if uniform.environment() {
            passes.push(Pass::Environment);
        }
        if uniform.depth_of_field() {
            passes.extend([Pass::DofHorizontal, Pass::DofVertical]);
        }
        let ids = passes.into_iter().map(|pass| pipelines.specialize(&cache, &pipeline, EnvPipelineKey { hdr: view.hdr, multisampled: *msaa != Msaa::Off, pass })).collect();
        commands.entity(entity).insert(EnvPipelines(ids));
    }
}

#[derive(Default)]
struct EnvNode;

impl ViewNode for EnvNode {
    type ViewQuery = (Read<ViewUniformOffset>, Read<ViewTarget>, Read<ViewDepthTexture>, Read<EnvPipelines>, Read<DynamicUniformIndex<EnvUniform>>, Read<EnvMap>, Read<Msaa>);

    fn run<'w>(&self, _graph: &mut RenderGraphContext, ctx: &mut RenderContext<'w>, (view_offset, target, depth, pipelines, env_index, map, msaa): QueryItem<'w, Self::ViewQuery>, world: &'w World) -> Result<(), NodeRunError> {
        let cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<EnvPipeline>();
        let (Some(view_binding), Some(env_binding)) = (world.resource::<ViewUniforms>().uniforms.binding(), world.resource::<ComponentUniforms<EnvUniform>>().binding()) else {
            return Ok(());
        };
        let images = world.resource::<RenderAssets<GpuImage>>();
        let fallback = world.resource::<FallbackImage>();
        let map_view = map.0.as_ref().and_then(|h| images.get(h)).map(|g| &g.texture_view).unwrap_or(&fallback.d2.texture_view);
        let layout = if *msaa != Msaa::Off { &pipeline.multisampled } else { &pipeline.single };
        for id in &pipelines.0 {
            let Some(render_pipeline) = cache.get_render_pipeline(*id) else { return Ok(()) };
            let post = target.post_process_write();
            let bind_group = ctx.render_device().create_bind_group(
                Some("loom environment pass"),
                layout,
                &BindGroupEntries::sequential((view_binding.clone(), depth.view(), post.source, env_binding.clone(), map_view, &pipeline.map_sampler)),
            );
            let attachment = RenderPassColorAttachment { view: post.destination, resolve_target: None, ops: Operations { load: LoadOp::Clear(Default::default()), store: StoreOp::Store } };
            let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor { label: Some("loom environment pass"), color_attachments: &[Some(attachment)], ..Default::default() });
            pass.set_pipeline(render_pipeline);
            pass.set_bind_group(0, &bind_group, &[view_offset.offset, env_index.index()]);
            pass.draw(0..3, 0..1);
        }
        Ok(())
    }
}

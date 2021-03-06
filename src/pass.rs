use amethyst::assets::{AssetStorage, Handle};
use amethyst::renderer::{
    batch::{GroupIterator, TwoLevelBatch},
    pass::Base3DPassDef,
    pipeline::{PipelineDescBuilder, PipelinesBuilder},
    pod::{SkinnedVertexArgs, VertexArgs},
    rendy::{
        command::{QueueId, RenderPassEncoder},
        factory::Factory,
        graph::{
            render::{PrepareResult, RenderGroup, RenderGroupDesc},
            GraphContext, NodeBuffer, NodeImage,
        },
        hal::{self, device::Device, format::Format, pso},
        mesh::{AsAttribute, AsVertex, VertexFormat},
        shader::{Shader, SpirvShader},
        util::types::vertex::{Normal, Position, Tangent},
    },
    resources::Tint,
    skinning::{JointCombined, JointTransforms},
    submodules::{DynamicVertexBuffer, EnvironmentSub, MaterialId, MaterialSub, SkinningSub},
    types::Backend,
    util,
};

use crate::{material::*, mesh::*};
use amethyst::core::{
    ecs::{Join, Read, ReadStorage, SystemData, World},
    transform::Transform,
};
use smallvec::SmallVec;
use std::marker::PhantomData;

#[derive(Clone, Derivative)]
#[derivative(Debug(bound = ""), Default(bound = ""))]
pub struct DrawVoxelDesc<B: Backend, D: Base3DPassDef> {
    marker: PhantomData<(B, D)>,
    skinning: bool,
    transparency: bool,
}

#[derive(Derivative)]
#[derivative(Debug(bound = ""))]
pub struct DrawVoxel<B: Backend, T: Base3DPassDef> {
    pipeline_basic: B::GraphicsPipeline,
    pipeline_skinned: Option<B::GraphicsPipeline>,
    pipeline_layout: B::PipelineLayout,
    static_batches: TwoLevelBatch<MaterialId, u32, SmallVec<[VertexArgs; 4]>>,
    skinned_batches: TwoLevelBatch<MaterialId, u32, SmallVec<[SkinnedVertexArgs; 4]>>,
    vertex_format_base: Vec<VertexFormat>,
    vertex_format_skinned: Vec<VertexFormat>,
    env: EnvironmentSub<B>,
    materials: MaterialSub<B, T::TextureSet>,
    skinning: SkinningSub<B>,
    models: DynamicVertexBuffer<B, VertexArgs>,
    skinned_models: DynamicVertexBuffer<B, SkinnedVertexArgs>,
    marker: PhantomData<T>,
    transparency: bool,
}

#[derive(Debug)]
pub struct VoxelPassDef<T: Base3DPassDef>(PhantomData<T>);

/// Type for combined texture coord and ambient occlusion attributes of vertex
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Surface {
    pub tex_ao: [f32; 3],
}

impl AsAttribute for Surface {
    const NAME: &'static str = "surface";
    const FORMAT: Format = Format::Rgb32Sfloat;
}

impl<B: Backend, T: Base3DPassDef> DrawVoxelDesc<B, T> {
    pub fn new(skinning: bool, transparency: bool) -> Self {
        Self {
            marker: PhantomData,
            skinning,
            transparency,
        }
    }
}

impl<'a, B, T> RenderGroupDesc<B, World> for DrawVoxelDesc<B, T>
where
    B: Backend,
    T: Base3DPassDef,
{
    fn build(
        self,
        _ctx: &GraphContext<B>,
        factory: &mut Factory<B>,
        _queue: QueueId,
        _aux: &World,
        framebuffer_width: u32,
        framebuffer_height: u32,
        subpass: hal::pass::Subpass<'_, B>,
        _buffers: Vec<NodeBuffer>,
        _images: Vec<NodeImage>,
    ) -> Result<Box<dyn RenderGroup<B, World>>, failure::Error> {
        let env = EnvironmentSub::new(
            factory,
            [
                hal::pso::ShaderStageFlags::VERTEX,
                hal::pso::ShaderStageFlags::FRAGMENT,
            ],
        )?;

        let materials = MaterialSub::new(factory)?;
        let skinning = SkinningSub::new(factory)?;

        let mut vertex_format_base = T::base_format();
        let mut vertex_format_skinned = T::skinned_format();

        let (mut pipelines, pipeline_layout) = build_pipelines::<B, T>(
            factory,
            subpass,
            framebuffer_width,
            framebuffer_height,
            &vertex_format_base,
            &vertex_format_skinned,
            self.skinning,
            self.transparency,
            vec![
                env.raw_layout(),
                materials.raw_layout(),
                skinning.raw_layout(),
            ],
        )?;

        vertex_format_base.sort();
        vertex_format_skinned.sort();

        Ok(Box::new(DrawVoxel::<B, T> {
            pipeline_basic: pipelines.remove(0),
            pipeline_skinned: pipelines.pop(),
            pipeline_layout,
            static_batches: Default::default(),
            skinned_batches: Default::default(),
            vertex_format_base,
            vertex_format_skinned,
            env,
            materials,
            skinning,
            models: DynamicVertexBuffer::new(),
            skinned_models: DynamicVertexBuffer::new(),
            marker: PhantomData,
            transparency: self.transparency,
        }))
    }
}

impl<T: Base3DPassDef> Base3DPassDef for VoxelPassDef<T> {
    const NAME: &'static str = "VoxelPass";
    type TextureSet = T::TextureSet;

    fn vertex_shader() -> &'static SpirvShader {
        &VOXEL_VERTEX
    }
    fn vertex_skinned_shader() -> &'static SpirvShader {
        &VOXEL_VERTEX_SKINNED
    }
    fn fragment_shader() -> &'static SpirvShader {
        T::fragment_shader()
    }
    fn base_format() -> Vec<VertexFormat> {
        vec![
            Position::vertex(),
            Normal::vertex(),
            Tangent::vertex(),
            Surface::vertex(),
        ]
    }
    fn skinned_format() -> Vec<VertexFormat> {
        vec![
            Position::vertex(),
            Normal::vertex(),
            Tangent::vertex(),
            Surface::vertex(),
            JointCombined::vertex(),
        ]
    }
}

impl<'a, B, T> RenderGroup<B, World> for DrawVoxel<B, T>
where
    B: Backend,
    T: Base3DPassDef,
{
    fn prepare(
        &mut self,
        factory: &Factory<B>,
        _queue: QueueId,
        index: usize,
        _subpass: hal::pass::Subpass<'_, B>,
        world: &World,
    ) -> PrepareResult {
        let (
            //visibility,
            mesh_storage,
            atlas_storage,
            meshes,
            transforms,
            joints,
            tints,
        ) = <(
            Read<'_, AssetStorage<VoxelMesh>>,
            Read<'_, AssetStorage<Atlas>>,
            ReadStorage<'_, Handle<VoxelMesh>>,
            ReadStorage<'_, Transform>,
            ReadStorage<'_, JointTransforms>,
            ReadStorage<'_, Tint>,
        )>::fetch(world);

        // Prepare environment
        self.env.process(factory, index, world);
        self.materials.maintain();

        self.static_batches.clear_inner();

        let materials_ref = &mut self.materials;
        let skinning_ref = &mut self.skinning;
        let statics_ref = &mut self.static_batches;
        let skinned_ref = &mut self.skinned_batches;
        let transparency = self.transparency;

        (&meshes, &transforms, tints.maybe(), !&joints)
            .join()
            .filter_map(|(mesh, tform, tint, _)| {
                if tint.map(|tint| tint.0.alpha < 1.0).unwrap_or(false) != transparency {
                    None
                } else {
                    Some((mesh.id(), VertexArgs::from_object_data(tform, tint)))
                }
            })
            .for_each_group(|mesh_id, data| {
                if let Some(mesh) = mesh_storage.get_by_id(mesh_id) {
                    if let Some(mat) = atlas_storage.get(&mesh.atlas) {
                        if let Some((mat, _)) = materials_ref.insert(factory, world, &mat.handle) {
                            statics_ref.insert(mat, mesh_id, data.drain(..));
                        }
                    }
                }
            });

        if self.pipeline_skinned.is_some() {
            (&meshes, &transforms, tints.maybe(), &joints)
                .join()
                .filter_map(|(mesh, tform, tint, joints)| {
                    if tint.map(|tint| tint.0.alpha < 1.0).unwrap_or(false) != transparency {
                        None
                    } else {
                        Some((
                            mesh.id(),
                            SkinnedVertexArgs::from_object_data(
                                tform,
                                tint,
                                skinning_ref.insert(joints),
                            ),
                        ))
                    }
                })
                .for_each_group(|mesh_id, data| {
                    if let Some(mesh) = mesh_storage.get_by_id(mesh_id) {
                        if let Some(mat) = atlas_storage.get(&mesh.atlas) {
                            if let Some((mat, _)) =
                                materials_ref.insert(factory, world, &mat.handle)
                            {
                                skinned_ref.insert(mat, mesh_id, data.drain(..));
                            }
                        }
                    }
                });
        }

        self.static_batches.prune();
        self.skinned_batches.prune();

        let static_changed = self.models.write(
            factory,
            index,
            self.static_batches.count() as u64,
            self.static_batches.data(),
        );
        let skinned_changed = self.skinned_models.write(
            factory,
            index,
            self.skinned_batches.count() as u64,
            self.skinned_batches.data(),
        );
        self.skinning.commit(factory, index);
        if static_changed || skinned_changed {
            PrepareResult::DrawRecord
        } else {
            PrepareResult::DrawReuse
        }
    }

    fn draw_inline(
        &mut self,
        mut encoder: RenderPassEncoder<'_, B>,
        index: usize,
        _subpass: hal::pass::Subpass<'_, B>,
        world: &World,
    ) {
        let mesh_storage = <Read<'_, AssetStorage<VoxelMesh>>>::fetch(world);
        let models_loc = self.vertex_format_base.len() as u32;

        encoder.bind_graphics_pipeline(&self.pipeline_basic);
        self.env.bind(index, &self.pipeline_layout, 0, &mut encoder);

        if self.models.bind(index, models_loc, 0, &mut encoder) {
            let mut instances_drawn = 0;
            for (&mat_id, batches) in self.static_batches.iter() {
                if self.materials.loaded(mat_id) {
                    self.materials
                        .bind(&self.pipeline_layout, 1, mat_id, &mut encoder);
                    for (mesh_id, batch_data) in batches {
                        if let Some(mesh) = unsafe {
                            mesh_storage
                                .get_by_id_unchecked(*mesh_id)
                                .inner
                                .as_ref()
                                .and_then(B::unwrap_mesh)
                        } {
                            mesh.bind_and_draw(
                                0,
                                &self.vertex_format_base,
                                instances_drawn..instances_drawn + batch_data.len() as u32,
                                &mut encoder,
                            )
                            .unwrap();
                        }
                        instances_drawn += batch_data.len() as u32;
                    }
                }
            }
        }
    }

    fn dispose(self: Box<Self>, factory: &mut Factory<B>, _aux: &World) {
        unsafe {
            factory
                .device()
                .destroy_graphics_pipeline(self.pipeline_basic);
            factory
                .device()
                .destroy_pipeline_layout(self.pipeline_layout);
        }
    }
}

lazy_static::lazy_static! {
    static ref VOXEL_VERTEX: SpirvShader = SpirvShader::from_bytes(
        include_bytes!("../compiled/voxels.vert.spv"),
        pso::ShaderStageFlags::VERTEX,
        "main",
    ).expect("Failed to load /compiled/voxels.vert.spv");

    static ref VOXEL_VERTEX_SKINNED: SpirvShader = SpirvShader::from_bytes(
        include_bytes!("../compiled/voxels_skinned.vert.spv"),
        pso::ShaderStageFlags::VERTEX,
        "main",
    ).expect("Failed to load /compiled/voxels.vert.spv");
}

#[allow(clippy::too_many_arguments)]
fn build_pipelines<B: Backend, T: Base3DPassDef>(
    factory: &Factory<B>,
    subpass: hal::pass::Subpass<'_, B>,
    framebuffer_width: u32,
    framebuffer_height: u32,
    vertex_format_base: &[VertexFormat],
    vertex_format_skinned: &[VertexFormat],
    skinning: bool,
    transparent: bool,
    layouts: Vec<&B::DescriptorSetLayout>,
) -> Result<(Vec<B::GraphicsPipeline>, B::PipelineLayout), failure::Error> {
    let pipeline_layout = unsafe {
        factory
            .device()
            .create_pipeline_layout(layouts, None as Option<(_, _)>)
    }?;

    let vertex_desc = vertex_format_base
        .iter()
        .map(|f| (f.clone(), pso::VertexInputRate::Vertex))
        .chain(Some((
            VertexArgs::vertex(),
            pso::VertexInputRate::Instance(1),
        )))
        .collect::<Vec<_>>();

    let shader_vertex_basic = unsafe { T::vertex_shader().module(factory).unwrap() };
    let shader_fragment = unsafe { T::fragment_shader().module(factory).unwrap() };
    let pipe_desc = PipelineDescBuilder::new()
        .with_vertex_desc(&vertex_desc)
        .with_shaders(util::simple_shader_set(
            &shader_vertex_basic,
            Some(&shader_fragment),
        ))
        .with_layout(&pipeline_layout)
        .with_subpass(subpass)
        .with_framebuffer_size(framebuffer_width, framebuffer_height)
        .with_face_culling(pso::Face::BACK)
        .with_depth_test(pso::DepthTest {
            fun: pso::Comparison::Less,
            write: !transparent,
        })
        .with_blend_targets(vec![pso::ColorBlendDesc {
            mask: pso::ColorMask::ALL,
            blend: if transparent {
                Some(pso::BlendState::PREMULTIPLIED_ALPHA)
            } else {
                None
            },
        }]);

    let pipelines = if skinning {
        let shader_vertex_skinned = unsafe { T::vertex_skinned_shader().module(factory).unwrap() };

        let vertex_desc = vertex_format_skinned
            .iter()
            .map(|f| (f.clone(), pso::VertexInputRate::Vertex))
            .chain(Some((
                SkinnedVertexArgs::vertex(),
                pso::VertexInputRate::Instance(1),
            )))
            .collect::<Vec<_>>();

        let pipe = PipelinesBuilder::new()
            .with_pipeline(pipe_desc.clone())
            .with_child_pipeline(
                0,
                pipe_desc
                    .with_vertex_desc(&vertex_desc)
                    .with_shaders(util::simple_shader_set(
                        &shader_vertex_skinned,
                        Some(&shader_fragment),
                    )),
            )
            .build(factory, None);

        unsafe {
            factory.destroy_shader_module(shader_vertex_skinned);
        }

        pipe
    } else {
        PipelinesBuilder::new()
            .with_pipeline(pipe_desc)
            .build(factory, None)
    };

    unsafe {
        factory.destroy_shader_module(shader_vertex_basic);
        factory.destroy_shader_module(shader_fragment);
    }

    match pipelines {
        Err(e) => {
            unsafe {
                factory.device().destroy_pipeline_layout(pipeline_layout);
            }
            Err(e)
        }
        Ok(pipelines) => Ok((pipelines, pipeline_layout)),
    }
}

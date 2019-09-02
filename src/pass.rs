use std::mem::replace;
use amethyst::renderer::{
    batch::{GroupIterator, TwoLevelBatch},
    mtl::{Material},
    pipeline::{PipelineDescBuilder, PipelinesBuilder},
    pass::{Base3DPassDef},
    pod::{SkinnedVertexArgs, VertexArgs},
    resources::Tint,
    submodules::{DynamicVertexBuffer, EnvironmentSub, MaterialId, MaterialSub, SkinningSub},
    types::{Backend, Mesh},
    util,
};
use amethyst::assets::{Handle};
use amethyst::core::{
    ecs::{Join, ReadStorage, WriteStorage, Resources, SystemData},
    transform::Transform,
};
use rendy::{
    command::{QueueId, RenderPassEncoder},
    factory::Factory,
    graph::{
        render::{PrepareResult, RenderGroup, RenderGroupDesc},
        GraphContext, NodeBuffer, NodeImage,
    },
    hal::{self, device::Device, pso},
    mesh::{AsVertex, VertexFormat, MeshBuilder},
    shader::{Shader},
};
use smallvec::SmallVec;
use std::marker::PhantomData;
use crate::{
    voxel::AsVoxel,
    coordinate::Pos,
    MutableVoxels,
};

/// Draw opaque 3d meshes with specified shaders and texture set
#[derive(Clone, Derivative)]
#[derivative(Debug(bound = ""), Default(bound = ""))]
pub struct DrawVoxelDesc<B: Backend, D: Base3DPassDef, V: AsVoxel> {
    marker: PhantomData<(B, D, V)>,
}

impl<B: Backend, T: Base3DPassDef, V: AsVoxel> DrawVoxelDesc<B, T, V> {
    pub fn new() -> Self {
        Self {
        	marker: PhantomData,
        }
    }
}

impl<B: Backend, T: Base3DPassDef, V: 'static +  AsVoxel> RenderGroupDesc<B, Resources> for DrawVoxelDesc<B, T, V> {
    fn build(
        self,
        _ctx: &GraphContext<B>,
        factory: &mut Factory<B>,
        _queue: QueueId,
        _aux: &Resources,
        framebuffer_width: u32,
        framebuffer_height: u32,
        subpass: hal::pass::Subpass<'_, B>,
        _buffers: Vec<NodeBuffer>,
        _images: Vec<NodeImage>,
    ) -> Result<Box<dyn RenderGroup<B, Resources>>, failure::Error> {
        let env = EnvironmentSub::new(factory)?;
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
            false,
            false,
            vec![
                env.raw_layout(),
                materials.raw_layout(),
                skinning.raw_layout(),
            ],
        )?;

        vertex_format_base.sort();
        vertex_format_skinned.sort();

        Ok(Box::new(DrawVoxel::<B, T, V> {
            pipeline_basic: pipelines.remove(0),
            pipeline_layout,
            static_batches: Default::default(),
            meshes: Vec::new(),
            vertex_format_base,
            vertex_format_skinned,
            env,
            materials,
            models: DynamicVertexBuffer::new(),
            marker: PhantomData,
        }))
    }
}

/// Base implementation of a 3D render pass which can be consumed by actual 3D render passes,
/// such as [pass::pbr::DrawPbr]
#[derive(Derivative)]
#[derivative(Debug(bound = ""))]
pub struct DrawVoxel<B: Backend, T: Base3DPassDef, V: AsVoxel> {
    pipeline_basic: B::GraphicsPipeline,
    pipeline_layout: B::PipelineLayout,
    static_batches: TwoLevelBatch<MaterialId, usize, SmallVec<[VertexArgs; 4]>>,
    meshes: Vec<Mesh>,
    vertex_format_base: Vec<VertexFormat>,
    vertex_format_skinned: Vec<VertexFormat>,
    env: EnvironmentSub<B>,
    materials: MaterialSub<B, T::TextureSet>,
    models: DynamicVertexBuffer<B, VertexArgs>,
    marker: PhantomData<(T, V)>,
}

impl<B: Backend, T: Base3DPassDef, V: 'static +  AsVoxel> RenderGroup<B, Resources> for DrawVoxel<B, T, V> {
    fn prepare(
        &mut self,
        factory: &Factory<B>,
        queue: QueueId,
        index: usize,
        _subpass: hal::pass::Subpass<'_, B>,
        resources: &Resources,
    ) -> PrepareResult {
        let (
            //visibility,
            mut meshes,
            materials,
            transforms,
            tints,
        ) = <(
            //ReadExpect<'_, Visibility>,
            WriteStorage<'_, MutableVoxels<V>>,
            ReadStorage<'_, Handle<Material>>,
            ReadStorage<'_, Transform>,
            ReadStorage<'_, Tint>,
        )>::fetch(resources);

        // Prepare environment
        self.env.process(factory, index, resources);
        self.materials.maintain();

        self.static_batches.clear_inner();

        let materials_ref = &mut self.materials;
        let statics_ref = &mut self.static_batches;
        let meshes_ref = &mut self.meshes;

        (&materials, &mut meshes, &transforms, tints.maybe())
            .join()
            .filter_map(|(mat, mesh, tform, tint)| {
                let id = match mesh.mesh {
                    Some(mesh_id) => mesh_id,
                    None => meshes_ref.len(),
                };

                if mesh.dirty {
                    let crate::triangulate::Mesh {
                        pos,
                        nml,
                        tex,
                        ind,
                    } = crate::triangulate::Mesh::build::<V>(&mesh.data, Pos::new(0.0, 0.0, 0.0), 16.0);

                    let new_mesh = B::wrap_mesh(MeshBuilder::new()
                        .with_indices(ind)
                        .with_vertices(pos)
                        .with_vertices(nml)
                        .with_vertices(tex)
                        .build(queue, factory)
                        .unwrap());

                    if id == meshes_ref.len() {
                        meshes_ref.push(new_mesh);
                    } else {
                        let _old_mesh = replace(&mut meshes_ref[id], new_mesh);
                        // todo: find out how to destroy the old mesh
                    }

                    mesh.mesh = Some(id);
                    mesh.dirty = false;
                }

                mesh.mesh.map(|id| {
                    ((mat, id), VertexArgs::from_object_data(tform, tint))
                })
            })
            .for_each_group(|(mat, id), data| {
                if let Some((mat, _)) = materials_ref.insert(factory, resources, mat) {
                    statics_ref.insert(mat, id, data.drain(..));
                }
            });

        self.static_batches.prune();
            
        self.models.write(
            factory,
            index,
            self.static_batches.count() as u64,
            self.static_batches.data(),
        );
        PrepareResult::DrawRecord
    }

    fn draw_inline(
        &mut self,
        mut encoder: RenderPassEncoder<'_, B>,
        index: usize,
        _subpass: hal::pass::Subpass<'_, B>,
        _resources: &Resources,
    ) {
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
                        if let Some(mesh) = B::unwrap_mesh(&self.meshes[*mesh_id])
                        {
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

    fn dispose(self: Box<Self>, factory: &mut Factory<B>, _aux: &Resources) {
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
        .with_depth_test(pso::DepthTest::On {
            fun: pso::Comparison::Less,
            write: !transparent,
        })
        .with_blend_targets(vec![pso::ColorBlendDesc(
            pso::ColorMask::ALL,
            if transparent {
                pso::BlendState::PREMULTIPLIED_ALPHA
            } else {
                pso::BlendState::Off
            },
        )]);

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

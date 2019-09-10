
use amethyst::renderer::{
    batch::{GroupIterator, TwoLevelBatch},
    pipeline::{PipelineDescBuilder, PipelinesBuilder},
    pass::{Base3DPassDef},
    pod::{SkinnedVertexArgs, VertexArgs},
    resources::Tint,
    submodules::{DynamicVertexBuffer, EnvironmentSub, MaterialId, MaterialSub, SkinningSub},
    types::{Backend, Mesh},
    util,
    skinning::{JointCombined},
};

use amethyst::core::{
    ecs::{Join, Read, ReadStorage, WriteStorage, Resources, SystemData},
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
    shader::{Shader, SpirvShader},
    util::types::vertex::{Position, Normal, Tangent, Color},
};
use smallvec::SmallVec;
use std::marker::PhantomData;
use crate::{
    voxel::{AsVoxel},
    context::{Context, VoxelContext},
    world::Chunk,
    coordinate::Pos,
    material::VoxelMaterialStorage,
    ambient_occlusion::*,
    MutableVoxelWorld,
    MutableVoxel,
};

#[derive(Clone, Derivative)]
#[derivative(Debug(bound = ""), Default(bound = ""))]
pub struct DrawVoxelDesc<B: Backend, D: Base3DPassDef, V: AsVoxel> {
    marker: PhantomData<(B, D, V)>,
}

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

#[derive(Debug)]
pub struct VoxelPassDef<T: Base3DPassDef>(PhantomData<T>);

impl<B: Backend, T: Base3DPassDef, V: AsVoxel> DrawVoxelDesc<B, T, V> {
    pub fn new() -> Self {
        Self {
        	marker: PhantomData,
        }
    }
}

impl<'a, B, T, V> RenderGroupDesc<B, Resources> for DrawVoxelDesc<B, T, V> where
    B: Backend,
    T: Base3DPassDef,
    V: 'static + AsVoxel,
    AmbientOcclusion<'a>: BuildAmbientOcclusion<'a, <V as AsVoxel>::Data, <V as AsVoxel>::Voxel>, 
{
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

impl<T: Base3DPassDef> Base3DPassDef for VoxelPassDef<T> {
    const NAME: &'static str = "Voxel";
    type TextureSet = T::TextureSet;

    fn vertex_shader() -> &'static SpirvShader {
        &VOXEL_VERTEX
    }
    fn vertex_skinned_shader() -> &'static SpirvShader {
        &VOXEL_VERTEX
    }
    fn fragment_shader() -> &'static SpirvShader {
        T::fragment_shader()
    }
    fn base_format() -> Vec<VertexFormat> {
        vec![
            Position::vertex(),
            Normal::vertex(),
            Tangent::vertex(),
            Color::vertex(),
        ]
    }
    fn skinned_format() -> Vec<VertexFormat> {
        vec![
            Position::vertex(),
            Normal::vertex(),
            Tangent::vertex(),
            Color::vertex(),
            JointCombined::vertex(),
        ]
    }
}

impl<'a, B, T, V> RenderGroup<B, Resources> for DrawVoxel<B, T, V> where
    B: Backend,
    T: Base3DPassDef,
    V: 'static + AsVoxel,
    AmbientOcclusion<'a>: BuildAmbientOcclusion<'a, <V as AsVoxel>::Data, <V as AsVoxel>::Voxel>, 
{
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
            mut worlds,
            materials,
            transforms,
            tints,
        ) = <(
            WriteStorage<'_, MutableVoxel<V>>,
            WriteStorage<'_, MutableVoxelWorld<V>>,
            Read<'_, VoxelMaterialStorage>,
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

        if let Some(mat) = materials.handle() {
            (&mut meshes, &transforms, tints.maybe())
                .join()
                .filter_map(|(mesh, tform, tint)| {
                    let id = match mesh.mesh {
                        Some(mesh_id) => mesh_id,
                        None => meshes_ref.len(),
                    };

                    if mesh.dirty {
                        let pos = Pos::new(0.0, 0.0, 0.0);
                        let scale = 16.0;
                        let new_mesh = build_mesh(&mesh, VoxelContext::new(&mesh.data), pos, scale, &materials, queue, factory);

                        if id == meshes_ref.len() {
                            meshes_ref.push(new_mesh);
                        } else {
                            meshes_ref[id] = new_mesh;
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

            (&mut worlds)
                .join()
                .flat_map(|world| {
                    for i in 0..world.data.len() {
                        let build_id = if let Some(chunk) = world.get_ready_chunk(i) {
                            if chunk.dirty {
                                chunk.mesh = match chunk.mesh {
                                    Some(id) => Some(id),
                                    None => Some(meshes_ref.len())
                                };
                                chunk.dirty = false;
                                chunk.mesh
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(id) = build_id {
                            let x = ((i) % world.dims[0]) as isize;
                            let y = ((i / (world.dims[0])) % world.dims[1]) as isize;
                            let z = ((i / (world.dims[0]*world.dims[1])) % world.dims[2]) as isize;
                            let scale = world.scale;
                            let pos = Pos::new(
                                (x + world.origin[0]) as f32 * scale, 
                                (y + world.origin[1]) as f32 * scale, 
                                (z + world.origin[2]) as f32 * scale
                            );
                            let chunk = world.data[i].get().unwrap();
                            let context = world.context([x,y,z]);
                            let new_mesh = build_mesh(chunk, context, pos, scale, &materials, queue, factory);
                            if id == meshes_ref.len() {
                                meshes_ref.push(new_mesh);
                            } else {
                                meshes_ref[id] = new_mesh;
                            }
                        }
                    }

                    world.data.iter().filter_map(|chunk| match chunk {
                        Chunk::Ready(chunk) => chunk.mesh.map(|id| {
                            ((mat, id), VertexArgs::from_object_data(&Transform::default(), None))
                        }),
                        _ => None,
                    })
                })
                .for_each_group(|(mat, id), data| {
                    if let Some((mat, _)) = materials_ref.insert(factory, resources, mat) {
                        statics_ref.insert(mat, id, data.drain(..));
                    }
                });
        }

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

lazy_static::lazy_static! {
    static ref VOXEL_VERTEX: SpirvShader = SpirvShader::new(
        include_bytes!("../compiled/voxels.vert.spv").to_vec(),
        pso::ShaderStageFlags::VERTEX,
        "main",
    );
}

fn build_mesh<'a, B, V, C>(
    voxel: &MutableVoxel<V>, 
    context: C, 
    pos: Pos, 
    scale: f32, 
    materials: &VoxelMaterialStorage,
    queue: QueueId, 
    factory: &Factory<B>
) -> Mesh where
    B: Backend, 
    V: AsVoxel, 
    C: Context,
    AmbientOcclusion<'a>: BuildAmbientOcclusion<'a, <V as AsVoxel>::Data, <V as AsVoxel>::Voxel>
{
    let ao = AmbientOcclusion::build(&voxel.data, &context);

    let crate::triangulate::Mesh {
        pos, nml, tan, tex, ind,
    } = crate::triangulate::Mesh::build::<V, C>(&voxel.data, &ao, &context, pos, scale);

    let tex: Vec<_> = tex.into_iter().map(|(mat, ao)| materials.coord(mat, ao)).collect();

    B::wrap_mesh(MeshBuilder::new()
        .with_vertices(pos).with_vertices(nml).with_vertices(tan).with_vertices(tex)
        .with_indices(ind).build(queue, factory).unwrap())
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

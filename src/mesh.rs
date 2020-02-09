use amethyst::{
    assets::*,
    core::{ArcThreadPool, Time},
    ecs::prelude::*,
    renderer::{
        batch::{GroupIterator, TwoLevelBatch},
        pass::Base3DPassDef,
        pipeline::{PipelineDescBuilder, PipelinesBuilder},
        pod::{SkinnedVertexArgs, VertexArgs},
        rendy::{command::QueueId, factory::Factory, mesh::MeshBuilder},
        resources::Tint,
        skinning::JointCombined,
        submodules::{DynamicVertexBuffer, EnvironmentSub, MaterialId, MaterialSub, SkinningSub},
        types::{Backend, Mesh},
        util,
    },
};

use nalgebra_glm::*;

use crate::ambient_occlusion::*;
use crate::context::*;
use crate::material::*;
use crate::model::*;
use crate::pass::*;
use crate::voxel::{Data, Voxel};

use std::marker::PhantomData;

pub struct VoxelMesh {
    pub(crate) inner: Option<Mesh>,
}

pub struct VoxelMeshProcessorSystem<B: Backend, V: Data>(PhantomData<(B, V)>);

impl Asset for VoxelMesh {
    const NAME: &'static str = "VoxelMesh";
    type Data = ModelData;
    type HandleStorage = DenseVecStorage<Handle<Self>>;
}

#[derive(SystemData)]
pub struct VoxelMeshProcessorSystemData<'a, B: Backend> {
    mesh_storage: Write<'a, AssetStorage<VoxelMesh>>,
    queue_id: ReadExpect<'a, QueueId>,
    time: Read<'a, Time>,
    pool: ReadExpect<'a, ArcThreadPool>,
    strategy: Option<Read<'a, HotReloadStrategy>>,
    factory: ReadExpect<'a, Factory<B>>,
    material_storage: WriteExpect<'a, VoxelMaterialStorage>,
}

impl<'a, B: Backend, V: Data + Default> System<'a> for VoxelMeshProcessorSystem<B, V> {
    type SystemData = VoxelMeshProcessorSystemData<'a, B>;

    fn run(&mut self, data: Self::SystemData) {
        use std::ops::Deref;

        let VoxelMeshProcessorSystemData {
            mut mesh_storage,
            queue_id,
            time,
            pool,
            strategy,
            factory,
            mut material_storage,
        } = data;

        mesh_storage.process(
            |b| {
                let materials: Vec<_> = b
                    .materials
                    .iter()
                    .map(|m| material_storage.create(m.clone()))
                    .collect();

                let mut voxel =
                    Voxel::<V>::from_iter(Default::default(), std::iter::repeat(Voxel::default()));

                for (index, material) in b.voxels {
                    let x = index % b.dimensions[0];
                    let y = (index / (b.dimensions[0] * b.dimensions[1])) % b.dimensions[2];
                    let z = (index / b.dimensions[0]) % b.dimensions[1];

                    if x < Voxel::<V>::WIDTH && y < Voxel::<V>::WIDTH && z < Voxel::<V>::WIDTH {
                        if let Some(sub) = voxel.get_mut(Voxel::<V>::coord_to_index(x, y, z)) {
                            std::mem::replace(
                                sub,
                                Voxel::filled(Default::default(), materials[material]),
                            );
                        }
                    }
                }

                Ok(ProcessingState::Loaded(VoxelMesh {
                    inner: build_mesh(
                        &voxel,
                        VoxelContext::new(&voxel),
                        vec3(0.0, 0.0, 0.0),
                        32.0,
                        &material_storage,
                        *queue_id,
                        &factory,
                    ),
                }))
            },
            time.frame_number(),
            &**pool,
            strategy.as_ref().map(Deref::deref),
        );
    }
}

fn build_mesh<B, V, C>(
    voxel: &Voxel<V>,
    context: C,
    pos: Vec3,
    scale: f32,
    materials: &VoxelMaterialStorage,
    queue: QueueId,
    factory: &Factory<B>,
) -> Option<Mesh>
where
    B: Backend,
    V: Data,
    C: Context<V>,
{
    let ao = AmbientOcclusion::build(voxel, &context);

    let crate::triangulate::Mesh {
        pos,
        nml,
        tan,
        tex,
        ind,
    } = crate::triangulate::Mesh::build::<V, C>(voxel, &ao, &context, pos, scale);

    let tex: Vec<_> = tex
        .into_iter()
        .map(|texturing| {
            let [u, v] = materials.coord(texturing.material_id, texturing.side, texturing.coord);
            Surface {
                tex_ao: [u, v, texturing.ao],
            }
        })
        .collect();

    if !pos.is_empty() {
        Some(B::wrap_mesh(
            MeshBuilder::new()
                .with_vertices(pos)
                .with_vertices(nml)
                .with_vertices(tan)
                .with_vertices(tex)
                .with_indices(ind)
                .build(queue, factory)
                .unwrap(),
        ))
    } else {
        None
    }
}

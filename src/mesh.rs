use amethyst::{
    assets::*,
    core::{ArcThreadPool, Time},
    ecs::prelude::*,
    renderer::{
        rendy::{command::QueueId, factory::Factory, mesh::MeshBuilder},
        types::{Backend, Mesh},
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
use std::ops::{Deref, DerefMut};

/// Asset for voxelmesh rendering
pub struct VoxelMesh {
    pub(crate) inner: Option<Mesh>,
}

/// A component that manages a dynamic voxelmesh
pub struct DynamicVoxelMesh<T: Data> {
    pub(crate) data: Voxel<T>,
    pub(crate) origin: Vec3,
    pub(crate) parent: Option<Entity>,
    pub(crate) dirty: bool,
}

pub struct VoxelMeshProcessorSystem<B: Backend, V: Data + Default>(PhantomData<(B, V)>);

#[derive(SystemData)]
pub struct VoxelMeshProcessorSystemData<'a, B: Backend, V: Data> {
    mesh_storage: Write<'a, AssetStorage<VoxelMesh>>,
    dynamic_mesh_storage: WriteStorage<'a, DynamicVoxelMesh<V>>,
    handle_storage: WriteStorage<'a, Handle<VoxelMesh>>,
    entities: Entities<'a>,
    queue_id: ReadExpect<'a, QueueId>,
    time: Read<'a, Time>,
    pool: ReadExpect<'a, ArcThreadPool>,
    strategy: Option<Read<'a, HotReloadStrategy>>,
    factory: ReadExpect<'a, Factory<B>>,
    material_storage: WriteExpect<'a, VoxelMaterialStorage>,
}

impl Asset for VoxelMesh {
    const NAME: &'static str = "VoxelMesh";
    type Data = ModelData;
    type HandleStorage = DenseVecStorage<Handle<Self>>;
}

impl<T: Data> Component for DynamicVoxelMesh<T> {
    type Storage = DenseVecStorage<Self>;
}

impl<T: Data> DynamicVoxelMesh<T> {
    /// Create a new `DynamicVoxelMesh` component.
    pub fn new(value: Voxel<T>) -> Self {
        DynamicVoxelMesh {
            data: value,
            origin: vec3(0.0, 0.0, 0.0),
            parent: None,
            dirty: true,
        }
    }

    /// Create a new `VoxelRender` component with a new `Voxel<T>` created from an iterator.
    pub fn from_iter<I>(data: T, iter: I) -> Self
    where
        I: IntoIterator<Item = Voxel<T>>,
    {
        DynamicVoxelMesh {
            data: Voxel::from_iter(data, iter),
            origin: vec3(0.0, 0.0, 0.0),
            parent: None,
            dirty: true,
        }
    }
}

impl<T: Data> Deref for DynamicVoxelMesh<T> {
    type Target = Voxel<T>;

    fn deref(&self) -> &Voxel<T> {
        &self.data
    }
}

impl<T: Data> DerefMut for DynamicVoxelMesh<T> {
    fn deref_mut(&mut self) -> &mut Voxel<T> {
        self.dirty = true;
        &mut self.data
    }
}

impl<B: Backend, V: Data + Default> VoxelMeshProcessorSystem<B, V> {
    pub fn new() -> Self {
        VoxelMeshProcessorSystem(PhantomData)
    }
}

impl<'a, B: Backend, V: Data + Default> System<'a> for VoxelMeshProcessorSystem<B, V> {
    type SystemData = VoxelMeshProcessorSystemData<'a, B, V>;

    fn run(&mut self, data: Self::SystemData) {
        let VoxelMeshProcessorSystemData {
            mut mesh_storage,
            mut dynamic_mesh_storage,
            mut handle_storage,
            entities,
            queue_id,
            time,
            pool,
            strategy,
            factory,
            mut material_storage,
        } = data;

        let new_meshes = (&entities, &mut dynamic_mesh_storage)
            .par_join()
            .filter_map(|(e, dynamic_mesh)| {
                if dynamic_mesh.dirty {
                    // triangulate the mesh
                    let voxel_mesh = VoxelMesh {
                        inner: build_mesh(
                            &dynamic_mesh.data,
                            VoxelContext::new(&dynamic_mesh.data),
                            dynamic_mesh.origin.clone(),
                            (Voxel::<V>::WIDTH) as f32,
                            &material_storage,
                            *queue_id,
                            &factory,
                        ),
                    };

                    // clear the dirty flag
                    dynamic_mesh.dirty = false;

                    Some((e, voxel_mesh))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for (e, voxel_mesh) in new_meshes {
            // create a mesh handle for the voxelmesh we just created.
            // the handle is picked up by the rendering system.
            let handle = mesh_storage.insert(voxel_mesh);

            // add the handle to the entity
            handle_storage.insert(e, handle.clone()).ok();
        }

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

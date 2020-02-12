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
use crate::world::VoxelWorld;

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
    pub(crate) scale: f32,
    pub(crate) parent: Option<(Entity, [isize; 3])>,
    pub(crate) dirty: bool,
}

pub struct VoxelMeshProcessorSystem<B: Backend, V: Data + Default> {
    triangulation_limit: usize,
    marker: PhantomData<(B, V)>,
}

#[derive(SystemData)]
pub struct VoxelMeshProcessorSystemData<'a, B: Backend, V: Data> {
    mesh_storage: Write<'a, AssetStorage<VoxelMesh>>,
    dynamic_mesh_storage: WriteStorage<'a, DynamicVoxelMesh<V>>,
    handle_storage: WriteStorage<'a, Handle<VoxelMesh>>,
    world_storage: ReadStorage<'a, VoxelWorld<V>>,
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
            scale: Voxel::<T>::WIDTH as f32,
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
            scale: Voxel::<T>::WIDTH as f32,
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
    pub fn new(triangulation_limit: usize) -> Self {
        VoxelMeshProcessorSystem {
            triangulation_limit,
            marker: PhantomData,
        }
    }
}

impl<'a, B: Backend, V: Data + Default> System<'a> for VoxelMeshProcessorSystem<B, V> {
    type SystemData = VoxelMeshProcessorSystemData<'a, B, V>;

    fn run(&mut self, mut data: Self::SystemData) {
        let dirty_meshes = (&data.entities, &mut data.dynamic_mesh_storage)
            .join()
            .filter_map(|(e, dynamic_mesh)| {
                if dynamic_mesh.dirty {
                    dynamic_mesh.dirty = false;
                    Some(e)
                } else {
                    None
                }
            })
            .take(self.triangulation_limit)
            .collect::<Vec<_>>();

        for dirty in dirty_meshes {
            let dynamic_mesh = data.dynamic_mesh_storage.get(dirty).unwrap();
            // triangulate the mesh
            let mesh = dynamic_mesh
                .parent
                .map(|(world, coord)| {
                    let world = data
                        .world_storage
                        .get(world)
                        .expect("DynamicVoxelMesh parent invalid");
                    build_mesh(
                        &dynamic_mesh.data,
                        WorldContext::new(coord, world, &data.dynamic_mesh_storage),
                        dynamic_mesh.origin.clone(),
                        dynamic_mesh.scale,
                        &data.material_storage,
                        *data.queue_id,
                        &data.factory,
                    )
                })
                .unwrap_or_else(|| {
                    build_mesh(
                        &dynamic_mesh.data,
                        VoxelContext::new(&dynamic_mesh.data),
                        dynamic_mesh.origin.clone(),
                        dynamic_mesh.scale,
                        &data.material_storage,
                        *data.queue_id,
                        &data.factory,
                    )
                });

            // create a mesh handle for the voxelmesh we just created.
            // the handle is picked up by the rendering system.
            let handle = data.mesh_storage.insert(VoxelMesh { inner: mesh });

            // add the handle to the entity
            data.handle_storage.insert(dirty, handle.clone()).ok();
        }

        data.mesh_storage.process(
            {
                let material_storage = &mut data.material_storage;
                let queue_id = &data.queue_id;
                let factory = &data.factory;
                move |b| {
                    let materials: Vec<_> = b
                        .materials
                        .iter()
                        .map(|m| material_storage.create(m.clone()))
                        .collect();

                    let mut voxel = Voxel::<V>::from_iter(
                        Default::default(),
                        std::iter::repeat(Voxel::default()),
                    );

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
                            Voxel::<V>::WIDTH as f32,
                            material_storage,
                            **queue_id,
                            factory,
                        ),
                    }))
                }
            },
            data.time.frame_number(),
            &**data.pool,
            data.strategy.as_ref().map(Deref::deref),
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

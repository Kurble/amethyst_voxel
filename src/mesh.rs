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

use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// Asset for voxelmesh rendering
pub struct VoxelMesh {
    pub(crate) inner: Option<Mesh>,
    pub(crate) atlas: Handle<Atlas>,
}

/// A component that manages a dynamic voxelmesh
pub struct DynamicVoxelMesh<T: Data> {
    pub(crate) data: Voxel<T>,
    pub(crate) atlas: Handle<Atlas>,
    pub(crate) origin: Vec3,
    pub(crate) scale: f32,
    pub(crate) parent: Option<(Entity, [isize; 3])>,
    pub(crate) dirty: bool,
}

pub struct DynamicVoxelMeshData<T: Data> {
    pub data: Voxel<T>,
    pub atlas: Handle<Atlas>,
}

pub struct TriangulatorSystem<B: Backend, V: Data + Default> {
    triangulation_limit: usize,
    marker: PhantomData<(B, V)>,
}

pub struct VoxelMeshProcessor<B: Backend, V: Data + Default> {
    marker: PhantomData<(B, V)>,
}

#[derive(SystemData)]
pub struct TriangulatorSystemData<'a, B: Backend, V: Data> {
    mesh_storage: Write<'a, AssetStorage<VoxelMesh>>,
    dynamic_mesh_storage: WriteStorage<'a, DynamicVoxelMesh<V>>,
    handle_storage: WriteStorage<'a, Handle<VoxelMesh>>,
    atlas_handle_storage: ReadStorage<'a, Handle<Atlas>>,
    world_storage: ReadStorage<'a, VoxelWorld<V>>,
    entities: Entities<'a>,
    queue_id: ReadExpect<'a, QueueId>,
    factory: ReadExpect<'a, Factory<B>>,
    atlas_storage: Read<'a, AssetStorage<Atlas>>,
}

#[derive(SystemData)]
pub struct VoxelMeshProcessorData<'a, B: Backend, V: Data> {
    mesh_storage: Write<'a, AssetStorage<VoxelMesh>>,
    voxel_storage: Write<'a, AssetStorage<DynamicVoxelMeshData<V>>>,
    atlas_storage: Read<'a, AssetStorage<Atlas>>,
    loader: ReadExpect<'a, Loader>,
    queue_id: ReadExpect<'a, QueueId>,
    time: Read<'a, Time>,
    pool: ReadExpect<'a, ArcThreadPool>,
    strategy: Option<Read<'a, HotReloadStrategy>>,
    factory: ReadExpect<'a, Factory<B>>,
}

impl Asset for VoxelMesh {
    const NAME: &'static str = "VoxelMesh";
    type Data = ModelData;
    type HandleStorage = DenseVecStorage<Handle<Self>>;
}

impl<T: Data> Asset for DynamicVoxelMeshData<T> {
    const NAME: &'static str = "DynamicVoxelMesh";
    type Data = ModelData;
    type HandleStorage = DenseVecStorage<Handle<Self>>;
}

impl<T: Data> Component for DynamicVoxelMesh<T> {
    type Storage = DenseVecStorage<Self>;
}

impl<T: Data> DynamicVoxelMesh<T> {
    /// Create a new `DynamicVoxelMesh` component.
    pub fn new(value: Voxel<T>, atlas: Handle<Atlas>) -> Self {
        DynamicVoxelMesh {
            data: value,
            atlas,
            origin: vec3(0.0, 0.0, 0.0),
            scale: Voxel::<T>::WIDTH as f32,
            parent: None,
            dirty: true,
        }
    }

    /// Create a new `VoxelRender` component with a new `Voxel<T>` created from an iterator.
    pub fn from_iter<I>(data: T, atlas: Handle<Atlas>, iter: I) -> Self
    where
        I: IntoIterator<Item = Voxel<T>>,
    {
        DynamicVoxelMesh {
            data: Voxel::from_iter(data, iter),
            atlas,
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

impl<B: Backend, V: Data + Default> TriangulatorSystem<B, V> {
    pub fn new(triangulation_limit: usize) -> Self {
        TriangulatorSystem {
            triangulation_limit,
            marker: PhantomData,
        }
    }
}

impl<'a, B: Backend, V: Data + Default> System<'a> for TriangulatorSystem<B, V> {
    type SystemData = TriangulatorSystemData<'a, B, V>;

    fn run(&mut self, mut data: Self::SystemData) {
        let dirty_meshes = (
            &data.entities,
            &data.atlas_handle_storage,
            &mut data.dynamic_mesh_storage,
        )
            .join()
            .filter_map({
                let atlas_storage = &data.atlas_storage;
                move |(e, a, dynamic_mesh)| {
                    if dynamic_mesh.dirty && atlas_storage.contains(a) {
                        dynamic_mesh.dirty = false;
                        Some(e)
                    } else {
                        None
                    }
                }
            })
            .take(self.triangulation_limit)
            .collect::<Vec<_>>();

        for dirty in dirty_meshes {
            let dynamic_mesh = data.dynamic_mesh_storage.get(dirty).unwrap();
            let atlas = data.atlas_storage.get(&dynamic_mesh.atlas).unwrap();
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
                        atlas,
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
                        atlas,
                        *data.queue_id,
                        &data.factory,
                    )
                });

            // create a mesh handle for the voxelmesh we just created.
            // the handle is picked up by the rendering system.
            let handle = data.mesh_storage.insert(VoxelMesh {
                inner: mesh,
                atlas: dynamic_mesh.atlas.clone(),
            });

            // add the handle to the entity
            data.handle_storage.insert(dirty, handle.clone()).ok();
        }
    }
}

impl<B: Backend, V: Data + Default> VoxelMeshProcessor<B, V> {
    pub fn new() -> Self {
        VoxelMeshProcessor {
            marker: PhantomData,
        }
    }
}

impl<'a, B: Backend, V: Data + Default> System<'a> for VoxelMeshProcessor<B, V> {
    type SystemData = VoxelMeshProcessorData<'a, B, V>;

    fn run(&mut self, mut data: Self::SystemData) {
        data.voxel_storage.process(
            {
                let loader = &data.loader;
                let atlas_storage = &data.atlas_storage;
                move |model| {
                    let mut atlas = AtlasData::default();
                    let data = build_voxel::<V>(model, &mut atlas);
                    let atlas = loader.load_from_data(atlas, (), atlas_storage);
                    Ok(ProcessingState::Loaded(DynamicVoxelMeshData {
                        data,
                        atlas,
                    }))
                }
            },
            data.time.frame_number(),
            &**data.pool,
            data.strategy.as_ref().map(Deref::deref),
        );

        data.mesh_storage.process(
            {
                let queue_id = &data.queue_id;
                let factory = &data.factory;
                let loader = &data.loader;
                let atlas_storage = &data.atlas_storage;
                move |model| {
                    let mut atlas = AtlasData::default();

                    let voxel = build_voxel::<V>(model, &mut atlas);

                    Ok(ProcessingState::Loaded(VoxelMesh {
                        inner: build_mesh(
                            &voxel,
                            VoxelContext::new(&voxel),
                            vec3(0.0, 0.0, 0.0),
                            1.0,
                            &atlas,
                            **queue_id,
                            factory,
                        ),
                        atlas: loader.load_from_data(atlas, (), atlas_storage),
                    }))
                }
            },
            data.time.frame_number(),
            &**data.pool,
            data.strategy.as_ref().map(Deref::deref),
        );
    }
}

fn build_voxel<V>(model: ModelData, atlas: &mut AtlasData) -> Voxel<V>
where
    V: Data + Default,
{
    let mut materials_map = HashMap::new();

    let voxels = model
        .voxels
        .iter()
        .map(|(index, material)| {
            (
                *index,
                materials_map
                    .entry(material)
                    .or_insert_with(|| atlas.create_without_id(model.materials[*material].clone()))
                    .clone(),
            )
        })
        .collect::<Vec<(usize, AtlasMaterialHandle)>>();

    let mut voxel = Voxel::<V>::from_iter(Default::default(), std::iter::repeat(Voxel::default()));

    for (index, material) in voxels {
        let x = index % model.dimensions[0];
        let y = (index / (model.dimensions[0] * model.dimensions[1])) % model.dimensions[2];
        let z = (index / model.dimensions[0]) % model.dimensions[1];

        if x < Voxel::<V>::WIDTH && y < Voxel::<V>::WIDTH && z < Voxel::<V>::WIDTH {
            if let Some(sub) = voxel.get_mut(Voxel::<V>::coord_to_index(x, y, z)) {
                std::mem::replace(sub, Voxel::filled(Default::default(), material));
            }
        }
    }

    voxel
}

fn build_mesh<B, V, C, A>(
    voxel: &Voxel<V>,
    context: C,
    pos: Vec3,
    scale: f32,
    atlas: &A,
    queue: QueueId,
    factory: &Factory<B>,
) -> Option<Mesh>
where
    B: Backend,
    V: Data,
    C: Context<V>,
    A: AtlasAccess,
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
            let [u, v] = atlas.coord(texturing.material_id, texturing.side, texturing.coord);
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

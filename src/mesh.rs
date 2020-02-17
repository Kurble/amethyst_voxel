use amethyst::{
    assets::*,
    core::{ArcThreadPool, Time},
    ecs::prelude::*,
    renderer::{
        rendy::{command::QueueId, factory::Factory, mesh::MeshBuilder},
        types::Backend,
    },
};

use nalgebra_glm::*;

use crate::ambient_occlusion::*;
use crate::context::*;
use crate::material::*;
use crate::model::*;
use crate::pass::*;
use crate::triangulate::Mesh;
use crate::voxel::{Data, Voxel};
use crate::world::VoxelWorld;

use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// Asset for voxelmesh rendering
pub struct VoxelMesh {
    pub(crate) inner: Option<amethyst::renderer::types::Mesh>,
    pub(crate) atlas: Handle<Atlas>,
}

/// A component that manages a dynamic voxelmesh
pub struct DynamicVoxelMesh<T: Data> {
    pub(crate) data: Voxel<T>,
    pub(crate) atlas: Handle<Atlas>,
    pub(crate) transform: Mat4x4,
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
            transform: Mat4x4::identity().scale(Voxel::<T>::WIDTH as f32),
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
            transform: Mat4x4::identity().scale(Voxel::<T>::WIDTH as f32),
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
        let dirty_meshes = (&data.entities, &mut data.dynamic_mesh_storage)
            .join()
            .filter_map({
                let atlas_storage = &data.atlas_storage;
                move |(e, dynamic_mesh)| {
                    if dynamic_mesh.dirty && atlas_storage.contains(&dynamic_mesh.atlas) {
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
                        Some((
                            &dynamic_mesh.data,
                            WorldContext::new(coord, world, &data.dynamic_mesh_storage),
                            &dynamic_mesh.transform,
                        )),
                        atlas,
                        *data.queue_id,
                        &data.factory,
                    )
                })
                .unwrap_or_else(|| {
                    build_mesh(
                        Some((
                            &dynamic_mesh.data,
                            VoxelContext::new(&dynamic_mesh.data),
                            &dynamic_mesh.transform,
                        )),
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
                    let data = build_voxel::<V>(&model, &model.submodels[0], &mut atlas);
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

                    let voxels = model
                        .submodels
                        .iter()
                        .map(|sub| (sub, build_voxel::<V>(&model, sub, &mut atlas)))
                        .collect::<Vec<_>>();

                    let mesh = build_mesh(
                        voxels
                            .iter()
                            .map(|(sub, voxel)| (voxel, VoxelContext::new(voxel), &sub.offset)),
                        &atlas,
                        **queue_id,
                        factory,
                    );

                    Ok(ProcessingState::Loaded(VoxelMesh {
                        inner: mesh,
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

fn build_voxel<V>(model: &ModelData, submodel: &SubModelData, atlas: &mut AtlasData) -> Voxel<V>
where
    V: Data + Default,
{
    let mut materials_map = HashMap::new();

    let voxels = submodel
        .voxels
        .iter()
        .map(|instance| {
            (
                instance.index,
                materials_map
                    .entry(instance.material)
                    .or_insert_with(|| {
                        atlas.create_without_id(model.materials[instance.material].clone())
                    })
                    .clone(),
            )
        })
        .collect::<Vec<(usize, AtlasMaterialHandle)>>();

    let mut voxel = Voxel::<V>::from_iter(Default::default(), std::iter::repeat(Voxel::default()));

    for (index, material) in voxels {
        let x = index % submodel.dimensions[0];
        let y =
            (index / (submodel.dimensions[0] * submodel.dimensions[1])) % submodel.dimensions[2];
        let z = (index / submodel.dimensions[0]) % submodel.dimensions[1];

        if x < Voxel::<V>::WIDTH && y < Voxel::<V>::WIDTH && z < Voxel::<V>::WIDTH {
            if let Some(sub) = voxel.get_mut(Voxel::<V>::coord_to_index(x, y, z)) {
                std::mem::replace(sub, Voxel::filled(Default::default(), material));
            }
        }
    }

    voxel
}

fn build_mesh<'a, B, V, C, A, I>(
    iter: I,
    atlas: &A,
    queue: QueueId,
    factory: &Factory<B>,
) -> Option<amethyst::renderer::types::Mesh>
where
    B: Backend,
    V: Data,
    C: Context<V>,
    A: AtlasAccess,
    I: IntoIterator<Item = (&'a Voxel<V>, C, &'a Mat4x4)>,
{
    let mut mesh = Mesh::default();

    for (voxel, context, transform) in iter {
        let ao = AmbientOcclusion::build(voxel, &context);
        let start = mesh.pos.len();
        mesh.build::<V, C>(voxel, &ao, &context, vec3(0.0, 0.0, 0.0), 1.0);
        for i in start..mesh.pos.len() {
            let pos: [f32; 3] = mesh.pos[i].0.into();
            let nml: [f32; 3] = mesh.nml[i].0.into();
            let tan: [f32; 3] = [mesh.tan[i].0[0], mesh.tan[i].0[1], mesh.tan[i].0[2]];
            mesh.pos[i] = transform.transform_point(&pos.into()).coords.into();
            mesh.nml[i] = transform.transform_vector(&nml.into()).into();
            let tan = transform.transform_vector(&tan.into());
            mesh.tan[i] = [tan[0], tan[1], tan[2], mesh.tan[i].0[3]].into();
        }
    }

    let Mesh {
        pos,
        nml,
        tan,
        tex,
        ind,
    } = mesh;

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

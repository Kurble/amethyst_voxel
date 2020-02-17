use crate::material::Atlas;
use crate::mesh::*;
use crate::voxel::*;

use amethyst::{
    assets::Handle,
    core::{
        ecs::storage::{GenericReadStorage, GenericWriteStorage},
        transform::Transform,
    },
    ecs::prelude::*,
    renderer::{ActiveCamera, Camera},
};
use crossbeam::atomic::AtomicCell;
use nalgebra_glm::*;
use rayon::ThreadPool;

use std::marker::PhantomData;
use std::mem::replace;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// A dynamically loaded infinite world component.
/// Voxel data is pulled from a VoxelSource component on the same entity.
/// The voxel world has a rendering range around the viewpoint of the current camera, which is automatically updated.
pub struct VoxelWorld<T: Data> {
    loaded: bool,
    limits: Limits,
    visibility: [f32; 6],
    view_range: f32,
    atlas: Handle<Atlas>,
    pub(crate) data: Vec<Chunk<T>>,
    pub(crate) dims: [usize; 3],
    pub(crate) origin: [isize; 3],
    pub(crate) scale: f32,
}

/// Utility struct for accessing `Voxel`s in a `VoxelWorld`.
pub struct VoxelWorldAccess<'a, 'b, T: Data> {
    pub world: &'a VoxelWorld<T>,
    pub chunks: &'a mut WriteStorage<'b, DynamicVoxelMesh<T>>,
}

pub enum VoxelSourceResult<T: Data> {
    Ok(Voxel<T>),
    Loading(Box<dyn FnOnce() -> Voxel<T> + Send>),
    Retry,
}

/// Voxel data source for `VoxelWorld`
pub trait VoxelSource<'s, T: Data>: Send + Sync {
    type SystemData: SystemData<'s>;

    /// Load chunk at the specified chunk coordinate.
    /// After this the returned FnOnce will be run on a background thread to get the final result.
    fn load_voxel(
        &mut self,
        system_data: &mut Self::SystemData,
        coord: [isize; 3],
    ) -> VoxelSourceResult<T>;

    /// When a chunk is removed from the `VoxelWorld`, some sources might want to persist the changes made
    /// to the voxel. When a chunk is removed, this function will be called dispose of the chunk properly.
    fn drop_voxel(
        &mut self,
        _system_data: &mut Self::SystemData,
        _coord: [isize; 3],
        _voxel: Voxel<T>,
    ) -> Box<dyn FnOnce() + Send> {
        Box::new(|| ())
    }

    /// Retrieve the limits in chunks that this VoxelSource can generate.
    /// Chunks that have neighbours according to the limits, but have no neighbours in the `VoxelWorld`
    /// will not be rendered to ensure that rendering glitches don't occur.
    fn limits(&self) -> Limits;
}

pub struct WorldSystem<T: Data, S: for<'s> VoxelSource<'s, T>> {
    pool: Arc<ThreadPool>,
    marker: PhantomData<(T, S)>,
}

/// Chunk coordinates to denote the rendering limits of a `VoxelWorld`.
/// `None` specifies a non-existing limit, the world will be infinite in that direction.
/// `Some` specifies an existing inclusive limit, chunk past this limit will not be requested.
#[derive(Clone)]
pub struct Limits {
    pub from: [Option<isize>; 3],
    pub to: [Option<isize>; 3],
}

pub(crate) enum Chunk<T: Data> {
    NotNeeded,
    NotReady(Arc<AtomicCell<Option<Voxel<T>>>>),
    Ready(Entity),
}

impl<T: Data> VoxelWorld<T> {
    /// Create a new `VoxelWorld` component with specified render distance `dims` and a specified chunk `scale`.
    /// The `VoxelWorld` will still require a `VoxelSource`, that should be added to the entity separately.
    pub fn new(atlas: Handle<Atlas>, dims: [usize; 3], scale: f32) -> Self {
        Self {
            loaded: false,
            limits: Limits {
                from: [None; 3],
                to: [None; 3],
            },
            visibility: [0.0; 6],
            view_range: 0.0,
            atlas,
            data: (0..dims[0] * dims[1] * dims[2])
                .map(|_| Chunk::NotNeeded)
                .collect(),
            dims,
            origin: [0, 0, 0],
            scale,
        }
    }

    pub fn get<'a, R: 'a + GenericReadStorage<Component = DynamicVoxelMesh<T>>>(
        &self,
        mut coord: [isize; 3],
        chunks: &'a R,
    ) -> Option<&'a Voxel<T>> {
        for i in 0..3 {
            coord[i] -= self.origin[i];
            if coord[i] < 0 || coord[i] >= self.dims[i] as isize {
                return None;
            }
        }

        let index = coord[0] as usize
            + coord[1] as usize * self.dims[0]
            + coord[2] as usize * self.dims[0] * self.dims[1];

        self.data[index]
            .get()
            .and_then(move |e| chunks.get(e))
            .map(|m| m.deref())
    }

    pub fn get_mut<'a, W: 'a + GenericWriteStorage<Component = DynamicVoxelMesh<T>>>(
        &self,
        mut coord: [isize; 3],
        chunks: &'a mut W,
    ) -> Option<&'a mut Voxel<T>> {
        for i in 0..3 {
            coord[i] -= self.origin[i];
            if coord[i] < 0 || coord[i] >= self.dims[i] as isize {
                return None;
            }
        }

        let index = coord[0] as usize
            + coord[1] as usize * self.dims[0]
            + coord[2] as usize * self.dims[0] * self.dims[1];

        self.data[index]
            .get()
            .and_then(move |e| chunks.get_mut(e))
            .map(|r| r.deref_mut())
    }

    /// Get a `Handle<Atlas>` to the texture atlas used by this `VoxelWorld`
    pub fn atlas(&self) -> &Handle<Atlas> {
        &self.atlas
    }
}

impl<T: Data> amethyst::ecs::Component for VoxelWorld<T> {
    type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

impl<'a, 'b, V: Data> VoxelWorldAccess<'a, 'b, V> {
    pub fn new(
        world: &'a VoxelWorld<V>,
        chunks: &'a mut WriteStorage<'b, DynamicVoxelMesh<V>>,
    ) -> Self {
        Self { world, chunks }
    }

    pub fn get(&self, coord: [isize; 3]) -> Option<&Voxel<V>> {
        self.world.get(coord, self.chunks)
    }

    pub fn get_mut(&mut self, coord: [isize; 3]) -> Option<&mut Voxel<V>> {
        self.world.get_mut(coord, self.chunks)
    }
}

impl<T: Data> Chunk<T> {
    pub fn get(&self) -> Option<Entity> {
        match *self {
            Chunk::NotNeeded => None,
            Chunk::NotReady(_) => None,
            Chunk::Ready(voxel) => Some(voxel),
        }
    }
}

impl<T: Data, S: for<'s> VoxelSource<'s, T>> WorldSystem<T, S> {
    pub fn new(pool: Arc<ThreadPool>) -> Self {
        WorldSystem {
            marker: PhantomData,
            pool,
        }
    }
}

impl<'s, T: Data, S: for<'a> VoxelSource<'a, T> + Component> System<'s> for WorldSystem<T, S> {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        WriteStorage<'s, VoxelWorld<T>>,
        WriteStorage<'s, DynamicVoxelMesh<T>>,
        WriteStorage<'s, S>,
        Entities<'s>,
        Read<'s, ActiveCamera>,
        ReadStorage<'s, Camera>,
        WriteStorage<'s, Transform>,
        <S as VoxelSource<'s, T>>::SystemData,
    );

    fn run(
        &mut self,
        (
            mut worlds,
            mut meshes,
            mut sources,
            entities,
            active_camera,
            cameras,
            mut transforms,
            mut source_data,
        ): Self::SystemData,
    ) {
        let identity = Transform::default();

        let transform = active_camera
            .entity
            .as_ref()
            .and_then(|ac| transforms.get(*ac))
            .or_else(|| (&cameras, &transforms).join().next().map(|(_c, t)| t))
            .unwrap_or(&identity);

        let center = {
            let m = transform.global_matrix().column(3).xyz();
            [m[0], m[1], m[2]]
        };

        for (world_entity, world, source) in (&entities, &mut worlds, &mut sources).join() {
            let limits = source.limits();
            world.limits = source.limits();

            let origin = {
                let f = |i: usize| {
                    let origin =
                        (center[i] / world.scale).floor() as isize - (world.dims[i] / 2) as isize;
                    origin
                        .max(limits.from[i].unwrap_or(origin))
                        .min(limits.to[i].unwrap_or(origin))
                };
                [f(0), f(1), f(2)]
            };

            for (i, &center) in center.iter().enumerate() {
                world.visibility[i * 2] = center - world.scale * (world.dims[i] / 2) as f32;
                world.visibility[i * 2 + 1] =
                    world.visibility[i * 2] + world.scale * world.dims[i] as f32;
            }

            let offset = {
                let f = |i| origin[i] - world.origin[i];
                [f(0), f(1), f(2)]
            };
            let dims = [
                world.dims[0] as isize,
                world.dims[1] as isize,
                world.dims[2] as isize,
            ];

            fn limit_visibility(v: &mut [f32; 6], center: [f32; 3], limit: [f32; 3], scale: f32) {
                for i in 0..3 {
                    if limit[i] + scale < center[i] {
                        v[i * 2] = v[i * 2].max(limit[i] + scale);
                    } else {
                        v[i * 2 + 1] = v[i * 2 + 1].min(limit[i]);
                    }
                }
            }

            fn for_loop(to: isize, reverse: bool, mut f: impl FnMut(isize)) {
                let range = 0..to;
                if reverse {
                    for i in range.rev() {
                        f(i);
                    }
                } else {
                    for i in range {
                        f(i);
                    }
                }
            }

            for_loop(dims[2], offset[2] < 0, |z| {
                let exists = z + offset[2] >= 0 && z + offset[2] < dims[2];
                for_loop(dims[1], offset[1] < 0, |y| {
                    let exists = exists && y + offset[1] >= 0 && y + offset[1] < dims[1];
                    for_loop(dims[0], offset[0] < 0, |x| {
                        let exists = exists && x + offset[0] >= 0 && x + offset[0] < dims[0];
                        let index = (z * dims[0] * dims[1] + y * dims[0] + x) as usize;

                        // retrieve the existing chunk
                        let moved_chunk = if exists {
                            let index = ((z + offset[2]) * dims[0] * dims[1]
                                + (y + offset[1]) * dims[0]
                                + (x + offset[0])) as usize;
                            replace(&mut world.data[index], Chunk::NotNeeded)
                        } else {
                            Chunk::NotNeeded
                        };

                        // process the chunk
                        let moved_chunk = match moved_chunk {
                            Chunk::NotNeeded => {
                                // todo: *check* if the chunk needs to be loaded
                                let coord = [x + origin[0], y + origin[1], z + origin[2]];
                                limit_visibility(
                                    &mut world.visibility,
                                    center,
                                    [
                                        coord[0] as f32 * world.scale,
                                        coord[1] as f32 * world.scale,
                                        coord[2] as f32 * world.scale,
                                    ],
                                    world.scale,
                                );

                                match source.load_voxel(&mut source_data, coord) {
                                    VoxelSourceResult::Ok(chunk) => {
                                        let entity = entities.create();
                                        let mut mesh =
                                            DynamicVoxelMesh::new(chunk, world.atlas.clone());
                                        let mut transform = Transform::default();
                                        transform.set_scale(vec3(
                                            world.scale,
                                            world.scale,
                                            world.scale,
                                        ));
                                        transform.set_translation(vec3(
                                            coord[0] as f32 * world.scale,
                                            coord[1] as f32 * world.scale,
                                            coord[2] as f32 * world.scale,
                                        ));
                                        mesh.parent = Some((world_entity, [x, y, z]));
                                        meshes.insert(entity, mesh).ok();
                                        transforms.insert(entity, transform).ok();
                                        Chunk::Ready(entity)
                                    }
                                    VoxelSourceResult::Loading(job) => {
                                        let request = Arc::new(AtomicCell::default());
                                        let weak = Arc::downgrade(&request);
                                        self.pool.spawn(move || {
                                            if let Some(request) = weak.upgrade() {
                                                let result = job();
                                                request.store(Some(result));
                                            }
                                        });
                                        Chunk::NotReady(request)
                                    }
                                    VoxelSourceResult::Retry => {
                                        world.loaded = false;
                                        Chunk::NotNeeded
                                    }
                                }
                            }
                            Chunk::NotReady(request) => {
                                let coord = [x + origin[0], y + origin[1], z + origin[2]];
                                limit_visibility(
                                    &mut world.visibility,
                                    center,
                                    [
                                        coord[0] as f32 * world.scale,
                                        coord[1] as f32 * world.scale,
                                        coord[2] as f32 * world.scale,
                                    ],
                                    world.scale,
                                );

                                match request.take() {
                                    Some(chunk) => {
                                        let entity = entities.create();
                                        let mut mesh =
                                            DynamicVoxelMesh::new(chunk, world.atlas.clone());
                                        let mut transform = Transform::default();
                                        transform.set_scale(vec3(
                                            world.scale,
                                            world.scale,
                                            world.scale,
                                        ));
                                        transform.set_translation(vec3(
                                            coord[0] as f32 * world.scale,
                                            coord[1] as f32 * world.scale,
                                            coord[2] as f32 * world.scale,
                                        ));
                                        mesh.parent = Some((world_entity, [x, y, z]));
                                        meshes.insert(entity, mesh).ok();
                                        transforms.insert(entity, transform).ok();
                                        Chunk::Ready(entity)
                                    }
                                    None => Chunk::NotReady(request),
                                }
                            }
                            Chunk::Ready(voxel) => Chunk::Ready(voxel),
                        };

                        // install the chunk
                        match replace(&mut world.data[index], moved_chunk) {
                            Chunk::NotReady(_future) => { /* this is a problem */ }
                            Chunk::Ready(entity) => {
                                let coord = [x + origin[0], y + origin[1], z + origin[2]];
                                let voxel = replace(
                                    meshes.get_mut(entity).unwrap().deref_mut(),
                                    Voxel::Placeholder,
                                );
                                entities.delete(entity).expect("Remove chunk entity failed");
                                let job = source.drop_voxel(&mut source_data, coord, voxel);
                                self.pool.spawn(move || job());
                            }
                            Chunk::NotNeeded => (),
                        }
                    })
                })
            });

            world.origin = origin;

            // todo: find out view range
            world.view_range = world
                .visibility
                .iter()
                .enumerate()
                .fold(1000.0, |view_range, (i, visibility)| {
                    view_range.min((visibility - center[i / 2]).abs())
                });
        }
    }
}

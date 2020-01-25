use crate::voxel::*;
use amethyst::{
    core::transform::Transform,
    ecs::prelude::{Component, Join, Read, ReadStorage, System, SystemData, WriteStorage},
    renderer::{ActiveCamera, Camera},
};
use crossbeam::atomic::AtomicCell;
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
    pub(crate) data: Vec<Chunk<T>>,
    pub(crate) dims: [usize; 3],
    pub(crate) origin: [isize; 3],
    pub(crate) scale: f32,
}

/// A component that renders and contains a single root voxel.
pub struct VoxelRender<T: Data> {
    pub(crate) data: Voxel<T>,
    pub(crate) dirty: bool,

    // todo: the associated mesh should be destroyed if the VoxelRender is destroyed
    pub(crate) mesh: Option<usize>,
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
    Ready(VoxelRender<T>),
}

impl<T: Data> VoxelWorld<T> {
    /// Create a new `VoxelWorld` component with specified render distance `dims` and a specified chunk `scale`.
    /// The `VoxelWorld` will still require a `VoxelSource`, that should be added to the entity separately.
    pub fn new(dims: [usize; 3], scale: f32) -> Self {
        Self {
            loaded: false,
            limits: Limits {
                from: [None; 3],
                to: [None; 3],
            },
            visibility: [0.0; 6],
            view_range: 0.0,
            data: (0..dims[0] * dims[1] * dims[2])
                .map(|_| Chunk::NotNeeded)
                .collect(),
            dims,
            origin: [0, 0, 0],
            scale,
        }
    }

    pub fn get(&self, mut coord: [isize; 3]) -> Option<&Voxel<T>> {
        for i in 0..3 {
            coord[i] -= self.origin[i];
            if coord[i] < 0 || coord[i] >= self.dims[i] as isize {
                return None;
            }
        }

        let index = coord[0] as usize
            + coord[1] as usize * self.dims[0]
            + coord[2] as usize * self.dims[0] * self.dims[1];

        self.data[index].get().map(|r| r.deref())
    }

    pub fn get_mut(&mut self, mut coord: [isize; 3]) -> Option<&mut Voxel<T>> {
        for i in 0..3 {
            coord[i] -= self.origin[i];
            if coord[i] < 0 || coord[i] >= self.dims[i] as isize {
                return None;
            }
        }

        let index = coord[0] as usize
            + coord[1] as usize * self.dims[0]
            + coord[2] as usize * self.dims[0] * self.dims[1];

        self.data[index].get_mut().map(|r| r.deref_mut())
    }

    pub(crate) fn get_ready_chunk(&mut self, index: usize) -> Option<&mut VoxelRender<T>> {
        if self.data[index].get_mut().map(|c| c.dirty).unwrap_or(false) {
            if self.available(0, index, 1) {
                self.data[index].get_mut()
            } else {
                None
            }
        } else {
            None
        }
    }

    fn available(&self, axis: usize, index: usize, offset: usize) -> bool {
        if axis < 3 {
            let x = (index / offset) % self.dims[axis];
            let pos = self.origin[axis] + x as isize;

            let left = self.limits.from[axis]
                .map(|limit| pos <= limit)
                .unwrap_or(false);
            let left = left
                || (x > 0 && self.available(axis + 1, index - offset, offset * self.dims[axis]));
            let right = self.limits.to[axis]
                .map(|limit| pos >= limit)
                .unwrap_or(false);
            let right = right
                || (x < self.dims[axis] - 1
                    && self.available(axis + 1, index + offset, offset * self.dims[axis]));

            left && right && self.available(axis + 1, index, offset * self.dims[axis])
        } else {
            self.data[index].get().is_some()
        }
    }
}

impl<T: Data> amethyst::ecs::Component for VoxelWorld<T> {
    type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

impl<T: Data> VoxelRender<T> {
    /// Create a new `VoxelRender` component.
    pub fn new(value: Voxel<T>) -> Self {
        VoxelRender {
            data: value,
            dirty: true,
            mesh: None,
        }
    }

    /// Create a new `VoxelRender` component with a new `Voxel<T>` created from an iterator.
    pub fn from_iter<I>(data: T, iter: I) -> Self
    where
        I: IntoIterator<Item = Voxel<T>>,
    {
        VoxelRender {
            data: Voxel::from_iter(data, iter),
            dirty: true,
            mesh: None,
        }
    }
}

impl<T: Data> Deref for VoxelRender<T> {
    type Target = Voxel<T>;

    fn deref(&self) -> &Voxel<T> {
        &self.data
    }
}

impl<T: Data> DerefMut for VoxelRender<T> {
    fn deref_mut(&mut self) -> &mut Voxel<T> {
        self.dirty = true;
        &mut self.data
    }
}

impl<T: Data> amethyst::ecs::Component for VoxelRender<T> {
    type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

impl<T: Data> Chunk<T> {
    pub fn get(&self) -> Option<&VoxelRender<T>> {
        match *self {
            Chunk::NotNeeded => None,
            Chunk::NotReady(_) => None,
            Chunk::Ready(ref voxel) => Some(voxel),
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut VoxelRender<T>> {
        match *self {
            Chunk::NotNeeded => None,
            Chunk::NotReady(ref request) => match request.take() {
                Some(voxel) => {
                    *self = Chunk::Ready(VoxelRender::new(voxel));
                    match *self {
                        Chunk::Ready(ref mut voxel) => Some(voxel),
                        _ => unreachable!(),
                    }
                }
                None => None,
            },
            Chunk::Ready(ref mut voxel) => Some(voxel),
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
        WriteStorage<'s, S>,
        Read<'s, ActiveCamera>,
        ReadStorage<'s, Camera>,
        ReadStorage<'s, Transform>,
        <S as VoxelSource<'s, T>>::SystemData,
    );

    fn run(
        &mut self,
        (mut worlds, mut sources, active_camera, cameras, transforms, mut source_data): Self::SystemData,
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

        for (world, source) in (&mut worlds, &mut sources).join() {
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

            if !world.loaded
                || origin[0] != world.origin[0]
                || origin[1] != world.origin[1]
                || origin[2] != world.origin[2]
            {
                world.loaded = true;

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

                fn limit_visibility(
                    v: &mut [f32; 6],
                    center: [f32; 3],
                    limit: [f32; 3],
                    scale: f32,
                ) {
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
                                    + (x + offset[0]))
                                    as usize;
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
                                            Chunk::Ready(VoxelRender::new(chunk))
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
                                    Chunk::NotReady(request)
                                }
                                Chunk::Ready(voxel) => Chunk::Ready(voxel),
                            };

                            // install the chunk
                            match replace(&mut world.data[index], moved_chunk) {
                                Chunk::NotReady(_future) => { /* this is a problem */ }
                                Chunk::Ready(voxel) => {
                                    let coord = [x + origin[0], y + origin[1], z + origin[2]];
                                    let job =
                                        source.drop_voxel(&mut source_data, coord, voxel.data);

                                    self.pool.spawn(move || job());
                                }
                                Chunk::NotNeeded => (),
                            }
                        })
                    })
                });

                world.origin = origin;
            }

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

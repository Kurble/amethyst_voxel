use crate::{
    voxel::*,
    MutableVoxel,
};
use futures::{Future, Async};
use amethyst::{
    core::{transform::Transform},
    ecs::prelude::{Join, Read, ReadStorage, WriteStorage, System, SystemData, Component},
    renderer::{ActiveCamera, Camera},
};
use std::mem::replace;
use std::marker::PhantomData;
use std::error::Error;

pub struct MutableVoxelWorld<V: AsVoxel> {
    loaded: bool,
    limits: Limits,
    visibility: [f32; 6],
    view_range: f32,
    pub(crate) data: Vec<Chunk<V>>,
    pub(crate) dims: [usize; 3],
    pub(crate) origin: [isize; 3],
    pub(crate) scale: f32,
}

pub type VoxelFuture<V> = Box<dyn Future<Item=<V as AsVoxel>::Voxel, Error=Box<dyn Error+Send+Sync>>+Send+Sync>;

/// Trait for loading new chunks
pub trait Source<'a, V: AsVoxel> {
    type SystemData: SystemData<'a>;

    /// Process requests that were made using `load` or `drop`.
    fn process(&mut self, system_data: &mut Self::SystemData);

    /// Load chunk at the specified chunk coordinate
    fn load(&mut self, coord: [isize; 3]) -> VoxelFuture<V>;

    /// Remove a chunk at the specified chunk coordinate
    fn drop(&mut self, coord: [isize; 3], voxel: V::Voxel);

    /// Retrieve the limits in chunks that this source can generate
    fn limits(&self) -> Limits;
}

pub struct WorldSourceSystem<V: AsVoxel, S: for<'s> Source<'s, V>>(PhantomData<(V,S)>);

#[derive(Clone)]
pub struct Limits {
    pub from: [Option<isize>; 3],
    pub to: [Option<isize>; 3],
}

pub(crate) enum Chunk<V: AsVoxel> {
    NotNeeded,
    NotReady(VoxelFuture<V>),
    Ready(MutableVoxel<V>),
}

impl<V: AsVoxel> MutableVoxelWorld<V> {
    pub fn new(dims: [usize; 3], scale: f32) -> Self {
        Self {
            loaded: false,
            limits: Limits { from: [None; 3], to: [None; 3] },
            visibility: [0.0; 6],
            view_range: 0.0,
            data: (0..dims[0]*dims[1]*dims[2]).map(|_| Chunk::NotNeeded).collect(),
            dims,
            origin: [0, 0, 0],
            scale,
        }
    }

    pub(crate) fn get_ready_chunk(&mut self, index: usize) -> Option<&mut MutableVoxel<V>> {
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
            let x = (index/offset)%self.dims[axis];
            let pos = self.origin[axis] + x as isize;

            let left = self.limits.from[axis].map(|limit| pos <= limit).unwrap_or(false);
            let left = left || (x > 0 && self.available(axis+1, index - offset, offset * self.dims[axis]));
            let right = self.limits.to[axis].map(|limit| pos >= limit).unwrap_or(false);
            let right = right || (x < self.dims[axis]-1 && self.available(axis+1, index + offset, offset * self.dims[axis]));

            left && right && self.available(axis+1, index, offset * self.dims[axis])
        } else {
            self.data[index].get().is_some()
        }
    }
}

impl<V: AsVoxel + 'static> amethyst::ecs::Component for MutableVoxelWorld<V> {
    type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

impl<V: AsVoxel> Chunk<V> {
    pub fn get(&self) -> Option<&MutableVoxel<V>> {
        match *self {
            Chunk::NotNeeded => None,
            Chunk::NotReady(_) => None,
            Chunk::Ready(ref voxel) => Some(voxel),
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut MutableVoxel<V>> {
        match *self {
            Chunk::NotNeeded => None,
            Chunk::NotReady(ref mut fut) => {
                match fut.poll() {
                    Ok(Async::Ready(voxel)) => {
                        *self = Chunk::Ready(MutableVoxel::new(voxel));
                        match *self {
                            Chunk::Ready(ref mut voxel) => Some(voxel),
                            _ => unreachable!(),
                        }
                    },
                    Ok(Async::NotReady) => {
                        None
                    },
                    Err(e) => {
                        println!("Chunk failed to load: {}", e);
                        *self = Chunk::NotNeeded;
                        None
                    },
                }
            },
            Chunk::Ready(ref mut voxel) => Some(voxel),
        }
    }
}

impl<V: AsVoxel, S: for<'s> Source<'s, V>> WorldSourceSystem<V, S> {
    pub fn new() -> Self {
        WorldSourceSystem(PhantomData)
    }
}

impl<'s, V: 'static + AsVoxel, S: for<'a> Source<'a, V> + Component> System<'s> for WorldSourceSystem<V, S> {
    type SystemData = (
        WriteStorage<'s, MutableVoxelWorld<V>>,
        WriteStorage<'s, S>,
        Read<'s, ActiveCamera>,
        ReadStorage<'s, Camera>,
        ReadStorage<'s, Transform>,
        <S as Source<'s, V>>::SystemData,
    );

    fn run(&mut self, (mut worlds, mut sources, active_camera, cameras, transforms, mut source_data): Self::SystemData) {
        let identity = Transform::default();

        let transform = active_camera.entity
            .as_ref()
            .and_then(|ac| transforms.get(*ac))
            .or_else(|| (&cameras, &transforms)
                .join()
                .next()
                .map(|(_c, t)| t))
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
                    let origin = if center[i] < 0.0 {
                        (center[i] / world.scale).floor() as isize - (world.dims[i]/2) as isize
                    } else {
                        (center[i] / world.scale).floor() as isize - (world.dims[i]/2) as isize
                    };
                    origin
                        .max(limits.from[i].unwrap_or(origin))
                        .min(limits.to[i].unwrap_or(origin))
                };
                [f(0), f(1), f(2)]
            };     

            if !world.loaded || 
                origin[0] != world.origin[0] || 
                origin[1] != world.origin[1] || 
                origin[2] != world.origin[2] 
            {
                world.loaded = true;

                for i in 0..3 {
                    world.visibility[i*2] = center[i] - world.scale * (world.dims[i]/2) as f32;
                    world.visibility[i*2+1] = world.visibility[i*2] + world.scale * world.dims[i] as f32;
                }

                let offset = {
                    let f = |i| origin[i] - world.origin[i];
                    [f(0), f(1), f(2)]
                };
                let dims = [world.dims[0] as isize, world.dims[1] as isize, world.dims[2] as isize];

                fn limit_visibility(v: &mut [f32; 6], center: [f32; 3], limit: [f32; 3], scale: f32) {
                    for i in 0..3 {
                        if limit[i] + scale < center[i] {
                            v[i*2] = v[i*2].max(limit[i] + scale);
                        } else {
                            v[i*2+1] = v[i*2+1].min(limit[i]);
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
                            let index = (z*dims[0]*dims[1]+y*dims[0]+x) as usize;

                            // retrieve the existing chunk
                            let moved_chunk = if exists {
                                let index = ((z+offset[2])*dims[0]*dims[1]+(y+offset[1])*dims[0]+(x+offset[0])) as usize;
                                replace(&mut world.data[index], Chunk::NotNeeded)
                            } else {
                                Chunk::NotNeeded
                            };

                            // process the chunk
                            let moved_chunk = match moved_chunk {
                                Chunk::NotNeeded => {
                                    // todo: *check* if the chunk needs to be loaded
                                    let coord = [x+origin[0], y+origin[1], z+origin[2]];
                                    limit_visibility(&mut world.visibility, center, [
                                        coord[0] as f32 * world.scale, 
                                        coord[1] as f32 * world.scale, 
                                        coord[2] as f32 * world.scale
                                    ], world.scale);
                                    Chunk::NotReady(source.load(coord))
                                },
                                Chunk::NotReady(future) => {
                                    let coord = [x+origin[0], y+origin[1], z+origin[2]];
                                    limit_visibility(&mut world.visibility, center, [
                                        coord[0] as f32 * world.scale, 
                                        coord[1] as f32 * world.scale, 
                                        coord[2] as f32 * world.scale
                                    ], world.scale);
                                    Chunk::NotReady(future)
                                },
                                Chunk::Ready(voxel) => Chunk::Ready(voxel),
                            };

                            // install the chunk
                            match replace(&mut world.data[index], moved_chunk) {
                                Chunk::NotReady(_future) => { /* this is a problem */ },
                                Chunk::Ready(voxel) => {
                                    let coord = [x+origin[0], y+origin[1], z+origin[2]];
                                    source.drop(coord, voxel.data);
                                },
                                Chunk::NotNeeded => (),
                            }
                        })
                    })
                });

                world.origin = origin;
            }

            // todo: find out view range
            world.view_range = world.visibility.iter().enumerate().fold(1000.0, |view_range, (i, visibility)| {
                view_range.min((visibility - center[i/2]).abs())
            });

            source.process(&mut source_data);
        }
    }
}

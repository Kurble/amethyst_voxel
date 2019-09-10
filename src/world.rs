use crate::{
    voxel::*,
    context::*,
    triangulate::Const,
    MutableVoxel,
};
use futures::{Future, Async};
use std::mem::replace;
use std::error::Error;
use amethyst::ecs::World;

pub struct MutableVoxelWorld<V: AsVoxel> {
    source: Box<dyn Source<V>+Send+Sync>,
    loaded: bool,
    pub(crate) data: Vec<Chunk<V>>,
    pub(crate) dims: [usize; 3],
    pub(crate) origin: [isize; 3],
    pub(crate) scale: f32,
}

pub type VoxelFuture<V> = Box<dyn Future<Item=<V as AsVoxel>::Voxel, Error=Box<dyn Error+Send+Sync>>+Send+Sync>;

/// Trait for loading new chunks
pub trait Source<V: AsVoxel> {
    /// Load chunk at the specified chunk coordinate
    fn load(&mut self, coord: [isize; 3]) -> VoxelFuture<V>;

    /// Retrieve the limits in chunks that this source can generate
    fn limits(&self) -> Limits;
}

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
    pub fn new(source: Box<dyn Source<V>+Send+Sync>, dims: [usize; 3], scale: f32) -> Self {
        Self {
            source,
            loaded: false,
            data: (0..dims[0]*dims[1]*dims[2]).map(|_| Chunk::NotNeeded).collect(),
            dims,
            origin: [0, 0, 0],
            scale,
        }
    }

    pub fn load(&mut self, center: [f32; 3], _range: f32) {
        let limits = self.source.limits();

        let origin = {
            let f = |i: usize| {
                let origin = if center[i] < 0.0 {
                    (center[i] / self.scale).floor() as isize - (self.dims[i]/2) as isize
                } else {
                    (center[i] / self.scale).floor() as isize - (self.dims[i]/2) as isize
                };
                origin
                    .max(limits.from[i].unwrap_or(origin))
                    .min(limits.to[i].unwrap_or(origin))
            };
            [f(0), f(1), f(2)]
        };     

        if self.loaded && 
            origin[0] == self.origin[0] && 
            origin[1] == self.origin[1] && 
            origin[2] == self.origin[2] 
        {
            return;
        }

        self.loaded = true;

        let offset = {
            let f = |i| origin[i] - self.origin[i];
            [f(0), f(1), f(2)]
        };
        let dims = [self.dims[0] as isize, self.dims[1] as isize, self.dims[2] as isize];

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

                    let moved_chunk = if exists {
                        let index = ((z+offset[2])*dims[0]*dims[1]+(y+offset[1])*dims[0]+(x+offset[0])) as usize;
                        replace(&mut self.data[index], Chunk::NotNeeded)
                    } else {
                        Chunk::NotNeeded
                    };

                    let index = (z*dims[0]*dims[1]+y*dims[0]+x) as usize;
                    self.data[index] = match moved_chunk {
                        Chunk::NotNeeded => {
                            // todo: *check* if the chunk needs to be loaded
                            let coord = [x+origin[0], y+origin[1], z+origin[2]];
                            Chunk::NotReady(self.source.load(coord))
                        },
                        chunk => chunk,
                    };
                })
            })
        });

        self.origin = origin;
    }

    pub(crate) fn get_ready_chunk(&mut self, index: usize) -> Option<&mut MutableVoxel<V>> {
        if self.data[index].get_mut().map(|c| c.dirty).unwrap_or(false) {
            let coord = [
                index % self.dims[0],
                (index / self.dims[0]) % self.dims[1],
                index / (self.dims[0] * self.dims[1]),
            ];
            let offset = [
                1, 
                self.dims[0], 
                self.dims[0]*self.dims[1]
            ];
            let limits = self.source.limits();

            let available = coord.iter().enumerate().fold(true, |available, (i, &x)| {
                if x == 0 { 
                    available && limits.from[i].map(|x| x == self.origin[i]).unwrap_or(false)
                } else if x == self.dims[i]-1 { 
                    available && limits.to[i].map(|x| x == self.origin[i] + self.dims[i] as isize - 1).unwrap_or(false)
                } else if self.data[index - offset[i]].get_mut().is_none() {
                    false
                } else if self.data[index + offset[i]].get_mut().is_none() {
                    false
                } else {
                    available
                }
            });

            if available {
                self.data[index].get_mut()
            } else {
                None
            }
        } else {
            None
        }
    }

    pub(crate) fn context<'a>(&'a self, chunk: [isize;3]) -> impl Context + 'a {
        WorldContext(chunk, self)
    }
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

struct WorldContext<'a, V: AsVoxel>([isize; 3], &'a MutableVoxelWorld<V>);

impl<'a, V: AsVoxel> Clone for WorldContext<'a, V> {
    fn clone(&self) -> Self {
        WorldContext(self.0, self.1)
    }
}

impl<'a, V: AsVoxel> WorldContext<'a, V> {
    fn find(&self, x: isize, y: isize, z: isize) -> Option<&'a <V::Voxel as Voxel<V::Data>>::Child> {
        let Self(chunk, ref world) = *self;

        let size = Const::<V::Data>::WIDTH as isize;
        let grid = |x| if x >= 0 { x / size } else { (x+1) / size - 1};
        let coord = [chunk[0]+grid(x), chunk[1]+grid(y), chunk[2]+grid(z)];
        
        if (0..3).fold(true, |b, i| b && coord[i] >= 0 && coord[i] < world.dims[i] as isize) {
            let index = coord[0] as usize + 
                coord[1] as usize * world.dims[0] + 
                coord[2] as usize * world.dims[0] * world.dims[1];
            if let Some(voxel) = world.data[index].get() {
                let grid_mod = |x: isize| if x%size >= 0 { x%size } else { x%size + size } as usize;
                voxel.child(
                    grid_mod(x)*Const::<V::Data>::DX + 
                    grid_mod(y)*Const::<V::Data>::DY + 
                    grid_mod(z)*Const::<V::Data>::DZ)
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl<'a, V: AsVoxel> Context for WorldContext<'a, V> {
    type Child = DetailContext<'a, <V::Voxel as Voxel<V::Data>>::ChildData, <V::Voxel as Voxel<V::Data>>::Child, Self>;

    fn visible(&self, x: isize, y: isize, z: isize) -> bool {
        self.find(x, y, z).map(|c| c.visible()).unwrap_or(false)
    }

    fn render(&self, x: isize, y: isize, z: isize) -> bool {
        self.find(x, y, z).map(|c| c.render()).unwrap_or(false)
    }

    fn child(self, x: isize, y: isize, z: isize) -> Self::Child {
        DetailContext::new(self.clone(), [x, y, z], self.find(x, y, z))
    }
}

impl<V: AsVoxel + 'static> amethyst::ecs::Component for MutableVoxelWorld<V> {
    type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

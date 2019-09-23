use crate::world::VoxelWorld;
use crate::triangulate::Triangulate;
use crate::voxel::{Voxel, Data, Const};

/// Trait for retrieving neighbour information between separate root voxels.
pub trait Context<T: Data> {
    /// Same as Triangulate::visible, but accepts a relative coordinate for selecting a child voxel.
    fn visible(&self, x: isize, y: isize, z: isize) -> bool;

    /// Same as Triangulate::render, but accepts a relative coordinate for selecting a child voxel.
    fn render(&self, x: isize, y: isize, z: isize) -> bool;

    /// Returns a Context for the child at the relative coordinate
    fn child<'a>(&'a self, x: isize, y: isize, z: isize) -> DetailContext<'a, T>;
}

/// Context sampling no neighbours at all.
pub struct VoxelContext<'a, T: Data> {
    voxel: &'a Voxel<T>
}

/// Context sampling the inner details of a voxel. Neighbours resolve through the parent Context.
pub struct DetailContext<'a, T: Data> {
    parent: &'a dyn Context<T>,
    coord: [isize; 3],
    voxel: Option<&'a Voxel<T>>,
}

/// Context sampling the chunks of a world.
pub struct WorldContext<'a, V: Data> {
    coord: [isize; 3], 
    world: &'a VoxelWorld<V>,
}

impl<'a, T: Data> VoxelContext<'a, T> {
    pub fn new(voxel: &'a Voxel<T>) -> Self {
        Self { voxel }
    }
}

impl<'a, T: Data> Context<T> for VoxelContext<'a, T> {
    fn visible(&self, _: isize, _: isize, _: isize) -> bool { false }

    fn render(&self, _: isize, _: isize, _: isize) -> bool { false }

    fn child<'b>(&'b self, x: isize, y: isize, z: isize) -> DetailContext<'b, T> { 
        if x >= 0 && x < Const::<T>::WIDTH as isize &&
           y >= 0 && y < Const::<T>::WIDTH as isize &&
           z >= 0 && z < Const::<T>::WIDTH as isize {
            let index = Const::<T>::coord_to_index(x as usize, y as usize, z as usize);
            DetailContext::new(self, [x, y, z], self.voxel.get(index))
        } else {
            DetailContext::new(self, [x, y, z], None)
        }
    }
}

impl<'a, T: Data> DetailContext<'a, T> {
    pub fn new(parent: &'a dyn Context<T>, coord: [isize; 3], voxel: Option<&'a Voxel<T>>) -> Self {
        Self {
            parent,
            coord,
            voxel,
        }
    }

    fn find(&self, x: isize, y: isize, z: isize) -> Option<&'a Voxel<T>> {
        let size = Const::<T>::WIDTH as isize;
        let coord = [x, y, z];
        if (0..3).fold(true, |b, i| b && coord[i] >= 0 && coord[i] < size) {
            self.voxel.and_then(|v| v.get(
                x as usize * Const::<T>::DX +
                y as usize * Const::<T>::DY +
                z as usize * Const::<T>::DZ))
        } else {
            let grid = |x| if x >= 0 { x / size } else { (x+1) / size - 1};
            let grid_mod = |x| if x%size >= 0 { x%size } else { x%size + size };
            self.parent.clone().child(
                self.coord[0] + grid(x),
                self.coord[1] + grid(y),
                self.coord[2] + grid(z)
            ).find(
                grid_mod(x), 
                grid_mod(y), 
                grid_mod(z)
            )
        }
    }
}

impl<'a, T: Data> Context<T> for DetailContext<'a, T> {
    fn visible(&self, x: isize, y: isize, z: isize) -> bool { 
        self.find(x, y, z).map(|v| v.visible()).unwrap_or(false)
    }

    fn render(&self, x: isize, y: isize, z: isize) -> bool { 
        self.find(x, y, z).map(|v| v.render()).unwrap_or(false) 
    }

    fn child<'b>(&'b self, x: isize, y: isize, z: isize) -> DetailContext<'b, T> { 
        DetailContext::new(self, [x, y, z], self.find(x, y, z))
    }
}

impl<'a, V: Data> WorldContext<'a, V> {
    pub fn new(coord: [isize; 3], world: &'a VoxelWorld<V>) -> Self {
        Self {
            coord,
            world,
        }
    }

    fn find(&self, x: isize, y: isize, z: isize) -> Option<&'a Voxel<V>> {
        let size = Const::<V>::WIDTH as isize;
        let grid = |x| if x >= 0 { x / size } else { (x+1) / size - 1};
        let coord = [self.coord[0]+grid(x), self.coord[1]+grid(y), self.coord[2]+grid(z)];
        
        if (0..3).fold(true, |b, i| b && coord[i] >= 0 && coord[i] < self.world.dims[i] as isize) {
            let index = coord[0] as usize + 
                coord[1] as usize * self.world.dims[0] + 
                coord[2] as usize * self.world.dims[0] * self.world.dims[1];
            if let Some(voxel) = self.world.data[index].get() {
                let grid_mod = |x: isize| if x%size >= 0 { x%size } else { x%size + size } as usize;
                voxel.get(
                    grid_mod(x)*Const::<V>::DX + 
                    grid_mod(y)*Const::<V>::DY + 
                    grid_mod(z)*Const::<V>::DZ)
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl<'a, V: Data> Context<V> for WorldContext<'a, V> {
    fn visible(&self, x: isize, y: isize, z: isize) -> bool {
        self.find(x, y, z).map(|c| c.visible()).unwrap_or(false)
    }

    fn render(&self, x: isize, y: isize, z: isize) -> bool {
        self.find(x, y, z).map(|c| c.render()).unwrap_or(false)
    }

    fn child<'b>(&'b self, x: isize, y: isize, z: isize) -> DetailContext<'b, V> {
        DetailContext::new(self, [x, y, z], self.find(x, y, z))
    }
}


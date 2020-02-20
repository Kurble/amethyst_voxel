use crate::mesh::DynamicVoxelMesh;
use crate::voxel::{Data, NestedVoxel, Voxel, ChildOf};
use crate::world::VoxelWorld;

use amethyst::core::ecs::storage::GenericReadStorage;

/// Trait for retrieving neighbour information between separate root voxels.
pub trait Context<T: Voxel> {
    /// Same as Triangulate::visible, but accepts a relative coordinate for selecting a child voxel.
    fn visible(&self, x: isize, y: isize, z: isize) -> bool;

    /// Same as Triangulate::render, but accepts a relative coordinate for selecting a child voxel.
    fn render(&self, x: isize, y: isize, z: isize) -> bool;

    /// Returns a Context for the child at the relative coordinate
    fn child<'a>(
        &'a self,
        x: isize,
        y: isize,
        z: isize,
    ) -> DetailContext<'a, T>;
}

/// Context sampling no neighbours at all.
#[derive(Clone)]
pub struct VoxelContext<'a, T: Voxel> {
    voxel: &'a T,
}

/// Context sampling the inner details of a voxel. Neighbours resolve through the parent Context.
#[derive(Clone)]
pub struct DetailContext<'a, P: Voxel> {
    parent: &'a dyn Context<P>,
    coord: [isize; 3],
    voxel: Option<&'a ChildOf<P>>,
}

/// Context sampling the chunks of a world.
pub struct WorldContext<'a, V: Data, S: 'a + GenericReadStorage<Component = DynamicVoxelMesh<V>>> {
    coord: [isize; 3],
    world: &'a VoxelWorld<V>,
    chunks: &'a S,
}

impl<'a, T: Voxel> VoxelContext<'a, T> {
    pub fn new(voxel: &'a T) -> Self {
        Self { voxel }
    }
}

impl<'a, T: Voxel> Context<T> for VoxelContext<'a, T> {
    fn visible(&self, _: isize, _: isize, _: isize) -> bool {
        false
    }

    fn render(&self, x: isize, y: isize, z: isize) -> bool {
        if x >= 0
            && x < T::WIDTH as isize
            && y >= 0
            && y < T::WIDTH as isize
            && z >= 0
            && z < T::WIDTH as isize
        {
            false
        } else {
            true
        }
    }

    fn child<'b>(
        &'b self,
        x: isize,
        y: isize,
        z: isize,
    ) -> DetailContext<'b, T> {
        if x >= 0
            && x < T::WIDTH as isize
            && y >= 0
            && y < T::WIDTH as isize
            && z >= 0
            && z < T::WIDTH as isize
        {
            let index = T::coord_to_index(x as usize, y as usize, z as usize);
            DetailContext::new(self, [x, y, z], self.voxel.get(index))
        } else {
            DetailContext::new(self, [x, y, z], None)
        }
    }
}

impl<'a, P: Voxel> DetailContext<'a, P> {
    pub fn new(parent: &'a dyn Context<P>, coord: [isize; 3], voxel: Option<&'a ChildOf<P>>) -> Self {
        Self {
            parent,
            coord,
            voxel,
        }
    }

    fn find(&self, x: isize, y: isize, z: isize) -> Option<&'a ChildOf<ChildOf<P>>> {
        let size = ChildOf::<P>::WIDTH as isize;
        let coord = [x, y, z];
        if (0..3).fold(true, |b, i| b && coord[i] >= 0 && coord[i] < size) {
            self.voxel.and_then(|v| {
                v.get(
                    x as usize * ChildOf::<P>::DX
                        + y as usize * ChildOf::<P>::DY
                        + z as usize * ChildOf::<P>::DZ,
                )
            })
        } else {
            let grid = |x| if x >= 0 { x / size } else { (x + 1) / size - 1 };
            let grid_mod = |x| {
                if x % size >= 0 {
                    x % size
                } else {
                    x % size + size
                }
            };

            let neighbour: Self = self.parent.child(
                self.coord[0] + grid(x),
                self.coord[1] + grid(y),
                self.coord[2] + grid(z),
            );

            neighbour.find(grid_mod(x), grid_mod(y), grid_mod(z))
        }
    }
}

impl<'a, P: Voxel> Context<ChildOf<P>> for DetailContext<'a, P> {
    fn visible(&self, x: isize, y: isize, z: isize) -> bool {
        self.find(x, y, z).map(|v| v.visible()).unwrap_or(false)
    }

    fn render(&self, x: isize, y: isize, z: isize) -> bool {
        self.find(x, y, z).map(|v| v.render()).unwrap_or(false)
    }

    fn child<'b>(&'b self, x: isize, y: isize, z: isize) -> DetailContext<'b, ChildOf<P>> {
        DetailContext::new(self, [x, y, z], self.find(x, y, z))
    }
}

impl<'a, V, S> WorldContext<'a, V, S>
where
    V: Data,
    S: 'a + GenericReadStorage<Component = DynamicVoxelMesh<V>>,
{
    pub fn new(coord: [isize; 3], world: &'a VoxelWorld<V>, chunks: &'a S) -> Self {
        Self {
            coord,
            world,
            chunks,
        }
    }

    fn find(&self, x: isize, y: isize, z: isize) -> Option<&'a V::Child> {
        let size = NestedVoxel::<V>::WIDTH as isize;
        let grid = |x| if x >= 0 { x / size } else { (x + 1) / size - 1 };
        let coord = [
            self.coord[0] + grid(x),
            self.coord[1] + grid(y),
            self.coord[2] + grid(z),
        ];
        let within_bounds = |b, i| b && coord[i] >= 0 && coord[i] < self.world.dims[i] as isize;

        if (0..3).fold(true, within_bounds) {
            let index = coord[0] as usize
                + coord[1] as usize * self.world.dims[0]
                + coord[2] as usize * self.world.dims[0] * self.world.dims[1];
            if let Some(voxel) = self.world.data[index]
                .get()
                .and_then(|e| self.chunks.get(e))
            {
                let grid_mod = |x: isize| if x%size >= 0 { x%size } else { x%size + size } as usize;
                voxel.get(
                    grid_mod(x) * NestedVoxel::<V>::DX
                        + grid_mod(y) * NestedVoxel::<V>::DY
                        + grid_mod(z) * NestedVoxel::<V>::DZ,
                )
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl<'a, V, S> Context<NestedVoxel<V>> for WorldContext<'a, V, S>
where
    V: Data,
    S: 'a + GenericReadStorage<Component = DynamicVoxelMesh<V>>,
{
    fn visible(&self, x: isize, y: isize, z: isize) -> bool {
        self.find(x, y, z).map(|c| c.visible()).unwrap_or(false)
    }

    fn render(&self, x: isize, y: isize, z: isize) -> bool {
        self.find(x, y, z).map(|c| c.render()).unwrap_or(false)
    }

    fn child<'b>(&'b self, x: isize, y: isize, z: isize) -> DetailContext<'b, NestedVoxel<V>> {
        DetailContext::new(self, [x, y, z], self.find(x, y, z))
    }
}

impl<'a, V, S> Clone for WorldContext<'a, V, S>
where
    V: Data,
    S: 'a + GenericReadStorage<Component = DynamicVoxelMesh<V>>,
     {
    fn clone(&self) -> Self {
        Self {
            coord: self.coord,
            world: self.world,
            chunks: self.chunks,
        }
    }
}
use std::marker::PhantomData;

use crate::triangulate::*;
use crate::voxel::*;

/// Trait for retrieving neighbour information between separate root voxels.
pub trait Context: Clone {
    type Child: Context;

    /// Same as Voxel::visible, but accepts a relative coordinate for selecting a child voxel.
    fn visible(&self, x: isize, y: isize, z: isize) -> bool;

    /// Same as Voxel::render, but accepts a relative coordinate for selecting a child voxel.
    fn render(&self, x: isize, y: isize, z: isize) -> bool;

    /// Returns a Context for the child at the relative coordinate
    fn child(self, x: isize, y: isize, z: isize) -> Self::Child;
}

/// Context sampling no neighbours at all.
pub struct VoxelContext<'a, T: VoxelData, V: Voxel<T>>(&'a V, PhantomData<T>);

/// Context sampling the inner details of a voxel. Neighbours resolve through the parent Context.
pub struct DetailContext<'a, T: VoxelData, V: Voxel<T>, P: Context<Child=Self>> {
    parent: P,
    coord: [isize; 3],
    voxel: Option<&'a V>,
    phantom: PhantomData<T>,
}

impl<'a, V: Voxel<T>, T: VoxelData> VoxelContext<'a, T, V> {
    pub fn new(voxel: &'a V) -> Self {
        Self(voxel, PhantomData)
    }
}

impl<'a, V: Voxel<T>, T: VoxelData> Clone for VoxelContext<'a, T, V> {
    fn clone(&self) -> Self {
        Self(self.0, PhantomData)
    }
}

impl<'a, V: Voxel<T>, T: VoxelData> Context for VoxelContext<'a, T, V> {
    type Child = DetailContext<'a, V::ChildData, V::Child, Self>;

    fn visible(&self, _: isize, _: isize, _: isize) -> bool { false }

    fn render(&self, _: isize, _: isize, _: isize) -> bool { false }

    fn child(self, x: isize, y: isize, z: isize) -> DetailContext<'a, V::ChildData, V::Child, Self> { 
        if x >= 0 && x < Const::<T>::WIDTH as isize &&
           y >= 0 && y < Const::<T>::WIDTH as isize &&
           z >= 0 && z < Const::<T>::WIDTH as isize {
            let index = Const::<T>::coord_to_index(x as usize, y as usize, z as usize);
            DetailContext::new(self.clone(), [x, y, z], self.0.child(index))
        } else {
            DetailContext::new(self.clone(), [x, y, z], None)
        }
    }
}

impl<'a, T: VoxelData, V: Voxel<T>, P: Context<Child=Self>> DetailContext<'a, T, V, P> {
    pub fn new(parent: P, coord: [isize; 3], voxel: Option<&'a V>) -> Self {
        Self {
            parent,
            coord,
            voxel,
            phantom: PhantomData,
        }
    }

    fn find(&self, x: isize, y: isize, z: isize) -> Option<&'a V::Child> {
        let size = Const::<T>::WIDTH as isize;
        let coord = [x, y, z];
        if (0..3).fold(true, |b, i| b && coord[i] >= 0 && coord[i] < size) {
            self.voxel.and_then(|v| v.child(
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

impl<'a, T: VoxelData, V: Voxel<T>, P: Context<Child=Self>> Clone for DetailContext<'a, T, V, P> {
    fn clone(&self) -> Self {
        DetailContext {
            parent: self.parent.clone(),
            coord: self.coord,
            voxel: self.voxel,
            phantom: PhantomData,
        }
    }
}

impl<'a, T: VoxelData, V: Voxel<T>, P: Context<Child=Self>> Context for DetailContext<'a, T, V, P> {
    type Child = DetailContext<'a, V::ChildData, V::Child, Self>;

    fn visible(&self, x: isize, y: isize, z: isize) -> bool { 
        self.find(x, y, z).map(|v| v.visible()).unwrap_or(false)
    }

    fn render(&self, x: isize, y: isize, z: isize) -> bool { 
        self.find(x, y, z).map(|v| v.render()).unwrap_or(false) 
    }

    fn child(self, x: isize, y: isize, z: isize) -> Self::Child { 
        DetailContext::new(self.clone(), [x, y, z], self.find(x, y, z))
    }
}

use std::iter::FromIterator;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::material::VoxelMaterialId;

/// Trait for user data associated with voxels.
pub trait Data: 'static + Clone + Send + Sync {
    /// The amount of subdivisions to do in order to create child voxels.
    /// A value of 4 means that 2^4=16 subvoxels would be created at every axis, for a total of 16^3=4096 subvoxels.
    const SUBDIV: usize;

    /// Informs the triangulator whether the voxel that owns this data should be considered
    ///  as a solid voxel or not.
    /// A solid voxel is a voxel that can't be seen through in any way.
    fn solid(&self) -> bool {
        false
    }

    /// Informs the triangulator whether the voxel that owns this data should be considered empty.
    /// Empty voxels are not voxelized.
    fn empty(&self) -> bool {
        false
    }
}

/// A single voxel with nesting capability.
#[derive(Clone)]
pub enum Voxel<T: Data> {
    /// An empty voxel, air for example.
    Empty {
        /// User data for the voxel
        data: T,
    },

    /// A detail voxel. This voxel contains a number of subvoxels determined by `T::subdivisions()`.
    Detail {
        /// A shared array of subvoxels. The array is shared so that templated detail voxels can be
        /// represented cheaply.
        detail: Arc<Vec<Self>>,

        /// User data for the voxel.
        data: T,
    },

    /// A completely solid voxel with a single material (color + specular)
    Material {
        /// The material id
        material: VoxelMaterialId,

        /// User data for the voxel.
        data: T,
    },
}

impl<T: Data> Voxel<T> {
    pub const WIDTH: usize = 1 << T::SUBDIV;
    pub const AO_WIDTH: usize = Self::WIDTH + 1;
    pub const LAST: usize = Self::WIDTH - 1;
    pub const COUNT: usize = Self::WIDTH * Self::WIDTH * Self::WIDTH;
    pub const DX: usize = 1;
    pub const DY: usize = Self::DX * Self::WIDTH;
    pub const DZ: usize = Self::DY * Self::WIDTH;
    pub const SCALE: f32 = 1.0 / Self::WIDTH as f32;

    /// Convert a coordinate in the format (x, y, z) to an array index
    pub fn coord_to_index(x: usize, y: usize, z: usize) -> usize {
        x * Self::DX + y * Self::DY + z * Self::DZ
    }

    /// Convert an array index to a coordinate in the format (x, y, z)
    pub fn index_to_coord(index: usize) -> (usize, usize, usize) {
        let x = index & Self::LAST;
        let y = (index >> T::SUBDIV) & Self::LAST;
        let z = (index >> (T::SUBDIV * 2)) & Self::LAST;
        (x, y, z)
    }

    /// Construct a new, empty voxel. The voxel will have no content at all.
    pub fn new(data: T) -> Self {
        Voxel::Empty { data }
    }

    /// Construct a Voxel::Detail from an iterator.
    pub fn from_iter<I>(data: T, iter: I) -> Self
    where
        I: IntoIterator<Item = Self>,
    {
        Voxel::Detail {
            data,
            detail: Arc::new(Vec::from_iter(iter.into_iter().take(Self::COUNT))),
        }
    }

    /// Construct a Voxel::Material voxel. The voxel will be filled with one single material.
    pub fn filled(data: T, material: VoxelMaterialId) -> Self {
        Voxel::Material { data, material }
    }

    /// Retrieve a reference to subvoxel at index.
    pub fn get(&self, index: usize) -> Option<&Self> {
        match *self {
            Voxel::Empty { .. } => None,
            Voxel::Detail { ref detail, .. } => detail.get(index),
            Voxel::Material { .. } => None,
        }
    }

    /// Mutably retrieve subvoxel at index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Self> {
        match *self {
            Voxel::Empty { .. } => None,
            Voxel::Detail { ref mut detail, .. } => Arc::make_mut(detail).get_mut(index),
            Voxel::Material { .. } => None,
        }
    }
}

impl<T: Data + Default> From<VoxelMaterialId> for Voxel<T> {
    fn from(material: VoxelMaterialId) -> Voxel<T> {
        Voxel::Material {
            data: Default::default(),
            material,
        }
    }
}

impl<T: Data + Default> Default for Voxel<T> {
    fn default() -> Voxel<T> {
        Voxel::Empty {
            data: Default::default(),
        }
    }
}

impl<T: Data> Deref for Voxel<T> {
    type Target = T;

    fn deref(&self) -> &T {
        match *self {
            Voxel::Empty { ref data, .. }
            | Voxel::Detail { ref data, .. }
            | Voxel::Material { ref data, .. } => data,
        }
    }
}

impl<T: Data> DerefMut for Voxel<T> {
    fn deref_mut(&mut self) -> &mut T {
        match *self {
            Voxel::Empty { ref mut data, .. }
            | Voxel::Detail { ref mut data, .. }
            | Voxel::Material { ref mut data, .. } => data,
        }
    }
}

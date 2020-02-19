use std::iter::FromIterator;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use nalgebra_glm::Vec3;

use crate::material::AtlasMaterialHandle;
use crate::triangulate::Mesh;
use crate::context::Context;
use crate::side::Side;
use crate::ambient_occlusion::AmbientOcclusion;

pub trait VoxelMarker: 'static + Clone + Send + Sync {
    type Data: Data;

    /// Construct a new, empty voxel.
    fn new_empty(data: Self::Data) -> Self;

    /// Construct a new, filled voxel. The voxel will be filled with one single material.
    fn new_filled(data: Self::Data, material: AtlasMaterialHandle) -> Self;

    /// Retrieve a reference to subvoxel at index.
    fn get(&self, index: usize) -> Option<&<Self::Data as Data>::Child>;

    /// Mutably retrieve subvoxel at index
    fn get_mut(&mut self, index: usize) -> Option<&mut <Self::Data as Data>::Child>;

    /// Returns whether this voxel is visible, i.e. if it has geometry.
    fn visible(&self) -> bool;

    /// Returns whether the neighbours of this voxel are visible if the camera was inside this voxel.
    fn render(&self) -> bool;

    /// Whether this voxel has subvoxels.
    fn is_detail(&self) -> bool;

    /// Triangulate the voxel on a specific side
    fn triangulate<'a, S: Side, C: Context<Self>>(
        &self,
        mesh: &mut Mesh,
        ao: &AmbientOcclusion,
        context: C,
        origin: Vec3,
        scale: f32,
    );
}

/// Trait for user data associated with voxels.
pub trait Data: 'static + Default + Clone + Send + Sync {
    /// The amount of subdivisions to do in order to create child voxels.
    /// A value of 4 means that 2^4=16 subvoxels would be created at every axis, for a total of 16^3=4096 subvoxels.
    const SUBDIV: usize;

    type Child: VoxelMarker;

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

#[allow(type_alias_bounds)]
pub type ChildOf<T: VoxelMarker> = <T::Data as Data>::Child;

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
        detail: Arc<Vec<T::Child>>,

        /// User data for the voxel.
        data: T,
    },

    /// A completely solid voxel with a single material (color + specular)
    Material {
        /// The material id
        material: AtlasMaterialHandle,

        /// User data for the voxel.
        data: T,
    },

    /// An empty voxel without data
    Placeholder,
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

    /// Construct a Voxel::Detail from an iterator.
    pub fn from_iter<I>(data: T, iter: I) -> Self
    where
        I: IntoIterator<Item = T::Child>,
    {
        Voxel::Detail {
            data,
            detail: Arc::new(Vec::from_iter(iter.into_iter().take(Self::COUNT))),
        }
    }
}

impl<T: Data> VoxelMarker for Voxel<T> {
    type Data = T;

    fn new_empty(data: Self::Data) -> Self {
        Voxel::Empty { data }
    }

    fn new_filled(data: Self::Data, material: AtlasMaterialHandle) -> Self {
        Voxel::Material { data, material }
    }

    fn get(&self, index: usize) -> Option<&<T as Data>::Child> {
        match *self {
            Voxel::Empty { .. } => None,
            Voxel::Detail { ref detail, .. } => detail.get(index),
            Voxel::Material { .. } => None,
            Voxel::Placeholder => None,
        }
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut <T as Data>::Child> {
        match *self {
            Voxel::Empty { .. } => None,
            Voxel::Detail { ref mut detail, .. } => Arc::make_mut(detail).get_mut(index),
            Voxel::Material { .. } => None,
            Voxel::Placeholder => None,
        }
    }

    fn visible(&self) -> bool {
        match *self {
            Voxel::Empty { .. } => false,
            Voxel::Detail { ref data, .. } => !data.empty(),
            Voxel::Material { .. } => true,
            Voxel::Placeholder => false,
        }
    }

    fn render(&self) -> bool {
        match *self {
            Voxel::Empty { .. } => true,
            Voxel::Detail { ref data, .. } => !data.solid(),
            Voxel::Material { .. } => false,
            Voxel::Placeholder => true,
        }
    }

    fn is_detail(&self) -> bool {
        if let Voxel::Detail { .. } = self {
            true
        } else {
            false
        }
    }

    fn triangulate<'a, S: Side, C: Context<Self>>(
        &self,
        mesh: &mut Mesh,
        ao: &AmbientOcclusion,
        context: C,
        origin: Vec3,
        scale: f32,
    ) {
        use crate::triangulate::*;
        match *self {
            Voxel::Empty { .. } => (),

            Voxel::Detail { ref detail, .. } => {
                triangulate_detail::<Self, S, C>(
                    mesh,
                    ao,
                    context,
                    origin,
                    scale,
                    detail.as_slice(),
                )
            }

            Voxel::Material { material, .. } => {
                triangulate_face::<T, S>(mesh, ao, origin, scale, material)
            }

            Voxel::Placeholder => (),
        }
    }
}

impl<T: Data + Default> From<AtlasMaterialHandle> for Voxel<T> {
    fn from(material: AtlasMaterialHandle) -> Voxel<T> {
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
            Voxel::Placeholder => panic!("Placeholder dereferenced"),
        }
    }
}

impl<T: Data> DerefMut for Voxel<T> {
    fn deref_mut(&mut self) -> &mut T {
        match *self {
            Voxel::Empty { ref mut data, .. }
            | Voxel::Detail { ref mut data, .. }
            | Voxel::Material { ref mut data, .. } => data,
            Voxel::Placeholder => panic!("Placeholder dereferenced"),
        }
    }
}

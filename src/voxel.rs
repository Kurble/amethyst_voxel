use std::iter::FromIterator;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use nalgebra_glm::Vec3;

use crate::ambient_occlusion::SharedVertexData;
use crate::context::Context;
use crate::material::AtlasMaterialHandle;
use crate::side::Side;
use crate::triangulate::Triangulation;

pub trait Voxel: 'static + Clone + Send + Sync {
    type Data: Data;

    const WIDTH: usize = 1 << <Self::Data as Data>::SUBDIV;
    const AO_WIDTH: usize = Self::WIDTH + 1;
    const LAST: usize = Self::WIDTH - 1;
    const COUNT: usize = Self::WIDTH * Self::WIDTH * Self::WIDTH;
    const DX: usize = 1;
    const DY: usize = Self::DX * Self::WIDTH;
    const DZ: usize = Self::DY * Self::WIDTH;
    const SCALE: f32 = 1.0 / Self::WIDTH as f32;

    /// Convert a coordinate in the format (x, y, z) to an array index
    fn coord_to_index(x: usize, y: usize, z: usize) -> usize {
        x * Self::DX + y * Self::DY + z * Self::DZ
    }

    /// Convert an array index to a coordinate in the format (x, y, z)
    fn index_to_coord(index: usize) -> (usize, usize, usize) {
        let x = index & Self::LAST;
        let y = (index >> <Self::Data as Data>::SUBDIV) & Self::LAST;
        let z = (index >> (<Self::Data as Data>::SUBDIV * 2)) & Self::LAST;
        (x, y, z)
    }

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

    /// Returns the skin binding for this voxel
    fn skin(&self) -> Option<u8>;

    /// Whether this voxel has subvoxels.
    fn is_detail(&self) -> bool;

    /// Triangulate the voxel on a specific side
    fn triangulate<'a, S: Side, C: Context<Self>>(
        &self,
        mesh: &mut Triangulation,
        ao: &SharedVertexData,
        context: &C,
        origin: Vec3,
        scale: f32,
    );
}

/// Trait for user data associated with voxels.
pub trait Data: 'static + Default + Clone + Send + Sync {
    /// The amount of subdivisions to do in order to create child voxels.
    /// A value of 4 means that 2^4=16 subvoxels would be created at every axis, for a total of 16^3=4096 subvoxels.
    const SUBDIV: usize;

    type Child: Voxel;

    /// Returns whether this voxel is visible, i.e. if it has geometry.
    fn visible(&self) -> bool {
        true
    }

    /// Returns whether the neighbours of this voxel are visible if the camera was inside this voxel.
    fn render(&self) -> bool {
        false
    }

    /// Returns the skin binding for this voxel
    fn skin(&self) -> Option<u8> {
        None
    }
}

#[allow(type_alias_bounds)]
pub type ChildOf<T: Voxel> = <T::Data as Data>::Child;

/// A single voxel with nesting capability.
#[derive(Clone)]
pub enum NestedVoxel<T: Data> {
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

#[derive(Clone)]
pub struct SimpleVoxel {
    material: Option<AtlasMaterialHandle>,
}

impl Data for () {
    type Child = SimpleVoxel;
    const SUBDIV: usize = 0;
}

impl Voxel for SimpleVoxel {
    type Data = ();

    fn new_empty(_: ()) -> Self {
        Self { material: None }
    }

    fn new_filled(_: (), material: AtlasMaterialHandle) -> Self {
        Self { material: Some(material) }
    }

    fn get(&self, _: usize) -> Option<&ChildOf<Self>> {
        None
    }

    fn get_mut(&mut self, _: usize) -> Option<&mut ChildOf<Self>> {
        None
    }

    fn visible(&self) -> bool {
        self.material.is_some()
    }

    fn render(&self) -> bool {
        self.material.is_none()
    }

    fn skin(&self) -> Option<u8> {
        None
    }

    fn is_detail(&self) -> bool {
        false
    }

    fn triangulate<'a, S: Side, C: Context<Self>>(
        &self,
        mesh: &mut Triangulation,
        ao: &SharedVertexData,
        _: &C,
        origin: Vec3,
        scale: f32,
    ) {
        use crate::triangulate::*;
        if let Some(material) = self.material {
            triangulate_face::<S>(mesh, ao, origin, scale, material);
        }
    }
}

impl<T: Data> NestedVoxel<T> {
    /// Construct a Voxel::Detail from an iterator.
    pub fn from_iter<I>(data: T, iter: I) -> Self
    where
        I: IntoIterator<Item = T::Child>,
    {
        Self::Detail {
            data,
            detail: Arc::new(Vec::from_iter(iter.into_iter().take(Self::COUNT))),
        }
    }
}

impl<T: Data> Voxel for NestedVoxel<T> {
    type Data = T;

    fn new_empty(data: Self::Data) -> Self {
        Self::Empty { data }
    }

    fn new_filled(data: Self::Data, material: AtlasMaterialHandle) -> Self {
        Self::Material { data, material }
    }

    fn get(&self, index: usize) -> Option<&<T as Data>::Child> {
        match *self {
            Self::Empty { .. } => None,
            Self::Detail { ref detail, .. } => detail.get(index),
            Self::Material { .. } => None,
            Self::Placeholder => None,
        }
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut <T as Data>::Child> {
        match *self {
            Self::Empty { .. } => None,
            Self::Detail { ref mut detail, .. } => Arc::make_mut(detail).get_mut(index),
            Self::Material { .. } => None,
            Self::Placeholder => None,
        }
    }

    fn visible(&self) -> bool {
        match *self {
            Self::Empty { .. } => false,
            Self::Detail { ref data, .. } => data.visible(),
            Self::Material { .. } => true,
            Self::Placeholder => false,
        }
    }

    fn render(&self) -> bool {
        match *self {
            Self::Empty { .. } => true,
            Self::Detail { ref data, .. } => data.render(),
            Self::Material { .. } => false,
            Self::Placeholder => true,
        }
    }

    fn skin(&self) -> Option<u8> {
        match *self {
            Self::Empty { .. } => None,
            Self::Detail { ref data, .. } |
            Self::Material { ref data, .. } => data.skin(),
            Self::Placeholder => None,
        }
    }

    fn is_detail(&self) -> bool {
        if let Self::Detail { .. } = self {
            true
        } else {
            false
        }
    }

    fn triangulate<'a, S: Side, C: Context<Self>>(
        &self,
        mesh: &mut Triangulation,
        shared: &SharedVertexData,
        context: &C,
        origin: Vec3,
        scale: f32,
    ) {
        use crate::triangulate::*;
        match *self {
            Self::Empty { .. } => (),

            Self::Detail { ref detail, .. } => triangulate_detail::<S, _, _>(
                mesh,
                shared,
                context,
                origin,
                scale,
                detail.as_slice(),
            ),

            Self::Material { material, .. } => {
                triangulate_face::<S>(mesh, shared, origin, scale, material)
            }

            Self::Placeholder => (),
        }
    }
}

impl<T: Data> From<AtlasMaterialHandle> for NestedVoxel<T> {
    fn from(material: AtlasMaterialHandle) -> Self {
        Self::Material {
            data: Default::default(),
            material,
        }
    }
}

impl<T: Data> Default for NestedVoxel<T> {
    fn default() -> Self {
        Self::Empty {
            data: Default::default(),
        }
    }
}

impl<T: Data> Deref for NestedVoxel<T> {
    type Target = T;

    fn deref(&self) -> &T {
        match *self {
            Self::Empty { ref data, .. }
            | Self::Detail { ref data, .. }
            | Self::Material { ref data, .. } => data,
            Self::Placeholder => panic!("Placeholder dereferenced"),
        }
    }
}

impl<T: Data> DerefMut for NestedVoxel<T> {
    fn deref_mut(&mut self) -> &mut T {
        match *self {
            Self::Empty { ref mut data, .. }
            | Self::Detail { ref mut data, .. }
            | Self::Material { ref mut data, .. } => data,
            Self::Placeholder => panic!("Placeholder dereferenced"),
        }
    }
}

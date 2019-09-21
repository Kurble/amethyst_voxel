use std::sync::Arc;
use std::ops::{Deref, DerefMut};
use std::marker::PhantomData;
use std::iter::FromIterator;

use crate::coordinate::Pos;
use crate::triangulate::*;
use crate::ambient_occlusion::*;
use crate::side::*;
use crate::context::Context;
use crate::material::VoxelMaterialId;

/// Trait for user data associated with voxels on a specific level.
pub trait VoxelData: 'static + Clone + Send + Sync {
    /// The amount of subdivisions to do in order to create child voxels.
    /// Since a value of 0 would mean that no subdivisions will be made, it is used to denote a
    /// voxel type that has no children. `Voxel::Detail{ .. }` is not allowed for these voxel types.
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

/// The required functionality to triangulate voxels.
pub trait Voxel<T: VoxelData>: 'static + Clone + Send + Sync {
    type ChildData: VoxelData;
    type Child: Voxel<Self::ChildData>;

    /// Returns whether this voxel is visible, i.e. if it has geometry.
    fn visible(&self) -> bool;

    /// Returns whether the neighbours of this voxel are visible if the camera was inside this voxel.
    fn render(&self) -> bool;

    /// Triangulate this voxel to the mesh.
    fn triangulate_self<S: Side<T>, C: Context>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, context: &C, origin: Pos, scale: f32);

    /// Triangulate this voxel to the mesh.
    fn triangulate_all<C: Context>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, context: &C, origin: Pos, scale: f32);

    /// Returns a child voxel where applicable
    fn child(&self, index: usize) -> Option<&Self::Child>;

    /// Construct a voxel from an iterator of children
    fn from_iter<I: IntoIterator<Item=Self::Child>>(data: T, iter: I) -> Self;
}

/// Trait that translates a tuple of VoxelData types to a Voxel type.
pub trait AsVoxel: Send + Sync {
    type Data: VoxelData;
    type Voxel: Voxel<Self::Data> + Clone;
}

/// A single voxel with nothing special.
#[derive(Clone)]
pub enum Simple {
    Material(VoxelMaterialId),
    Empty
}

/// A single voxel with nesting capability.
#[derive(Clone)]
pub enum Nested<T: VoxelData, U: VoxelData, V: Voxel<U> + Clone> {
    /// An empty voxel, air for example.
    Empty {
        ph: PhantomData<U>,
        /// Metadata for the voxel
        data: T,
    },

    /// A detail voxel. This voxel contains a number of subvoxels determined by `T::subdivisions()`.
    /// The subvoxels will have the child type of `T`.
    Detail {
        /// A shared array of subvoxels. The array is shared so that templated detail voxels can be
        /// represented cheaply.
        detail: Arc<Vec<V>>,

        /// Metadata for the voxel.
        data: T,
    },

    /// A completely solid voxel with a single material (color + specular)
    Material {
        /// The material id
        material: VoxelMaterialId,

        /// Metadata for the voxel.
        data: T,
    },
}

impl VoxelData for () {
    const SUBDIV: usize = 1;
}

impl<T: VoxelData> AsVoxel for T {
    type Data = T;
    type Voxel = Nested<T, (), Simple>;
}

macro_rules! define_chain {
    ($head:ident, $($tail:ident),+) => {
        impl<$head: VoxelData, $($tail: VoxelData),+> AsVoxel for ($head, $($tail),+) where ($($tail),+): AsVoxel {
            type Data = $head;
            type Voxel = Nested<$head, <($($tail),+) as AsVoxel>::Data, <($($tail),+) as AsVoxel>::Voxel>;
        }
    };
}

define_chain!(A, B);
define_chain!(A, B, C);
define_chain!(A, B, C, D);
define_chain!(A, B, C, D, E);
define_chain!(A, B, C, D, E, F);
define_chain!(A, B, C, D, E, F, G);
define_chain!(A, B, C, D, E, F, G, H);

impl<T: VoxelData, U: VoxelData, V: Voxel<U> + Clone> Nested<T, U, V> {
    /// Construct a new, empty voxel. The voxel will have no content at all.
    pub fn new(data: T) -> Self {
        Nested::Empty {
            data,
            ph: PhantomData,
        }
    }

    /// Construct a Nested::Detail from an iterator.
    pub fn from_iter<I>(data: T, iter: I) -> Self where
        I: IntoIterator<Item = V>
    {
        Nested::Detail {
            data,
            detail: Arc::new(Vec::from_iter(iter.into_iter().take(Const::<T>::COUNT))),
        }
    }

    /// Construct a Nested::Material voxel. The voxel will be filled with one single material.
    pub fn filled(data: T, material: VoxelMaterialId) -> Self {
        Nested::Material {
            data,
            material
        }
    }

    pub fn get(&self, index: usize) -> Option<&V> {
        match *self {
            Nested::Empty { .. } => None,
            Nested::Detail { ref detail, .. } => detail.get(index),
            Nested::Material { .. } => None,
        }
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut V> {
        match *self {
            Nested::Empty { .. } => None,
            Nested::Detail { ref mut detail, .. } => Arc::make_mut(detail).get_mut(index),
            Nested::Material { .. } => None,
        }
    }
}

impl Voxel<()> for Simple {
    type ChildData = ();
    type Child = Simple;

    fn visible(&self) -> bool {
        if let Simple::Material(_) = *self {
            true
        } else {
            false
        }
    }

    fn render(&self) -> bool {
        if let Simple::Empty = *self {
            true
        } else {
            false
        }
    }

    fn triangulate_self<S: Side<()>, C: Context>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, _: &C, origin: Pos, scale: f32) {
        if let Simple::Material(material) = *self {
            triangulate_face::<(), S>(mesh, ao, origin, scale, material);
        }
    }

    fn triangulate_all<C: Context>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, _: &C, origin: Pos, scale: f32) {
        if let Simple::Material(material) = *self {
            triangulate_face::<(), Left>(mesh, ao, origin, scale, material);
            triangulate_face::<(), Right>(mesh, ao, origin, scale, material);
            triangulate_face::<(), Below>(mesh, ao, origin, scale, material);
            triangulate_face::<(), Above>(mesh, ao, origin, scale, material);
            triangulate_face::<(), Back>(mesh, ao, origin, scale, material);
            triangulate_face::<(), Front>(mesh, ao, origin, scale, material);
        }
    }

    fn child(&self, _: usize) -> Option<&Self::Child> {
        None
    }

    fn from_iter<I: IntoIterator<Item=Self::Child>>(_: (), _: I) -> Self {
        Simple::Empty
    }
}

impl From<VoxelMaterialId> for Simple {
    fn from(material: VoxelMaterialId) -> Simple {
        Simple::Material(material)
    }
}

impl Default for Simple {
    fn default() -> Simple {
        Simple::Empty
    }
}

impl<T: VoxelData, U: VoxelData, V: Voxel<U> + Clone> Voxel<T> for Nested<T, U, V> {
    type ChildData = U;
    type Child = V;

    fn visible(&self) -> bool {
        match *self {
            Nested::Empty { .. } => false,
            Nested::Detail { ref data, .. } => !data.empty(),
            Nested::Material { .. } => true,
        }
    }

    fn render(&self) -> bool {
        match *self {
            Nested::Empty { .. } => true,
            Nested::Detail { ref data, .. } => !data.solid(),
            Nested::Material { .. } => false,
        }
    }

    fn triangulate_self<S: Side<T>, C: Context>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, context: &C, origin: Pos, scale: f32) {
        match *self {
            Nested::Empty { .. } =>
                (),

            Nested::Detail { ref detail, .. } => match S::SIDE {
                0 => triangulate_detail::<T,U,V,S,Right,C>(mesh, ao, context, origin, scale, detail.as_slice()),
                1 => triangulate_detail::<T,U,V,S,Left,C>(mesh, ao, context, origin, scale, detail.as_slice()),
                2 => triangulate_detail::<T,U,V,S,Above,C>(mesh, ao, context, origin, scale, detail.as_slice()),
                3 => triangulate_detail::<T,U,V,S,Below,C>(mesh, ao, context, origin, scale, detail.as_slice()),
                4 => triangulate_detail::<T,U,V,S,Front,C>(mesh, ao, context, origin, scale, detail.as_slice()),
                5 => triangulate_detail::<T,U,V,S,Back,C>(mesh, ao, context, origin, scale, detail.as_slice()),
                _ => panic!(),
            },

            Nested::Material { material, .. } =>
                triangulate_face::<T, S>(mesh, ao, origin, scale, material),
        }
    }

    fn triangulate_all<C: Context>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, context: &C, origin: Pos, scale: f32) {
        self.triangulate_self::<Left,C>(mesh, ao, context, origin, scale);
        self.triangulate_self::<Right,C>(mesh, ao, context, origin, scale);
        self.triangulate_self::<Below,C>(mesh, ao, context, origin, scale);
        self.triangulate_self::<Above,C>(mesh, ao, context, origin, scale);
        self.triangulate_self::<Back,C>(mesh, ao, context, origin, scale);
        self.triangulate_self::<Front,C>(mesh, ao, context, origin, scale);
    }

    fn child(&self, index: usize) -> Option<&Self::Child> {
        match *self {
            Nested::Empty { .. } |
            Nested::Material { .. } => None,
            Nested::Detail { ref detail, .. } => if index < Const::<T>::COUNT {
                Some(&detail[index])
            } else {
                None
            },
        }
    }

    fn from_iter<I: IntoIterator<Item=V>>(data: T, iter: I) -> Self {
        Nested::Detail {
            data,
            detail: Arc::new(Vec::from_iter(iter.into_iter()))
        }
    }
}

impl<T: VoxelData + Default, U: VoxelData, V: Voxel<U> + Clone> From<VoxelMaterialId> for Nested<T, U, V> {
    fn from(material: VoxelMaterialId) -> Nested<T, U, V> {
        Nested::Material { data: Default::default(), material }
    }
}

impl<T: VoxelData + Default, U: VoxelData, V: Voxel<U> + Clone> Default for Nested<T, U, V> {
    fn default() -> Nested<T, U, V> {
        Nested::Empty { data: Default::default(), ph: PhantomData }
    }
}

impl<T: VoxelData, U: VoxelData, V: Voxel<U> + Clone> Deref for Nested<T, U, V> {
    type Target = T;

    fn deref(&self) -> &T {
        match *self {
            Nested::Empty { ref data, .. } |
            Nested::Detail { ref data, .. } |
            Nested::Material { ref data, .. } => data,
        }
    }
}

impl<T: VoxelData, U: VoxelData, V: Voxel<U> + Clone> DerefMut for Nested<T, U, V> {
    fn deref_mut(&mut self) -> &mut T {
        match *self {
            Nested::Empty { ref mut data, .. } |
            Nested::Detail { ref mut data, .. } |
            Nested::Material { ref mut data, .. } => data,
        }
    }
}

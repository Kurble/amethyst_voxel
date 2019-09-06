use std::sync::Arc;
use std::ops::{Deref, DerefMut};
use std::marker::PhantomData;
use std::iter::FromIterator;

use nalgebra_glm::*;

use crate::coordinate::Pos;
use crate::triangulate::*;
use crate::ambient_occlusion::*;
use crate::side::*;
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
    fn triangulate_self<S: Side<T>>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, origin: Pos, scale: f32);

    /// Triangulate this voxel to the mesh.
    fn triangulate_all(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, origin: Pos, scale: f32);

    /// Perform hitdetect on this voxel. Returns whether the voxel was hit.
    fn hit(&self, transform: Mat4, origin: Vec3, direction: Vec3) -> bool;

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

/// Trait for retrieving neighbour information between separate root voxels.
pub trait Context {
    /// Same as Voxel::visible, but accepts a relative coordinate for selecting a child voxel.
    fn visible(&self, x: isize, y: isize, z: isize) -> bool;

    /// Same as Voxel::render, but accepts a relative coordinate for selecting a child voxel.
    fn render(&self, x: isize, y: isize, z: isize) -> bool;
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

impl Context for () {
    fn visible(&self, _: isize, _: isize, _: isize) -> bool { false }

    fn render(&self, _: isize, _: isize, _: isize) -> bool { false }
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
        /*impl<$head: VoxelData, $($tail: VoxelData),+> AsNestedVoxel for ($head, $($tail),+) where ($($tail),+): AsVoxel {
            type ChildData = <($($tail),+) as AsVoxel>::Data;
            type Child = <($($tail),+) as AsVoxel>::Voxel;

            fn from_iter<I: IntoIterator<Item=Self::Child>>(data: Self::Data, iter: I) -> Self::Voxel {
                Nested::Detail {
                    data,
                    detail: Arc::new(Vec::from_iter(iter.into_iter().take(Const::<Self::Data>::COUNT))),
                }
            }
        }*/
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
    pub fn new(data: T) -> Self {
        Nested::Empty {
            data,
            ph: PhantomData,
        }
    }

    pub fn from_iter<I>(data: T, iter: I) -> Self where
        I: IntoIterator<Item = V>
    {
        Nested::Detail {
            data,
            detail: Arc::new(Vec::from_iter(iter.into_iter().take(Const::<T>::COUNT))),
        }
    }

    pub fn filled(data: T, material: VoxelMaterialId) -> Self {
        Nested::Material {
            data,
            material
        }
    }

    pub fn hit_detect(&self,
                      vox_to_world: Mat4,
                      origin: Vec3,
                      direction: Vec3
    ) -> Option<(usize, Mat4)> {
        // the current location being checked on the ray
        // scales the origin so that we're in subvoxel space.
        let transform = inverse(&vox_to_world);
        let scale = (1 << T::SUBDIV) as f32;
        let current_direction = transform.transform_vector(&direction);
        let current = transform * vec4(origin[0], origin[1], origin[2], 1.0);
        let mut current = vec4_to_vec3(&(current * scale));

        // move the origin of the ray to the start of the box, but only if we're not inside the
        //  box already.
        for i in 0..3 {
            let t = if current_direction[i] > 0.0 {
                (0.0-current[i]) / current_direction[i]
            } else {
                ((1<<T::SUBDIV) as f32 - current[i]) / current_direction[i]
            };
            if t > 0.0 {
                current += current_direction * t;
            }
        }

        // keep the current location as integer coordinates, to mitigate rounding errors on
        //  integrated values
        let mut current_i = [current[0] as i64, current[1] as i64, current[2] as i64];
        for i in 0..3 {
            if (current[i].floor() - current[i]).abs() < std::f32::EPSILON && current_direction[i] < 0.0 {
                current_i[i] -= 1;
            }
        }

        // find nearest intersection with a 1d grid, with grid lines at all integer positions
        let intersect = |position: f32, direction: f32| -> f32 {
            if direction == 0.0 {
                ::std::f32::INFINITY
            } else {
                let target = if direction < 0.0 {
                    let t = position.floor();
                    if (t - position).abs() < std::f32::EPSILON {
                        t - 1.0
                    } else {
                        t
                    }
                } else {
                    let t = position.ceil();
                    if (t - position).abs() < std::f32::EPSILON {
                        t + 1.0
                    } else {
                        t
                    }
                };

                (target-position) / direction
            }
        };

        // lambda that checks if we hit something
        let hit = |[x, y, z]: [i64; 3]| -> Option<(usize, Mat4)>{
            if x >= 0 && x < Const::<T>::WIDTH as i64 &&
                y >= 0 && y < Const::<T>::WIDTH as i64 &&
                z >= 0 && z < Const::<T>::WIDTH as i64 {
                let i = x as usize + y as usize * Const::<T>::DY + z as usize * Const::<T>::DZ;
                match self.get(i) {
                    Some(voxel) => {
                        if voxel.visible() {
                            let sc = Const::<T>::SCALE;
                            let s = scaling(&vec3(sc, sc, sc));
                            let t = translation(&vec3(x as f32 * sc, y as f32 * sc, z as f32 * sc));
                            let w = vox_to_world;
                            if voxel.hit(w*t*s, origin, direction) {
                                Some((i, transform))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    },
                    None => None,
                }
            } else {
                None
            }
        };

        // first we'll find out the nearest block hit
        for _ in 0..T::SUBDIV * 6 {
            // try to find the nearest intersection with the grid
            let i = vec3(
                intersect(current[0], current_direction[0]),
                intersect(current[1], current_direction[1]),
                intersect(current[2], current_direction[2]),
            );

            // advance by the distance of the nearest intersection
            for d in 0..3 {
                let e = (d+1)%3;
                let f = (d+2)%3;
                if i[d] <= i[e] && i[d] <= i[f] {
                    current += current_direction * i[d];
                    if current_direction[d] < 0.0 {
                        current_i[d] -= 1;
                        current[d] = current_i[d] as f32;
                        if let Some(hit) = hit(current_i) {
                            return Some(hit);
                        }
                    } else {
                        current_i[d] += 1;
                        current[d] = current_i[d] as f32;
                        if let Some(hit) = hit(current_i) {
                            return Some(hit);
                        }
                    }
                    break;
                }
            }
        }
        None
    }

    pub fn hit_get(&self,
                   vox_to_world: Mat4,
                   origin: Vec3,
                   direction: Vec3) -> Option<(&V, Mat4)> {
        self.hit_detect(vox_to_world, origin, direction)
            .and_then(move |(i, transform)| self.get(i).map(|v| (v, transform)))
    }

    pub fn hit_get_mut(&mut self,
                       vox_to_world: Mat4,
                       origin: Vec3,
                       direction: Vec3) -> Option<(&mut V, Mat4)> {
        self.hit_detect(vox_to_world, origin, direction)
            .and_then(move |(i, transform)| self.get_mut(i).map(|v| (v, transform)))
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

    fn triangulate_self<S: Side<()>>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, origin: Pos, scale: f32) {
        if let Simple::Material(material) = *self {
            triangulate_face::<(), S>(mesh, ao, origin, scale, material);
        }
    }

    fn triangulate_all(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, origin: Pos, scale: f32) {
        if let Simple::Material(material) = *self {
            triangulate_face::<(), Left>(mesh, ao, origin, scale, material);
            triangulate_face::<(), Right>(mesh, ao, origin, scale, material);
            triangulate_face::<(), Below>(mesh, ao, origin, scale, material);
            triangulate_face::<(), Above>(mesh, ao, origin, scale, material);
            triangulate_face::<(), Back>(mesh, ao, origin, scale, material);
            triangulate_face::<(), Front>(mesh, ao, origin, scale, material);
        }
    }

    fn hit(&self, _transform: Mat4, _origin: Vec3, _direction: Vec3) -> bool {
        // todo: check if the voxel is missed entirely.
        match *self {
            Simple::Empty => false,
            Simple::Material(_) => true,
        }
    }

    fn child(&self, _: usize) -> Option<&Self::Child> {
        None
    }

    fn from_iter<I: IntoIterator<Item=Self::Child>>(_: (), _: I) -> Self {
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

    fn triangulate_self<S: Side<T>>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, origin: Pos, scale: f32) {
        match *self {
            Nested::Empty { .. } =>
                (),

            Nested::Detail { ref detail, .. } => match S::SIDE {
                0 => triangulate_detail::<T,U,V,S,Right>(mesh, ao, origin, scale, detail.as_slice()),
                1 => triangulate_detail::<T,U,V,S,Left>(mesh, ao, origin, scale, detail.as_slice()),
                2 => triangulate_detail::<T,U,V,S,Above>(mesh, ao, origin, scale, detail.as_slice()),
                3 => triangulate_detail::<T,U,V,S,Below>(mesh, ao, origin, scale, detail.as_slice()),
                4 => triangulate_detail::<T,U,V,S,Front>(mesh, ao, origin, scale, detail.as_slice()),
                5 => triangulate_detail::<T,U,V,S,Back>(mesh, ao, origin, scale, detail.as_slice()),
                _ => panic!(),
            },

            Nested::Material { material, .. } =>
                triangulate_face::<T, S>(mesh, ao, origin, scale, material),
        }
    }

    fn triangulate_all(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, origin: Pos, scale: f32) {
        self.triangulate_self::<Left>(mesh, ao, origin, scale);
        self.triangulate_self::<Right>(mesh, ao, origin, scale);
        self.triangulate_self::<Below>(mesh, ao, origin, scale);
        self.triangulate_self::<Above>(mesh, ao, origin, scale);
        self.triangulate_self::<Back>(mesh, ao, origin, scale);
        self.triangulate_self::<Front>(mesh, ao, origin, scale);
    }

    fn hit(&self, transform: Mat4, origin: Vec3, direction: Vec3) -> bool {
        // todo check if we miss entirely
        match *self {
            Nested::Empty { .. } =>  return false,
            Nested::Detail { .. } => (),
            Nested::Material { .. } => return true,
        };

        self.hit_detect(transform, origin, direction)
            .is_some()
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

use std::sync::Arc;
use std::ops::{Deref, DerefMut};
use std::marker::PhantomData;
use std::iter::FromIterator;

use downcast_rs::Downcast;
use nalgebra_glm::*;
use amethyst::{
    ecs::prelude::{Component, DenseVecStorage},
};

use crate::coordinate::Pos;
use crate::triangulate::*;
use crate::side::*;

/// Trait for user data associated with voxels on a specific level.
pub trait Metadata: 'static + Clone + Send + Sync {
    /// The amount of subdivisions to do in order to create child voxels.
    /// Since a value of 0 would mean that no subdivisions will be made, it is used to denote a
    /// voxel type that has no children. `Voxel::Detail{ .. }` is not allowed for these voxel types.
    const SUBDIV: usize;

    /// Informs the triangulator whether this voxel should be considered as a solid voxel or not.
    /// A solid voxel is a voxel that can't be seen through in any way.
    fn solid(&self) -> bool {
        false
    }

    /// Informs the triangulator whether this voxel should be considered empty. Empty voxels
    /// are not voxelized.
    fn empty(&self) -> bool {
        false
    }
}

impl Metadata for () {
    const SUBDIV: usize = 1;
}

pub struct End;

pub trait AsVoxel {
    type Meta: Metadata;
    type Voxel: Voxel<Self::Meta> + Clone;
}

impl<T: Metadata> AsVoxel for T {
    type Meta = T;
    type Voxel = Dynamic<T, (), Static>;
}

macro_rules! define_chain {
    ($head:ident, $($tail:ident),+) => {
        impl<$head: Metadata, $($tail: Metadata),+> AsVoxel for ($head, $($tail),+) where ($($tail),+): AsVoxel {
            type Meta = $head;
            type Voxel = Dynamic<$head, <($($tail),+) as AsVoxel>::Meta, <($($tail),+) as AsVoxel>::Voxel>;
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

/// The required functionality to triangulate voxels.
pub trait Voxel<T: Metadata>: 'static + Send + Sync + Downcast + GenericVoxel {
    /// Returns whether this voxel is visible, i.e. if it has geometry.
    fn visible(&self) -> bool;

    /// Returns whether the neighbours of this voxel are visible if the camera was inside this voxel.
    fn render(&self) -> bool;

    /// Triangulate this voxel to the mesh.
    fn triangulate_self<S: Side<T>>(&self, mesh: &mut Mesh, origin: Pos, scale: f32);
}

pub trait GenericVoxel: Downcast + Send + Sync {
    /// Triangulate this voxel to the mesh.
    fn triangulate_all(&self, mesh: &mut Mesh, origin: Pos, scale: f32);

    /// Perform hitdetect on this voxel. Returns whether the voxel was hit.
    fn hit(&self, transform: Mat4, origin: Vec3, direction: Vec3) -> bool;
}

/// A single static voxel (no recursion).
#[derive(Clone)]
pub enum Static {
    Material(u32),
    Empty
}

/// A single dynamic voxel.
#[derive(Clone)]
pub enum Dynamic<T: Metadata, U: Metadata, V: Voxel<U> + Clone> {
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
        material: u32,

        /// Metadata for the voxel.
        data: T,
    },
}

impl_downcast!(GenericVoxel);

impl Voxel<()> for Static {
    fn visible(&self) -> bool {
        if let &Static::Material(_) = self {
            true
        } else {
            false
        }
    }

    fn render(&self) -> bool {
        if let &Static::Empty = self {
            true
        } else {
            false
        }
    }

    fn triangulate_self<S: Side<()>>(&self, mesh: &mut Mesh, origin: Pos, scale: f32) {
        if let &Static::Material(material) = self {
            triangulate_face::<(), S>(mesh, origin, scale, material);
        }
    }
}

impl GenericVoxel for Static {
    fn triangulate_all(&self, mesh: &mut Mesh, origin: Pos, scale: f32) {
        self.triangulate_self::<Left>(mesh, origin, scale);
        self.triangulate_self::<Right>(mesh, origin, scale);
        self.triangulate_self::<Below>(mesh, origin, scale);
        self.triangulate_self::<Above>(mesh, origin, scale);
        self.triangulate_self::<Back>(mesh, origin, scale);
        self.triangulate_self::<Front>(mesh, origin, scale);
    }

    fn hit(&self, _transform: Mat4, _origin: Vec3, _direction: Vec3) -> bool {
        // todo: check if the voxel is missed entirely.
        match self {
            Static::Empty => false,
            Static::Material(_) => true,
        }
    }
}

impl<T: Metadata, U: Metadata, V: Voxel<U> + Clone> Deref for Dynamic<T, U, V> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            &Dynamic::Empty { ref data, .. } |
            &Dynamic::Detail { ref data, .. } |
            &Dynamic::Material { ref data, .. } => data,
        }
    }
}

impl<T: Metadata, U: Metadata, V: Voxel<U> + Clone> DerefMut for Dynamic<T, U, V> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            &mut Dynamic::Empty { ref mut data, .. } |
            &mut Dynamic::Detail { ref mut data, .. } |
            &mut Dynamic::Material { ref mut data, .. } => data,
        }
    }
}

impl<T: Metadata, U: Metadata, V: Voxel<U> + Clone> Dynamic<T, U, V> {
    pub fn new(data: T) -> Self {
        Dynamic::Empty {
            data,
            ph: PhantomData,
        }
    }

    pub fn from_iter<I>(data: T, iter: I) -> Self where
        I: IntoIterator<Item = V>
    {
        Dynamic::Detail {
            data,
            detail: Arc::new(Vec::from_iter(iter.into_iter().take(Const::<T>::COUNT))),
        }
    }

    pub fn filled(data: T, material: u32) -> Self {
        Dynamic::Material {
            data,
            material
        }
    }

    pub fn hit_detect(&self,
                      vox_to_world: Mat4,
                      origin: Vec3,
                      direction: Vec3) -> Option<(usize, Mat4)> {
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
            if current[i].floor() == current[i] && current_direction[i] < 0.0 {
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
                    if t == position {
                        t - 1.0
                    } else {
                        t
                    }
                } else {
                    let t = position.ceil();
                    if t == position {
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
                    current = current + current_direction * i[d];
                    if current_direction[d] < 0.0 {
                        current_i[d] -= 1;
                        current[d] = current_i[d] as f32;
                        if let Some(hit) = hit(current_i.clone()) {
                            return Some(hit);
                        }
                    } else {
                        current_i[d] += 1;
                        current[d] = current_i[d] as f32;
                        if let Some(hit) = hit(current_i.clone()) {
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
        match self {
            &Dynamic::Empty { .. } => None,
            &Dynamic::Detail { ref detail, .. } => detail.get(index),
            &Dynamic::Material { .. } => None,
        }
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut V> {
        match self {
            &mut Dynamic::Empty { .. } => None,
            &mut Dynamic::Detail { ref mut detail, .. } => Arc::make_mut(detail).get_mut(index),
            &mut Dynamic::Material { .. } => None,
        }
    }
}

impl<T: Metadata, U: Metadata, V: Voxel<U> + Clone> Voxel<T> for Dynamic<T, U, V> {
    fn visible(&self) -> bool {
        match self {
            &Dynamic::Empty { .. } => false,
            &Dynamic::Detail { ref data, .. } => !data.empty(),
            &Dynamic::Material { .. } => true,
        }
    }

    fn render(&self) -> bool {
        match self {
            &Dynamic::Empty { .. } => true,
            &Dynamic::Detail { ref data, .. } => !data.solid(),
            &Dynamic::Material { .. } => false,
        }
    }

    fn triangulate_self<S: Side<T>>(&self, mesh: &mut Mesh, origin: Pos, scale: f32) {
        match self {
            &Dynamic::Empty { .. } =>
                (),

            &Dynamic::Detail { ref detail, .. } => match S::SIDE {
                0 => triangulate_detail::<T,U,V,S,Right>(mesh, origin, scale, detail.as_slice()),
                1 => triangulate_detail::<T,U,V,S,Left>(mesh, origin, scale, detail.as_slice()),
                2 => triangulate_detail::<T,U,V,S,Above>(mesh, origin, scale, detail.as_slice()),
                3 => triangulate_detail::<T,U,V,S,Below>(mesh, origin, scale, detail.as_slice()),
                4 => triangulate_detail::<T,U,V,S,Front>(mesh, origin, scale, detail.as_slice()),
                5 => triangulate_detail::<T,U,V,S,Back>(mesh, origin, scale, detail.as_slice()),
                _ => panic!(),
            },

            &Dynamic::Material { material, .. } =>
                triangulate_face::<T, S>(mesh, origin, scale, material),
        }
    }
}

impl<T: Metadata, U: Metadata, V: Voxel<U> + Clone> GenericVoxel for Dynamic<T, U, V> {
    fn triangulate_all(&self, mesh: &mut Mesh, origin: Pos, scale: f32) {
        self.triangulate_self::<Left>(mesh, origin, scale);
        self.triangulate_self::<Right>(mesh, origin, scale);
        self.triangulate_self::<Below>(mesh, origin, scale);
        self.triangulate_self::<Above>(mesh, origin, scale);
        self.triangulate_self::<Back>(mesh, origin, scale);
        self.triangulate_self::<Front>(mesh, origin, scale);
    }

    fn hit(&self, transform: Mat4, origin: Vec3, direction: Vec3) -> bool {
        // todo check if we miss entirely
        match self {
            &Dynamic::Empty { .. } =>  return false,
            &Dynamic::Detail { .. } => (),
            &Dynamic::Material { .. } => return true,
        };

        self.hit_detect(transform, origin, direction)
            .is_some()
    }
}

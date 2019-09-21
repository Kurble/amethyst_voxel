use crate::world::MutableVoxelWorld;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::any::Any;
use nalgebra_glm::*;
use crate::voxel::*;
use crate::triangulate::Const;

/// A ray that can be used to perform raycasting on a specific type that implements `Raycast`.
/// The ray is not compatible with other `Raycast` implementations.
pub struct Ray<T: Raycast> {
    transform: Mat4,
    origin: Vec3,
    direction: Vec3, 
    index: Option<usize>,
    marker: PhantomData<T>,
}

/// A "root" type that can create rays as well as being raycasted. 
pub trait RaycastBase: Raycast {
    /// Create a new ray compatible with `Self`. 
    fn ray(&self, origin: Vec3, direction: Vec3) -> Ray<Self>;
}

/// A type that can be raycasted.
pub trait Raycast: Any + Sized {
    /// The `Raycast`able type that is being checked for when casting
    type Child: Raycast;

    /// Cast a `Ray` on self, returning a ray that can be casted on the child type.
    fn cast(&self, ray: &Ray<Self>) -> Option<Ray<Self::Child>>;

    fn get(&self, ray: &Ray<Self::Child>) -> Option<&Self::Child>;

    fn get_mut(&mut self, ray: &Ray<Self::Child>) -> Option<&mut Self::Child>;

    fn select<T: Any>(&self, ray: &Ray<Self>) -> Option<&T> {
        if (self as &dyn Any).is::<T>() {
            (self as &dyn Any).downcast_ref()
        } else if let Some(ray) = self.cast(ray) {
            if let Some(child) = self.get(&ray) {
                child.select(&ray)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn select_mut<T: Any>(&mut self, ray: &Ray<Self>) -> Option<&mut T> {
        if (self as &mut dyn Any).is::<T>() {
            (self as &mut dyn Any).downcast_mut()
        } else if let Some(ray) = self.cast(ray) {
            if let Some(child) = self.get_mut(&ray) {
                child.select_mut(&ray)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn hit(&self, ray: &Ray<Self>) -> Option<f32> {
        self.cast(ray)
            .and_then(|ray| self
                .get(&ray)
                .and_then(|sub| sub.hit(&ray))
                .or_else(|| Some(ray.distance())))
    }
}

impl<T: Raycast> Ray<T> {
    pub fn distance(&self) -> f32 {
        // todo
        0.0
    }
}

impl<V: 'static + AsVoxel> RaycastBase for MutableVoxelWorld<V> where
    V::Voxel: Raycast
{
    fn ray(&self, origin: Vec3, direction: Vec3) -> Ray<Self> {
        Ray {
            origin,
            direction,
            transform: Mat4::identity(),
            index: None,
            marker: PhantomData,
        }
    }
}

impl<V: 'static + AsVoxel> Raycast for MutableVoxelWorld<V> where
    V::Voxel: Raycast
{
    type Child = V::Voxel;

    fn cast(&self, ray: &Ray<Self>) -> Option<Ray<Self::Child>> {
        // the current location being checked on the ray
        let mut current = ray.origin * (1.0/self.scale) - vec3(
            self.origin[0] as f32, 
            self.origin[1] as f32,
            self.origin[2] as f32);
        // keep the current location as integer coordinates, to mitigate rounding errors on
        //  integrated values
        let mut current_i = [current[0] as isize, current[1] as isize, current[2] as isize];
        for i in 0..3 {
            if (current[i].floor() - current[i]).abs() < std::f32::EPSILON && ray.direction[i] < 0.0 {
                current_i[i] -= 1;
            }
        }
        let hit = |coord:[isize;3]|->Option<Ray<Self::Child>> {
            if (0..3).fold(true, |b, i| b && coord[i] >= 0 && coord[i] < self.dims[i] as isize) {
                let i = coord[0] as usize + 
                    coord[1] as usize * self.dims[0] + 
                    coord[2] as usize * self.dims[0] * self.dims[1];
                match self.data[i].get() {
                    Some(voxel) => {
                        if voxel.visible() {
                            let sc = self.scale;
                            let s = scaling(&vec3(sc, sc, sc));
                            let t = translation(&vec3(
                                (self.origin[0] + coord[0]) as f32 * sc, 
                                (self.origin[1] + coord[1]) as f32 * sc, 
                                (self.origin[2] + coord[2]) as f32 * sc));
                            let r = Ray {
                                transform: ray.transform*t*s, 
                                origin: ray.origin, 
                                direction: ray.direction,
                                index: Some(i),
                                marker: PhantomData,
                            };
                            if voxel.cast(&r).is_some() {
                                Some(r)
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

        // don't forget to skip the starting position
        if let Some(hit) = hit(current_i) {
            return Some(hit);
        }

        // try to find the nearest voxel hit
        for _ in 0..30 {
            // try to find the nearest intersection with the grid
            let i = vec3(
                intersect(current[0], ray.direction[0]),
                intersect(current[1], ray.direction[1]),
                intersect(current[2], ray.direction[2]),
            );

            // advance by the distance to the nearest grid intersection
            for d in 0..3 {
                let e = (d+1)%3;
                let f = (d+2)%3;
                if i[d] <= i[e] && i[d] <= i[f] {
                    current += ray.direction * i[d];
                    if ray.direction[d] < 0.0 {
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

    fn get(&self, ray: &Ray<Self::Child>) -> Option<&Self::Child> {
        ray.index.and_then(move |index| self.data[index].get()).map(|c| c.deref())
    }

    fn get_mut(&mut self, ray: &Ray<Self::Child>) -> Option<&mut Self::Child> {
        ray.index.and_then(move |index| self.data[index].get_mut()).map(|c| c.deref_mut())
    }
}

impl Raycast for Simple {
    type Child = Simple;

    fn cast(&self, ray: &Ray<Simple>) -> Option<Ray<Simple>> {
        match *self {
            Simple::Empty => None,
            Simple::Material(_) => Some(Ray {
                transform: ray.transform,
                origin: ray.origin,
                direction: ray.direction,
                index: None,
                marker: PhantomData,
            }),
        }
    }

    fn get(&self, _: &Ray<Simple>) -> Option<&Self::Child> { None }

    fn get_mut(&mut self, _: &Ray<Simple>) -> Option<&mut Self::Child> { None }
}

impl<T: VoxelData, U: VoxelData, V: Voxel<U> + Raycast> Raycast for Nested<T, U, V> {
    type Child = V;

    fn cast(&self, ray: &Ray<Self>) -> Option<Ray<V>> {
        // the current location being checked on the ray
        // scales the origin so that we're in subvoxel space.
        let transform = inverse(&ray.transform);
        let scale = (1 << T::SUBDIV) as f32;
        let current_direction = transform.transform_vector(&ray.direction);
        let current = transform * vec4(ray.origin[0], ray.origin[1], ray.origin[2], 1.0);
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

        // lambda that checks if we hit something
        let hit = |[x, y, z]: [i64; 3]| -> Option<Ray<V>>{
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
                            let r = Ray {
                                transform: ray.transform*t*s, 
                                origin: ray.origin, 
                                direction: ray.direction,
                                index: Some(i),
                                marker: PhantomData,
                            };
                            if voxel.cast(&r).is_some() {
                                Some(r)
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

        // check the voxel that we're inside of first.
        if let Some(hit) = hit(current_i) {
            return Some(hit)
        }

        // then we'll find out the nearest block hit
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

    fn get(&self, ray: &Ray<V>) -> Option<&V> { 
        ray.index.and_then(move |index| self.get(index))
    }

    fn get_mut(&mut self, ray: &Ray<V>) -> Option<&mut V> { 
        ray.index.and_then(move |index| self.get_mut(index))
    }
}

/// find nearest intersection with a 1d grid, with grid lines at all integer positions
fn intersect(position: f32, direction: f32) -> f32 {
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
}

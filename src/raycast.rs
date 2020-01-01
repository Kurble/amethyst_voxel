use crate::triangulate::Triangulate;
use crate::voxel::{Data, Voxel};
use crate::world::VoxelWorld;
use nalgebra_glm::*;
use std::any::Any;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// A ray that can be used to perform raycasting on a specific type that implements `Raycast`.
/// The ray is not compatible with other `Raycast` implementations.
pub struct Ray<T: Raycast> {
    transform: Mat4,
    origin: Vec3,
    direction: Vec3,
    length: Option<f32>,
    index: Option<usize>,
    pub intersection: Option<Vec3>,
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

    /// Immutably retrieve the child for the casted ray.
    fn get(&self, ray: &Ray<Self::Child>) -> Option<&Self::Child>;

    /// Mutably retrieve the child for the casted ray.
    fn get_mut(&mut self, ray: &Ray<Self::Child>) -> Option<&mut Self::Child>;

    /// Get an immutable reference to a child voxel at a specific nesting depth by casting a ray.
    fn select<T: Data>(&self, ray: &Ray<Self>, depth: usize) -> Option<&Voxel<T>> {
        if depth == 0 {
            (self as &dyn Any).downcast_ref()
        } else if let Some(ray) = self.cast(ray) {
            if let Some(child) = self.get(&ray) {
                child.select(&ray, depth - 1)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Get a mutable reference to a child voxel at a specific nesting depth by casting a ray.
    fn select_mut<T: Data>(&mut self, ray: &Ray<Self>, depth: usize) -> Option<&mut Voxel<T>> {
        if depth == 0 {
            (self as &mut dyn Any).downcast_mut()
        } else if let Some(ray) = self.cast(ray) {
            if let Some(child) = self.get_mut(&ray) {
                child.select_mut(&ray, depth - 1)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Get the distance on the ray to the nearest hit.
    fn hit(&self, ray: &Ray<Self>) -> Option<f32> {
        self.cast(ray).map(|result| result.distance())
    }
}

impl<T: Raycast> Ray<T> {
    /// Get the distance this ray has travelled until it hit something
    pub fn distance(&self) -> f32 {
        self.intersection
            .map(|i| (i - self.origin).magnitude())
            .unwrap_or(::std::f32::INFINITY)
    }

    /// Set the distance that must be checked at least
    pub fn length(mut self, length: f32) -> Self {
        self.length = Some(length);
        self
    }
}

impl<V: Data> RaycastBase for VoxelWorld<V> {
    fn ray(&self, origin: Vec3, direction: Vec3) -> Ray<Self> {
        Ray {
            origin,
            direction,
            length: None,
            transform: Mat4::identity(),
            index: None,
            intersection: None,
            marker: PhantomData,
        }
    }
}

impl<V: Data> Raycast for VoxelWorld<V> {
    type Child = Voxel<V>;

    fn cast(&self, ray: &Ray<Self>) -> Option<Ray<Self::Child>> {
        // the current location being checked on the ray
        let mut current = ray.origin * (1.0 / self.scale)
            - vec3(
                self.origin[0] as f32,
                self.origin[1] as f32,
                self.origin[2] as f32,
            );
        // keep the current location as integer coordinates, to mitigate rounding errors on
        //  integrated values
        let mut current_i = [
            current[0] as isize,
            current[1] as isize,
            current[2] as isize,
        ];
        for i in 0..3 {
            if (current[i].floor() - current[i]).abs() < std::f32::EPSILON && ray.direction[i] < 0.0
            {
                current_i[i] -= 1;
            }
        }
        let hit = |coord: [isize; 3]| -> Option<Ray<Self::Child>> {
            if (0..3).fold(true, |b, i| {
                b && coord[i] >= 0 && coord[i] < self.dims[i] as isize
            }) {
                let i = coord[0] as usize
                    + coord[1] as usize * self.dims[0]
                    + coord[2] as usize * self.dims[0] * self.dims[1];
                if let Some(voxel) = self.data[i].get() {
                    if voxel.visible() {
                        let sc = self.scale;
                        let s = scaling(&vec3(sc, sc, sc));
                        let t = translation(&vec3(
                            (self.origin[0] + coord[0]) as f32 * sc,
                            (self.origin[1] + coord[1]) as f32 * sc,
                            (self.origin[2] + coord[2]) as f32 * sc,
                        ));
                        let r = Ray {
                            transform: ray.transform * t * s,
                            origin: ray.origin,
                            direction: ray.direction,
                            length: ray.length,
                            index: Some(i),
                            intersection: None,
                            marker: PhantomData,
                        };
                        if let Some(mut sub) = voxel.cast(&r) {
                            sub.index = Some(i);
                            return Some(sub);
                        }
                    }
                }
            }
            None
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
                let e = (d + 1) % 3;
                let f = (d + 2) % 3;
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
        ray.index
            .and_then(move |index| self.data[index].get())
            .map(|c| c.deref())
    }

    fn get_mut(&mut self, ray: &Ray<Self::Child>) -> Option<&mut Self::Child> {
        ray.index
            .and_then(move |index| self.data[index].get_mut())
            .map(|c| c.deref_mut())
    }
}

impl<T: Data> Raycast for Voxel<T> {
    type Child = Self;

    fn cast(&self, ray: &Ray<Self>) -> Option<Ray<Self>> {
        // the current location being checked on the ray
        // scales the origin so that we're in subvoxel space.
        let transform = inverse(&ray.transform);
        let scale = (1 << T::SUBDIV) as f32;
        let current_direction = transform.transform_vector(&ray.direction);
        let current = transform * vec4(ray.origin[0], ray.origin[1], ray.origin[2], 1.0);
        let mut current = vec4_to_vec3(&current) * scale;

        // move the origin of the ray to the start of the box, but only if we're not inside the
        //  box already.
        for i in 0..3 {
            let t = if current_direction[i] > 0.0 {
                (0.0 - current[i]) / current_direction[i]
            } else if current_direction[i] < 0.0 {
                ((1 << T::SUBDIV) as f32 - current[i]) / current_direction[i]
            } else {
                0.0
            };
            if t > 0.0 {
                current += current_direction * t;
            }
        }

        // keep the current location as integer coordinates, to mitigate rounding errors on
        //  integrated values
        let mut current_i = [current[0] as i64, current[1] as i64, current[2] as i64];
        for i in 0..3 {
            if (current[i].floor() - current[i]).abs() < std::f32::EPSILON
                && current_direction[i] < 0.0
            {
                current_i[i] -= 1;
            }
        }

        match self {
            Voxel::Empty { .. } => None,
            Voxel::Material { .. } => {
                let current = current / scale;
                let intersection = ray.transform * vec4(current[0], current[1], current[2], 1.0);

                Some(Ray {
                    transform: ray.transform,
                    origin: ray.origin,
                    direction: ray.direction,
                    length: ray.length,
                    index: Some(0),
                    intersection: Some(vec4_to_vec3(&intersection)),
                    marker: PhantomData,
                })
            }
            Voxel::Detail { .. } => {
                // lambda that checks if we hit something
                let hit = |[x, y, z]: [i64; 3]| -> Option<Ray<Self>> {
                    if x >= 0
                        && x < Self::WIDTH as i64
                        && y >= 0
                        && y < Self::WIDTH as i64
                        && z >= 0
                        && z < Self::WIDTH as i64
                    {
                        let i = x as usize + y as usize * Self::DY + z as usize * Self::DZ;
                        if let Some(voxel) = self.get(i) {
                            if voxel.visible() {
                                let sc = Self::SCALE;
                                let s = scaling(&vec3(sc, sc, sc));
                                let t =
                                    translation(&vec3(x as f32 * sc, y as f32 * sc, z as f32 * sc));
                                let r = Ray {
                                    transform: ray.transform * t * s,
                                    origin: ray.origin,
                                    direction: ray.direction,
                                    length: None,
                                    index: Some(i),
                                    intersection: None,
                                    marker: PhantomData,
                                };
                                if let Some(mut sub) = voxel.cast(&r) {
                                    sub.index = Some(i);
                                    return Some(sub);
                                }
                            }
                        }
                    }

                    None
                };

                // check the voxel that we're inside of first.
                if let Some(hit) = hit(current_i) {
                    return Some(hit);
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
                        let e = (d + 1) % 3;
                        let f = (d + 2) % 3;
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
        }
    }

    fn get(&self, ray: &Ray<Self>) -> Option<&Self> {
        ray.index.and_then(move |index| self.get(index))
    }

    fn get_mut(&mut self, ray: &Ray<Self>) -> Option<&mut Self> {
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

        (target - position) / direction
    }
}

use nalgebra_glm::*;
use std::ops::{Deref, DerefMut};

use crate::voxel::{Data, NestedVoxel, Voxel};
use crate::world::VoxelWorldAccess;

/// A ray that can be used to perform raycasting on a specific type that implements `Raycast`.
/// The ray is not compatible with other `Raycast` implementations.
pub struct Ray {
    origin: Vec3,
    direction: Vec3,
    transform: Mat4,
    length: Option<f32>,
}

/// The result from performing a raycast
pub struct Intersection {
    /// The inner result. If the voxel that this intersection hit has subvoxels, the
    ///  inner intersection contains the intersection with that subvoxel.
    pub inner: Option<Box<Intersection>>,
    /// The index of the subvoxel that this intersection intersects with.
    pub index: usize,
    /// The position of this intersection.
    pub position: Vec3,
    /// The normal of this intersection.
    pub normal: Vec3,
}

/// A "root" type that can create rays as well as being raycasted.
pub trait RaycastBase: Raycast {
    /// Create a new ray compatible with `Self`.
    fn ray(&self, origin: Vec3, direction: Vec3) -> Ray;
}

/// A type that can be raycasted.
pub trait Raycast {
    type Child: Raycast;

    /// Cast a `Ray` on self, returning a ray that can be casted on the child type.
    fn cast(&self, ray: &Ray) -> Option<Intersection>;

    fn check(
        &self,
        ray: &Ray,
        current: Vec3,
        coord: [isize; 3],
        normal: Vec3,
    ) -> Option<Intersection>;

    /// Immutably retrieve the child for the casted ray.
    fn get_hit(&self, intersection: &Intersection) -> Option<&Self::Child>;

    /// Mutably retrieve the child for the casted ray.
    fn get_hit_mut(&mut self, ray: &Intersection) -> Option<&mut Self::Child>;

    /// Get the distance on the ray to the nearest hit.
    fn hit(&self, ray: &Ray) -> Option<f32> {
        self.cast(ray)
            .map(|result| (result.innermost().position - ray.origin).magnitude())
    }
}

impl Ray {
    pub fn length(mut self, length: f32) -> Self {
        self.length = Some(length);
        self
    }

    pub fn debug(self) -> Self {
        self
    }
}

impl Intersection {
    pub fn level(&self, level: usize) -> Option<&Intersection> {
        if level == 0 {
            Some(self)
        } else {
            self.inner.as_ref().and_then(|i| i.level(level - 1))
        }
    }

    pub fn innermost(&self) -> &Intersection {
        self.inner.as_ref().map(|i| i.innermost()).unwrap_or(self)
    }
}

impl<'a, 'b, V: Data> RaycastBase for VoxelWorldAccess<'a, 'b, V> {
    fn ray(&self, origin: Vec3, direction: Vec3) -> Ray {
        Ray {
            origin,
            direction,
            transform: Mat4::identity(),
            length: None,
        }
    }
}

impl<'a, 'b, V: Data> Raycast for VoxelWorldAccess<'a, 'b, V> {
    type Child = NestedVoxel<V>;

    fn cast(&self, ray: &Ray) -> Option<Intersection> {
        let origin = vec3(
            self.world.origin[0] as f32,
            self.world.origin[1] as f32,
            self.world.origin[2] as f32,
        );
        // the current location being checked on the ray
        let current = ray.origin * (1.0 / self.world.scale) - origin;
        cast(self, ray, current, ray.direction, 30).map(|mut intersection| {
            intersection.position = intersection.position + origin;
            intersection.position = intersection.position * self.world.scale;
            intersection
        })
    }

    fn check(
        &self,
        ray: &Ray,
        current: Vec3,
        coord: [isize; 3],
        normal: Vec3,
    ) -> Option<Intersection> {
        if (0..3).fold(true, |b, i| {
            b && coord[i] >= 0 && coord[i] < self.world.dims[i] as isize
        }) {
            let i = coord[0] as usize
                + coord[1] as usize * self.world.dims[0]
                + coord[2] as usize * self.world.dims[0] * self.world.dims[1];
            if let Some(voxel) = self.world.data[i].get().and_then(|e| self.chunks.get(e)) {
                if voxel.visible() {
                    let sc = self.world.scale;
                    let s = scaling(&vec3(sc, sc, sc));
                    let t = translation(&vec3(
                        (self.world.origin[0] + coord[0]) as f32 * sc,
                        (self.world.origin[1] + coord[1]) as f32 * sc,
                        (self.world.origin[2] + coord[2]) as f32 * sc,
                    ));
                    let r = Ray {
                        transform: ray.transform * t * s,
                        origin: ray.origin,
                        direction: ray.direction,
                        length: ray.length,
                    };
                    if let Some(sub) = voxel.cast(&r) {
                        return Some(Intersection {
                            inner: Some(Box::new(sub)),
                            index: i,
                            position: current,
                            normal,
                        });
                    }
                }
            } else {
                return Some(Intersection {
                    inner: None,
                    index: 0,
                    position: current,
                    normal,
                });
            }
        }
        None
    }

    fn get_hit(&self, intersection: &Intersection) -> Option<&Self::Child> {
        self.world.data[intersection.index]
            .get()
            .and_then(|e| self.chunks.get(e))
            .map(|c| c.deref())
    }

    fn get_hit_mut(&mut self, intersection: &Intersection) -> Option<&mut Self::Child> {
        self.world.data[intersection.index]
            .get()
            .and_then(move |e| self.chunks.get_mut(e))
            .map(|c| c.deref_mut())
    }
}

impl<T: Voxel> Raycast for T {
    type Child = <T::Data as Data>::Child;

    fn cast(&self, ray: &Ray) -> Option<Intersection> {
        // the current location being checked on the ray
        // scales the origin so that we're in subvoxel space.
        let transform = inverse(&ray.transform);
        let scale = (1 << <T::Data as Data>::SUBDIV) as f32;
        let current_direction = transform.transform_vector(&ray.direction);
        let current = transform * vec4(ray.origin[0], ray.origin[1], ray.origin[2], 1.0);
        let mut current = vec4_to_vec3(&current) * scale;

        // move the origin of the ray to the start of the box, but only if we're not inside the
        //  box already.
        for i in 0..3 {
            let t = if current_direction[i] > 0.0 {
                (0.0 - current[i]) / current_direction[i]
            } else if current_direction[i] < 0.0 {
                (scale - current[i]) / current_direction[i]
            } else {
                0.0
            };
            if t > 0.0 {
                current += current_direction * t;
            }
        }

        cast(
            self,
            ray,
            current,
            current_direction,
            6 * T::WIDTH,
        )
        .map(|mut intersection| {
            let mut pos = vec3_to_vec4(&intersection.position) / scale;
            pos.w = 1.0;
            pos = ray.transform * pos;
            intersection.position = vec4_to_vec3(&pos);
            intersection
        })
    }

    fn check(
        &self,
        ray: &Ray,
        current: Vec3,
        coord: [isize; 3],
        normal: Vec3,
    ) -> Option<Intersection> {
        if (0..3).fold(true, |b, i| {
            b && coord[i] >= 0 && coord[i] < T::WIDTH as isize
        }) {
            let i = coord[0] as usize
                + coord[1] as usize * T::DY
                + coord[2] as usize * T::DZ;
            if let Some(voxel) = self.get(i) {
                if voxel.visible() {
                    if voxel.is_detail() {
                        let sc = T::SCALE;
                        let s = scaling(&vec3(sc, sc, sc));
                        let t = translation(&vec3(
                            coord[0] as f32 * sc,
                            coord[1] as f32 * sc,
                            coord[2] as f32 * sc,
                        ));
                        let r = Ray {
                            transform: ray.transform * t * s,
                            origin: ray.origin,
                            direction: ray.direction,
                            length: ray.length,
                        };
                        if let Some(sub) = voxel.cast(&r) {
                            return Some(Intersection {
                                inner: Some(Box::new(sub)),
                                index: i,
                                position: current,
                                normal,
                            });
                        }
                    } else {
                        return Some(Intersection {
                            inner: None,
                            index: i,
                            position: current,
                            normal,
                        });
                    }
                }
            }
        }

        None
    }

    fn get_hit(&self, intersection: &Intersection) -> Option<&<T::Data as Data>::Child> {
        self.get(intersection.index)
    }

    fn get_hit_mut(&mut self, intersection: &Intersection) -> Option<&mut <T::Data as Data>::Child> {
        self.get_mut(intersection.index)
    }
}

/// raycast: the Raycast implementation that will be cast on
/// current: the current position on the ray
/// direction: the direction of the ray
fn cast<R: Raycast>(
    raycast: &R,
    ray: &Ray,
    mut current: Vec3,
    direction: Vec3,
    iterations: usize,
) -> Option<Intersection> {
    // keep the current location as integer coordinates, to mitigate rounding errors on
    //  integrated values
    let mut current_i = [
        current[0].floor() as isize,
        current[1].floor() as isize,
        current[2].floor() as isize,
    ];
    for i in 0..3 {
        if current[i] - current[i].floor() < std::f32::EPSILON && direction[i] < 0.0 {
            current_i[i] -= 1;
        }
    }

    let normals = [
        vec3(1.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        vec3(0.0, 0.0, 1.0),
    ];

    // don't forget to skip the starting position
    if let Some(hit) = raycast.check(ray, current, current_i, vec3(0.0, 0.0, 0.0)) {
        return Some(hit);
    }

    // try to find the nearest voxel hit
    for _ in 0..iterations {
        // try to find the nearest intersection with the grid
        let i = vec3(
            intersect(current_i[0], current[0], direction[0]),
            intersect(current_i[1], current[1], direction[1]),
            intersect(current_i[2], current[2], direction[2]),
        );

        // advance by the distance to the nearest grid intersection
        for d in 0..3 {
            let e = (d + 1) % 3;
            let f = (d + 2) % 3;
            if i[d] <= i[e] && i[d] <= i[f] {
                current += direction * i[d];
                if direction[d] < 0.0 {
                    current_i[d] -= 1;
                    current[d] = current_i[d] as f32 + 1.0;
                    if let Some(hit) = raycast.check(ray, current, current_i, normals[d]) {
                        return Some(hit);
                    }
                } else {
                    current_i[d] += 1;
                    current[d] = current_i[d] as f32;
                    if let Some(hit) = raycast.check(ray, current, current_i, -normals[d]) {
                        return Some(hit);
                    }
                }
                break;
            }
        }
    }
    None
}

/// find nearest intersection with a 1d grid, with grid lines at all integer positions
fn intersect(reference: isize, position: f32, direction: f32) -> f32 {
    if direction == 0.0 {
        ::std::f32::INFINITY
    } else {
        let target = if direction < 0.0 {
            reference as f32
        } else {
            (reference + 1) as f32
        };
        (target - position) / direction
    }
}

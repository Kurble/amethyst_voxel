use crate::ambient_occlusion::*;
use crate::context::Context;
use crate::material::AtlasMaterialHandle;
use crate::side::*;
use crate::voxel::*;
use nalgebra_glm::*;
use rendy::mesh::{Normal, Position, Tangent};
use std::iter::repeat;

pub struct Texturing {
    pub material_id: u32,
    pub side: u8,
    pub coord: u8,
    pub ao: f32,
}

/// Triangulated mesh data created from a single voxel definition.
#[derive(Default)]
pub struct Mesh {
    pub pos: Vec<Position>,
    pub nml: Vec<Normal>,
    pub tan: Vec<Tangent>,
    pub tex: Vec<Texturing>,
    pub ind: Vec<u32>,
}

impl Mesh {
    /// Create a new mesh
    pub fn build<'a, T: Voxel, C: Context<T>>(
        &mut self,
        root: &T,
        ao: &AmbientOcclusion,
        context: C,
        origin: Vec3,
        scale: f32,
    ) {
        root.triangulate::<Left, C>(self, ao, context.clone(), origin, scale);
        root.triangulate::<Right, C>(self, ao, context.clone(), origin, scale);
        root.triangulate::<Below, C>(self, ao, context.clone(), origin, scale);
        root.triangulate::<Above, C>(self, ao, context.clone(), origin, scale);
        root.triangulate::<Back, C>(self, ao, context.clone(), origin, scale);
        root.triangulate::<Front, C>(self, ao, context.clone(), origin, scale);
    }
}

pub fn triangulate_detail<'a, T, S, C>(
    mesh: &mut Mesh,
    ao: &'a AmbientOcclusion<'a>,
    context: C,
    origin: Vec3,
    scale: f32,
    sub: &[ChildOf<T>],
) where
    T: Voxel,
    S: Side,
    C: Context<T>,
{
    // the scale of a single sub-voxel
    let scale = scale * T::SCALE;
    // loop over all sub-voxels and check for visible faces
    for i in 0..T::COUNT {
        if sub[i].visible() {
            let x = (i) & T::LAST;
            let y = (i >> <T::Data as Data>::SUBDIV) & T::LAST;
            let z = (i >> (<T::Data as Data>::SUBDIV * 2)) & T::LAST;
            let j = (i as isize + S::offset::<T>()) as usize;

            if sub[i].render()
                || (S::accept::<T>(x, y, z) && sub[j].render())
                || context.render(x as isize + S::DX, y as isize + S::DY, z as isize + S::DZ)
            {
                let ao = &ao.sub(x, y, z);
                let ctx = context.child(x as isize, y as isize, z as isize);
                let src = vec3(
                    origin.x + x as f32 * scale,
                    origin.y + y as f32 * scale,
                    origin.z + z as f32 * scale,
                );

                // add the visible face
                sub[i].triangulate::<S, _>(mesh, ao, ctx, src, scale);
            }
        }
    }
}

pub fn triangulate_face<T, S>(
    mesh: &mut Mesh,
    ao: &AmbientOcclusion,
    origin: Vec3,
    scale: f32,
    material: AtlasMaterialHandle,
) where
    T: Data,
    S: Side,
{
    let sc = scale * 0.5;
    let quad = [
        vec3(-sc, sc, sc),
        vec3(sc, sc, sc),
        vec3(sc, -sc, sc),
        vec3(-sc, -sc, sc),
    ];
    let begin = mesh.pos.len() as u32;
    let transform = S::orientation();
    let center = vec3(origin.x + sc, origin.y + sc, origin.z + sc);
    let normal = transform * vec3(0.0, 0.0, 1.0);
    let tangent = transform * vec3(1.0, 0.0, 0.0);
    let occlusion = ao.quad::<T, S>();

    mesh.pos.extend(
        quad.iter()
            .map(|pos| Position(convert3(transform * pos + center))),
    );
    mesh.nml.extend(repeat(Normal(convert3(normal))).take(4));
    mesh.tan.extend(repeat(Tangent(convert4(tangent))).take(4));
    mesh.tex
        .extend(occlusion.iter().enumerate().map(|(i, &ao)| Texturing {
            material_id: material.0,
            side: S::SIDE as u8,
            coord: i as u8,
            ao,
        }));

    if occlusion[0] + occlusion[2] > occlusion[1] + occlusion[3] {
        mesh.ind
            .extend_from_slice(&[begin, begin + 1, begin + 2, begin, begin + 2, begin + 3]);
    } else {
        mesh.ind
            .extend_from_slice(&[begin, begin + 1, begin + 3, begin + 1, begin + 2, begin + 3]);
    }
}

#[inline]
fn convert3(v: Vec3) -> [f32; 3] {
    [v[0], v[1], v[2]]
}

#[inline]
fn convert4(v: Vec3) -> [f32; 4] {
    [v[0], v[1], v[2], 1.0]
}

use std::iter::repeat;
use crate::coordinate::*;
use crate::voxel::*;
use crate::context::Context;
use crate::side::*;
use crate::ambient_occlusion::*;
use crate::material::VoxelMaterialId;

use nalgebra_glm::*;

use rendy::mesh::{Position, Normal, Tangent};

pub(crate) struct Const<T>(T);

/// Triangulated mesh data created from a single voxel definition.
pub struct Mesh {
    pub pos: Vec<Position>,
    pub nml: Vec<Normal>,
    pub tan: Vec<Tangent>,
    pub tex: Vec<(u32, f32)>,
    pub ind: Vec<u32>,
}

impl<T: VoxelData> Const<T> {
    pub const WIDTH: usize = 1 << T::SUBDIV;
    pub const AO_WIDTH: usize = Self::WIDTH + 1;
    pub const LAST: usize = Self::WIDTH - 1;
    pub const COUNT: usize = Self::WIDTH * Self::WIDTH * Self::WIDTH;
    pub const DX: usize = 1;
    pub const DY: usize = Self::DX * Self::WIDTH;
    pub const DZ: usize = Self::DY * Self::WIDTH;
    pub const SCALE: f32 = 1.0 / Self::WIDTH as f32;

    pub fn coord_to_index(x: usize, y: usize, z: usize) -> usize {
        x * Self::DX + y * Self::DY + z * Self::DZ
    }

    pub fn index_to_coord(index: usize) -> (usize, usize, usize) {
        let x = index & Self::LAST;
        let y = (index >> T::SUBDIV) & Self::LAST;
        let z = (index >> (T::SUBDIV*2)) & Self::LAST;
        (x, y, z)
    }
}

impl Mesh {
    /// Create a new mesh
    pub fn build<V: AsVoxel, C: Context>(root: &V::Voxel, ao: &AmbientOcclusion, context: &C, origin: Pos, scale: f32) -> Self {
        let mut result = Self { 
            pos: Vec::new(), 
            nml: Vec::new(),
            tan: Vec::new(),
            tex: Vec::new(),
            ind: Vec::new(),
        };
        root.triangulate_all(&mut result, ao, context, origin, scale);
        result
    }
}

#[inline]
fn convert3(v: Vec3) -> [f32; 3] { [v[0], v[1], v[2]] }

#[inline]
fn convert4(v: Vec3) -> [f32; 4] { [v[0], v[1], v[2], 1.0] }

pub fn triangulate_detail<'a, T, U, V, S, Q, C>(mesh: &mut Mesh, ao: &'a AmbientOcclusion<'a>, context: &'a C, origin: Pos, scale: f32, sub: &[V])
    where
        T: VoxelData,
        U: VoxelData,
        V: Voxel<U>,
        S: Side<T>,
        Q: Side<U>,
        C: Context,
{
    // the scale of a single sub-voxel
    let scale = scale * Const::<T>::SCALE;
    // loop over all sub-voxels and check for visible faces
    for i in 0..Const::<T>::COUNT {
        if sub[i].visible() {
            let x = (i) & Const::<T>::LAST;
            let y = (i >> T::SUBDIV) & Const::<T>::LAST;
            let z = (i >> (T::SUBDIV * 2)) & Const::<T>::LAST;
            let j = (i as isize + S::OFFSET) as usize;

            if sub[i].render() || 
                (S::accept(x, y, z) && sub[j].render()) || 
                context.render(x as isize + S::DX, y as isize + S::DY, z as isize + S::DZ) 
            {
                let ao = &ao.sub(x, y, z);
                let ctx = &context.clone().child(x as isize, y as isize, z as isize);
                let src = Pos {
                    x: origin.x + x as f32 * scale,
                    y: origin.y + y as f32 * scale,
                    z: origin.z + z as f32 * scale,
                };

                // add the visible face
                sub[i].triangulate_self::<Q, C::Child>(mesh, ao, ctx, src, scale);
            }
        }
    }
}

pub fn triangulate_face<T, S>(m: &mut Mesh, ao: &AmbientOcclusion, ori: Pos, sc: f32, mat: VoxelMaterialId) 
    where
        T: VoxelData,
        S: Side<T>,
{
    let sc = sc * 0.5;
    let quad = [vec3(-sc, sc, sc), vec3(sc, sc, sc), vec3(sc, -sc, sc), vec3(-sc, -sc, sc)];
    let begin = m.pos.len() as u32;
    let transform = S::orientation();
    let center = vec3(ori.x+sc, ori.y+sc, ori.z+sc);
    let normal = transform * vec3(0.0, 0.0, 1.0);
    let tangent = transform * vec3(1.0, 0.0, 0.0);
    let occlusion = ao.quad::<T, S>();

    m.pos.extend(quad.iter().map(|pos| Position(convert3(transform*pos + center))));
    m.nml.extend(repeat(Normal(convert3(normal))).take(4));
    m.tan.extend(repeat(Tangent(convert4(tangent))).take(4));
    m.tex.extend(repeat(mat.0).zip(occlusion.iter().cloned()));

    if occlusion[0]+occlusion[2] > occlusion[1]+occlusion[3] {
        m.ind.extend_from_slice(&[begin, begin+1, begin+2, begin, begin+2, begin+3]);
    } else {
        m.ind.extend_from_slice(&[begin, begin+1, begin+3, begin+1, begin+2, begin+3]);
    }
}
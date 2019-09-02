use std::iter::repeat;
use crate::coordinate::*;
use crate::voxel::*;
use crate::side::*;

use nalgebra_glm::*;

use rendy::mesh::{PosNorm, Position, Normal, TexCoord};

pub(crate) struct Const<T>(T);

impl<T: VoxelData> Const<T> {
    pub const WIDTH: usize = 1 << T::SUBDIV;
    pub const LAST: usize = Self::WIDTH - 1;
    pub const COUNT: usize = Self::WIDTH * Self::WIDTH * Self::WIDTH;
    pub const DX: usize = 1;
    pub const DY: usize = Self::DX * Self::WIDTH;
    pub const DZ: usize = Self::DY * Self::WIDTH;
    pub const SCALE: f32 = 1.0 / Self::WIDTH as f32;
}

/// Triangulated mesh data created from a single voxel definition.
pub struct Mesh {
    pub pos: Vec<Position>,
    pub nml: Vec<Normal>,
    pub tex: Vec<TexCoord>,
    pub ind: Vec<u32>,
}

impl Mesh {
    /// Create a new mesh
    pub fn build<V: AsVoxel>(root: &V::Voxel, origin: Pos, scale: f32) -> Self {
        let mut result = Self { 
            pos: Vec::new(), 
            nml: Vec::new(),
            tex: Vec::new(),
            ind: Vec::new(),
        };
        root.triangulate_all(&mut result, origin, scale);
        result
    }
}

pub fn triangulate_detail<T, U, V, S, Q>(m: &mut Mesh, origin: Pos, scale: f32, sub: &[V])
    where
        T: VoxelData,
        U: VoxelData,
        V: Voxel<U>,
        S: Side<T>,
        Q: Side<U>,
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

            if (S::accept(x, y, z) && sub[j].render()) || sub[i].render() || !S::accept(x, y, z) {
                let src = Pos {
                    x: origin.x + x as f32 * scale,
                    y: origin.y + y as f32 * scale,
                    z: origin.z + z as f32 * scale,
                };

                // add the visible face
                sub[i].triangulate_self::<Q>(m, src, scale);
            }
        }
    }
}

#[inline]
fn convert(v: Vec3) -> [f32; 3] { [v[0], v[1], v[2]] }

pub fn triangulate_face<T, S>(m: &mut Mesh, ori: Pos, sc: f32, mat: u32) where
    T: VoxelData,
    S: Side<T>,
{
    let sc = sc * 0.5;
    let quad = [vec3(-sc, sc, sc), vec3(sc, sc, sc), vec3(sc, -sc, sc), vec3(-sc, -sc, sc)];
    let transform = S::orientation();
    let center = vec3(ori.x+sc, ori.y+sc, ori.z+sc);
    let up = vec3(0.0, 0.0, 1.0);

    m.pos.extend(quad.iter().map(|pos| Position(convert(transform*pos + center))));
    m.nml.extend(repeat(Normal(convert(transform*up))).take(4));
    m.tex.extend(repeat(TexCoord([0.0, 0.0])).take(4));

    let begin = m.pos.len() as u32;

    m.ind.extend_from_slice(&[begin, begin+1, begin+2, begin, begin+2, begin+3]);
}
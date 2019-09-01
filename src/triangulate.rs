use crate::coordinate::*;
use crate::voxel::*;
use crate::side::*;

use nalgebra_glm::*;
use std::time::*;
use rendy::mesh::{AsVertex, PosNorm, VertexFormat};

pub(crate) struct Const<T>(T);

impl<T: Metadata> Const<T> {
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
    pub vbuf: Vec<Vertex>,
    pub ibuf: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nml: [f32; 3],
    pub mat: u32,
}

impl Mesh {
    /// Create a new mesh
    pub fn new(root: &dyn GenericVoxel, origin: Pos, scale: f32) -> Self {
        let now = Instant::now();
        let mut result = Self { vbuf: Vec::with_capacity(2048), ibuf: Vec::with_capacity(2048) };
        result.reuse(root, origin, scale);
        println!("Triangulated in {:?}", now.elapsed());
        result
    }

    /// Re-use the vertex and index buffer for a new mesh
    pub fn reuse(&mut self, root: &dyn GenericVoxel, origin: Pos, scale: f32) {
        self.vbuf.clear();
        self.ibuf.clear();
        root.triangulate_all(self, origin, scale);
    }
}

impl AsVertex for Vertex {
    fn vertex() -> VertexFormat {
        VertexFormat::with_stride(PosNorm::vertex(), 36)
    }
}

pub fn triangulate_detail<T, U, V, S, Q>(m: &mut Mesh, origin: Pos, scale: f32, sub: &[V])
    where
        T: Metadata,
        U: Metadata,
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
    T: Metadata,
    S: Side<T>,
{
    let sc = sc * 0.5;
    let transform = S::orientation();
    let center = vec3(ori.x+sc, ori.y+sc, ori.z+sc);
    let up = vec3(0.0, 0.0, 1.0);

    m.vbuf.extend([vec3(-sc, sc, sc), vec3(sc, sc, sc), vec3(sc, -sc, sc), vec3(-sc, -sc, sc)]
        .iter()
        .map(|pos| Vertex {
            pos: convert(transform*pos + center),
            nml: convert(transform*up),
            mat: mat,
        })
    );

    let begin = m.vbuf.len() as u32;

    m.ibuf.extend_from_slice(&[begin, begin+1, begin+2, begin, begin+2, begin+3]);
}
use std::iter::{repeat, FromIterator};
use crate::coordinate::*;
use crate::voxel::*;
use crate::side::*;
use crate::material::VoxelMaterialId;

use nalgebra_glm::*;

use rendy::mesh::{Position, Normal, Tangent};

pub(crate) struct Const<T>(T);

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
}

/// Triangulated mesh data created from a single voxel definition.
pub struct Mesh {
    pub pos: Vec<Position>,
    pub nml: Vec<Normal>,
    pub tan: Vec<Tangent>,
    pub tex: Vec<(u32, f32)>,
    pub ind: Vec<u32>,
}

pub enum AmbientOcclusion<'a> {
    Big {
        ao: Vec<u16>,
        w: usize,
    },
    Borrowed { ao: &'a Self },
    Small {
        ao: [u16; 8],
    },
}

pub trait BuildAmbientOcclusion<T: VoxelData, U: Voxel<T>> {
    fn build(from: &U) -> AmbientOcclusion;
}

impl Mesh {
    /// Create a new mesh
    pub fn build<V: AsVoxel>(root: &V::Voxel, origin: Pos, scale: f32) -> Self {
        let mut result = Self { 
            pos: Vec::new(), 
            nml: Vec::new(),
            tan: Vec::new(),
            tex: Vec::new(),
            ind: Vec::new(),
        };
        root.triangulate_all(&mut result, origin, scale);
        result
    }
}

impl AmbientOcclusion<'_> {
    pub fn sub(&self, x: usize, y: usize, z: usize) -> AmbientOcclusion {
        match *self {
            AmbientOcclusion::Small{ ao } => AmbientOcclusion::Small{ ao },
            AmbientOcclusion::Borrowed{ ao } => ao.sub(x, y, z),
            AmbientOcclusion::Big{ ref ao, w } => {
                let i = x+y*w+z*w*w;
                let x = 1;
                let y = w;
                let z = w*w;
                AmbientOcclusion::Small {
                    ao: [ao[i  ], ao[i  +x], ao[i+  y], ao[i+  y+x],
                         ao[i+z], ao[i+z+x], ao[i+z+y], ao[i+z+y+x]],
                }
            },
        }
    }

    pub fn quad<T: VoxelData, S: Side<T>>(&self) -> [f32; 4] {
        let f = |d: u16, s: u16| 1.0 - f32::from((d >> s) & 0x03) / 4.0;
        match *self {
            AmbientOcclusion::Small{ ao } => {
                match S::SIDE {
                    0 => [f(ao[6], 10), f(ao[2], 10), f(ao[0], 10), f(ao[4], 10)],
                    1 => [f(ao[3],  8), f(ao[7],  8), f(ao[5],  8), f(ao[1],  8)],
                    2 => [f(ao[4],  6), f(ao[5],  6), f(ao[1],  6), f(ao[0],  6)],
                    3 => [f(ao[3],  4), f(ao[2],  4), f(ao[6],  4), f(ao[7],  4)],
                    4 => [f(ao[2],  2), f(ao[3],  2), f(ao[1],  2), f(ao[0],  2)],
                    5 => [f(ao[7],  0), f(ao[6],  0), f(ao[4],  0), f(ao[5],  0)],
                    _ => unreachable!(),
                }
            },
            _ => unreachable!(),
        }        
    }
}

impl<T: VoxelData, U: VoxelData, V: Voxel<U>> BuildAmbientOcclusion<T, Nested<T, U, V>> for AmbientOcclusion<'_> {
    fn build(root: &Nested<T, U, V>) -> AmbientOcclusion {
        let w = Const::<T>::AO_WIDTH as isize;
        match *root {
            Nested::Empty{ .. } |
            Nested::Material{ .. } => AmbientOcclusion::Small {  
                ao: [0xfff; 8],
            },

            Nested::Detail{ ref detail, .. } => {
                let bound = |x| x < 0 || x > Const::<T>::LAST as isize;
                let sample = |x, y, z| if bound(x) || bound(y) || bound(z) {
                    0
                } else if detail[Const::<T>::coord_to_index(x as usize, y as usize, z as usize)].visible() { 
                    1 
                } else { 
                    0 
                };
                let process = |s: [u16; 8]| {
                    let neg_x = (s[0]+s[1]+s[4]+s[5]).min(3);
                    let pos_x = (s[2]+s[3]+s[6]+s[7]).min(3);
                    let neg_y = (s[0]+s[1]+s[2]+s[3]).min(3);
                    let pos_y = (s[4]+s[5]+s[6]+s[7]).min(3);
                    let neg_z = (s[0]+s[2]+s[4]+s[6]).min(3);
                    let pos_z = (s[1]+s[3]+s[5]+s[7]).min(3);

                    (neg_x << 10)|(pos_x << 8)|(neg_y << 6)|(pos_y << 4)|(neg_z << 2)|(pos_z)
                };

                AmbientOcclusion::Big {
                    ao: Vec::from_iter((0..w)
                        .flat_map(move |z| (0..w)
                            .flat_map(move |y| (0..w)
                                .map(move |x| process([
                                    sample(x-1, y-1, z-1), sample(x-1, y-1, z),
                                    sample(x,   y-1, z-1), sample(x,   y-1, z),
                                    sample(x-1, y,   z-1), sample(x-1, y,   z),
                                    sample(x,   y,   z-1), sample(x,   y,   z),
                                ]))))),
                    w: Const::<T>::AO_WIDTH,
                }
            },
        }        
    }
}

impl BuildAmbientOcclusion<(), Simple> for AmbientOcclusion<'_> { 
    fn build(_: &Simple) -> AmbientOcclusion {
        AmbientOcclusion::Small {  
            ao: [0xfff; 8],
        }
    }
}

pub fn triangulate_detail<'a, T, U, V, S, Q>(mesh: &mut Mesh, ao: &'a AmbientOcclusion<'a>, origin: Pos, scale: f32, sub: &[V])
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
                sub[i].triangulate_self::<Q>(mesh, &ao.sub(x, y, z), src, scale);
            }
        }
    }
}

#[inline]
fn convert(v: Vec3) -> [f32; 3] { [v[0], v[1], v[2]] }

#[inline]
fn convert4(v: Vec3) -> [f32; 4] { [v[0], v[1], v[2], 1.0] }

pub fn triangulate_face<T, S>(m: &mut Mesh, ao: &AmbientOcclusion, ori: Pos, sc: f32, mat: VoxelMaterialId) where
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

    m.pos.extend(quad.iter().map(|pos| Position(convert(transform*pos + center))));
    m.nml.extend(repeat(Normal(convert(normal))).take(4));
    m.tan.extend(repeat(Tangent(convert4(tangent))).take(4));
    m.tex.extend(repeat(mat.0).zip(ao.quad::<T, S>().iter().cloned()));
    m.ind.extend_from_slice(&[begin, begin+1, begin+2, begin, begin+2, begin+3]);
}
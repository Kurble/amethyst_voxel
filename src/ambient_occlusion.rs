use std::iter::{FromIterator};

use crate::voxel::*;
use crate::side::*;
use crate::triangulate::*;

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

pub(crate) trait BuildAmbientOcclusion<T: VoxelData, U: Voxel<T>> {
    fn build(from: &U) -> AmbientOcclusion;
}

impl<T, U, V> BuildAmbientOcclusion<T, Nested<T, U, V>> for AmbientOcclusion<'_> where
    T: VoxelData, 
    U: VoxelData, 
    V: Voxel<U>
{
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

impl AmbientOcclusion<'_> {
    pub(crate) fn sub(&self, x: usize, y: usize, z: usize) -> AmbientOcclusion {
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

    pub(crate) fn quad<T: VoxelData, S: Side<T>>(&self) -> [f32; 4] {
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

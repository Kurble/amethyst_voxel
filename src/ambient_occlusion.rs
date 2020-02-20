use crate::context::Context;
use crate::side::Side;
use crate::voxel::{Data, Voxel};
use std::collections::HashMap;

pub enum AmbientOcclusion<'a> {
    Big {
        occlusion: Vec<u16>,
        detail: HashMap<usize, AmbientOcclusion<'a>>,
        width: usize,
    },
    Borrowed {
        target: &'a Self,
    },
    Small {
        occlusion: [u16; 8],
    },
}

impl AmbientOcclusion<'_> {
    pub fn build<'a, T: Voxel, C: Context<T>>(root: &T, neighbours: &C) -> Self {
        let w = T::AO_WIDTH as isize;
        if root.is_detail() {
            let bound = |x| x < 0 || x > T::LAST as isize;
            let sample = |x, y, z| {
                if bound(x) || bound(y) || bound(z) {
                    if neighbours.visible(x, y, z) {
                        1
                    } else {
                        0
                    }
                } else if root
                    .get(T::coord_to_index(
                        x as usize, y as usize, z as usize,
                    ))
                    .unwrap()
                    .visible()
                {
                    1
                } else {
                    0
                }
            };
            let process = |s: [u16; 8]| {
                let table = |s: [u16; 4]| match s {
                    [0, 0, 0, 0] => 0,
                    [1, 0, 0, 0] | [0, 1, 0, 0] | [0, 0, 1, 0] | [0, 0, 0, 1] => 1,
                    [1, 1, 0, 0] | [0, 0, 1, 1] | [0, 1, 0, 1] | [1, 0, 1, 0] => 2,
                    _ => 3,
                };
                let neg_x = table([s[0], s[1], s[4], s[5]]);
                let pos_x = table([s[2], s[3], s[6], s[7]]);
                let neg_y = table([s[0], s[1], s[2], s[3]]);
                let pos_y = table([s[4], s[5], s[6], s[7]]);
                let neg_z = table([s[0], s[2], s[4], s[6]]);
                let pos_z = table([s[1], s[3], s[5], s[7]]);

                (neg_x << 10) | (pos_x << 8) | (neg_y << 6) | (pos_y << 4) | (neg_z << 2) | (pos_z)
            };

            let occlusion = (0..w)
                .flat_map(move |z| {
                    (0..w).flat_map(move |y| {
                        (0..w).map(move |x| {
                            process([
                                sample(x - 1, y - 1, z - 1),
                                sample(x - 1, y - 1, z),
                                sample(x, y - 1, z - 1),
                                sample(x, y - 1, z),
                                sample(x - 1, y, z - 1),
                                sample(x - 1, y, z),
                                sample(x, y, z - 1),
                                sample(x, y, z),
                            ])
                        })
                    })
                })
                .collect();

            AmbientOcclusion::Big {
                occlusion,
                detail: (0..T::COUNT)
                    .filter_map(|index| {
                        root.get(index).and_then(|voxel| {
                            if voxel.is_detail() {
                                let (x, y, z) = T::index_to_coord(index);
                                Some((
                                    index,
                                    Self::build(
                                        voxel,
                                        &neighbours.child(x as isize, y as isize, z as isize),
                                    ),
                                ))
                            } else {
                                None
                            }
                        })
                    })
                    .collect(),
                width: T::AO_WIDTH,
            }
        } else {
            AmbientOcclusion::Small {
                occlusion: [0xfff; 8],
            }
        }
    }

    pub fn sub(&self, x: usize, y: usize, z: usize) -> AmbientOcclusion {
        match *self {
            AmbientOcclusion::Big {
                ref occlusion,
                ref detail,
                width,
            } => {
                let index = x + y * width + z * width * width;
                detail
                    .get(&index)
                    .map(|target| AmbientOcclusion::Borrowed { target })
                    .unwrap_or_else(|| {
                        let x = 1;
                        let y = width;
                        let z = width * width;
                        AmbientOcclusion::Small {
                            occlusion: [
                                occlusion[index],
                                occlusion[index + x],
                                occlusion[index + y],
                                occlusion[index + y + x],
                                occlusion[index + z],
                                occlusion[index + z + x],
                                occlusion[index + z + y],
                                occlusion[index + z + y + x],
                            ],
                        }
                    })
            }

            AmbientOcclusion::Borrowed { target } => target.sub(x, y, z),

            AmbientOcclusion::Small { .. } => unreachable!(),
        }
    }

    pub fn quad<T: Data, S: Side>(&self) -> [f32; 4] {
        let f = |d: u16, s: u16| 1.0 - f32::from((d >> s) & 0x03) / 4.0;
        match *self {
            AmbientOcclusion::Small { occlusion } => {
                let o = &occlusion;
                match S::SIDE {
                    0 => [f(o[6], 10), f(o[2], 10), f(o[0], 10), f(o[4], 10)],
                    1 => [f(o[3], 8), f(o[7], 8), f(o[5], 8), f(o[1], 8)],
                    2 => [f(o[5], 6), f(o[4], 6), f(o[0], 6), f(o[1], 6)],
                    3 => [f(o[3], 4), f(o[2], 4), f(o[6], 4), f(o[7], 4)],
                    4 => [f(o[2], 2), f(o[3], 2), f(o[1], 2), f(o[0], 2)],
                    5 => [f(o[7], 0), f(o[6], 0), f(o[4], 0), f(o[5], 0)],
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}

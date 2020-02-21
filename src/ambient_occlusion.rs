use crate::context::Context;
use crate::side::Side;
use crate::voxel::Voxel;
use std::collections::HashMap;

pub enum SharedVertexData<'a> {
    Big {
        occlusion: Vec<Vertex>,
        detail: HashMap<usize, SharedVertexData<'a>>,
        width: usize,
    },
    Borrowed {
        target: &'a Self,
    },
    Small {
        occlusion: [Vertex; 8],
    },
}

#[derive(Clone, Copy)]
pub struct Vertex {
    occlusion: u16,
    skins: [(u8, u8); 4],
}

pub struct SharedVertex {
    pub occlusion: f32,
    pub skins: [(u8, u8); 4],
}

impl SharedVertexData<'_> {
    pub fn build<'a, T: Voxel, C: Context<T>>(root: &T, neighbours: &C) -> Self {
        let w = T::AO_WIDTH as isize;
        if root.is_detail() {
            let bound = |x| x < 0 || x > T::LAST as isize;
            let sample_occlusion = |x, y, z| {
                if bound(x) || bound(y) || bound(z) {
                    if neighbours.visible(x, y, z) {
                        1
                    } else {
                        0
                    }
                } else if root
                    .get(T::coord_to_index(x as usize, y as usize, z as usize))
                    .unwrap()
                    .visible()
                {
                    1
                } else {
                    0
                }
            };
            let sample_skin = |x, y, z| {
                if bound(x) || bound(y) || bound(z) {
                    neighbours.skin(x, y, z)
                } else {
                    root.get(T::coord_to_index(x as usize, y as usize, z as usize))
                        .unwrap()
                        .skin()
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
                            let occlusion = process([
                                sample_occlusion(x - 1, y - 1, z - 1),
                                sample_occlusion(x - 1, y - 1, z),
                                sample_occlusion(x, y - 1, z - 1),
                                sample_occlusion(x, y - 1, z),
                                sample_occlusion(x - 1, y, z - 1),
                                sample_occlusion(x - 1, y, z),
                                sample_occlusion(x, y, z - 1),
                                sample_occlusion(x, y, z),
                            ]);

                            let mut skins = [(0u8, 0u8); 4];

                            for skin in [
                                sample_skin(x - 1, y - 1, z - 1),
                                sample_skin(x - 1, y - 1, z),
                                sample_skin(x, y - 1, z - 1),
                                sample_skin(x, y - 1, z),
                                sample_skin(x - 1, y, z - 1),
                                sample_skin(x - 1, y, z),
                                sample_skin(x, y, z - 1),
                                sample_skin(x, y, z),
                            ].iter().filter_map(|&e| e) {
                                for i in 0..4 {
                                    if skins[i].0 == skin {
                                        skins[i].1 += 1;
                                        break;
                                    }
                                    if skins[i].1 == 0 {
                                        skins[i] = (skin, 1);
                                        break;
                                    }
                                }
                            }

                            let mut total: u16 = skins.iter().map(|s| s.1 as u16).sum();
                            let mut left: u16 = 255;

                            skins[0].1 = 255;

                            for i in 0..4 {
                                if total > 0 {
                                    let points = skins[1].1 as u16;
                                    skins[i].1 = ((points * left) / total) as u8;
                                    total -= points;
                                    left -= skins[1].1 as u16;
                                }
                            }

                            assert_eq!(skins.iter().map(|s| s.1 as u16).sum::<u16>(), 255u16);

                            Vertex { occlusion, skins }
                        })
                    })
                })
                .collect();

            SharedVertexData::Big {
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
            SharedVertexData::Small {
                occlusion: [Vertex {
                    occlusion: 0xfff,
                    skins: [(0, 64); 4],
                }; 8],
            }
        }
    }

    pub fn sub(&self, x: usize, y: usize, z: usize) -> SharedVertexData {
        match *self {
            SharedVertexData::Big {
                ref occlusion,
                ref detail,
                width,
            } => {
                let index = x + y * width + z * width * width;
                detail
                    .get(&index)
                    .map(|target| SharedVertexData::Borrowed { target })
                    .unwrap_or_else(|| {
                        let x = 1;
                        let y = width;
                        let z = width * width;
                        SharedVertexData::Small {
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

            SharedVertexData::Borrowed { target } => target.sub(x, y, z),

            SharedVertexData::Small { .. } => unreachable!(),
        }
    }

    pub fn quad<S: Side>(&self) -> [SharedVertex; 4] {
        let f = |d: Vertex, s: u16| SharedVertex {
            occlusion: 1.0 - f32::from((d.occlusion >> s) & 0x03) / 4.0,
            skins: d.skins,
        };
        match *self {
            SharedVertexData::Small { occlusion } => {
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

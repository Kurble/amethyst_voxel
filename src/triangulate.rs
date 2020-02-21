use crate::ambient_occlusion::*;
use crate::context::Context;
use crate::material::{AtlasAccess, AtlasMaterialHandle};
use crate::pass::Surface;
use crate::side::*;
use crate::voxel::*;
use amethyst::renderer::{
    rendy::{command::QueueId, factory::Factory, mesh::MeshBuilder},
    skinning::{JointCombined, JointIds, JointWeights},
    types::{Backend, Mesh},
};
use nalgebra_glm::*;
use rendy::mesh::{Normal, Position, Tangent};
use std::iter::repeat;

/// Triangulated mesh data created from a single voxel definition.
pub struct Triangulation {
    skinned: bool,
    pos: Vec<Position>,
    nml: Vec<Normal>,
    tan: Vec<Tangent>,
    tex: Vec<Texturing>,
    jnt: Vec<JointCombined>,
    ind: Vec<u32>,
}

struct Texturing {
    material_id: u32,
    side: u8,
    coord: u8,
    ao: f32,
}

impl Triangulation {
    pub fn new(skinned: bool) -> Self {
        Triangulation {
            skinned,
            pos: Vec::new(),
            nml: Vec::new(),
            tan: Vec::new(),
            tex: Vec::new(),
            jnt: Vec::new(),
            ind: Vec::new(),
        }
    }

    /// Create a new mesh
    pub fn append<'a, T: Voxel, C: Context<T>>(
        &mut self,
        root: &T,
        ao: &SharedVertexData,
        context: &C,
        origin: Vec3,
        scale: f32,
        transform: &Mat4x4,
    ) {
        let start = self.pos.len();
        root.triangulate::<Left, C>(self, ao, context, origin, scale);
        root.triangulate::<Right, C>(self, ao, context, origin, scale);
        root.triangulate::<Below, C>(self, ao, context, origin, scale);
        root.triangulate::<Above, C>(self, ao, context, origin, scale);
        root.triangulate::<Back, C>(self, ao, context, origin, scale);
        root.triangulate::<Front, C>(self, ao, context, origin, scale);
        for i in start..self.pos.len() {
            let pos: [f32; 3] = self.pos[i].0.into();
            let nml: [f32; 3] = self.nml[i].0.into();
            let tan: [f32; 3] = [self.tan[i].0[0], self.tan[i].0[1], self.tan[i].0[2]];
            self.pos[i] = transform.transform_point(&pos.into()).coords.into();
            self.nml[i] = transform.transform_vector(&nml.into()).into();
            let tan = transform.transform_vector(&tan.into());
            self.tan[i] = [tan[0], tan[1], tan[2], self.tan[i].0[3]].into();
        }
    }

    /// Transform into a rendy Mesh
    pub fn to_mesh<A, B>(self, atlas: &A, queue: QueueId, factory: &Factory<B>) -> Option<Mesh>
    where
        A: AtlasAccess,
        B: Backend,
    {
        if !self.pos.is_empty() {
            let tex = self
                .tex
                .into_iter()
                .map(|texturing| {
                    let [u, v] =
                        atlas.coord(texturing.material_id, texturing.side, texturing.coord);
                    Surface {
                        tex_ao: [u, v, texturing.ao],
                    }
                })
                .collect::<Vec<_>>();

            let mut builder = MeshBuilder::new()
                .with_indices(self.ind)
                .with_vertices(self.pos)
                .with_vertices(self.nml)
                .with_vertices(self.tan)
                .with_vertices(tex);

            if self.skinned {
                builder = builder.with_vertices(self.jnt);
            }

            Some(B::wrap_mesh(builder.build(queue, factory).unwrap()))
        } else {
            None
        }
    }
}

pub fn triangulate_detail<S, T, C>(
    triangulation: &mut Triangulation,
    shared: &SharedVertexData,
    context: &C,
    origin: Vec3,
    scale: f32,
    sub: &[ChildOf<T>],
) where
    S: Side,
    T: Voxel,
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
                let shared = shared.sub(x, y, z);
                let ctx = context.child(x as isize, y as isize, z as isize);
                let src = vec3(
                    origin.x + x as f32 * scale,
                    origin.y + y as f32 * scale,
                    origin.z + z as f32 * scale,
                );

                // add the visible face
                sub[i].triangulate::<S, _>(triangulation, &shared, &ctx, src, scale);
            }
        }
    }
}

pub fn triangulate_face<S: Side>(
    triangulation: &mut Triangulation,
    shared: &SharedVertexData,
    origin: Vec3,
    scale: f32,
    material: AtlasMaterialHandle,
) {
    let sc = scale * 0.5;
    let quad = [
        vec3(-sc, sc, sc),
        vec3(sc, sc, sc),
        vec3(sc, -sc, sc),
        vec3(-sc, -sc, sc),
    ];
    let begin = triangulation.pos.len() as u32;
    let transform = S::orientation();
    let center = vec3(origin.x + sc, origin.y + sc, origin.z + sc);
    let normal = transform * vec3(0.0, 0.0, 1.0);
    let tangent = transform * vec3(1.0, 0.0, 0.0);
    let shared = shared.quad::<S>();

    triangulation.pos.extend(
        quad.iter()
            .map(|pos| Position(convert3(transform * pos + center))),
    );
    triangulation
        .nml
        .extend(repeat(Normal(convert3(normal))).take(4));
    triangulation
        .tan
        .extend(repeat(Tangent(convert4(tangent))).take(4));
    triangulation
        .tex
        .extend(shared.iter().enumerate().map(|(i, shared)| Texturing {
            material_id: material.0,
            side: S::SIDE as u8,
            coord: i as u8,
            ao: shared.occlusion,
        }));

    if triangulation.skinned {
        triangulation
            .jnt
            .extend(shared.iter().map(|shared| JointCombined {
                joint_ids: JointIds([
                    shared.skins[0].0 as u16,
                    shared.skins[1].0 as u16,
                    shared.skins[2].0 as u16,
                    shared.skins[3].0 as u16,
                ]),
                joint_weights: JointWeights([
                    shared.skins[0].1 as f32 / 255.0,
                    shared.skins[1].1 as f32 / 255.0,
                    shared.skins[2].1 as f32 / 255.0,
                    shared.skins[3].1 as f32 / 255.0,
                ]),
            }));
    }

    if shared[0].occlusion + shared[2].occlusion > shared[1].occlusion + shared[3].occlusion {
        triangulation.ind.extend_from_slice(&[
            begin,
            begin + 1,
            begin + 2,
            begin,
            begin + 2,
            begin + 3,
        ]);
    } else {
        triangulation.ind.extend_from_slice(&[
            begin,
            begin + 1,
            begin + 3,
            begin + 1,
            begin + 2,
            begin + 3,
        ]);
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

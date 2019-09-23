use std::iter::repeat;
use crate::voxel::*;
use crate::context::Context;
use crate::side::*;
use crate::ambient_occlusion::*;
use crate::material::VoxelMaterialId;
use nalgebra_glm::*;
use rendy::mesh::{Position, Normal, Tangent};

/// The required functionality to triangulate voxels.
pub trait Triangulate<T: Data> {
    /// Returns whether this voxel is visible, i.e. if it has geometry.
    fn visible(&self) -> bool;

    /// Returns whether the neighbours of this voxel are visible if the camera was inside this voxel.
    fn render(&self) -> bool;

    /// Triangulate this voxel to the mesh.
    fn triangulate_self<S: Side<T>, C: Context<T>>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, context: &C, origin: Vec3, scale: f32);

    /// Triangulate this voxel to the mesh.
    fn triangulate_all<C: Context<T>>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, context: &C, origin: Vec3, scale: f32);
}

/// Triangulated mesh data created from a single voxel definition.
pub struct Mesh {
    pub pos: Vec<Position>,
    pub nml: Vec<Normal>,
    pub tan: Vec<Tangent>,
    pub tex: Vec<(u32, f32)>,
    pub ind: Vec<u32>,
}

impl Mesh {
    /// Create a new mesh
    pub fn build<T: Data, C: Context<T>>(root: &Voxel<T>, ao: &AmbientOcclusion, context: &C, origin: Vec3, scale: f32) -> Self {
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

impl<T: Data> Triangulate<T> for Voxel<T> {
    fn visible(&self) -> bool {
        match *self {
            Voxel::Empty { .. } => false,
            Voxel::Detail { ref data, .. } => !data.empty(),
            Voxel::Material { .. } => true,
        }
    }

    fn render(&self) -> bool {
        match *self {
            Voxel::Empty { .. } => true,
            Voxel::Detail { ref data, .. } => !data.solid(),
            Voxel::Material { .. } => false,
        }
    }

    fn triangulate_self<S: Side<T>, C: Context<T>>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, context: &C, origin: Vec3, scale: f32) {
        match *self {
            Voxel::Empty { .. } =>
                (),

            Voxel::Detail { ref detail, .. } => 
                triangulate_detail::<T,Self,S,C>(mesh, ao, context, origin, scale, detail.as_slice()),

            Voxel::Material { material, .. } =>
                triangulate_face::<T, S>(mesh, ao, origin, scale, material),
        }
    }

    fn triangulate_all<C: Context<T>>(&self, mesh: &mut Mesh, ao: &AmbientOcclusion, context: &C, origin: Vec3, scale: f32) {
        self.triangulate_self::<Left,C>(mesh, ao, context, origin, scale);
        self.triangulate_self::<Right,C>(mesh, ao, context, origin, scale);
        self.triangulate_self::<Below,C>(mesh, ao, context, origin, scale);
        self.triangulate_self::<Above,C>(mesh, ao, context, origin, scale);
        self.triangulate_self::<Back,C>(mesh, ao, context, origin, scale);
        self.triangulate_self::<Front,C>(mesh, ao, context, origin, scale);
    }
}

fn triangulate_detail<'a, D, T, S, C>(mesh: &mut Mesh, ao: &'a AmbientOcclusion<'a>, context: &'a C, 
                                      origin: Vec3, scale: f32, sub: &[T]) where
    D: Data,
    T: Triangulate<D>,
    S: Side<D>,
    C: Context<D>, 
{
    // the scale of a single sub-voxel
    let scale = scale * Const::<D>::SCALE;
    // loop over all sub-voxels and check for visible faces
    for i in 0..Const::<D>::COUNT {
        if sub[i].visible() {
            let x = (i) & Const::<D>::LAST;
            let y = (i >> D::SUBDIV) & Const::<D>::LAST;
            let z = (i >> (D::SUBDIV * 2)) & Const::<D>::LAST;
            let j = (i as isize + S::OFFSET) as usize;

            if sub[i].render() || 
                (S::accept(x, y, z) && sub[j].render()) || 
                context.render(x as isize + S::DX, y as isize + S::DY, z as isize + S::DZ) 
            {
                let ao = &ao.sub(x, y, z);
                let ctx = &context.clone().child(x as isize, y as isize, z as isize);
                let src = vec3(
                    origin.x + x as f32 * scale,
                    origin.y + y as f32 * scale,
                    origin.z + z as f32 * scale
                );

                // add the visible face
                sub[i].triangulate_self::<S, _>(mesh, ao, ctx, src, scale);
            }
        }
    }
}

fn triangulate_face<T, S>(mesh: &mut Mesh, ao: &AmbientOcclusion, 
                          origin: Vec3, scale: f32, material: VoxelMaterialId) where
    T: Data, 
    S: Side<T>,
{
    let sc = scale * 0.5;
    let quad = [vec3(-sc, sc, sc), vec3(sc, sc, sc), vec3(sc, -sc, sc), vec3(-sc, -sc, sc)];
    let begin = mesh.pos.len() as u32;
    let transform = S::orientation();
    let center = vec3(origin.x+sc, origin.y+sc, origin.z+sc);
    let normal = transform * vec3(0.0, 0.0, 1.0);
    let tangent = transform * vec3(1.0, 0.0, 0.0);
    let occlusion = ao.quad::<T, S>();

    mesh.pos.extend(quad.iter().map(|pos| Position(convert3(transform*pos + center))));
    mesh.nml.extend(repeat(Normal(convert3(normal))).take(4));
    mesh.tan.extend(repeat(Tangent(convert4(tangent))).take(4));
    mesh.tex.extend(repeat(material.0).zip(occlusion.iter().cloned()));

    if occlusion[0]+occlusion[2] > occlusion[1]+occlusion[3] {
        mesh.ind.extend_from_slice(&[begin, begin+1, begin+2, begin, begin+2, begin+3]);
    } else {
        mesh.ind.extend_from_slice(&[begin, begin+1, begin+3, begin+1, begin+2, begin+3]);
    }
}

#[inline]
fn convert3(v: Vec3) -> [f32; 3] { [v[0], v[1], v[2]] }

#[inline]
fn convert4(v: Vec3) -> [f32; 4] { [v[0], v[1], v[2], 1.0] }

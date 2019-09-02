use std::borrow::Cow;
use std::iter::repeat;
use amethyst::{
    ecs::prelude::*,
    assets::{Loader, Handle, AssetStorage},
    renderer::{
        types::{Texture},
        mtl::{Material, MaterialDefaults},
        rendy::{
            mesh::TexCoord,
            hal::image::{Kind, ViewKind},
            texture::{
                TextureBuilder,
                pixel::{Pixel, Rgba, _8, Unorm},
            },
        },
    },
};

/// A material
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelMaterial {
    pub albedo: [u8; 3],
    pub emission: [u8; 3],
    pub alpha: u8,
    pub metallic: u8,
    pub roughness: u8,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct VoxelMaterialId(pub(crate) u32);

pub struct VoxelMaterialStorage {
    materials: Vec<VoxelMaterial>,
    size: usize,
    dirty: bool,
    handle: Option<Handle<Material>>,
}

pub struct VoxelMaterialSystem;

impl VoxelMaterialStorage {
    pub fn create(&mut self, material: VoxelMaterial) -> VoxelMaterialId {
        let result = self.materials
            .iter()
            .enumerate()
            .find_map(|(i, m)| if m.eq(&material) {
                Some(i as u32)
            } else {
                None
            });

        VoxelMaterialId(result.unwrap_or_else(|| {
            self.materials.push(material);
            self.materials.len() as u32 - 1
        }))
    }

    pub(crate) fn coord(&self, material: u32) -> TexCoord {
        let x = (material as usize % self.size) as f32;
        let y = (material as usize / self.size) as f32;
        let w = self.size as f32;
        TexCoord([x / w, y / w])
    }

    pub(crate) fn handle(&self) -> Option<&Handle<Material>> {
        self.handle.as_ref()
    }
}

impl Default for VoxelMaterial {
    fn default() -> Self {
        VoxelMaterial {
            albedo: [255, 255, 255],
            emission: [0, 0, 0],
            alpha: 255,
            metallic: 0,
            roughness: 0,
        }
    }
}

impl Default for VoxelMaterialStorage {
    fn default() -> Self {
        VoxelMaterialStorage {
            materials: Vec::new(),
            size: 1,
            dirty: true,
            handle: None,
        }
    }
}

fn build_texture<'a, I: Iterator<Item=[u8;4]>>(width: usize, iter: I) -> TextureBuilder<'a> {
    let size = width * width;
    TextureBuilder::new()
        .with_kind(Kind::D2(width as u32, width as u32, 1, 1))
        .with_view_kind(ViewKind::D2)
        .with_data_width(width as u32)
        .with_data_height(width as u32)
        .with_data(Cow::<[Pixel<Rgba, _8, Unorm>]>::from(iter
            .take(size)
            .map(|p| Pixel{ repr: p })
            .collect::<Vec<_>>()))
}

impl<'a> System<'a> for VoxelMaterialSystem {
    type SystemData = (
        Write<'a, VoxelMaterialStorage>, 
        ReadExpect<'a, MaterialDefaults>, 
        ReadExpect<'a, Loader>,
        Read<'a, AssetStorage<Texture>>,
        Read<'a, AssetStorage<Material>>,
    );

    fn run(&mut self, (mut storage, defaults, loader, textures, materials): Self::SystemData) {
        if storage.dirty {
            println!("{:?}", storage.materials);

            storage.size = {
                let mut size = 32;
                while storage.materials.len() > size*size {
                    size *= 2;
                }
                size
            };

            let albedo = loader.load_from_data(
                build_texture(storage.size, storage.materials
                    .iter()
                    .map(|m| [m.albedo[0], m.albedo[1], m.albedo[2], m.alpha])
                    .chain(repeat([0,0,0,255]))
                ).into(),
                (),
                &textures,
            );
            
            let emission = loader.load_from_data(
                build_texture(storage.size, storage.materials
                    .iter()
                    .map(|m| [m.emission[0], m.emission[1], m.emission[2], 255])
                    .chain(repeat([0,0,0,255]))
                ).into(),
                (),
                &textures,
            );

            let metallic_roughness = loader.load_from_data(
                build_texture(storage.size, storage.materials
                    .iter()
                    .map(|m| [0, m.metallic, m.roughness, 255])
                    .chain(repeat([0,0,0,255]))
                ).into(),
                (),
                &textures,
            );

            let mat = Material {
                albedo,
                emission,
                metallic_roughness,
                ..defaults.0.clone()
            };

            storage.handle = Some(loader.load_from_data(mat, (), &materials));
            storage.dirty = false;
        }
    }

    fn setup(&mut self, res: &mut Resources) {
        Self::SystemData::setup(res);
        //
    }
}

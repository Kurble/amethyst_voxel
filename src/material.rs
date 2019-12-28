use amethyst::{
    assets::{AssetStorage, Handle, Loader},
    ecs::prelude::*,
    renderer::{
        mtl::{Material, MaterialDefaults},
        palette::*,
        rendy::{
            hal::image::{Kind, ViewKind},
            texture::{pixel::*, TextureBuilder},
        },
        types::Texture,
    },
};
use std::borrow::Cow;
use std::iter::repeat;
use std::sync::Arc;

/// A material. For a better explanation of the properties,
/// take a look at the amethyst PBR model.
pub trait VoxelMaterial {
    fn dimension(&self) -> usize;

    fn albedo_alpha(&self, x: usize, y: usize) -> [u8; 4];

    fn emission(&self, x: usize, y: usize) -> [u8; 3];

    fn metallic_roughness(&self, x: usize, y: usize) -> [u8; 2];
}

#[derive(Clone)]
pub struct Color {
    /// The diffuse albedo of the material
    pub albedo: [u8; 3],
    /// Emissive color of the material
    pub emission: [u8; 3],
    /// Alpha blending factor of the material (unused for now)
    pub alpha: u8,
    /// The metallic factor of the material
    pub metallic: u8,
    /// The roughness factor of the material
    pub roughness: u8,
}

/// A material id issued by the `VoxelMaterialStorage`.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct VoxelMaterialId(pub(crate) u32);

/// A storage resource for `VoxelMaterial`s.
pub struct VoxelMaterialStorage {
    materials: Vec<Arc<dyn VoxelMaterial + Send + Sync>>,
    size: usize,
    grid: usize,
    dirty: bool,
    handle: Option<Handle<Material>>,
}

/// System that manages the `VoxelMaterialStorage`.
pub struct VoxelMaterialSystem;

impl VoxelMaterialStorage {
    /// Create a new material.
    /// If an identical material already exists, it's ID will be returned instead.
    pub fn create(&mut self, material: Arc<dyn VoxelMaterial + Send + Sync>) -> VoxelMaterialId {
        self.dirty = true;
        self.grid = self.grid.max(material.dimension());
        self.materials.push(material);
        VoxelMaterialId(self.materials.len() as u32 - 1)
    }

    pub(crate) fn coord(&self, material: u32, _side: u8, coord: u8) -> [f32; 2] {
        let slots = self.size / self.grid;
        let width = self.size as f32;

        const COORD_MAP_X: [f32; 4] = [0.0, 1.0, 1.0, 0.0];
        const COORD_MAP_Y: [f32; 4] = [1.0, 1.0, 0.0, 0.0];

        let x = ((material as usize % slots) * self.grid) as f32;
        let y = ((material as usize / slots) * self.grid) as f32;
        let x = x + self.grid as f32 * COORD_MAP_X[coord as usize & 0x3];
        let y = y + self.grid as f32 * COORD_MAP_Y[coord as usize & 0x3];

        [x / width, y / width]
    }

    pub(crate) fn handle(&self) -> Option<&Handle<Material>> {
        self.handle.as_ref()
    }
}

impl Default for Color {
    fn default() -> Self {
        Color {
            albedo: [255, 255, 255],
            emission: [0, 0, 0],
            alpha: 255,
            metallic: 8,
            roughness: 250,
        }
    }
}

impl Default for VoxelMaterialStorage {
    fn default() -> Self {
        VoxelMaterialStorage {
            materials: Vec::new(),
            size: 1,
            grid: 4,
            dirty: true,
            handle: None,
        }
    }
}

impl VoxelMaterial for Color {
    fn dimension(&self) -> usize {
        1
    }

    fn albedo_alpha(&self, _: usize, _: usize) -> [u8; 4] {
        [self.albedo[0], self.albedo[1], self.albedo[2], self.alpha]
    }

    fn emission(&self, _: usize, _: usize) -> [u8; 3] {
        [self.emission[0], self.emission[1], self.emission[2]]
    }

    fn metallic_roughness(&self, _: usize, _: usize) -> [u8; 2] {
        [self.metallic, self.roughness]
    }
}

fn build_texture<'a, F: Fn(usize, usize) -> [u8; 4]>(width: usize, pixel: F) -> TextureBuilder<'a> {
    TextureBuilder::new()
        .with_kind(Kind::D2(width as u32, width as u32, 1, 1))
        .with_view_kind(ViewKind::D2)
        .with_data_width(width as u32)
        .with_data_height(width as u32)
        .with_data(Cow::<[Rgba8Unorm]>::from(
            repeat(())
                .take(width)
                .enumerate()
                .flat_map(|(y, _)| {
                    repeat(y).take(width).enumerate().map(|(x, y)| {
                        let px = pixel(x, y);
                        Rgba8Unorm::from(Srgba::new(px[0], px[1], px[2], px[3]))
                    })
                })
                .collect::<Vec<_>>(),
        ))
}

#[allow(clippy::type_complexity)]
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
            storage.size = {
                let mut size = 32;
                while storage.materials.len() * storage.grid * storage.grid > size * size {
                    size *= 2;
                }
                size
            };

            let slots = storage.size / storage.grid;

            let find_material = |x, y| {
                let texture_x = x as usize - x as usize / storage.grid;
                let texture_y = y as usize - y as usize / storage.grid;
                storage
                    .materials
                    .get((y as usize / storage.grid) * slots + x as usize / storage.grid)
                    .map(|m| (m, texture_x, texture_y))
            };

            let albedo = loader.load_from_data(
                build_texture(storage.size, |x, y| {
                    find_material(x, y)
                        .map(|(m, x, y)| m.albedo_alpha(x, y))
                        .unwrap_or([255, 0, 255, 255])
                })
                .into(),
                (),
                &textures,
            );

            let wrap = |x: [u8; 3]| [x[0], x[1], x[2], 255];
            let emission = loader.load_from_data(
                build_texture(storage.size, |x, y| {
                    find_material(x, y)
                        .map(|(m, x, y)| wrap(m.emission(x, y)))
                        .unwrap_or([0, 0, 0, 255])
                })
                .into(),
                (),
                &textures,
            );

            let wrap = |x: [u8; 2]| [0, x[0], x[1], 255];
            let metallic_roughness = loader.load_from_data(
                build_texture(storage.size, |x, y| {
                    find_material(x, y)
                        .map(|(m, x, y)| wrap(m.metallic_roughness(x, y)))
                        .unwrap_or([0, 240, 8, 255])
                })
                .into(),
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

    fn setup(&mut self, world: &mut World) {
        Self::SystemData::setup(world);
        //
    }
}

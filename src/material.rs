use amethyst::{
    assets::{Asset, AssetStorage, Handle, HotReloadStrategy, Loader, ProcessingState},
    core::{ArcThreadPool, Time},
    ecs::prelude::*,
    renderer::{
        mtl::{Material, MaterialDefaults},
        palette::*,
        rendy::{
            hal::image::*,
            texture::{pixel::*, MipLevels, TextureBuilder},
        },
        types::Texture,
    },
};
use core::num::NonZeroU8;
use serde_derive::*;
use std::borrow::Cow;
use std::collections::HashMap;
use std::iter::repeat;
use std::ops::Deref;
use std::sync::Arc;

/// A material. For a better explanation of the properties,
/// take a look at the amethyst PBR model.
pub trait VoxelMaterial: 'static + Send + Sync {
    /// The width and height of this material.
    fn dimension(&self) -> usize;
    /// Get a pixel value for the albedo/alpha channel. The format is [r, g, b, a].
    fn albedo_alpha(&self, x: usize, y: usize) -> [u8; 4];
    /// Get a pixel value for the emission channel. The format is [r, g, b].
    fn emission(&self, x: usize, y: usize) -> [u8; 3];
    /// Get a pixel value for the metallic/roughness channel. The format is [m, r].
    fn metallic_roughness(&self, x: usize, y: usize) -> [u8; 2];
    /// The submaterials of this material. Should be at least self.
    fn submaterials(&self) -> Vec<Box<dyn VoxelMaterial>>;
    /// What submaterial to render for the given properties.
    fn sub_side(&self, side: u8) -> usize;
    /// The amount of animation frmaes for this material
    fn sub_frames(&self) -> usize;
    /// The kind of tiling to bake into the atlas for this material.
    fn tiling(&self) -> Tiling;
}

pub trait AtlasAccess {
    fn coord(&self, material: u32, side: u8, coord: u8) -> [f32; 2];

    /// Retrieve material handle for the given id.
    fn get(&self, id: &str) -> Option<AtlasMaterialHandle>;
}

/// A material handle issued by an `Atlas`.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AtlasMaterialHandle(pub(crate) u32);

/// A storage resource for `VoxelMaterial`s.
pub struct Atlas {
    materials: Vec<Box<dyn VoxelMaterial>>,
    lookup: HashMap<String, AtlasMaterialHandle>,
    size: usize,
    grid: usize,
    pub(crate) handle: Handle<Material>,
}

/// Data for creating a material atlas.
#[derive(Default)]
pub struct AtlasData {
    materials: Vec<Box<dyn VoxelMaterial>>,
    lookup: HashMap<String, AtlasMaterialHandle>,
    size: usize,
    grid: usize,
}

/// System that loads the `Atlas` resources from `AtlasData`.
pub struct AtlasProcessor;

/// `SystemData` for the `AtlasProcessor` system.
#[derive(SystemData)]
pub struct AtlasProcessorData<'a> {
    atlas_storage: Write<'a, AssetStorage<Atlas>>,
    texture_storage: Read<'a, AssetStorage<Texture>>,
    material_storage: Read<'a, AssetStorage<Material>>,
    material_defaults: ReadExpect<'a, MaterialDefaults>,
    loader: ReadExpect<'a, Loader>,
    time: Read<'a, Time>,
    pool: ReadExpect<'a, ArcThreadPool>,
    strategy: Option<Read<'a, HotReloadStrategy>>,
}

/// The tiling of the the textured material. This is only relevant when filtering is enabled.
#[derive(Deserialize, Clone, Copy)]
pub enum Tiling {
    None,
    Horizontal,
    Vertical,
    Both,
}

#[derive(Clone)]
pub struct ColoredMaterial {
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

#[derive(Clone)]
pub struct TexturedMaterial {
    /// The size of both the width and the height of this texture. Must be a power of 2.
    pub size: usize,
    /// The tiling of the the textured material. This is only relevant when filtering is enabled.
    pub tiling: Tiling,
    /// The albedo/alpha texture. One entry [r, g, b, a] per pixel.
    /// If you don't care abou this texture you can leave it empty, [0, 0, 0, 255] will be used i f the vector is empty.
    pub albedo_alpha: Arc<[[u8; 4]]>,
    /// The emission texture. One entry [r, g, b] per pixel.
    /// If you don't care about this texture you can leave it empty, [0, 0, 0] will be used if the vector is empty.
    pub emission: Arc<[[u8; 3]]>,
    /// The metallic/roughness texture. One entry [m, r] per pixel.
    /// If you don't care abou this texture you can leave it empty, [240, 8] will be used i f the vector is empty.
    pub metallic_roughness: Arc<[[u8; 2]]>,
}

impl AtlasAccess for Atlas {
    fn coord(&self, material: u32, side: u8, coord: u8) -> [f32; 2] {
        let slots = self.size / self.grid;
        const COORD_MAP_X: [f32; 4] = [0.0, 1.0, 1.0, 0.0];
        const COORD_MAP_Y: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

        let (material_id, material_size) = self
            .materials
            .get(material as usize)
            .map(|m| (material as usize + m.sub_side(side), m.dimension()))
            .unwrap_or((material as usize, 1));

        let border = ((self.grid - material_size) / 2) as f32;

        let x = ((material_id as usize % slots) * self.grid) as f32
            + border
            + COORD_MAP_X[coord as usize & 0x3] * material_size as f32;

        let y = ((material_id as usize / slots) * self.grid) as f32
            + border
            + COORD_MAP_Y[coord as usize & 0x3] * material_size as f32;

        let w = 1.0 / self.size as f32;
        [x * w, y * w]
    }

    fn get(&self, id: &str) -> Option<AtlasMaterialHandle> {
        self.lookup.get(id).cloned()
    }
}

impl Asset for Atlas {
    const NAME: &'static str = "Atlas";
    type Data = AtlasData;
    type HandleStorage = DenseVecStorage<Handle<Self>>;
}

impl AtlasData {
    /// Create a new material with a id if a material with the given id doesn't exist already.
    /// Returns the material handle for the given id.
    pub fn create<T: AsRef<dyn VoxelMaterial>, S: Into<String>>(
        &mut self,
        id: S,
        material: T,
    ) -> AtlasMaterialHandle {
        self.lookup
            .entry(id.into())
            .or_insert_with({
                let materials = &mut self.materials;
                let grid = &mut self.grid;
                let size = &mut self.size;
                move || {
                    let material = material.as_ref();
                    let id = materials.len();
                    materials.extend(material.submaterials().into_iter());
                    *grid = (*grid).max(material.dimension() * 2);
                    *size = {
                        let mut size = 32;
                        while materials.len() * (*grid) * (*grid) > size * size {
                            size *= 2;
                        }
                        size
                    };
                    AtlasMaterialHandle(id as u32)
                }
            })
            .clone()
    }

    /// Create a material without assigning an id to it. This means the material can't be looked up at a later point.
    /// The returned handle is the only way to refer to the created material.
    pub fn create_without_id<T: AsRef<dyn VoxelMaterial>>(
        &mut self,
        material: T,
    ) -> AtlasMaterialHandle {
        let material = material.as_ref();
        let id = self.materials.len();
        self.materials.extend(material.submaterials().into_iter());
        self.grid = self.grid.max(material.dimension() * 2);
        self.size = {
            let mut size = 32;
            while self.materials.len() * self.grid * self.grid > self.size * self.size {
                size *= 2;
            }
            size
        };
        AtlasMaterialHandle(id as u32)
    }
}

impl AtlasAccess for AtlasData {
    fn coord(&self, material: u32, side: u8, coord: u8) -> [f32; 2] {
        let slots = self.size / self.grid;
        const COORD_MAP_X: [f32; 4] = [0.0, 1.0, 1.0, 0.0];
        const COORD_MAP_Y: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

        let (material_id, material_size) = self
            .materials
            .get(material as usize)
            .map(|m| (material as usize + m.sub_side(side), m.dimension()))
            .unwrap_or((material as usize, 1));

        let border = ((self.grid - material_size) / 2) as f32;

        let x = ((material_id as usize % slots) * self.grid) as f32
            + border
            + COORD_MAP_X[coord as usize & 0x3] * material_size as f32;

        let y = ((material_id as usize / slots) * self.grid) as f32
            + border
            + COORD_MAP_Y[coord as usize & 0x3] * material_size as f32;

        let w = 1.0 / self.size as f32;
        [x * w, y * w]
    }

    fn get(&self, id: &str) -> Option<AtlasMaterialHandle> {
        self.lookup.get(id).cloned()
    }
}

impl Default for ColoredMaterial {
    fn default() -> Self {
        ColoredMaterial {
            albedo: [255, 255, 255],
            emission: [0, 0, 0],
            alpha: 255,
            metallic: 8,
            roughness: 250,
        }
    }
}

impl VoxelMaterial for ColoredMaterial {
    fn dimension(&self) -> usize {
        1
    }

    fn sub_side(&self, _: u8) -> usize {
        0
    }

    fn sub_frames(&self) -> usize {
        0
    }

    fn tiling(&self) -> Tiling {
        Tiling::Both
    }

    fn submaterials(&self) -> Vec<Box<dyn VoxelMaterial>> {
        vec![Box::new(self.clone())]
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

impl VoxelMaterial for TexturedMaterial {
    fn dimension(&self) -> usize {
        self.size
    }

    fn sub_side(&self, _: u8) -> usize {
        0
    }

    fn sub_frames(&self) -> usize {
        0
    }

    fn submaterials(&self) -> Vec<Box<dyn VoxelMaterial>> {
        vec![Box::new(self.clone())]
    }

    fn tiling(&self) -> Tiling {
        self.tiling
    }

    fn albedo_alpha(&self, x: usize, y: usize) -> [u8; 4] {
        self.albedo_alpha
            .get(y * self.size + x)
            .unwrap_or(&[255, 0, 255, 255])
            .clone()
    }

    fn emission(&self, x: usize, y: usize) -> [u8; 3] {
        self.emission
            .get(y * self.size + x)
            .unwrap_or(&[0, 0, 0])
            .clone()
    }

    fn metallic_roughness(&self, x: usize, y: usize) -> [u8; 2] {
        self.metallic_roughness
            .get(y * self.size + x)
            .unwrap_or(&[240, 8])
            .clone()
    }
}

impl Tiling {
    fn horizontal(&self) -> bool {
        match self {
            Tiling::Horizontal => true,
            Tiling::Both => true,
            _ => false,
        }
    }

    fn vertical(&self) -> bool {
        match self {
            Tiling::Vertical => true,
            Tiling::Both => true,
            _ => false,
        }
    }
}

impl Default for Tiling {
    fn default() -> Self {
        Tiling::Both
    }
}

impl<'a> System<'a> for AtlasProcessor {
    type SystemData = AtlasProcessorData<'a>;

    fn run(&mut self, mut data: Self::SystemData) {
        data.atlas_storage.process(
            {
                let loader = &data.loader;
                let texture_storage = &*data.texture_storage;
                let material_storage = &*data.material_storage;
                let material_defaults = &*data.material_defaults;
                move |atlas| {
                    let grid = {
                        atlas
                            .materials
                            .iter()
                            .map(|e| e.dimension() * 2)
                            .max()
                            .unwrap_or(1)
                    };

                    let size = {
                        let mut size = 32;
                        while atlas.materials.len() * grid * grid > size * size {
                            size *= 2;
                        }
                        size
                    };

                    let handle = build_material(
                        size,
                        grid,
                        &atlas.materials,
                        loader,
                        texture_storage,
                        material_storage,
                        material_defaults,
                    );

                    Ok(ProcessingState::Loaded(Atlas {
                        materials: atlas.materials,
                        lookup: atlas.lookup,
                        size,
                        grid,
                        handle,
                    }))
                }
            },
            data.time.frame_number(),
            &**data.pool,
            data.strategy.as_ref().map(Deref::deref),
        );
    }

    fn setup(&mut self, world: &mut World) {
        Self::SystemData::setup(world);
    }
}

fn build_material(
    size: usize,
    grid: usize,
    materials: &Vec<Box<dyn VoxelMaterial>>,
    loader: &Loader,
    texture_storage: &AssetStorage<Texture>,
    material_storage: &AssetStorage<Material>,
    defaults: &MaterialDefaults,
) -> Handle<Material> {
    let slots = size / grid;

    let find_material = |x, y| {
        let texture_x = x as usize - (x as usize / grid) * grid;
        let texture_y = y as usize - (y as usize / grid) * grid;
        materials
            .get((y as usize / grid) * slots + x as usize / grid)
            .map(|m| {
                let border = (grid - m.dimension()) / 2;
                let border = |x, tile| match (x < border, tile) {
                    (true, true) => ((x + m.dimension()) - border) % m.dimension(),
                    (true, false) => 0,
                    (false, true) => (x - border) % m.dimension(),
                    (false, false) => (m.dimension() - 1).min(x - border),
                };
                let t = m.tiling();
                (
                    m,
                    border(texture_x, t.horizontal()),
                    border(texture_y, t.vertical()),
                )
            })
    };

    let mips = {
        let mut i = 1;
        let mut room = grid / 2;
        while room > 2 {
            i += 1;
            room /= 2;
        }
        NonZeroU8::new(i as u8).unwrap()
    };

    let albedo = loader.load_from_data(
        build_texture(size, mips, |x, y| {
            find_material(x, y)
                .map(|(m, x, y)| m.albedo_alpha(x, y))
                .unwrap_or([255, 0, 255, 255])
        })
        .into(),
        (),
        texture_storage,
    );

    let wrap = |x: [u8; 3]| [x[0], x[1], x[2], 255];
    let emission = loader.load_from_data(
        build_texture(size, mips, |x, y| {
            find_material(x, y)
                .map(|(m, x, y)| wrap(m.emission(x, y)))
                .unwrap_or([0, 0, 0, 255])
        })
        .into(),
        (),
        texture_storage,
    );

    let wrap = |x: [u8; 2]| [0, x[0], x[1], 255];
    let metallic_roughness = loader.load_from_data(
        build_texture(size, mips, |x, y| {
            find_material(x, y)
                .map(|(m, x, y)| wrap(m.metallic_roughness(x, y)))
                .unwrap_or([0, 240, 8, 255])
        })
        .into(),
        (),
        texture_storage,
    );

    let mat = Material {
        albedo,
        emission,
        metallic_roughness,

        ..defaults.0.clone()
    };

    loader.load_from_data(mat, (), material_storage)
}

fn build_texture<'a, F: Fn(usize, usize) -> [u8; 4]>(
    width: usize,
    mips: NonZeroU8,
    pixel: F,
) -> TextureBuilder<'a> {
    let mut sampler_info = SamplerInfo::new(Filter::Linear, WrapMode::Clamp);
    sampler_info.min_filter = Filter::Linear;
    sampler_info.mag_filter = Filter::Nearest;
    sampler_info.mip_filter = Filter::Linear;
    sampler_info.anisotropic = Anisotropic::On(2);
    TextureBuilder::new()
        .with_kind(Kind::D2(width as u32, width as u32, 1, 1))
        .with_view_kind(ViewKind::D2)
        .with_data_width(width as u32)
        .with_data_height(width as u32)
        .with_mip_levels(MipLevels::GenerateLevels(mips))
        .with_sampler_info(sampler_info)
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

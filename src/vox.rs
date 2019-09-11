use std::io::*;
use std::sync::Arc;
use byteorder::*;
use amethyst::assets::{Format};
use crate::{
    model::VoxelModelData,
    material::{VoxelMaterial},
};

type E = LittleEndian;

#[derive(Clone, Copy, Debug, Default)]
pub struct VoxFormat;

impl Format<VoxelModelData> for VoxFormat {
    fn name(&self) -> &'static str { "MagicaVoxel" }

    fn import_simple(&self, bytes: Vec<u8>) -> amethyst::Result<VoxelModelData> {
        let val = load_vox(bytes.as_slice())
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        Ok(val)
    }
}

fn load_vox<R>(mut reader: R) -> Result<Vec<VoxelModelData>> where
    R: ReadBytesExt,
{
    // Read the vox file header and check if the version is supported.
    reader.read_exact(&mut [0;4])?;
    let version = reader.read_u32::<E>()?;
    check(version >= 150)?;

    // Read the main chunk and check if it is indeed the main chunk.
    let (main, _) = Chunk::load(&mut reader)?;
    check(main.is("MAIN"))?;

    // Some vectors to store processed chunks in
    let mut sizes = Vec::new();
    let mut voxels = Vec::new();
    let mut materials = DEFAULT_MATERIALS.iter().cloned().map(|m| {
        let r = ((m      ) & 0xff) as u8;
        let g = ((m >>  8) & 0xff) as u8;
        let b = ((m >> 16) & 0xff) as u8;
        let a = ((m >> 24) & 0xff) as u8;
        rgba_to_material(r, g, b, a)
    }).collect::<Vec<_>>();

    // Process all child chunks from the main chunk
    for mut chunk in main.children {
        // the size for a model
        if chunk.is("SIZE") {
            let w = chunk.content.read_u32::<E>()? as usize;
            let h = chunk.content.read_u32::<E>()? as usize;
            let d = chunk.content.read_u32::<E>()? as usize;
            sizes.push((w, h, d));
        }

        // the content for a model
        if chunk.is("XYZI") {
            let num = chunk.content.read_u32::<E>()? as usize;
            let mut vox = Vec::new();
            for _ in 0..num {
                let x = chunk.content.read_u8()?;
                let y = chunk.content.read_u8()?;
                let z = chunk.content.read_u8()?;
                let i = chunk.content.read_u8()?;
                vox.push((x, y, z, i));
            }
            voxels.push(vox);
        }

        // the used palette. Colors are diffuse. Overwrites the current palette.
        if chunk.is("RGBA") {
            materials.clear();
            materials.push(VoxelMaterial::default());
            for _ in 0..255 {
                let r = chunk.content.read_u8()?;
                let g = chunk.content.read_u8()?;
                let b = chunk.content.read_u8()?;
                let a = chunk.content.read_u8()?;
                materials.push(rgba_to_material(r, g, b, a));
            }
        }

        // PBR properties for a single material in the palette
        if chunk.is("MATT") {
            let id = chunk.content.read_u32::<E>()? as usize;
            let ty = chunk.content.read_u32::<E>()?;
            let weight = chunk.content.read_f32::<E>()?;
            let props = chunk.content.read_u32::<E>()?;
            let old = materials[id];
            let _plastic =     if bit(props, 0) { chunk.content.read_f32::<E>()? } else { 0.0 };
            let roughness =    if bit(props, 1) { chunk.content.read_f32::<E>()? } else { 0.0 };
            let _specular =    if bit(props, 2) { chunk.content.read_f32::<E>()? } else { 0.0 };
            let _ior =         if bit(props, 3) { chunk.content.read_f32::<E>()? } else { 0.0 };
            let _attenuation = if bit(props, 4) { chunk.content.read_f32::<E>()? } else { 0.0 };
            let _power =       if bit(props, 5) { chunk.content.read_f32::<E>()? } else { 0.0 };
            let _glow =        if bit(props, 6) { chunk.content.read_f32::<E>()? } else { 0.0 };
            materials[id] = match ty {
                0 /*diffuse*/ => VoxelMaterial {
                    albedo: old.albedo,
                    emission: old.emission,
                    alpha: old.alpha,
                    metallic: mul_value(255, weight),
                    roughness: mul_value(255, roughness),
                },
                1 /*metal*/ => VoxelMaterial {
                    albedo: old.albedo,
                    emission: old.emission,
                    alpha: old.alpha,
                    metallic: mul_value(255, weight),
                    roughness: mul_value(255, roughness),
                },
                2 /*glass*/ => VoxelMaterial {
                    albedo: old.albedo,
                    emission: old.emission,
                    alpha: old.alpha,
                    metallic: mul_value(255, weight),
                    roughness: mul_value(255, roughness),
                },
                3 /*emissive*/ => VoxelMaterial {
                    albedo: old.albedo,
                    emission: old.albedo,
                    alpha: old.alpha,
                    metallic: mul_value(255, weight),
                    roughness: mul_value(255, roughness),
                },
                _ => old,
            }
        }
    }

    let materials = Arc::<[VoxelMaterial]>::from(materials);

    // Convert the stored chunk data to our own voxel format.
    Ok(sizes
        .into_iter()
        .zip(voxels)
        .map(|(size, voxels)| {
            VoxelModelData {
                materials: materials.clone(),
                voxels: voxels.into_iter().map(|(x, y, z, i)| {
                    let index = x as usize + 
                        y as usize * size.0 + 
                        z as usize * size.0 * size.1;
                    (index, i as usize)
                }).collect(),
                dimensions: [size.0, size.1, size.2],
            }
        })
        .collect())
}

// assert without panicking, instead returns an error.
fn check(b: bool) -> Result<()> {
    if b { Ok(()) } else { Err(ErrorKind::InvalidData.into()) }
}

// multiply a unorm value by a scalar
fn mul_value(value: u8, scalar: f32) -> u8 {
    ((f32::from(value) / 255.0) * scalar * 255.0) as u8
}

// check if bit is present in u32
fn bit(field: u32, bit: u32) -> bool {
    (field & (0x01 << bit)) > 0
}

// convert a simple r,g,b,a material to a VoxelMaterial
fn rgba_to_material(r: u8, g: u8, b: u8, a: u8) -> VoxelMaterial {
    VoxelMaterial {
        albedo: [r, g, b],
        emission: [0, 0, 0],
        alpha: a,
        metallic: 8,
        roughness: 240,
    }
}

struct Chunk {
    id: [u8; 4],
    content: Cursor<Vec<u8>>,
    children: Vec<Chunk>,
}

impl Chunk {
    fn load<R>(reader: &mut R) -> Result<(Self, usize)> where
        R: ReadBytesExt,
    {
        // load id
        let mut id = [0u8; 4];
        reader.read_exact(&mut id)?;

        // load content
        let content_size = reader.read_u32::<E>()? as usize;
        let children_size = reader.read_u32::<E>()? as usize;
        let mut content = Vec::new();
        content.resize(content_size, 0);
        reader.read_exact(content.as_mut_slice())?;

        // load children
        let mut loaded = 0;
        let mut children = Vec::new();
        while loaded < children_size {
            let (chunk, size) = Chunk::load(reader)?;
            children.push(chunk);
            loaded += size;
        }
        check(loaded == children_size)?;

        // build chunk struct
        let chunk = Chunk {
            id,
            content: Cursor::new(content),
            children,
        };
        let size = 12 + content_size + children_size;
        Ok((chunk, size))
    }

    fn is(&self, id: &str) -> bool {
        id.as_bytes().eq(&self.id)
    }
}

/// VOX format default materials
const DEFAULT_MATERIALS: [u32; 256] = [
    0x0000_0000, 0xffff_ffff, 0xffcc_ffff, 0xff99_ffff, 0xff66_ffff, 0xff33_ffff, 0xff00_ffff, 0xffff_ccff,
    0xffcc_ccff, 0xff99_ccff, 0xff66_ccff, 0xff33_ccff, 0xff00_ccff, 0xffff_99ff, 0xffcc_99ff, 0xff99_99ff,
    0xff66_99ff, 0xff33_99ff, 0xff00_99ff, 0xffff_66ff, 0xffcc_66ff, 0xff99_66ff, 0xff66_66ff, 0xff33_66ff,
    0xff00_66ff, 0xffff_33ff, 0xffcc_33ff, 0xff99_33ff, 0xff66_33ff, 0xff33_33ff, 0xff00_33ff, 0xffff_00ff,
    0xffcc_00ff, 0xff99_00ff, 0xff66_00ff, 0xff33_00ff, 0xff00_00ff, 0xffff_ffcc, 0xffcc_ffcc, 0xff99_ffcc,
    0xff66_ffcc, 0xff33_ffcc, 0xff00_ffcc, 0xffff_cccc, 0xffcc_cccc, 0xff99_cccc, 0xff66_cccc, 0xff33_cccc,
    0xff00_cccc, 0xffff_99cc, 0xffcc_99cc, 0xff99_99cc, 0xff66_99cc, 0xff33_99cc, 0xff00_99cc, 0xffff_66cc,
    0xffcc_66cc, 0xff99_66cc, 0xff66_66cc, 0xff33_66cc, 0xff00_66cc, 0xffff_33cc, 0xffcc_33cc, 0xff99_33cc,
    0xff66_33cc, 0xff33_33cc, 0xff00_33cc, 0xffff_00cc, 0xffcc_00cc, 0xff99_00cc, 0xff66_00cc, 0xff33_00cc,
    0xff00_00cc, 0xffff_ff99, 0xffcc_ff99, 0xff99_ff99, 0xff66_ff99, 0xff33_ff99, 0xff00_ff99, 0xffff_cc99,
    0xffcc_cc99, 0xff99_cc99, 0xff66_cc99, 0xff33_cc99, 0xff00_cc99, 0xffff_9999, 0xffcc_9999, 0xff99_9999,
    0xff66_9999, 0xff33_9999, 0xff00_9999, 0xffff_6699, 0xffcc_6699, 0xff99_6699, 0xff66_6699, 0xff33_6699,
    0xff00_6699, 0xffff_3399, 0xffcc_3399, 0xff99_3399, 0xff66_3399, 0xff33_3399, 0xff00_3399, 0xffff_0099,
    0xffcc_0099, 0xff99_0099, 0xff66_0099, 0xff33_0099, 0xff00_0099, 0xffff_ff66, 0xffcc_ff66, 0xff99_ff66,
    0xff66_ff66, 0xff33_ff66, 0xff00_ff66, 0xffff_cc66, 0xffcc_cc66, 0xff99_cc66, 0xff66_cc66, 0xff33_cc66,
    0xff00_cc66, 0xffff_9966, 0xffcc_9966, 0xff99_9966, 0xff66_9966, 0xff33_9966, 0xff00_9966, 0xffff_6666,
    0xffcc_6666, 0xff99_6666, 0xff66_6666, 0xff33_6666, 0xff00_6666, 0xffff_3366, 0xffcc_3366, 0xff99_3366,
    0xff66_3366, 0xff33_3366, 0xff00_3366, 0xffff_0066, 0xffcc_0066, 0xff99_0066, 0xff66_0066, 0xff33_0066,
    0xff00_0066, 0xffff_ff33, 0xffcc_ff33, 0xff99_ff33, 0xff66_ff33, 0xff33_ff33, 0xff00_ff33, 0xffff_cc33,
    0xffcc_cc33, 0xff99_cc33, 0xff66_cc33, 0xff33_cc33, 0xff00_cc33, 0xffff_9933, 0xffcc_9933, 0xff99_9933,
    0xff66_9933, 0xff33_9933, 0xff00_9933, 0xffff_6633, 0xffcc_6633, 0xff99_6633, 0xff66_6633, 0xff33_6633,
    0xff00_6633, 0xffff_3333, 0xffcc_3333, 0xff99_3333, 0xff66_3333, 0xff33_3333, 0xff00_3333, 0xffff_0033,
    0xffcc_0033, 0xff99_0033, 0xff66_0033, 0xff33_0033, 0xff00_0033, 0xffff_ff00, 0xffcc_ff00, 0xff99_ff00,
    0xff66_ff00, 0xff33_ff00, 0xff00_ff00, 0xffff_cc00, 0xffcc_cc00, 0xff99_cc00, 0xff66_cc00, 0xff33_cc00,
    0xff00_cc00, 0xffff_9900, 0xffcc_9900, 0xff99_9900, 0xff66_9900, 0xff33_9900, 0xff00_9900, 0xffff_6600,
    0xffcc_6600, 0xff99_6600, 0xff66_6600, 0xff33_6600, 0xff00_6600, 0xffff_3300, 0xffcc_3300, 0xff99_3300,
    0xff66_3300, 0xff33_3300, 0xff00_3300, 0xffff_0000, 0xffcc_0000, 0xff99_0000, 0xff66_0000, 0xff33_0000,
    0xff00_00ee, 0xff00_00dd, 0xff00_00bb, 0xff00_00aa, 0xff00_0088, 0xff00_0077, 0xff00_0055, 0xff00_0044,
    0xff00_0022, 0xff00_0011, 0xff00_ee00, 0xff00_dd00, 0xff00_bb00, 0xff00_aa00, 0xff00_8800, 0xff00_7700,
    0xff00_5500, 0xff00_4400, 0xff00_2200, 0xff00_1100, 0xffee_0000, 0xffdd_0000, 0xffbb_0000, 0xffaa_0000,
    0xff88_0000, 0xff77_0000, 0xff55_0000, 0xff44_0000, 0xff22_0000, 0xff11_0000, 0xffee_eeee, 0xffdd_dddd,
    0xffbb_bbbb, 0xffaa_aaaa, 0xff88_8888, 0xff77_7777, 0xff55_5555, 0xff44_4444, 0xff22_2222, 0xff11_1111
];
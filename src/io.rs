use std::io::*;
use std::sync::Arc;
use byteorder::*;
use crate::{
    voxel::*,
    material::{VoxelMaterial, VoxelMaterialId, VoxelMaterialStorage},
    triangulate::Const,
};

type E = LittleEndian;

// assert without panicking, instead returns an error.
fn check(b: bool) -> Result<()> {
    if b { Ok(()) } else { Err(ErrorKind::InvalidData.into()) }
}

// multiply a unorm value by a scalar
fn mul_value(value: u8, scalar: f32) -> u8 {
    ((value as f32 / 255.0) * scalar * 255.0) as u8
}

// check if bit is present in u32
fn bit(field: u32, bit: u32) -> bool {
    (field & (0x01 << bit)) > 0
}

//pub fn load_vox_from_file<T: VoxelData + Default, P: AsRef<Path>>(path: P, store: &mut VoxelMaterialStorage) -> Result<Vec<Nested<T, (), Simple>>> {
    // todo
//}

/// load a MagicaVoxel .vox file. Since the amount of subvoxels is fixed by the voxel format,
///  the MagicaVoxel model is centered and cropped to fit in the fixed voxel format.
/// Any materials that don't exist in the world yet will be added.
pub fn load_vox<T, R>(mut reader: R, store: &mut VoxelMaterialStorage) -> Result<Vec<Nested<T, (), Simple>>> where
    T: VoxelData + Default,
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
        let r = ((m >>  0) & 0xff) as u8;
        let g = ((m >>  8) & 0xff) as u8;
        let b = ((m >> 16) & 0xff) as u8;
        let a = ((m >> 24) & 0xff) as u8;
        rgba_to_material(r, g, b, a)
    }).collect::<Vec<_>>();

    // Process all child chunks from the main chunk
    for mut chunk in main.children {
        // the size for a model
        if chunk.is("SIZE") {
            let w = chunk.content.read_u32::<E>()? as isize;
            let h = chunk.content.read_u32::<E>()? as isize;
            let d = chunk.content.read_u32::<E>()? as isize;
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
                    metallic: mul_value(255, 1.0 - weight),
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

    // Convert materials to material id's by adding them to the world.
    let materials: Vec<VoxelMaterialId> = materials
        .into_iter()
        .map(|material| store.create(material))
        .collect();

    // Convert the stored chunk data to our own voxel format.
    Ok(sizes
        .into_iter()
        .zip(voxels)
        .map(|(size, voxels)| {
            // calculate offsets to center the model
            let x_offset = (Const::<T>::WIDTH as isize - size.0) / 2;
            let y_offset = (Const::<T>::WIDTH as isize - size.1) / 2;
            let z_offset = (Const::<T>::WIDTH as isize - size.2) / 2;
            // Next we will expand the compressed format to a vector that has one entry per
            //  element, even if it's empty.
            let mut expand = Vec::new();
            expand.resize(Const::<T>::COUNT, Simple::Empty);
            // Perform the expansion.
            for (x, y, z, i) in voxels {
                let m = materials[i as usize];
                let x = (x as isize + x_offset) as usize;
                let y = (y as isize + y_offset) as usize;
                let z = (z as isize + z_offset) as usize;
                let last = Const::<T>::LAST;
                // Material 0 is empty voxel
                if i > 0 && x <= last && y <= last && z <= last {
                    let i =
                        x*Const::<T>::DX+
                        y*Const::<T>::DY+
                        z*Const::<T>::DZ;
                    expand[i] = Simple::Material(m);
                }
            }

            Nested::Detail {
                data: T::default(),
                detail: Arc::new(expand),
            }
        })
        .collect())
}

fn rgba_to_material(r: u8, g: u8, b: u8, a: u8) -> VoxelMaterial {
    VoxelMaterial {
        albedo: [r, g, b],
        emission: [0, 0, 0],
        alpha: a,
        metallic: 0,
        roughness: 255,
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
    0x00000000, 0xffffffff, 0xffccffff, 0xff99ffff, 0xff66ffff, 0xff33ffff, 0xff00ffff, 0xffffccff,
    0xffccccff, 0xff99ccff, 0xff66ccff, 0xff33ccff, 0xff00ccff, 0xffff99ff, 0xffcc99ff, 0xff9999ff,
    0xff6699ff, 0xff3399ff, 0xff0099ff, 0xffff66ff, 0xffcc66ff, 0xff9966ff, 0xff6666ff, 0xff3366ff,
    0xff0066ff, 0xffff33ff, 0xffcc33ff, 0xff9933ff, 0xff6633ff, 0xff3333ff, 0xff0033ff, 0xffff00ff,
    0xffcc00ff, 0xff9900ff, 0xff6600ff, 0xff3300ff, 0xff0000ff, 0xffffffcc, 0xffccffcc, 0xff99ffcc,
    0xff66ffcc, 0xff33ffcc, 0xff00ffcc, 0xffffcccc, 0xffcccccc, 0xff99cccc, 0xff66cccc, 0xff33cccc,
    0xff00cccc, 0xffff99cc, 0xffcc99cc, 0xff9999cc, 0xff6699cc, 0xff3399cc, 0xff0099cc, 0xffff66cc,
    0xffcc66cc, 0xff9966cc, 0xff6666cc, 0xff3366cc, 0xff0066cc, 0xffff33cc, 0xffcc33cc, 0xff9933cc,
    0xff6633cc, 0xff3333cc, 0xff0033cc, 0xffff00cc, 0xffcc00cc, 0xff9900cc, 0xff6600cc, 0xff3300cc,
    0xff0000cc, 0xffffff99, 0xffccff99, 0xff99ff99, 0xff66ff99, 0xff33ff99, 0xff00ff99, 0xffffcc99,
    0xffcccc99, 0xff99cc99, 0xff66cc99, 0xff33cc99, 0xff00cc99, 0xffff9999, 0xffcc9999, 0xff999999,
    0xff669999, 0xff339999, 0xff009999, 0xffff6699, 0xffcc6699, 0xff996699, 0xff666699, 0xff336699,
    0xff006699, 0xffff3399, 0xffcc3399, 0xff993399, 0xff663399, 0xff333399, 0xff003399, 0xffff0099,
    0xffcc0099, 0xff990099, 0xff660099, 0xff330099, 0xff000099, 0xffffff66, 0xffccff66, 0xff99ff66,
    0xff66ff66, 0xff33ff66, 0xff00ff66, 0xffffcc66, 0xffcccc66, 0xff99cc66, 0xff66cc66, 0xff33cc66,
    0xff00cc66, 0xffff9966, 0xffcc9966, 0xff999966, 0xff669966, 0xff339966, 0xff009966, 0xffff6666,
    0xffcc6666, 0xff996666, 0xff666666, 0xff336666, 0xff006666, 0xffff3366, 0xffcc3366, 0xff993366,
    0xff663366, 0xff333366, 0xff003366, 0xffff0066, 0xffcc0066, 0xff990066, 0xff660066, 0xff330066,
    0xff000066, 0xffffff33, 0xffccff33, 0xff99ff33, 0xff66ff33, 0xff33ff33, 0xff00ff33, 0xffffcc33,
    0xffcccc33, 0xff99cc33, 0xff66cc33, 0xff33cc33, 0xff00cc33, 0xffff9933, 0xffcc9933, 0xff999933,
    0xff669933, 0xff339933, 0xff009933, 0xffff6633, 0xffcc6633, 0xff996633, 0xff666633, 0xff336633,
    0xff006633, 0xffff3333, 0xffcc3333, 0xff993333, 0xff663333, 0xff333333, 0xff003333, 0xffff0033,
    0xffcc0033, 0xff990033, 0xff660033, 0xff330033, 0xff000033, 0xffffff00, 0xffccff00, 0xff99ff00,
    0xff66ff00, 0xff33ff00, 0xff00ff00, 0xffffcc00, 0xffcccc00, 0xff99cc00, 0xff66cc00, 0xff33cc00,
    0xff00cc00, 0xffff9900, 0xffcc9900, 0xff999900, 0xff669900, 0xff339900, 0xff009900, 0xffff6600,
    0xffcc6600, 0xff996600, 0xff666600, 0xff336600, 0xff006600, 0xffff3300, 0xffcc3300, 0xff993300,
    0xff663300, 0xff333300, 0xff003300, 0xffff0000, 0xffcc0000, 0xff990000, 0xff660000, 0xff330000,
    0xff0000ee, 0xff0000dd, 0xff0000bb, 0xff0000aa, 0xff000088, 0xff000077, 0xff000055, 0xff000044,
    0xff000022, 0xff000011, 0xff00ee00, 0xff00dd00, 0xff00bb00, 0xff00aa00, 0xff008800, 0xff007700,
    0xff005500, 0xff004400, 0xff002200, 0xff001100, 0xffee0000, 0xffdd0000, 0xffbb0000, 0xffaa0000,
    0xff880000, 0xff770000, 0xff550000, 0xff440000, 0xff220000, 0xff110000, 0xffeeeeee, 0xffdddddd,
    0xffbbbbbb, 0xffaaaaaa, 0xff888888, 0xff777777, 0xff555555, 0xff444444, 0xff222222, 0xff111111
];
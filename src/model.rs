use amethyst::{
	ecs::{System, Write, WriteExpect, Read, ReadExpect, VecStorage},
	assets::{Asset, AssetStorage, Handle, HotReloadStrategy, ProcessingState},
	core::{ArcThreadPool, Time},
};
use crate::{
    material::{VoxelMaterial, VoxelMaterialId, VoxelMaterialStorage},
    voxel::{AsVoxel, Voxel},
    triangulate::{Const},
    world::*,
};
use std::sync::Arc;
use std::iter::repeat;

pub struct VoxelModelData {
    pub materials: Arc<[VoxelMaterial]>,
    pub voxels: Vec<(usize, usize)>,
    pub dimensions: [usize; 3],
}

pub struct VoxelModel {
    pub voxels: Vec<Option<VoxelMaterialId>>,
    pub dimensions: [usize; 3],
}

#[derive(Debug, Default)]
pub struct VoxelModelProcessor;

impl Asset for VoxelModel {
	const NAME: &'static str = "amethyst_voxel::VoxelModel";
    
    type Data = VoxelModelData;
    type HandleStorage = VecStorage<Handle<VoxelModel>>;
}

impl<'a> System<'a> for VoxelModelProcessor {
    type SystemData = (
        Write<'a, AssetStorage<VoxelModel>>,
        Read<'a, Time>,
        ReadExpect<'a, ArcThreadPool>,
        Option<Read<'a, HotReloadStrategy>>,
        WriteExpect<'a, VoxelMaterialStorage>,
    );

    fn run(
        &mut self,
        (mut model_storage, time, pool, strategy, mut material_storage): Self::SystemData,
    ) {
        use std::ops::Deref;
        model_storage.process(
            |b| {
                let materials: Vec<_> = b.materials
                	.iter()
                	.map(|&m| material_storage.create(m))
                	.collect();
                let mut voxels: Vec<_> = repeat(None)
                	.take(b.dimensions[0]*b.dimensions[1]*b.dimensions[2])
                	.collect();

                for (index, material) in b.voxels {
                	voxels[index] = Some(materials[material]);
                }

                Ok(ProcessingState::Loaded(VoxelModel {
                	voxels,
                	dimensions: b.dimensions,
                }))
            },
            time.frame_number(),
            &**pool,
            strategy.as_ref().map(Deref::deref),
        );
    }
}

impl<V: AsVoxel> Source<V> for VoxelModel where
    V::Data: Default,
    <V::Voxel as Voxel<V::Data>>::Child: Default + From<VoxelMaterialId>,
{
    fn load(&mut self, coord: [isize; 3]) -> VoxelFuture<V> {
        let w = Const::<V::Data>::WIDTH as isize;
        
        let mut from = [0, 0, 0];
        let mut to = [0, 0, 0];

        for i in 0..3 {
            from[i] = (coord[i]*w).max(0) as usize;
            to[i] = (coord[i]*w + w).max(0).min(self.dimensions[i] as isize) as usize;
        }

        let range = [to[0]-from[0], to[1]-from[1], to[2]-from[2]];

        let iter = (0..Const::<V::Data>::COUNT).map(|i| {
            let (x, y, z) = Const::<V::Data>::index_to_coord(i);
            if x < range[0] && y < range[1] && z < range[2] {
                let x = from[0]+x;
                let y = from[1]+y;
                let z = from[2]+z;
                let index = x + y * self.dimensions[0] + z * self.dimensions[0] * self.dimensions[1];
                self.voxels[index].map(|id| id.into()).unwrap_or(Default::default())
            } else {
                Default::default()
            }
        });

        Box::new(futures::future::ok(<V::Voxel as Voxel<V::Data>>::from_iter(Default::default(), iter)))
    }

    fn limits(&self) -> Limits {
        Limits {
            from: [Some(0), Some(0), Some(0)],
            to: [
                Some((self.dimensions[0] / Const::<V::Data>::WIDTH) as isize), 
                Some((self.dimensions[1] / Const::<V::Data>::WIDTH) as isize), 
                Some((self.dimensions[2] / Const::<V::Data>::WIDTH) as isize)
            ],
        }
    }
}

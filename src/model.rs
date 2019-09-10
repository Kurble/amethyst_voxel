use amethyst::{
	ecs::{System, WriteStorage, Write, WriteExpect, Read, ReadExpect, VecStorage, Component, Join},
	assets::{Asset, AssetStorage, Handle, HotReloadStrategy, ProcessingState},
	core::{ArcThreadPool, Time},
};
use crate::{
    material::{VoxelMaterial, VoxelMaterialId, VoxelMaterialStorage},
    voxel::{AsVoxel, Voxel},
    triangulate::{Const},
    world::*,
};
use std::sync::{Arc, Mutex};
use std::iter::repeat;
use std::marker::PhantomData;
use futures::{Future, Async};

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

#[derive(Debug, Default)]
pub struct VoxelModelSourceLoader<V: AsVoxel>(pub PhantomData<V>);

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

pub struct VoxelModelSource<V: AsVoxel> {
    handle: Handle<VoxelModel>,
    requests: Vec<Load<V>>,
}

impl<V: AsVoxel + 'static> Component for VoxelModelSource<V> {
    type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

impl<V: AsVoxel + 'static> Source<V> for VoxelModelSource<V> {
    fn load(&mut self, coord: [isize; 3]) -> VoxelFuture<V> {
        let load = Load { inner: Arc::new(Mutex::new((coord, None))) };
        self.requests.push(load.clone());
        Box::new(load)
    }

    fn limits(&self) -> Limits {
        Limits {
            from: [Some(0), Some(0), Some(0)],
            to: [None, None, None],
        }
    }
}

impl<'s, V: 'static + AsVoxel> System<'s> for VoxelModelSourceLoader<V> where
    V::Data: Default,
    V::Voxel: Default,
    <V::Voxel as Voxel<V::Data>>::Child: Default + From<VoxelMaterialId>, 
{
    type SystemData = (
        WriteStorage<'s, VoxelModelSource<V>>,
        Read<'s, AssetStorage<VoxelModel>>,
    );

    fn run(&mut self, (mut sources, models): Self::SystemData) {
        for source in (&mut sources).join() {
            if let Some(model) = models.get(&source.handle) {
                for request in source.requests.drain(..) {
                    request.process(model);
                }
            }
        }
    }
}

struct Load<V: AsVoxel> {
    inner: Arc<Mutex<([isize; 3], Option<V::Voxel>)>>
}

impl<V: AsVoxel> Future for Load<V> {
    type Item = V::Voxel;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn poll(&mut self) -> Result<Async<V::Voxel>, Self::Error> {
        let mut guard = self.inner.lock().unwrap();

        match guard.1.take() {
            Some(voxel) => Ok(Async::Ready(voxel)),
            None => Ok(Async::NotReady),
        }
    }
}

impl<V: AsVoxel> Clone for Load<V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone()
        }
    }
}

impl<V: AsVoxel> Load<V> where
    V::Data: Default,
    V::Voxel: Default,
    <V::Voxel as Voxel<V::Data>>::Child: Default + From<VoxelMaterialId>, 
{
    fn process(&self, model: &VoxelModel) {
        if let Ok(mut guard) = self.inner.lock() {
            let w = Const::<V::Data>::WIDTH as isize;
            
            let mut from = [0, 0, 0];
            let mut to = [0, 0, 0];

            for i in 0..3 {
                from[i] = (guard.0[i]*w).max(0) as usize;
                to[i] = (guard.0[i]*w + w).max(0).min(model.dimensions[i] as isize) as usize;
            }

            let range = [to[0]-from[0], to[1]-from[1], to[2]-from[2]];

            let iter = (0..Const::<V::Data>::COUNT).map(|i| {
                let (x, y, z) = Const::<V::Data>::index_to_coord(i);
                if x < range[0] && y < range[1] && z < range[2] {
                    let x = from[0]+x;
                    let y = from[1]+y;
                    let z = from[2]+z;
                    let index = x + 
                        y * model.dimensions[0] + 
                        z * model.dimensions[0] * model.dimensions[1];
                    model.voxels[index].map(|id| id.into()).unwrap_or(Default::default())
                } else {
                    Default::default()
                }
            });

            guard.1 = Some(<V as AsVoxel>::Voxel::from_iter(Default::default(), iter));
        }
    }
}

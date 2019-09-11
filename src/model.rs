use amethyst::{
	ecs::{Component, System, 
        Write, WriteExpect, 
        Read, ReadExpect, DenseVecStorage, VecStorage},
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

pub struct VoxelModelSource {
    handle: Handle<VoxelModel>,
    requests: Vec<Box<dyn FnOnce(&VoxelModel)+Send+Sync>>,
    limits: Limits,
}

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
                    let x = index % b.dimensions[0];
                    let y = (index / (b.dimensions[0] * b.dimensions[1])) % b.dimensions[2];
                    let z = (index / b.dimensions[0]) % b.dimensions[1];
                	voxels[x+y*b.dimensions[0]+z*b.dimensions[2]*b.dimensions[0]] = Some(materials[material]);
                }

                Ok(ProcessingState::Loaded(VoxelModel {
                	voxels,
                	dimensions: [b.dimensions[0], b.dimensions[2], b.dimensions[1]],
                }))
            },
            time.frame_number(),
            &**pool,
            strategy.as_ref().map(Deref::deref),
        );
    }
}

impl VoxelModelSource {
    pub fn new(handle: Handle<VoxelModel>) -> Self {
        Self {
            handle,
            requests: Vec::new(),
            limits: Limits { from: [Some(0); 3], to: [None; 3] }
        }
    }
}

impl Component for VoxelModelSource {
    type Storage = DenseVecStorage<VoxelModelSource>;
}

impl<'a, V> Source<'a, V> for VoxelModelSource where
    V: 'static + AsVoxel,
    V::Data: Default,
    <V::Voxel as Voxel<V::Data>>::Child: From<VoxelMaterialId>,
    <V::Voxel as Voxel<V::Data>>::Child: Default,
{
    type SystemData = Read<'a, AssetStorage<VoxelModel>>;

    fn process(&mut self, models: &mut Self::SystemData) {
        if let Some(model) = models.get(&self.handle) {
            for i in 0..3 {
                self.limits.to[i] = Some(model.dimensions[i] as isize / Const::<V::Data>::WIDTH as isize);
            }
            for request in self.requests.drain(..) {
                request(model);
            }
        }
    }

    fn load(&mut self, coord: [isize; 3]) -> VoxelFuture<V> {
        let handle = Arc::new(Mutex::new(None));

        let load = Load::<V> { 
            handle: handle.clone(),
        };

        self.requests.push(Box::new(move |model| {
            if let Ok(mut guard) = handle.lock() {
                let w = Const::<V::Data>::WIDTH as isize;
                
                let mut from = [0, 0, 0];
                let mut to = [0, 0, 0];
                let mut range = [0, 0, 0];

                for i in 0..3 {
                    from[i] = (coord[i]*w).max(0) as usize;
                    to[i] = (coord[i]*w + w).max(0).min(model.dimensions[i] as isize) as usize;
                    if to[i] > from[i] {
                        range[i] = to[i] - from[i];
                    }
                }

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

                *guard = Some(<V as AsVoxel>::Voxel::from_iter(Default::default(), iter));
            }
        }));
        
        Box::new(load)
    }

    fn drop(&mut self, _: [isize; 3], _: V::Voxel) {
        // do nothing
    }

    fn limits(&self) -> Limits {
        self.limits.clone()
    }
}

struct Load<V: AsVoxel> {
    handle: Arc<Mutex<Option<V::Voxel>>>,
}

impl<V: AsVoxel> Future for Load<V> {
    type Item = V::Voxel;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn poll(&mut self) -> Result<Async<V::Voxel>, Self::Error> {
        let mut guard = self.handle.lock().unwrap();

        match guard.take() {
            Some(voxel) => {
                Ok(Async::Ready(voxel))
            },
            None => Ok(Async::NotReady),
        }
    }
}

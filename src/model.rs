use crate::{
    material::{VoxelMaterial, VoxelMaterialId, VoxelMaterialStorage},
    voxel::{Data, Voxel},
    world::*,
};
use amethyst::{
    assets::{Asset, AssetStorage, Handle, HotReloadStrategy, ProcessingState},
    core::{ArcThreadPool, Time},
    ecs::{Component, DenseVecStorage, Read, ReadExpect, System, VecStorage, Write, WriteExpect},
};
use std::iter::repeat;
use std::sync::Arc;

/// Data type for the `Model` asset.
pub struct ModelData {
    materials: Arc<[Arc<dyn VoxelMaterial>]>,
    voxels: Vec<(usize, usize)>,
    dimensions: [usize; 3],
}

/// A predefined model of voxels.
pub struct Model {
    pub voxels: Arc<[Option<VoxelMaterialId>]>,
    pub dimensions: [usize; 3],
}

/// A `VoxelSource` that loads chunks from a `Model`.
pub struct ModelSource {
    handle: Handle<Model>,
    limits: Limits,
}

pub struct ModelProcessor;

impl ModelData {
    /// Create new `ModelData` from raw parst:
    /// materials: a shared arc slice of materials.
    /// voxels: a list of voxels in the format (index, material).
    ///         the index references the index within the dimensions.
    ///         the material references to an index in the materials slice.
    /// dimensions: the three dimensional size of the model.
    pub fn new(
        materials: Arc<[Arc<dyn VoxelMaterial>]>,
        voxels: Vec<(usize, usize)>,
        dimensions: [usize; 3],
    ) -> Self {
        Self {
            materials,
            voxels,
            dimensions,
        }
    }
}

impl Asset for Model {
    const NAME: &'static str = "amethyst_voxel::Model";
    type Data = ModelData;
    type HandleStorage = VecStorage<Handle<Model>>;
}

impl<'a> System<'a> for ModelProcessor {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        Write<'a, AssetStorage<Model>>,
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
                let materials: Vec<_> = b
                    .materials
                    .iter()
                    .map(|m| material_storage.create(m.clone()))
                    .collect();
                let mut voxels: Vec<_> = repeat(None)
                    .take(b.dimensions[0] * b.dimensions[1] * b.dimensions[2])
                    .collect();

                for (index, material) in b.voxels {
                    let x = index % b.dimensions[0];
                    let y = (index / (b.dimensions[0] * b.dimensions[1])) % b.dimensions[2];
                    let z = (index / b.dimensions[0]) % b.dimensions[1];
                    voxels[x + y * b.dimensions[0] + z * b.dimensions[2] * b.dimensions[0]] =
                        Some(materials[material]);
                }

                Ok(ProcessingState::Loaded(Model {
                    voxels: voxels.into(),
                    dimensions: [b.dimensions[0], b.dimensions[2], b.dimensions[1]],
                }))
            },
            time.frame_number(),
            &**pool,
            strategy.as_ref().map(Deref::deref),
        );
    }
}

impl ModelSource {
    pub fn new(handle: Handle<Model>) -> Self {
        Self {
            handle,
            limits: Limits {
                from: [Some(0); 3],
                to: [None; 3],
            },
        }
    }
}

impl Component for ModelSource {
    type Storage = DenseVecStorage<ModelSource>;
}

impl<'a, V> VoxelSource<'a, V> for ModelSource
where
    V: Data + Default,
    Voxel<V>: From<VoxelMaterialId>,
    Voxel<V>: Default,
{
    type SystemData = Read<'a, AssetStorage<Model>>;

    fn load_voxel(
        &mut self,
        models: &mut Self::SystemData,
        coord: [isize; 3],
    ) -> VoxelSourceResult<V> {
        if let Some(model) = models.get(&self.handle) {
            let dimensions = model.dimensions.clone();
            let voxels = model.voxels.clone();
            let w = Voxel::<V>::WIDTH as isize;
            for i in 0..3 {
                let d = dimensions[i] as isize;
                self.limits.to[i] = Some(if d % w == 0 { d / w - 1 } else { d / w });
            }

            VoxelSourceResult::Loading(Box::new(move || {
                let w = Voxel::<V>::WIDTH as isize;
                let mut from = [0, 0, 0];
                let mut to = [0, 0, 0];
                let mut range = [0, 0, 0];

                for i in 0..3 {
                    from[i] = (coord[i] * w).max(0) as usize;
                    to[i] = (coord[i] * w + w).max(0).min(dimensions[i] as isize) as usize;
                    if to[i] > from[i] {
                        range[i] = to[i] - from[i];
                    }
                }

                let iter = (0..Voxel::<V>::COUNT).map(|i| {
                    let (x, y, z) = Voxel::<V>::index_to_coord(i);
                    if x < range[0] && y < range[1] && z < range[2] {
                        let x = from[0] + x;
                        let y = from[1] + y;
                        let z = from[2] + z;
                        let index = x + y * dimensions[0] + z * dimensions[0] * dimensions[1];
                        voxels[index].map(|id| id.into()).unwrap_or_default()
                    } else {
                        Default::default()
                    }
                });

                Voxel::from_iter(Default::default(), iter)
            }))
        } else {
            VoxelSourceResult::Retry
        }
    }

    fn drop_voxel(
        &mut self,
        _: &mut Self::SystemData,
        _: [isize; 3],
        _: Voxel<V>,
    ) -> Box<dyn FnOnce() + Send> {
        Box::new(|| ())
    }

    fn limits(&self) -> Limits {
        self.limits.clone()
    }
}

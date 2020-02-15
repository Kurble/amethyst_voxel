use crate::material::AtlasProcessor;
use crate::{mesh::*, voxel::Data, world::VoxelSource, world::VoxelWorld};
use amethyst::{
    core::bundle::SystemBundle,
    ecs::prelude::{Component, DispatcherBuilder, WorldExt},
    error::Error,
    prelude::World,
    renderer::Backend,
};
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::sync::Arc;

type SystemRegistrator = dyn for<'a, 'b> FnOnce(&mut World, &mut DispatcherBuilder<'a, 'b>);

/// Main bundle for supporting voxels in your amethyst project.
/// Before any `Voxel<T>` type will work,
///  you have to specify which `Data` and `Source` implementations you plan to use.
pub struct VoxelBundle {
    triangulation_limit: usize,
    systems: Vec<Box<SystemRegistrator>>,
    pool: Arc<ThreadPool>,
}

impl VoxelBundle {
    pub fn new(triangulation_limit: usize) -> Self {
        VoxelBundle {
            triangulation_limit,
            systems: Vec::new(),
            pool: Arc::new(
                ThreadPoolBuilder::new()
                    .num_threads(2)
                    .build()
                    .expect("Unable to create threadpool for voxel loading"),
            ),
        }
    }

    /// Configure systems that load voxels with `Data` `V` from the source `S`.
    pub fn with_source<V, S>(mut self) -> Self
    where
        V: Data,
        S: for<'s> VoxelSource<'s, V> + Component + Send + Sync,
    {
        let pool = self.pool.clone();
        self.systems.push(Box::new(|_world, builder| {
            builder.add(
                crate::world::WorldSystem::<V, S>::new(pool),
                "world_sourcing",
                &[],
            )
        }));
        self
    }

    /// Configure systems that work with `Data` `V`.
    pub fn with_voxel<B: Backend, V: Data + Default>(mut self) -> Self {
        self.systems.push(Box::new({
            let triangulation_limit = self.triangulation_limit;
            move |world, builder| {
                world.register::<VoxelWorld<V>>();

                let triangulator = TriangulatorSystem::<B, V>::new(triangulation_limit);
                builder.add(triangulator, "triangulator", &[]);

                let processor = VoxelMeshProcessor::<B, V>::new();
                builder.add(processor, "voxel_mesh_processor", &[]);
            }
        }));
        self
    }
}

impl<'a, 'b> SystemBundle<'a, 'b> for VoxelBundle {
    fn build(
        self,
        world: &mut World,
        builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), Error> {
        builder.add(AtlasProcessor, "atlas_processor", &[]);
        for sys in self.systems.into_iter() {
            sys(world, builder);
        }
        Ok(())
    }
}

use crate::{voxel::Data, world::VoxelSource, world::VoxelWorld};
use amethyst::{
    core::bundle::SystemBundle,
    ecs::prelude::{Component, DispatcherBuilder, WorldExt},
    error::Error,
    prelude::World,
};
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::sync::Arc;

type SystemRegistrator = dyn for<'a, 'b> FnOnce(&mut World, &mut DispatcherBuilder<'a, 'b>);

/// Main bundle for supporting voxels in your amethyst project.
/// Before any `Voxel<T>` type will work,
///  you have to specify which `Data` and `Source` implementations you plan to use.
pub struct VoxelBundle {
    systems: Vec<Box<SystemRegistrator>>,
    pool: Arc<ThreadPool>,
}

impl VoxelBundle {
    pub fn new() -> Self {
        VoxelBundle {
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
    pub fn with_voxel<V: Data>(mut self) -> Self {
        self.systems.push(Box::new(|world, builder| {
            world.register::<VoxelWorld<V>>();
            builder.add(
                crate::movement::MovementSystem::<V>::new(),
                "voxel_movement",
                &[],
            )
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
        builder.add(
            crate::material::VoxelMaterialSystem,
            "voxel_material_system",
            &[],
        );
        builder.add(crate::model::ModelProcessor, "voxel_model_processor", &[]);
        for sys in self.systems.into_iter() {
            sys(world, builder);
        }
        Ok(())
    }
}

use amethyst::{
    core::bundle::SystemBundle,
    ecs::prelude::{Component, DispatcherBuilder},
    error::Error,
};
use crate::{
    voxel::{AsVoxel},
    world::Source,
};


#[derive(Default)]
pub struct VoxelBundle {
    systems: Vec<Box<dyn for<'a, 'b> FnOnce(&mut DispatcherBuilder<'a, 'b>)>>,
}

impl VoxelBundle {
    pub fn new() -> Self {
        VoxelBundle{
            systems: Vec::new(),
        }
    }

    pub fn with_source<V, S>(mut self) -> Self where
        V: 'static + AsVoxel, 
        S: for<'s> Source<'s, V> + Component + Send + Sync
    {
        self.systems.push(Box::new(|builder| builder.add(
            crate::world::WorldSourceSystem::<V, S>::new(), "world_sourcing", &[]
        )));
        self
    }
}

impl<'a, 'b> SystemBundle<'a, 'b> for VoxelBundle {
    fn build(
        self,
        builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), Error> {
        builder.add(crate::material::VoxelMaterialSystem, "voxel_material_system", &[]);
        builder.add(crate::model::VoxelModelProcessor, "voxel_model_processor", &[]);
        for sys in self.systems.into_iter() {
            sys(builder);
        }
        Ok(())
    }
}

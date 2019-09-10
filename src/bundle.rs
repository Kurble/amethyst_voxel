use amethyst::{
    core::bundle::SystemBundle,
    ecs::prelude::DispatcherBuilder,
    error::Error,
};
use crate::{
    voxel::{Voxel, AsVoxel},
    material::VoxelMaterialId,
};
use std::marker::PhantomData;

#[derive(Default)]
pub struct VoxelBundle<V: AsVoxel>(PhantomData<V>);

impl<V: AsVoxel> VoxelBundle<V> {
    pub fn new() -> Self {
        VoxelBundle(PhantomData)
    }
}

impl<'a, 'b, V: 'static + AsVoxel> SystemBundle<'a, 'b> for VoxelBundle<V> where
    V::Data: Default,
    V::Voxel: Default,
    <V::Voxel as Voxel<V::Data>>::Child: Default + From<VoxelMaterialId>, 
{
    fn build(
        self,
        builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), Error> {
        builder.add(crate::material::VoxelMaterialSystem, "voxel_material_system", &[]);
        builder.add(crate::model::VoxelModelProcessor, "voxel_model_processor", &[]);
        builder.add(crate::model::VoxelModelSourceLoader::<V>(PhantomData), "voxel_model_processor", &[]);
        builder.add(crate::system::WorldLoaderSystem::<V>(PhantomData), "world_loader_system", &[]);
        Ok(())
    }
}

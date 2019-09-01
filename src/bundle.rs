use amethyst::{
    core::bundle::SystemBundle,
    ecs::prelude::DispatcherBuilder,
    error::Error,
};
use crate::{
    voxel::GenericVoxel,
};
use std::marker::PhantomData;

#[derive(Default)]
pub struct VoxelBundle<V: GenericVoxel>(PhantomData<V>);

impl<V: GenericVoxel> VoxelBundle<V> {
    pub fn new() -> Self {
        VoxelBundle(PhantomData)
    }
}

impl<'a, 'b, V: GenericVoxel> SystemBundle<'a, 'b> for VoxelBundle<V> {
    fn build(
        self,
        builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), Error> {
        builder.add(
            crate::system::VoxelSystem::<V>::new(), "voxel_system", &[]);
        Ok(())
    }
}

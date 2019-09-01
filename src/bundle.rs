use amethyst::{
    core::bundle::SystemBundle,
    ecs::prelude::DispatcherBuilder,
    error::Error,
};
use crate::{
    voxel::AsVoxel,
};
use std::marker::PhantomData;

#[derive(Default)]
pub struct VoxelBundle<V: AsVoxel>(PhantomData<V>);

impl<V: AsVoxel> VoxelBundle<V> {
    pub fn new() -> Self {
        VoxelBundle(PhantomData)
    }
}

impl<'a, 'b, V: 'static + AsVoxel> SystemBundle<'a, 'b> for VoxelBundle<V> {
    fn build(
        self,
        builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), Error> {
        builder.add(
            crate::system::VoxelSystem::<V>::new(), "voxel_system", &[]);
        Ok(())
    }
}

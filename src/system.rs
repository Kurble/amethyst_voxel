use std::marker::PhantomData;
use crate::voxel::GenericVoxel;
use crate::MutableVoxels;
use amethyst::ecs::prelude::*;

pub struct VoxelSystem<V: GenericVoxel>(PhantomData<V>);

impl<V: GenericVoxel> VoxelSystem<V> {
	pub fn new() -> Self {
		VoxelSystem(PhantomData)
	}
}

impl<'a, V: GenericVoxel> System<'a> for VoxelSystem<V> {
    type SystemData = (
        Entities<'a>,
        WriteStorage<'a, MutableVoxels<V>>,
    );

    fn run(&mut self, _: Self::SystemData) {
        // todo, what does this system do??
    }

    fn setup(&mut self, res: &mut Resources) {
        Self::SystemData::setup(res);
    }
}

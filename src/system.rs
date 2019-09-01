use std::marker::PhantomData;
use crate::voxel::AsVoxel;
use crate::MutableVoxels;
use amethyst::ecs::prelude::*;

pub struct VoxelSystem<V: AsVoxel>(PhantomData<V>);

impl<V: AsVoxel> VoxelSystem<V> {
	pub fn new() -> Self {
		VoxelSystem(PhantomData)
	}
}

impl<'a, V: 'static + AsVoxel> System<'a> for VoxelSystem<V> {
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

use crate::{
	raycast::*,
	voxel::{Data, Voxel},
	world::VoxelWorld,
};
use amethyst::{core::transform::Transform, ecs::prelude::*};
use nalgebra_glm::*;
use std::marker::PhantomData;

pub struct Pos {
	pub position: Vec3,
	pub velocity: Vec3,
}

#[derive(Default)]
pub struct MovementSystem<V: Data> {
	marker: PhantomData<V>,
}

impl Component for Pos {
	type Storage = DenseVecStorage<Self>;
}

impl<V: Data> MovementSystem<V> {
	pub fn new() -> Self {
		Self {
			marker: PhantomData,
		}
	}
}

impl<'a, V: Data> System<'a> for MovementSystem<V>
where
	Voxel<V>: Raycast,
{
	type SystemData = (
		ReadStorage<'a, VoxelWorld<V>>,
		WriteStorage<'a, Pos>,
		WriteStorage<'a, Transform>,
	);

	fn run(&mut self, (worlds, mut positions, mut transforms): Self::SystemData) {
		for (pos, transform) in (&mut positions, &mut transforms).join() {
			let velocity = (&worlds).join().fold(pos.velocity, |mut velocity, world| {
				for i in 0..3 {
					let mut dir = vec3(0.0, 0.0, 0.0);
					dir[i] = velocity[i];
					velocity[i] *= world
						.hit(&world.ray(pos.position, dir))
						.unwrap_or(1.0)
						.min(1.0);
				}
				velocity
			});

			pos.position += velocity;

			transform.set_translation(pos.position);
		}
	}
}

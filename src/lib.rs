#[macro_use]
extern crate derivative;

mod coordinate;
mod side;
mod voxel;
mod triangulate;

mod bundle;
mod system;
mod plugin;
mod pass;

pub use voxel::{Simple, Nested, AsVoxel, VoxelData};
pub use bundle::VoxelBundle;

pub type RenderVoxelPbr<V> = plugin::RenderVoxel<amethyst::renderer::pass::ShadedPassDef, V>;

pub struct MutableVoxels<V: AsVoxel> {
	pub(crate) data: V::Voxel,
	pub(crate) dirty: bool,
	pub(crate) mesh: Option<usize>,
}

use std::ops::{Deref, DerefMut};

impl<V: AsVoxel> MutableVoxels<V> {
	pub fn new(value: V::Voxel) -> Self {
		MutableVoxels {
			data: value,
			dirty: true,
			mesh: None,
		}
	}
}

impl<V: voxel::AsNestedVoxel> MutableVoxels<V> {
	pub fn from_iter<I>(data: V::Data, iter: I) -> Self where
        I: IntoIterator<Item = V::Child>
    {
        MutableVoxels {
        	data: V::from_iter(data, iter),
        	dirty: true,
        	mesh: None,
        }
    }
}

impl<V: AsVoxel> Deref for MutableVoxels<V> {
	type Target = V::Voxel;

	fn deref(&self) -> &V::Voxel {
		&self.data
	}
}

impl<V: AsVoxel> DerefMut for MutableVoxels<V> {
	fn deref_mut(&mut self) -> &mut V::Voxel {
		self.dirty = true;
		&mut self.data
	}
}

impl<V: AsVoxel + 'static> amethyst::ecs::Component for MutableVoxels<V> {
	type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

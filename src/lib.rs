#[macro_use]
extern crate downcast_rs;
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

pub use voxel::{GenericVoxel, Static, Dynamic, AsVoxel, Metadata};
pub use bundle::VoxelBundle;

pub type RenderVoxelPbr<V> = plugin::RenderVoxel<amethyst::renderer::pass::PbrPassDef, V>;

pub struct MutableVoxels<V: GenericVoxel> {
	pub(crate) data: V,
	pub(crate) dirty: bool,
	pub(crate) mesh: Option<usize>,
}

use std::ops::{Deref, DerefMut};

impl<V: GenericVoxel> From<V> for MutableVoxels<V> {
	fn from(value: V) -> Self {
		MutableVoxels {
			data: value,
			dirty: true,
			mesh: None,
		}
	}
}

impl<V: GenericVoxel> Deref for MutableVoxels<V> {
	type Target = V;

	fn deref(&self) -> &V {
		&self.data
	}
}

impl<V: GenericVoxel> DerefMut for MutableVoxels<V> {
	fn deref_mut(&mut self) -> &mut V {
		self.dirty = true;
		&mut self.data
	}
}

impl<V: GenericVoxel> amethyst::ecs::Component for MutableVoxels<V> {
	type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

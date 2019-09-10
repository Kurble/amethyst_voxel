#[macro_use]
extern crate derivative;

mod coordinate;
mod side;
mod voxel;
mod context;
mod world;
mod triangulate;
mod ambient_occlusion;
mod material;
mod model;
mod vox;

mod bundle;
mod system;
mod plugin;
mod pass;

pub use voxel::{Simple, Nested, Voxel, AsVoxel, VoxelData};
pub use world::{MutableVoxelWorld, Source, Limits, VoxelFuture};
pub use bundle::VoxelBundle;
pub use material::{VoxelMaterial, VoxelMaterialStorage, VoxelMaterialId};
pub use model::{VoxelModel};
pub use vox::{VoxFormat};

pub type RenderVoxelPbr<V> = plugin::RenderVoxel<pass::VoxelPassDef<amethyst::renderer::pass::PbrPassDef>, V>;

pub struct MutableVoxel<V: AsVoxel> {
	pub(crate) data: V::Voxel,
	pub(crate) dirty: bool,
	pub(crate) mesh: Option<usize>,
}

use std::ops::{Deref, DerefMut};

impl<V: AsVoxel> MutableVoxel<V> {
	pub fn new(value: V::Voxel) -> Self {
		MutableVoxel {
			data: value,
			dirty: true,
			mesh: None,
		}
	}

	pub fn from_iter<I>(data: V::Data, iter: I) -> Self where
        I: IntoIterator<Item = <<V as AsVoxel>::Voxel as Voxel<V::Data>>::Child>
    {
        MutableVoxel {
        	data: <V as AsVoxel>::Voxel::from_iter(data, iter),
        	dirty: true,
        	mesh: None,
        }
    }
}

impl<V: AsVoxel> Deref for MutableVoxel<V> {
	type Target = V::Voxel;

	fn deref(&self) -> &V::Voxel {
		&self.data
	}
}

impl<V: AsVoxel> DerefMut for MutableVoxel<V> {
	fn deref_mut(&mut self) -> &mut V::Voxel {
		self.dirty = true;
		&mut self.data
	}
}

impl<V: AsVoxel + 'static> amethyst::ecs::Component for MutableVoxel<V> {
	type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

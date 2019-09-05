#[macro_use]
extern crate derivative;

mod coordinate;
mod side;
mod voxel;
mod triangulate;
mod ambient_occlusion;
mod material;
mod io;

mod bundle;
mod system;
mod plugin;
mod pass;

pub use voxel::{Simple, Nested, AsVoxel, VoxelData, Context};
pub use bundle::VoxelBundle;
pub use material::{VoxelMaterial, VoxelMaterialStorage};
pub use io::load_vox;

pub type RenderVoxelPbr<V> = plugin::RenderVoxel<pass::VoxelPassDef<amethyst::renderer::pass::PbrPassDef>, V>;

pub struct MutableVoxels<V: AsVoxel> {
	pub(crate) data: V::Voxel,
	pub(crate) dirty: bool,
	pub(crate) mesh: Option<usize>,
}

pub struct MutableChunkedVoxels<V: AsVoxel> {
	data: Vec<MutableVoxels<V>>,
	dims: [usize; 3],
	center: [isize; 3],
	//
}

pub struct Focus<'a, V: AsVoxel>([isize; 3], &'a MutableChunkedVoxels<V>);

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

impl<'a, V: AsVoxel> Focus<'a, V> {
	fn find(&self, x: isize, y: isize, z: isize) -> Option<(usize, usize)> {
		//let pitch = Const::<V::Data>::WIDTH as isize;
		//let grid = |x| if x >= 0 { x / pitch } else { (x+1) / pitch - 1};
		//let coord = [grid(x), grid(y), grid(z)];
		//let
		None
	}
}

impl<'a, V: AsVoxel> Context for Focus<'a, V> {
	fn visible(&self, x: isize, y: isize, z: isize) -> bool {
		if let Some((chunk, index)) = self.find(x, y, z) {
			// todo
		}

		false
	}

	fn render(&self, x: isize, y: isize, z: isize) -> bool {
		false
	}
}

impl<V: AsVoxel + 'static> amethyst::ecs::Component for MutableVoxels<V> {
	type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

impl<V: AsVoxel + 'static> amethyst::ecs::Component for MutableChunkedVoxels<V> {
	type Storage = amethyst::ecs::DenseVecStorage<Self>;
}

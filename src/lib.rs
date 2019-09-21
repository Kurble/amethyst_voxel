//! # amethyst_voxel
//! 
//! A voxel toolbox for the amethyst game engine.
//! 
//! This library makes some tools available to the user to work with voxels in the amethyst game engine.
//! The most important starting point are the `Voxel` and `VoxelData` trait, these specify how you want to work with voxels.
//! With amethyst_voxel, the idea is that you define a recursive structure for your voxels, to which you can attach user data.
//! Each type of user data has to implement the `VoxelData` trait, which specifies a subdivision level. 
//! If you were to make a minecraft clone for example, you could have 3 VoxelData implementations, one for each level of recursiveness:
//! * `Region` a 128^3 area that contains chunks
//! * `Chunk` a 16^3 block of blocks
//! * `Block` a simple block. But because you could have detailed blocks, it has 16^3 subvoxels. 
//! This is of course not entirely how Minecraft works, but I think it's close enough for understanding the idea.
//! One big difference for example, is that amethyst_voxel only has voxels with one solid color and no textures. 
//! Recursive voxels can thus be very useful for adding a little more detail to your project.
//!
//! For a more detailed explanation, you can take a look at the examples or the detail explanations in this api documentation.

#[macro_use]
extern crate derivative;

pub mod voxel;
pub mod material;
pub mod world;
pub mod collision;
pub mod movement;
pub mod model;
pub mod vox;

mod coordinate;
mod side;
mod context;
mod triangulate;
mod ambient_occlusion;
mod bundle;
mod plugin;
mod pass;

pub mod prelude;
pub mod utils;
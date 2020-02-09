use amethyst::assets::{Format, Handle};
use serde::Deserialize;

use crate::mesh::VoxelMesh;
use crate::model::ModelData;

#[derive(Debug, Clone, Deserialize)]
pub enum VoxelMeshPrefab<F: Format<ModelData>> {
    File(String, F),

    #[serde(skip)]
    Handle(Handle<VoxelMesh>),
}

pub enum DynamicVoxelMeshPrefab<F: Format<ModelData>> {
    File(String, F),

    Empty,
}

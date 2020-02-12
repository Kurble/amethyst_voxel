use amethyst::ecs::*;
use amethyst::assets::{Format, Handle, PrefabData, ProgressCounter, Loader, AssetStorage, Progress};
use amethyst::error::*;
use serde::Deserialize;
use std::marker::PhantomData;

use crate::voxel::{Voxel, Data};
use crate::mesh::{DynamicVoxelMesh, VoxelMesh};
use crate::model::ModelData;

#[derive(Clone, Deserialize)]
pub enum VoxelMeshPrefab<V: Data, F: Format<ModelData>> {
    File(String, F),

    #[serde(skip)]
    Handle(Handle<VoxelMesh>),

    #[serde(skip)]
    Marker(PhantomData<V>),
}

#[derive(Deserialize)]
pub enum DynamicVoxelMeshPrefab<V: Data, F: Format<ModelData>> {
    File(String, F),

    #[serde(skip)]
    Handle(Handle<Voxel<V>>),
}

impl<'a, V: Data, F: Format<ModelData>> PrefabData<'a> for VoxelMeshPrefab<V, F> {
    type SystemData = (ReadExpect<'a, Loader>, Read<'a, AssetStorage<VoxelMesh>>, WriteStorage<'a, Handle<VoxelMesh>>);
    type Result = ();

    fn add_to_entity(
        &self,
        entity: Entity,
        (_, _, handles): &mut Self::SystemData,
        _: &[Entity],
        _: &[Entity],
    ) -> Result<Self::Result, Error> {
        match self {
            VoxelMeshPrefab::Handle(handle) => {
                handles.insert(entity, handle.clone())?;
            }

            _ => unreachable!(),
        }
        Ok(())
    }

    fn load_sub_assets(
        &mut self,
        mut progress: &mut ProgressCounter,
        (loader, storage, _): &mut Self::SystemData,
    ) -> Result<bool, Error> {        
        match std::mem::replace(self, VoxelMeshPrefab::Marker(PhantomData)) {
            VoxelMeshPrefab::File(file, format) => {
                
                progress.add_assets(1);
                *self = VoxelMeshPrefab::Handle(loader.load(file, format, progress, storage));

                Ok(true)
            }

            _ => Ok(false),
        }
    }
}

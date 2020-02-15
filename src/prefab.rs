use amethyst::assets::{AssetStorage, Handle, Loader, PrefabData, Progress, ProgressCounter};
use amethyst::ecs::*;
use amethyst::error::*;
use serde::Deserialize;

use crate::mesh::{DynamicVoxelMesh, DynamicVoxelMeshData, VoxelMesh};
use crate::vox::VoxFormat;
use crate::voxel::Data;

#[derive(Clone, Deserialize)]
pub enum VoxelMeshPrefab {
    File(String),

    #[serde(skip)]
    Handle(Handle<VoxelMesh>),

    #[serde(skip)]
    Placeholder,
}

#[derive(Deserialize)]
pub enum DynamicVoxelMeshPrefab<V: Data> {
    File(String),

    #[serde(skip)]
    Handle(Handle<DynamicVoxelMeshData<V>>),

    #[serde(skip)]
    Placeholder,
}

impl<'a> PrefabData<'a> for VoxelMeshPrefab {
    type SystemData = (
        ReadExpect<'a, Loader>,
        Read<'a, AssetStorage<VoxelMesh>>,
        WriteStorage<'a, Handle<VoxelMesh>>,
    );
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
        match std::mem::replace(self, VoxelMeshPrefab::Placeholder) {
            VoxelMeshPrefab::File(file) => {
                progress.add_assets(1);
                *self = VoxelMeshPrefab::Handle(loader.load(file, VoxFormat, progress, storage));
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl<'a, V: Data> PrefabData<'a> for DynamicVoxelMeshPrefab<V> {
    type SystemData = (
        ReadExpect<'a, Loader>,
        Read<'a, AssetStorage<DynamicVoxelMeshData<V>>>,
        WriteStorage<'a, DynamicVoxelMesh<V>>,
    );
    type Result = ();

    fn add_to_entity(
        &self,
        entity: Entity,
        (_, storage, mesh): &mut Self::SystemData,
        _: &[Entity],
        _: &[Entity],
    ) -> Result<Self::Result, Error> {
        match self {
            DynamicVoxelMeshPrefab::Handle(handle) => {
                let voxel = storage.get(handle).expect("Voxel not loaded");
                mesh.insert(
                    entity,
                    DynamicVoxelMesh::new(voxel.data.clone(), voxel.atlas.clone()),
                )?;
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
        match std::mem::replace(self, DynamicVoxelMeshPrefab::Placeholder) {
            DynamicVoxelMeshPrefab::File(file) => {
                progress.add_assets(1);
                *self =
                    DynamicVoxelMeshPrefab::Handle(loader.load(file, VoxFormat, progress, storage));
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

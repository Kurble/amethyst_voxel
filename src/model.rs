use crate::material::VoxelMaterial;
use std::sync::Arc;

/// Data type for the `Model` asset.
pub struct ModelData {
    pub materials: Arc<[Arc<dyn VoxelMaterial>]>,
    pub voxels: Vec<(usize, usize)>,
    pub dimensions: [usize; 3],
}

impl ModelData {
    /// Create new `ModelData` from raw parst:
    /// materials: a shared arc slice of materials.
    /// voxels: a list of voxels in the format (index, material).
    ///         the index references the index within the dimensions.
    ///         the material references to an index in the materials slice.
    /// dimensions: the three dimensional size of the model.
    pub fn new(
        materials: Arc<[Arc<dyn VoxelMaterial>]>,
        voxels: Vec<(usize, usize)>,
        dimensions: [usize; 3],
    ) -> Self {
        Self {
            materials,
            voxels,
            dimensions,
        }
    }
}

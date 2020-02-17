use crate::material::VoxelMaterial;
use nalgebra_glm::*;
use std::sync::Arc;

/// Data type for the `Model` asset.
pub struct ModelData {
    /// List of materials used in this `ModelData`.
    pub materials: Arc<[Arc<dyn VoxelMaterial>]>,
    /// List of submodels.
    pub submodels: Vec<SubModelData>,
    /// Optional list of bones, together forming a skeleton for skinning.
    pub skeleton: Vec<Bone>,
}

pub struct SubModelData {
    /// Voxel data for the sub model
    pub voxels: Vec<Instance>,
    /// The dimensions of the voxel data
    pub dimensions: [usize; 3],
    /// Offset from the origin for this submodel
    pub offset: Mat4x4,
}

pub struct Instance {
    /// the index references the index within the dimensions of the
    ///  `SubModelData` this `Instance` is contained in.
    pub index: usize,
    /// The material references to an index in the materials slice
    ///  of the `ModelData` this `Instance` is contained in.
    pub material: usize,
    /// Reference to a bone in the `ModelData`.
    /// Ignored if the `ModelData` doesn't have bones.
    pub bone: usize,
}

pub struct Bone {
    pub parent: Option<usize>,
    pub bind_matrix: Mat4x4,
}

impl ModelData {
    /// Create new `ModelData` from raw parts:
    /// materials: a shared arc slice of materials.
    /// submodels: a list of submodels
    pub fn new(
        materials: Arc<[Arc<dyn VoxelMaterial>]>,
        submodels: Vec<SubModelData>,
        skeleton: Vec<Bone>,
    ) -> Self {
        Self {
            materials,
            submodels,
            skeleton,
        }
    }
}

impl SubModelData {
    /// Create a new `SubModelData`.
    /// voxels: a list of voxels in the format (index, material).
    ///         the index references the index within the dimensions.
    ///         the material references to an index in the materials slice.
    /// dimensions: the three dimensional size of the model.
    pub fn new(voxels: Vec<Instance>, dimensions: [usize; 3]) -> Self {
        Self { voxels, dimensions, offset: Mat4x4::identity() }
    }
}

use crate::mesh::VoxelMesh;
use crate::pass::*;

use amethyst::{
    assets::Handle,
    core::ecs::{DispatcherBuilder, World, WorldExt},
    error::Error,
    renderer::{
        bundle::{RenderOrder, RenderPlan, RenderPlugin, Target},
        pass::Base3DPassDef,
        Backend, Factory,
        rendy::graph::render::RenderGroupDesc,
    },
};

/// A `RenderPlugin` for forward rendering of 3d voxels.
/// Generic over 3d pass rendering method.
#[derive(derivative::Derivative)]
#[derivative(Default(bound = ""), Debug(bound = ""))]
pub struct RenderVoxel<D: Base3DPassDef> {
    target: Target,
    skinning: bool,
    marker: std::marker::PhantomData<D>,
}

impl<D: Base3DPassDef> RenderVoxel<D> {
    /// Set target to which 3d meshes will be rendered.
    pub fn with_target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    /// Enable or disable skinning.
    pub fn with_skinning(mut self, skinned: bool) -> Self {
        self.skinning = skinned;
        self
    }
}

impl<B, D> RenderPlugin<B> for RenderVoxel<D>
where
    B: Backend,
    D: Base3DPassDef,
{
    fn on_build<'a, 'b>(
        &mut self,
        world: &mut World,
        _builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), Error> {
        world.register::<Handle<VoxelMesh>>();
        //builder.add(VisibilitySortingSystem::new(), "visibility_system", &[]);
        Ok(())
    }

    fn on_plan(
        &mut self,
        plan: &mut RenderPlan<B>,
        _factory: &mut Factory<B>,
        _world: &World,
    ) -> Result<(), Error> {
        let skinning = self.skinning;
        plan.extend_target(self.target, move |ctx| {
            ctx.add(
                RenderOrder::Opaque,
                DrawVoxelDesc::<B, D>::new(skinning, false).builder(),
            )?;
            Ok(())
        });
        plan.extend_target(self.target, move |ctx| {
            ctx.add(
                RenderOrder::Transparent,
                DrawVoxelDesc::<B, D>::new(skinning, true).builder(),
            )?;
            Ok(())
        });
        Ok(())
    }
}

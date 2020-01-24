use crate::pass::*;
use crate::voxel::Data;
use crate::world::VoxelRender;

use amethyst::{
    core::ecs::{DispatcherBuilder, World, WorldExt},
    error::Error,
    renderer::{
        bundle::{RenderOrder, RenderPlan, RenderPlugin, Target},
        pass::Base3DPassDef,
        Backend, Factory,
    },
};
use rendy::graph::render::RenderGroupDesc;

/// A `RenderPlugin` for forward rendering of 3d voxels.
/// Generic over 3d pass rendering method.
#[derive(derivative::Derivative)]
#[derivative(Default(bound = ""), Debug(bound = ""))]
pub struct RenderVoxel<D: Base3DPassDef, V: Data> {
    target: Target,
    marker: std::marker::PhantomData<(D, V)>,
    triangulate_limit: usize,
}

impl<D: Base3DPassDef, V: Data> RenderVoxel<D, V> {
    /// Set target to which 3d meshes will be rendered.
    pub fn with_target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    /// Set a per frame triangulation limit. The default is 4
    pub fn with_triangulate_limit(mut self, triangulate_limit: usize) -> Self {
        self.triangulate_limit = triangulate_limit;
        self
    }
}

impl<B, D, V> RenderPlugin<B> for RenderVoxel<D, V>
where
    B: Backend,
    D: Base3DPassDef,
    V: Data,
{
    fn on_build<'a, 'b>(
        &mut self,
        world: &mut World,
        _builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), Error> {
        world.register::<VoxelRender<V>>();
        //builder.add(VisibilitySortingSystem::new(), "visibility_system", &[]);
        Ok(())
    }

    fn on_plan(
        &mut self,
        plan: &mut RenderPlan<B>,
        _factory: &mut Factory<B>,
        _world: &World,
    ) -> Result<(), Error> {
        let limit = if self.triangulate_limit == 0 {
            2
        } else {
            self.triangulate_limit
        };

        plan.extend_target(self.target, move |ctx| {
            ctx.add(
                RenderOrder::Opaque,
                DrawVoxelDesc::<B, D, V>::new(limit, false).builder(),
            )?;
            Ok(())
        });
        plan.extend_target(self.target, move |ctx| {
            ctx.add(
                RenderOrder::Transparent,
                DrawVoxelDesc::<B, D, V>::new(limit, true).builder(),
            )?;
            Ok(())
        });
        Ok(())
    }
}

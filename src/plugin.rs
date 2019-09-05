use crate::voxel::AsVoxel;
use crate::pass::*;
use crate::ambient_occlusion::*;
use amethyst::{
    renderer::{
        bundle::{RenderOrder, RenderPlan, RenderPlugin, Target},
        pass::Base3DPassDef,
        Backend, Factory,
    },
    core::ecs::{DispatcherBuilder, Resources},
    error::{Error},
};
use rendy::graph::render::RenderGroupDesc;

/// A `RenderPlugin` for forward rendering of 3d objects.
/// Generic over 3d pass rendering method.
#[derive(derivative::Derivative)]
#[derivative(Default(bound = ""), Debug(bound = ""))]
pub struct RenderVoxel<D: Base3DPassDef, V: AsVoxel> {
    target: Target,
    marker: std::marker::PhantomData<(D, V)>,
}

impl<D: Base3DPassDef, V: AsVoxel> RenderVoxel<D, V> {
    /// Set target to which 3d meshes will be rendered.
    pub fn with_target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }
}

impl<'ao, B, D, V> RenderPlugin<B> for RenderVoxel<D, V> where
    B: Backend,
    D: Base3DPassDef,
    V: 'static + AsVoxel,
    AmbientOcclusion<'ao>: BuildAmbientOcclusion<'ao, <V as AsVoxel>::Data, <V as AsVoxel>::Voxel>, 
{
    fn on_build<'a, 'b>(
        &mut self,
        _builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), Error> {
        //builder.add(VisibilitySortingSystem::new(), "visibility_system", &[]);
        Ok(())
    }

    fn on_plan(
        &mut self,
        plan: &mut RenderPlan<B>,
        _factory: &mut Factory<B>,
        _resources: &Resources,
    ) -> Result<(), Error> {
        plan.extend_target(self.target, move |ctx| {
            ctx.add(RenderOrder::Opaque, DrawVoxelDesc::<B, D, V>::new().builder())?;
            Ok(())
        });
        Ok(())
    }
}
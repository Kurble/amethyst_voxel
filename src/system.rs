use crate::voxel::AsVoxel;
use crate::world::*;
use amethyst::{
    core::{transform::Transform},
    ecs::prelude::{Join, Read, ReadStorage, System, WriteStorage},
    renderer::{ActiveCamera, Camera},
};
use std::marker::PhantomData;

pub struct WorldLoaderSystem<V: AsVoxel>(pub PhantomData<V>);

impl<'s, V: 'static + AsVoxel> System<'s> for WorldLoaderSystem<V> {
    type SystemData = (
        WriteStorage<'s, MutableVoxelWorld<V>>,
        Read<'s, ActiveCamera>,
        ReadStorage<'s, Camera>,
        ReadStorage<'s, Transform>,
    );

    fn run(&mut self, (mut worlds, active_camera, cameras, transforms): Self::SystemData) {
        let identity = Transform::default();

        let transform = active_camera.entity
            .as_ref()
            .and_then(|ac| transforms.get(*ac))
            .or_else(|| (&cameras, &transforms)
                .join()
                .next()
                .map(|(_c, t)| t))
            .unwrap_or(&identity);

        let camera_location = {
            let m = transform.global_matrix().column(3).xyz();
            [m[0], m[1], m[2]]
        };

        for world in (&mut worlds).join() {
            world.load(camera_location, 64.0);
        }
    }
}
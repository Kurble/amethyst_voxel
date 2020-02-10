use amethyst::prelude::*;
use amethyst::{
    assets::{PrefabLoader, PrefabLoaderSystemDesc, RonFormat},
    controls::FlyControlBundle,
    core::transform::{Transform, TransformBundle},
    ecs::prelude::*,
    input::{
        is_key_down, is_mouse_button_down, InputBundle, /*InputHandler,*/ StringBindings,
        VirtualKeyCode,
    },
    renderer::{
        palette::Srgb,
        plugins::{RenderSkybox, RenderToWindow},
        rendy::mesh::{Normal, Position, TexCoord},
        types::DefaultBackend,
        ActiveCamera, Camera, RenderingBundle,
    },
    utils::application_root_dir,
    utils::scene::BasicScenePrefab,
    window::ScreenDimensions,
    winit::MouseButton,
};
use amethyst_voxel::prelude::*;
use nalgebra_glm::*;

type MyPrefabData = BasicScenePrefab<(Vec<Position>, Vec<Normal>, Vec<TexCoord>), f32>;

#[derive(Clone, Default)]
pub struct ExampleVoxel;

impl Data for ExampleVoxel {
    const SUBDIV: usize = 4;
}

struct Example {
    voxels: Option<Entity>,
}

impl Example {
    pub fn new() -> Self {
        Self { voxels: None }
    }
}

impl SimpleState for Example {
    fn on_start(&mut self, data: StateData<GameData>) {
        let prefab_handle = data.world.exec(|loader: PrefabLoader<'_, MyPrefabData>| {
            loader.load("prefab/hello_voxel.ron", RonFormat, ())
        });
        data.world.create_entity().with(prefab_handle).build();

        let model_handle = {
            let loader = &data.world.read_resource::<amethyst::assets::Loader>();
            loader.load(
                "vox/monu9.vox",
                VoxFormat,
                (),
                &data
                    .world
                    .read_resource::<amethyst::assets::AssetStorage<Model>>(),
            )
        };

        let world = VoxelWorld::<ExampleVoxel>::new([12, 8, 12], 16.0);

        let source = ModelSource::new(model_handle);

        self.voxels = Some(
            data.world
                .create_entity()
                .with(world)
                .with(source)
                .with(Transform::default())
                .build(),
        );
    }

    fn update(&mut self, _: &mut StateData<GameData>) -> SimpleTrans {
        Trans::None
    }

    fn handle_event(
        &mut self,
        state: StateData<'_, GameData<'_, '_>>,
        event: StateEvent,
    ) -> SimpleTrans {
        if let StateEvent::Window(event) = event {
            if is_key_down(&event, VirtualKeyCode::Escape) {
                return Trans::Quit;
            } else if is_mouse_button_down(&event, MouseButton::Left) {
                let mut store = state.world.write_storage::<VoxelWorld<ExampleVoxel>>();
                let screen = state.world.read_resource::<ScreenDimensions>();
                let active_camera = state.world.read_resource::<ActiveCamera>();
                let cameras = state.world.read_storage::<Camera>();
                let transforms = state.world.read_storage::<Transform>();

                let (origin, direction) = {
                    let (camera, transform) = active_camera
                        .entity
                        .as_ref()
                        .and_then(|ac| {
                            cameras
                                .get(*ac)
                                .and_then(|c| transforms.get(*ac).map(|t| (c, t)))
                        })
                        .or_else(|| (&cameras, &transforms).join().next())
                        .unwrap();

                    //let mouse = input.mouse_position().map(|(x, y)| [x, y].into()).unwrap();
                    let screen = screen.diagonal();
                    let point = [screen.x * 0.5, screen.y * 0.5, -1.0].into();
                    let point = camera
                        .projection()
                        .screen_to_world_point(point, screen, transform);
                    let origin = transform.global_matrix().column(3).xyz();
                    let direction = vec3(0.0, -1.0, 0.0).normalize();

                    (origin, direction)
                };

                //println!("position: {},{},{}", origin.x, origin.y, origin.z);

                let voxels = store.get_mut(self.voxels.unwrap()).unwrap();

                let ray = voxels.ray(origin, direction);
                if let Some(hit) = voxels.hit(&ray) {
                    //println!("hit a voxel! distance: {}", hit);
                }
                //if let Some(voxel) = voxels.select_mut::<ExampleVoxel>(&ray, 2) {
                //    if let Some(isct) = isct.unwrap().intersection {
                //        println!("found a voxel: {},{},{}", isct.x, isct.y, isct.z);
                //    }
                //    replace(voxel, Voxel::default());
                //}
            }
        }

        Trans::None
    }
}

fn main() -> amethyst::Result<()> {
    amethyst::start_logger(Default::default());

    let app_root = application_root_dir()?;

    let assets_directory = app_root.join("examples/assets/");

    let display_config_path = app_root.join("examples/config/display.ron");

    let key_bindings_path = app_root.join("examples/config/input.ron");

    let game_data = GameDataBuilder::default()
        .with_system_desc(PrefabLoaderSystemDesc::<MyPrefabData>::default(), "", &[])
        .with_bundle(
            FlyControlBundle::<StringBindings>::new(
                Some(String::from("move_x")),
                Some(String::from("move_y")),
                Some(String::from("move_z")),
            )
            .with_sensitivity(0.1, 0.1)
            .with_speed(5.0),
        )?
        .with_bundle(TransformBundle::new().with_dep(&["fly_movement"]))?
        .with_bundle(
            InputBundle::<StringBindings>::new().with_bindings_from_file(&key_bindings_path)?,
        )?
        .with_bundle(
            VoxelBundle::new()
                .with_voxel::<ExampleVoxel>()
                .with_source::<ExampleVoxel, ModelSource>(),
        )?
        .with_bundle(
            RenderingBundle::<DefaultBackend>::new()
                .with_plugin(
                    RenderToWindow::from_config_path(display_config_path)
                        .with_clear([0.0, 0.0, 0.0, 1.0]),
                )
                .with_plugin(RenderVoxelPbr::<ExampleVoxel>::default())
                .with_plugin(RenderSkybox::with_colors(
                    Srgb::new(0.82, 0.51, 0.50),
                    Srgb::new(0.18, 0.11, 0.85),
                )),
        )?;

    let mut game = Application::build(assets_directory, Example::new())?.build(game_data)?;
    game.run();

    Ok(())
}

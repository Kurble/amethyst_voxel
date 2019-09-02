use amethyst::prelude::*;
use amethyst::{
	assets::{PrefabLoader, PrefabLoaderSystem, RonFormat, Handle, Loader},
	core::{
        //shrev::{EventChannel, ReaderId},
        transform::{TransformBundle, Transform},
    },
    controls::{ArcBallControlBundle},
	renderer::{
		palette::{Srgb, Srgba},
	    plugins::{RenderToWindow, RenderShaded3D, RenderSkybox},
	    types::DefaultBackend,
	    rendy::{
            mesh::{Normal, Position, TexCoord},
            texture::palette::load_from_srgba,
        },
	    RenderingBundle,
        Material, MaterialDefaults
	},
	utils::{
		application_root_dir,
	},
	input::{VirtualKeyCode, InputBundle, StringBindings, is_key_down},
	utils::{scene::BasicScenePrefab},
};
use amethyst_voxel::*;
use std::iter::repeat_with;

type MyPrefabData = BasicScenePrefab<(Vec<Position>, Vec<Normal>, Vec<TexCoord>), f32>;

#[derive(Clone)]
pub struct ExampleVoxel;

impl VoxelData for ExampleVoxel {
    const SUBDIV: usize = 4;
}

struct Example;

impl SimpleState for Example {
    fn on_start(&mut self, data: StateData<GameData>) {
        let prefab_handle = data.world.exec(|loader: PrefabLoader<'_, MyPrefabData>| {
            loader.load("prefab/arc_ball_camera.ron", RonFormat, ())
        });
        data.world.create_entity().with(prefab_handle).build();

        let mtl = {
            let world = &data.world;
            let mat_defaults = world.read_resource::<MaterialDefaults>();
            let loader = world.read_resource::<Loader>();

            let textures = &world.read_resource();
            let materials = &world.read_resource();

            let albedo = loader.load_from_data(
                load_from_srgba(Srgba::new(0.1, 0.5, 0.3, 1.0)).into(),
                (),
                textures,
            );
            let mat: Handle<Material> = loader.load_from_data(
                Material {
                    albedo,
                    ..mat_defaults.0.clone()
                },
                (),
                materials,
            );

            mat
        };

        data.world
            .create_entity()
            .with(MutableVoxels::<ExampleVoxel>::from_iter(ExampleVoxel, repeat_with(|| {
                if rand::random() {
                    Simple::Material(1)
                } else {
                    Simple::Empty
                }
            })))
            .with(mtl)
            .with(Transform::default())
            .build();
    }

    fn on_stop(&mut self, _: StateData<GameData>) {
        println!("End!");
    }

    fn update(&mut self, _: &mut StateData<GameData>) -> SimpleTrans {
        Trans::None
    }

    fn handle_event(&mut self, _: StateData<'_, GameData<'_, '_>>, event: StateEvent) -> SimpleTrans {
        if let StateEvent::Window(event) = event {
            if is_key_down(&event, VirtualKeyCode::Escape) {
                Trans::Quit
            } else {
                Trans::None
            }
        } else {
            Trans::None
        }
    }
}

fn main() -> amethyst::Result<()> {
    amethyst::start_logger(Default::default());

    let app_root = application_root_dir()?;

    let assets_directory = app_root.join("examples/assets/");

    let display_config_path = app_root.join("config/display.ron");

    let key_bindings_path = app_root.join("config/input.ron");

    let game_data = GameDataBuilder::default()
    	.with(PrefabLoaderSystem::<MyPrefabData>::default(), "", &[])
    	.with_bundle(TransformBundle::new().with_dep(&[]))?
    	.with_bundle(
            InputBundle::<StringBindings>::new()
                .with_bindings_from_file(&key_bindings_path)?,
        )?
        .with_bundle(ArcBallControlBundle::<StringBindings>::new())?
    	.with_bundle(VoxelBundle::<ExampleVoxel>::new())?
    	.with_bundle(
        	RenderingBundle::<DefaultBackend>::new()
	            .with_plugin(
	                RenderToWindow::from_config_path(display_config_path)
	                    .with_clear([0.0, 0.0, 0.0, 1.0]),
	            )
	            .with_plugin(RenderShaded3D::default())
	            .with_plugin(RenderSkybox::with_colors(
                    Srgb::new(0.82, 0.51, 0.50),
                    Srgb::new(0.18, 0.11, 0.85),
                ))
	            .with_plugin(RenderVoxelPbr::<ExampleVoxel>::default()),
    	)?;

    let mut game = Application::build(assets_directory, Example)?
        .build(game_data)?;
    game.run();

    Ok(())
}
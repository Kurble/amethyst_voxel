use amethyst::prelude::*;
use amethyst::{
	assets::{PrefabLoader, PrefabLoaderSystem, RonFormat},
	core::{
        //shrev::{EventChannel, ReaderId},
        transform::{TransformBundle},
    },
    controls::{ArcBallControlBundle, ArcBallControlTag},
	renderer::{
		palette::Srgb,
	    plugins::{RenderToWindow, RenderShaded3D, RenderSkybox},
	    types::DefaultBackend,
	    rendy::mesh::{Normal, Position, TexCoord},
	    RenderingBundle,
	},
	utils::{
		application_root_dir,
	},
	input::{VirtualKeyCode, InputBundle, StringBindings, is_key_down},
	utils::{scene::BasicScenePrefab},
};
use amethyst_voxel::*;

type MyPrefabData = BasicScenePrefab<(Vec<Position>, Vec<Normal>, Vec<TexCoord>), f32>;

#[derive(Clone)]
pub struct Simple;

impl Metadata for Simple {
    const SUBDIV: usize = 4;
}

pub type Voxel = <Simple as AsVoxel>::Voxel;

struct Example;

impl SimpleState for Example {
    fn on_start(&mut self, data: StateData<GameData>) {
        let prefab_handle = data.world.exec(|loader: PrefabLoader<'_, MyPrefabData>| {
            loader.load("prefab/arc_ball_camera.ron", RonFormat, ())
        });
        data.world.create_entity().with(prefab_handle).build();
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
            InputBundle::<StringBindings>::new().with_bindings_from_file(&key_bindings_path)?,
        )?
        .with_bundle(ArcBallControlBundle::<StringBindings>::new())?
    	.with_bundle(VoxelBundle::<Voxel>::new())?
    	.with_bundle(
        	RenderingBundle::<DefaultBackend>::new()
	            // The RenderToWindow plugin provides all the scaffolding for opening a window and drawing on it
	            .with_plugin(
	                RenderToWindow::from_config_path(display_config_path)
	                    .with_clear([0.0, 0.0, 0.0, 1.0]),
	            )
	            .with_plugin(RenderShaded3D::default())
	            .with_plugin(RenderSkybox::with_colors(
                    Srgb::new(0.82, 0.51, 0.50),
                    Srgb::new(0.18, 0.11, 0.85),
                ))
	            .with_plugin(RenderVoxelPbr::<Voxel>::default()),
    	)?;

    let mut game = Application::new(assets_directory, Example, game_data).expect("Fatal error");
    game.run();

    Ok(())
}
use amethyst::prelude::*;
use amethyst::{
	assets::{PrefabLoader, PrefabLoaderSystem, RonFormat},
	core::{
        //shrev::{EventChannel, ReaderId},
        transform::{TransformBundle, Transform},
    },
    controls::{FlyControlBundle},
	renderer::{
		palette::{Srgb},
	    plugins::{RenderToWindow, RenderSkybox},
	    types::DefaultBackend,
	    rendy::{
            mesh::{Normal, Position, TexCoord},
        },
	    RenderingBundle
	},
	utils::{
		application_root_dir,
	},
	input::{VirtualKeyCode, InputBundle, StringBindings, is_key_down},
	utils::{scene::BasicScenePrefab},
    ecs::Resources,
};
use amethyst_voxel::*;
use rand::Rng;
use std::iter::repeat_with;

type MyPrefabData = BasicScenePrefab<(Vec<Position>, Vec<Normal>, Vec<TexCoord>), f32>;

#[derive(Clone, Default)]
pub struct ExampleVoxel;

impl VoxelData for ExampleVoxel {
    const SUBDIV: usize = 4;
}

struct Example;

/*
struct FlatLoader {
    materials: Vec<VoxelMaterialId>,
}

impl Source<ExampleVoxel> for FlatLoader {
    fn limits(&self) -> Limits {
        Limits { 
            from: [None, Some(0), None], 
            to: [None, Some(0), None],
        }
    }

    fn ready(&self, _: &Resources) -> bool { 
        true
    }

    fn load(&mut self, _: &Resources, _coord: [isize; 3]) -> VoxelFuture<ExampleVoxel> {
        let mut rng = rand::thread_rng();

        let materials_ref = &self.materials;

        let chunk = Nested::Detail {
            data: ExampleVoxel,
            detail: std::sync::Arc::new((0..16)
                .flat_map(|_| (0..16)
                    .flat_map(move |y| (0..16)
                        .map(move |_| {
                            let limit = 5 + rng.gen_range(0, 3);
                            if y < limit || (y == limit && 16u8 > rand::random()) {
                                Simple::Material(materials_ref[rng.gen_range(0, 4)])
                            } else {
                                Simple::Empty
                            }
                        }))).collect()),
        };

        Box::new(futures::future::ok(chunk))
    }
}*/

impl SimpleState for Example {
    fn on_start(&mut self, data: StateData<GameData>) {
        data.world.register::<MutableVoxel<ExampleVoxel>>();
        data.world.register::<MutableVoxelWorld<ExampleVoxel>>();

        let prefab_handle = data.world.exec(|loader: PrefabLoader<'_, MyPrefabData>| {
            loader.load("prefab/hello_voxel.ron", RonFormat, ())
        });
        data.world.create_entity().with(prefab_handle).build();

        let mut rng = rand::thread_rng();

        /*let materials: Vec<_> = {
            let mut materials = data.world.write_resource::<VoxelMaterialStorage>();
            repeat_with(|| materials.create(VoxelMaterial {
                albedo: [128 + rng.gen_range(0, 128), rng.gen(), rng.gen()],
                alpha: 255,
                emission: [0, 0, 0],
                metallic: rng.gen(),
                roughness: rng.gen(),
            })).take(4).collect()
        };

        let loader = Box::new(FlatLoader { materials });*/

        let model_handle = {
            let loader = &data.world.read_resource::<amethyst::assets::Loader>();
            loader.load(
                "vox/monu9.vox",
                VoxFormat,
                (),
                &data.world.read_resource::<amethyst::assets::AssetStorage<VoxelModel>>(),
            )
        };

        let world = MutableVoxelWorld::<ExampleVoxel>::new(Box::new(model_handle), [14, 1, 14], 16.0);

        data.world
            .create_entity()
            .with(world)
            .with(Transform::default())
            .build();
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

    let display_config_path = app_root.join("examples/config/display.ron");

    let key_bindings_path = app_root.join("examples/config/input.ron");

    let game_data = GameDataBuilder::default()
    	.with(PrefabLoaderSystem::<MyPrefabData>::default(), "", &[])
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
            InputBundle::<StringBindings>::new()
                .with_bindings_from_file(&key_bindings_path)?,
        )?
        
    	.with_bundle(VoxelBundle::<ExampleVoxel>::new())?
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

    let mut game = Application::build(assets_directory, Example)?
        .build(game_data)?;
    game.run();

    Ok(())
}
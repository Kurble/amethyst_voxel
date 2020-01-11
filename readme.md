# Amethyst-voxel [![](https://github.com/Kurble/amethyst_voxel/workflows/Clippy/badge.svg)](https://github.com/Kurble/amethyst_voxel/actions)

Amethyst-voxel is a toolbox that delivers voxel functionality and rendering to the [Amethyst game engine](https://amethyst.rs/). Voxels in amethyst-voxel are treated as a resource, which can be rendered and collided with.  Amethyst-voxel tries to be agnostic towards the aesthetics the user desires from their voxel based game.

## Features
The current features of amethyst-voxel are:
- Voxel representation through chunks of user defined dimensions (as long as they are 2^x on each axis);
- A recursive structure, each voxel can contain a chunk of subvoxels;
- PBR rendering pipeline;
- Raycasting on individual voxel objects;
- Voxel model components, a single voxel chunk is used a normal 3D mesh;
- Voxel world components, voxel chunks are dynamically loaded in a "3D viewport";
- Support for MagicaVoxel `.vox` files;
- Simple movement of entities through voxel worlds, not physics based.

## Development status
Amethyst-voxel is still in development, and it's final goals have not quite been defined yet. If you want to discuss the project join us on the [amethyst-voxel discord server](https://discord.gg/ZJcan7E)!

## Documentation
To get started check out any of the following documentation resources:
- The examples directory contains examples that show you how to use amethyst-voxel.
- Follow one of the guides (that are still TODO).
- The [docs.rs](docs.rs) documentation that serves as a technical reference.

## Setup
Amethyst-voxel requires the amethyst game engine to run, which is the biggest part of setting it up. Check out the [Amethyst game engine](https://amethyst.rs/) for more information on setting it up. After setting the amethyst game engine up for use, setting up amethyst-voxel is as simple as adding the dependency to your `Cargo.toml` file:
```toml
amethyst_voxel = "0.1"
```

(todo: publish to crates.io...)


## Contributing

Contributions are welcome! Feel free to submit pull requests.

Pull requests that fix bugs or improve documentation are likely to be quickly reviewed, while pull
requests that add features or change the API may be more controversial and take more time.

If your change adds, removes or modifies a trait or a function, please add an entry to the
`changelog.md` file as part of your pull request.

## License

Amethyst-voxel is free and open source software distributed under the terms of both the MIT License and the Apache License 2.0.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

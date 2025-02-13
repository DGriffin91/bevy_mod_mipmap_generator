//! Loads and renders a glTF file as a scene.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_mod_mipmap_generator::{
    generate_mipmaps, MipmapGeneratorDebugTextPlugin, MipmapGeneratorPlugin,
    MipmapGeneratorSettings,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let use_compression = args.contains(&"--compress".to_string());
    let use_cache = args.contains(&"--cache".to_string());
    let low_quality = args.contains(&"--low-quality".to_string());

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.1)))
        .add_plugins(DefaultPlugins)
        .insert_resource(MipmapGeneratorSettings {
            // Manually setting anisotropic filtering to 16x
            anisotropic_filtering: 16,
            compression: Option::from(use_compression.then(Default::default)),
            compressed_image_data_cache_path: if use_cache {
                Some(PathBuf::from("compressed_texture_cache"))
            } else {
                None
            },
            low_quality,
            ..default()
        })
        .add_systems(Startup, setup)
        // Add MipmapGeneratorPlugin after default plugins
        .add_plugins((MipmapGeneratorPlugin, MipmapGeneratorDebugTextPlugin))
        // Add material types to be converted
        .add_systems(Update, generate_mipmaps::<StandardMaterial>)
        //.add_system(animate_light_direction)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(1.0, 0.2, 1.0).looking_at(Vec3::new(0.0, 0.3, 0.0), Vec3::Y),
    ));

    // light
    commands.spawn((
        PointLight {
            intensity: 2000.0 * 1000.0,
            shadows_enabled: true,
            range: 10.0,
            ..default()
        },
        Transform::from_xyz(-1.0, 2.0, -3.0),
    ));
    commands.spawn(SceneRoot(
        asset_server.load("models/FlightHelmet/FlightHelmet.gltf#Scene0")
    ));
    commands.spawn(SceneRoot(
        asset_server.load("models/Wood/wood.gltf#Scene0")
    ));
}

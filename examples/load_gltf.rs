//! Loads and renders a glTF file as a scene.

use std::num::NonZeroU8;

use bevy::prelude::*;
use bevy_mod_mipmap_generator::{generate_mipmaps, MipmapGeneratorPlugin, MipmapGeneratorSettings};

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.1, 0.1, 0.1)))
        .add_plugins(DefaultPlugins)
        // Manually setting anisotropic filtering to 16x
        .insert_resource(MipmapGeneratorSettings {
            anisotropic_filtering: NonZeroU8::new(16),
            ..default()
        })
        // Add MipmapGeneratorPlugin after default plugins
        .add_plugin(MipmapGeneratorPlugin)
        // Add material types to be converted
        .add_system(generate_mipmaps::<StandardMaterial>)
        .add_startup_system(setup)
        //.add_system(animate_light_direction)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(1.0, 0.2, 1.0).looking_at(Vec3::new(0.0, 0.3, 0.0), Vec3::Y),
        ..default()
    });

    // light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 2000.0,
            shadows_enabled: true,
            range: 10.0,
            ..default()
        },
        transform: Transform::from_xyz(-1.0, 2.0, -3.0),
        ..default()
    });
    commands.spawn(SceneBundle {
        scene: asset_server.load("models/FlightHelmet/FlightHelmet.gltf#Scene0"),
        ..default()
    });
    commands.spawn(SceneBundle {
        scene: asset_server.load("models/Wood/wood.gltf#Scene0"),
        ..default()
    });
}

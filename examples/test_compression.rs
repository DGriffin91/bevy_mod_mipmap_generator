use std::{f32::consts::PI, path::PathBuf};

use bevy::{
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    },
};
use bevy_mod_mipmap_generator::{generate_mipmaps, MipmapGeneratorPlugin, MipmapGeneratorSettings};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let use_cache = args.contains(&"--cache".to_string());

    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .insert_resource(MipmapGeneratorSettings {
            compression: Some(Default::default()),
            compressed_image_data_cache_path: if use_cache {
                Some(PathBuf::from("compressed_texture_cache"))
            } else {
                None
            },
            ..default()
        })
        .add_systems(Startup, setup)
        // Add MipmapGeneratorPlugin after default plugins
        .add_plugins(MipmapGeneratorPlugin)
        // Add material types to be converted
        .add_systems(Update, generate_mipmaps::<StandardMaterial>);

    app.run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let image_r = create_test_image(2048, -0.8, 0.156, 1);
    let mut mat_r = StandardMaterial::from(images.add(image_r));
    mat_r.unlit = true;

    let image_rg = create_test_image(2048, -0.8, 0.156, 2);
    let mut mat_rg = StandardMaterial::from(images.add(image_rg));
    mat_rg.unlit = true;

    let image_rgba = create_test_image(2048, -0.8, 0.156, 4);
    let mut mat_rgba = StandardMaterial::from(images.add(image_rgba));
    mat_rgba.unlit = true;

    let plane_h = meshes.add(Plane3d::default().mesh().size(20.0, 30.0));

    // planes
    commands.spawn(PbrBundle {
        mesh: plane_h.clone(),
        material: materials.add(mat_r),
        transform: Transform::from_xyz(-3.0, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_z(-PI * 0.5)),
        ..default()
    });
    commands.spawn(PbrBundle {
        mesh: plane_h.clone(),
        material: materials.add(mat_rg),
        transform: Transform::from_xyz(3.0, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_z(PI * 0.5)),
        ..default()
    });
    commands.spawn(PbrBundle {
        mesh: plane_h.clone(),
        material: materials.add(mat_rgba),
        transform: Transform::from_xyz(0.0, -3.0, 0.0),
        ..default()
    });

    // camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(0.0, 0.0, 18.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
}

fn create_test_image(size: u32, cx: f32, cy: f32, channels: u32) -> Image {
    let data: Vec<u8> = (0..size * size)
        .flat_map(|id| {
            let mut x = 4.0 * (id % size) as f32 / (size - 1) as f32 - 2.0;
            let mut y = 2.0 * (id / size) as f32 / (size - 1) as f32 - 1.0;
            let mut count = 0;
            while count < 0xFF && x * x + y * y < 4.0 {
                let old_x = x;
                x = x * x - y * y + cx;
                y = 2.0 * old_x * y + cy;
                count += 1;
            }
            let mut values = vec![0xFF - (count * 2) as u8];
            if channels > 1 {
                values.push(0xFF - (count * 5) as u8);
            }
            if channels > 2 {
                values.push(0xFF - (count * 13) as u8);
                values.push(std::u8::MAX);
            }
            values
        })
        .collect();

    Image {
        texture_descriptor: TextureDescriptor {
            label: None,
            size: Extent3d {
                width: size,
                height: size,
                ..default()
            },
            dimension: TextureDimension::D2,
            format: if channels == 1 {
                TextureFormat::R8Unorm
            } else if channels == 2 {
                TextureFormat::Rg8Unorm
            } else {
                TextureFormat::Rgba8UnormSrgb
            },
            mip_level_count: 1,
            sample_count: 1,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        data,
        ..Default::default()
    }
}

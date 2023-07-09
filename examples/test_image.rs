use bevy::{
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    },
};
use bevy_mod_mipmap_generator::{generate_mipmaps, MipmapGeneratorPlugin};

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
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
    let image = create_test_image(4096, -0.8, 0.156);

    // plane
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Plane {
            size: 20.0,
            ..default()
        })),
        material: materials.add(StandardMaterial::from(images.add(image))),
        ..default()
    });
    // light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });
    // camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(0.0, 0.5, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
}

fn create_test_image(size: u32, cx: f32, cy: f32) -> Image {
    use std::iter;

    let data = (0..size * size)
        .flat_map(|id| {
            // get high five for recognizing this ;)
            let mut x = 4.0 * (id % size) as f32 / (size - 1) as f32 - 2.0;
            let mut y = 2.0 * (id / size) as f32 / (size - 1) as f32 - 1.0;
            let mut count = 0;
            while count < 0xFF && x * x + y * y < 4.0 {
                let old_x = x;
                x = x * x - y * y + cx;
                y = 2.0 * old_x * y + cy;
                count += 1;
            }
            iter::once(0xFF - (count * 2) as u8)
                .chain(iter::once(0xFF - (count * 5) as u8))
                .chain(iter::once(0xFF - (count * 13) as u8))
                .chain(iter::once(std::u8::MAX))
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
            format: TextureFormat::Rgba8UnormSrgb,
            mip_level_count: 1,
            sample_count: 1,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[TextureFormat::Rgba8UnormSrgb],
        },
        data,
        ..Default::default()
    }
}

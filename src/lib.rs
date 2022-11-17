use anyhow::anyhow;
use std::num::NonZeroU8;

use bevy::{
    prelude::*,
    render::{
        render_resource::{SamplerDescriptor, TextureFormat},
        texture::ImageSampler,
    },
};
use image::{imageops::FilterType, DynamicImage, ImageBuffer};

#[derive(Resource, Deref)]
pub struct DefaultSampler(SamplerDescriptor<'static>);

#[derive(Resource)]
pub struct MipmapGeneratorSettings {
    /// Valid values: 1, 2, 4, 8, and 16.
    pub anisotropic_filtering: Option<NonZeroU8>,
    pub filter_type: FilterType,
    pub minimum_mip_resolution: u32,
}

///Mipmaps will not be generated for materials found on entities that also have the `NoMipmapGeneration` component.
#[derive(Component)]
pub struct NoMipmapGeneration;

impl Default for MipmapGeneratorSettings {
    fn default() -> Self {
        Self {
            // Default to 8x anisotropic filtering
            anisotropic_filtering: NonZeroU8::new(8),
            filter_type: FilterType::Triangle,
            minimum_mip_resolution: 2,
        }
    }
}

pub struct MipmapGeneratorPlugin;
impl Plugin for MipmapGeneratorPlugin {
    fn build(&self, app: &mut App) {
        if let Some(image_plugin) = app.get_added_plugins::<ImagePlugin>().first() {
            let default_sampler = image_plugin.default_sampler.clone();
            app.insert_resource(DefaultSampler(default_sampler))
                .init_resource::<MipmapGeneratorSettings>();
        } else {
            warn!("No ImagePlugin found. Try adding MipmapGeneratorPlugin after DefaultPlugins");
        }
    }
}

pub fn generate_mipmaps<M: Material + GetImages>(
    mut material_events: EventReader<AssetEvent<M>>,
    mut materials: ResMut<Assets<M>>,
    no_mipmap: Query<&Handle<M>, With<NoMipmapGeneration>>,
    mut images: ResMut<Assets<Image>>,
    default_sampler: Res<DefaultSampler>,
    settings: Res<MipmapGeneratorSettings>,
) {
    'outer: for event in material_events.iter() {
        let handle = match event {
            AssetEvent::Created { handle } => handle,
            _ => continue,
        };
        for m in no_mipmap.iter() {
            if m == handle {
                continue 'outer;
            }
        }
        if let Some(material) = materials.get_mut(handle) {
            for image_h in material.get_images().into_iter().flatten() {
                if let Some(image) = images.get_mut(image_h) {
                    if image.texture_descriptor.mip_level_count == 1 {
                        generate_mips_texture(image, &settings, &default_sampler);
                    }
                }
            }
        }
    }
}

fn generate_mips_texture(
    image: &mut Image,
    settings: &MipmapGeneratorSettings,
    default_sampler: &DefaultSampler,
) {
    match try_into_dynamic(image.clone()) {
        Ok(mut dyn_image) => {
            let (mip_level_count, image_data) = generate_mips(
                &mut dyn_image,
                settings.minimum_mip_resolution,
                u32::MAX,
                settings.filter_type,
            );
            image.texture_descriptor.mip_level_count = mip_level_count;
            image.data = image_data;
            let mut descriptor = match image.sampler_descriptor.clone() {
                ImageSampler::Default => (*default_sampler).clone(),
                ImageSampler::Descriptor(descriptor) => descriptor,
            };
            descriptor.anisotropy_clamp = settings.anisotropic_filtering;
            image.sampler_descriptor = ImageSampler::Descriptor(descriptor);
        }
        Err(e) => warn!("{}", e),
    }
}

/// Returns the number of mip levels, and a vec of bytes containing the image data.
/// The `max_mip_count` includes the first input mip level. So setting this to 2 will
/// result in a single additional mip level being generated, for a total of 2 levels.
fn generate_mips(
    dyn_image: &mut DynamicImage,
    minimum_mip_resolution: u32,
    max_mip_count: u32,
    filter_type: FilterType,
) -> (u32, Vec<u8>) {
    let mut image_data = dyn_image.as_bytes().to_vec();
    let mut mip_level_count = 1;
    let mut width = dyn_image.width();
    let mut height = dyn_image.height();

    while width / 2 >= minimum_mip_resolution.max(1)
        && height / 2 >= minimum_mip_resolution.max(1)
        && mip_level_count < max_mip_count
    {
        width /= 2;
        height /= 2;
        *dyn_image = dyn_image.resize_exact(width, height, filter_type);
        image_data.append(&mut dyn_image.as_bytes().to_vec());
        mip_level_count += 1;
    }

    (mip_level_count, image_data)
}

// Implement the GetImages trait for any materials that need conversion
pub trait GetImages {
    fn get_images(&self) -> Vec<&Option<Handle<Image>>>;
}

impl GetImages for StandardMaterial {
    fn get_images(&self) -> Vec<&Option<Handle<Image>>> {
        vec![
            &self.base_color_texture,
            &self.emissive_texture,
            &self.metallic_roughness_texture,
            &self.normal_map_texture,
            &self.occlusion_texture,
        ]
    }
}

pub fn try_into_dynamic(image: Image) -> anyhow::Result<DynamicImage> {
    match image.texture_descriptor.format {
        TextureFormat::R8Unorm => ImageBuffer::from_raw(
            image.texture_descriptor.size.width,
            image.texture_descriptor.size.height,
            image.data,
        )
        .map(DynamicImage::ImageLuma8),
        TextureFormat::Rg8Unorm => ImageBuffer::from_raw(
            image.texture_descriptor.size.width,
            image.texture_descriptor.size.height,
            image.data,
        )
        .map(DynamicImage::ImageLumaA8),
        TextureFormat::Rgba8UnormSrgb => ImageBuffer::from_raw(
            image.texture_descriptor.size.width,
            image.texture_descriptor.size.height,
            image.data,
        )
        .map(DynamicImage::ImageRgba8),
        TextureFormat::Rgba8Unorm => ImageBuffer::from_raw(
            image.texture_descriptor.size.width,
            image.texture_descriptor.size.height,
            image.data,
        )
        .map(DynamicImage::ImageRgba8),
        // Throw and error if conversion isn't supported
        texture_format => {
            return Err(anyhow!(
                "Conversion into dynamic image not supported for {:?}.",
                texture_format
            ))
        }
    }
    .ok_or_else(|| {
        anyhow!(
            "Failed to convert into {:?}.",
            image.texture_descriptor.format
        )
    })
}

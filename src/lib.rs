#[cfg(feature = "compress")]
use std::{
    fs::{self, File},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Write},
    path::Path,
};

use anyhow::anyhow;

use bevy::{
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        texture::{ImageSampler, ImageSamplerDescriptor},
    },
    tasks::{AsyncComputeTaskPool, Task},
    utils::HashMap,
};
use futures_lite::future;
use image::{imageops::FilterType, DynamicImage, ImageBuffer};

#[derive(Resource, Deref)]
pub struct DefaultSampler(ImageSamplerDescriptor);

#[derive(Resource, Clone)]
pub struct MipmapGeneratorSettings {
    /// Valid values: 1, 2, 4, 8, and 16.
    pub anisotropic_filtering: u16,
    pub filter_type: FilterType,
    pub minimum_mip_resolution: u32,
    /// Set to Some(CompressionSpeed) to enable compression.
    /// The compress feature also needs to be enabled. Only BCn currently supported.
    /// Compression can take a long time, CompressionSpeed::UltraFast (default) is recommended.
    /// Currently supported conversions:
    ///- R8Unorm -> Bc4RUnorm
    ///- Rg8Unorm -> Bc5RgUnorm
    ///- Rgba8Unorm -> Bc7RgbaUnorm
    ///- Rgba8UnormSrgb -> Bc7RgbaUnormSrgb
    pub compression: Option<CompressionSpeed>,
    /// If set, raw compressed image data will be cached in this directory.
    /// Images that are not BCn compressed are not cached.
    pub compressed_image_data_cache_path: Option<std::path::PathBuf>,
}

impl Default for MipmapGeneratorSettings {
    fn default() -> Self {
        Self {
            // Default to 8x anisotropic filtering
            anisotropic_filtering: 8,
            filter_type: FilterType::Triangle,
            minimum_mip_resolution: 1,
            compression: None,
            compressed_image_data_cache_path: None,
        }
    }
}

#[derive(Default, Clone, Copy)]
pub enum CompressionSpeed {
    #[default]
    UltraFast,
    VeryFast,
    Fast,
    Medium,
    Slow,
}

impl CompressionSpeed {
    #[cfg(feature = "compress")]
    fn get_bc7_encoder(&self) -> intel_tex_2::bc7::EncodeSettings {
        // Bevy doesn't differentiate Images with alpha vs not currently.
        match self {
            CompressionSpeed::UltraFast => intel_tex_2::bc7::alpha_ultra_fast_settings(),
            CompressionSpeed::VeryFast => intel_tex_2::bc7::alpha_very_fast_settings(),
            CompressionSpeed::Fast => intel_tex_2::bc7::alpha_fast_settings(),
            CompressionSpeed::Medium => intel_tex_2::bc7::alpha_basic_settings(),
            CompressionSpeed::Slow => intel_tex_2::bc7::alpha_slow_settings(),
        }
    }
}

///Mipmaps will not be generated for materials found on entities that also have the `NoMipmapGeneration` component.
#[derive(Component)]
pub struct NoMipmapGeneration;

/// Tracks the amount of bytes that have been cached since startup.
/// Used to warn at 1GB increments to avoid continuously caching images that change every frame.
#[derive(Resource, Default)]
pub struct CachedDataSize(pub usize);

pub struct MipmapGeneratorPlugin;
impl Plugin for MipmapGeneratorPlugin {
    fn build(&self, app: &mut App) {
        if let Some(image_plugin) = app
            .init_resource::<CachedDataSize>()
            .get_added_plugins::<ImagePlugin>()
            .first()
        {
            let default_sampler = image_plugin.default_sampler.clone();
            app.insert_resource(DefaultSampler(default_sampler))
                .init_resource::<MipmapGeneratorSettings>();
        } else {
            warn!("No ImagePlugin found. Try adding MipmapGeneratorPlugin after DefaultPlugins");
        }
    }
}

pub struct TaskData {
    added_cache_size: usize,
    image: Image,
}

#[derive(Resource, Default, Deref, DerefMut)]
#[allow(clippy::type_complexity)]
pub struct MipmapTasks<M: Material + GetImages>(
    HashMap<Handle<Image>, (Task<TaskData>, Vec<Handle<M>>)>,
);

#[allow(clippy::too_many_arguments)]
pub fn generate_mipmaps<M: Material + GetImages>(
    mut commands: Commands,
    mut material_events: EventReader<AssetEvent<M>>,
    mut materials: ResMut<Assets<M>>,
    no_mipmap: Query<&Handle<M>, With<NoMipmapGeneration>>,
    mut images: ResMut<Assets<Image>>,
    default_sampler: Res<DefaultSampler>,
    mut cached_data_size: ResMut<CachedDataSize>,
    settings: Res<MipmapGeneratorSettings>,
    mut tasks_res: Option<ResMut<MipmapTasks<M>>>,
) {
    let mut new_tasks = MipmapTasks(HashMap::new());

    let tasks = if let Some(ref mut tasks) = tasks_res {
        tasks
    } else {
        &mut new_tasks
    };

    let thread_pool = AsyncComputeTaskPool::get();
    'outer: for event in material_events.read() {
        let material_h = match event {
            AssetEvent::Added { id } => id,
            AssetEvent::LoadedWithDependencies { id } => id,
            _ => continue,
        };
        for m in no_mipmap.iter() {
            if m.id() == *material_h {
                continue 'outer;
            }
        }
        // get_mut(material_h) here so we see the filtering right away
        // and even if mipmaps aren't made, we still get the filtering
        if let Some(material) = materials.get_mut(*material_h) {
            for image_h in material.get_images().into_iter() {
                if let Some((_, material_handles)) = tasks.get_mut(image_h) {
                    material_handles.push(Handle::Weak(*material_h));
                    continue; //There is already a task for this image
                }
                if let Some(image) = images.get_mut(image_h) {
                    let mut descriptor = match image.sampler.clone() {
                        ImageSampler::Default => default_sampler.0.clone(),
                        ImageSampler::Descriptor(descriptor) => descriptor,
                    };
                    descriptor.anisotropy_clamp = settings.anisotropic_filtering;
                    image.sampler = ImageSampler::Descriptor(descriptor);
                    if image.texture_descriptor.mip_level_count == 1
                        && check_image_compatible(image).is_ok()
                    {
                        let mut image = image.clone();
                        let settings = settings.clone();
                        let mut added_cache_size = 0;
                        let task = thread_pool.spawn(async move {
                            match generate_mips_texture(
                                &mut image,
                                &settings.clone(),
                                &mut added_cache_size,
                            ) {
                                Ok(_) => (),
                                Err(e) => warn!("{}", e),
                            }
                            TaskData {
                                added_cache_size,
                                image,
                            }
                        });
                        tasks.insert(image_h.clone(), (task, vec![Handle::Weak(*material_h)]));
                    }
                }
            }
        }
    }

    fn bytes_to_gb(bytes: usize) -> usize {
        bytes / 1024_usize.pow(3)
    }

    tasks.retain(|image_h, (task, material_handles)| {
        match future::block_on(future::poll_once(task)) {
            Some(task_data) => {
                if let Some(image) = images.get_mut(image_h) {
                    *image = task_data.image;
                    let prev_cached_data_gb = bytes_to_gb(cached_data_size.0);
                    cached_data_size.0 += task_data.added_cache_size;
                    let current_cached_data_gb = bytes_to_gb(cached_data_size.0);
                    if current_cached_data_gb > prev_cached_data_gb {
                        warn!(
                            "Generated cached texture data from just this run exceeds {}GB",
                            current_cached_data_gb
                        );
                    }
                    // Touch material to trigger change detection
                    for material_h in material_handles.iter() {
                        let _ = materials.get_mut(material_h);
                    }
                }
                false
            }
            None => true,
        }
    });

    if tasks_res.is_none() {
        commands.insert_resource(new_tasks);
    }
}

/// `added_cache_size` is for tracking the amount of data that was cached by this call.
/// Compressed BCn data is cached on disk if cache_compressed_image_data is enabled.
pub fn generate_mips_texture(
    image: &mut Image,
    settings: &MipmapGeneratorSettings,
    #[allow(unused)] added_cache_size: &mut usize,
) -> anyhow::Result<()> {
    check_image_compatible(image)?;
    match try_into_dynamic(image.clone()) {
        Ok(mut dyn_image) => {
            #[cfg(feature = "compress")]
            let mut compressed_format = None;
            #[allow(unused_mut)]
            let mut compression_settings = settings.compression;
            #[cfg(feature = "compress")]
            {
                if let Some(encoder_setting) = settings.compression {
                    compressed_format = bcn_equivalent_format_of_dyn_image(
                        &dyn_image,
                        image.texture_descriptor.format.is_srgb(),
                    )
                    .ok();
                    compression_settings = compressed_format.map(|_| encoder_setting);
                }
            }

            #[cfg(feature = "compress")]
            let mut input_hash = u64::MAX;
            #[allow(unused_mut)]
            let mut loaded_from_cache = false;
            let mut new_image_data = Vec::new();

            #[cfg(feature = "compress")]
            if compression_settings.is_some() && compressed_format.is_some() {
                if let Some(cache_path) = &settings.compressed_image_data_cache_path {
                    input_hash = calculate_hash(&image);
                    if let Some(compressed_image_data) = load_from_cache(input_hash, &cache_path) {
                        new_image_data = compressed_image_data;
                        loaded_from_cache = true;
                    }
                }
            }

            let mip_count = calculate_mip_count(
                dyn_image.width(),
                dyn_image.height(),
                settings.minimum_mip_resolution,
                u32::MAX,
                compression_settings,
            );

            if !loaded_from_cache {
                new_image_data = generate_mips(
                    &mut dyn_image,
                    mip_count,
                    settings.filter_type,
                    compression_settings,
                );
                #[cfg(feature = "compress")]
                if let Some(cache_path) = &settings.compressed_image_data_cache_path {
                    if compression_settings.is_some() && compressed_format.is_some() {
                        *added_cache_size += new_image_data.len();
                        save_to_cache(input_hash, &new_image_data, &cache_path).unwrap();
                    }
                }
            }

            image.texture_descriptor.mip_level_count = mip_count;
            #[cfg(feature = "compress")]
            if let Some(format) = compressed_format {
                image.texture_descriptor.format = format;
                // Remove view formats for compressed textures.
                // TODO Is this an issue? A bit difficult to work around since it's &['static]
                image.texture_descriptor.view_formats = &[];
            }

            image.data = new_image_data;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Returns a vec of bytes containing the image data for all generated mips.
/// Use `calculate_mip_count()` to find the value for `mip_count`.
pub fn generate_mips(
    dyn_image: &mut DynamicImage,
    mip_count: u32,
    filter_type: FilterType,
    compression: Option<CompressionSpeed>,
) -> Vec<u8> {
    let mut width = dyn_image.width();
    let mut height = dyn_image.height();

    #[allow(unused_mut)]
    let mut compressed_image_data = None;
    #[cfg(feature = "compress")]
    if let Some(compression_settings) = compression {
        compressed_image_data = bcn_compress_dyn_image(compression_settings, dyn_image).ok();
    }

    #[cfg(not(feature = "compress"))]
    if compression.is_some() {
        warn!("Compression is Some but compress feature is disabled. Falling back to generating mips without compression.")
    }

    let mut image_data = compressed_image_data.unwrap_or(dyn_image.as_bytes().to_vec());

    #[cfg(feature = "compress")]
    let min = if compression.is_some() { 4 } else { 1 };
    #[cfg(not(feature = "compress"))]
    let min = 1;

    for _ in 0..mip_count {
        width /= 2;
        height /= 2;
        *dyn_image = dyn_image.resize_exact(width, height, filter_type);

        #[allow(unused_mut)]
        let mut compressed_image_data = None;
        #[cfg(feature = "compress")]
        if let Some(compression_settings) = compression {
            compressed_image_data = bcn_compress_dyn_image(compression_settings, dyn_image).ok();
        }
        image_data.append(&mut compressed_image_data.unwrap_or(dyn_image.as_bytes().to_vec()));
        if width <= min || height <= min {
            break;
        }
    }

    image_data
}

/// Returns the number of mip levels
/// The `max_mip_count` includes the first input mip level. So setting this to 2 will
/// result in a single additional mip level being generated, for a total of 2 levels.
pub fn calculate_mip_count(
    mut width: u32,
    mut height: u32,
    minimum_mip_resolution: u32,
    max_mip_count: u32,
    #[allow(unused)] compression: Option<CompressionSpeed>,
) -> u32 {
    let mut mip_level_count = 1;

    #[cfg(feature = "compress")]
    let min = if compression.is_some() { 4 } else { 1 };
    #[cfg(not(feature = "compress"))]
    let min = 1;

    // Use log to avoid loop? Are there edge cases with rounding?

    while width / 2 >= minimum_mip_resolution.max(min)
        && height / 2 >= minimum_mip_resolution.max(min)
        && mip_level_count < max_mip_count
    {
        width /= 2;
        height /= 2;
        mip_level_count += 1;
    }

    mip_level_count
}

/// Extract a specific individual mip level as a new image.
pub fn extract_mip_level(image: &Image, mip_level: u32) -> anyhow::Result<Image> {
    check_image_compatible(image)?;

    let descriptor = &image.texture_descriptor;

    if descriptor.mip_level_count < mip_level {
        return Err(anyhow!(
            "Mip level {mip_level} requested, but only {} are avaliable.",
            descriptor.mip_level_count
        ));
    }

    let block_size = descriptor.format.block_copy_size(None).unwrap() as usize;

    //let mip_factor = 2u32.pow(mip_level - 1);
    //let final_width = descriptor.size.width/mip_factor;
    //let final_height = descriptor.size.height/mip_factor;

    let mut width = descriptor.size.width as usize;
    let mut height = descriptor.size.height as usize;

    let mut byte_offset = 0usize;

    for _ in 0..mip_level - 1 {
        byte_offset += width * block_size * height;
        width /= 2;
        height /= 2;
    }

    let mut new_descriptor = descriptor.clone();

    new_descriptor.mip_level_count = 1;
    new_descriptor.size = Extent3d {
        width: width as u32,
        height: height as u32,
        depth_or_array_layers: 1,
    };

    Ok(Image {
        data: image.data[byte_offset..byte_offset + (width * block_size * height)].to_vec(),
        texture_descriptor: new_descriptor,
        sampler: image.sampler.clone(),
        texture_view_descriptor: image.texture_view_descriptor.clone(),
        asset_usage: RenderAssetUsages::default(),
    })
}

pub fn check_image_compatible(image: &Image) -> anyhow::Result<()> {
    if image.is_compressed() {
        return Err(anyhow!("Compressed images not supported"));
    }
    let descriptor = &image.texture_descriptor;

    if descriptor.dimension != TextureDimension::D2 {
        return Err(anyhow!(
            "Image has dimension {:?} but only TextureDimension::D2 is supported.",
            descriptor.dimension
        ));
    }

    if descriptor.size.depth_or_array_layers != 1 {
        return Err(anyhow!(
            "Image contains {} layers only a single layer is supported.",
            descriptor.size.depth_or_array_layers
        ));
    }

    Ok(())
}

// Implement the GetImages trait for any materials that need conversion
pub trait GetImages {
    fn get_images(&self) -> Vec<&Handle<Image>>;
}

impl GetImages for StandardMaterial {
    fn get_images(&self) -> Vec<&Handle<Image>> {
        vec![
            &self.base_color_texture,
            &self.emissive_texture,
            &self.metallic_roughness_texture,
            &self.normal_map_texture,
            &self.occlusion_texture,
        ]
        .into_iter()
        .flatten()
        .collect()
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

#[cfg(feature = "compress")]
fn bcn_compress_dyn_image(
    compression_speed: CompressionSpeed,
    dyn_image: &mut DynamicImage,
) -> anyhow::Result<Vec<u8>> {
    let width = dyn_image.width();
    let height = dyn_image.height();
    let mut image_data;
    match dyn_image {
        DynamicImage::ImageLuma8(data) => {
            image_data = vec![0u8; intel_tex_2::bc4::calc_output_size(width, height)];
            let surface = intel_tex_2::RSurface {
                width,
                height,
                stride: width,
                data,
            };
            intel_tex_2::bc4::compress_blocks_into(&surface, &mut image_data);
        }
        DynamicImage::ImageLumaA8(data) => {
            image_data = vec![0u8; intel_tex_2::bc5::calc_output_size(width, height)];
            let surface = intel_tex_2::RgSurface {
                width,
                height,
                stride: width * 2,
                data,
            };
            intel_tex_2::bc5::compress_blocks_into(&surface, &mut image_data);
        }
        DynamicImage::ImageRgba8(data) => {
            image_data = vec![0u8; intel_tex_2::bc7::calc_output_size(width, height)];
            let surface = intel_tex_2::RgbaSurface {
                width,
                height,
                stride: width * 4,
                data,
            };
            intel_tex_2::bc7::compress_blocks_into(
                &compression_speed.get_bc7_encoder(),
                &surface,
                &mut image_data,
            );
        }
        // Throw and error if conversion isn't supported
        dyn_image => {
            return Err(anyhow!(
                "Conversion into dynamic image not supported for {:?}.",
                dyn_image
            ))
        }
    };
    Ok(image_data)
}

pub fn bcn_equivalent_format_of_dyn_image(
    dyn_image: &DynamicImage,
    is_srgb: bool,
) -> anyhow::Result<TextureFormat> {
    if dyn_image.width() < 4 || dyn_image.height() < 4 {
        return Err(anyhow!("Image size too small for BCn compression"));
    }
    match dyn_image {
        DynamicImage::ImageLuma8(_) => Ok(TextureFormat::Bc4RUnorm),
        DynamicImage::ImageLumaA8(_) => Ok(TextureFormat::Bc5RgUnorm),
        DynamicImage::ImageRgba8(_) => Ok(if is_srgb {
            TextureFormat::Bc7RgbaUnormSrgb
        } else {
            TextureFormat::Bc7RgbaUnorm
        }),
        // Throw and error if conversion isn't supported
        dyn_image => Err(anyhow!(
            "Conversion into dynamic image not supported for {:?}.",
            dyn_image
        )),
    }
}

/// Calculate the hash for the non-compressed non-mipmapped image.
#[cfg(feature = "compress")]
fn calculate_hash(image: &Image) -> u64 {
    let mut hasher = DefaultHasher::new();
    image.data.hash(&mut hasher);
    image.texture_descriptor.hash(&mut hasher);
    hasher.finish()
}

/// Save raw image bytes to disk cache
#[cfg(feature = "compress")]
fn save_to_cache(hash: u64, bytes: &[u8], cache_dir: &Path) -> std::io::Result<()> {
    if !cache_dir.exists() {
        fs::create_dir(cache_dir)?;
    }
    let file_path = cache_dir.join(format!("{:x}", hash));
    let mut file = File::create(file_path)?;
    file.write_all(&zstd::encode_all(bytes, 0).unwrap())?;
    Ok(())
}

/// Load from disk cache for matching input hash
#[cfg(feature = "compress")]
fn load_from_cache(hash: u64, cache_dir: &Path) -> Option<Vec<u8>> {
    let file_path = cache_dir.join(format!("{:x}", hash));
    if !file_path.exists() {
        return None;
    }
    let Ok(mut file) = File::open(file_path) else {
        return None;
    };
    let mut cached_bytes = Vec::new();
    if !file.read_to_end(&mut cached_bytes).is_ok() {
        return None;
    };
    zstd::decode_all(cached_bytes.as_slice()).ok()
}

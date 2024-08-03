# bevy_mod_mipmap_generator

## A basic mipmap generator for bevy 0.14.

Optionally use the `compress` feature and corresponding setting in `MipmapGeneratorSettings` to enable BCn compression. Note: Compression can take a long time depending on the quantity and resolution of the images.

Currently supported conversions:
- R8Unorm -> Bc4RUnorm
- Rg8Unorm -> Bc5RgUnorm
- Rgba8Unorm -> Bc7RgbaUnorm
- Rgba8UnormSrgb -> Bc7RgbaUnormSrgb

Optionally set `compressed_image_data_cache_path` in `MipmapGeneratorSettings` to cache raw compressed image data on disk. Only textures that are BCn compressed will be stored.

![example_without](example_without.jpg)
*without mipmaps*

![example_with](example_with.jpg)
*with mipmaps*

## Note

Bevy supports a [variety of compressed image formats](https://docs.rs/bevy/latest/bevy/render/texture/enum.ImageFormat.html) that can also contain mipmaps. This plugin is intended for situations where the use of those formats is impractical (mostly prototyping/testing). With this plugin, mipmap generation happens slowly on the cpu.

Instead of using this plugin, consider using the new [CompressedImageSaver](https://bevyengine.org/news/bevy-0-12/#compressedimagesaver).

For generating compressed textures ahead of time also check out:
- [klafsa](https://github.com/superdump/klafsa)
- [kram](https://github.com/alecazam/kram)
- [toktx](https://github.khronos.org/KTX-Software/ktxtools/toktx.html)
- [compressonator](https://gpuopen.com/compressonator/)
- [basis_universal](https://github.com/BinomialLLC/basis_universal)

In my experience, many of these compressed formats can be used with bevy in `gltf` files. This can be done by converting and replacing the images included in the `gltf` and then setting the mimeType with something like: `"mimeType": "image/ktx2"` (for ktx2)

## Usage

```rust
    .add_plugins(DefaultPlugins)
    // Add MipmapGeneratorPlugin after default plugins
    .add_plugin(MipmapGeneratorPlugin)
    // Add material types to be converted
    .add_systems(Update, generate_mipmaps::<StandardMaterial>)
```

When materials are created, mipmaps will be created for the images used in the material.

Mipmaps will not be generated for materials found on entities that also have the `NoMipmapGeneration` component.

## Custom Materials
For use with custom materials, just implement the GetImages trait for the custom material.

```rust
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
```

## TODO
- Support more texture formats.
- Support re-running if images are updated.

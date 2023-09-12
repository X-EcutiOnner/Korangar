use std::collections::HashMap;
use std::sync::Arc;

use derive_new::new;
use procedural::*;
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage};
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, CopyBufferToImageInfo, PrimaryAutoCommandBuffer, PrimaryCommandBufferAbstract,
};
use vulkano::device::Queue;
use vulkano::format::Format;
use vulkano::image::view::ImageView;
use vulkano::image::{Image, ImageCreateInfo, ImageUsage};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter};
use vulkano::sync::future::FenceSignalFuture;
use vulkano::sync::GpuFuture;

use super::{conversion_result, ConversionError};
#[cfg(feature = "debug")]
use crate::debug::*;
use crate::graphics::MemoryAllocator;
use crate::interface::{ElementCell, PrototypeElement};
use crate::loaders::{ByteStream, FromBytes, GameFileLoader, MinorFirst, Version};

#[derive(Clone, Debug, PrototypeElement)]
pub struct Sprite {
    #[hidden_element]
    pub textures: Vec<Arc<ImageView>>,
    #[cfg(feature = "debug")]
    sprite_data: SpriteData,
}

#[derive(Clone, Debug, Named)]
struct EncodedData(pub Vec<u8>);

impl FromBytes for EncodedData {
    fn from_bytes(byte_stream: &mut ByteStream, length_hint: Option<usize>) -> Result<Self, Box<ConversionError>> {
        let image_size = length_hint.unwrap();

        if image_size == 0 {
            return Ok(Self(Vec::new()));
        }

        let mut data = vec![0; image_size];
        let mut encoded = conversion_result::<Self, _>(u16::from_bytes(byte_stream, None))?;
        let mut next = 0;

        while next < image_size && encoded > 0 {
            let byte = byte_stream.next::<Self>()?;
            encoded -= 1;

            if byte == 0 {
                let length = usize::max(byte_stream.next::<Self>()? as usize, 1);
                encoded -= 1;

                if next + length > image_size {
                    return Err(ConversionError::from_message("too much data encoded in palette image"));
                }

                next += length;
            } else {
                data[next] = byte;
                next += 1;
            }
        }

        if next != image_size || encoded > 0 {
            return Err(ConversionError::from_message("badly encoded palette image"));
        }

        Ok(Self(data))
    }
}

impl PrototypeElement for EncodedData {
    fn to_element(&self, display: String) -> ElementCell {
        self.0.to_element(display)
    }
}

#[derive(Clone, Debug, Named, FromBytes, PrototypeElement)]
struct PaletteImageData {
    pub width: u16,
    pub height: u16,
    #[version_equals_or_above(2, 1)]
    #[length_hint(self.width as usize * self.height as usize)]
    pub encoded_data: Option<EncodedData>,
    #[version_smaller(2, 1)]
    #[length_hint(self.width as usize * self.height as usize)]
    pub raw_data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Named, FromBytes, PrototypeElement)]
struct RgbaImageData {
    pub width: u16,
    pub height: u16,
    #[length_hint(self.width as usize * self.height as usize * 4)]
    pub data: Vec<u8>,
}

#[derive(Copy, Clone, Debug, Default, Named, FromBytes, PrototypeElement)]
struct PaletteColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub reserved: u8,
}

impl PaletteColor {
    pub fn color_bytes(&self, index: u8) -> [u8; 4] {
        let alpha = match index {
            0 => 0,
            _ => 255,
        };

        [self.red, self.green, self.blue, alpha]
    }
}

#[derive(Clone, Debug, Named, FromBytes, PrototypeElement)]
struct Palette {
    pub colors: [PaletteColor; 256],
}

#[derive(Clone, Debug, Named, FromBytes, PrototypeElement)]
struct SpriteData {
    #[version]
    pub version: Version<MinorFirst>,
    pub palette_image_count: u16,
    #[version_equals_or_above(1, 2)]
    pub rgba_image_count: Option<u16>,
    #[repeating(self.palette_image_count)]
    pub palette_image_data: Vec<PaletteImageData>,
    #[repeating(self.rgba_image_count.unwrap_or_default())]
    pub rgba_image_data: Vec<RgbaImageData>,
    #[version_equals_or_above(1, 1)]
    pub palette: Option<Palette>,
}

#[derive(new)]
pub struct SpriteLoader {
    memory_allocator: Arc<MemoryAllocator>,
    queue: Arc<Queue>,
    #[new(default)]
    load_buffer: Option<AutoCommandBufferBuilder<PrimaryAutoCommandBuffer<MemoryAllocator>, MemoryAllocator>>,
    #[new(default)]
    cache: HashMap<String, Arc<Sprite>>,
}

impl SpriteLoader {
    fn load(&mut self, path: &str, game_file_loader: &mut GameFileLoader) -> Result<Arc<Sprite>, String> {
        #[cfg(feature = "debug")]
        let timer = Timer::new_dynamic(format!("load sprite from {MAGENTA}{path}{NONE}"));

        let bytes = game_file_loader.get(&format!("data\\sprite\\{path}"))?;
        let mut byte_stream = ByteStream::new(&bytes);

        if <[u8; 2]>::from_bytes(&mut byte_stream, None).unwrap() != [b'S', b'P'] {
            return Err(format!("failed to read magic number from {path}"));
        }

        let sprite_data = SpriteData::from_bytes(&mut byte_stream, None).unwrap();
        #[cfg(feature = "debug")]
        let cloned_sprite_data = sprite_data.clone();

        let palette = sprite_data.palette.unwrap(); // unwrap_or_default() as soon as i know what
        // the default palette is

        let rgba_images/*: Vec<Arc<ImmutableImage>>*/ = sprite_data
            .rgba_image_data
            .into_iter();

        let palette_images = sprite_data.palette_image_data.into_iter().map(|image_data| {
            // decode palette image data if necessary
            let data: Vec<u8> = image_data
                .encoded_data
                .map(|encoded| encoded.0)
                .unwrap_or_else(|| image_data.raw_data.unwrap())
                .iter()
                .flat_map(|palette_index| palette.colors[*palette_index as usize].color_bytes(*palette_index))
                .collect();

            RgbaImageData {
                width: image_data.width,
                height: image_data.height,
                data,
            }
        });

        let load_buffer = self.load_buffer.get_or_insert_with(|| {
            AutoCommandBufferBuilder::primary(
                &*self.memory_allocator,
                self.queue.queue_family_index(),
                CommandBufferUsage::OneTimeSubmit,
            )
            .unwrap()
        });

        let textures = rgba_images
            .chain(palette_images)
            .map(|image_data| {
                let buffer = Buffer::from_iter(
                    &*self.memory_allocator,
                    BufferCreateInfo {
                        usage: BufferUsage::TRANSFER_SRC,
                        ..Default::default()
                    },
                    AllocationCreateInfo {
                        memory_type_filter: MemoryTypeFilter::PREFER_HOST | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                        ..Default::default()
                    },
                    image_data.data.iter().copied(),
                )
                .unwrap();

                let image = Image::new(
                    &*self.memory_allocator,
                    ImageCreateInfo {
                        format: Format::R8G8B8A8_UNORM,
                        extent: [image_data.width as u32, image_data.height as u32, 1],
                        usage: ImageUsage::TRANSFER_DST | ImageUsage::SAMPLED,
                        ..Default::default()
                    },
                    AllocationCreateInfo::default(),
                )
                .unwrap();

                load_buffer
                    .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(buffer, image.clone()))
                    .unwrap();

                ImageView::new_default(image).unwrap()
            })
            .collect();

        let sprite = Arc::new(Sprite {
            textures,
            #[cfg(feature = "debug")]
            sprite_data: cloned_sprite_data,
        });

        self.cache.insert(path.to_string(), sprite.clone());

        #[cfg(feature = "debug")]
        timer.stop();

        Ok(sprite)
    }

    pub fn get(&mut self, path: &str, game_file_loader: &mut GameFileLoader) -> Result<Arc<Sprite>, String> {
        match self.cache.get(path) {
            Some(sprite) => Ok(sprite.clone()),
            None => self.load(path, game_file_loader),
        }
    }

    pub fn submit_load_buffer(&mut self) -> Option<FenceSignalFuture<Box<dyn GpuFuture>>> {
        self.load_buffer.take().map(|buffer| {
            buffer
                .build()
                .unwrap()
                .execute(self.queue.clone())
                .unwrap()
                .boxed()
                .then_signal_fence_and_flush()
                .unwrap()
        })
    }
}

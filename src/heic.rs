use std::{io::BufRead, marker::PhantomData};

use image::{
    error::{DecodingError, ImageFormatHint},
    ImageDecoder, ImageError,
};
use imageio::{ImageMetadata, ImageSource};

pub struct HeicDecoder<R> {
    metadata: ImageMetadata,
    img: ImageSource,
    _p: PhantomData<R>,
}

impl<R: BufRead> ImageDecoder for HeicDecoder<R> {
    fn dimensions(&self) -> (u32, u32) {
        (self.metadata.width as u32, self.metadata.height as u32)
    }

    fn color_type(&self) -> image::ColorType {
        image::ColorType::Rgba8
    }

    fn read_image(self, buf: &mut [u8]) -> image::ImageResult<()>
    where
        Self: Sized,
    {
        assert!(self.total_bytes() == buf.len().try_into().unwrap());
        assert!(self.img.frame_count() > 0);

        let decoded_img = self
            .img
            .decode_image_at_index(self.img.primary_image_index())
            .map_err(|err| {
                ImageError::Decoding(DecodingError::new(
                    ImageFormatHint::PathExtension("heic".into()),
                    err,
                ))
            })?;

        buf.copy_from_slice(&decoded_img.bgra);

        Ok(())
    }

    fn read_image_boxed(self: Box<Self>, buf: &mut [u8]) -> image::ImageResult<()> {
        (*self).read_image(buf)
    }
}

fn metadata_from_source(source: &ImageSource) -> Result<ImageMetadata, imageio::ImageError> {
    let frame_count = source.frame_count();
    if frame_count == 0 {
        return Err(imageio::ImageError::NoImagesInSource);
    }
    let properties = source.properties_at_index(0)?;
    let width = properties
        .i64("PixelWidth")?
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let height = properties
        .i64("PixelHeight")?
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let has_alpha = properties.bool("HasAlpha")?.unwrap_or(false);
    Ok(ImageMetadata {
        width,
        height,
        frame_count,
        has_alpha,
        source_format: source.source_type(),
    })
}

impl<R> HeicDecoder<R>
where
    R: BufRead,
{
    pub fn new(reader: R) -> Result<Self, ImageError> {
        let bytes: Vec<_> = reader.bytes().flat_map(|b| b).collect();
        let source = ImageSource::from_bytes(&bytes).map_err(|err| {
            image::ImageError::Decoding(DecodingError::new(
                ImageFormatHint::PathExtension("heic".into()),
                err,
            ))
        })?;
        let metadata = metadata_from_source(&source);
        match (metadata, source) {
            (Ok(metadata), img) => Ok(Self {
                metadata,
                img,
                _p: PhantomData,
            }),
            (Err(err), _) => Err(ImageError::Decoding(DecodingError::new(
                ImageFormatHint::PathExtension("heic".into()),
                err,
            ))),
        }
    }
}

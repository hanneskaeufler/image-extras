use std::{io::BufRead, marker::PhantomData};

use image::{
    error::{DecodingError, ImageFormatHint},
    metadata::Orientation,
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

    fn orientation(&mut self) -> image::ImageResult<Orientation> {
        let properties = self.img.properties_at_index(0).map_err(|err| {
            ImageError::Decoding(DecodingError::new(
                ImageFormatHint::PathExtension("heic".into()),
                err,
            ))
        })?;
        let value = properties.i64("Orientation").map_err(|err| {
            ImageError::Decoding(DecodingError::new(
                ImageFormatHint::PathExtension("heic".into()),
                err,
            ))
        })?;
        Ok(Orientation::from_exif(value.unwrap_or(1) as u8).unwrap_or(Orientation::NoTransforms))
    }

    fn exif_metadata(&mut self) -> image::ImageResult<Option<Vec<u8>>> {
        let properties = self.img.properties_at_index(0).map_err(|err| {
            ImageError::Decoding(DecodingError::new(
                ImageFormatHint::PathExtension("heic".into()),
                err,
            ))
        })?;
        let value = properties.i64("Orientation").map_err(|err| {
            ImageError::Decoding(DecodingError::new(
                ImageFormatHint::PathExtension("heic".into()),
                err,
            ))
        })?;
        let orientation = value.unwrap_or(1) as u8;
        if orientation <= 1 {
            return Ok(None);
        }
        Ok(Some(make_exif_chunk(orientation)))
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

fn make_exif_chunk(orientation: u8) -> Vec<u8> {
    // Build a minimal valid little-endian TIFF EXIF chunk containing
    // just the orientation tag, which is what the image crate's
    // Orientation::remove_from_exif_chunk / from_exif_chunk expect.
    //
    // Layout:
    //   [0..4)   TIFF header (endian + magic)
    //   [4..8)   IFD offset (u32 = 8)
    //   [8..10)  IFD entry count (u16 = 1)
    //   [10..22) Single IFD entry: tag(0x0112) + type(SHORT) + count(1) + value + pad
    //   [22..26) Next IFD offset (u32 = 0)
    let mut chunk = Vec::with_capacity(26);
    chunk.extend_from_slice(b"II");               // 0-1:  little-endian
    chunk.extend_from_slice(&[0x2A, 0x00]);       // 2-3:  TIFF magic 42
    chunk.extend_from_slice(&[8, 0, 0, 0]);        // 4-7:  IFD offset = 8
    chunk.extend_from_slice(&[1, 0]);              // 8-9:  1 IFD entry
    chunk.extend_from_slice(&[0x12, 0x01]);        // 10-11: tag = Orientation
    chunk.extend_from_slice(&[3, 0]);              // 12-13: type = SHORT
    chunk.extend_from_slice(&[1, 0, 0, 0]);        // 14-17: count = 1
    chunk.extend_from_slice(&[orientation, 0, 0, 0]); // 18-21: value + padding
    chunk.extend_from_slice(&[0, 0, 0, 0]);        // 22-25: next IFD = 0
    chunk
}

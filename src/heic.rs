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
    exif_data: Option<Vec<u8>>,
    _p: PhantomData<R>,
}

impl<R: BufRead> ImageDecoder for HeicDecoder<R> {
    fn dimensions(&self) -> (u32, u32) {
        (self.metadata.width as u32, self.metadata.height as u32)
    }

    fn color_type(&self) -> image::ColorType {
        image::ColorType::Rgba8
    }

    fn exif_metadata(&mut self) -> image::ImageResult<Option<Vec<u8>>> {
        Ok(self.exif_data.clone())
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
        let exif_data = extract_exif_from_bytes(&bytes);
        let metadata = metadata_from_source(&source);
        match (metadata, source) {
            (Ok(metadata), img) => Ok(Self {
                metadata,
                img,
                exif_data,
                _p: PhantomData,
            }),
            (Err(err), _) => Err(ImageError::Decoding(DecodingError::new(
                ImageFormatHint::PathExtension("heic".into()),
                err,
            ))),
        }
    }
}

fn extract_exif_from_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    parse_boxes(bytes, 0, bytes.len())
}

fn parse_boxes(bytes: &[u8], start: usize, end: usize) -> Option<Vec<u8>> {
    let mut offset = start;
    while offset + 8 <= end {
        let raw_size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let box_type = &bytes[offset + 4..offset + 8];

        let (box_end, header_size) = if raw_size == 1 {
            if offset + 16 > end {
                break;
            }
            let large_size = u64::from_be_bytes(bytes[offset + 8..offset + 16].try_into().unwrap());
            (offset + usize::try_from(large_size).ok()?, 16)
        } else if raw_size == 0 {
            (end, 8)
        } else {
            (offset + raw_size as usize, 8)
        };

        if box_end > end {
            break;
        }

        if box_type == b"Exif" {
            return Some(bytes[offset + header_size..box_end].to_vec());
        }

        // Recurse into container boxes that may hold an Exif box
        if matches!(box_type, b"moov" | b"meta" | b"moof" | b"traf") {
            if let Some(exif) = parse_boxes(bytes, offset + header_size, box_end) {
                return Some(exif);
            }
        }

        if box_end <= offset {
            break;
        }
        offset = box_end;
    }
    None
}

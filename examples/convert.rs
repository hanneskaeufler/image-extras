//! An example of opening an image.
extern crate image;
extern crate image_extras;

use std::env;
use std::error::Error;
use std::path::Path;

use image::metadata::Orientation;
use image::{DynamicImage, ImageReader};

fn main() -> Result<(), Box<dyn Error>> {
    image_extras::register();

    let (from, into) = if env::args_os().count() == 3 {
        (
            env::args_os().nth(1).unwrap(),
            env::args_os().nth(2).unwrap(),
        )
    } else {
        println!("Please enter a from and into path.");
        std::process::exit(1);
    };

    // Use the open function to load an image from a Path.
    // ```open``` returns a dynamic image.
    let reader = ImageReader::open(&from)?;
    let mut decoder = reader.into_decoder()?;
    use crate::image::ImageDecoder;

    let mut exif = decoder.exif_metadata().unwrap_or(None);

    let mut img = DynamicImage::from_decoder(decoder).unwrap();

    if let Some(exif) = &mut exif {
        let orientation = Orientation::remove_from_exif_chunk(exif);
        if let Some(orientation) = orientation {
            img.apply_orientation(orientation);
        }
    }

    img.save(Path::new(&into)).unwrap();
    Ok(())
}

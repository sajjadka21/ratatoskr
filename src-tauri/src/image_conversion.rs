use image::{codecs::jpeg::JpegEncoder, DynamicImage, ImageReader, Limits};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormatChoice {
    Png,
    Jpeg,
    Webp,
}

impl ImageFormatChoice {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "png" => Ok(Self::Png),
            "jpeg" => Ok(Self::Jpeg),
            "webp" => Ok(Self::Webp),
            _ => Err("unsupported image output format".to_owned()),
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }

    fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
        }
    }
}

#[derive(Debug)]
pub struct ConvertedImage {
    pub path: PathBuf,
    pub filename: String,
    pub mime_type: &'static str,
    pub bytes: u64,
}

pub fn convert_file(source: &Path, format: ImageFormatChoice) -> Result<ConvertedImage, String> {
    if !source.is_file() {
        return Err("downloaded image file is missing".to_owned());
    }

    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case(format.extension())
        || (format == ImageFormatChoice::Jpeg && extension.eq_ignore_ascii_case("jpeg"))
    {
        return Ok(ConvertedImage {
            path: source.to_path_buf(),
            filename: source
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("image")
                .to_owned(),
            mime_type: format.mime_type(),
            bytes: fs::metadata(source)
                .map_err(|error| format!("could not read image size: {error}"))?
                .len(),
        });
    }

    let mut reader = ImageReader::open(source)
        .map_err(|error| format!("could not open image: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("could not identify image format: {error}"))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(30_000);
    limits.max_image_height = Some(30_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| format!("this image cannot be converted: {error}"))?;

    let destination = unique_destination(source, format.extension())?;
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| format!("could not create converted image: {error}"))?;
    let result = encode(image, output, format);
    if let Err(error) = result {
        let _ = fs::remove_file(&destination);
        return Err(error);
    }

    let bytes = fs::metadata(&destination)
        .map_err(|error| format!("could not inspect converted image: {error}"))?
        .len();
    let filename = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("image")
        .to_owned();
    Ok(ConvertedImage {
        path: destination,
        filename,
        mime_type: format.mime_type(),
        bytes,
    })
}

fn unique_destination(source: &Path, extension: &str) -> Result<PathBuf, String> {
    let parent = source.parent().ok_or("image has no parent folder")?;
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("image");
    let first = parent.join(format!("{stem}.{extension}"));
    if !first.exists() {
        return Ok(first);
    }
    for index in 1..=9_999 {
        let candidate = parent.join(format!("{stem} ({index}).{extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("could not choose a unique name for the converted image".to_owned())
}

fn encode(image: DynamicImage, file: File, format: ImageFormatChoice) -> Result<(), String> {
    let mut writer = BufWriter::new(file);
    match format {
        ImageFormatChoice::Png => image
            .write_to(&mut writer, image::ImageFormat::Png)
            .map_err(|error| format!("could not encode PNG image: {error}"))?,
        ImageFormatChoice::Jpeg => JpegEncoder::new_with_quality(&mut writer, 92)
            .encode_image(&image.to_rgb8())
            .map_err(|error| format!("could not encode JPEG image: {error}"))?,
        ImageFormatChoice::Webp => image
            .write_to(&mut writer, image::ImageFormat::WebP)
            .map_err(|error| format!("could not encode WebP image: {error}"))?,
    }
    writer
        .flush()
        .map_err(|error| format!("could not finish converted image: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{convert_file, ImageFormatChoice};
    use image::{ImageBuffer, Rgba};

    #[test]
    fn creates_a_real_webp_copy_and_keeps_the_source_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("car.png");
        let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_pixel(12, 8, Rgba([180, 80, 20, 255]));
        image.save(&source).unwrap();

        let converted = convert_file(&source, ImageFormatChoice::Webp).unwrap();

        assert_eq!(converted.filename, "car.webp");
        assert_eq!(converted.mime_type, "image/webp");
        assert!(source.exists(), "the original download is preserved");
        assert_eq!(
            image::ImageReader::open(converted.path)
                .unwrap()
                .with_guessed_format()
                .unwrap()
                .format(),
            Some(image::ImageFormat::WebP)
        );
    }

    #[test]
    fn adds_a_number_instead_of_overwriting_an_existing_converted_image() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("car.png");
        let existing = temp.path().join("car.webp");
        ImageBuffer::<Rgba<u8>, Vec<u8>>::from_pixel(4, 4, Rgba([0, 0, 0, 255]))
            .save(&source)
            .unwrap();
        std::fs::write(&existing, b"existing file").unwrap();

        let converted = convert_file(&source, ImageFormatChoice::Webp).unwrap();

        assert_eq!(converted.filename, "car (1).webp");
        assert_eq!(std::fs::read(existing).unwrap(), b"existing file");
        assert!(source.exists());
    }
}

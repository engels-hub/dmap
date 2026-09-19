//! Image files decoded on a worker thread.

// Rust guideline compliant 2026-02-21

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

// These aliases are the instrumented endpoints when the `hotpath` feature is
// on, and the plain `std::sync::mpsc` types when it is off.
use hotpath::wrap::std::sync::mpsc::{Receiver, Sender};
use image::{DynamicImage, ImageReader};

/// The file extensions of the images a map can come from.
///
/// These are the formats the `image` crate builds with, in `Cargo.toml`.
/// A name added here and not there shows files the decoder refuses.
pub const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

/// A decoded image: RGBA8 rows, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub size: (u32, u32),
    pub rgba: Vec<u8>,
}

/// Decodes PNG, JPEG or WebP bytes. Images beyond `max_side` pixels are shrunk.
///
/// # Errors
///
/// Returns the decoder's message when `bytes` is not an image it can read.
#[hotpath::measure]
pub fn decode(bytes: &[u8], max_side: u32) -> Result<Decoded, String> {
    let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    let size = (image.width(), image.height());
    let fitted = shrink_to_fit(size, max_side);
    let image = if fitted == size {
        image
    } else {
        image.resize_exact(fitted.0, fitted.1, image::imageops::FilterType::Triangle)
    };
    Ok(Decoded {
        size: fitted,
        rgba: image.into_rgba8().into_raw(),
    })
}

/// The largest size with the same aspect ratio that fits in `max_side`.
pub fn shrink_to_fit(size: (u32, u32), max_side: u32) -> (u32, u32) {
    let longest = size.0.max(size.1);
    if longest <= max_side {
        return size;
    }
    let scale = f64::from(max_side) / f64::from(longest);
    (
        (f64::from(size.0) * scale).round() as u32,
        (f64::from(size.1) * scale).round() as u32,
    )
}

/// Loads image files on a worker thread, so the window never waits.
pub struct Loader {
    requests: Sender<PathBuf>,
    results: Receiver<(PathBuf, Result<Decoded, String>)>,
}

impl std::fmt::Debug for Loader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Loader").finish_non_exhaustive()
    }
}

impl Loader {
    /// Starts the worker. It calls `wake` after each file, done or failed.
    pub fn spawn(max_side: u32, wake: impl Fn() + Send + 'static) -> Self {
        let (requests, request_rx) =
            hotpath::channel!(channel::<PathBuf>(), label = "load-requests");
        let (result_tx, results) = hotpath::channel!(channel(), label = "load-results");
        std::thread::spawn(move || {
            for path in request_rx {
                let result = std::fs::read(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| decode(&bytes, max_side));
                if result_tx.send((path, result)).is_err() {
                    break;
                }
                wake();
            }
        });
        Self { requests, results }
    }

    /// Queues a file. Its result comes back through `poll`.
    pub fn request(&self, path: PathBuf) {
        // The worker only stops when this `Loader` is dropped, so a failed
        // send cannot happen while `self` exists.
        let _ = self.requests.send(path);
    }

    /// The next finished file, if any.
    pub fn poll(&self) -> Option<(PathBuf, Result<Decoded, String>)> {
        self.results.try_recv().ok()
    }
}

/// Opens an image file and finds its format from its first bytes.
///
/// The extension counts only when the bytes match no format. A JPEG saved
/// as `map.png` then opens as the JPEG it is, the same as in [`decode`].
fn reader(file: &Path) -> Result<ImageReader<BufReader<File>>, String> {
    ImageReader::open(file)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| format!("{}: {error}", file.display()))
}

/// Checks that `file` is an image the decoder reads.
///
/// Only the header is read, so a large map costs no more than a small one.
///
/// # Errors
///
/// Returns a message that names the file when it cannot be opened, or when
/// its format is not one of [`EXTENSIONS`].
pub fn check(file: &Path) -> Result<(), String> {
    reader(file)?
        .into_dimensions()
        .map(drop)
        .map_err(|error| format!("{}: {error}", file.display()))
}

/// Decodes a whole image file at its full size.
///
/// # Errors
///
/// Returns a message that names the file when it cannot be opened or
/// decoded.
pub fn open(file: &Path) -> Result<DynamicImage, String> {
    reader(file)?
        .decode()
        .map_err(|error| format!("{}: {error}", file.display()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{check, decode, open, shrink_to_fit};

    fn encode(width: u32, height: u32, format: image::ImageFormat) -> Vec<u8> {
        let mut bytes = Vec::new();
        let image =
            image::RgbaImage::from_fn(width, height, |x, _| image::Rgba([x as u8, 0, 0, 255]));
        image
            .write_to(&mut Cursor::new(&mut bytes), format)
            .unwrap();
        bytes
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        encode(width, height, image::ImageFormat::Png)
    }

    #[test]
    fn decodes_webp_bytes_to_rgba() {
        let decoded = decode(&encode(4, 2, image::ImageFormat::WebP), 8192).unwrap();
        assert_eq!(decoded.size, (4, 2));
        assert_eq!(&decoded.rgba[4..8], &[1, 0, 0, 255]);
    }

    #[test]
    fn check_passes_an_image_and_refuses_other_files() {
        let dir = std::env::temp_dir().join(format!("dmap-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let map = dir.join("map.webp");
        std::fs::write(&map, encode(4, 2, image::ImageFormat::WebP)).unwrap();
        let notes = dir.join("notes.txt");
        std::fs::write(&notes, b"not an image").unwrap();
        check(&map).unwrap();
        check(&notes).unwrap_err();
        check(&dir.join("gone.png")).unwrap_err();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_under_the_wrong_name_opens_as_what_it_is() {
        let dir = std::env::temp_dir().join(format!("dmap-misnamed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // JPEG holds no alpha, so this image is RGB.
        let mut jpeg = Vec::new();
        image::RgbImage::new(4, 2)
            .write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .unwrap();
        for name in ["map.png", "map"] {
            let file = dir.join(name);
            std::fs::write(&file, &jpeg).unwrap();
            check(&file).unwrap();
            let image = open(&file).unwrap();
            assert_eq!((image.width(), image.height()), (4, 2));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn decodes_png_bytes_to_rgba() {
        let decoded = decode(&png(4, 2), 8192).unwrap();
        assert_eq!(decoded.size, (4, 2));
        assert_eq!(decoded.rgba.len(), 4 * 2 * 4);
        assert_eq!(&decoded.rgba[4..8], &[1, 0, 0, 255]);
    }

    #[test]
    fn shrinks_an_image_larger_than_the_limit() {
        let decoded = decode(&png(40, 20), 16).unwrap();
        assert_eq!(decoded.size, (16, 8));
    }

    #[test]
    fn rejects_bytes_that_are_not_an_image() {
        decode(b"not an image", 8192).unwrap_err();
    }

    #[test]
    fn keeps_the_aspect_ratio_when_it_shrinks() {
        assert_eq!(shrink_to_fit((10000, 5000), 8192), (8192, 4096));
        assert_eq!(shrink_to_fit((5000, 10000), 8192), (4096, 8192));
        assert_eq!(shrink_to_fit((100, 50), 8192), (100, 50));
    }
}

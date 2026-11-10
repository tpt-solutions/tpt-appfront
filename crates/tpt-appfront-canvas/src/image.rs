//! Image decoding for `NodeKind::Image`.
//!
//! The format is detected from the leading bytes. Supported formats are PNG,
//! JPEG, GIF and WebP. Animated GIF and WebP decode to their first frame. Every
//! format becomes RGBA8, so the painter has one code path. Pure Rust decoders
//! only, all MIT OR Apache-2.0 (see `Cargo.toml`).

use std::collections::HashMap;
use std::io::Cursor;
use std::rc::Rc;

/// Largest image accepted, in pixels. Above this, decoding is refused rather than
/// allocating a huge buffer for a hostile or corrupt file.
const MAX_PIXELS: u64 = 25_000_000;

/// A decoded image: `width * height * 4` bytes of RGBA8, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Why an image could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The bytes do not start with a supported format's signature.
    UnknownFormat,
    /// The format was recognized, but the data is corrupt or unsupported.
    Corrupt(String),
    /// The image is larger than [`MAX_PIXELS`].
    TooLarge { width: u32, height: u32 },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::UnknownFormat => write!(f, "unknown image format"),
            DecodeError::Corrupt(why) => write!(f, "corrupt image: {why}"),
            DecodeError::TooLarge { width, height } => {
                write!(f, "image {width}x{height} is too large")
            }
        }
    }
}

/// Decodes `bytes` as PNG, JPEG, GIF or WebP, detected from the leading bytes.
pub fn decode(bytes: &[u8]) -> Result<Decoded, DecodeError> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        decode_png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        decode_jpeg(bytes)
    } else if bytes.starts_with(b"GIF8") {
        decode_gif(bytes)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        decode_webp(bytes)
    } else {
        Err(DecodeError::UnknownFormat)
    }
}

fn check_size(width: u32, height: u32) -> Result<(), DecodeError> {
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        Err(DecodeError::TooLarge { width, height })
    } else {
        Ok(())
    }
}

fn corrupt(e: impl std::fmt::Display) -> DecodeError {
    DecodeError::Corrupt(e.to_string())
}

fn decode_png(bytes: &[u8]) -> Result<Decoded, DecodeError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    // Expand palette, low bit depth and 16-bit data to 8-bit channels.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(corrupt)?;
    let (width, height) = (reader.info().width, reader.info().height);
    check_size(width, height)?;
    let mut buf = vec![0u8; reader.output_buffer_size().ok_or_else(|| corrupt("no output size"))?];
    let info = reader.next_frame(&mut buf).map_err(corrupt)?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>().0.iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>().0.iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return Err(corrupt("palette was not expanded")),
    };
    finish(width, height, rgba)
}

fn decode_jpeg(bytes: &[u8]) -> Result<Decoded, DecodeError> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;

    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(corrupt)?;
    let (width, height) = decoder.dimensions().ok_or_else(|| corrupt("no dimensions"))?;
    let (width, height) = (width as u32, height as u32);
    check_size(width, height)?;
    let rgba = decoder.decode().map_err(corrupt)?;
    finish(width, height, rgba)
}

fn decode_gif(bytes: &[u8]) -> Result<Decoded, DecodeError> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(bytes).map_err(corrupt)?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    check_size(width, height)?;
    let frame = decoder
        .read_next_frame()
        .map_err(corrupt)?
        .ok_or_else(|| corrupt("no frames"))?;
    // A frame may cover only part of the logical screen. Place it at its offset on a
    // transparent screen. Later frames are not composited (first frame only).
    let (fw, fh) = (usize::from(frame.width), usize::from(frame.height));
    let (left, top) = (usize::from(frame.left), usize::from(frame.top));
    let (sw, sh) = (width as usize, height as usize);
    let mut rgba = vec![0u8; sw * sh * 4];
    for row in 0..fh {
        let dy = top + row;
        if dy >= sh {
            break;
        }
        for col in 0..fw {
            let dx = left + col;
            if dx >= sw {
                break;
            }
            let src = (row * fw + col) * 4;
            let dst = (dy * sw + dx) * 4;
            if let Some(px) = frame.buffer.get(src..src + 4) {
                rgba[dst..dst + 4].copy_from_slice(px);
            }
        }
    }
    finish(width, height, rgba)
}

fn decode_webp(bytes: &[u8]) -> Result<Decoded, DecodeError> {
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(corrupt)?;
    let (width, height) = decoder.dimensions();
    check_size(width, height)?;
    let size = decoder
        .output_buffer_size()
        .ok_or_else(|| corrupt("no output size"))?;
    let mut buf = vec![0u8; size];
    if decoder.is_animated() {
        decoder.read_frame(&mut buf).map_err(corrupt)?;
    } else {
        decoder.read_image(&mut buf).map_err(corrupt)?;
    }
    let rgba = if decoder.has_alpha() {
        buf
    } else {
        buf.as_chunks::<3>().0.iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect()
    };
    finish(width, height, rgba)
}

/// Checks the RGBA buffer length against the dimensions before returning it.
fn finish(width: u32, height: u32, rgba: Vec<u8>) -> Result<Decoded, DecodeError> {
    let expected = width as usize * height as usize * 4;
    if rgba.len() != expected {
        return Err(corrupt(format!(
            "expected {expected} bytes for {width}x{height} RGBA, got {}",
            rgba.len()
        )));
    }
    Ok(Decoded {
        width,
        height,
        rgba,
    })
}

/// Loads image bytes for a `src` string. The canvas never fetches anything itself.
/// The host supplies this, for example from `include_bytes!`, a file, or a cache.
pub type ImageLoader = Box<dyn Fn(&str) -> Option<Vec<u8>>>;

/// Decoded images for the painter, keyed by `src`. Each source is loaded and
/// decoded once. Failures are cached too, so a broken image is not retried every frame.
#[derive(Default)]
pub struct ImageCache {
    loader: Option<ImageLoader>,
    entries: HashMap<String, Option<Rc<Decoded>>>,
}

impl ImageCache {
    pub fn new(loader: Option<ImageLoader>) -> Self {
        ImageCache {
            loader,
            entries: HashMap::new(),
        }
    }

    /// Returns the decoded image for `src`, or `None` when there is no loader, the
    /// loader has no bytes, or decoding fails.
    pub fn get(&mut self, src: &str) -> Option<Rc<Decoded>> {
        if let Some(entry) = self.entries.get(src) {
            return entry.clone();
        }
        let decoded = self
            .loader
            .as_ref()
            .and_then(|load| load(src))
            .and_then(|bytes| decode(&bytes).ok())
            .map(Rc::new);
        self.entries.insert(src.to_owned(), decoded.clone());
        decoded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED_GREEN_PNG: &[u8] = include_bytes!("../tests/fixtures/red_green_2x2.png");
    const LOSSLESS_WEBP: &[u8] = include_bytes!("../tests/fixtures/lossless_2x2.webp");
    const SOLID_JPEG: &[u8] = include_bytes!("../tests/fixtures/solid_red_16.jpg");
    const LOSSY_WEBP: &[u8] = include_bytes!("../tests/fixtures/lossy_16.webp");
    const ANIM_GIF: &[u8] = include_bytes!("../tests/fixtures/anim_red_green.gif");
    const ANIM_WEBP: &[u8] = include_bytes!("../tests/fixtures/anim_red_green.webp");

    /// The four pixels of the 2x2 fixtures, row-major: red, green, blue, white.
    const QUAD: [[u8; 3]; 4] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]];

    fn assert_quad(d: &Decoded) {
        assert_eq!((d.width, d.height), (2, 2));
        for (i, want) in QUAD.iter().enumerate() {
            let p = &d.rgba[i * 4..i * 4 + 4];
            assert_eq!(&p[..3], want, "pixel {i}");
            assert_eq!(p[3], 255, "pixel {i} alpha");
        }
    }

    fn close(a: u8, b: u8) -> bool {
        (i32::from(a) - i32::from(b)).abs() <= 6
    }

    #[test]
    fn png_decodes_exact_pixels() {
        assert_quad(&decode(RED_GREEN_PNG).unwrap());
    }

    #[test]
    fn lossless_webp_decodes_exact_pixels() {
        assert_quad(&decode(LOSSLESS_WEBP).unwrap());
    }

    #[test]
    fn gif_decodes_first_frame() {
        // The first frame of the fixture is solid red.
        let d = decode(ANIM_GIF).unwrap();
        assert_eq!((d.width, d.height), (2, 2));
        for px in d.rgba.as_chunks::<4>().0.iter() {
            assert_eq!(*px, [255, 0, 0, 255]);
        }
    }

    #[test]
    fn animated_webp_decodes_first_frame() {
        let d = decode(ANIM_WEBP).unwrap();
        assert_eq!((d.width, d.height), (2, 2));
        for px in d.rgba.as_chunks::<4>().0.iter() {
            assert_eq!(*px, [255, 0, 0, 255]);
        }
    }

    #[test]
    fn jpeg_decodes_close_to_source_colour() {
        // JPEG is lossy, so check the average colour rather than exact pixels.
        let d = decode(SOLID_JPEG).unwrap();
        assert_eq!((d.width, d.height), (16, 16));
        for px in d.rgba.as_chunks::<4>().0.iter() {
            assert!(close(px[0], 255) && close(px[1], 0) && close(px[2], 0), "{px:?}");
            assert_eq!(px[3], 255);
        }
    }

    #[test]
    fn lossy_webp_decodes_close_to_source_colour() {
        let d = decode(LOSSY_WEBP).unwrap();
        assert_eq!((d.width, d.height), (16, 16));
        let px = &d.rgba[0..4];
        assert!(close(px[0], 255) && close(px[1], 0) && close(px[2], 0), "{px:?}");
    }

    #[test]
    fn unknown_bytes_are_rejected() {
        assert_eq!(decode(b"not an image at all"), Err(DecodeError::UnknownFormat));
        assert_eq!(decode(&[]), Err(DecodeError::UnknownFormat));
    }

    #[test]
    fn truncated_png_is_corrupt_not_a_panic() {
        let truncated = &RED_GREEN_PNG[..RED_GREEN_PNG.len() / 2];
        assert!(matches!(decode(truncated), Err(DecodeError::Corrupt(_))));
    }

    #[test]
    fn cache_loads_once_and_remembers_failures() {
        use std::cell::Cell;
        use std::rc::Rc as StdRc;

        let calls = StdRc::new(Cell::new(0));
        let counter = calls.clone();
        let mut cache = ImageCache::new(Some(Box::new(move |src: &str| {
            counter.set(counter.get() + 1);
            match src {
                "png" => Some(RED_GREEN_PNG.to_vec()),
                _ => None,
            }
        })));
        assert!(cache.get("png").is_some());
        assert!(cache.get("png").is_some());
        assert!(cache.get("missing").is_none());
        assert!(cache.get("missing").is_none());
        assert_eq!(calls.get(), 2, "each source is loaded exactly once");
    }

    #[test]
    fn cache_without_loader_yields_none() {
        assert!(ImageCache::new(None).get("anything").is_none());
    }

    #[test]
    fn image_node_paints_decoded_pixels_and_falls_back_to_the_caption() {
        use crate::raster::Canvas;
        use crate::scene::Scene;
        use tpt_appfront_core::{ContainerBuilder, UITree};

        let ui: UITree<()> = UITree::container(|c: &mut ContainerBuilder<()>| {
            c.image("pic.png", "a picture");
        });
        let red_pixels = |scene: &mut Scene| {
            let (w, h) = (320usize, 200usize);
            let mut buf = vec![0u32; w * h];
            let mut canvas = Canvas::new(&mut buf, w, h);
            scene.frame(&mut canvas, &ui, &[], &|_: ()| {});
            buf.iter().filter(|&&p| p == 0xff0000).count()
        };

        // The loader supplies the 2x2 PNG, so its red quadrant is painted.
        let mut loaded = Scene::with_image_loader(Box::new(|src| {
            (src == "pic.png").then(|| RED_GREEN_PNG.to_vec())
        }));
        assert!(red_pixels(&mut loaded) > 1000);

        // No bytes for the source, so only the caption shows and nothing is red.
        let mut missing = Scene::with_image_loader(Box::new(|_| None));
        assert_eq!(red_pixels(&mut missing), 0);
    }

    #[test]
    fn corrupt_bytes_from_loader_fall_back_to_none() {
        let mut cache = ImageCache::new(Some(Box::new(|_| Some(b"\x89PNG garbage".to_vec()))));
        assert!(cache.get("bad").is_none());
    }
}

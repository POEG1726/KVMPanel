use axum::body::Bytes;
use serde_json::{json, Value};
use std::{io, ptr::NonNull};

use crate::ApiError;

type Handle = *mut libc::c_void;
type Init = unsafe extern "C" fn() -> Handle;
type Destroy = unsafe extern "C" fn(Handle) -> i32;
type Header = unsafe extern "C" fn(
    Handle,
    *const u8,
    libc::c_ulong,
    *mut i32,
    *mut i32,
    *mut i32,
    *mut i32,
) -> i32;
type Read =
    unsafe extern "C" fn(Handle, *const u8, libc::c_ulong, *mut u8, i32, i32, i32, i32, i32) -> i32;

struct Codec {
    init: Init,
    destroy: Destroy,
    header: Header,
    read: Read,
    #[cfg(not(target_arch = "arm"))]
    _library: libloading::Library,
}

#[cfg(target_arch = "arm")]
#[link(name = "turbojpeg", kind = "static")]
extern "C" {
    fn tjInitDecompress() -> Handle;
    fn tjDestroy(handle: Handle) -> i32;
    fn tjDecompressHeader3(
        handle: Handle,
        jpeg: *const u8,
        size: libc::c_ulong,
        width: *mut i32,
        height: *mut i32,
        sampling: *mut i32,
        color: *mut i32,
    ) -> i32;
    fn tjDecompress2(
        handle: Handle,
        jpeg: *const u8,
        size: libc::c_ulong,
        out: *mut u8,
        width: i32,
        pitch: i32,
        height: i32,
        pixels: i32,
        flags: i32,
    ) -> i32;
}

fn codec() -> io::Result<&'static Codec> {
    #[cfg(target_arch = "arm")]
    {
        static CODEC: Codec = Codec {
            init: tjInitDecompress,
            destroy: tjDestroy,
            header: tjDecompressHeader3,
            read: tjDecompress2,
        };
        Ok(&CODEC)
    }
    #[cfg(not(target_arch = "arm"))]
    {
        static CODEC: std::sync::OnceLock<Result<Codec, libloading::Error>> =
            std::sync::OnceLock::new();
        CODEC
            .get_or_init(|| unsafe {
                let library = libloading::Library::new("libturbojpeg.so.0")?;
                Ok(Codec {
                    init: *library.get(b"tjInitDecompress\0")?,
                    destroy: *library.get(b"tjDestroy\0")?,
                    header: *library.get(b"tjDecompressHeader3\0")?,
                    read: *library.get(b"tjDecompress2\0")?,
                    _library: library,
                })
            })
            .as_ref()
            .map_err(|error| io::Error::other(error.to_string()))
    }
}

struct Decoder<'a> {
    codec: &'static Codec,
    handle: NonNull<libc::c_void>,
    jpeg: &'a [u8],
    width: usize,
    height: usize,
}

impl<'a> Decoder<'a> {
    fn open(jpeg: &'a [u8]) -> io::Result<Self> {
        let codec = codec()?;
        let handle = NonNull::new(unsafe { (codec.init)() })
            .ok_or_else(|| io::Error::other("JPEG decoder initialization failed"))?;
        let mut decoder = Self {
            codec,
            handle,
            jpeg,
            width: 0,
            height: 0,
        };
        let (mut width, mut height, mut sampling, mut color) = (0, 0, 0, 0);
        let status = unsafe {
            (codec.header)(
                handle.as_ptr(),
                jpeg.as_ptr(),
                jpeg.len() as libc::c_ulong,
                &mut width,
                &mut height,
                &mut sampling,
                &mut color,
            )
        };
        // Bound the full-resolution working set before allocating a pixel buffer.
        if status != 0 || !(1..=4096).contains(&width) || !(1..=2160).contains(&height) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid HDMI JPEG dimensions",
            ));
        }
        decoder.width = width as usize;
        decoder.height = height as usize;
        Ok(decoder)
    }

    fn pixels(&self, divisor: usize, channels: usize) -> io::Result<(Vec<u8>, usize, usize)> {
        let width = self.width.div_ceil(divisor);
        let height = self.height.div_ceil(divisor);
        let mut pixels = vec![0; width * height * channels];
        let format = if channels == 1 { 6 } else { 0 }; // TJPF_GRAY or TJPF_RGB
        let status = unsafe {
            (self.codec.read)(
                self.handle.as_ptr(),
                self.jpeg.as_ptr(),
                self.jpeg.len() as libc::c_ulong,
                pixels.as_mut_ptr(),
                width as i32,
                0,
                height as i32,
                format,
                0,
            )
        };
        if status != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid HDMI JPEG pixels",
            ));
        }
        Ok((pixels, width, height))
    }
}

impl Drop for Decoder<'_> {
    fn drop(&mut self) {
        unsafe { (self.codec.destroy)(self.handle.as_ptr()) };
    }
}

struct Region {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Region {
    fn from_json(value: Option<&Value>) -> Result<Self, ApiError> {
        let field = |key, default| value.and_then(|v| v[key].as_f64()).unwrap_or(default);
        let region = Self {
            x: field("x", 0.0),
            y: field("y", 0.0),
            w: field("w", 1.0),
            h: field("h", 1.0),
        };
        if region.x + region.w > 1.0 + f64::EPSILON || region.y + region.h > 1.0 + f64::EPSILON {
            return Err(ApiError::new(400, "bad_region"));
        }
        Ok(region)
    }

    fn bounds(&self, width: usize, height: usize) -> (usize, usize, usize, usize) {
        let x = ((self.x * width as f64).floor() as usize).min(width - 1);
        let y = ((self.y * height as f64).floor() as usize).min(height - 1);
        let right = (((self.x + self.w) * width as f64).ceil() as usize).clamp(x + 1, width);
        let bottom = (((self.y + self.h) * height as f64).ceil() as usize).clamp(y + 1, height);
        (x, y, right - x, bottom - y)
    }
}

pub(super) fn render(
    jpeg: Bytes,
    region: Option<&Value>,
    max_width: u64,
    quality: Option<u8>,
) -> Result<(Vec<u8>, Value), ApiError> {
    let region = Region::from_json(region)?;
    let decoder = Decoder::open(&jpeg).map_err(ApiError::device)?;
    let (x, y, crop_w, crop_h) = region.bounds(decoder.width, decoder.height);
    let width = crop_w.min(max_width.max(1) as usize);
    let height = (crop_h * width).div_ceil(crop_w).max(1);
    let source_size = (decoder.width, decoder.height);
    let image = if width == decoder.width && height == decoder.height && quality.is_none() {
        jpeg.to_vec()
    } else {
        let divisor = [8, 4, 2]
            .into_iter()
            .find(|d| crop_w / d >= width && crop_h / d >= height)
            .unwrap_or(1);
        let (pixels, decoded_w, decoded_h) =
            decoder.pixels(divisor, 3).map_err(ApiError::device)?;
        let (left, top, right, bottom) = (
            x / divisor,
            y / divisor,
            (x + crop_w).div_ceil(divisor).min(decoded_w),
            (y + crop_h).div_ceil(divisor).min(decoded_h),
        );
        let mut output = Vec::with_capacity(width * height * 3);
        for row in 0..height {
            let src_y = top + row * (bottom - top) / height;
            for column in 0..width {
                let src_x = left + column * (right - left) / width;
                let offset = (src_y * decoded_w + src_x) * 3;
                output.extend_from_slice(&pixels[offset..offset + 3]);
            }
        }
        let mut encoded = Vec::new();
        jpeg_encoder::Encoder::new(&mut encoded, quality.unwrap_or(70))
            .encode(
                &output,
                width as u16,
                height as u16,
                jpeg_encoder::ColorType::Rgb,
            )
            .map_err(|error| ApiError::device(io::Error::other(error)))?;
        encoded
    };
    let metadata = json!({"width":width,"height":height,"source_width":source_size.0,"source_height":source_size.1,
        "region":{"x":region.x,"y":region.y,"w":region.w,"h":region.h},"size":image.len()});
    Ok((image, metadata))
}

pub(super) fn difference_frame(jpeg: &[u8], region: Option<&Value>) -> Result<Vec<u8>, ApiError> {
    let region = Region::from_json(region)?;
    let (pixels, width, height) = Decoder::open(jpeg)
        .and_then(|decoder| decoder.pixels(8, 1))
        .map_err(ApiError::device)?;
    let (x, y, crop_w, crop_h) = region.bounds(width, height);
    let mut luma = Vec::with_capacity(crop_w * crop_h);
    for row in pixels.chunks_exact(width).skip(y).take(crop_h) {
        luma.extend_from_slice(&row[x..x + crop_w]);
    }
    Ok(luma)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_crop_scale_and_luma() {
        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 85)
            .encode(
                &vec![100; 73 * 45 * 3],
                73,
                45,
                jpeg_encoder::ColorType::Rgb,
            )
            .unwrap();
        let jpeg = Bytes::from(jpeg);
        let (untouched, meta) = render(jpeg.clone(), None, 1280, None).unwrap();
        assert_eq!(untouched, jpeg);
        assert_eq!(meta["width"], 73);
        let region = json!({"x":0.5,"y":0.25,"w":0.5,"h":0.5});
        let (rendered, meta) = render(jpeg.clone(), Some(&region), 18, Some(55)).unwrap();
        let image = Decoder::open(&rendered).unwrap();
        assert_eq!(image.width, 18);
        assert_eq!(meta["region"], region);
        assert_eq!(meta["source_height"], 45);
        assert!(!difference_frame(&jpeg, Some(&region)).unwrap().is_empty());
        assert_eq!(difference_frame(&jpeg, None).unwrap().len(), 10 * 6);
        assert!(render(
            jpeg,
            Some(&json!({"x":0.5,"y":0.0,"w":0.51,"h":1.0})),
            18,
            None
        )
        .is_err());
    }
}

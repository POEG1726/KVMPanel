//! Pre-rendered blurred colour blobs for the E2 background.
//!
//! The mockup blurs 55%-wide discs with `filter: blur(40px)` at 420 px; at 1080 px that is an
//! 831.6 px disc with a ~103 px gaussian. We render the disc once at quarter resolution (the
//! renderer scales it up) and only rebuild the pixels when the brightness changes.

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const DISC_RADIUS: f32 = 415.8;
const SIGMA: f32 = 103.0;
/// Full-size image edge: disc plus 3 sigma of blur on each side (matches `blob-size` in the .slint).
const FULL: f32 = 2.0 * (DISC_RADIUS + 3.0 * SIGMA);
const RES: u32 = 362;

pub const COLORS: [[u8; 3]; 3] = [[0x1e, 0x6f, 0xd9], [0x1f, 0xa3, 0x6b], [0x8a, 0x4f, 0xd8]];

/// Abramowitz-Stegun 7.1.26 erfc approximation.
fn erfc(x: f32) -> f32 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * z);
    let y = t * (0.254_829_6 + t * (-0.284_496_7 + t * (1.421_413_7 + t * (-1.453_152 + t * 1.061_405_4))));
    let r = y * (-z * z).exp();
    if x >= 0.0 { r } else { 2.0 - r }
}

/// Alpha profile of a gaussian-blurred disc, computed once.
pub struct Profile(Vec<f32>);

impl Profile {
    pub fn new() -> Self {
        let scale = FULL / RES as f32;
        let c = RES as f32 / 2.0;
        let mut v = Vec::with_capacity((RES * RES) as usize);
        for y in 0..RES {
            for x in 0..RES {
                let dx = (x as f32 + 0.5 - c) * scale;
                let dy = (y as f32 + 0.5 - c) * scale;
                let r = (dx * dx + dy * dy).sqrt();
                v.push(0.5 * erfc((r - DISC_RADIUS) / (SIGMA * std::f32::consts::SQRT_2)));
            }
        }
        Profile(v)
    }

    /// Blob image in `color` with overall opacity `intensity` (the mockup's `--int`).
    pub fn image(&self, color: [u8; 3], intensity: f32) -> Image {
        let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(RES, RES);
        for (px, a) in buf.make_mut_slice().iter_mut().zip(&self.0) {
            *px = Rgba8Pixel { r: color[0], g: color[1], b: color[2], a: (a * intensity * 255.0) as u8 };
        }
        Image::from_rgba8(buf)
    }
}

use axum::body::Bytes;
use jpeg_encoder::{ColorType, Encoder};
use std::{
    collections::VecDeque,
    io,
    sync::{Mutex, OnceLock},
};
pub(super) fn pattern(index: usize, quality: u8) -> io::Result<Bytes> {
    type CachedFrame = ((usize, u8), Bytes);
    static CACHE: OnceLock<Mutex<VecDeque<CachedFrame>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    let key = (index % 8, quality);
    if let Some((_, bytes)) = cache.iter().find(|p| p.0 == key) {
        return Ok(bytes.clone());
    }
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    let mut rgb = vec![0; WIDTH * HEIGHT * 3];
    for (y, row) in rgb.chunks_exact_mut(WIDTH * 3).enumerate() {
        let green = 61 + (y * 60 / HEIGHT) as u8;
        for (x, pixel) in row.chunks_exact_mut(3).enumerate() {
            let rgb = if (key.0 * 240..key.0 * 240 + 120).contains(&x) {
                [94, 196, 255]
            } else {
                [15 + (x * 40 / WIDTH) as u8, green, 110]
            };
            pixel.copy_from_slice(&rgb);
        }
    }
    let mut bytes = Vec::new();
    Encoder::new(&mut bytes, quality)
        .encode(&rgb, WIDTH as u16, HEIGHT as u16, ColorType::Rgb)
        .map_err(io::Error::other)?;
    let bytes = Bytes::from(bytes);
    if cache.len() == 16 {
        cache.pop_front();
    }
    cache.push_back((key, bytes.clone()));
    Ok(bytes)
}

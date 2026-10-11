//! The app icon, drawn from code so every size stays sharp and there is no
//! binary asset to keep in sync: a page with ruled lines and a shield.
//! Used by the window at runtime and by `build.rs` for the Windows `.ico`.
//! Standard library only, because the build script compiles this file too.

type Rgba = [f32; 4];

const BACKGROUND_TOP: Rgba = [64.0, 132.0, 240.0, 255.0];
const BACKGROUND_BOTTOM: Rgba = [30.0, 64.0, 175.0, 255.0];
const PAGE: Rgba = [248.0, 250.0, 252.0, 255.0];
const RULE: Rgba = [148.0, 163.0, 184.0, 255.0];
const SHIELD: Rgba = [245.0, 158.0, 11.0, 255.0];
const WHITE: Rgba = [255.0, 255.0, 255.0, 255.0];

fn length(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// Signed distance to a rounded rectangle (negative inside).
fn rounded_box(p: (f32, f32), center: (f32, f32), half: (f32, f32), radius: f32) -> f32 {
    let qx = (p.0 - center.0).abs() - half.0 + radius;
    let qy = (p.1 - center.1).abs() - half.1 + radius;
    length(qx.max(0.0), qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

/// Signed distance to a line segment with round caps of `radius`.
fn capsule(p: (f32, f32), a: (f32, f32), b: (f32, f32), radius: f32) -> f32 {
    let (px, py) = (p.0 - a.0, p.1 - a.1);
    let (bx, by) = (b.0 - a.0, b.1 - a.1);
    let t = ((px * bx + py * by) / (bx * bx + by * by)).clamp(0.0, 1.0);
    length(px - bx * t, py - by * t) - radius
}

/// Signed distance to a closed polygon (Inigo Quilez's formulation).
fn polygon(p: (f32, f32), points: &[(f32, f32)]) -> f32 {
    let mut distance = f32::MAX;
    let mut sign = 1.0;
    let mut previous = points[points.len() - 1];
    for &current in points {
        let (ex, ey) = (previous.0 - current.0, previous.1 - current.1);
        let (wx, wy) = (p.0 - current.0, p.1 - current.1);
        let t = ((wx * ex + wy * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        distance = distance.min(length(wx - ex * t, wy - ey * t));
        let above = p.1 >= current.1;
        let below = p.1 < previous.1;
        let cross = ex * wy > ey * wx;
        if (above && below && cross) || (!above && !below && !cross) {
            sign = -sign;
        }
        previous = current;
    }
    sign * distance
}

/// The icon's colour at a point in unit coordinates (y down).
fn shade(x: f32, y: f32) -> Option<Rgba> {
    let p = (x, y);
    if rounded_box(p, (0.5, 0.5), (0.44, 0.44), 0.2) > 0.0 {
        return None;
    }
    let t = ((y - 0.06) / 0.88).clamp(0.0, 1.0);
    let mut color = [0.0; 4];
    for (channel, value) in color.iter_mut().enumerate() {
        *value =
            BACKGROUND_TOP[channel] + (BACKGROUND_BOTTOM[channel] - BACKGROUND_TOP[channel]) * t;
    }
    if rounded_box(p, (0.44, 0.45), (0.22, 0.28), 0.05) < 0.0 {
        color = PAGE;
        for (row, end) in [(0.30, 0.56), (0.39, 0.56), (0.48, 0.47)] {
            if capsule(p, (0.30, row), (end, row), 0.018) < 0.0 {
                color = RULE;
            }
        }
    }
    let scale = 0.19;
    let local = ((x - 0.67) / scale, (y - 0.67) / scale);
    let shield = polygon(
        local,
        &[
            (0.0, -1.0),
            (0.88, -0.68),
            (0.8, 0.2),
            (0.0, 1.0),
            (-0.8, 0.2),
            (-0.88, -0.68),
        ],
    );
    if shield < 0.22 {
        color = WHITE;
    }
    if shield < 0.0 {
        color = SHIELD;
        let check = capsule(local, (-0.38, 0.0), (-0.1, 0.3), 0.13).min(capsule(
            local,
            (-0.1, 0.3),
            (0.42, -0.32),
            0.13,
        ));
        if check < 0.0 {
            color = WHITE;
        }
    }
    Some(color)
}

/// Straight-alpha RGBA pixels, row by row, with 4×4 supersampling.
pub fn rgba(size: u32) -> Vec<u8> {
    const SAMPLES: u32 = 4;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for row in 0..size {
        for column in 0..size {
            let mut sum = [0.0f32; 4];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (column as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    let y = (row as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    if let Some(color) = shade(x, y) {
                        let alpha = color[3] / 255.0;
                        for channel in 0..3 {
                            sum[channel] += color[channel] * alpha;
                        }
                        sum[3] += alpha;
                    }
                }
            }
            let coverage = sum[3] / (SAMPLES * SAMPLES) as f32;
            for channel in 0..3 {
                let value = if sum[3] > 0.0 {
                    sum[channel] / sum[3]
                } else {
                    0.0
                };
                pixels.push(value.round() as u8);
            }
            pixels.push((coverage * 255.0).round() as u8);
        }
    }
    pixels
}

/// A Windows `.ico` with the standard sizes, as 32-bit BMP images.
/// Called by `build.rs`; the app itself only needs [`rgba`].
#[cfg_attr(not(test), allow(dead_code))]
pub fn ico() -> Vec<u8> {
    const SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];
    let images: Vec<Vec<u8>> = SIZES.iter().map(|&size| bmp_image(size)).collect();
    let mut file = Vec::new();
    file.extend_from_slice(&[0, 0, 1, 0]);
    file.extend_from_slice(&(SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * SIZES.len() as u32;
    for (size, image) in SIZES.iter().zip(&images) {
        // A dimension byte of 0 means 256.
        let dimension = if *size >= 256 { 0 } else { *size as u8 };
        file.extend_from_slice(&[dimension, dimension, 0, 0]);
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&32u16.to_le_bytes());
        file.extend_from_slice(&(image.len() as u32).to_le_bytes());
        file.extend_from_slice(&offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        file.extend_from_slice(&image);
    }
    file
}

/// A BITMAPINFOHEADER image: bottom-up BGRA rows, then an empty AND mask
/// (transparency comes from the alpha channel).
#[cfg_attr(not(test), allow(dead_code))]
fn bmp_image(size: u32) -> Vec<u8> {
    let pixels = rgba(size);
    let mask_row = size.div_ceil(32) * 4;
    let mut image = Vec::new();
    for value in [40u32, size, size * 2] {
        image.extend_from_slice(&value.to_le_bytes());
    }
    image.extend_from_slice(&1u16.to_le_bytes());
    image.extend_from_slice(&32u16.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&(size * size * 4 + mask_row * size).to_le_bytes());
    image.extend_from_slice(&[0; 16]);
    for row in (0..size).rev() {
        for column in 0..size {
            let index = ((row * size + column) * 4) as usize;
            let [r, g, b, a] = [
                pixels[index],
                pixels[index + 1],
                pixels[index + 2],
                pixels[index + 3],
            ];
            image.extend_from_slice(&[b, g, r, a]);
        }
    }
    image.extend(std::iter::repeat_n(0, (mask_row * size) as usize));
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_transparent_corners_and_an_opaque_centre() {
        let size = 64;
        let pixels = rgba(size);
        assert_eq!(pixels.len(), (size * size * 4) as usize);
        assert_eq!(pixels[3], 0, "Rounded corners are transparent");
        let centre = ((size / 2 * size + size / 2) * 4) as usize;
        assert_eq!(pixels[centre + 3], 255);
    }

    #[test]
    fn ico_lists_every_size_with_consistent_offsets() {
        let file = ico();
        assert_eq!(&file[..4], &[0, 0, 1, 0]);
        let count = u16::from_le_bytes([file[4], file[5]]) as usize;
        assert_eq!(count, 7);
        for entry in 0..count {
            let base = 6 + entry * 16;
            let read = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap());
            let (bytes, offset) = (read(base + 8) as usize, read(base + 12) as usize);
            assert!(offset + bytes <= file.len());
            assert_eq!(
                read(offset),
                40,
                "Each image starts with a BITMAPINFOHEADER"
            );
        }
    }
}

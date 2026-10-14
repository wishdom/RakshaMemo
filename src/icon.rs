//! The app icon, drawn from code so every size stays sharp and there is no
//! binary asset to keep in sync: a lotus mandala, the flower seen from above
//! with two rings of eight gold petals around a seed pod that holds a keyhole,
//! against a lamp-lit indigo night. Used by the window and the app's logo at
//! runtime, and by `build.rs` for the Windows `.ico`. Standard library only,
//! because the build script compiles this file too.

type Rgba = [f32; 4];
type Point = (f32, f32);

const NIGHT_TOP: Rgba = [58.0, 34.0, 112.0, 255.0];
const NIGHT_BOTTOM: Rgba = [20.0, 12.0, 44.0, 255.0];
const LAMP_GLOW: Rgba = [214.0, 106.0, 38.0, 255.0];
const GOLD_TOP: Rgba = [252.0, 214.0, 122.0, 255.0];
const GOLD_BOTTOM: Rgba = [204.0, 136.0, 42.0, 255.0];
const GOLD_SHADOW: Rgba = [150.0, 92.0, 26.0, 255.0];
const SANCTUM: Rgba = [26.0, 14.0, 50.0, 255.0];

const CENTER: Point = (0.5, 0.5);
/// The dark outline that separates overlapping petals.
const PETAL_GAP: f32 = 0.011;
const POD_RADIUS: f32 = 0.125;
const POD_RIM: f32 = 0.013;

fn length(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let mut color = a;
    for (channel, value) in color.iter_mut().enumerate() {
        *value += (b[channel] - a[channel]) * t;
    }
    color
}

/// Signed distance to a rounded rectangle (negative inside).
fn rounded_box(p: Point, center: Point, half: Point, radius: f32) -> f32 {
    let qx = (p.0 - center.0).abs() - half.0 + radius;
    let qy = (p.1 - center.1).abs() - half.1 + radius;
    length(qx.max(0.0), qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

/// Signed distance to a closed polygon (Inigo Quilez's formulation).
fn polygon(p: Point, points: &[Point]) -> f32 {
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

/// A keyhole of height about `size`: a round head above a flared slot.
fn keyhole(p: Point, top: Point, size: f32) -> f32 {
    let head = length(p.0 - top.0, p.1 - top.1) - 0.36 * size;
    let slot = polygon(
        p,
        &[
            (top.0 - 0.2 * size, top.1 + 0.05 * size),
            (top.0 + 0.2 * size, top.1 + 0.05 * size),
            (top.0 + 0.34 * size, top.1 + 0.85 * size),
            (top.0 - 0.34 * size, top.1 + 0.85 * size),
        ],
    );
    head.min(slot)
}

/// A petal outline from `base` along `angle` (radians clockwise from up):
/// full near the base, tapering to a point.
fn petal(base: Point, angle: f32, length: f32, width: f32) -> Vec<Point> {
    const STEPS: usize = 16;
    let along = (angle.sin(), -angle.cos());
    let across = (-along.1, along.0);
    let point = |t: f32, side: f32| {
        // sin(π) rounds to a tiny negative number, which powf turns into NaN.
        let swell = (std::f32::consts::PI * t).sin().max(0.0).powf(0.8);
        let half = width * swell * (1.0 - 0.3 * t);
        (
            base.0 + along.0 * length * t + across.0 * half * side,
            base.1 + along.1 * length * t + across.1 * half * side,
        )
    };
    let t = |step: usize| step as f32 / STEPS as f32;
    let mut points: Vec<Point> = (0..=STEPS).map(|step| point(t(step), 1.0)).collect();
    points.extend((1..STEPS).rev().map(|step| point(t(step), -1.0)));
    points
}

/// Eight petals radiating from `start` away from the centre, the first at
/// `offset` degrees clockwise from straight up.
fn petal_ring(offset: f32, start: f32, length: f32, width: f32) -> Vec<Vec<Point>> {
    (0..8)
        .map(|index| {
            let angle = (index as f32 * 45.0 + offset).to_radians();
            let base = (
                CENTER.0 + angle.sin() * start,
                CENTER.1 - angle.cos() * start,
            );
            petal(base, angle, length, width)
        })
        .collect()
}

/// The petal outlines, built once per image.
struct Lotus {
    /// The outer ring, set between the inner petals and shaded deeper.
    outer: Vec<Vec<Point>>,
    inner: Vec<Vec<Point>>,
}

impl Lotus {
    fn new() -> Self {
        Self {
            outer: petal_ring(22.5, 0.1, 0.29, 0.09),
            inner: petal_ring(0.0, 0.08, 0.24, 0.095),
        }
    }

    /// The icon's colour at a point in unit coordinates (y down).
    fn shade(&self, x: f32, y: f32) -> Option<Rgba> {
        let p = (x, y);
        if rounded_box(p, CENTER, (0.44, 0.44), 0.2) > 0.0 {
            return None;
        }
        // Night sky, warmed by lamplight behind the flower.
        let mut color = mix(NIGHT_TOP, NIGHT_BOTTOM, (y - 0.06) / 0.88);
        let glow = (1.0 - length(x - CENTER.0, y - CENTER.1) / 0.42).max(0.0);
        color = mix(color, LAMP_GLOW, glow * glow * 0.7);

        let gold = mix(GOLD_TOP, GOLD_BOTTOM, (y - 0.15) / 0.7);
        let outer = mix(gold, GOLD_SHADOW, 0.45);
        // Back to front: each petal's outline separates it from those behind.
        let rings = [(&self.outer, outer), (&self.inner, gold)];
        let mut covered = false;
        for (ring, fill) in rings {
            for outline in ring {
                let distance = polygon(p, outline);
                if distance < 0.0 {
                    color = fill;
                    covered = true;
                } else if distance < PETAL_GAP && covered {
                    color = SANCTUM;
                }
            }
        }
        // The seed pod, rimmed in shadow, holds the keyhole.
        let pod = length(x - CENTER.0, y - CENTER.1);
        if pod < POD_RADIUS {
            color = if pod > POD_RADIUS - POD_RIM {
                SANCTUM
            } else {
                gold
            };
        }
        if keyhole(p, (CENTER.0, CENTER.1 - 0.015), 0.085) < 0.0 {
            color = SANCTUM;
        }
        Some(color)
    }
}

/// Straight-alpha RGBA pixels, row by row, with 4×4 supersampling.
pub fn rgba(size: u32) -> Vec<u8> {
    const SAMPLES: u32 = 4;
    let lotus = Lotus::new();
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for row in 0..size {
        for column in 0..size {
            let mut sum = [0.0f32; 4];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (column as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    let y = (row as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    if let Some(color) = lotus.shade(x, y) {
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

    fn pixel(pixels: &[u8], size: u32, x: f32, y: f32) -> [u8; 4] {
        let column = (x * size as f32) as u32;
        let row = (y * size as f32) as u32;
        let index = ((row * size + column) * 4) as usize;
        pixels[index..index + 4].try_into().unwrap()
    }

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
    fn petals_are_gold_and_the_keyhole_is_dark() {
        let size = 128;
        let pixels = rgba(size);
        // The top inner petal, halfway out.
        let petal = pixel(&pixels, size, 0.5, 0.25);
        assert!(
            petal[0] > 200 && petal[1] > 150 && petal[2] < 140,
            "{petal:?}"
        );
        let keyhole = pixel(&pixels, size, 0.5, 0.5);
        assert!(keyhole[0] < 60 && keyhole[1] < 60, "{keyhole:?}");
        let sky = pixel(&pixels, size, 0.18, 0.18);
        assert!(sky[2] > sky[1], "The night sky is indigo: {sky:?}");
    }

    #[test]
    fn petal_outlines_are_closed_and_finite() {
        let lotus = Lotus::new();
        for outline in lotus.outer.iter().chain(&lotus.inner) {
            assert_eq!(outline.len(), 32);
            assert!(outline.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
        }
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

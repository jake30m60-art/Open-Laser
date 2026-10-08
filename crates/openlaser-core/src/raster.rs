// SPDX-License-Identifier: GPL-3.0-or-later

//! Raster images for engraving. An image is a grid of gray levels laid on
//! the drawing as a parallelogram: its top-left corner and the vectors along
//! its top edge and down its left edge. Placing a part moves, turns and
//! mirrors those three like any other geometry, so a copy of an image part
//! engraves its own copy of the picture. The pixels are shared, never
//! copied, and stored as base64 so a part file stays text.

use crate::geometry::{Point, Transform};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// The most pixels one image may hold.
pub const MAX_PIXELS: u64 = 25_000_000;

/// A gray image on the drawing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Raster {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// Gray levels row by row from the top, 0 black to 255 white.
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub pixels: Arc<[u8]>,
    /// Where the image's top-left corner lies, in millimetres.
    pub origin: Point,
    /// From the top-left corner to the top-right one.
    pub across: Point,
    /// From the top-left corner to the bottom-left one.
    pub down: Point,
}

impl Raster {
    /// An upright image `size` millimetres wide and high with its lower
    /// left corner at the origin, as the frame of an imported image lies.
    pub fn upright(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        size: [f64; 2],
    ) -> Result<Self, String> {
        let raster = Self {
            width,
            height,
            pixels: pixels.into(),
            origin: Point::new(0., size[1]),
            across: Point::new(size[0], 0.),
            down: Point::new(0., -size[1]),
        };
        raster.check()?;
        Ok(raster)
    }

    /// Whether the image is whole: as many pixels as its size says, within
    /// [`MAX_PIXELS`], on a parallelogram that is not flat.
    pub fn check(&self) -> Result<(), String> {
        let count = u64::from(self.width) * u64::from(self.height);
        if self.width == 0 || self.height == 0 || count > MAX_PIXELS {
            return Err(format!("an image has 1 to {MAX_PIXELS} pixels"));
        }
        if usize::try_from(count).ok() != Some(self.pixels.len()) {
            return Err("the image's pixels do not match its size".into());
        }
        let finite = [self.origin, self.across, self.down].iter().all(|p| p.is_finite());
        if !finite || self.across.cross(self.down).abs() < 1e-9 {
            return Err("the image needs a size".into());
        }
        Ok(())
    }

    /// The image placed by `transform`; the pixels are shared.
    #[must_use]
    pub fn transformed(&self, transform: &Transform) -> Self {
        let [a, b, c, d, ..] = transform.0;
        let linear = |v: Point| Point::new(a * v.x + c * v.y, b * v.x + d * v.y);
        Self {
            origin: transform.apply(self.origin),
            across: linear(self.across),
            down: linear(self.down),
            ..self.clone()
        }
    }

    /// The four corners: top-left, top-right, bottom-right, bottom-left.
    #[must_use]
    pub fn corners(&self) -> [Point; 4] {
        let o = self.origin;
        [o, o + self.across, o + self.across + self.down, o + self.down]
    }

    /// The size of one pixel along the top edge and down the side, in
    /// millimetres.
    #[must_use]
    pub fn pixel_size(&self) -> [f64; 2] {
        [self.across.norm() / f64::from(self.width), self.down.norm() / f64::from(self.height)]
    }

    /// The gray level at `point`, 0 black to 1 white, blended between the
    /// four nearest pixels; `None` outside the image.
    #[must_use]
    #[allow(
        clippy::many_single_char_names,
        reason = "u, v and the corner grays are the bilinear formula"
    )]
    pub fn gray_at(&self, point: Point) -> Option<f64> {
        let rel = point - self.origin;
        let det = self.across.cross(self.down);
        // Solve rel = u·across + v·down.
        let u = rel.cross(self.down) / det;
        let v = self.across.cross(rel) / det;
        if !(-1e-9..=1. + 1e-9).contains(&u) || !(-1e-9..=1. + 1e-9).contains(&v) {
            return None;
        }
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let x = (u * w - 0.5).clamp(0., w - 1.);
        let y = (v * h - 0.5).clamp(0., h - 1.);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to the image"
        )]
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) =
            ((x0 + 1).min(self.width as usize - 1), (y0 + 1).min(self.height as usize - 1));
        let (fx, fy) = (x - x.floor(), y - y.floor());
        let at = |x: usize, y: usize| f64::from(self.pixels[y * self.width as usize + x]);
        let top = at(x0, y0) * (1. - fx) + at(x1, y0) * fx;
        let bottom = at(x0, y1) * (1. - fx) + at(x1, y1) * fx;
        Some((top * (1. - fy) + bottom * fy) / 255.)
    }
}

impl Raster {
    /// The image as an 8-bit gray Windows bitmap, which every browser shows.
    #[must_use]
    pub fn bitmap(&self) -> Vec<u8> {
        let (w, h) = (self.width as usize, self.height as usize);
        let stride = w.div_ceil(4) * 4;
        let offset = 14 + 40 + 256 * 4;
        let size = offset + stride * h;
        let word = |v: usize| u32::try_from(v).unwrap_or(u32::MAX).to_le_bytes();
        let mut out = Vec::with_capacity(size);
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&word(size));
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&word(offset));
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&8u16.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&word(stride * h));
        out.extend_from_slice(&2835u32.to_le_bytes());
        out.extend_from_slice(&2835u32.to_le_bytes());
        out.extend_from_slice(&256u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        for gray in 0..=255u8 {
            out.extend_from_slice(&[gray, gray, gray, 0]);
        }
        // Rows run from the bottom up.
        for row in (0..h).rev() {
            out.extend_from_slice(&self.pixels[row * w..(row + 1) * w]);
            out.resize(out.len() + stride - w, 0);
        }
        out
    }
}

/// A raster laid on one contour of a placed drawing: the frame it engraves.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedRaster {
    /// The placed contour that frames the image.
    pub contour: usize,
    /// The image, placed.
    pub raster: Raster,
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes standard base64, padded or not; whitespace is not allowed.
pub fn unbase64(text: &str) -> Result<Vec<u8>, String> {
    let digits = text.trim_end_matches('=').as_bytes();
    let value = |c: u8| -> Result<u32, String> {
        match c {
            b'A'..=b'Z' => Ok(u32::from(c - b'A')),
            b'a'..=b'z' => Ok(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Ok(u32::from(c - b'0') + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("not base64".into()),
        }
    };
    if digits.len() % 4 == 1 {
        return Err("not base64".into());
    }
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * i);
        }
        #[allow(clippy::cast_possible_truncation, reason = "bytes of a 24-bit group")]
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..chunk.len() - 1]);
    }
    Ok(out)
}

fn to_base64<S: Serializer>(pixels: &Arc<[u8]>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&base64(pixels))
}

fn from_base64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<[u8]>, D::Error> {
    let text = String::deserialize(deserializer)?;
    unbase64(&text).map(Into::into).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for bytes in
            [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar", &[0, 255, 128, 7]]
        {
            assert_eq!(unbase64(&base64(bytes)).unwrap(), bytes);
        }
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert!(unbase64("Zm9v!").is_err());
    }

    /// A 2×1 image, black on the left and white on the right, 20 by 10 mm.
    fn halves() -> Raster {
        Raster::upright(2, 1, vec![0, 255], [20., 10.]).unwrap()
    }

    #[test]
    fn gray_follows_the_image_and_its_placement() {
        let image = halves();
        assert_eq!(image.gray_at(Point::new(2., 5.)), Some(0.));
        assert_eq!(image.gray_at(Point::new(18., 5.)), Some(1.));
        assert!((image.gray_at(Point::new(10., 5.)).unwrap() - 0.5).abs() < 1e-9);
        assert_eq!(image.gray_at(Point::new(21., 5.)), None);
        // Mirrored about the Y axis, black lands on the right.
        let mirrored = image.transformed(&Transform([-1., 0., 0., 1., 0., 0.]));
        assert_eq!(mirrored.gray_at(Point::new(-2., 5.)), Some(0.));
        assert_eq!(mirrored.gray_at(Point::new(-18., 5.)), Some(1.));
        // Turned a quarter, the black half lies below.
        let turned = image.transformed(&Transform([0., 1., -1., 0., 0., 0.]));
        assert_eq!(turned.gray_at(Point::new(-5., 2.)), Some(0.));
        assert_eq!(turned.gray_at(Point::new(-5., 18.)), Some(1.));
    }

    #[test]
    fn a_broken_image_is_refused() {
        assert!(Raster::upright(2, 2, vec![0; 3], [1., 1.]).is_err());
        assert!(Raster::upright(0, 2, Vec::new(), [1., 1.]).is_err());
        assert!(Raster::upright(1, 1, vec![0], [0., 1.]).is_err());
    }

    #[test]
    fn a_bitmap_holds_the_rows_bottom_up_and_padded() {
        let image = Raster::upright(3, 2, vec![1, 2, 3, 4, 5, 6], [3., 2.]).unwrap();
        let bmp = image.bitmap();
        assert_eq!(&bmp[..2], b"BM");
        let offset = 14 + 40 + 1024;
        assert_eq!(bmp.len(), offset + 2 * 4);
        assert_eq!(&bmp[offset..], &[4, 5, 6, 0, 1, 2, 3, 0]);
    }

    #[test]
    fn it_survives_a_file() {
        let image = halves();
        let text = serde_json::to_string(&image).unwrap();
        assert!(text.contains("\"pixels\":\"AP8=\""));
        assert_eq!(serde_json::from_str::<Raster>(&text).unwrap(), image);
    }
}

// SPDX-License-Identifier: GPL-2.0-only
//! A plain RGBA image.

use crate::{Error, Result};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA, 4 bytes per pixel, no padding.
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Self {
        Image {
            width,
            height,
            rgba: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    pub fn set(&mut self, x: u32, y: u32, p: [u8; 4]) {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.rgba[i..i + 4].copy_from_slice(&p);
    }

    pub fn write_png(&self, path: &Path) -> Result<()> {
        let io = |e: std::io::Error| Error::Io {
            path: path.to_owned(),
            source: e,
        };
        let file = std::fs::File::create(path).map_err(io)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let png_err = |e: png::EncodingError| Error::Other(format!("{}: {e}", path.display()));
        let mut w = enc.write_header().map_err(png_err)?;
        w.write_image_data(&self.rgba).map_err(png_err)?;
        w.finish().map_err(png_err)
    }
}

//! PNG screenshots of the 800 × 600 frame.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub fn write_png(path: &Path, (w, h): (u32, u32), rgba: &[u8]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}

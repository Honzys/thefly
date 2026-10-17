//! `.irc` sprite/background files: a 16-byte header followed by a Windows BMP.

use crate::Error;

/// Bytes before the embedded BMP.
const HEADER_LEN: usize = 16;
/// Colour that the game treats as transparent.
pub const COLOR_KEY: [u8; 3] = [0xff, 0x00, 0xff];
/// Largest accepted side, to bound allocations on hostile input.
const MAX_SIDE: u32 = 16_384;

/// Decoded top-down RGBA8 image; pixels equal to [`COLOR_KEY`] have alpha 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Irc {
    /// Header u32 at offset 0; equals the BMP `bfSize` field in every shipped file.
    pub declared_len: u32,
    /// Header f64 at offset 8; a function of `declared_len` only, otherwise unused.
    pub aux: f64,
    /// Bits per pixel of the source bitmap (4, 8, 16 or 24).
    pub bits_per_pixel: u16,
    pub image: Image,
}

/// Little-endian fixed-size read at `at`, or `Truncated`.
fn le<const N: usize>(b: &[u8], at: usize) -> Result<[u8; N], Error> {
    let end = at + N;
    let slice = b.get(at..end).ok_or(Error::Truncated {
        needed: end,
        have: b.len(),
    })?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    Ok(out)
}

/// Expand a 5-bit channel to 8 bits so that 0x1F maps to 0xFF.
fn expand5(c: u16) -> u8 {
    let c = (c & 0x1f) as u8;
    (c << 3) | (c >> 2)
}

fn rgb_at(row: &[u8], x: usize, bpp: u16, palette: &[[u8; 3]]) -> Result<[u8; 3], Error> {
    let bad = Error::Invalid("palette index out of range");
    match bpp {
        4 => {
            let byte = row[x / 2];
            let idx = if x % 2 == 0 { byte >> 4 } else { byte & 0x0f };
            palette.get(usize::from(idx)).copied().ok_or(bad)
        }
        8 => palette.get(usize::from(row[x])).copied().ok_or(bad),
        16 => {
            let v = u16::from_le_bytes([row[2 * x], row[2 * x + 1]]);
            Ok([expand5(v >> 10), expand5(v >> 5), expand5(v)])
        }
        _ => Ok([row[3 * x + 2], row[3 * x + 1], row[3 * x]]),
    }
}

pub fn parse(bytes: &[u8]) -> Result<Irc, Error> {
    let declared_len = u32::from_le_bytes(le(bytes, 0)?);
    let aux = f64::from_le_bytes(le(bytes, 8)?);
    let bmp = bytes.get(HEADER_LEN..).unwrap_or_default();
    if le::<2>(bmp, 0)? != *b"BM" {
        return Err(Error::BadMagic);
    }
    let data_off = u32::from_le_bytes(le(bmp, 10)?) as usize;
    if u32::from_le_bytes(le(bmp, 14)?) != 40 {
        return Err(Error::Unsupported("DIB header other than BITMAPINFOHEADER"));
    }
    let w = i32::from_le_bytes(le(bmp, 18)?);
    let h = i32::from_le_bytes(le(bmp, 22)?);
    let bpp = u16::from_le_bytes(le(bmp, 28)?);
    if u32::from_le_bytes(le(bmp, 30)?) != 0 {
        return Err(Error::Unsupported("compressed bitmap"));
    }
    let clr_used = u32::from_le_bytes(le(bmp, 46)?) as usize;
    if !matches!(bpp, 4 | 8 | 16 | 24) {
        return Err(Error::Unsupported("bit depth"));
    }
    let side_ok = |v: i32| v != 0 && v.unsigned_abs() <= MAX_SIDE;
    if w <= 0 || !side_ok(w) || !side_ok(h) {
        return Err(Error::Invalid("image dimensions"));
    }
    let (width, height) = (w as usize, h.unsigned_abs() as usize);

    let mut palette = Vec::new();
    if bpp <= 8 {
        let n = if clr_used == 0 { 1 << bpp } else { clr_used };
        if n > 1 << bpp {
            return Err(Error::Invalid("palette larger than bit depth allows"));
        }
        for i in 0..n {
            let [b, g, r, _] = le(bmp, 54 + 4 * i)?;
            palette.push([r, g, b]);
        }
    }

    let stride = (width * usize::from(bpp)).div_ceil(32) * 4;
    let pixels = bmp
        .get(data_off..)
        .and_then(|d| d.get(..stride * height))
        .ok_or(Error::Truncated {
            needed: HEADER_LEN + data_off + stride * height,
            have: bytes.len(),
        })?;

    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let src = if h > 0 { height - 1 - y } else { y };
        let row = &pixels[src * stride..(src + 1) * stride];
        for x in 0..width {
            let rgb = rgb_at(row, x, bpp, &palette)?;
            rgba.extend_from_slice(&rgb);
            rgba.push(if rgb == COLOR_KEY { 0 } else { 255 });
        }
    }
    let image = Image {
        width: width as u32,
        height: height as u32,
        rgba,
    };
    Ok(Irc {
        declared_len,
        aux,
        bits_per_pixel: bpp,
        image,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an irc file. `rows` are bottom-up, already padded to 4 bytes.
    fn build(w: i32, h: i32, bpp: u16, palette: &[[u8; 3]], rows: &[u8]) -> Vec<u8> {
        let pal_len = palette.len() * 4;
        let off = 14 + 40 + pal_len;
        let size = (off + rows.len()) as u32;
        let mut v = Vec::new();
        v.extend(size.to_le_bytes());
        v.extend(0u32.to_le_bytes());
        v.extend(1.5f64.to_le_bytes());
        v.extend(b"BM");
        v.extend(size.to_le_bytes());
        v.extend(0u32.to_le_bytes());
        v.extend((off as u32).to_le_bytes());
        v.extend(40u32.to_le_bytes());
        v.extend(w.to_le_bytes());
        v.extend(h.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(bpp.to_le_bytes());
        v.extend(0u32.to_le_bytes());
        v.extend((rows.len() as u32).to_le_bytes());
        v.extend([0u8; 8]);
        v.extend((palette.len() as u32).to_le_bytes());
        v.extend(0u32.to_le_bytes());
        for [r, g, b] in palette {
            v.extend([*b, *g, *r, 0]);
        }
        v.extend(rows);
        v.extend([0u8; 6]); // the shipped files carry trailing junk
        v
    }

    const MAGENTA: [u8; 3] = [255, 0, 255];
    const RED: [u8; 3] = [255, 0, 0];

    #[test]
    fn parses_8bpp_bottom_up_with_key_and_header_fields() {
        // 2x2: bottom row = [0,1], top row = [1,0]
        let f = build(2, 2, 8, &[MAGENTA, RED], &[0, 1, 0, 0, 1, 0, 0, 0]);
        let irc = parse(&f).unwrap();
        assert_eq!(irc.declared_len, (14 + 40 + 8 + 8) as u32);
        assert_eq!(irc.aux, 1.5);
        assert_eq!(irc.bits_per_pixel, 8);
        assert_eq!((irc.image.width, irc.image.height), (2, 2));
        let red = [255, 0, 0, 255];
        let none = [255, 0, 255, 0];
        // top row first: [red, key] then bottom: [key, red]
        assert_eq!(irc.image.rgba, [red, none, none, red].concat());
    }

    #[test]
    fn skips_row_padding() {
        // 3x1, stride 4
        let f = build(3, 1, 8, &[MAGENTA, RED], &[1, 1, 0, 9]);
        let irc = parse(&f).unwrap();
        assert_eq!(irc.image.rgba.len(), 12);
        assert_eq!(&irc.image.rgba[8..12], &[255, 0, 255, 0]);
    }

    #[test]
    fn parses_24bpp_bgr_and_keys_magenta() {
        let rows = [
            0, 0, 255, /* red */ 255, 0, 255, /* magenta */ 0, 0,
        ]; // 2 px + 2 pad
        let f = build(2, 1, 24, &[], &rows);
        let irc = parse(&f).unwrap();
        assert_eq!(irc.image.rgba, [255, 0, 0, 255, 255, 0, 255, 0]);
    }

    #[test]
    fn parses_4bpp_high_nibble_first() {
        let f = build(3, 1, 4, &[MAGENTA, RED], &[0x10, 0x10, 0, 0]);
        let irc = parse(&f).unwrap();
        let px: Vec<u8> = irc.image.rgba.chunks(4).map(|p| p[3]).collect();
        assert_eq!(px, [255, 0, 255]);
    }

    #[test]
    fn parses_16bpp_555() {
        let red = 0x7c00u16.to_le_bytes();
        let key = 0x7c1fu16.to_le_bytes();
        let f = build(2, 1, 16, &[], &[red[0], red[1], key[0], key[1]]);
        let irc = parse(&f).unwrap();
        assert_eq!(irc.image.rgba, [255, 0, 0, 255, 255, 0, 255, 0]);
    }

    #[test]
    fn top_down_bitmaps_keep_row_order() {
        let f = build(1, -2, 8, &[MAGENTA, RED], &[1, 0, 0, 0, 0, 0, 0, 0]);
        let irc = parse(&f).unwrap();
        assert_eq!(irc.image.height, 2);
        assert_eq!(irc.image.rgba[3], 255); // first stored row is index 1 = red
        assert_eq!(irc.image.rgba[7], 0);
    }

    #[test]
    fn rejects_short_header() {
        assert!(matches!(parse(&[0; 10]), Err(Error::Truncated { .. })));
    }

    #[test]
    fn rejects_missing_bm_marker() {
        let mut f = build(1, 1, 8, &[MAGENTA], &[0, 0, 0, 0]);
        f[16] = b'X';
        assert_eq!(parse(&f), Err(Error::BadMagic));
    }

    #[test]
    fn rejects_truncated_pixels() {
        let f = build(4, 4, 8, &[MAGENTA], &[0; 16]);
        assert!(matches!(
            parse(&f[..f.len() - 20]),
            Err(Error::Truncated { .. })
        ));
    }

    #[test]
    fn rejects_compression_and_odd_depths() {
        let mut f = build(1, 1, 8, &[MAGENTA], &[0; 4]);
        f[16 + 14 + 16..16 + 14 + 20].copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(parse(&f), Err(Error::Unsupported(_))));
        let f = build(1, 1, 32, &[], &[0; 4]);
        assert!(matches!(parse(&f), Err(Error::Unsupported(_))));
    }

    #[test]
    fn rejects_zero_and_huge_dimensions() {
        assert!(matches!(
            parse(&build(0, 1, 8, &[MAGENTA], &[])),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            parse(&build(i32::MAX, 1, 8, &[MAGENTA], &[0; 4])),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn rejects_palette_index_out_of_range() {
        let f = build(1, 1, 8, &[MAGENTA], &[5, 0, 0, 0]);
        assert!(matches!(parse(&f), Err(Error::Invalid(_))));
    }
}

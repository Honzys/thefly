//! Drawing by the game's logical sprite names (game-spec §9) with SR's frame cuts.

use thefly_platform::{Color, Error, FRect, Frame, Renderer};

use crate::assets::{self, Sheet};

pub const WHITE: Color = Color::RGB(255, 255, 255);
pub const BLACK: Color = Color::RGB(0, 0, 0);
/// Menu and dialog text (SR §2.2, §2.5).
pub const SELECTED: Color = Color::RGB(52, 255, 0);
pub const NORMAL: Color = Color::RGB(0, 168, 255);

/// Default outline offset of `FUN_0044daa6` (SR §1.1).
pub const OUTLINE_PX: f32 = 3.0;

pub struct Gfx<'g, 'a, 'tc> {
    pub r: &'g mut Renderer<'a, 'tc>,
}

fn sheet(name: &str) -> Result<Sheet, Error> {
    assets::sheet(name).ok_or_else(|| Error::Sdl(format!("unknown sprite {name:?}")))
}

fn frame(s: Sheet, index: u32) -> Frame {
    Frame {
        index,
        cols: s.cols,
        rows: s.rows,
    }
}

/// `0..=255` as the platform's `0..=1`.
pub fn a8(alpha: f32) -> f32 {
    (alpha / 255.0).clamp(0.0, 1.0)
}

impl Gfx<'_, '_, '_> {
    /// Frame size of `name` in pixels.
    pub fn size(&mut self, name: &str) -> Result<(f32, f32), Error> {
        let s = sheet(name)?;
        let (w, h) = self.r.sprite_size(s.file)?;
        let size = assets::frame_size(s, (w, h));
        Ok((size.w as f32, size.h as f32))
    }

    /// Frame `index` (0-based; SR's frame n is index n − 1) with its top-left at `(x, y)`.
    pub fn spr(&mut self, name: &str, x: f32, y: f32, index: u32) -> Result<(), Error> {
        self.spr_ex(name, x, y, index, 255.0, false)
    }

    pub fn spr_ex(
        &mut self,
        name: &str,
        x: f32,
        y: f32,
        index: u32,
        alpha: f32,
        flip: bool,
    ) -> Result<(), Error> {
        let s = sheet(name)?;
        self.r
            .draw_sprite(s.file, (x, y), frame(s, index), flip, a8(alpha))
    }

    /// Scaled about the frame's top-left (the D3D path of SR §1.1 scales from the sprite origin).
    pub fn spr_scaled(
        &mut self,
        name: &str,
        x: f32,
        y: f32,
        index: u32,
        scale: f32,
        alpha: f32,
    ) -> Result<(), Error> {
        let s = sheet(name)?;
        let (w, h) = self.r.sprite_size(s.file)?;
        let (sx, sy, fw, fh) = frame(s, index).src_rect((w, h));
        let src = FRect::new(sx as f32, sy as f32, fw as f32, fh as f32);
        let dst = FRect::new(x, y, fw as f32 * scale, fh as f32 * scale);
        self.r.draw_region(s.file, src, dst, false, a8(alpha))
    }

    /// Scaled about its centre `(cx, cy)`: puffs, shockwaves, particles.
    pub fn spr_centred(
        &mut self,
        name: &str,
        cx: f32,
        cy: f32,
        index: u32,
        scale: f32,
        alpha: f32,
    ) -> Result<(), Error> {
        let (w, h) = self.size(name)?;
        let (x, y) = (cx - w * scale / 2.0, cy - h * scale / 2.0);
        self.spr_scaled(name, x, y, index, scale, alpha)
    }

    /// A slice of a frame: source `(sx, sy, w, h)` relative to the frame, drawn unscaled at `(x, y)`.
    pub fn spr_part(
        &mut self,
        name: &str,
        index: u32,
        part: (f32, f32, f32, f32),
        x: f32,
        y: f32,
    ) -> Result<(), Error> {
        let s = sheet(name)?;
        let size = self.r.sprite_size(s.file)?;
        let (fx, fy, _, _) = frame(s, index).src_rect(size);
        let (sx, sy, w, h) = part;
        if w <= 0.0 || h <= 0.0 {
            return Ok(());
        }
        let src = FRect::new(fx as f32 + sx, fy as f32 + sy, w, h);
        self.r
            .draw_region(s.file, src, FRect::new(x, y, w, h), false, 1.0)
    }

    /// Frame `index` stretched onto `(x, y, w, h)`.
    pub fn spr_stretched(
        &mut self,
        name: &str,
        index: u32,
        (x, y, w, h): (f32, f32, f32, f32),
        alpha: f32,
    ) -> Result<(), Error> {
        let s = sheet(name)?;
        let size = self.r.sprite_size(s.file)?;
        let (sx, sy, fw, fh) = frame(s, index).src_rect(size);
        let src = FRect::new(sx as f32, sy as f32, fw as f32, fh as f32);
        self.r
            .draw_region(s.file, src, FRect::new(x, y, w, h), false, a8(alpha))
    }

    pub fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) -> Result<(), Error> {
        self.r.fill_rect(FRect::new(x, y, w, h), c)
    }

    /// Full-screen fade (SR §1.4): `color_shade` cell 1 (black) over the screen.
    pub fn fade(&mut self, alpha: f64) -> Result<(), Error> {
        self.shade(0, alpha as f32)
    }

    /// One `color_shade` cell stretched over the screen (fades, the meal tint). The source is
    /// inset so the linear filter does not bleed the neighbouring cells in.
    pub fn shade(&mut self, cell: u32, alpha: f32) -> Result<(), Error> {
        const INSET: f32 = 2.0;
        if alpha <= 0.0 {
            return Ok(());
        }
        let s = sheet("color_shade")?;
        let size = self.r.sprite_size(s.file)?;
        let (x, y, w, h) = frame(s, cell).src_rect(size);
        let src = FRect::new(
            x as f32 + INSET,
            y as f32 + INSET,
            w as f32 - 2.0 * INSET,
            h as f32 - 2.0 * INSET,
        );
        let dst = FRect::new(0.0, 0.0, 800.0, 600.0);
        self.r.draw_region(s.file, src, dst, false, a8(alpha))
    }

    pub fn text(
        &mut self,
        font: &str,
        size: u16,
        s: &str,
        x: f32,
        y: f32,
        c: Color,
    ) -> Result<(), Error> {
        self.r.draw_text(font, size, s, (x, y), c)
    }

    /// `s` centred on `cx`.
    pub fn text_centred(
        &mut self,
        font: &str,
        size: u16,
        s: &str,
        (cx, y): (f32, f32),
        c: Color,
    ) -> Result<(), Error> {
        let w = self.r.text_width(font, size, s)?;
        self.text(font, size, s, cx - w / 2.0, y, c)
    }

    /// `FUN_0044daa6`: four copies at (±d, ±d) in the outline colour, then the fill.
    pub fn text_outlined(
        &mut self,
        font: &str,
        size: u16,
        s: &str,
        (x, y): (f32, f32),
        (fill, outline): (Color, Color),
    ) -> Result<(), Error> {
        let d = OUTLINE_PX;
        for (dx, dy) in [(-d, -d), (d, -d), (-d, d), (d, d)] {
            self.text(font, size, s, x + dx, y + dy, outline)?;
        }
        self.text(font, size, s, x, y, fill)
    }

    pub fn line(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        w: u32,
        c: Color,
    ) -> Result<(), Error> {
        self.r.draw_line(from, to, w, c)
    }
}

/// GDI text centring as the original does it: each character assumed `size / 2` wide.
pub fn centred_x(len: usize, size: f32) -> f32 {
    (800.0 - len as f32 * size / 2.0) / 2.0
}

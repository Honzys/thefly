//! Drawing into the logical frame: sprites from `.irc` files, TTF text and filled rectangles.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};

use sdl3::pixels::PixelFormat;
use sdl3::render::{BlendMode, Canvas, Texture, TextureCreator};
use sdl3::ttf::{Font, Sdl3TtfContext};
use sdl3::video::{Window, WindowContext};

pub use sdl3::pixels::Color;
pub use sdl3::render::FRect;

use crate::error::{Error, sdl};

/// Cell `index` of a sprite sheet cut into `cols` x `rows` equal cells, numbered row by row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub index: u32,
    pub cols: u32,
    pub rows: u32,
}

impl Frame {
    /// The whole image.
    pub const WHOLE: Frame = Frame {
        index: 0,
        cols: 1,
        rows: 1,
    };

    /// Source rectangle `(x, y, w, h)` in an image of `size`. Out-of-range indices wrap.
    pub fn src_rect(&self, size: (u32, u32)) -> (u32, u32, u32, u32) {
        let (cols, rows) = (self.cols.max(1), self.rows.max(1));
        let (w, h) = (size.0 / cols, size.1 / rows);
        let i = self.index % (cols * rows);
        ((i % cols) * w, (i / cols) * h, w, h)
    }
}

/// Lazily loaded, cached GPU copies of the game's images and fonts.
pub(crate) struct AssetStore<'tc> {
    creator: &'tc TextureCreator<WindowContext>,
    ttf: Sdl3TtfContext,
    data_dir: PathBuf,
    sprites: HashMap<String, Texture<'tc>>,
    fonts: HashMap<(String, u16), Font<'static>>,
}

impl<'tc> AssetStore<'tc> {
    pub(crate) fn new(
        creator: &'tc TextureCreator<WindowContext>,
        ttf: Sdl3TtfContext,
        data_dir: &Path,
    ) -> Self {
        Self {
            creator,
            ttf,
            data_dir: data_dir.to_path_buf(),
            sprites: HashMap::new(),
            fonts: HashMap::new(),
        }
    }

    /// `name` is a file name in `data/rsc/`, for example `ig37.irc`.
    fn sprite(&mut self, name: &str) -> Result<&mut Texture<'tc>, Error> {
        match self.sprites.entry(name.to_owned()) {
            Entry::Occupied(e) => Ok(e.into_mut()),
            Entry::Vacant(e) => {
                let path = self.data_dir.join("rsc").join(name);
                Ok(e.insert(load_irc(self.creator, &path)?))
            }
        }
    }

    /// `name` is a file name in `data/rsc/`, for example `casual.ttf`.
    fn font(&mut self, name: &str, size: u16) -> Result<&Font<'static>, Error> {
        match self.fonts.entry((name.to_owned(), size)) {
            Entry::Occupied(e) => Ok(e.into_mut()),
            Entry::Vacant(e) => {
                let path = self.data_dir.join("rsc").join(name);
                // the sdl3 crate panics on non-UTF-8 font paths, so reject them here
                if path.to_str().is_none() {
                    return Err(Error::Sdl(format!(
                        "non-UTF-8 font path {}",
                        path.display()
                    )));
                }
                let font = self
                    .ttf
                    .load_font(&path, f32::from(size))
                    .map_err(|err| Error::Sdl(format!("{}: {err}", path.display())))?;
                Ok(e.insert(font))
            }
        }
    }
}

fn load_irc<'tc>(
    creator: &'tc TextureCreator<WindowContext>,
    path: &Path,
) -> Result<Texture<'tc>, Error> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let img = thefly_assets::irc::parse(&bytes)
        .map_err(|source| Error::Asset {
            path: path.to_path_buf(),
            source,
        })?
        .image;
    let mut tex = creator
        .create_texture_static(PixelFormat::RGBA32, img.width, img.height)
        .map_err(sdl)?;
    tex.update(None, &img.rgba, img.width as usize * 4)
        .map_err(sdl)?;
    tex.set_blend_mode(BlendMode::Blend);
    Ok(tex)
}

/// What [`crate::Game::render`] draws with. Coordinates are logical pixels.
pub struct Renderer<'a, 'tc> {
    pub(crate) canvas: &'a mut Canvas<Window>,
    pub(crate) assets: &'a mut AssetStore<'tc>,
}

impl Renderer<'_, '_> {
    pub fn clear(&mut self, color: Color) {
        self.canvas.set_draw_color(color);
        self.canvas.clear();
    }

    pub fn fill_rect(&mut self, rect: FRect, color: Color) -> Result<(), Error> {
        self.canvas.set_blend_mode(BlendMode::Blend);
        self.canvas.set_draw_color(color);
        self.canvas.fill_rect(rect).map_err(sdl)
    }

    /// Pixel size of a whole sprite image (`name` as for [`Renderer::draw_sprite`]).
    pub fn sprite_size(&mut self, name: &str) -> Result<(u32, u32), Error> {
        let q = self.assets.sprite(name)?.query();
        Ok((q.width, q.height))
    }

    /// Draws one frame of `name` (a file in `data/rsc/`, e.g. `ig37.irc`) with its top-left at `pos`.
    /// `flip` mirrors horizontally; `alpha` in `[0, 1]` fades it.
    pub fn draw_sprite(
        &mut self,
        name: &str,
        pos: (f32, f32),
        frame: Frame,
        flip: bool,
        alpha: f32,
    ) -> Result<(), Error> {
        let tex = self.assets.sprite(name)?;
        let q = tex.query();
        let (x, y, w, h) = frame.src_rect((q.width, q.height));
        tex.set_alpha_mod((alpha.clamp(0.0, 1.0) * 255.0).round() as u8);
        let src = FRect::new(x as f32, y as f32, w as f32, h as f32);
        let dst = FRect::new(pos.0, pos.1, w as f32, h as f32);
        self.canvas
            .copy_ex(&*tex, src, dst, 0.0, None, flip, false)
            .map_err(sdl)
    }

    /// Draws `text` in font `font` (a file in `data/rsc/`, e.g. `casual.ttf`) at `size` points.
    pub fn draw_text(
        &mut self,
        font: &str,
        size: u16,
        text: &str,
        pos: (f32, f32),
        color: Color,
    ) -> Result<(), Error> {
        if text.is_empty() {
            return Ok(());
        }
        let surface = self
            .assets
            .font(font, size)?
            .render(text)
            .blended(color)
            .map_err(sdl)?;
        // shortcut: rasterises every call; cache by (font, size, text) if text gets heavy
        let tex = self
            .assets
            .creator
            .create_texture_from_surface(&surface)
            .map_err(sdl)?;
        let dst = FRect::new(
            pos.0,
            pos.1,
            surface.width() as f32,
            surface.height() as f32,
        );
        self.canvas.copy(&tex, None, dst).map_err(sdl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_frame_is_the_image() {
        assert_eq!(Frame::WHOLE.src_rect((181, 178)), (0, 0, 181, 178));
    }

    #[test]
    fn grid_frames_go_row_by_row_and_wrap() {
        let f = |index| Frame {
            index,
            cols: 4,
            rows: 2,
        };
        assert_eq!(f(1).src_rect((400, 100)), (100, 0, 100, 50));
        assert_eq!(f(5).src_rect((400, 100)), (100, 50, 100, 50));
        assert_eq!(f(9).src_rect((400, 100)), (100, 0, 100, 50));
    }

    #[test]
    fn vertical_strip() {
        let f = Frame {
            index: 1,
            cols: 1,
            rows: 2,
        };
        assert_eq!(f.src_rect((800, 1200)), (0, 600, 800, 600));
    }
}

//! SDL3 platform layer for The Fly: window, fixed-timestep loop, assets, audio, input and text.
//! No gameplay lives here; a [`Game`] plugs into [`Platform::run`].

mod audio;
pub mod data_dir;
mod error;
mod input;
pub mod letterbox;
mod render;
pub mod timestep;

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::Instant;

use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::Mod;
use sdl3::pixels::PixelFormat;
use sdl3::render::{Canvas, ScaleMode};
use sdl3::video::Window;
use sdl3::{EventPump, Sdl};

pub use audio::{Audio, Voice};
pub use error::Error;
pub use input::{Input, Key, MouseButton};
pub use render::{Color, FRect, Frame, Renderer};

use error::sdl;
use letterbox::Viewport;
use render::AssetStore;
use timestep::Timestep;

/// Scale filter for presenting the logical frame in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    /// The "better graphics" mode.
    Linear,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub title: String,
    pub logical_size: (u32, u32),
    pub tick_hz: u32,
    pub filter: Filter,
    /// Resolved original `data/` dir, see [`data_dir::resolve`].
    pub data_dir: PathBuf,
    /// `.sf2` used for MIDI music; `None` skips MIDI with a warning.
    pub soundfont: Option<PathBuf>,
}

impl Config {
    pub fn new(title: &str, data_dir: PathBuf) -> Self {
        Self {
            title: title.to_owned(),
            logical_size: (800, 600),
            tick_hz: 60,
            filter: Filter::Nearest,
            data_dir,
            soundfont: None,
        }
    }
}

/// What the game supplies. `update` runs at the fixed tick rate, `render` once per displayed frame.
pub trait Game {
    /// Return `ControlFlow::Break(())` to quit.
    fn update(&mut self, input: &Input, audio: &mut Audio) -> ControlFlow<()>;
    /// `alpha` in `[0, 1)` is how far the clock is between the last tick and the next.
    fn render(&self, r: &mut Renderer<'_, '_>, alpha: f32) -> Result<(), Error>;
}

/// An open window with audio, ready to [`run`](Platform::run) a game.
pub struct Platform {
    config: Config,
    _sdl: Sdl,
    canvas: Canvas<Window>,
    events: EventPump,
    ttf: sdl3::ttf::Sdl3TtfContext,
    audio: Audio,
}

impl Platform {
    pub fn new(config: Config) -> Result<Self, Error> {
        let sdl_ctx = sdl3::init().map_err(sdl)?;
        let video = sdl_ctx.video().map_err(sdl)?;
        let (w, h) = config.logical_size;
        let window = video
            .window(&config.title, w, h)
            .resizable()
            .high_pixel_density()
            .position_centered()
            .build()
            .map_err(sdl)?;
        sdl3::hint::set("SDL_RENDER_VSYNC", "1");
        let canvas = window.into_canvas();
        let ttf = sdl3::ttf::init().map_err(sdl)?;
        let events = sdl_ctx.event_pump().map_err(sdl)?;
        let subsystem = sdl_ctx
            .audio()
            .inspect_err(|e| {
                eprintln!("warning: audio init failed ({e}); continuing without sound")
            })
            .ok();
        let audio = Audio::new(subsystem, &config.data_dir, config.soundfont.as_deref());
        Ok(Self {
            config,
            _sdl: sdl_ctx,
            canvas,
            events,
            ttf,
            audio,
        })
    }

    /// Runs until the window closes or the game breaks out of `update`.
    pub fn run(mut self, mut game: impl Game) -> Result<(), Error> {
        let logical = self.config.logical_size;
        let creator = self.canvas.texture_creator();
        let mut frame = creator
            .create_texture_target(PixelFormat::RGBA32, logical.0, logical.1)
            .map_err(sdl)?;
        frame.set_scale_mode(match self.config.filter {
            Filter::Nearest => ScaleMode::Nearest,
            Filter::Linear => ScaleMode::Linear,
        });
        let mut assets = AssetStore::new(&creator, self.ttf.clone(), &self.config.data_dir);
        let mut input = Input::default();
        let mut step = Timestep::new(self.config.tick_hz);
        let mut last = Instant::now();
        let mut fullscreen = false;
        loop {
            for event in self.events.poll_iter() {
                if matches!(event, Event::Quit { .. }) {
                    return Ok(());
                }
                if is_fullscreen_toggle(&event) {
                    fullscreen = !fullscreen;
                    self.canvas
                        .window_mut()
                        .set_fullscreen(fullscreen)
                        .map_err(sdl)?;
                }
                feed(&mut input, &event, self.canvas.window(), logical);
            }
            let now = Instant::now();
            for _ in 0..step.advance(now - last) {
                if game.update(&input, &mut self.audio).is_break() {
                    return Ok(());
                }
                input.end_tick();
            }
            last = now;
            self.audio.pump();

            let mut drawn = Ok(());
            self.canvas
                .with_texture_canvas(&mut frame, |canvas| {
                    let mut r = Renderer {
                        canvas,
                        assets: &mut assets,
                    };
                    drawn = game.render(&mut r, step.alpha());
                })
                .map_err(sdl)?;
            drawn?;
            self.canvas.set_draw_color(Color::BLACK);
            self.canvas.clear();
            let v = Viewport::letterbox(self.canvas.output_size().map_err(sdl)?, logical);
            self.canvas
                .copy(&frame, None, FRect::new(v.x, v.y, v.w, v.h))
                .map_err(sdl)?;
            self.canvas.present();
        }
    }
}

fn is_fullscreen_toggle(event: &Event) -> bool {
    let Event::KeyDown {
        scancode: Some(key),
        keymod,
        repeat: false,
        ..
    } = event
    else {
        return false;
    };
    *key == Key::F11 || (*key == Key::Return && keymod.intersects(Mod::LALTMOD | Mod::RALTMOD))
}

fn feed(input: &mut Input, event: &Event, window: &Window, logical: (u32, u32)) {
    // mouse events are in window points, which differ from pixels on HiDPI screens
    let to_logical =
        |x: f32, y: f32| Viewport::letterbox(window.size(), logical).to_logical(logical, (x, y));
    match *event {
        Event::KeyDown {
            scancode: Some(k), ..
        } => input.key_down(k),
        Event::KeyUp {
            scancode: Some(k), ..
        } => input.key_up(k),
        Event::MouseMotion { x, y, .. } => input.set_mouse(to_logical(x, y)),
        Event::MouseButtonDown {
            mouse_btn, x, y, ..
        } => {
            input.set_mouse(to_logical(x, y));
            input.button_down(mouse_btn);
        }
        Event::MouseButtonUp {
            mouse_btn, x, y, ..
        } => {
            input.set_mouse(to_logical(x, y));
            input.button_up(mouse_btn);
        }
        Event::Window {
            win_event: WindowEvent::FocusLost,
            ..
        } => input.release_all(),
        _ => {}
    }
}

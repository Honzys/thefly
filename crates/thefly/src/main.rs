//! The Fly. For now an asset viewer that exercises every platform subsystem:
//! arrows cycle `.irc` files, Space plays a sound, M plays MIDI music, Esc quits.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use thefly_platform::{
    Audio, Color, Config, Error, FRect, Filter, Frame, Game, Input, Key, Platform, Renderer,
    data_dir,
};

const USAGE: &str = "usage: thefly [--data <dir>] [--soundfont <file.sf2>] [--linear]";
const FONT: &str = "casual.ttf";
const SOUND: &str = "i_ha.wav";
const MUSIC: &str = "i_music.mid";

struct Viewer {
    files: Vec<String>,
    current: usize,
}

impl Game for Viewer {
    fn update(&mut self, input: &Input, audio: &mut Audio) -> ControlFlow<()> {
        if input.pressed(Key::Escape) {
            return ControlFlow::Break(());
        }
        let n = self.files.len();
        if input.pressed(Key::Right) {
            self.current = (self.current + 1) % n;
        }
        if input.pressed(Key::Left) {
            self.current = (self.current + n - 1) % n;
        }
        if input.pressed(Key::Space) {
            log_err(audio.play(SOUND, 1.0));
        }
        if input.pressed(Key::M) {
            log_err(audio.play_music(MUSIC, 0.8));
        }
        ControlFlow::Continue(())
    }

    fn render(&self, r: &mut Renderer<'_, '_>, _alpha: f32) -> Result<(), Error> {
        checkerboard(r)?;
        let name = &self.files[self.current];
        let (w, h) = r.sprite_size(name)?;
        let pos = (
            (800.0 - w as f32).max(0.0) / 2.0,
            (600.0 - h as f32).max(0.0) / 2.0,
        );
        r.draw_sprite(name, pos, Frame::WHOLE, false, 1.0)?;
        r.fill_rect(FRect::new(0.0, 0.0, 800.0, 34.0), Color::RGBA(0, 0, 0, 180))?;
        let label = format!(
            "{name}  {w}x{h}  ({}/{})",
            self.current + 1,
            self.files.len()
        );
        r.draw_text(FONT, 22, &label, (8.0, 4.0), Color::WHITE)
    }
}

/// Grey checkerboard so transparent pixels are visible.
fn checkerboard(r: &mut Renderer<'_, '_>) -> Result<(), Error> {
    const CELL: u32 = 20;
    r.clear(Color::RGB(90, 90, 90));
    for cy in 0..600 / CELL {
        for cx in (cy % 2..800 / CELL).step_by(2) {
            let rect = FRect::new(
                (cx * CELL) as f32,
                (cy * CELL) as f32,
                CELL as f32,
                CELL as f32,
            );
            r.fill_rect(rect, Color::RGB(140, 140, 140))?;
        }
    }
    Ok(())
}

fn log_err<T>(r: Result<T, Error>) {
    if let Err(e) = r {
        eprintln!("warning: {e}");
    }
}

struct Args {
    data: Option<PathBuf>,
    soundfont: Option<PathBuf>,
    linear: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        data: None,
        soundfont: None,
        linear: false,
    };
    let mut it = std::env::args_os().skip(1);
    while let Some(arg) = it.next() {
        match arg.to_str() {
            Some("--data") => args.data = Some(it.next().ok_or(USAGE)?.into()),
            Some("--soundfont") => args.soundfont = Some(it.next().ok_or(USAGE)?.into()),
            Some("--linear") => args.linear = true,
            _ => {
                return Err(format!(
                    "unknown argument {}\n{USAGE}",
                    arg.to_string_lossy()
                ));
            }
        }
    }
    Ok(args)
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate executable: {e}"))?;
    let exe_dir = exe.parent().unwrap_or(Path::new("."));
    let env = std::env::var_os("THEFLY_DATA").map(PathBuf::from);
    let data =
        data_dir::resolve(args.data, env, exe_dir, Path::is_dir).map_err(|e| e.to_string())?;

    let rsc = data.join("rsc");
    let files: Vec<String> = thefly_assets::files_with_ext(&rsc, "irc")
        .map_err(|e| format!("{}: {e}", rsc.display()))?
        .iter()
        .filter_map(|p| p.file_name()?.to_str().map(str::to_owned))
        .collect();
    if files.is_empty() {
        return Err(format!("{}: no .irc files", rsc.display()));
    }

    let mut config = Config::new("The Fly", data);
    config.soundfont = args
        .soundfont
        .or_else(|| std::env::var_os("THEFLY_SOUNDFONT").map(PathBuf::from));
    if args.linear {
        config.filter = Filter::Linear;
    }
    let platform = Platform::new(config).map_err(|e| e.to_string())?;
    platform
        .run(Viewer { files, current: 0 })
        .map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("thefly: {e}");
            ExitCode::FAILURE
        }
    }
}

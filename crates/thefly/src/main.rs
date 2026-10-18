//! The Fly 1.2, reimplemented: the sim from `thefly-sim` on the `thefly-platform` window.

mod assets;
mod draw;
mod fx;
mod game;
mod gfx;
mod lerp;
mod mixer;
mod script;
mod shot;
mod viewer;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use thefly_platform::{Config, Filter, Platform, data_dir};
use thefly_sim::sim::Sim;

use crate::game::FlyGame;
use crate::mixer::Mixer;
use crate::script::Script;
use crate::viewer::Viewer;

const USAGE: &str = "usage: thefly [--data <dir>] [--soundfont <file.sf2>] [--linear] [--viewer]
              [--seed <n>] [--keys <tick[-tick]:Key,...>] [--shot <tick>:<file.png>]...
              [--exit-after-shots]";

#[derive(Default)]
struct Args {
    data: Option<PathBuf>,
    soundfont: Option<PathBuf>,
    linear: bool,
    viewer: bool,
    seed: Option<u32>,
    script: Script,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = std::env::args_os().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || -> Result<String, String> {
            let v = it.next().ok_or(USAGE)?;
            v.into_string()
                .map_err(|v| format!("not UTF-8: {}", v.to_string_lossy()))
        };
        match arg.to_str() {
            Some("--data") => args.data = Some(value()?.into()),
            Some("--soundfont") => args.soundfont = Some(value()?.into()),
            Some("--linear") => args.linear = true,
            Some("--viewer") => args.viewer = true,
            Some("--seed") => args.seed = Some(value()?.parse().map_err(|_| USAGE)?),
            Some("--keys") => args.script.holds.extend(script::parse_keys(&value()?)?),
            Some("--shot") => args.script.shots.push(script::parse_shot(&value()?)?),
            Some("--exit-after-shots") => args.script.exit_after_shots = true,
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

/// The original seeds `srand` from the clock; any 32 bits of it will do.
fn clock_seed() -> u32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (now.as_secs() as u32) ^ now.subsec_nanos()
}

fn viewer(data: &Path) -> Result<Viewer, String> {
    let rsc = data.join("rsc");
    let files: Vec<String> = thefly_assets::files_with_ext(&rsc, "irc")
        .map_err(|e| format!("{}: {e}", rsc.display()))?
        .iter()
        .filter_map(|p| p.file_name()?.to_str().map(str::to_owned))
        .collect();
    if files.is_empty() {
        return Err(format!("{}: no .irc files", rsc.display()));
    }
    Ok(Viewer { files, current: 0 })
}

fn game(data: &Path, seed: u32, script: Script) -> Result<FlyGame, String> {
    let sons = data.join("sons");
    let mixer = Mixer::new(assets::sound_files(&sons)?, assets::click_file(&sons));
    let sim = Sim::new(seed, assets::game_data(data)?, assets::persisted(data));
    // saves go to the working directory, which `assets::persisted` reads first
    let save_dir = std::env::current_dir().map_err(|e| format!("no working directory: {e}"))?;
    Ok(FlyGame::new(sim, mixer, save_dir, script, seed))
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate executable: {e}"))?;
    let exe_dir = exe.parent().unwrap_or(Path::new("."));
    let env = std::env::var_os("THEFLY_DATA").map(PathBuf::from);
    let data =
        data_dir::resolve(args.data, env, exe_dir, Path::is_dir).map_err(|e| e.to_string())?;

    let mut config = Config::new("The Fly", data.clone());
    config.soundfont = args
        .soundfont
        .or_else(|| std::env::var_os("THEFLY_SOUNDFONT").map(PathBuf::from));
    if args.linear {
        config.filter = Filter::Linear;
    }
    if args.viewer {
        let v = viewer(&data)?;
        let platform = Platform::new(config).map_err(|e| e.to_string())?;
        return platform.run(v).map_err(|e| e.to_string());
    }
    let g = game(&data, args.seed.unwrap_or_else(clock_seed), args.script)?;
    let platform = Platform::new(config).map_err(|e| e.to_string())?;
    platform.run(g).map_err(|e| e.to_string())
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

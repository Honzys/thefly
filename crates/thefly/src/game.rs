//! The platform `Game`: ticks the sim at 65 Hz, keeps the previous state for interpolation, maps
//! events to audio, effects and save files, and plays an optional key script with screenshots.

use std::cell::RefCell;
use std::ops::ControlFlow;
use std::path::PathBuf;

use thefly_platform::{Audio, Error, Input, Key, Renderer};
use thefly_sim::event::{Effect, Event, SoundOp, SoundSlot};
use thefly_sim::input::{KeyState, Scancode};
use thefly_sim::screen::{MatchScreen, Screen};
use thefly_sim::shop::TARGET_OFFSET;
use thefly_sim::sim::{Sim, State};
use thefly_sim::winner::Stage;

use crate::draw::{self, View};
use crate::fx::Fx;
use crate::gfx::Gfx;
use crate::lerp::Lerp;
use crate::mixer::Mixer;
use crate::script::Script;

/// SDL3 scancodes the sim can see (`Scancode` is a u8).
const SCANCODES: i32 = 256;

pub struct FlyGame {
    sim: Sim,
    /// The state before the last tick; `None` right after a screen change.
    prev: Option<State>,
    mixer: Mixer,
    fx: Fx,
    trophy_landed_ms: Option<u64>,
    /// Where `scoot.btt` / `boite.btt` are written (the directory they were read from).
    save_dir: PathBuf,
    script: Script,
    tick: u64,
    /// Pending screenshots `(tick, path)`, taken in `render`.
    shots: RefCell<Vec<(u64, PathBuf)>>,
    exit_after_shots: bool,
}

impl FlyGame {
    pub fn new(sim: Sim, mixer: Mixer, save_dir: PathBuf, script: Script, seed: u32) -> Self {
        let shots = RefCell::new(script.shots.clone());
        let exit_after_shots = script.exit_after_shots;
        Self {
            sim,
            prev: None,
            mixer,
            fx: Fx::new(seed),
            trophy_landed_ms: None,
            save_dir,
            script,
            tick: 0,
            shots,
            exit_after_shots,
        }
    }

    fn keys(&self, input: &Input) -> KeyState {
        let codes = (0..SCANCODES).filter_map(|i| Some((Key::from_i32(i)?, u8::try_from(i).ok()?)));
        let mut held = Vec::new();
        let mut pressed = Vec::new();
        let mut released = Vec::new();
        for (k, code) in codes {
            let sc = Scancode(code);
            let (h, p, r) = self.script.at(self.tick, sc);
            if input.held(k) || h {
                held.push(sc);
            }
            if input.pressed(k) || p {
                pressed.push(sc);
            }
            if input.released(k) || r {
                released.push(sc);
            }
        }
        KeyState::from_parts(held, pressed, released)
    }

    fn handle(&mut self, e: Event, audio: &mut Audio) {
        self.mixer.apply(&e, audio);
        match e {
            Event::Effect(Effect::Spark {
                x,
                y,
                frame,
                vx,
                vy,
                life_ms,
            }) => {
                let x = x + draw::spark_shift(self.sim.state(), x) as i32;
                self.fx.effect(Effect::Spark {
                    x,
                    y,
                    frame,
                    vx,
                    vy,
                    life_ms,
                });
            }
            Event::Effect(fx) => self.fx.effect(fx),
            Event::ScreenChanged => {
                self.fx.clear();
                self.trophy_landed_ms = None;
            }
            Event::Sound {
                slot: SoundSlot::BOOM,
                op: SoundOp::Play(_),
            } => {
                if matches!(self.sim.state().screen, Screen::Menu(_)) {
                    self.fx.menu_dust();
                }
            }
            Event::Save { file, bytes } => {
                let path = self.save_dir.join(file.file());
                if let Err(err) = std::fs::write(&path, bytes) {
                    eprintln!("warning: cannot save {}: {err}", path.display());
                }
            }
            _ => {}
        }
    }

    /// Frontend-only reactions to state changes the sim does not emit as events.
    fn react(&mut self, before: &State) {
        let cur = self.sim.state();
        match (&cur.screen, &before.screen) {
            (Screen::HighScores(h), Screen::HighScores(p)) if h.revealed < p.revealed => {
                // SR §5.3: up to 30 stardust over the newly revealed row
                let y = 163.0 + 40.0 * h.revealed as f32;
                self.fx.stardust(50.0, y, 400, 30, 30);
            }
            _ => {}
        }
        if let Some((x, y)) = tally_landed(cur, before) {
            // SR §3.1: 20 `DUST` puffs at the machine outlet. shortcut: the puff area is not in
            // the docs; it spans the hopper's mouth. Revisit when `FUN_0042322f` is read.
            self.fx
                .dust_burst(x as f32, y as f32 - 30.0, 120, 40, 20, 200.0);
        }
        if let Some(fly_won) = trophy_landed(cur, before) {
            // SR §5.1 state 2: 30 puffs at (trophyX − 20, 310) over 480 × 100
            let tx = if fly_won { 50.0 } else { 430.0 };
            self.fx.dust_burst(tx - 20.0, 310.0, 480, 100, 30, 200.0);
            self.trophy_landed_ms = Some(self.sim.clock().now_ms());
        }
    }

    fn shots_done(&self) -> bool {
        self.shots.borrow().is_empty()
    }
}

impl thefly_platform::Game for FlyGame {
    fn update(&mut self, input: &Input, audio: &mut Audio) -> ControlFlow<()> {
        let steps = if self.tick < self.script.warp_until {
            WARP_TICKS
        } else {
            1
        };
        for _ in 0..steps {
            if self.exit_after_shots && self.shots_done() {
                return ControlFlow::Break(());
            }
            // hold the sim until a due screenshot is rendered, so shots land on their exact tick
            if self.shots.borrow().iter().any(|(t, _)| *t <= self.tick) {
                return ControlFlow::Continue(());
            }
            self.step(input, audio)?;
        }
        ControlFlow::Continue(())
    }

    fn render(&self, r: &mut Renderer<'_, '_>, alpha: f32) -> Result<(), Error> {
        let view = View {
            cur: self.sim.state(),
            prev: self.prev.as_ref(),
            l: Lerp { alpha },
            now: self.sim.clock().now_ms(),
            fx: &self.fx,
            trophy_landed_ms: self.trophy_landed_ms,
        };
        let mut g = Gfx { r };
        draw::frame(&mut g, &view)?;
        if self.script.window_shots {
            return Ok(());
        }
        self.take_shots(g.r, SHOT_SIZE)
    }

    fn presented(&self, r: &mut Renderer<'_, '_>, size: (u32, u32)) -> Result<(), Error> {
        if !self.script.window_shots {
            return Ok(());
        }
        self.take_shots(r, size)
    }
}

impl FlyGame {
    fn step(&mut self, input: &Input, audio: &mut Audio) -> ControlFlow<()> {
        let keys = self.keys(input);
        let before = self.sim.state().clone();
        let events = self.sim.tick(&keys);
        self.tick += 1;
        self.fx.step();
        let changed = events.iter().any(|e| matches!(e, Event::ScreenChanged));
        for e in events {
            self.handle(e, audio);
        }
        self.react(&before);
        self.prev = (!changed).then_some(before);
        if matches!(self.sim.state().screen, Screen::Exit) {
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    fn take_shots(&self, r: &mut Renderer<'_, '_>, size: (u32, u32)) -> Result<(), Error> {
        let due: Vec<PathBuf> = {
            let mut shots = self.shots.borrow_mut();
            let (due, later) = shots.drain(..).partition(|(t, _)| *t <= self.tick);
            *shots = later;
            due.into_iter().map(|(_, p)| p).collect()
        };
        if due.is_empty() {
            return Ok(());
        }
        let pixels = r.read_pixels(size)?;
        for path in due {
            if let Err(e) = crate::shot::write_png(&path, size, &pixels) {
                eprintln!("warning: screenshot {}: {e}", path.display());
            }
        }
        Ok(())
    }
}

const SHOT_SIZE: (u32, u32) = (800, 600);
/// Sim ticks per frame while `--warp` fast-forwards a scripted run.
const WARP_TICKS: u32 = 64;

/// A tally landing re-arms the machine shake (answers.md "Shop machine shake"): the outlet.
fn tally_landed(cur: &State, before: &State) -> Option<(i32, i32)> {
    let machine = |s: &State| match &s.screen {
        Screen::Match(m) => match &m.screen {
            MatchScreen::GoodiesShop(g) => Some(g.machine),
            _ => None,
        },
        _ => None,
    };
    let m = machine(cur)?;
    let landed = m.shake.is_armed() && machine(before).is_some_and(|b| b.shake != m.shake);
    landed.then_some((m.x + TARGET_OFFSET.0, m.y + TARGET_OFFSET.1))
}

/// `Some(fly_won)` on the tick the winner screen's trophy lands (falls into the hops stage).
fn trophy_landed(cur: &State, before: &State) -> Option<bool> {
    let winner = |s: &State| match &s.screen {
        Screen::Match(m) => match &m.screen {
            MatchScreen::Winner(w) => Some((w.stage, w.fly_won)),
            _ => None,
        },
        _ => None,
    };
    let (stage, fly_won) = winner(cur)?;
    let landed = stage == Stage::Hops && winner(before).is_some_and(|(s, _)| s != Stage::Hops);
    landed.then_some(fly_won)
}

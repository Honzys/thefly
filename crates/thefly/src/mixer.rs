//! Sound events to voices, emulating the original's DirectSound buffers: one buffer per slot whose
//! volume, frequency and pan persist between plays (game-spec §8).

use std::collections::HashMap;

use thefly_platform::{Audio, PlayParams as VoiceParams, Voice};
use thefly_sim::event::{Event, Music, Pan, PlayParams, SoundOp, SoundSlot};

/// DirectSound's floor, −100 dB.
const MIN_CB: i32 = -10_000;
/// DirectSound's pan range, ±10000 hundredths of a dB of attenuation on the far side.
const MAX_PAN: i32 = 10_000;
const SCREEN_W: f64 = 800.0;
/// shortcut: `FUN_0040d1b2` takes the source's wrap, which `Pan::ScreenX` does not carry; the
/// object layer's 5000 (answers.md "Screen x") is used for every source, the farmer's 4000 ring
/// included. Revisit when the sim's `Pan::ScreenX` gains a wrap.
const WRAP: f64 = 5000.0;
/// O14 (answers.md "Positional sound curve"): the off-screen pan peak and the volume loss behind.
const OFF_PAN_CB: f64 = 3000.0;
const BEHIND_CB: f64 = 1500.0;
/// The buzz (`FUN_0040d005`, answers.md "Fly buzz: pitch, pan and volume"): pan ±2000 around
/// the screen centre at volume −700.
const BUZZ_PAN_CB: f64 = 2000.0;
const BUZZ_CB: i32 = -700;

/// DirectSound volume (hundredths of a dB, ≤ 0) as a linear gain.
pub fn gain(cb: i32) -> f32 {
    10f32.powf(cb.clamp(MIN_CB, 0) as f32 / 2000.0)
}

/// A DirectSound pan as `(left, right)` gains: the side away from the pan is attenuated.
pub fn pan_gains(pan: i32) -> (f32, f32) {
    let p = pan.clamp(-MAX_PAN, MAX_PAN);
    if p < 0 {
        (1.0, gain(p))
    } else {
        (gain(-p), 1.0)
    }
}

/// `(left, right)` gains for a [`Pan`] on `slot`; a positional volume change is folded in.
pub fn pan(slot: SoundSlot, p: Pan) -> (f32, f32) {
    let (pan_cb, volume_cb) = match p {
        Pan::Centre => return (1.0, 1.0),
        Pan::Fixed(v) => return pan_gains(v),
        Pan::ScreenX { x, w } if slot == SoundSlot::FLY_BUZZ => buzz(f64::from(x), f64::from(w)),
        Pan::ScreenX { x, w } => positional(f64::from(x), f64::from(w)),
    };
    let (l, r) = pan_gains(pan_cb);
    let v = gain(volume_cb);
    (l * v, r * v)
}

/// O14, `FUN_0040d1b2` (answers.md "Positional sound curve"): `(pan, volume offset)` in cb. On
/// screen, or past wrap − w, the source is centred at full volume.
fn positional(x: f64, w: f64) -> (i32, i32) {
    let x = x.rem_euclid(WRAP);
    let half = (WRAP + SCREEN_W) / 2.0;
    let (d, side) = if x > SCREEN_W && x <= half {
        (((x - SCREEN_W) / (half - SCREEN_W) * 100.0).trunc(), -1.0)
    } else if x > half && x <= WRAP - w {
        (((1.0 - (x - half) / ((WRAP - w) - half)) * 100.0).trunc(), 1.0)
    } else {
        return (0, 0);
    };
    let (p, offset) = if d < 50.0 {
        ((d * 100.0 / 50.0 * 30.0 * side).trunc(), 0.0)
    } else {
        let behind = (d - 50.0) / 50.0;
        (((1.0 - behind) * OFF_PAN_CB * side).trunc(), -(behind * BEHIND_CB).trunc())
    };
    (-p as i32, offset as i32)
}

/// `FUN_0040d005`: the buzz pans around the screen centre and loses |x| or x − 800 off screen.
fn buzz(x: f64, w: f64) -> (i32, i32) {
    let centre = SCREEN_W / 2.0;
    let p = if x <= centre - w {
        -((1.0 - x / (centre - w)) * BUZZ_PAN_CB).trunc()
    } else if x >= centre {
        ((x - centre) / centre * BUZZ_PAN_CB).trunc()
    } else {
        0.0
    };
    let off = if x < 0.0 {
        -x
    } else {
        (x - SCREEN_W).max(0.0)
    };
    // `as` saturates, so i32::MIN / MAX sources stay in range
    (p as i32, BUZZ_CB.saturating_sub(off as i32))
}

/// The persistent settings of one slot's buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slot {
    voice: Option<Voice>,
    volume_cb: i32,
    freq_hz: Option<u32>,
    pan: (f32, f32),
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            voice: None,
            volume_cb: 0,
            freq_hz: None,
            pan: (1.0, 1.0),
        }
    }
}

pub struct Mixer {
    /// File name per slot (`SOUND_FILES` resolved on disk).
    files: Vec<String>,
    click: Option<String>,
    slots: HashMap<SoundSlot, Slot>,
}

impl Mixer {
    pub fn new(files: Vec<String>, click: Option<String>) -> Self {
        Self {
            files,
            click,
            slots: HashMap::new(),
        }
    }

    /// Performs the audio side of `event`; other events are ignored.
    pub fn apply(&mut self, event: &Event, audio: &mut Audio) {
        let result = match event {
            Event::Sound { slot, op } => self.sound(*slot, *op, audio),
            Event::Music(m) => music(*m, audio),
            Event::Click { volume_cb, freq_hz } => self.click(*volume_cb, *freq_hz, audio),
            _ => Ok(()),
        };
        if let Err(e) = result {
            eprintln!("warning: {e}");
        }
    }

    fn click(&self, volume_cb: i32, freq_hz: Option<u32>, audio: &mut Audio) -> Result<(), String> {
        let Some(file) = &self.click else {
            return Ok(());
        };
        let params = VoiceParams {
            freq_hz,
            ..VoiceParams::new(gain(volume_cb))
        };
        audio.play_with(file, params).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn sound(&mut self, slot: SoundSlot, op: SoundOp, audio: &mut Audio) -> Result<(), String> {
        let file = self
            .files
            .get(usize::from(slot.0))
            .ok_or_else(|| format!("no sound slot {}", slot.0))?;
        let s = self.slots.entry(slot).or_default();
        match op {
            SoundOp::Play(p) => start(slot, s, file, p, false, audio)?,
            SoundOp::Loop(p) => start(slot, s, file, p, true, audio)?,
            SoundOp::Stop => {
                if let Some(v) = s.voice.take() {
                    audio.stop(v);
                }
            }
            // out-of-range positive volumes made the original call fail (event.rs SetVolume)
            SoundOp::SetVolume(cb) if cb > 0 => {}
            SoundOp::SetVolume(cb) => {
                s.volume_cb = cb;
                if let Some(v) = s.voice {
                    audio.set_volume(v, gain(cb));
                }
            }
            SoundOp::SetFrequency(hz) => {
                s.freq_hz = Some(hz);
                if let Some(v) = s.voice {
                    audio.set_frequency(v, hz);
                }
            }
            SoundOp::Move(p) => {
                s.pan = pan(slot, p);
                if let Some(v) = s.voice {
                    audio.set_pan(v, s.pan);
                }
            }
        }
        Ok(())
    }
}

/// Play restarts the slot's one buffer with the event's settings, which then persist.
fn start(
    slot: SoundSlot,
    s: &mut Slot,
    file: &str,
    p: PlayParams,
    looping: bool,
    audio: &mut Audio,
) -> Result<(), String> {
    if let Some(v) = s.voice.take() {
        audio.stop(v);
    }
    s.volume_cb = p.volume_cb.min(0);
    s.pan = pan(slot, p.pan);
    if p.freq_hz.is_some() {
        s.freq_hz = p.freq_hz;
    }
    let params = VoiceParams {
        volume: gain(s.volume_cb),
        pan: s.pan,
        freq_hz: s.freq_hz,
        looping,
    };
    s.voice = audio.play_with(file, params).map_err(|e| e.to_string())?;
    Ok(())
}

fn music(m: Music, audio: &mut Audio) -> Result<(), String> {
    let Some(file) = m.file() else {
        audio.stop_music();
        return Ok(());
    };
    audio.play_music(file, 1.0).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn volume_is_hundredths_of_a_db() {
        assert!(close(gain(0), 1.0));
        assert!(close(gain(-2000), 0.1));
        assert!(close(gain(-10_000), 1e-5));
        assert!(close(gain(-20_000), 1e-5));
    }

    #[test]
    fn pan_attenuates_the_far_side() {
        let (l, r) = pan_gains(-2000);
        assert!(close(l, 1.0) && close(r, 0.1));
        let (l, r) = pan_gains(500);
        assert!(close(l, gain(-500)) && close(r, 1.0));
    }

    const OTHER: SoundSlot = SoundSlot(47);

    fn screen(x: i32, w: i32) -> Pan {
        Pan::ScreenX { x, w }
    }

    /// `(left, right)` for a DirectSound pan and volume offset, built independently of `pan`.
    fn expect(pan_cb: i32, volume_cb: i32) -> (f32, f32) {
        let v = gain(volume_cb);
        if pan_cb < 0 {
            (v, v * gain(pan_cb))
        } else {
            (v * gain(-pan_cb), v)
        }
    }

    fn assert_gains(got: (f32, f32), want: (f32, f32)) {
        assert!(close(got.0, want.0) && close(got.1, want.1), "{got:?} != {want:?}");
    }

    #[test]
    fn on_screen_and_past_the_wrap_edge_sources_are_centred_at_full_volume() {
        for (x, w) in [(0, 120), (400, 0), (800, 0), (4950, 100), (5000, 0)] {
            assert_eq!(pan(OTHER, screen(x, w)), (1.0, 1.0), "x {x}");
        }
    }

    #[test]
    fn a_source_right_of_the_screen_pans_right_up_to_3000_then_fades_behind() {
        // wrap 5000: half 2900. x 1617 (w 235): d 38, p −2280, SetPan(+2280).
        assert_gains(pan(OTHER, screen(1617, 235)), expect(2280, 0));
        // x 1850: d 50, p −3000, no volume change yet.
        assert_gains(pan(OTHER, screen(1850, 0)), expect(3000, 0));
        // x 2900 = half: d 100, pan back to 0, volume −1500.
        assert_gains(pan(OTHER, screen(2900, 0)), expect(0, -1500));
    }

    #[test]
    fn a_source_wrapped_left_of_the_screen_mirrors_with_a_left_pan() {
        // x 3400 (w 100): d = trunc((1 − 500 / 2000) × 100) = 75, p 1500, volume −750.
        assert_gains(pan(OTHER, screen(3400, 100)), expect(-1500, -750));
        // x 4400 (w 100): d 25, p 1500, SetPan(−1500).
        assert_gains(pan(OTHER, screen(4400, 100)), expect(-1500, 0));
        // shortcut wrap: a negative x is taken on the ring, −600 ≡ 4400
        assert_eq!(pan(OTHER, screen(-600, 100)), pan(OTHER, screen(4400, 100)));
    }

    #[test]
    fn extreme_screen_x_stays_in_gain_range() {
        for slot in [OTHER, SoundSlot::FLY_BUZZ] {
            for x in [-1, i32::MIN, i32::MAX] {
                let (l, r) = pan(slot, screen(x, 0));
                assert!(
                    (0.0..=1.0).contains(&l) && (0.0..=1.0).contains(&r),
                    "x {x}: {l} {r}"
                );
            }
        }
        assert_eq!(
            pan(OTHER, Pan::Fixed(-100_000)),
            pan(OTHER, Pan::Fixed(-MAX_PAN))
        );
    }

    #[test]
    fn the_buzz_pans_around_the_screen_centre_at_minus_700() {
        let buzz = |x, w| pan(SoundSlot::FLY_BUZZ, screen(x, w));
        // 400 − w < x < 400: no pan
        assert_gains(buzz(340, 120), expect(0, -700));
        // x ≥ 400: trunc(340 / 400 × 2000) = 1700
        assert_gains(buzz(740, 120), expect(1700, -700));
        // x ≤ 400 − w: −trunc((1 − x / (400 − w)) × 2000); off screen loses |x|
        assert_gains(buzz(0, 120), expect(-2000, -700));
        assert_gains(buzz(-400, 0), expect(-4000, -1100));
        assert_gains(buzz(1000, 0), expect(3000, -900));
    }
}

//! Sound events to voices, emulating the original's DirectSound buffers: one buffer per slot whose
//! volume, frequency and pan persist between plays (game-spec §8).

use std::collections::HashMap;

use thefly_platform::{Audio, PlayParams as VoiceParams, Voice};
use thefly_sim::event::{Event, Music, Pan, PlayParams, SoundOp, SoundSlot};

/// DirectSound's floor, −100 dB.
const MIN_CB: i32 = -10_000;
/// DirectSound's pan range, ±10000 hundredths of a dB of attenuation on the far side.
const MAX_PAN: i32 = 10_000;
const SCREEN_W: i32 = 800;
/// shortcut: open item O14, the positional curve is not decoded. Linear pan from the screen
/// centre reaching this many cb at the edges; revisit if the curve is decoded.
const EDGE_PAN_CB: i32 = 2000;
/// shortcut: O14 as above. Off-screen sources lose this many cb per pixel past the edge...
const OFFSCREEN_CB_PER_PX: i32 = 3;
/// ...down to this much.
const OFFSCREEN_MAX_CB: i32 = -3000;

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

/// `(left, right)` gains for a [`Pan`].
pub fn pan(p: Pan) -> (f32, f32) {
    match p {
        Pan::Centre => (1.0, 1.0),
        Pan::Fixed(v) => pan_gains(v),
        Pan::ScreenX { x, w } => {
            let centre = x.saturating_add(w / 2);
            let off = i32::try_from(centre.abs_diff(centre.clamp(0, SCREEN_W))).unwrap_or(i32::MAX);
            let fade = gain((-off.saturating_mul(OFFSCREEN_CB_PER_PX)).max(OFFSCREEN_MAX_CB));
            let half = SCREEN_W / 2;
            let cb = ((centre.clamp(0, SCREEN_W) - half) * EDGE_PAN_CB) / half;
            let (l, r) = pan_gains(cb);
            (l * fade, r * fade)
        }
    }
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
            SoundOp::Play(p) => start(s, file, p, false, audio)?,
            SoundOp::Loop(p) => start(s, file, p, true, audio)?,
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
                s.pan = pan(p);
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
    s.pan = pan(p.pan);
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

    #[test]
    fn negative_screen_x_pans_hard_left_and_stays_in_gain_range() {
        let fade = gain(-300);
        let (l, r) = pan(Pan::ScreenX { x: -100, w: 0 });
        assert!(close(l, fade) && close(r, fade * 0.1), "{l} {r}");
        for x in [-1, i32::MIN, i32::MAX] {
            let (l, r) = pan(Pan::ScreenX { x, w: 0 });
            assert!(
                (0.0..=1.0).contains(&l) && (0.0..=1.0).contains(&r),
                "x {x}: {l} {r}"
            );
        }
        assert_eq!(pan(Pan::Fixed(-100_000)), pan(Pan::Fixed(-MAX_PAN)));
    }

    #[test]
    fn positional_pan_follows_screen_x_and_fades_off_screen() {
        assert_eq!(pan(Pan::ScreenX { x: 340, w: 120 }), (1.0, 1.0));
        let (l, r) = pan(Pan::ScreenX { x: 740, w: 120 });
        assert!(l < r && close(r, 1.0));
        let (l, r) = pan(Pan::ScreenX { x: -400, w: 0 });
        assert!(l < 1.0 && r < l);
    }
}

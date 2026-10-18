//! Sound effects from `data/sons/` and one music channel (WAV or MIDI via a SoundFont).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;

use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use sdl3::AudioSubsystem;
use sdl3::audio::{AudioDevice, AudioFormat, AudioSpec, AudioSpecWAV, AudioStreamOwner};

use crate::error::{Error, sdl};

const MIDI_RATE: i32 = 44_100;
/// Longest MIDI file rendered, to bound memory on hostile input (~200 MB of f32 stereo).
const MAX_MIDI_SECONDS: f64 = 600.0;
/// At most this many `.mid` files are pre-rendered at start-up (the release has two).
const MAX_PRELOAD_MIDI: usize = 8;

/// Handle to a playing voice. Stale handles are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Voice(u64);

/// How a voice starts. `gain` is per channel `(left, right)`, so panning is the caller's curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayParams {
    pub volume: f32,
    pub pan: (f32, f32),
    /// Playback rate in Hz; `None` keeps the file's own rate.
    pub freq_hz: Option<u32>,
    pub looping: bool,
}

impl PlayParams {
    pub fn new(volume: f32) -> Self {
        Self {
            volume,
            pan: (1.0, 1.0),
            freq_hz: None,
            looping: false,
        }
    }
}

/// Decoded PCM as interleaved f32 at the file's own rate.
struct Sound {
    rate: i32,
    channels: usize,
    samples: Vec<f32>,
}

impl Sound {
    fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1)
    }
}

struct Playing {
    id: Voice,
    stream: AudioStreamOwner,
    sound: Rc<Sound>,
    looping: bool,
    pan: (f32, f32),
    /// Next frame of `sound` to queue.
    cursor: usize,
}

/// Seconds of a voice queued ahead of the device, so a pan change is heard within about this long.
const LEAD_SECONDS: f32 = 0.08;
/// SDL's accepted frequency-ratio range.
const RATIO_RANGE: (f32, f32) = (0.01, 100.0);
/// Queued chunks per voice per pump; bounds the refill loop for very short looping sounds.
const MAX_CHUNKS_PER_PUMP: usize = 64;

struct Device {
    subsystem: AudioSubsystem,
    device: AudioDevice,
}

pub struct Audio {
    device: Option<Device>,
    sons: PathBuf,
    soundfont: Option<Arc<SoundFont>>,
    /// MIDI files still rendering on background threads, by file name.
    pending: HashMap<String, JoinHandle<Result<Sound, Error>>>,
    cache: HashMap<String, Rc<Sound>>,
    voices: Vec<Playing>,
    music: Option<Playing>,
    next_id: u64,
}

impl Audio {
    /// Without a device every call is a silent no-op, so a machine without sound still runs.
    pub(crate) fn new(
        subsystem: Option<AudioSubsystem>,
        data_dir: &Path,
        soundfont: Option<&Path>,
    ) -> Self {
        let device = subsystem.and_then(|subsystem| {
            let spec = AudioSpec::new(Some(MIDI_RATE), Some(2), Some(AudioFormat::F32LE));
            match subsystem.open_playback_device(&spec) {
                Ok(device) => Some(Device { subsystem, device }),
                Err(e) => {
                    eprintln!("warning: no audio device ({e}); continuing without sound");
                    None
                }
            }
        });
        let soundfont = soundfont.and_then(|p| match load_soundfont(p) {
            Ok(sf) => Some(sf),
            Err(e) => {
                eprintln!("warning: {e}; MIDI music disabled");
                None
            }
        });
        let sons = data_dir.join("sons");
        let pending = soundfont
            .as_ref()
            .map(|sf| preload_midi(&sons, sf))
            .unwrap_or_default();
        Self {
            device,
            sons,
            soundfont,
            pending,
            cache: HashMap::new(),
            voices: Vec::new(),
            music: None,
            next_id: 0,
        }
    }

    pub fn enabled(&self) -> bool {
        self.device.is_some()
    }

    /// Plays `name` (a `.wav` in `data/sons/`) once at `volume` (1.0 = original).
    /// `Ok(None)` means nothing plays (no audio device); it is not an error.
    pub fn play(&mut self, name: &str, volume: f32) -> Result<Option<Voice>, Error> {
        self.play_with(name, PlayParams::new(volume))
    }

    /// Like [`Audio::play`] but repeats until [`Audio::stop`].
    pub fn play_loop(&mut self, name: &str, volume: f32) -> Result<Option<Voice>, Error> {
        let params = PlayParams {
            looping: true,
            ..PlayParams::new(volume)
        };
        self.play_with(name, params)
    }

    /// Plays `name` with explicit pan, rate and looping.
    pub fn play_with(&mut self, name: &str, params: PlayParams) -> Result<Option<Voice>, Error> {
        self.start(name, params).map(|p| self.keep(p))
    }

    /// Replaces the music channel with `name` (`.wav` or `.mid` in `data/sons/`), looping.
    /// `Ok(None)` means nothing plays: no audio device, or MIDI without a SoundFont (warned).
    pub fn play_music(&mut self, name: &str, volume: f32) -> Result<Option<Voice>, Error> {
        self.music = None;
        let params = PlayParams {
            looping: true,
            ..PlayParams::new(volume)
        };
        self.music = self.start(name, params)?;
        Ok(self.music.as_ref().map(|p| p.id))
    }

    pub fn set_volume(&mut self, voice: Voice, volume: f32) {
        let Some(p) = self.find(voice) else { return };
        if let Err(e) = p.stream.set_gain(volume) {
            eprintln!("warning: set volume: {e}");
        }
    }

    /// Changes the playback rate; heard immediately, including the audio already queued.
    pub fn set_frequency(&mut self, voice: Voice, hz: u32) {
        let Some(p) = self.find(voice) else { return };
        set_ratio(p, hz);
    }

    /// Changes the `(left, right)` gains; heard once the queued lead ([`LEAD_SECONDS`]) plays out.
    pub fn set_pan(&mut self, voice: Voice, pan: (f32, f32)) {
        if let Some(p) = self.voices.iter_mut().find(|p| p.id == voice) {
            p.pan = pan;
        }
    }

    pub fn stop(&mut self, voice: Voice) {
        self.voices.retain(|p| p.id != voice);
        if self.music.as_ref().is_some_and(|p| p.id == voice) {
            self.music = None;
        }
    }

    /// True while `voice` still has sound to play.
    pub fn is_playing(&self, voice: Voice) -> bool {
        self.find(voice).is_some()
    }

    /// Queues more audio and drops finished voices. Called once per frame by the loop.
    pub(crate) fn pump(&mut self) {
        for p in self.voices.iter_mut().chain(self.music.as_mut()) {
            refill(p);
        }
        self.voices.retain(|p| !finished(p));
    }

    fn find(&self, voice: Voice) -> Option<&Playing> {
        self.voices
            .iter()
            .chain(self.music.as_ref())
            .find(|p| p.id == voice)
    }

    fn keep(&mut self, p: Option<Playing>) -> Option<Voice> {
        let p = p?;
        let id = p.id;
        self.voices.push(p);
        Some(id)
    }

    /// `None` when there is nothing to play (no device, or MIDI without a SoundFont).
    fn start(&mut self, name: &str, params: PlayParams) -> Result<Option<Playing>, Error> {
        if self.device.is_none() {
            return Ok(None);
        }
        let Some(sound) = self.sound(name)? else {
            return Ok(None);
        };
        let Some(dev) = &self.device else {
            return Ok(None);
        };
        let spec = AudioSpec::new(Some(sound.rate), Some(2), Some(AudioFormat::F32LE));
        let stream = dev.subsystem.new_playback_stream(&spec, None).map_err(sdl)?;
        dev.device.bind_stream(&stream).map_err(sdl)?;
        stream.set_gain(params.volume).map_err(sdl)?;
        self.next_id += 1;
        let mut p = Playing {
            id: Voice(self.next_id),
            stream,
            sound,
            looping: params.looping,
            pan: params.pan,
            cursor: 0,
        };
        if let Some(hz) = params.freq_hz {
            set_ratio(&p, hz);
        }
        refill(&mut p);
        Ok(Some(p))
    }

    fn sound(&mut self, name: &str) -> Result<Option<Rc<Sound>>, Error> {
        if let Some(s) = self.cache.get(name) {
            return Ok(Some(Rc::clone(s)));
        }
        let path = self.sons.join(name);
        let is_midi = Path::new(name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("mid"));
        let sound = match (is_midi, &self.soundfont) {
            (false, _) => load_wav(&path)?,
            (true, Some(sf)) => match self.pending.remove(name) {
                // waits only if the background render has not finished yet
                Some(job) => job
                    .join()
                    .unwrap_or_else(|_| Err(midi_err(&path, "render thread panicked")))?,
                None => render_midi(&path, sf)?,
            },
            (true, None) => {
                eprintln!("warning: no SoundFont (set THEFLY_SOUNDFONT); skipping {name}");
                return Ok(None);
            }
        };
        let sound = Rc::new(sound);
        self.cache.insert(name.to_owned(), Rc::clone(&sound));
        Ok(Some(sound))
    }
}

fn set_ratio(p: &Playing, hz: u32) {
    let ratio = (hz as f32 / p.sound.rate.max(1) as f32).clamp(RATIO_RANGE.0, RATIO_RANGE.1);
    // SAFETY: the stream pointer is valid for as long as `p.stream` lives.
    let ok = unsafe { sdl3::sys::audio::SDL_SetAudioStreamFrequencyRatio(p.stream.raw(), ratio) };
    if !ok {
        eprintln!("warning: set frequency: {}", sdl3::get_error());
    }
}

/// A one-shot voice is done once all of it is queued and the device has drained it.
fn finished(p: &Playing) -> bool {
    !p.looping && p.cursor >= p.sound.frames() && p.stream.queued_bytes().unwrap_or(0) == 0
}

/// Queues stereo frames from the cursor, panned, until [`LEAD_SECONDS`] are ahead of the device.
fn refill(p: &mut Playing) {
    let total = p.sound.frames();
    if total == 0 {
        return;
    }
    let lead = (LEAD_SECONDS * p.sound.rate as f32) as usize;
    let chunk = lead.clamp(1, total);
    for _ in 0..MAX_CHUNKS_PER_PUMP {
        let queued = usize::try_from(p.stream.queued_bytes().unwrap_or(0)).unwrap_or(0);
        if queued / STEREO_FRAME_BYTES >= lead {
            return;
        }
        if p.cursor >= total && !p.looping {
            // lets SDL play out the last partial sample frame so queued_bytes reaches 0
            if let Err(e) = p.stream.flush() {
                eprintln!("warning: audio flush: {e}");
            }
            return;
        }
        p.cursor %= total;
        let end = (p.cursor + chunk).min(total);
        let buf = stereo(&p.sound, p.cursor..end, p.pan);
        p.cursor = end;
        if let Err(e) = p.stream.put_data_f32(&buf) {
            eprintln!("warning: audio refill: {e}");
            return;
        }
    }
}

const STEREO_FRAME_BYTES: usize = 2 * std::mem::size_of::<f32>();

/// Frames `range` of `sound` as interleaved stereo, each channel scaled by `pan`.
fn stereo(sound: &Sound, range: std::ops::Range<usize>, pan: (f32, f32)) -> Vec<f32> {
    let ch = sound.channels.max(1);
    range
        .flat_map(|f| {
            let s = &sound.samples[f * ch..f * ch + ch];
            let (l, r) = (s[0], s[ch - 1]);
            [l * pan.0, r * pan.1]
        })
        .collect()
}

fn load_wav(path: &Path) -> Result<Sound, Error> {
    let wav =
        AudioSpecWAV::load_wav(path).map_err(|e| Error::Sdl(format!("{}: {e}", path.display())))?;
    let bytes = wav.buffer();
    let samples: Vec<f32> = match wav.format {
        AudioFormat::U8 => bytes.iter().map(|&b| (f32::from(b) - 128.0) / 128.0).collect(),
        AudioFormat::S16LE => bytes
            .chunks_exact(2)
            .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0)
            .collect(),
        AudioFormat::F32LE => bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        other => {
            return Err(Error::Sdl(format!(
                "{}: unsupported sample format {other:?}",
                path.display()
            )));
        }
    };
    Ok(Sound {
        rate: wav.freq,
        channels: usize::from(wav.channels).max(1),
        samples,
    })
}

fn load_soundfont(path: &Path) -> Result<Arc<SoundFont>, Error> {
    let file = File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let sf = SoundFont::new(&mut BufReader::new(file)).map_err(|e| Error::Midi {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    Ok(Arc::new(sf))
}

/// Starts rendering every `.mid` in `sons` (up to [`MAX_PRELOAD_MIDI`]) on its own thread, so the
/// first `play_music` does not stall the loop for seconds.
fn preload_midi(
    sons: &Path,
    sf: &Arc<SoundFont>,
) -> HashMap<String, JoinHandle<Result<Sound, Error>>> {
    let files = thefly_assets::files_with_ext(sons, "mid").unwrap_or_default();
    files
        .into_iter()
        .take(MAX_PRELOAD_MIDI)
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_owned();
            let sf = Arc::clone(sf);
            Some((name, std::thread::spawn(move || render_midi(&path, &sf))))
        })
        .collect()
}

fn midi_err(path: &Path, reason: impl Into<String>) -> Error {
    Error::Midi {
        path: path.to_path_buf(),
        reason: reason.into(),
    }
}

/// Pre-renders a whole MIDI file to interleaved stereo f32 (~47 MB for the 134 s `i_music.mid`).
fn render_midi(path: &Path, sf: &Arc<SoundFont>) -> Result<Sound, Error> {
    let file = File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let midi =
        MidiFile::new(&mut BufReader::new(file)).map_err(|e| midi_err(path, e.to_string()))?;
    let synth = Synthesizer::new(sf, &SynthesizerSettings::new(MIDI_RATE))
        .map_err(|e| midi_err(path, e.to_string()))?;
    let mut seq = MidiFileSequencer::new(synth);
    let midi = Arc::new(midi);
    // shortcut: no loop-point handling; the whole file repeats from the start
    seq.play(&midi, false);
    if !(0.0..=MAX_MIDI_SECONDS).contains(&midi.get_length()) {
        return Err(midi_err(path, format!("longer than {MAX_MIDI_SECONDS} s")));
    }
    let frames = (midi.get_length() * f64::from(MIDI_RATE)).ceil() as usize;
    let (mut left, mut right) = (vec![0.0f32; frames], vec![0.0f32; frames]);
    seq.render(&mut left, &mut right);
    let samples = left
        .iter()
        .zip(&right)
        .flat_map(|(l, r)| [*l, *r])
        .collect();
    Ok(Sound {
        rate: MIDI_RATE,
        channels: 2,
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_device_every_play_is_an_honest_none() {
        let mut a = Audio::new(None, Path::new("/nonexistent"), None);
        assert!(!a.enabled());
        assert!(a.play("x.wav", 1.0).unwrap().is_none());
        assert!(a.play_loop("x.wav", 1.0).unwrap().is_none());
        assert!(a.play_music("x.mid", 1.0).unwrap().is_none());
    }

    #[test]
    fn mono_is_spread_to_both_channels_and_panned() {
        let s = Sound {
            rate: 11_025,
            channels: 1,
            samples: vec![0.5, -1.0, 0.25],
        };
        assert_eq!(stereo(&s, 1..3, (1.0, 0.5)), vec![-1.0, -0.5, 0.25, 0.125]);
    }

    #[test]
    fn stereo_keeps_its_channels() {
        let s = Sound {
            rate: 44_100,
            channels: 2,
            samples: vec![0.1, 0.2, 0.3, 0.4],
        };
        assert_eq!(stereo(&s, 0..2, (1.0, 1.0)), vec![0.1, 0.2, 0.3, 0.4]);
    }
}

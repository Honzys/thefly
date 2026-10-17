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

/// Decoded PCM plus the format SDL needs to convert it.
struct Sound {
    spec: AudioSpec,
    pcm: Vec<u8>,
}

struct Playing {
    id: Voice,
    stream: AudioStreamOwner,
    sound: Rc<Sound>,
    looping: bool,
}

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
        self.start(name, volume, false).map(|p| self.keep(p))
    }

    /// Like [`Audio::play`] but repeats until [`Audio::stop`].
    pub fn play_loop(&mut self, name: &str, volume: f32) -> Result<Option<Voice>, Error> {
        self.start(name, volume, true).map(|p| self.keep(p))
    }

    /// Replaces the music channel with `name` (`.wav` or `.mid` in `data/sons/`), looping.
    /// `Ok(None)` means nothing plays: no audio device, or MIDI without a SoundFont (warned).
    pub fn play_music(&mut self, name: &str, volume: f32) -> Result<Option<Voice>, Error> {
        self.music = None;
        self.music = self.start(name, volume, true)?;
        Ok(self.music.as_ref().map(|p| p.id))
    }

    pub fn set_volume(&mut self, voice: Voice, volume: f32) {
        let Some(p) = self.find(voice) else { return };
        if let Err(e) = p.stream.set_gain(volume) {
            eprintln!("warning: set volume: {e}");
        }
    }

    pub fn stop(&mut self, voice: Voice) {
        self.voices.retain(|p| p.id != voice);
        if self.music.as_ref().is_some_and(|p| p.id == voice) {
            self.music = None;
        }
    }

    /// Refills looping voices and drops finished ones. Called once per frame by the loop.
    pub(crate) fn pump(&mut self) {
        self.voices
            .retain(|p| p.looping || p.stream.queued_bytes().unwrap_or(0) > 0);
        for p in self.voices.iter().chain(self.music.as_ref()) {
            refill(p);
        }
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
    fn start(&mut self, name: &str, volume: f32, looping: bool) -> Result<Option<Playing>, Error> {
        if self.device.is_none() {
            return Ok(None);
        }
        let Some(sound) = self.sound(name)? else {
            return Ok(None);
        };
        let Some(dev) = &self.device else {
            return Ok(None);
        };
        let stream = dev
            .subsystem
            .new_playback_stream(&sound.spec, None)
            .map_err(sdl)?;
        dev.device.bind_stream(&stream).map_err(sdl)?;
        stream.set_gain(volume).map_err(sdl)?;
        stream.put_data(&sound.pcm).map_err(sdl)?;
        if !looping {
            // lets SDL play out the last partial sample frame so queued_bytes reaches 0
            stream.flush().map_err(sdl)?;
        }
        self.next_id += 1;
        Ok(Some(Playing {
            id: Voice(self.next_id),
            stream,
            sound,
            looping,
        }))
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

/// Keeps at least one copy of a looping sound queued; a frame stall longer than the sound leaves a gap.
fn refill(p: &Playing) {
    if !p.looping {
        return;
    }
    let queued = p.stream.queued_bytes().unwrap_or(0);
    if usize::try_from(queued).unwrap_or(0) < p.sound.pcm.len() {
        if let Err(e) = p.stream.put_data(&p.sound.pcm) {
            eprintln!("warning: audio refill: {e}");
        }
    }
}

fn load_wav(path: &Path) -> Result<Sound, Error> {
    let wav =
        AudioSpecWAV::load_wav(path).map_err(|e| Error::Sdl(format!("{}: {e}", path.display())))?;
    Ok(Sound {
        spec: AudioSpec::new(
            Some(wav.freq),
            Some(i32::from(wav.channels)),
            Some(wav.format),
        ),
        pcm: wav.buffer().to_vec(),
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
    let pcm = left
        .iter()
        .zip(&right)
        .flat_map(|(l, r)| [*l, *r])
        .flat_map(f32::to_le_bytes)
        .collect();
    Ok(Sound {
        spec: AudioSpec::new(Some(MIDI_RATE), Some(2), Some(AudioFormat::F32LE)),
        pcm,
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
}

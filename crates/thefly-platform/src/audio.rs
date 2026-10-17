//! Sound effects from `data/sons/` and one music channel (WAV or MIDI via a SoundFont).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use sdl3::AudioSubsystem;
use sdl3::audio::{AudioDevice, AudioFormat, AudioSpec, AudioSpecWAV, AudioStreamOwner};

use crate::error::{Error, sdl};

const MIDI_RATE: i32 = 44_100;
/// Longest MIDI file rendered, to bound memory on hostile input (~200 MB of f32 stereo).
const MAX_MIDI_SECONDS: f64 = 600.0;

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
        Self {
            device,
            sons: data_dir.join("sons"),
            soundfont,
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
    pub fn play(&mut self, name: &str, volume: f32) -> Result<Voice, Error> {
        self.start(name, volume, false).map(|p| self.keep(p))
    }

    /// Like [`Audio::play`] but repeats until [`Audio::stop`].
    pub fn play_loop(&mut self, name: &str, volume: f32) -> Result<Voice, Error> {
        self.start(name, volume, true).map(|p| self.keep(p))
    }

    /// Replaces the music channel with `name` (`.wav` or `.mid` in `data/sons/`), looping.
    /// MIDI without a SoundFont is skipped with a warning.
    pub fn play_music(&mut self, name: &str, volume: f32) -> Result<Voice, Error> {
        self.music = None;
        let playing = self.start(name, volume, true)?;
        let id = playing.as_ref().map_or(Voice(u64::MAX), |p| p.id);
        self.music = playing;
        Ok(id)
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

    fn keep(&mut self, p: Option<Playing>) -> Voice {
        let Some(p) = p else { return Voice(u64::MAX) };
        let id = p.id;
        self.voices.push(p);
        id
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
            (true, Some(sf)) => render_midi(&path, sf)?,
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

/// Pre-renders a whole MIDI file to interleaved stereo f32 (~47 MB for the 134 s `i_music.mid`).
/// shortcut: the first play blocks the loop for the render (seconds in debug builds); stream it if that hitch matters.
fn render_midi(path: &Path, sf: &Arc<SoundFont>) -> Result<Sound, Error> {
    let midi_err = |reason: String| Error::Midi {
        path: path.to_path_buf(),
        reason,
    };
    let file = File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let midi = MidiFile::new(&mut BufReader::new(file)).map_err(|e| midi_err(e.to_string()))?;
    let synth = Synthesizer::new(sf, &SynthesizerSettings::new(MIDI_RATE))
        .map_err(|e| midi_err(e.to_string()))?;
    let mut seq = MidiFileSequencer::new(synth);
    let midi = Arc::new(midi);
    // shortcut: no loop-point handling; the whole file repeats from the start
    seq.play(&midi, false);
    if !(0.0..=MAX_MIDI_SECONDS).contains(&midi.get_length()) {
        return Err(midi_err(format!("longer than {MAX_MIDI_SECONDS} s")));
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

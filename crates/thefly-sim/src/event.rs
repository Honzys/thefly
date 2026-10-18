//! What one tick asks the frontend to do. The sim never calls out; [`crate::Sim::tick`] returns
//! these in emission order and the frontend performs them.

/// A DirectSound slot, 0..=58 (`game-spec.md` §8.2). The file is [`SOUND_FILES`]`[slot]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundSlot(pub u8);

impl SoundSlot {
    pub const MENU_MELODY: Self = Self(0);
    pub const MENU_COWBOY: Self = Self(1);
    pub const MENU_PIG: Self = Self(2);
    pub const MENU_SELECT: Self = Self(3);
    pub const MENU_PIG_SWEAR: Self = Self(4);
    pub const MENU_PIG_KICKED: Self = Self(5);
    pub const BOOM: Self = Self(6);
    pub const FLY_BUZZ: Self = Self(7);
    pub const SHOP_COCKROACH: Self = Self(8);
    pub const SHOP_CONVEYOR: Self = Self(9);
    pub const SHOP_MACHINE: Self = Self(10);
    pub const MONEY: Self = Self(11);
    pub const ERROR: Self = Self(12);
    pub const TRAP_BUILD: Self = Self(13);
    /// 14..=21, the eight animal sounds.
    pub const AMBIENT_FIRST: Self = Self(14);
    /// 22..=26, the five footsteps.
    pub const STEP_FIRST: Self = Self(22);
    pub const SWING: Self = Self(27);
    pub const FLY_RICOCHET: Self = Self(28);
    pub const FLY_CRASH: Self = Self(29);
    pub const VOICE_WIN: Self = Self(30);
    pub const VOICE_HATE: Self = Self(31);
    pub const VOICE_MISS: Self = Self(32);
    pub const VOICE_LOSE: Self = Self(33);
    pub const VOICE_SHOUT: Self = Self(34);
    pub const TURBO: Self = Self(35);
    pub const SUPERFLY: Self = Self(36);
    pub const GOODIE_FARMER: Self = Self(37);
    pub const GOODIE_FLY: Self = Self(38);
    pub const ZAPPER_ARC: Self = Self(39);
    pub const ZAPPER_HUM: Self = Self(40);
    pub const ASPIRATE: Self = Self(41);
    pub const FLYING_GOODIE: Self = Self(42);
    pub const ZAPPER_GRILL: Self = Self(43);
    pub const FAN: Self = Self(44);
    pub const FAN_HIT: Self = Self(45);
    pub const SPLASH: Self = Self(46);
    pub const WINCH: Self = Self(47);
    pub const MIDGES: Self = Self(48);
    pub const RING_A: Self = Self(49);
    pub const RING_B: Self = Self(50);
    pub const CHEER_FLY: Self = Self(51);
    pub const COUNTDOWN: Self = Self(52);
    pub const CHEER_FARMER: Self = Self(53);
    pub const TROPHY: Self = Self(54);

    pub fn file(self) -> &'static str {
        SOUND_FILES[usize::from(self.0)]
    }
}

/// Slot to file stem under `data/sons/`. The on-disk extension case (`.wav`/`.WAV`) is resolved
/// by the frontend case-insensitively, as `thefly_assets::files_with_ext` does.
pub const SOUND_FILES: [&str; 59] = [
    "i_melody",
    "i_Cowboy",
    "i_pig",
    "i_ha",
    "i_fucking_pig",
    "i_kickpig",
    "z_boom",
    "z_Bzz",
    "g_blatte",
    "g_tapis",
    "g_machine",
    "g_money",
    "gp_error",
    "p_build",
    "j_birds1",
    "j_Birds2",
    "j_sheep",
    "j_dog1",
    "j_dog2",
    "j_cow",
    "j_rooster",
    "j_chicken",
    "j_pas1",
    "j_pas2",
    "j_pas3",
    "j_pas4",
    "j_pas5",
    "j_frappe_tap",
    "j_ricoche_fly",
    "j_crash_fly",
    "j_fvoix_gagne",
    "j_fvoix_hate",
    "j_fvoix_rate",
    "j_fvoix_perdu",
    "j_fvoixhaaa",
    "j_turbo",
    "j_super_fly",
    "j_goodie",
    "j_goodie",
    "jpt_eclair",
    "jpt_tuemouc",
    "j_aspi",
    "j_goovolant",
    "jpt_grillemouc",
    "jpv_ventilo",
    "jpv_chocventilo",
    "j_splash",
    "j_treuil",
    "j_moucherons",
    "b_ring",
    "b_ring",
    "b_iha",
    "b_321",
    "b_ihatap",
    "w_glingcup",
    "w_vibrofly1",
    "w_vibrofly2",
    "w_vibrofarmer1",
    "w_vibrofarmer2",
];

/// Where a sound sits. The original's positional helpers (`FUN_0040d005`, `FUN_0040d1b2`) pan
/// and attenuate by screen x; their curve is not decoded (open item O14), so the frontend maps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pan {
    #[default]
    Centre,
    /// Raw DirectSound pan, −10000..=10000.
    Fixed(i32),
    /// Positional, from the source's screen x (and the source width, centred on it).
    ScreenX { x: i32, w: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayParams {
    /// DirectSound hundredths of a dB, −10000..=0. 0 is full volume.
    pub volume_cb: i32,
    /// SetFrequency before playing, in Hz. None keeps the file's rate.
    pub freq_hz: Option<u32>,
    pub pan: Pan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundOp {
    Play(PlayParams),
    Loop(PlayParams),
    Stop,
    /// Out-of-range values (+5000 for slot 29, +8000 for slot 6) are passed through; the
    /// original call most likely failed and left the volume unchanged (fly-and-world §9).
    SetVolume(i32),
    SetFrequency(u32),
    /// Positional update of a playing loop (the buzz, the winch).
    Move(Pan),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Music {
    /// `music_game.wav` or `music_game2.wav`, picked per match.
    Game(bool),
    Goodies,
    Bonus,
    /// `i_music.mid`.
    Menu,
    /// `w_music_winner.mid`.
    Winner,
    Stop,
}

impl Music {
    pub fn file(self) -> Option<&'static str> {
        Some(match self {
            Music::Game(false) => "music_game.wav",
            Music::Game(true) => "music_game2.wav",
            Music::Goodies => "music_goodies.wav",
            Music::Bonus => "music_bonus.wav",
            Music::Menu => "i_music.mid",
            Music::Winner => "w_music_winner.mid",
            Music::Stop => return None,
        })
    }
}

/// Cosmetic effects the frontend draws on its own. They do not feed back into gameplay; anything
/// that does (the splat duration, the appear animation) is a sim timer instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// `stardust` burst, 30 particles (goodie use, respawn).
    Stardust { x: i32, y: i32 },
    /// Landing dust of size weight × 20 / 130 (fly-and-world §5.3).
    Dust { x: i32, y: i32, size: i32 },
    /// Swatter shockwave at display scale power × 70 + 150 (ai-and-rules §1.3).
    Shockwave {
        x: i32,
        y: i32,
        scale: i32,
        sparks: bool,
    },
    /// The fly's crash animation, falling toward (850, 850).
    Crash { x: i32, y: i32 },
    /// 60 + 11 splat particles; `dir` is the fan's push direction, 0 for none.
    Splat { x: i32, y: i32, dir: i32 },
    /// Boot floor impact.
    Stomp { x: i32, y: i32 },
    /// Flying-goodie spawn puff.
    GoodieSpawn { x: i32, y: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFile {
    /// `scoot.btt`.
    Scores,
    /// `boite.btt`.
    Options,
}

impl SaveFile {
    pub fn file(self) -> &'static str {
        match self {
            SaveFile::Scores => "scoot.btt",
            SaveFile::Options => "boite.btt",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Sound {
        slot: SoundSlot,
        op: SoundOp,
    },
    Music(Music),
    /// `i_bouton.WAV`, the UI click; `volume_cb` and `freq_hz` as in [`PlayParams`].
    Click {
        volume_cb: i32,
        freq_hz: Option<u32>,
    },
    Effect(Effect),
    /// The screen changed this tick (the new one is `sim.state().screen`).
    ScreenChanged,
    /// Write these bytes to the save file, relative to the working directory.
    Save {
        file: SaveFile,
        bytes: Vec<u8>,
    },
}

impl Event {
    pub fn play(slot: SoundSlot) -> Self {
        Event::Sound {
            slot,
            op: SoundOp::Play(PlayParams::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_names_match_the_spec_table() {
        assert_eq!(SoundSlot::FLY_BUZZ.file(), "z_Bzz");
        assert_eq!(SoundSlot::WINCH.file(), "j_treuil");
        assert_eq!(SoundSlot(58).file(), "w_vibrofarmer2");
        assert_eq!(SoundSlot::TROPHY.file(), "w_glingcup");
    }
}

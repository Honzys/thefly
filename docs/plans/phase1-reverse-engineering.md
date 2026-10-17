# Phase 1: reverse engineering

Goal: document every original file format and the game's behaviour well enough to write a new engine from the docs alone. Phase 2 (the Rust + SDL3 engine) gets its own spec once this phase lands.

## Inputs (local only, never committed)

- Original release: `~/projects/thefly/original/The Fly Release 1.2 - 15-12-02/`
  - `The Fly Release 1.2.exe`: 443 KB PE32, i386, imports DDRAW, DSOUND, DINPUT, WINMM.
  - `data/rsc/*.irc` (135): 16-byte header, then an 8-bit BMP. Palette entry 0 is `FF 00 FF` (likely the colour key).
  - `data/bor/*.bor` (29): u16 grid, `0xFFFF` = empty. Probably per-object collision masks.
  - `data/sons/`: PCM WAV and MIDI.
  - `data/rsc/Intro.avi`, two TTF fonts.
  - `scoot.btt` (488 B) and `boite.btt` (8 B): raw struct dumps, probably high scores and settings.
- Decompiler output goes to `~/projects/thefly/re/`.

## Clean-room boundary

- Committed docs describe formats and behaviour in our own words: tables, constants, state machines, pseudocode.
- No decompiled C, disassembly listings or copied original strings beyond short identifiers (file names, a few UI labels) are committed.
- No original asset bytes are committed, including test fixtures. Tests that need real data read `THEFLY_DATA` and skip when it is unset.

## Task A: asset formats and Rust parsers

Owns: `Cargo.toml` (workspace), `crates/thefly-assets/`, `docs/formats/`.

- Cargo workspace with one library crate `thefly-assets` and a `thefly-extract` binary inside it. The binary dumps a data dir to PNG, WAV copies and a manifest JSON.
- Parsers for `.irc` (header fields identified, image decoded with colour key), `.bor` (dimensions inferred and validated against the matching `.irc`), `.btt` (both files).
- Probe `Intro.avi` codec and document how phase 2 can play it, or whether it should transcode at install time.
- `docs/formats/<format>.md` per format. Each doc gives the byte layout, field meanings, the evidence for each claim, and open questions.

Acceptance:
- `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` are clean.
- With `THEFLY_DATA` set, every `.irc` and `.bor` in the release parses, and the extractor writes PNGs that look right on visual spot check.
- Synthetic unit tests (built in-memory, not from original bytes) cover header parsing and error cases.

## Task B: executable behaviour spec

Owns: `docs/re/`, plus local `~/projects/thefly/re/` (not committed).

- Confirm the exe is unpacked. Identify the compiler and linker.
- Run Ghidra headless to decompile all functions to `~/projects/thefly/re/`. Mine strings, MSVC RTTI class names and resources.
- Optional: run the game under Wine plus Xvfb for screenshots of the menus and levels as behavioural ground truth.
- Write `docs/re/game-spec.md` covering:
  - Screens and flow: intro, menus, options, game modes, win and lose, high scores.
  - Input: controls and devices.
  - Game objects: each entity, its states, its update rules, and which asset files it uses.
  - Timing: frame rate, tick logic, physics constants, scoring, difficulty, level and round structure.
  - Rendering: resolution, layers, sprite sheets, how `.bor` masks are used.
  - Audio: which sound plays on which event, music handling.
  - Save files: what `scoot.btt` and `boite.btt` hold.
  - Asset map: every file in `data/` to the place it is used.
- Each claim cites a function address in the local dump, so a reviewer with the dump can check it.

Acceptance:
- A reader who has never seen the exe can tell from the spec what happens on every screen and in every game mode.
- No decompiled code in the committed doc.

## Cross-review

Each worker reviews the other's docs where they meet: `.bor` and `.irc` usage, and the save files.

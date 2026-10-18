# Roadmap

Status as of 2026-10-18, and what is left before a 1.0 that plays exactly like The Fly 1.2.

## Where we are

- The whole game runs on real data: boot, intro, menus, options, 1P and 2P matches, the shop, the bonus round, the winner and stats screens, high scores and credits.
- The `thefly-sim` crate is deterministic: a fixed 65 Hz tick with the original's MSVC `rand`, so the same seed and keys give the same game.
- Every rule in it comes from `docs/re/`. That folder holds behaviour notes from the decompiled code, plus Wine observations in `observed.md`.
- 44 PRs are merged. 459 tests pass, including hand-derived rand-order goldens. CI is green on Linux, Windows and macOS.
- Soaks run from boot to the menu: 500 synthetic matches, and 30 to 100 random-key matches on real data, all without a panic.
- A `v*` tag builds release archives for Linux, Windows and macOS universal (`.github/workflows/release.yml`). No tag has been pushed yet.

## How the work runs

Each round has two steps.

1. **RE oracle.** A worker with access to the local decompiler output answers a batch of questions. Each question is one of the port's `shortcut:` guesses. The answers are written in `docs/re/answers.md` in our own words, with a function address, a [C]/[I] tag and a verdict against the port: correct, wrong, or port decision.
2. **Fix workers.** Each worker owns a separate set of files. It applies the answers test-first, using goldens worked out by hand from the seed-1 rand sequence, then runs the soak and the real-data run, and lands through CI.

Six answer rounds are applied. The round-6 fixes are half done (next section).

## Next steps

### 1. Finish round 6 (in progress, stopped)

Two worktrees hold uncommitted, unfinished work. Resume each with a fresh worker that is given the round-6 sections of `answers.md`:

- **`worktrees/r9-intro`** (`menu.rs`):
  - the intro skip is ignored during the pig and once its 3 s timer runs
  - skip settles the decor and drops the pig instead of jumping to the end pose
  - the first intro fly and the 900 ms of nothing at the start
  - the credits letterbox shrinks on the Esc tick
- **`worktrees/r9-misc`**:
  - a Play on a slot that is still playing does nothing (`mixer.rs`)
  - a falling midge keeps its x speed
  - the bonus AI aims from the swatter centre at the target centre
  - 15 coin frames, not 16

### 2. Remaining sim guesses

`grep -rn 'shortcut:' crates/` finds 37, mostly in the frontend. Still open in the sim:

- the bonus swatter's start x (uninitialised memory in the original; 0 matches the captures)
- how dt is split each tick in the bonus round
- whether the bonus constructor zeroes the swing state
- whether a DirectSound Stop followed by Play rewinds the sound
- whether Esc in the bonus round behaves exactly as in a match

The DirectSound question needs a Wine run of the original, not the decompiled code.

### 3. Check against the original, not only the docs

The biggest remaining risk is a doc that is wrong in a way no test can catch.

- **Side-by-side traces.** Run the original under Wine with scripted input, and log the rand draw count and the fly and object positions per tick, with a hook or by reading process memory. Diff that against `Sim` on the same seed and keys. A match there is the real proof of "same gameplay".
- **Screenshot diffs.** Compare the port's screens with `re/shots/` captures at matching ticks.

### 4. Frontend and graphics

- Draw the midge splat (frame 4) and the zapped husk (frame 5).
- Settle the frontend `shortcut:`s in `draw/front.rs`, `draw/play.rs`, `draw/between.rs` and `fx.rs` against captures. These are particle looks, frame choices and font fallbacks.
- **Better graphics,** without touching gameplay:
  - an integer or smooth upscale filter (`--linear` exists)
  - widescreen that shows more of the scene, which needs a check that the camera stays a pure view
  - optional HD sprite packs that load in place of the original `.bor` sheets
- **The intro video** (`data/rsc/Intro.avi`): play it through ffmpeg or a pure-Rust decoder, skippable.
- **The cockroach dance** is not built.

### 5. Audio

- Bundle-free music. The game's MIDI needs a user-supplied SoundFont; document a free GM SoundFont and auto-detect common install paths.
- Confirm the mixer's volume and pan curve by ear against the original under Wine. The menu drone in particular should be quiet and mostly on the right.

### 6. Release 1.0

- Push `v0.1.0` to exercise the release job once, then launch the Windows and macOS builds on real machines. So far only the Linux build has been run.
- Gamepad support and key remapping, as optional extras.
- Save files currently go to the working directory. Move them to the per-user data directory, keeping the original `scoot.btt` and `boite.btt` formats.
- Run `/ponytail-review` on the full diff since the first commit before tagging 1.0.

## Rules that stay

- No original asset, executable or decompiled code is ever committed. Players bring their own `data/`.
- Engine workers never read `re/`; only RE oracle workers do. Docs describe behaviour, not code.
- Gameplay changes only through `docs/re/` evidence. Graphics and quality-of-life changes must leave the sim's tick output unchanged.

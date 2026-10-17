# Phase 2a: platform layer

Goal: a Rust + SDL3 runtime that later phases build the game on. It works the same on Linux, macOS and Windows. It contains no gameplay. Gameplay rules come from `docs/re/game-spec.md` in phase 2b.

## Scope

Owns: `crates/thefly-platform/` (library) and `crates/thefly/` (the game binary, which is a viewer for now). Workspace `Cargo.toml` gets the new members.

- **SDL3 binding.** Use the `sdl3` crate, statically built from source, so CI and users need no system SDL. Record any NixOS shell packages needed in `shell.nix` at the repo root.
- **Window and presentation.**
  - The game renders at a fixed logical resolution, a parameter that defaults to 800x600 (`ac0.irc` is 800x600; the spec will confirm).
  - Presentation scales it to any window size with aspect-correct letterboxing.
  - Scale filter is selectable: nearest, or linear for the "better graphics" mode.
  - Fullscreen toggle on Alt+Enter and F11. Vsync on.
- **Game loop.**
  - Fixed-timestep simulation at a configurable tick rate, defaulting to 65 Hz, the original's rate (`docs/re/game-spec.md` §5.1).
  - Rendering is decoupled and uncapped with vsync, and receives an interpolation alpha.
  - The loop caps catch-up ticks per frame (8 max) so it never spirals.
- **Assets.**
  - `AssetStore` loads `.irc` through `thefly-assets` into SDL textures with the FF00FF colour key applied, lazily and cached.
  - The data dir comes from a `--data <path>` CLI argument, else `THEFLY_DATA`, else `./data` next to the executable. A clear error names what is missing.
- **Audio.**
  - Load WAV from `data/sons/`. Play one-shot and looping sounds with per-voice volume. Support music on a separate channel.
  - MIDI: play the two `.mid` files with a pure-Rust synth (`rustysynth`). The SoundFont is user-supplied or a freely licensed one that is downloaded, never committed. Without a SoundFont, MIDI is silently skipped and a warning is logged.
- **Input.** Keyboard and mouse state per tick (pressed, held, released), plus mouse position mapped from window to logical coordinates through the letterbox.
- **Text.** Render TTF text with the original fonts from `data/rsc/` (`sdl3` ttf feature).
- **Viewer binary.** `thefly` opens a window and cycles through every `.irc` with the arrow keys. It shows file name and dimensions as TTF text and plays a WAV on Space. This proves every subsystem end to end.

## Boundaries

- No gameplay or screen logic.
- Never commit original data or SoundFonts.
- The platform layer exposes small traits and structs that phase 2b calls: `Platform`, `Renderer` (draw sprite at position, frame index, flip, alpha; draw text; fill rect), `Audio`, `Input`, and `run(game: impl Game)` with `update(&mut self, input) / render(&self, r, alpha)`.

## Acceptance

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` are clean locally. CI is green on all three OSes, with the CI matrix building the binary.
- Unit tests cover the pure parts: the fixed-timestep accumulator (catch-up cap, alpha), the window-to-logical mouse mapping with letterbox, and data dir resolution order.
- Run the viewer under Xvfb with real data, take a screenshot showing a sprite with transparency and text, and check it with the Read tool.

# thefly

An open-source, cross-platform reimplementation of the engine for **The Fly** (Release 1.2, 2002), a French freeware PC game for Windows 98–XP and DirectX 7. The goal is to run it natively on Linux, macOS and Windows.

## You need the original game data

This repository contains **no original game assets**. The original game is freeware with no license ("talk about it around you, but do not sell it"), so its art, sound, video and executable cannot be redistributed here.

To play, get a copy of the original release and point the engine at its `data/` directory; see [Playing](#playing).

## Playing

### Get the game data

You supply the original game yourself. The Fly 1.2 is freeware; its original download page was `dlxcaiman.net`. You need the extracted release folder (it contains `data/` with `rsc/` and `sons/`). Do not ask the project for the files.

Download a build from the [Releases](https://github.com/Honzys/thefly/releases) page, or [build from source](#building-from-source).

### Point the engine at the data

The engine looks in this order and uses the first one that is set:

1. `--data <dir>`
2. the `THEFLY_DATA` environment variable
3. `./data` next to the executable

The path can be the `data/` folder itself or the release folder that contains it. A path that is set but wrong is an error; there is no fall-through.

```sh
thefly --data "/path/to/The Fly Release 1.2/data"
THEFLY_DATA="/path/to/The Fly Release 1.2" thefly
```

Saves (`scoot.btt` high scores, `boite.btt` options) are written to the working directory.

### Keys

| Action | Fly | Farmer |
| --- | --- | --- |
| Move | Arrow keys | T up, G down, F left, H right |
| Grab / drop | Enter | |
| Swing | | Tab |
| Use goodie | Right Shift | Q (cycles through the goodies) |

In menus: arrows move, Enter selects, Esc goes back.

| Key | Effect |
| --- | --- |
| Esc | back; in a match, opens the quit prompt |
| Alt+Enter, F11 | toggle fullscreen |

### Optional extras

- **SoundFont for music.** The music is MIDI. Without a General MIDI `.sf2` file it is skipped (with a warning); sound effects still play. Pass `--soundfont <file.sf2>` or set `THEFLY_SOUNDFONT`.
- **Arial Black.** The game's text uses Arial Black. The engine looks for `ariblk.ttf` or `Arial Black.ttf` in `data/rsc/` and in the operating system's fonts, and falls back to `casual.ttf` if it finds neither. The stand-in is wider than the original face.

### Command-line flags

| Flag | Meaning |
| --- | --- |
| `--data <dir>` | game data folder |
| `--soundfont <file.sf2>` | SoundFont for the MIDI music |
| `--linear` | smooth scaling instead of sharp pixels |
| `--window <w>x<h>` | initial window size, for example `1280x720` |
| `--viewer` | browse the game's images and sounds instead of playing |
| `--seed <n>` | fixed random seed |
| `--keys`, `--shot`, `--exit-after-shots`, `--warp` | scripted input and screenshots, for testing |
| `-h`, `--help` | list the flags |

### Building from source

You need Rust 1.85 or newer and CMake. SDL3 is built from source and linked statically by the `sdl3` crate, so no SDL install is needed.

```sh
cargo build --release --bin thefly
```

- **Linux:** a C toolchain plus the X11, Wayland and audio development packages. On Debian or Ubuntu: `libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxi-dev libxfixes-dev libxss-dev libxtst-dev libxcb1-dev libxkbcommon-dev libwayland-dev libwayland-bin wayland-protocols libdecor-0-dev libegl-dev libgl-dev libasound2-dev libpulse-dev`. On NixOS, `nix-shell` uses `shell.nix`.
- **macOS:** Xcode command-line tools and CMake (`brew install cmake`).
- **Windows:** Visual Studio Build Tools (MSVC) and CMake.

The release builds on the Releases page are unsigned. On macOS, Gatekeeper blocks them at first launch: clear the quarantine flag with `xattr -d com.apple.quarantine thefly` or allow it in System Settings, Privacy & Security. On Windows, SmartScreen may warn; choose "More info", then "Run anyway".

### Project status

Early. The sim and the engine cover the menus, one- and two-player matches, the shop and the bonus rounds; behaviour is rebuilt from written specifications and is still being checked against the original, so expect differences. Bug reports are welcome.

## Clean-room policy

The engine is written from behavioural specifications in `docs/`, not copied from decompiled code. Decompiler output stays on the researcher's machine and is never committed.

## License

The engine source is GPL-3.0-or-later. See `LICENSE`. That license does not cover the original game's assets, which remain the property of their authors.

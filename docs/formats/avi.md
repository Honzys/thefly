# `Intro.avi`

`data/rsc/Intro.avi`, 1,351,680 bytes. Probed with `ffprobe` (ffmpeg 9.0.1).

| Stream | Value |
|---|---|
| Container | AVI |
| Duration | 8.00 s, 192 frames (counted) |
| Video | Intel Indeo Video 5 (FourCC `IV50`), 640x480, 24 fps, yuv410p, about 629 kb/s |
| Audio | PCM signed 16-bit little-endian, 22050 Hz, stereo, 705 kb/s |

`ffmpeg -i Intro.avi -f null -` decodes the whole file with no errors, so libavcodec's Indeo 5 decoder handles it.

## Phase 2 recommendation

Indeo 5 is a dead 1990s codec. No Rust crate decodes it and SDL3 has no video playback. Do not decode it at runtime.

Convert once, at extract/install time, with the ffmpeg CLI, to formats the engine already handles:

1. Video: `ffmpeg -i Intro.avi -q:v 2 intro/%04d.jpg` (or PNG). 192 frames at 640x480; JPEG is roughly 5-10 MB in total.
2. Audio: `ffmpeg -i Intro.avi -vn intro.wav` (same PCM as the sound effects).
3. Playback: upload one frame per 1/24 s to an SDL texture and play the WAV through the normal audio path. Drive the frame index from the audio clock to avoid drift.

The current `thefly-extract` does not run ffmpeg; adding that is a phase 2 task. If `ffmpeg` is missing at install time, the engine should skip the intro (it is cosmetic) instead of failing.

A runtime alternative is linking ffmpeg (`ffmpeg-next`) and decoding `Intro.avi` directly. It avoids the conversion step but makes the engine depend on a system ffmpeg build, which is worse for a self-contained cross-platform binary.

No original bytes are committed, so any converted output stays local like the rest of the data.

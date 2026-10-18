# Observed behaviour of the original under Wine

Phase 1d ground truth. The original The Fly 1.2 was run under Wine on a headless display, driven by synthetic key presses and recorded. Every claim here comes from that run and is tagged [O] (observed). Where a measured value supports a reading, it is tagged [I] (inference). The spec values compared against are from `game-spec.md`, `screens-and-rendering.md`, `ai-and-rules.md` and `fly-and-world.md`.

Screenshots and recordings stay local (`re/shots/`, not committed). This file holds numbers only.

## 1. Setup and its caveats

- **Stack** [O]:
  - Wine 11.0 (old-style WoW64; the game runs as a 32-bit process)
  - Mesa llvmpipe (software GL) and Wine's `renderer=gl`
  - Xvfb 800×600 at depth 24, inside a Wine virtual desktop of 800×600
- **No sound device.** A null ALSA output was used. Audio was not observed.
- **Intro video patched out.** This Wine build cannot open the intro AVI and the game aborts. A copy of the exe changes one immediate (`push 0x8c` → `push 0x0a` at VA 0x407AED) so boot goes straight to the logo state. The intro video was therefore not observed. Nothing else in the exe was changed.
- **Music Off.** With Music On, the winner screen froze (frames stopped about 14 s in, keys ignored) [O]. This is most likely the MIDI path under Wine with no sequencer [I]. Every run reaching the winner screen used Music Off.
- **Fades are not rendered.** No screen fade was ever visible. Every screen change is a single-frame cut (luma jumps in one frame, no ramp) [O]. This covers:
  - logo in and out (both by key and by timeout)
  - menu to loading, loading to game, game to shop
  - bonus to winner, winner to stats
  - Wine renderer variants tried for this: CSMT off, Vulkan (crashes), GDI (no D3D device), depth 16. Fades are drawn as D3D7 alpha quads (screens-and-rendering §1.4), so this is a gap in the emulation stack rather than in the game [I]. The fade *time* appears to be spent anyway (see the logo duration in §3).
- **Host load.** Other jobs ran at the same time (load average 4.4–5.3 on 4 cores). Rate measurements below are lower bounds.
- **Capture.** `ffmpeg -f x11grab` at 70 fps (one run at 150 fps), lossless. Sprite positions were read from colour masks over the captured frames.

## 2. Measurements against the spec

| Quantity | Method | Spec | Observed [O] | Verdict |
|---|---|---|---|---|
| Update rate | distinct frames per second over steady gameplay (framemd5), at 70 and 150 fps capture | 65 Hz tick | 62.0/s (70 fps capture), 63.3/s (150 fps capture). Winner count-up: 18 value changes in 0.275 s, about 62/s. Wine `Flip` calls with tracing on: 21–65/s, peak 65 | **slightly under 65**; under load, cannot separate from a true 65 Hz |
| Gameplay period | scene cut from gameplay to the goodies shop, timed from the first countdown digit, 2P Normal (two runs) | 2 min | 122.4 s from "3", 119.4 s from "Go!", 118.4 s from the banner leaving. The clock hand makes exactly one turn | confirms 2 min (start reference within ±1.5 s) |
| Period with Tutorial On (2P) | same; the keys page and tutorial page were shown, preset inventories present | 3 min | 122.5 s from "3", the same as Normal. The clock hand points right at +37 s and is back up at +120 s, so the game's own clock ran a 120 s period (one 2P run) | **contradicts**: 2 min, not 3 |
| End of period | frames around the cut | an end-of-period banner exists in the draw list | a direct cut from live gameplay to the goodies shop; no banner, no "time up" screen, no visible fade | **not seen** |
| Loading screen | scene cuts: menu/tutorial page → loading → first game frame (3 samples) | about 2.3 s (9 × 200 + 500 ms) | 2.75 s, 2.79 s, 2.69 s | **contradicts**: about 0.45 s longer. 9 × 250 + 500 ms = 2.75 s fits exactly, so the step wait may be 250 ms rather than 200 [I] |
| Countdown | scene cuts on the banner digits | one string per 1000 ms | "3" at 0, "2" at +0.987, "1" at +2.000, "Go!" at +3.012, banner gone at +4.025 s | confirms 1000 ms steps |
| Logo screen, untouched | luma timeline from launch | 500 ms fade in, auto exit after 10000 ms, 500 ms fade out | the logo is on screen for 11.7 s, then a hard cut to the menu intro | consistent with 10 s plus two fades of invisible alpha [I] |
| Logo skip | Return pressed at 6.2 s after launch | key accepted after 3000 ms | cut to the menu intro 1.6 s after the key | key accepted; the 1.6 s includes the fade-out time and the menu load [I] |
| Screen fades | luma ramps (signalstats) | 500 ms (logo, shops), 600 ms (winner, stats) | none visible (see §1) | **not verifiable** under this stack |
| Fly vertical speed | track the green fly body per 70 fps frame during a 0.6 s Down hold | maxY = 550 + 60 × level px/s (level 0: 550) | a steady 8.0 px per game frame. 127.8 px in 0.257 s wall-clock = 497 px/s. From rest to full speed in about 0.12 s. Bounces off the floor at y ≈ 448 | close to the cap. 550/65 = 8.46 px/tick; truncation to whole pixels per tick would give 8 [I] |
| Swing timing | red swatter-mesh pixel count per frame after a Tab press | frames 1,2 wind up; 3,4,4 hit; then retract 3,2,1,0: 9 ticks | sequence f1, f2, f3, f4, f4, f3, f2, f1, f0, from first raise to rest in 0.143 s | confirms the 9-tick shape (9/65 = 0.138 s) |
| Clock hand | red-pixel bbox in the clock face at 30 s intervals | starts pointing up; moves (π/2)/(period/4) per second; one full turn is [I] in the spec | up at +8 s, right at +37 s, down at +67 s, left at +97 s, up again at +123 s (2P Normal). Steps once per second, clockwise | confirms one turn per period, clockwise |

## 3. Boot and screen flow notes

- **Boot without keys** [O]:
  - The logo appears 1.4 s after launch.
  - The menu intro (pig and boot animation) starts at about 13 s.
  - The menu items appear about 45 s after launch.
- **Boot with keys** [O]: Return at about 6 s and again about 2.5 s later reaches the item-ready main menu in about 11 s. The local launcher script relies on this.
- **Main menu navigation** [O]:
  - From Play, Down selects Credits.
  - The difficulty page wraps: Up from the top lands on the last entry (Intelligent).
  - High scores reached from the menu close on Esc, not on Return.
- **Quit dialog** [O]: defaults to No, and the background keeps animating behind it.
- **Credits** [O]: credits cycle through name captions; Esc returns to the menu.
- **Keys pages** [O]:
  - 1P covers the left half ("1 Player : The Fly").
  - 2P shows both halves.
  - The art shows the farmer moving on T (up), F, G, H, with Tab to hit and the Q key (A on AZERTY) for goodies. The fly uses the arrows, Enter to catch and activate, and Right Shift to choose goodies.
  - This matches the code key map in game-spec §3. The spec's open item says the in-game help text names D, F, R, G; the keys pages seen here do not [O].
- **Period captions seen** [O]: "Breakfast Inside... Day 1", "Lunch Outside... Day 1", "Diner Inside... Day 1" (Extreme).
- **Shops** [O]:
  - The goodies shop opens straight after a period.
  - The traps shop was skipped whenever the farmer had nothing to place. It was not reached in these runs.
  - With Tutorial On, both sides start the shop with 0 $ but with preset goodie counts.
- **Bourse** [O]: a "Results" panel ranked the fly 1st and the farmer 2nd at 0 $ in the run observed.
- **Bonus round** [O]:
  - Letterboxed (black bars top and bottom).
  - The result dialog scores targets × 2 $: 86 × 2 $ and 90 × 2 $ in two runs. The farmer line reads 0 × 10 $.
  - The background without the dialog stays up for about 1.2 s before the cut to the winner screen.
- **Winner** [O]: titled "Final Results", with trophies and gauges. Enter leads on to the stats page. The stats page counts life time; a never-killed fly showed a maximum life time of 122 s, matching the period length.
- **Fly carrying** [O]: the fly latched onto an orange but did not lift it while Up was held for 2.5 s (fly strength level 0).

## 4. Positions

All coordinates are 800×600 screen pixels, inclusive bounding boxes of the coloured glyph or sprite pixels.

### 4.1 Main menu labels

Measured on the selected (green) state of each item. The spec gives 150×100 item cells by top-left corner (screens-and-rendering §2.2).

| Item (label shown) | Spec cell top-left | Spec cell centre | Observed label bbox [O] | Observed centre [O] | Offset |
|---|---|---|---|---|---|
| Play ("Play") | (257,431) | (332,481) | x 281..376, y 465..499 | (328,482) | (−4,+1) |
| Options ("Options") | (119,495) | (194,545) | x 141..246, y 512..563 | (194,538) | (0,−7) |
| Credits ("Credits") | (257,499) | (332,549) | x 259..393, y 532..567 | (326,550) | (−6,+1) |
| Quit ("Exit") | (385,497) | (460,547) | x 410..505, y 527..563 | (458,545) | (−2,−2) |

- Every label sits centred in its spec cell within 7 px [O]. The spec numbers confirm.
- The fourth item reads "Exit" on screen [O].

### 4.2 HUD and in-game overlays

| Element | Spec | Observed [O] | Verdict |
|---|---|---|---|
| Clock face `horloge` | 52×55 at (3,3) | wooden frame visible from about (4,5) to (52,57) | consistent (rim anti-aliasing) |
| Clock hand pivot | face + (25,26) = (28,29) | the hand's fixed end is at x 27..29, y 29..30 in all four quarter poses | confirms |
| Clock hand length / width | 15 px, 3 px wide, red | pose bboxes: up x 27..29 y 13..30; right x 27..43 y 28..31; down x 27..29 y 28..45; left x 13..29 y 27..30. That is a 3 px line reaching 15–16 px from the pivot | confirms |
| Loading sprite `spriteload` | 65×64 at (674,497) | animated pixels inside x 677..736, y 498..561 | confirms |
| Countdown digit | Joint 200, yellow outline, blue fill | "2": yellow outline x 324..497, y 216..355; blue fill x 361..471, y 219..328. Centre about (410,285) | style confirms; centred about 10 px right of the screen centre |
| Countdown caption | Joint 40 italic, y ≈ 380 | white glyphs x 164..665, y 382..419 | confirms if y is the top of the text |
| Farmer goodie plate `plaque_tap` | centred, x = (800 − w)/2, y 350 | grey plate x 335..468 (w 134), top edge y 547, running to the bottom of the screen. Shown only while the farmer's goodie selector is open | x **confirms**. y is **open**: a frame diff across the Q press changes only this plate, so nothing of the overlay is visible above y 547. Spec y 350 holds only if the asset has about 197 transparent rows on top; check the `plaque_tap` height |
| Farmer goodie icons | (349 + 36·i, 568) | icon sprites not visible. Their counters ("x20", "x15") start at about x 354 and x 390 at y ≈ 555, 36 px apart | slot layout **confirms** (349 + 36·i); the icons themselves are not drawn, likely because of the D3D alpha path [I] |

## 5. Not observed

- the intro video (patched out, see §1)
- any screen fade or alpha overlay (§1)
- the traps shop (no farmer trap was ever owned at a shop visit)
- a high-score name entry, 1P Highscore stats, and the farewell screen (these need a full 1P Highscore campaign)
- audio of any kind
- the fly lifting an object (strength 0 could not lift the orange in the time held)
- swing cooldown at higher farmer levels, and the update rate on an unloaded host

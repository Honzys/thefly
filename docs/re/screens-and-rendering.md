# The Fly 1.2: screens, layout and rendering

Phase 1c output: what each screen draws, where, with which asset and font, and how the engine animates it. This is a companion to `game-spec.md`. That file covers rules and flow; this one covers the look.

## Conventions

- **Tags.** [C] means read from code, [O] means observed under Wine, [I] means inferred. A function address after a tag says where the code claim comes from.
- **Coordinates.** Screen pixels on the 800×600 back buffer, origin at the top left, x to the right, y down. A sprite position is its top-left corner unless the text says otherwise.
- **Colours.** Given as RGB(r,g,b). The exe stores GDI COLORREF values as 0x00BBGGRR, and they are converted here.
- **Fonts.** "Joint" means the face "Joint by PizzaDude" (`JOINPD__.TTF`). "Arial Black" and "Arial" are system faces. All fonts are created at regular weight with antialiased quality (CreateFontA quality 2). A size is the font height in pixels.
  - Joint is used for menus, dialogs, banners, the bonus countdown, bourse labels and the large figures and headings of the end screens (winner scores, "Stats", "Your Score").
  - Arial Black is used for the HUD, bourse prices, stats rows, high-score rows, shop counts, the credits lines and the keys-page lines. Each screen section names the face it uses.
  - `casual.ttf` is registered at start-up, but no screen read here selects it [C for the screens read, I for the rest].
- **Assets.** An asset is named by its exe name and its file, using the tables in `game-spec.md` §9. For example, `i_soleil` is ac12.
- **Frames.** A frame is "w×h, c×r", meaning frame size and columns × rows. Frames are numbered from 1 in row-major order (see §1.2).
- **Wine.** No screen could be captured under Wine; §0 says why. Every layout claim below is therefore [C] or [I]. No claim is [O].

---

## 0. Behavioural capture under Wine: not achieved

**What was tried:**
1. The release was copied to `re/wine-game/` and the exe renamed to `fly.exe`. It was never run natively.
2. The prescribed shell, `nix-shell -p wineWowPackages.stable xvfb-run xdotool imagemagick`, was started.

**What failed:**
- There is no binary-cache hit for `wine-wow-11.0` on this host, so Nix builds Wine from source. That includes the mingw PE cross-build of every DLL and its tests.
- The first call timed out after 600 s and was left building in the background. It was still compiling (`dlls/riched20/tests`) more than 50 minutes later.
- That is past the 45-minute timebox, so no screenshot or frame-rate measurement exists. `re/shots/` is empty.

**What next:**
- `re/WINE.md` has the full recipe to run once the build is in the store.
- The build result persists in `/nix/store`, so the next attempt starts at once.
- The 65 Hz claim therefore stays [C]-only. See §1.6 for why wall-clock timing matters more than the tick rate for most animations.

---

## 1. Engine rendering model

### 1.1 Surfaces and blits [C]

- **Display:** DirectDraw 7, 800×600×16, with one back buffer flipped each tick (`FUN_00406803`, `FUN_004070ef`).
- **Opaque and colour-keyed images:**
  - These are drawn with BltFast. Flags 0x11 mean source colour key plus wait.
  - The colour key is magenta, RGB(255,0,255).
  - A failed blit with DDERR_SURFACELOST restores the surface and retries once.
- **Alpha and scaled images go through Direct3D 7.** Every fade, dust puff, tint, caption tween, HUD dimming and high-score highlight uses it.
  - `FUN_00418f34` draws one textured quad as a 4-vertex triangle strip.
  - Render state: colour-key enable on, alpha blending on, source blend SRCALPHA, destination blend INVSRCALPHA.
  - The vertex diffuse alpha is the per-draw alpha (0–255).
  - `FUN_00414229(sx, sy, s, …)` sets a scale on the sprite; `FUN_00414298(alpha, …)` draws it through this path.
  - Every fade, dust puff, "you win" caption, high-score highlight and HUD dimming below uses this path.
- **Text** is GDI TextOutA on the back buffer's DC, obtained with GetDC and released each time.
  - The background mode is transparent (SetBkMode 1).
  - "Outlined" text, `FUN_0044daa6`: the string is drawn four times in the outline colour at (±d, ±d), then once in the fill colour at (x, y). The usual d is 3 px.
- **Solid rectangles** (bars, panels) are GDI Rectangle + FillRect with a solid brush. FrameRect gives 1-px outlines.
- **Lines:** the clock hand is a GDI pen line.

### 1.2 Sprite object [C, ctor `FUN_00413a00`, loaders `FUN_00413ad0`/`FUN_00413b73`/`FUN_00413c28`]

**Sheet cutting:**
- A sprite holds one sheet. Frames are cut either by frame size (`FUN_00413ce0(fw, fh)`) or explicitly (`FUN_00414093(fw, fh, cols, rows)`).
- With `FUN_00413ce0`, cols = sheetW / fw and rows = sheetH / fh, using integer division. Frame count = cols × rows.
- With no cut call, the whole sheet is one frame.
- **Frame order is row-major:** frame 1 is the top-left cell, frames go left to right, then the next row.
- Most sheets are one row, but `i_soleil` (4×2), `Goodies` (5×2), `i_items1` (2×4) and `blood` (3×2) are grids. The frame cut of each sheet is given in this document (§4.6 and the screen sections); the code passes it at load time.

**Animation state:**
- Current frame, a [start, start+len) range, an animate flag, loop, ping-pong and direction (`FUN_004143c0` steps it).
- **Frame delay is wall-clock:** default 50 ms (`FUN_00413f92`), measured with GetTickCount (`FUN_00414051`). It does not depend on the tick rate.
- Without loop, the animation stops on the last frame. With ping-pong it reverses at each end.

**Position, drawing and clipping:**
- The position is an integer (x, y). Drawing clips to 0..799 × 0..599 (`FUN_00414611`).
- A world-space variant subtracts the camera and wraps at the world width (+0x2c, flag +0x30).

**Helpers:**
- `FUN_004135c2` / `FUN_00413825` draw a sheet tiled horizontally across 800 px. These are the scrolling clouds/bars style.

### 1.3 Mover (sprite + motion) [C, `FUN_00417xxx`, `FUN_004180af`]

- **Velocity** is in px/s (+0x20, +0x24). The time-based and FPS-based integrators (`FUN_00417db6`, `FUN_004180af`) are specified in `fly-and-world.md` §4.1.
- **Wander mode** (`FUN_00417144`, `FUN_004174c0`): specified in `fly-and-world.md` §6.2 (wander mover).
- **Optional clamp box** (`FUN_004171bd`, enabled by `FUN_00417225`).
- **Visibility flag** (`FUN_00418b94`).

**Alpha/scale tween** (`FUN_0041773d(scaleStart, scaleEnd, …, alphaStart, alphaEnd, ms)`, started by `FUN_004177e7`, advanced by `FUN_0041787c`):
- It interpolates alpha from alphaStart to alphaEnd over `ms`. `FUN_0041787c` returns non-zero when the tween ends.
- Drawn by `FUN_00417b93`.

### 1.4 Full-screen fade [C, `FUN_004123e0`, setup `FUN_00413219`, run `FUN_0041331b`, start `FUN_00413449`]

**What it draws:**
- One cell of `color_shade` (div8, 80×15 cut into 20×15, 4×1), scaled up to cover 800×600 (scale 40).
- Cell 1 is the fade colour for every screen fade [C]. Its pixel colour was not checked; black is assumed [I].

**Direction:**
- `FUN_00413449(on, +1)` starts at alpha 255 and runs down to 0. This is a fade *in* from the colour.
- `FUN_00413449(on, −1)` runs from 0 up to 255. This is a fade *out* to the colour.

**Speed:**
- Each tick, alpha changes by 255 / (fps × duration / 1000), so a fade lasts `duration` ms whatever the fps.
- Durations: 500 ms (logo, shops) and 600 ms (winner, stats).

**Completion:** `FUN_0041331b` returns true on the tick the fade reaches its end. Screens use that edge to change state.

### 1.5 Menu widget [C, `FUN_004148b0`; config `FUN_00414c87`; draw `FUN_00415003`/`FUN_00415e82`; input `FUN_00415599`]

**Contents:**
- Up to 5 decoration sprites (`FUN_00414e50(slot, …, x, y, fw, fh, animate)`).
- Up to 10 items (`FUN_004152de(i, label, value, x, y, isDefault)`).
- An optional cursor sprite (`FUN_00415084(…, fw, fh, animate, ms, loop, dx, dy)`), drawn at (itemX − dx, itemY − dy).

**Text mode:**
- Every label is drawn in the normal colour, then the selected one is redrawn in the selected colour.
- The font comes from `FUN_00415160(sel, normal, size, face)`.
- A toggle value is drawn at labelX + len × size + 20, or at an explicit position (`FUN_00415519`).
- A subtitle, if any, is drawn under the label (`FUN_0041537c`).

**Image mode:** item i shows sheet frame i+1+stride×i, or i+2+stride×i when selected (stride 1 for `i_items1`).

**Input:**
- Edge-triggered: a key counts on the tick it goes from up to down.
- Up/Down wrap around.
- In grid mode (`FUN_00414cdf`), arrows move in 2-D, and cells holding −1 or −2 are skipped in the direction of travel.
- Enter confirms; on a toggle item it cycles the value.
- Each move plays `i_bouton` (`FUN_0040df5b`).

### 1.6 Timing notes

- **Logic tick:** the state machine steps once per tick (`game-spec.md` §2.2 and §5.1; the 65 Hz rate is from the timer constant, not measured) [C].
- **Wall-clock elements** (independent of fps):
  - sprite frame delays (GetTickCount)
  - mover velocities (px/s, spread over fps)
  - fades (ms)
  - all screen timers (`FUN_004193df`/`FUN_004194a9`)
- **Per-tick elements** (they scale with the tick rate):
  - Count-up animations add a fixed amount *per tick*: +10 points on the winner and stats screens, +1 on the bourse bars.
  - Velocity changes in the menu intro add or subtract fixed px/s *per tick* (gravity-like).
  - A port must run these at the original tick rate to match the feel [C for the code, I for the consequence].

---

## 2. Front-end screens

### 2.1 Publisher logo (states 0x0a–0x13) [C, `FUN_00407b64`, `FUN_00407d60`]

| Element | Asset / text | Position | Notes |
|---|---|---|---|
| Backdrop | `Present` | (0,0) full screen | |
| Version label | "Release 1.2", default GDI font, white | (700,7) | |
| Fade | §1.4, 500 ms | full screen | Fades in on entry. A key is accepted after 3000 ms; after 10000 ms the screen fades out by itself. Fade-out done → state 0x14. |

### 2.2 Main menu scene [C, init `FUN_0041ae17`, render `FUN_0041d22a` / intro render `FUN_0041d3cc`, input `FUN_0041be05`, actions `FUN_0041c327`, pages `FUN_0041c95d`]

**Scene sprites (top widget, slots 0–4):**

| Slot | Asset | Frames | Rest position | Animation |
|---|---|---|---|---|
| 0 | `i_decors1` (800×1200) | 1 | (0, −600) after the intro | static; the tall sky/landscape is scrolled during the intro |
| 1 | `i_soleil` (2548×678) | 637×339, 4×2 = 8 | (162, decorY+450) | 100 ms, loop |
| 2 | `i_decors2` | 1 | (231, decorY+610) | static |
| 3 | `i_cochon_corps` | 1 | x 0, resting on the bottom edge (y = 600 − h) | static |
| 4 | `i_cochon_tete` | 250×345, 2 frames | x = 550 (pig body frame width), same y as the body | 50 ms; plays when kicked or on intro end |

**Menu items (image mode, `i_items1` 300×400 → 150×100, 2×4):**
- Grid navigation over a 3×2 layout with cells [−1, 0, −2 / 1, 2, 3].

| Item | Label (frame pair) | Position |
|---|---|---|
| 0 | Jouer / Play | (257,431) |
| 1 | Options | (119,495) |
| 2 | Crédits | (257,499) |
| 3 | Quitter / Quit | (385,497) |

**Ambient animation:**
- **Clouds:** 15 × `i_nuage`, frame 317×82, 3×1.
  - Random start x −100..899 and y −50..699, random frame 1–3.
  - Horizontal drift ±(10..29) px/s.
  - They wrap around the screen.
- **Dust puffs:** 15 × `DUST` (div10, 66×60, 3×1).
  - Spawned on the pig's landing (`FUN_0041de39`): x 120..569, at least 10 px apart; y 560..599.
  - Each puff grows from scale 0.1 by +(3..6)/100 per tick.
  - Alpha starts at 200 − 2s (s = 3..6) and falls by about s per tick.
  - Drawn through the D3D alpha path (`FUN_0041e025`).

**First-visit intro** (state at menu+0x32a4; later visits jump straight to the end pose, `FUN_0041e4a8`):

| Step | What moves | Numbers |
|---|---|---|
| 0 | Decor, sun and decors2 scroll up together | vy = −60 px/s. The small fly `i_PetiteMouche` (36×28, 4 frames, 20 ms) appears at (830,300) in wander mode and buzzes (slot 0). |
| 1 | Scroll eases out | Once decorY < −300: vy = −60 × (1 − (\|y\| − 300)/300), with the factor not below 0.2. The scroll stops when the decor bottom reaches 600 (decorY snaps to −600). |
| 2 | The big fly crosses | `transi-est` (120×94, frames 6–9, 20 ms) starts at x −120, random y 100..399, and flies right at 400 px/s with positional buzz. After x > 800 or 3000 ms, the pig is placed above the screen (y = −50 − h, vy 50). |
| 3 | The pig drops | vy += 120 px/s per tick. At the floor (y = 600 − h) it bounces with vy = −vy/3 until \|vy\| ≤ 370. Each bounce plays the boom sound (slot 6, volume −500, frequency 10000–14999 Hz) and spawns dust. |

- **Skip:** Enter or Esc after 900 ms on the first visit jumps to the end state (`FUN_0041e1be`).

**Sub-pages (second widget at menu+0xa14, text mode):**
- **Boot:** `i_botte` (270×575) is sprite 0, at x 200.
- **Font:** Joint 25. Selected RGB(52,255,0) green, normal RGB(0,168,255) blue.
- **Item x** = bootX − 55 + dx. **Item y** = the listed value.

| Page | Items (dx, y) | Extras |
|---|---|---|
| 5, play | 1 Player (+3,127); 2 Players (+3,177); HighScores (+13,252) | |
| 1, options | Music (0,120); Tutorial (+5,175); ViewKeys (−10,240) | On/Off values at (bootX−55, 140) and (bootX−55, 195) |
| 2, 1P mode | Normal (−5,130); Highscore (0,205) | "Mode" subtitle under each, at y 155 and 230 |
| 6, difficulty | Easy (−4,130); Normal (−4,180); Inteligent (+8,230) | the label is spelled this way in the exe |
| 0xb, 2P mode | Normal (−5,130); Extreme (−2,205) | "Mode" subtitles at y 155 and 230 |
| 3, length | 2 / 4 / 6 days (0,130 / 180 / 230) | |

- The currently stored option is preselected on entry.
- **Boot slide** (`FUN_0041e63c`):
  - **In:** starts at y = −50 − h with vy 200, then +90 per tick, until y = 350 − h.
  - **Then:** the pig head animates, a kick sound plays (slot 2 pan 500, and slot 5 at 11000–13999 Hz), and a 1000 ms timer starts. Pig and boot jiggle ±250 px/s between rest y and rest y + 15.
  - **Out:** vy −200, then −90 per tick, until off the top. It parks at y = −5 − h.
- **Esc** on a sub-page goes back to the parent page, or to the top menu.

### 2.3 Credits [C, `FUN_0041eb60`, `FUN_0041f3b6`]

- **Scene:** the whole menu scene slides *down* at vy 520, slowing by 5 px/s per tick.
- **Letterbox:** black bars grow 10 px per tick to 120 px, at the top and bottom.
- **Photos:**
  - `i_credit_seb` (200×111, 3 frames) at x 100 and `i_credit_gaga` (200×113, 3 frames) at x 400.
  - Both start at y −300 and fall with the scene.
  - Animation: 20 ms, ping-pong.
- **Text:**
  - Two lines in white Arial Black 27. One slides in from x 810 at y 530, the other from x −200 at y 50.
  - They hold 7000 ms, then slide out.
  - Five pairs in total. Their content is credits names, not reproduced here.
- **Return:**
  - The scene rises at vy −20, accelerating by −5 then −40 per tick.
  - It bounces with −v/2 until \|v\| < 61.
  - Boom sound at volume 300.

### 2.4 View-keys dialog (menu page 10) [C]

- `boitedlg` (div0, 510×400), centred, top-left (145,100).
- Three white Joint 18 lines at (265,200), (265,225) and (265,250).
- One OK button at (400,440). Enter or Esc closes it.

### 2.5 Quit / stop dialog (states 0x3c–0x42) [C, `FUN_0040922a`, `FUN_00409495`; dialog class `FUN_004116e3`, render `FUN_00411d0d`]

**Layout:**

| Element | Asset / text | Position |
|---|---|---|
| Frame | `boitedlg` 510×400 | (145,100) |
| Question, from the menu | two white Joint 20 lines asking whether to quit to Windows | (300,281) and (335,311) |
| Question, in a match | one white Joint 20 line, "Stop the game ?" | (325,281) |
| YES / NO | Joint 25; selected green RGB(52,255,0), normal blue RGB(0,168,255) | y 425 |

**Button positions:**
- **YES:** x = centred position for 3 characters − 127.5. **NO:** x = centred position for 2 characters + 127.5.
- The centring formula is dialogX + (510 − size × n / 2) / 2, which assumes each character is size/2 wide.
- The size field the code uses is not set on the first show, so on the very first show the buttons are a few pixels off. Steady state, with size 20: YES ≈ 257, NO ≈ 517 [C for the formula, I for the first-show values].
- **Default selection is NO.** Left/Right move between the buttons.

**Behind the dialog:** the screen underneath keeps rendering — the menu scene, the game with input zeroed, or the shop. So the dialog is an overlay, not a frozen frame [C].

### 2.6 Keys page (0x5a) and tutorial page (0xa0) [C, `FUN_00409964`, `FUN_00409ac2`, `FUN_00409d44`]

- **Keys page:** `touches` full screen at (0,0).
- **In 1P only:**
  - A solid fill covers x 0..399 over the full height. Its colour was not resolved [I].
  - Then two white Arial Black 35 lines: "1 Player : " at (130,245) and "The Fly" at (130,285).
- **Tutorial page:** `page_mongol` full screen at (0,0). Enter → gameplay.

---

## 3. Between-period screens

### 3.1 Goodies shop (states 0x1e–0x27) [C for init, `FUN_0042123c`; I for parts of the render, `FUN_0042322f`, `FUN_00422571`, `FUN_00423be2` not fully read]

**Menu widget:**
- Configured as 6 columns × 1 row (`FUN_00414c87(5, 0, 6, 1, 0)`), image mode with a cursor only.
- Items have no label art. The cursor sprite marks the choice.

| Element | Asset | Frames | Position | Animation |
|---|---|---|---|---|
| Backdrop | `g_background` (800×537) | 1 | (0,0) | static |
| Conveyor belt | `g_TapiRoulant` | 456×60 | (346,214) | 100 ms, loop, runs only while an item is moving [I] |
| Top bar | `g_barre_haut_tapette` | 1 | (0,0) | static |
| Bottom bars | `g_barres_bas` | 800×231 | (0,369) | 100 ms, no loop |
| Belt bars | `g_barres_tapis` | 1 | (414,67) | static |
| Cursor | `g_Selection` | 44×58 | item + (5,52) | 30 ms, loop |
| Machine | `g_Machine` | 1 | starts at (7,143) | wanders inside a small box (x 3..w+11, y 139..h+147), giving a shaking machine |
| Machine mask | `g_Machine_cache` | 1 | machine + (46,156) | drawn over the item coming out [I] |
| Goodie icons | `Goodies` (div14) | 51×50, 5×2 | the selected icon at (slotX, 5) | |
| Cockroach | `g_Blatte` | 110×180 | (500,400) | hidden until triggered [I] |
| Can | `can1` | 1 | x 805 (0x325), y 239 − h | decoration |
| Dead-fly tally | `g_flymort` | 100×75, 3×1 | | |
| Dust | `DUST` | 66×60, 3×1 | at the machine outlet | 20 puffs |

- **Item slots (x, y 3):** 44, 179, 309, 440, 570, 713.
  - Slots 0–4 are the five goodies. Slot 5 is the exit / next button.
  - The selected-icon x positions for the first five are 44, 179, 309, 440, 570.
- **Fonts:** Arial Black 22, Joint 43, Arial Black 30.
- Prices and counts are drawn in Arial Black 30, green RGB(0,255,0), as "x" followed by a number (`FUN_0042322f`).
- **Exit:**
  - Choosing slot 5 with no money left, or as the second side, fades out (500 ms) and continues.
  - Otherwise the other side's turn starts. The music slot 9 volume changes by −100.
- **Open:** the exact counter text positions and the purchase animation (item rides the belt, cockroach) are in `FUN_0042322f`, `FUN_00422571` and `FUN_00423be2`, which were not read line by line.

### 3.2 Traps shop (states 0x28–0x31) [C, `FUN_004244e2`; render not fully read]

| Element | Asset | Frames | Position |
|---|---|---|---|
| Backdrop | `p_traps` (800×600) | 1 | loaded with y −600; whether that is top or bottom y, and whether it slides in, was not checked |
| Cursor | `p_fleche` | 270×45 | item position, offset (0,0); 30 ms, loop |
| Cross | `p_croix` | 1 | y 254 |
| Trap icons | `p_icone_colle_mouche`, `p_icone_ventilo_gauche`, `p_icone_ventilo_droite`, `p_icone_tue_mouche`, `p_icone_treuil` | 1 each | not read |

- **Items:** (23,281), (265,281), (506,281) and (266,530). The last one is the "done" button [I].
- **Font:** one face at 50 px. The face name was not read.
- **Fade:** 500 ms.

### 3.3 Bourse / price board (states 0x50–0x56) [C, init `FUN_0042008e`, draw `FUN_00420476`]

**Assets:**

| Element | Asset | Frames | Position |
|---|---|---|---|
| Backdrop | `g_boursebg` (800×600) | 1 | (0,0) |
| Column cover | `g_bourse_cache` | 55×178, 2×1 | per object column, see below |
| Leader badge | `g_classe_bourse` | 84×61, 2×1 | (171,484) and (570,484) |

**Layout:**
- 22 object columns in two rows of 11.
- Column i (0..10) and column i+11 share x = 50 + 69·i.
- The baseline is y 208 for the top row and y 392 for the bottom row.
- The object shown in each column is set by a fixed permutation (`0x4672b8`):

  | Columns | Object index |
  |---|---|
  | 0–10 | 3, 4, 20, 8, 17, 2, 1, 15, 7, 11, 5 |
  | 11–21 | 13, 18, 19, 12, 16, 10, 0, 21, 6, 14, 9 |

**Price bars:**
- Each bar is 9 px wide (x .. x+9) and grows upward from the baseline.
- The bar height is the object's current price (`FUN_0040b9d8`, 1 px per price unit), split into bands of 100.
- The band colours go green RGB(29,222,38), then yellow RGB(255,255,0), then red RGB(255,0,0); the next band is black.
- The part below the top band is filled in the previous band's colour up to 100 px.
- **Count-up:**
  - On entry every bar starts at 0.
  - Every 10 ms timer edge, all unfinished bars grow by 1.
  - While any bar is still growing, a ticking sound loops on slot 49 or 50 (0x31/0x32, b_ring), at 21500–21999 Hz.

**Text:**
- **Price label:** white Arial Black 22, "<n>$", at (x − 5, baseline + 5). It is shifted left by 5 for 3-character strings and by 10 for 4 or more.
- **Totals:** white Joint 26, "<n> $".
  - Left: data+0x38 at x 183. Right: data+0x30 at x 585. Both at y 554.
  - Each moves 8 px left for 4 characters and 15 px left for 5 or more.

**Leader badge:** frame 1 on the side with the larger total and frame 2 on the other. Ties give the right side frame 1.

**Column covers:**
- Cover x = 27 + 69·i, y 98 (top row) or 282 (bottom row).
- Columns whose object belongs to the current scene set (mask table `0x4672e8`, matched against the inside/outside/extreme class) get cover frame 2 at full alpha. All others get frame 1 at alpha 150, which dims the objects not in play.
- The in-play column with the highest price blinks, toggling every 300 ms, once its bar has finished.

**Exit:** Enter → gameplay (`game-spec.md` §2.2).

---

## 4. Gameplay screen

### 4.1 Loading screen [C, `FUN_004015c8`, progress `FUN_00405ba4`]

- **Backdrop:** `LOADING` or `LOADING2` (div17/div18), picked 50/50.
- **Animation:** `spriteload` (div28, 195×64 → 65×64, 3 frames) at (674,497).
- **Progress steps:** `FUN_00405ba4(step)` redraws the backdrop each step.
  - The sheet is three frames: empty, half and full.
  - Up to step 4.5: frame 1 is drawn, plus a slice of frame 2 (source x from frameW) that is (frameW / 4.5) × step wide.
  - Past step 4.5: frame 2 is drawn, plus a slice of frame 3 (source x from 2 × frameW) that is (frameW / 4.5) × (step − 4.5) wide.
  - So a 9-step bar fills across two frames.
  - Each step flips the screen, plays a click (volume −1000, 33000 Hz) and busy-waits 200 ms.
  - After loading there is a further 500 ms wait.
- **Timing consequence:** the loading screen always takes about 9 × 200 + 500 ms, about 2.3 s [C for the waits, I for the step count of 9].

### 4.2 Scene and camera [C, layers `FUN_004416b1`, draw `FUN_00441baf`]

- **Layer table:** four layer slots, each with an image, a y position, a parallax factor and an accumulated scroll value. The scene variant is chosen by data+0x2c: 1 = inside, 2 = outside, and extreme mode uses the arena.

  | Layer | Parallax | Inside (1) | Outside (2) | Extreme |
  |---|---|---|---|---|
  | 0 sky | 0.25 | `Paysage`, y 0 | `paysage2`, y 0 | `paysage2`, y 0 |
  | 1 middle | 0.75 inside, 0.5 otherwise | `Jardin`, y 205 | `collines`, y 280 | `collines`, y 280 |
  | 2 main (sets the 4000 px world width) | 1.0 | `Maison`, y 0 | `dehors`, y 230 | `arene`, y 123 |
  | 3 table front | 1.25 | `Table`, y 525 | `table2`, y 525 | `table2`, y 525 |
- **Scroll per frame:**
  - Each layer's accumulator gets round(cameraDelta) added.
  - The layer's source x = round(accumulator × factor).
  - Factors are 0.25, 0.5 or 0.75, 1.0 and 1.25.
- **Wrap:**
  - When the source x passes the layer width it wraps (the accumulator is re-derived), and negative values wrap from the other end. Every layer repeats horizontally.
  - Drawing is two BltFast calls: [srcX, w) placed at screen x 0, then [0, …) to fill the rest of 800.
- **Height clip:** a layer whose y + h exceeds 600 is clipped to 600 − y.
- **Colour key:** layer 0 (sky) is opaque. Layers 1–3 use the magenta key.
- **Draw passes:** each frame draws layers 0–2 first, the gameplay objects next, and layer 3 (the table front, factor 1.25) last.
- **Layer 1 start offset:** 50 px.
- **Camera delta:** it comes from the game object (game+0x407c0, a double). How it follows the fly is in the fly/camera RE (another task), not here.

### 4.3 Per-frame draw order [C for the order, I for the role of the unnamed effects; `FUN_00403e36`]

1. Back scene layers (`FUN_00441baf`).
2. Effect layer at game+0x3fd90: the meal tint. A `color_shade` cell scaled ×40 is drawn at a fixed alpha over the back layers only, when the tint flag is on [C].
   - The cell is frame 3 or 4, chosen by meal.
   - Alpha: inside 51 (breakfast) / 102 (dinner), outside 25 / 63 [C, values from the init].
   - This is how "dinner" looks darker. The tint sits *behind* the front layer and the actors.
3. Dust (game+0x158b0): a mover sprite updated and drawn in the same call.
4. Swatter shadow / back part (`FUN_0043d74a`).
5. Midges (`FUN_00404ef3`).
6. Front scene layers (second `FUN_00441baf` pass).
7. Table objects.
8. Boot (back).
9. Traps (back).
10. Table-object manager (`FUN_0042d2e3`).
11. Effect at game+0x15740 (the fly-effect sheet, a mover sprite updated and drawn in the same call).
12. Fly (`FUN_004388ca`).
13. Boot (front).
14. HUD (`FUN_00404640`: clock `horloge`, goodie panel `croix_goodies`, swatter plate).
15. Flying goodies.
16. Swatter (`FUN_0043db53`).
17. Traps (front).
18. Tutorial bubbles.
19. Period label (`FUN_00405538`).
20. End-of-period banner (game+0x3fe40).

### 4.4 HUD [C, `FUN_00404640`; clock `FUN_0044214e`/`FUN_004421dd`/`FUN_004422c8`]

**Clock:**

| Element | Asset | Position | Behaviour |
|---|---|---|---|
| Clock face | `horloge` (52×55) | (3,3) | static |
| Clock hand | GDI line, 3 px, red RGB(255,0,0) | from face + (25,26), length 15 | see below |

- **Hand motion:** the hand starts at π/2 and, on each new second of game time, moves by (π/2) / (period/4).
  - Period length = minutes × 60 s.
  - The step sign is taken from a per-quarter table [−1, +1, −1, +1].
  - The endpoint is rebuilt per quadrant from cos or sin and a square root, radius 15. A threshold at `0x45e708` was not read.
  - Whether this adds up to one full turn per period is [I].
- **Pause:** the timer is pause-aware (`FUN_004421dd` adds paused time back).

**Fly goodies (shown only while the fly's goodie selector is open):**
- 3 icons, `Goodies` frames 3–5.
- Positioned at (flyX + 12 + 36·i, flyY − 10), so they follow the fly.

**Farmer goodies (shown only while the farmer's selector is open):**
- First `plaque_tap`, centred: x = (800 − w)/2, y 350.
- Then 3 icons, `Goodies` frames 8–10, at (349 + 36·i, 568).

**Icon states:**
- The selected icon is drawn at full size and alpha.
- The other icons are drawn through the alpha path with a scale/alpha factor based on 0.3.
- An icon with a count of 0 gets the `croix_goodies` cross (44×43) at icon + (3,3).
- Otherwise the count is drawn in white Arial Black 22 at icon x + 4, near the icon bottom.

### 4.5 Period start banner [C, `FUN_00405538`]

- **Countdown:** the strings "3", "2", "1", "Go!", one per 1000 ms. The start sound is slot 52.
- **Digit style:** Joint 200.
  - First a 4-way ±3 px outline in yellow RGB(255,255,0), then the fill in blue RGB(5,55,255).
  - Centred: x = (800 − len × size/2) / 2.
  - y = (600 − len × size/0.9) / 2 for the digits. On "Go!" (value < 1) it is (600 − len × size/2) / 2 + 25 instead.
- **Caption:** "<meal> Inside... / Outside... Day n" in Joint 40 italic.
  - Outline grey RGB(130,130,130), fill white.
  - x = (800 − len × size/2) / 2 + 20; y is about 380.
- **When the countdown ends:** music starts (slot 0 or 3, random), the tutorial bubbles start, and the fonts are freed.

### 4.6 Midges, sheets and effects (render-relevant numbers) [C]

| Sheet | Frame cut | Notes |
|---|---|---|
| `moucheron`, `moucheron1` (ig25/26, 162×27) | 27×27, 6×1 | midges; spawn speed ±(200..399) px/s, wander |
| `onde_active` (ig27, 300×150) | 150×150, 2×1 | sheet owned by the fly. Game+0x15740 (frame 1) and +0x158b0 (frame 2) borrow it (`FUN_00405f60`) and tween scale 0.1 → 3.0 with alpha 255 → 0 over 600 ms: an expanding ring [C for the tween, I for which fly sheet] |
| `onde_frappe` (ig28) | 150×150, 1×1 | swatter impact |
| `blood` (ig5) | 14×14, 3×2 | fly death particles |
| `trenblood` (ig50) | 10×11 | |
| `eclatement_fly` (ig18, 374×34) | 34×34, 11×1 | splat |
| `crash-face0011` (ig14) | 339×244 | single frame |
| `eclaires` (ig17, 500×156) | 125×156, 4×1 | zapper lightning |
| `etincelle` (ig19) | 12×12, 3×1 | sparks |
| `vent` (ig56, 20×5) | 5×5, 4×1 | fan wind particle |
| `ailesgoodies` (ig0, 300×44) | 100×44, 3×1 | flying-goodie wings |
| `Goodies` (div14, 255×100) | 51×50, 5×2 | HUD, shop and flying goodies |
| `goodiesmystere` (div15, 102×50) | 51×50, 2×1 [I] | |
| `croix_goodies` (ig15) | 44×43 | |
| `stardust` (ig34) | 20×20, 3×1 | sparkle; 30-particle bursts |
| `piece` (ig31, 675×50) | 45×45 in the bonus round, 45×50 on stats → 15×1 | coin |
| `piecetap` (ig32) | 48×44 | |
| `tutoBulFly`, `tutoBulFarmer` (ig55/54, 468×182) | 234×182, 2×1 | tutorial bubbles |
| `transi-*`, `Face`, `Aspi` (1200×94) | 120×94, 10×1 | fly headings; the menu and stats use frames 5–9 |
| `tapette-*` (ig37–41) | one frame per file; `tapette-8` is 134×187 on stats | swatter swing |
| `DUST` (div10, 198×60) | 66×60, 3×1 | |
| `color_shade` (div8, 80×15) | 20×15, 4×1 | fade and tint cells |
| `spriteload` (div28, 195×64) | 65×64, 3×1 | |

Animation rates not listed default to 50 ms per frame (§1.2). The fly, swatter, trap and object animations (frame ranges per action) belong to the entity RE and are not repeated here.

The `.bor` collision outlines are not render masks. Their structure and use are in `fly-and-world.md` §4.2 and §4.3; the byte layout is in `docs/formats/bor.md`.

---

## 5. End-of-match screens

### 5.1 Winner screen (states 0x78–0x7e) [C, init `FUN_0044aedb`, choreography `FUN_0044b431`, render `FUN_0044bbbf`, skip `FUN_0044bb55`]

**Winner rule:** the fly wins ties; the end-of-match top-up is added to both scores first. Authoritative detail: `ai-and-rules.md` §8.4. The winner sets the trophy side, the caption position and the sounds below.

**Static layout:**

| Element | Asset | Position |
|---|---|---|
| Backdrop | `winnerbck` (800×600) | (0,0) |
| Fly | `winnerfly` | (504,326) |
| Farmer | `winnerfarmer` | (74,235) |
| Trophy | `winnercoupe` | starts at y −50; x 430 if the farmer wins, x 50 if the fly wins (it falls on the loser) |
| Caption | `you_win` 280×70 | (71,148) if the farmer wins, (452,148) if the fly wins; alpha tween 0→255 over 2000 ms |
| Score bars | GDI fill, 10 px wide | farmer x 16 in RGB(255,99,99); fly x 774 in RGB(0,168,255); bottom y 503 |
| Score text | Joint 25, "<n> $" | farmer at (18,51) in red RGB(255,0,0); fly at (690,51) in blue RGB(0,0,255) |

**Choreography:**

| State | What happens |
|---|---|
| 0 | 600 ms fade in, plus a 2000 ms wait |
| 1 | Both counters count up by 10 per tick, the loser's to its score and the winner's to its score. Bar height = counter × 413 / winnerScore, so the winner's bar ends 413 px tall. A tick sound plays at 21500–21999 Hz. → state 2 when the winner's counter is full. |
| 2 | The trophy falls (gravity mover). When its bottom passes the loser sprite, the loser sprite is hidden (crushed). When it lands (bottom at y 587), it snaps to y 587 − h. Then: a dust burst of 30 puffs at (trophyX − 20, 310) spread 480×100; slot 54 (0x36, w_glingcup) at volume 2000; then slot 53 (0x35, b_ihatap) if the farmer won or slot 51 (0x33, b_iha) if the fly won; the caption tween starts; Enter/Esc becomes active after 1500 ms. → state 3 |
| 3 | The trophy hops around at random inside a small box over the floor (wander mode). It lands and rests 1000–5999 ms; each landing gives a smaller dust burst (alpha 140) and a thud sound. During a hop, a random cheer plays (slots 55 or 56, 0x37/0x38, w_vibrofly, when the farmer won; slots 57 or 58, 0x39/0x3a, w_vibrofarmer, when the fly won; 20500–22499 Hz). |
| 4 | Enter/Esc pressed: 600 ms fade out → stats |

### 5.2 Statistics (states 0xaa–0xb0) [C, init `FUN_0044c17a`, update `FUN_0044c73d`, render `FUN_0044cab3`]

**Common layout:**

| Element | Asset / text | Position |
|---|---|---|
| Backdrop | `highscores` | (0,0) |
| Title panel | fill RGB(124,150,228) | rect (203,15)–(586,115) |
| Left panel | fill RGB(124,150,228) | rect (11,119)–(372,590) |
| Title | "Stats", Joint 70, fill RGB(222,19,0), white outline 3 px | (320,30) |
| Fly icon | `transi-est` frame 5 (120×94) | (100,160) |
| Swatter icon | `tapette-8` (134×187) | (100,340) |

**Stat rows:**
- White Arial Black 22, 23 px apart.
- Label x 280, value x 530, written as ": <value>".
- **Fly block:** starts at y 153. Rows: Objects saved (delivered), Average objects saved (delivered ÷ available × 100, shown with " %"), Average price of saved objects (price total ÷ delivered, a whole number), Maximum life time (s), Average life time (s).
- **Farmer block (2P only):** starts at y 363. Rows: Flies killed, Average flies killed (per period, 2 decimals), Swatter hits, Average damage, Ratio (hit ratio × 100, shown with " %").

**1P highscore mode** (1P and mode 2) replaces the farmer block with a score breakdown:
- **Frame:** a white frame of 4 nested rectangles at (185,315), 450×210.
- **Heading:** "Your Score", Joint 38, red RGB(222,19,0) with a white outline, at (317,330).
- **Equation row** (Joint 27 white, y 410): `points` at x 240, a coin (`piece` frame 15) at (313,398), "-", the kill count at x 448, and a dead-fly icon (`g_flymort` frame 2, scale 0.8) at (501,394). The coin and the dead fly each have a white frame box with 8×5 px padding.
- **Result:** "= <n> Pts", Joint 38, fill RGB(89,110,171), white outline, at (318,470).
- **Count-up:**
  - The points count up by 10 per tick.
  - After 1500 ms, kills count up one per 30 ms, each one taking 10 off the result (score − 10 × kills).
  - Each step plays a tick at 18500–18999 Hz (kills) or 21500–21999 Hz (points).
- **Keys:** the first Enter/Esc jumps to the final numbers; the second fades out (600 ms, armed after 500 ms).

**Fade:** 600 ms fade in; the fade out runs once the 3000 ms timer has passed and a key is pressed.

### 5.3 High scores (states 0x6e–0x74) [C, init `FUN_00419904`, render `FUN_00419e2f`, input `FUN_00419cd2`, name entry `FUN_0041a7ac`/`FUN_0041a8d9`, particles `FUN_0041a492`/`FUN_0041a5f3`]

**Layout:**
- **Backdrop:** `highscores` at (0,0). Also `winnercoupe` with its bottom at y 585 and x 415.
  - The loader `FUN_004186e2` takes a *bottom* y and sets top = y − h, so "600" puts an 800×600 image at 0.
- **Table:** 10 rows, row i (0 = best) at y 163 + 40·i, in Arial Black 25.
  - Columns: rank "N." at x 50, name at x 100, score "N  Pts" at x 250.
  - Each row is first drawn in black at (−1,+1) as a shadow, then in colour.

  | Rank | Colour |
  |---|---|
  | 1 | gold RGB(255,208,24) |
  | 2 | grey RGB(162,162,162) |
  | 3 | bronze RGB(221,112,46) |
  | 4–10 | white |

**Reveal:**
- Rows appear bottom-up, from rank 10 to rank 1. Only rows at or below the reveal counter are drawn.
- For each row, a highlight bar sweeps: `highscores_color` 20×20 scaled ×15 to 300×20, at (50, rowY), with alpha 255→0 in 200 ms.
- When the bar ends, the counter moves up one row and a burst of up to 30 `stardust` particles (20×20, 3 frames, 20–79 ms per frame) is thrown over the row area.
  - Each particle has velocity ±(30..79) px/s on both axes and fades out in 400–499 ms.
- 90 particle slots in total. A music cue (slot 36, 0x24, j_super_fly) plays during the reveal.

**Name entry** (from stats when the score qualifies):
- The new row shows the typed name with a "_" cursor that blinks every 200 ms.
- Confirming saves to `scoot.btt` (`game-spec.md` §7.1; the stored value is `ai-and-rules.md` §8.5) and plays slot 51 (0x33).
- A key during the reveal skips it (slot 38, 0x26).
- Enter / Esc → menu.

### 5.4 Farewell (states 0x82–0x88) [C partly, `FUN_00449de0`, `FUN_00449fae`]

- Two `color_shade` layers (20×15 cells, 4×1) at (0,0), plus the `logo` (div19, 700×112) and midges (`moucheron`, 27×27) in wander mode (params 100,10,50,50,30).
- The exact timing was not read (open list in `game-spec.md`, item "Farewell timing").

---

## 6. Bonus round screen

Not read in this pass beyond the asset cuts. Known from the cuts:
- `piece` 45×45
- `piecetap` 48×44
- `ailesgoodies` 100×44
- `stardust` 20×20 ×3
- a `color_shade` fade
- `bckbonusfond` (800×440) and `bckbonusbarre` (800×317) backdrops
- `boitedlg` for the tally (`FUN_00446030`)

Layout is open.

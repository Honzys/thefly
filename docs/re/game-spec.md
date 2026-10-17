# The Fly 1.2: behaviour spec

This spec describes how `The Fly Release 1.2.exe` behaves, as reverse-engineered from a local Ghidra dump. It is written for a clean-room reimplementation. It holds no decompiled code and no original assets. Byte layouts of the data formats are in `docs/formats/` (Task A); this document covers behaviour only.

This file is the **index**. It holds the flow (screens, state machine, menus, input), the audio slot map, the save files, the asset map and the one open-questions list. Every other rule lives in exactly one of the detail documents below, and the sections here say where.

| Topic | Authoritative document |
|---|---|
| RNG, tick order, the fly, camera and layers, outlines and collision, table objects, lift, trap effects on the fly, flying goodies, friendly boot, midges (behaviour) | `fly-and-world.md` |
| Farmer and swatter, AI farmer, AI shopping, trap placement, prices and shop, scoring, balance meter, bonus round, winner, high score, statistics counters, period and match progression | `ai-and-rules.md` |
| Per-screen layout, fonts, colours, the sprite and blit engine, scene layers and draw order, sheet cuts | `screens-and-rendering.md` |
| State machine, menus, input, clock, durations, save files, audio slot map, asset map, open list | this file |

## Conventions

- **[C] confirmed:** read in the decompiled code (or the disassembly for float constants), or checked against a shipped file.
- **[I] inferred:** deduced from names, strings, call structure or asset sizes, but the deciding code was not read line by line.
- `FUN_xxxxxxxx` and `DAT_xxxxxxxx` are addresses in the local dump (see `~/projects/thefly/re/README.md` for how to regenerate it). Each claim cites at least one of them so a reviewer with the dump can check it.
- `game+0x…` is a field of the in-game object (`FUN_00401000`, 0x408f0 bytes). `data+0x…` is a field of the global game-data manager (`DAT_0046c5c0`, built by `FUN_0040ac00`). `app+0x…` is a field of the application object (`FUN_00406382`).
- **Units.** `tick` = one 65 Hz update. `px/s` = pixels per second, scaled by dt. `px/s per tick` = a velocity change applied once per tick. `ms` = milliseconds of wall-clock time.

## 1. What the game is

The Fly is an asymmetric game for two players on one keyboard, or for one player against an AI [C, menu code `FUN_0041c327`]. One player is a house fly. It steals food from a table and carries each item to a lift (the winch) that takes it away for money. The other player is the farmer, shown only as a fly swatter. The farmer tries to swat the fly before the clock runs out [C, `FUN_0042c888`, `FUN_0043d1a0`, `FUN_0044214e`].

A match is a number of days. Each day has three meals: breakfast, lunch and dinner. Between meals, both players spend their earnings on power-ups ("goodies") and the farmer buys traps. A stock-exchange page shows how the price of each food item has moved [C, `FUN_0040b7b2`, `FUN_00422223`, `FUN_00420476`]. After dinner there is a coin-collecting bonus round. At the end of the match come a winner screen, a statistics page and, in high-score mode, the high-score table [C, state machine in section 2].

## 2. Screens and flow

### 2.1 Process start [C]

- The entry point (`FUN_00406210`) creates the application window, with title "The Fly" [C].
- Init (`FUN_00406653`) runs these steps in order:
  1. Builds the game-data manager (`FUN_0040ac00`).
  2. Checks that every required data file exists (`FUN_004101f9`). If one is missing, it shows a "File Error" box and quits.
  3. Creates `data\temp\` (`FUN_0040ff1b`).
  4. Starts DirectX (`FUN_00406803`).
  5. Registers the two TTF fonts from `data\rsc\`.
- At exit, `FUN_0040ff39` deletes the temporary BMP copies of all four image tables [C].
- Each `.irc` file is a 16-byte key header followed by a plain BMP. On first use the loader copies the BMP part to `data\temp\<name>.bmp` (`FUN_004100c5`) and loads that file. See `docs/formats/irc.md` [C].

### 2.2 State machine

The main loop (`FUN_004073d1`) runs one step of the current state per 65 Hz tick, through the dispatcher `FUN_004074e5` on `app+0xf8` [C]. Each screen has three state codes: init, run and cleanup. In the table, "→" means the next state.

| Code (hex) | Screen | Function | Behaviour and exits |
|---|---|---|---|
| 0, 3 | boot | `FUN_00407a12`, `FUN_00407ae6` | Creates the sound manager (`FUN_0040c270`) → 0x8c [C] |
| 8c, 8f, 92 | intro video | `FUN_00408321`, `FUN_00408501`, `FUN_004085f1` | Plays `Intro.avi` through DirectShow. Esc or Enter skips it (`FUN_0040f15d`). When it ends → 0x0a [C] |
| 0a, 0d, 10, 13 | publisher logo | `FUN_00407b64`, `FUN_00407d60` | Shows `Present` with a colour-shade fade and the label "Release 1.2" at (700, 7). A key skips it after 3000 ms; it fades out on its own after 10000 ms → 0x14 [C] |
| 14, 17, 1a, 1d | main menu | `FUN_00407f42`, `FUN_00408068`, `FUN_00408113` | Plays a scripted intro the first time only (a 4-step scroll, a fly pass and a pig drop; the 900 ms wait only guards skipping it; `FUN_0041e1be`; `screens-and-rendering.md` §2.2). Menu result 0 → 0x3c (quit dialog), 1 → 0x5a (keys), 2 → start a match (section 2.4), 3 → 0x6e (high scores) [C] |
| 1e, 21, 24, 27 | goodies shop | `FUN_00408650`, `FUN_00408799`, `FUN_00408918` | Each side buys goodies with its own keys; the side shopping is set by data+0x22fc (`FUN_0040aa80`). Result 1 → 0x28 (traps), 2 → 0x50 (bourse), 0 → quit dialog [C] |
| 28, 2b, 2e, 31 | traps shop (farmer) | `FUN_00408984`, `FUN_00408acd` | The farmer places traps. Result 1 → 0x50 [C] |
| 50, 53, 56 | bourse (price board) | `FUN_00409757`, `FUN_00420476` | Shows the current object prices. Enter → 0x32 [C] |
| 32, 38, 3b | gameplay period | `FUN_00408c2a`, `FUN_00408d5c` | On entry, advances the period (`FUN_0040b7b2`). If the match is over → 0x14; otherwise builds the game object and loads assets behind a random `LOADING` or `LOADING2` screen that always takes about 2.3 s (`FUN_004015c8`; `screens-and-rendering.md` §4.1). Every period then opens with a "3, 2, 1, Go!" banner and a "<meal> Inside/Outside Day n" caption, and the music starts after "Go!" (`FUN_00405538`; `screens-and-rendering.md` §4.5). When the period ends: if dinner or extreme mode (`FUN_0040b922`) → 0x64 (bonus), else → 0x1e. Esc pauses and opens the quit dialog, unless game+0x407bc blocks it [C] |
| 64, 67, 6a | bonus round | `FUN_00408f6a`, `FUN_0040906f` | `ai-and-rules.md` §8.3. Afterwards: match over (`FUN_0040b94c`) → 0x78, else → 0x1e [C] |
| 3c, 3f, 42 | quit / stop dialog | `FUN_0040922a`, `FUN_00409495` | Uses the `boitedlg` frame. From the menu it asks whether to quit to Windows, otherwise whether to stop the game. The default button is No and the screen behind keeps animating. Yes from the menu → 0x82; Yes from a game → 0x14; No → back to the previous state [C] |
| 5a, 5d, 60 | keys page | `FUN_00409964` | Shows `touches`; in 1P it covers the left half with a fill and adds a line saying the player is the fly (`screens-and-rendering.md` §2.6). Esc or Enter → 0xa0 if it was reached from match start, else back [C] |
| a0, a3, a6 | tutorial page | `FUN_00409d44` | Shows `page_mongol`. Enter → 0x32 [C] |
| 6e, 71, 74 | high scores | `FUN_00409f45`, `FUN_00419610`, `FUN_00419904` | Opens name entry (`FUN_0041a7ac`) when reached from the stats page; done → 0x14 [C] |
| 78, 7b, 7e | winner screen | `FUN_0040a162`, `FUN_0044b431`, `FUN_0044bbbf` | Animated trophy for the winner (winner rule: `ai-and-rules.md` §8.4; choreography: `screens-and-rendering.md` §5.1). Esc or Enter skips it (`FUN_0044bb55`) → 0xaa [C] |
| aa, ad, b0 | statistics | `FUN_0040a38c`, `FUN_0044c73d`, `FUN_0044cab3` | Match statistics (section 4.13). In 1P highscore mode, if the fly score qualifies (`FUN_0040abb0` ≥ 0; `ai-and-rules.md` §8.5) → 0x6e, else → 0x14 [C] |
| 82, 85, 88 | farewell animation | `FUN_0040a663`, `FUN_00449de0` | Logo with flying midges → 0x46 [C] |
| 46, 49, 4c | exit | `FUN_0040a8f8` | Closes the application [C] |

### 2.3 Menu tree [C, pages built in `FUN_0041c95d`, selections handled in `FUN_0041c327`]

The menu is driven by the arrow keys, Enter and Esc (`FUN_00408113` passes Up/Left/Down/Right, Enter and Esc to `FUN_0041be05`) [C].

- **Top-level page 0** (image items): Play → page 5; Options → page 1; Credits → page 7 (an animation through pages 7, 9 and 8, then back to 0; credits text is built in `FUN_0041f3b6`); Quit → quit dialog.
- **Page 5:** "1 Player" (sets players to 1) → page 2; "2 Players" (players 2) → page 0x0b; "HighScores" → high-score screen.
- **Page 2 (1P):** "Normal Mode" (mode 1) → page 6; "Highscore Mode" (mode 2, AI level 1, 3 days) → start.
- **Page 6 (AI level):** "Easy" (0), "Normal" (1), "Inteligent" (2, spelled this way in the exe) → page 3.
- **Page 0x0b (2P):** "Normal Mode" (mode 1) → page 3; "Extreme Mode" (mode 2) → start.
- **Page 3 (length):** "2 days", "4 days", "6 days" → `FUN_0040b787(days)` → start.
- **Page 1 (Options):** Music On/Off, Tutorial On/Off, ViewKeys. The toggles are saved to `boite.btt` (section 7).
- **Defaults:** 2 days, 1 player, mode 1, AI level 1.
- **Config fields:** data+0x23 = player count, data+0x24 = mode (1 normal; 2 = highscore in 1P, extreme in 2P), data+0x25 = AI level, data+0x22 = days, data+0x28 = extreme flag (set when players = 2 and mode = 2) [C].

### 2.4 Starting a match [C, `FUN_00408113`]

1. Reset the match (`FUN_0040b005`, which also loads the shop price table, `ai-and-rules.md` §7.1).
2. Set up the farmer (`FUN_0040b50c`) and the fly (`FUN_0040b3c7`), plus extreme-mode setup (`FUN_004242d0`, `FUN_0040b994`).
3. If music is on, pick `music_game` or `music_game2` at random (`FUN_00405538`).
4. If the tutorial is off → first gameplay period (0x32). If it is on → keys page → tutorial page → gameplay. The tutorial period lasts 3 minutes instead of 2 (`FUN_00405ed0`).

### 2.5 Game modes

| Mode | Menu path | Who plays what | Structure | Ends with |
|---|---|---|---|---|
| 1P Normal | 1 Player → Normal → AI level → 2/4/6 days | The human is the fly; the AI (`FUN_0043ea40`) plays the farmer, including shopping (`FUN_004414ea`) and trap placement (`FUN_00425ba2`) [C] | days × (breakfast, lunch, dinner) | winner screen → stats → menu [C] |
| 1P Highscore | 1 Player → Highscore Mode | as 1P Normal, AI level fixed at 1 [C] | 3 days [C] | winner → stats → name entry if the fly score qualifies [C] |
| 2P Normal | 2 Players → Normal → 2/4/6 days | the fly on the arrow keys, the farmer on T/F/G/H [C] | days × 3 meals [C] | winner → stats → menu [C] |
| 2P Extreme | 2 Players → Extreme Mode | as 2P; both sides start with large inventories (`ai-and-rules.md` §7.7) and the arena scene [C] | a single period at a random meal and random inside/outside, then the bonus round [C, `FUN_0040b7b2`] | winner → stats [C] |
| Tutorial | Options → Tutorial On, then any mode | preset inventories and money (`ai-and-rules.md` §7.7); hint bubbles overlay the game (`FUN_00443772`, `FUN_00443488`) [C] | 3-minute periods [C] | as the chosen mode [I] |

### 2.6 Period, day and match progression [C, `FUN_0040b7b2`]

Authoritative detail: `ai-and-rules.md` §9 (advance §9.1, period end order §9.2) and §8.6 (the clock).

- data+0x20 holds the period (0 before the first, then 1 = breakfast, 2 = lunch, 4 = dinner) and data+0x21 the day; data+0x2c is 1 = inside or 2 = outside (`FUN_00405538`: 2 shows "Outside", any other value "Inside").
- A period ends when the 2-minute clock (3 in the tutorial) runs out, or when every object on the table has been delivered.
- After a period: dinner (or extreme mode) → bonus round; otherwise shops → bourse → next period. Extreme mode is a single period.
- The period banner reads "<meal> Inside/Outside Day <n>". The meal labels in the exe are "Breakfast", "Lunch" and "Diner" [C].

## 3. Input [C, `FUN_00407241`; DirectInput keyboard, no mouse or joystick]

| Role | Up | Down | Left | Right | Action | Goodie select / use |
|---|---|---|---|---|---|---|
| Fly | Up arrow | Down arrow | Left arrow | Right arrow | Enter (grab or drop an object) | Right Shift cycles the goodie; Enter uses it [C, `FUN_00404198`] |
| Farmer | T | G | F | H | Tab (swing) | the key at DIK scancode 0x10 (Q on QWERTY, A on AZERTY) [C, `FUN_004043c1`] |
| Menus | arrows | | | | Enter to select | Esc to go back or open the quit dialog [C] |

- App fields: Up +0x44, Down +0x48, Right +0x4c, Left +0x50, H +0x54, F +0x58, T +0x5c, G +0x60, Enter +0x64, Esc +0x6c, Tab +0x74, scancode 0x10 at +0x78, Right Shift +0x7c [C].
- `FUN_0040735a` detects a fresh press of Esc or Enter. Screens use it to skip, so that a held key does not skip several screens [C].
- The shops use the keys of the side whose turn it is (`FUN_00408799`) [C].
- The farmer's four direction keys are T up, F left, G down, H right [C, `FUN_00407241` order checked against the parameter order of `FUN_0043cea5`]. The in-game key help text lists D, F, R, G; the port follows the code (open list item 1).

## 4. Game objects

The game object's update is `FUN_0040326c`. Each tick it receives the fly's five keys, the farmer's five keys, the two goodie keys, the frame time and the FPS [C]. Unless a subsection says otherwise, everything below runs once per 65 Hz tick. The fixed order of the sub-updates is in `fly-and-world.md` §2. Each subsection here is an index entry: it names the owner of the rules and keeps only facts no other document holds.

### 4.1 World and camera [C, `FUN_004416b1`, `FUN_00441baf`]

Authoritative detail: `fly-and-world.md` §4.1 (layers, camera, wrap), `screens-and-rendering.md` §4.2 (scene layers, scrolling, draw).

- The world is 4000 px wide (the width of the main scenery layers `Maison`, `dehors` and `arene`, section 9.3) and one screen (600 px) tall.
- The camera scrolls horizontally only. game+0x407c0 is a per-tick delta in fly-layer pixels, assigned each tick; it is not an offset. The fly lives in screen space. Table objects, the lift and the boot live on a 1.25 parallax layer whose wrap width is trunc(W × 1.25) = 5000.
- The farmer and the collision tests wrap around the world width (`FUN_0043cea5`, `FUN_0042f6de`) [C].
- Ambient sound: animal noises (slots 14–21) play at random intervals (`FUN_00441b3c`) [C].

### 4.2 Fly (game+0x40; init `FUN_00436df6`, physics `FUN_00437547`, update `FUN_00433ddd`, render `FUN_004388ca`)

Authoritative detail: `fly-and-world.md` §3 (fields §3.2, init §3.3, physics §3.4, update and carrying §3.5, damage, stun and death §3.6, respawn §3.7, Turbo and SuperFly §3.8).

- States: flying, carrying, stunned (ricochet), dying (crash animation), splatted, and respawning. Health is 100; a running damage total above 100 kills.
- **Assets** [C, `FUN_00435cd4`]: `Face` and the eight `transi-*` strips (heading sprites), `crash-face0001..0004` and `crash-face0011`, `eclatement_fly`, `blood`, `Aspi` (grab effect), `onde_active`, `stardust`, `DUST`.
- **Buzz** (slot 7) is positional (`FUN_0040d005`: pan from x around screen centre 400; frequency 10000 + random 0–999 + a speed term; quieter off-screen) [C].

### 4.3 Farmer / swatter (game+0x13d20; init `FUN_0043c5f2`, move `FUN_0043cea5`, swing `FUN_0043d510`, hit test `FUN_0043d1a0`)

Authoritative detail: `ai-and-rules.md` §1 (constants §1.1, movement §1.2, swing §1.3, hit test and damage §1.4, voices and footsteps §1.5, Triple Hit §1.6, Nuclear Hit §1.7). L = farmer level, data+0x7c (0..5).

- **AI (1P):** `FUN_0043ea40` (at game+0x14e48) drives the same key inputs as a human farmer, with three levels (Easy, Normal, Inteligent) from data+0x25. Creation, parameters, controller, goodie decisions: `ai-and-rules.md` §2; shopping: §3; trap placement: §4.4.
- **Assets** [C, `FUN_0043c061`, `FUN_00448802`]: `tapette-1`, `-3`, `-5`, `-7`, `-8` (swing frames), `onde_frappe` (shockwave), `fleche_tap` (position arrow), `etincelle` (sparks), `plaque_tap` (HUD plate). `piecetap` is used by the bonus-round swatter [I].

### 4.4 Table objects (game+0x3df60 manager `FUN_0042c888`; instances in `DAT_0049bd20`, stride 0x874)

Authoritative detail: `fly-and-world.md` §5 (type table with base prices §5.1, placement §5.2, fields and states §5.3, motion §5.4, manager tick and delivery §5.5).

- Object set: 22 normal objects, plus 3 that appear only in extreme mode. Each has a sprite (`div*.irc`), a `.bor` outline (section 9.6), a weight and a base price [C, `FUN_0040ae51`]. Up to 15 objects are placed per period.
- Price fluctuation, payout and delivery scoring: `ai-and-rules.md` §7.1, §7.3 and §7.5.
- A landing object plays its stored impact sound, slot 6 (`FUN_0042dc21`). The footsteps in slots 22–26 belong to the farmer (section 8.2).

### 4.5 Lift / winch (`DAT_0049bba0`; `FUN_00430e48`, `FUN_00431115`, `FUN_00431931`, `FUN_0042a760`)

Authoritative detail: `fly-and-world.md` §5.6 (anchors, three-state machine, hook windows, hoisting) and §5.5 (delivery).

- The lift is drawn with `treuil` and a `treuil_corde` rope. The winch sound (slot 47) plays while it moves [C]. `FUN_0042a760` is the manager destructor.

### 4.6 Traps (game+0x3ffb0 manager, update `FUN_00427270`, render back `FUN_00427a27`, render front `FUN_00427b54`)

Authoritative detail: `ai-and-rules.md` §4 (types, costs, caps, lifetime, placement validator, AI placement). Effects on the fly: `fly-and-world.md` §7.4. Effects on midges: `fly-and-world.md` §8.2.

- **Assets** [C]: `colle_mouche1`, `colle_mouche2` (flypaper); `ventilo_gauche`, `ventilo_droite`, `vent` (wind particles) with the `palesventi_*` and `piedventi_*` outlines; `tue_mouche`, `eclaires`, `etincelle` (zapper).
- **Trap sounds** [C]: fan loop slot 44 at −400 or −100 (louder when blowing), positional when there is a single fan, and slot 45 on impact at 20000–25999 Hz. Zapper loop slot 40 at −500 idle or −200 while arcing (−450 positional when there is a single zapper), slot 39 on an arc, slot 43 when a fly is grilled. The flypaper has no sound. Placing any trap plays slot 13.

### 4.7 Fly goodies [C, `FUN_00404198`]

The goodie selector (game+0x408a8) cycles through 0..3 with Right Shift; Enter uses the selected one if its count is above 0. Using a goodie plays slot 37/38 (j_goodie) and a stardust effect.

| # | Goodie | Count field | Duration | Sound | Authoritative detail |
|---|---|---|---|---|---|
| 1 | Turbo (`FUN_00436fc1`) | data+0x8a | 1500 ms | 35 | `fly-and-world.md` §3.8 |
| 2 | SuperFly (`FUN_0043728f`) | data+0x8c | 8000 ms | 36 | `fly-and-world.md` §3.8 |
| 3 | Midges (`FUN_00404caf`) | data+0x8e | until they leave | 48 | `fly-and-world.md` §8 |

### 4.8 Farmer goodies [C, `FUN_004043c1`]

The selector is game+0x408aa; the goodie key (scancode 0x10) cycles and uses it. Selector and activation rules: `ai-and-rules.md` §5.

| # | Goodie | Count field | Duration | Authoritative detail |
|---|---|---|---|---|
| 1 | Triple Hit (`FUN_0043c8e2`) | data+0x80 | not timed: 3-swing bursts, ends after more than 6 bursts | `ai-and-rules.md` §1.6 |
| 2 | Nuclear Hit (`FUN_0043cb71`) | data+0x82 | 5000 ms (20000 ms from a mystery goodie) | `ai-and-rules.md` §1.7 |
| 3 | Friendly Boot | data+0x84 | the boot leaves after 10 stomps | `fly-and-world.md` §7 |

### 4.9 Midges (game+0x178a0, 300 slots; spawn `FUN_00404caf`, render `FUN_00404ef3`)

Authoritative detail: `fly-and-world.md` §8 (spawn, homing, kills, fan push); the slowdown they cause: `ai-and-rules.md` §6.3.

- **Assets:** `moucheron`, `moucheron1` [C].

### 4.10 Friendly boot (game+0x3dc60; `FUN_0043aabe`, `FUN_0043abc7`, `FUN_0043b157`, `FUN_0043b370`, render `FUN_0043b41b`)

Authoritative detail: `fly-and-world.md` §7 (spawn, update, effect on the fly). The boot can kill a low fly.

- A giant boot (`i_botte`) stomps down; the active flag is game+0x3df58. The boom (slot 6) plays [C].

### 4.11 Flying goodies (game+0x15000; `FUN_00432684` spawn, `FUN_00432dfb` update, `FUN_0043309b` catch, render `FUN_00432e7b`)

Authoritative detail: `fly-and-world.md` §6 (spawn, update, catch). The spawn chance and the re-arm times use the balance meter, `ai-and-rules.md` §8.2.

- Two flyers can be on screen: one for the fly (sub-object +0x1c8) and one for the farmer (+0x18), drawn with `goodiesmystere`/`goodies` icons and `ailesgoodies` wings [C].
- Flags game+0x40898 and game+0x40899 hold the active mystery state.

### 4.12 Bonus round (`FUN_00444da0`, `FUN_004452a1`, update `FUN_00446030` and `FUN_004472df`, render `FUN_00446993`)

Authoritative detail: `ai-and-rules.md` §8.3 (handicaps, coins, targets, bonus AI, result). Screen: `screens-and-rendering.md` §6. The round lasts 35000 ms.

- **Assets** [C]: backdrop `bckbonusfond` with the bar `bckbonusbarre`; coins `piece`; the bonus swatter uses `FUN_00449203`; the countdown uses the `JOINPD__.TTF` face; music_bonus (`FUN_00446e7d`).

### 4.13 Statistics (`FUN_0040d3e0`, `FUN_0040d4ea`; page `FUN_0044bf60`)

Authoritative detail: `ai-and-rules.md` §8.7 (counters and page values); layout: `screens-and-rendering.md` §5.2.

## 5. Timing and scoring constants

### 5.1 Clock and tick [C, `FUN_004193df`, `FUN_00419506`, `FUN_004194a9`]

- The timer class runs on QueryPerformanceCounter when available: a period of frequency / rate, with elapsed time in seconds. Otherwise it uses GetTickCount: a period of `rate` ms, scaled by 0.001.
- The frame timer is created with rate 65, so the game updates at **65 Hz**. Each tick it does one update, one render and one flip. The measured elapsed time in seconds (dt, timer +0x20 via `FUN_00419595`) scales velocities.
- One-shot timers are armed in ms (`FUN_004194a9`) and polled per tick.
- The display requests 60 Hz, falling back to the default refresh rate (`FUN_00406803`). An FPS counter refreshes every 1000 ms (`FUN_00407195`).

### 5.2 Durations [C]

| Thing | Value |
|---|---|
| gameplay period | 2 minutes (data+0x26); 3 minutes in the tutorial |
| logo skip / auto-advance | 3000 / 10000 ms |
| first menu intro skip guard | 900 ms |
| Turbo | 1500 ms; a mystery Turbo is armed for 25000 ms but is cut when the 15000 ms mystery timer ends (`fly-and-world.md` §6.3) |
| SuperFly | 8000 ms |
| Nuclear | 5000 ms (20000 ms from a mystery goodie) |
| Triple Hit | not timed (`ai-and-rules.md` §1.6) |
| mystery effect timer | 15000 ms |
| fly stun | 4 ticks hit-in, the kick, 4 ticks back out; the ricochet decelerates over 23 ticks (`fly-and-world.md` §3.6) |
| fly death to respawn | 1500 ms after the crash animation |
| fan re-hit guard | 100 ms |
| flying goodie lifetime | 8000 ms (fly), 12000 ms (farmer) |
| bonus countdown step | 1000 ms |
| bonus round | 35000 ms |

### 5.3 Scoring [C]

Authoritative detail: `ai-and-rules.md` §8.1 (points per event), §7.3 (payout), §8.4 (winner), §8.5 (high score).

- Fly score (data+0x30) and money (data+0x92) rise at the shop payout by the saved goods at the old price; the farmer's (data+0x38, data+0x90) rise by 42 per kill. The payout pays the old price and then fluctuates it, once per type present that period.
- The bonus round adds 2 per coin to the fly and 10 per target to the farmer.
- The fly wins ties: the farmer wins only if fly score < farmer score after the end-of-match top-up.
- The high-score table stores the fly score minus 10 × kills, and ties rank above older entries.

### 5.4 Difficulty knobs [C]

- Fly speed and strength levels 0–5, and farmer level 0–5, are bought in the shop.
- AI level 0–2 sets the AI farmer's parameters (`ai-and-rules.md` §2.2).
- Period length 2 minutes; match length 2, 4 or 6 days (3 in highscore mode).

### 5.5 Balance meter [C, `FUN_0040c086`]

Authoritative detail: `ai-and-rules.md` §8.2. Both scores are included; the meter is exactly 0.5 at a tie and the trailing side gets more than 0.5. It drives the flying-goodie spawns (`fly-and-world.md` §6) and the bonus-round handicaps (`ai-and-rules.md` §8.3).

### 5.6 Shop prices [C]

Authoritative detail: `ai-and-rules.md` §7.1 (price table with caps and increments), §7.2 (purchase rules, the farmer level special case and the paid sequences).

### 5.7 Starting inventories [C]

Authoritative detail: `ai-and-rules.md` §7.7.

## 6. Rendering

Authoritative detail: `screens-and-rendering.md` (engine §1, per-screen layout §2–§6).

### 6.1 Display [C, `FUN_00406803`, `FUN_004070ef`]

- DirectDraw 7, full-screen 800 × 600 at 16-bit colour. A primary surface with one back buffer, flipped each tick. On a lost surface it restores.
- Direct3D 7 draws every alpha-blended or scaled image (fades, dust, tints, caption tweens, HUD dimming, high-score highlights); opaque and colour-keyed images use plain blits (`screens-and-rendering.md` §1.1).
- Text is drawn with GDI TextOut in `JOINPD__.TTF` and in system faces; which screen uses which is in `screens-and-rendering.md` (Conventions, Fonts).

### 6.2 In-game scene layers [C, `FUN_004416b1`]

Authoritative detail: `screens-and-rendering.md` §4.2 (the layer table: assets, y, parallax; scroll and wrap).

### 6.3 Draw order per frame [C, `FUN_00403e36`]

Authoritative detail: `screens-and-rendering.md` §4.3 (the 20-step order, the meal tint).

### 6.4 Sprite sheets and outlines

- **Sprite sheets:** authoritative detail `screens-and-rendering.md` §1.2 and §4.6. Sheets are cut into a column × row grid and frames run in row-major order; frame delays are wall-clock (default 50 ms). The frame count is passed in code at load time.
- **`.bor` outlines:** authoritative detail `fly-and-world.md` §4.2 and §4.3; byte layout `docs/formats/bor.md`. They are per-scanline collision outlines, not render masks.

## 7. Save files

Both files are opened relative to the working directory: read with "rb" (`FUN_0040ab80`), written by `FUN_0040fe40`. See `docs/formats/btt.md` for byte layouts [C].

### 7.1 `scoot.btt`: high-score table [C, `FUN_00410a69`, `FUN_00410ad3`, `FUN_00410c24`, `FUN_00410cbd`, `FUN_00410f04`]

- **Layout:** ten 48-byte records, then an 8-byte trailer.
  - Each record holds a name (12 bytes, NUL padded) at +0, the score (signed 32-bit) at +0x0c, and a checksum (64-bit float) at +0x28.
  - Name entry (`FUN_0041a7ac`) also fills +0x10..+0x21 with the run's statistics: objects saved (int16 at +0x10), maximum life time (float32 s at +0x14), average life time (float32 s at +0x18), average price (int32 at +0x1c) and flies killed (int16 at +0x20) [C]. Bytes +0x22..+0x27 are not written deliberately, and the shipped default rows hold stack garbage in all of +0x10..+0x27. Validation reads only score and checksum, and the table screen shows only rank, name and score, so nothing in this exe reads the statistics back [I].
  - The score field holds data+0x34, the fly score minus 10 × kills (`ai-and-rules.md` §8.5).
  - The trailer is the checksum of the sum of all scores.
- **Checksum c(s):** x = 5.365799563 × s − sqrt(5.365799563); if x < 0, x = 0.1835; c = sqrt(x).
  - Empty slots get a random filler checksum (random 0–7999 / 5.63).
  - If the score sum is below 1, the trailer gets random 0–7999 / 6.8899.
- **Validation** (`FUN_00410ad3`): every non-zero score must match its checksum, and the trailer must match.
- **Insert** (`FUN_00410cbd`): finds the first slot whose score is lower than or equal to the new one, so a tie ranks above the older entry; shifts the rest down, then rewrites every checksum. If no slot qualifies, nothing is stored. `FUN_00410f04` sets the name. Qualification and the stored value: `ai-and-rules.md` §8.5.
- **Shipped file:** record 0 is "None" with score 1 and checksum 1.74625 = c(1); the trailer is equal to it [C, checked against the shipped file].

### 7.2 `boite.btt`: options [C, `FUN_00411061`, `FUN_004110b3`, `FUN_00411105`]

- Two unsigned 32-bit values: [0] music on (1) / off (0), [1] tutorial on / off.
- Written when the Options toggles change (`FUN_0041c327`).

## 8. Audio

### 8.1 Engine [C, `FUN_0040c270`]

- **Sound effects:** 59 DirectSound WAV slots, indexed 0–58. Wrappers: play / loop `FUN_0040ce7b`, volume `FUN_0040cea9`, frequency `FUN_0040ced5`, pan `FUN_0040cf01`, stop `FUN_0040cf2d`, is-playing `FUN_0040cf55`, positional `FUN_0040d1b2`.
- **Preloading:** each screen preloads its group by name prefix:
  - menu `FUN_0040c833`
  - goodies shop `FUN_0040c99c`
  - bourse `FUN_0040c9dc`
  - traps shop `FUN_0040ca12`
  - game `FUN_0040ca52`
  - bonus `FUN_0040cb02`
  - winner `FUN_0040cbf1`
  - stats `FUN_0040cd8b`
  - silence `FUN_0040c6eb`
- **Streamed music** (`FUN_0040cf7f`): `music_game`, `music_bonus`, `music_goodies`, `music_game2`.
- **MIDI** through DirectMusic: `i_music.mid` (menu) and `w_music_winner.mid` (winner screen).
- **Music option:** the boite.btt flag gates in-game music (`FUN_00405538`) [C]. Whether it also mutes the MIDI is [I].
- **Button click:** `i_bouton.WAV` is a separate DirectSound buffer (`FUN_0040ddbb`) used by the menu, shops and game init [C].

### 8.2 Slot → event map

Slots are decimal here. Other documents and the code show some of them in hex, for example slot 54 is 0x36.

| Slot | File | Event (function) |
|---|---|---|
| 0 | i_melody | menu intro / ambience (`FUN_0041d3cc`, `FUN_0041e598`) [C call, I meaning] |
| 1 | i_Cowboy | menu animation (`FUN_0041e598`) [C call, I meaning] |
| 2 | i_pig | menu pig animation (`FUN_0041e1be`, `FUN_0041e63c`) [C] |
| 3 | i_ha | menu selection (`FUN_0041be05`) [C] |
| 4 | i_fucking_pig | menu pig animation (`FUN_0041e598`) [C] |
| 5 | i_kickpig | menu pig kicked (`FUN_0041e63c`) [C] |
| 6 | z_boom | menu (`FUN_0041d3cc`, `FUN_0041eb60`); table-object landing impact (`FUN_0042dc21`, via the stored impact slot); friendly-boot stomp (`FUN_0043b370`) [C] |
| 7 | z_Bzz | fly buzz, positional (`FUN_0040d005`); menu (`FUN_0041d3cc`) [C] |
| 8 | g_blatte | goodies shop cockroach (`FUN_0042123c`, `FUN_00423be2`, `FUN_00423f2e`) [C] |
| 9 | g_tapis | goodies shop conveyor (`FUN_0042123c`, `FUN_00421e6a`, `FUN_0042322f`) [C] |
| 10 | g_machine | goodies shop machine (`FUN_0042123c`, `FUN_0042322f`) [C] |
| 11 | g_money | purchase; object delivered (`FUN_0042c888`); stats tally (`FUN_0044c73d`) [C] |
| 12 | gp_error | purchase refused (shops) [C] |
| 13 | p_build | trap placed (`FUN_00425456`) [C] |
| 14–21 | j_birds1, j_Birds2, j_sheep, j_dog1, j_dog2, j_cow, j_rooster, j_chicken | random ambience (`FUN_00441b3c`) [C] |
| 22–26 | j_pas1..5 | farmer footsteps (`FUN_0043c730`; `ai-and-rules.md` §1.5) [C] |
| 27 | j_frappe_tap | swatter swing (`FUN_0043d510`, `FUN_00449203`) [C] |
| 28 | j_ricoche_fly | fly hit, ricochet (`FUN_00437e99`) [C] |
| 29 | j_crash_fly | fly killed (`FUN_00437e99`); the call sets its volume to +5000, which is out of range, so it is a volume and not a frequency (`fly-and-world.md` §3.6, §9) [C] |
| 30–33 | j_fvoix_gagne, _hate, _rate, _perdu | farmer voice lines (`FUN_0043c730`; 33 also `FUN_0044a9a8`): 30 when the fly dies, 31 on a random timer, 32 after a long miss streak, 33 when objects are left on the table (`ai-and-rules.md` §1.5) [C] |
| 34 | j_fvoixhaaa | swing shout, 20% chance (`FUN_0043d510`, `FUN_00449203`) [C] |
| 35 | j_turbo | Turbo used (`FUN_00436fc1`) [C] |
| 36 | j_super_fly | SuperFly used; high-score screen (`FUN_00419904`) [C] |
| 37, 38 | j_goodie | goodie used; fly respawn; high scores [C] |
| 39 | jpt_eclair | zapper arc (`FUN_00428b47`) [C] |
| 40 | jpt_tuemouc | zapper hum (`FUN_00427270`) [C] |
| 41 | j_aspi | fly grabs an object (`FUN_00433ddd`) [C] |
| 42 | j_goovolant | flying goodie spawned (positional, `FUN_00432684`) and caught (`FUN_0043309b`); bonus target swatted (`FUN_004472df`) [C] |
| 43 | jpt_grillemouc | fly grilled by the zapper (`FUN_004288fe`) [C] |
| 44 | jpv_ventilo | fan running (`FUN_00427270`) [C] |
| 45 | jpv_chocventilo | fly hits a fan (`FUN_0042a075`, `FUN_0042754d`) [C] |
| 46 | j_splash | fly splatter (`FUN_004397c0`, `FUN_0044a641`) [C] |
| 47 | j_treuil | winch / lift moving (`FUN_00431931`, `FUN_0042a760`) [C] |
| 48 | j_moucherons | Midges goodie (`FUN_00404caf`) [C] |
| 49, 50 | b_ring | alternating tick sounds: bourse bar growth (`FUN_00420476`), winner count-up (`FUN_0044b431`), bonus coin collected (`FUN_004472df`); preloaded for bourse, winner and stats [C] |
| 51 | b_iha | bonus result, fly ahead; every 50th bonus coin; winner screen when the fly won; high scores [C] |
| 52 | b_321 | bonus countdown; game start (`FUN_004015c8`) [C] |
| 53 | b_ihatap | bonus result, farmer ahead; every 10th bonus hit; winner screen when the farmer won [C] |
| 54 | w_glingcup | winner trophy (`FUN_0044b431`) [C] |
| 55–58 | w_vibrofly1/2, w_vibrofarmer1/2 | winner screen cheers (`FUN_0044b431`): 55/56 when the farmer won, 57/58 when the fly won [C] |

## 9. Asset map

There are 235 files under `data/`: 138 in `rsc`, 29 in `bor` and 68 in `sons`. The exe refers to each image by name. The `.irc` file index is the name's position in the exe's name table, confirmed by matching each file's BMP dimensions against what the code expects (`~/projects/thefly/re/scripts/irc_dims.py`). Names below are the logical names without the table prefix and `.bmp` extension; the in-exe names carry a prefix (`i_`, `j_`, `g_`, `p_`, or none).

### 9.1 `rsc/ac*.irc`: menu and front-end (table `0x45df10`, 16 entries) [C]

| File | Name | Size | Used by |
|---|---|---|---|
| ac0 | highscores | 800×600 | high-score screen (`FUN_00419610`) |
| ac1 | highscores_color | 20×20 | high-score screen, colour swatch |
| ac2 | i_botte | 270×575 | friendly boot (`FUN_0043a958`) and menu |
| ac3 | i_cochon_corps | 550×345 | menu pig body (`FUN_0041ae17`) |
| ac4 | i_cochon_tete | 500×345 | menu pig head |
| ac5 | i_credit_gaga | 600×113 | credits page |
| ac6 | i_credit_seb | 600×111 | credits page |
| ac7 | i_decors1 | 800×1200 | menu backdrop |
| ac8 | i_decors2 | 569×182 | menu backdrop part |
| ac9 | i_items1 | 300×400 | menu item buttons |
| ac10 | i_nuage | 951×82, 24-bit | menu clouds |
| ac11 | i_PetiteMouche | 144×28 | menu cursor fly |
| ac12 | i_soleil | 2548×678 | menu sun animation strip |
| ac13 | Present | 800×600, 16-bit | publisher logo screen (`FUN_00407b64`) |
| ac14 | touches | 800×600 | keys page (`FUN_00409964`) |
| ac15 | page_mongol | 800×600 | tutorial page (`FUN_00409d44`) |

### 9.2 `rsc/div*.irc`: objects, dialogs, loading, winner (table `0x45df50`, 39 entries) [C]

| File | Name | Size | Used by |
|---|---|---|---|
| div0 | boitedlg | 510×400 | quit dialog (`FUN_0040922a`), bonus tally (`FUN_00446030`) |
| div1 | cacahuete | | table object |
| div2 | can1 | 152×217 | table object; goodies shop decoration (`FUN_0042123c`) |
| div3 | can2 | | table object |
| div4 | can3 | | table object |
| div5 | chips1 | | table object |
| div6 | chips2 | | table object |
| div7 | clope | | table object |
| div8 | color_shade | 80×15, 4-bit | logo fade, loading bar (`FUN_00407b64`, `FUN_004015c8`) |
| div9 | cornichon | | table object |
| div10 | DUST | 198×60 | dust puffs (menu, shop, fly) |
| div11 | extreme1 | 275×275 | extreme-mode table object |
| div12 | extreme2 | 213×213 | extreme-mode table object |
| div13 | extreme3 | 149×149 | extreme-mode table object |
| div14 | goodies | 255×100 | goodie icons (HUD, shop, flying goodie) |
| div15 | goodiesmystere | 102×50 | mystery flying goodie (`FUN_004321f8`) |
| div16 | lait | 209×300 | table object |
| div17 | LOADING | 800×600, 24-bit | loading screen (`FUN_004015c8`) |
| div18 | LOADING2 | 800×600, 24-bit | loading screen, second variant |
| div19 | logo | 700×112 | farewell animation (`FUN_00449de0`) |
| div20 | mais | 430×130 | table object |
| div21 | mandarine | | table object |
| div22 | muffin | | table object |
| div23 | poivriere | | table object |
| div24 | pomme | | table object |
| div25 | poulet | | table object |
| div26 | saucisson | | table object |
| div27 | saucisson1 | | table object |
| div28 | spriteload | 195×64 | loading animation (`FUN_004015c8`) |
| div29 | sucre | | table object |
| div30 | toast | | table object |
| div31 | toaster | 468×270 | table object |
| div32 | tomate | | table object |
| div33 | winnerbck | 800×600, 24-bit | winner screen backdrop (`FUN_0044aedb`) |
| div34 | winnercoupe | | winner trophy |
| div35 | winnerfarmer | | winner: farmer |
| div36 | winnerfly | | winner: fly |
| div37 | you_win | 280×70 | winner caption |
| div38 | zippo | 140×160 | table object |

Blank sizes were not needed for the spec; `irc_dims.py` prints them. All table objects are loaded through the object table in `FUN_0040ae51` and the manager in `FUN_0042c888`.

### 9.3 `rsc/ig*.irc`: in-game sprites (table `0x45dff0`, 59 entries, all named `j_*` in the exe) [C unless marked]

| File | Name | Size | Used by |
|---|---|---|---|
| ig0 | ailesgoodies | 300×44 | flying-goodie wings (`FUN_004321f8`) |
| ig1 | arene | 4000×402 | extreme scene, main layer (`FUN_004416b1`) |
| ig2 | Aspi | 1200×94 | fly grab effect (`FUN_00435cd4`) |
| ig3 | bckbonusbarre | 800×317 | bonus round bar (`FUN_004452a1`) |
| ig4 | bckbonusfond | 800×440 | bonus round backdrop |
| ig5 | blood | | fly death effect (`FUN_00435cd4`) |
| ig6 | charbon | | no code reference found |
| ig7 | colle_mouche1 | 53×300 | flypaper |
| ig8 | colle_mouche2 | 53×300 | flypaper, second variant |
| ig9 | collines | 2000×258 | outside and extreme scenes, middle layer |
| ig10–ig13 | crash-face0001..0004 | | fly crash frames |
| ig14 | crash-face0011 | | fly crash frame |
| ig15 | croix_goodies | | HUD goodie selector (`FUN_004015c8`) |
| ig16 | dehors | 4000×295 | outside scene, main layer |
| ig17 | eclaires | 500×156 | zapper lightning (`FUN_0042699d`) |
| ig18 | eclatement_fly | 374×34 | fly splatter |
| ig19 | etincelle | | sparks: zapper (`FUN_00428540`), Nuclear swatter |
| ig20 | Face | 1200×94 | fly, facing camera |
| ig21 | fleche_tap | | swatter position arrow |
| ig22 | horloge | 52×55 | period clock (`FUN_004015c8`) |
| ig23 | Jardin | 800×217 | inside scene, middle layer |
| ig24 | Maison | 4000×553 | inside scene, main layer |
| ig25 | moucheron | 162×27 | midges; farewell animation |
| ig26 | moucheron1 | 162×27 | midges, second variant |
| ig27 | onde_active | 300×150 | fly effect wave (`FUN_00435cd4`) [I role] |
| ig28 | onde_frappe | 150×150 | swatter impact shockwave |
| ig29 | Paysage | 800×286, 24-bit | inside sky layer |
| ig30 | paysage2 | 1000×384, 24-bit | outside and extreme sky layer |
| ig31 | piece | 675×50 | bonus coin strip |
| ig32 | piecetap | | bonus-round swatter coin [I] |
| ig33 | plaque_tap | | swatter HUD plate (`FUN_004015c8`) |
| ig34 | stardust | | goodie-use sparkle |
| ig35 | Table | 900×79 | inside table front layer |
| ig36 | table2 | 900×79 | outside and extreme table front layer |
| ig37–ig41 | tapette-1, -3, -5, -7, -8 | 181 wide, 178–530 tall | swatter swing frames (`FUN_0043c061`) |
| ig42–ig49 | transi-est, -nord, -nord-est, -nord-ouest, -ouest, -sud, -sud-est, -sud-ouest | 1200×94 each | fly heading strips (east, north, north-east, north-west, west, south, south-east, south-west) |
| ig50 | trenblood | | fly death effect [I] |
| ig51 | treuil | 235×151 | lift / winch (`FUN_00430e48`) |
| ig52 | treuil_corde | 7×380 | winch rope |
| ig53 | tue_mouche | 635×131 | zapper |
| ig54 | tutoBulFarmer | 468×182 | tutorial bubble for the farmer (`FUN_00443488`) |
| ig55 | tutoBulFly | 468×182 | tutorial bubble for the fly |
| ig56 | vent | 20×5 | fan wind particle (`FUN_00429605`) |
| ig57 | ventilo_droite | 666×300 | right-blowing fan |
| ig58 | ventilo_gauche | 666×300 | left-blowing fan |

### 9.4 `rsc/iig*.irc`: shops and bourse (table `0x45e0e0`, 21 entries) [C]

| File | Name | Size | Used by |
|---|---|---|---|
| iig0 | g_background | 800×537 | goodies shop backdrop (`FUN_0042123c`) |
| iig1 | g_barre_haut_tapette | | goodies shop top bar (farmer side) |
| iig2 | g_barres_bas | | goodies shop bottom bars |
| iig3 | g_barres_tapis | | goodies shop conveyor bars |
| iig4 | g_blatte | | goodies shop cockroach |
| iig5 | g_bourse_cache | | bourse page overlay (`FUN_0042008e`) [I] |
| iig6 | g_boursebg | 800×600 | bourse backdrop (`FUN_0042008e`) |
| iig7 | g_classe_bourse | | bourse price rows |
| iig8 | g_flymort | | goodies shop dead-fly tally (`FUN_0042322f`) |
| iig9 | g_Machine | | goodies shop machine |
| iig10 | g_Machine_cache | | goodies shop machine overlay [I] |
| iig11 | g_Selection | | goodies shop selection frame |
| iig12 | g_TapiRoulant | | goodies shop conveyor belt |
| iig13 | p_croix | | traps shop cursor (`FUN_004244e2`) |
| iig14 | p_fleche | | traps shop arrow |
| iig15 | p_icone_colle_mouche | 8×46 | traps shop flypaper icon |
| iig16 | p_icone_treuil | | traps shop winch icon [I role] |
| iig17 | p_icone_tue_mouche | | traps shop zapper icon |
| iig18 | p_icone_ventilo_droite | | traps shop right-fan icon |
| iig19 | p_icone_ventilo_gauche | | traps shop left-fan icon |
| iig20 | p_traps | 800×600 | traps shop backdrop |

### 9.5 `rsc/` non-image files [C]

| File | Used by |
|---|---|
| Intro.avi | intro video, DirectShow (`FUN_00408321`); format notes in `docs/formats/avi.md` |
| casual.ttf | font registered at init (`FUN_00406653`); no screen read selects it (`screens-and-rendering.md`, Conventions) |
| JOINPD__.TTF | font face "Joint by PizzaDude": menus, dialogs, banners, the bonus countdown and the end-screen headings. The HUD, bourse prices, stats rows and high-score rows use the system face Arial Black |

### 9.6 `bor/` (29 files) [C, `FUN_0040ae51`, `FUN_00427954`, `FUN_00429605`, loader `FUN_0042e040`]

| Files | Used by |
|---|---|
| cacahuete, CAN1, CAN2, CAN3, CHIPS1, CHIPS2, CLOPE, cornichon, LAIT, MAIS, mandarine, MUFFIN, poivriere, POMME, POULET, saucisson, saucisson1, SUCRE, TOAST, TOASTER, TOMATE, ZIPPO (22) | collision outline of the matching table object |
| extreme1, extreme2, extreme3 (3) | outlines of the extreme-mode objects |
| palesventi_dt, palesventi_gc | fan blades, right- and left-facing (`FUN_00429605`) |
| piedventi_dt, piedventi_gc | fan stand, right- and left-facing (`FUN_00427954`) |

The startup file check (`FUN_004101f9`) verifies the object and fan outlines [C].

### 9.7 `sons/` (68 files) [C]

| Files | Used by |
|---|---|
| the 57 distinct WAVs in section 8.2 (slots 0–58; j_goodie and b_Ring each fill two slots) | the events in section 8.2 |
| music_game.wav, music_game2.wav | gameplay music, one chosen at random per match (`FUN_00405538`) |
| music_goodies.wav | shop music (`FUN_00421e6a`) |
| music_bonus.wav | bonus round (`FUN_00446e7d`) |
| i_music.mid | menu MIDI (`FUN_0040c833`) |
| w_music_winner.mid | winner screen MIDI (`FUN_0040cbf1`) |
| i_bouton.WAV | UI click (`FUN_004015c8`, `FUN_0041ae17`, `FUN_0042123c`, `FUN_004244e2`) |
| silence.wav | silence buffer (`FUN_0040c6eb`) |
| g_error.wav, j_sifflet.wav, silence (no extension) | no code reference found |

### 9.8 Files outside `data/`

| File | Role |
|---|---|
| The Fly Release 1.2.exe | the game: PE32, MSVC 5 (linker 5.0), not packed [C] |
| scoot.btt, boite.btt | save files (section 7) [C] |
| Readme_English.txt, Readme_français.txt, www.killthefly.htm | documentation and a web link; not read by the exe [I] |

## 10. Coverage of earlier "not read" items

| Item | Status |
|---|---|
| fly update, grab animation, crash sub-states | decoded: `fly-and-world.md` §3.5, §3.6 |
| lift timing | decoded: `fly-and-world.md` §5.6 |
| bourse animation | decoded: `screens-and-rendering.md` §3.3 |
| AI decision rules | decoded: `ai-and-rules.md` §2–§4 |
| winner-screen choreography | decoded: `screens-and-rendering.md` §5.1 |
| stats page layout | decoded: `screens-and-rendering.md` §5.2 |
| flying-goodie spawn probability | decoded: `fly-and-world.md` §6.2 |
| what is still unread | the open list below |

## Open questions

One list for all four documents. **Answered** items say where; **open** items are still unknown.

| # | Question | Status |
|---|---|---|
| 1 | Swatter keys: the in-game help text names D, F, R, G, but the code polls T, F, G, H. | **Answered** for the port: T up, F left, G down, H right (section 3, [C]). **Open:** why the help text differs (cosmetic). |
| 2 | Inside/outside: can data+0x2c take any value other than 1 or 2? | **Answered:** 0 before the first advance, then 1 or 2 only (`ai-and-rules.md` §9.1). |
| 3 | Unreferenced files: `charbon`, `g_error.wav`, `j_sifflet.wav`, extensionless `silence`. | **Open:** no reference in the exe; probably leftovers. |
| 4 | High-score records: what do bytes +0x10..+0x27 hold, and does anything read them? | **Answered:** +0x10..+0x21 hold the run's statistics written by name entry, +0x22..+0x27 are unwritten, and nothing in this exe reads them back (section 7.1). |
| 5 | Mystery-goodie odds and the spawn chance per check. | **Answered:** `fly-and-world.md` §6.2. |
| 6 | AI behaviour per level. | **Answered:** `ai-and-rules.md` §2–§4. |
| 7 | Bonus round length and handicap. | **Answered:** 35000 ms; `ai-and-rules.md` §8.3. |
| 8 | The farmer level item's special case two below the cap. | **Answered:** the price rises by increment − 1; sequence 200/400/600/800/999 (`ai-and-rules.md` §7.2). |
| 9 | Does the boot damage the fly? | **Answered:** it can kill (`fly-and-world.md` §7.3). |
| 10 | Exact effect of midges on the farmer. | **Answered:** `ai-and-rules.md` §6.3, `fly-and-world.md` §8.2. |
| 11 | Winner rule. | **Answered:** the fly wins ties (`ai-and-rules.md` §8.4). |
| 12 | Screenshots and Wine capture: no screen was captured, so every layout is [C] or [I], never [O]. | **Open:** blocked on a from-source Wine build; recipe in the RE tooling's WINE.md. Check positions side by side, especially centred-text formulas (they assume char width = size / 2). |
| 13 | Stack push (`FUN_0042e28a`): exact formula for how a push spreads through a stack (summed weights, square root). | **Open.** |
| 14 | Camera accumulation and the world width W. | **Answered:** `screens-and-rendering.md` §4.2 for the per-layer accumulation; W = 4000 from the asset widths (section 9.3). |
| 15 | The swatter flag that gates the farmer's flying-goodie catch. | **Answered:** the swatter must be in its hit window (swing frame 3 or later) and not blocked (`fly-and-world.md` §6.3). |
| 16 | Splat particle lifetimes (`FUN_00439c2b`): how long a splat lasts before the kill and respawn. | **Open:** the kill and respawn fire when the last particle ends; each particle's lifetime was not read. |
| 17 | Whether the "contained span" y quirk in the outline test (`FUN_0042f6de`) ever changes a result. | **Open.** |
| 18 | The per-delivery price statistic. | **Answered:** price total += trunc(price) per delivery; average = total ÷ delivered (`ai-and-rules.md` §7.5, §8.7). |
| 19 | Do exact replays need the wall-clock reseeding reproduced? | **Answered:** the port needs the same distributions only (`ai-and-rules.md` §0). A deterministic mode is a project decision, not an RE gap. |
| 20 | AI controller: exact reference points for `dy` (`FUN_0043b756`, `FUN_0043bbfe`) and the x-direction "hold" encoding during the reaction delay. | **Open.** |
| 21 | Do the AI goodie budgets reset each period? | **Answered [I]:** the AI is created with the in-game object (`FUN_0043ee0b` step 6), which is built for every period (section 2.2), so they restart (`ai-and-rules.md` §2.6). |
| 22 | Flying-goodie types: the value of N and the farmer type mapping. | **Answered:** N = 3, kinds 3–5; Turbo, SuperFly, Midges for the fly and Triple, Nuclear, Boot for the farmer (`fly-and-world.md` §6.2, §6.3). |
| 23 | Which libc function is `FUN_00450734`? | **Answered:** `sin` (its error handler carries the name "sin"). It shapes the boot hover and the bonus coin entry (`fly-and-world.md` §7.2, `ai-and-rules.md` §8.3). |
| 24 | Boot horizontal speed expression, and whether a boot crush counts as a kill. | **Answered:** `fly-and-world.md` §7.2 (speed) and §7.3 (kill recorded when the splat particles end). |
| 25 | Midge–swatter contact box (`FUN_00442e51`). | **Answered:** an axis-aligned overlap of the midge's top-left, grown by its size, with the swatter's box (`ai-and-rules.md` §6.3). |
| 26 | Trap type 4 (flypaper variant): how a human places it. | **Open.** |
| 27 | Meaning of the 0.152 trap and winch x scale. | **Answered [I]:** the trap shop's 0..760 x range maps onto the object layer's 0..5000 units (`ai-and-rules.md` §4.3). |
| 28 | AI+0x08, the flag that zeroes AI damage output. | **Open.** |
| 29 | Conveyor double press: can a second purchase press during the animation exceed a cap? The owned count lags the press. | **Open.** |
| 30 | Bonus round: displayed intro sequence (the counter shows 3..0) and the screen layout and animation (`FUN_004452a1`, `FUN_00446993`, `FUN_00446030`). | **Open:** the rules are in `ai-and-rules.md` §8.3; the layout is not read (`screens-and-rendering.md` §6). |
| 31 | Measured tick rate. | **Open:** 65 Hz comes from the timer rate constant and was not measured. |
| 32 | Goodies shop render details: counter text positions, the purchase sequence (belt, machine, cockroach), the side-switch animation (`FUN_0042322f`, `FUN_00422571`, `FUN_00423be2`). | **Open.** |
| 33 | Traps shop render: icon positions, font face, whether the backdrop slides in from y −600. | **Open.** |
| 34 | Farewell animation timing and choreography (`FUN_00449fae`, `FUN_0044a641`). Referred to elsewhere as "Farewell timing". | **Open.** |
| 35 | Colours not checked: the `color_shade` cell colours (cell 1 is assumed black) and the 1P keys-page fill. | **Open.** |
| 36 | Bourse: meaning of the class mask values (1/2/4 combinations at `0x4672e8`) beyond "object in play this scene". | **Open.** |
| 37 | Do zapper arcs really kill midges (`FUN_004278f3`)? | **Open [I]:** the arc test runs per midge and puts it into its dying state (`fly-and-world.md` §8.2). |

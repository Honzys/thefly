# The Fly 1.2: behaviour spec

This spec describes how `The Fly Release 1.2.exe` behaves, as reverse-engineered from a local Ghidra dump. It is written for a clean-room reimplementation. It holds no decompiled code and no original assets. Byte layouts of the data formats are in `docs/formats/` (Task A); this document covers behaviour only.

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
| 14, 17, 1a, 1d | main menu | `FUN_00407f42`, `FUN_00408068`, `FUN_00408113` | Plays an intro animation the first time only (`FUN_0041e1be`, 900 ms wait). Menu result 0 → 0x3c (quit dialog), 1 → 0x5a (keys), 2 → start a match (section 2.4), 3 → 0x6e (high scores) [C] |
| 1e, 21, 24, 27 | goodies shop | `FUN_00408650`, `FUN_00408799`, `FUN_00408918` | Each side buys goodies with its own keys; the side shopping is set by data+0x22fc (`FUN_0040aa80`). Result 1 → 0x28 (traps), 2 → 0x50 (bourse), 0 → quit dialog [C] |
| 28, 2b, 2e, 31 | traps shop (farmer) | `FUN_00408984`, `FUN_00408acd` | The farmer places traps. Result 1 → 0x50 [C] |
| 50, 53, 56 | bourse (price board) | `FUN_00409757`, `FUN_00420476` | Shows the current object prices. Enter → 0x32 [C] |
| 32, 38, 3b | gameplay period | `FUN_00408c2a`, `FUN_00408d5c` | On entry, advances the period (`FUN_0040b7b2`). If the match is over → 0x14; otherwise builds the game object and loads assets (`FUN_004015c8`). When the period ends: if dinner or extreme mode (`FUN_0040b922`) → 0x64 (bonus), else → 0x1e. Esc pauses and opens the quit dialog, unless game+0x407bc blocks it [C] |
| 64, 67, 6a | bonus round | `FUN_00408f6a`, `FUN_0040906f` | Section 4.12. Afterwards: match over (`FUN_0040b94c`) → 0x78, else → 0x1e [C] |
| 3c, 3f, 42 | quit / stop dialog | `FUN_0040922a`, `FUN_00409495` | Uses the `boitedlg` frame. From the menu it asks whether to quit to Windows, otherwise whether to stop the game. Yes from the menu → 0x82; Yes from a game → 0x14; No → back to the previous state [C] |
| 5a, 5d, 60 | keys page | `FUN_00409964` | Shows `touches`; in 1P it adds a line saying the player is the fly. Esc or Enter → 0xa0 if it was reached from match start, else back [C] |
| a0, a3, a6 | tutorial page | `FUN_00409d44` | Shows `page_mongol`. Enter → 0x32 [C] |
| 6e, 71, 74 | high scores | `FUN_00409f45`, `FUN_00419610`, `FUN_00419904` | Opens name entry (`FUN_0041a7ac`) when reached from the stats page; done → 0x14 [C] |
| 78, 7b, 7e | winner screen | `FUN_0040a162`, `FUN_0044b431`, `FUN_0044bbbf` | Animated trophy for the winner. Esc or Enter skips it (`FUN_0044bb55`) → 0xaa [C] |
| aa, ad, b0 | statistics | `FUN_0040a38c`, `FUN_0044c73d`, `FUN_0044cab3` | Match statistics (section 4.13). In 1P highscore mode, if the fly score qualifies (`FUN_0040abb0` ≥ 0) → 0x6e, else → 0x14 [C] |
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

1. Reset the match (`FUN_0040b005`, which also loads the shop price table in section 5.6).
2. Set up the farmer (`FUN_0040b50c`) and the fly (`FUN_0040b3c7`), plus extreme-mode setup (`FUN_004242d0`, `FUN_0040b994`).
3. If music is on, pick `music_game` or `music_game2` at random (`FUN_00405538`).
4. If the tutorial is off → first gameplay period (0x32). If it is on → keys page → tutorial page → gameplay. The tutorial period lasts 3 minutes instead of 2 (`FUN_00405ed0`).

### 2.5 Game modes

| Mode | Menu path | Who plays what | Structure | Ends with |
|---|---|---|---|---|
| 1P Normal | 1 Player → Normal → AI level → 2/4/6 days | The human is the fly; the AI (`FUN_0043ea40`) plays the farmer, including shopping (`FUN_004414ea`) and trap placement (`FUN_00425ba2`) [C] | days × (breakfast, lunch, dinner) | winner screen → stats → menu [C] |
| 1P Highscore | 1 Player → Highscore Mode | as 1P Normal, AI level fixed at 1 [C] | 3 days [C] | winner → stats → name entry if the fly score qualifies [C] |
| 2P Normal | 2 Players → Normal → 2/4/6 days | the fly on the arrow keys, the farmer on T/F/G/H [C] | days × 3 meals [C] | winner → stats → menu [C] |
| 2P Extreme | 2 Players → Extreme Mode | as 2P; both sides start with large inventories (section 5.7) and the arena scene [C] | a single period at a random meal and random inside/outside, then the bonus round [C, `FUN_0040b7b2`] | winner → stats [C] |
| Tutorial | Options → Tutorial On, then any mode | preset inventories and money (section 5.7); hint bubbles overlay the game (`FUN_00443772`, `FUN_00443488`) [C] | 3-minute periods [C] | as the chosen mode [I] |

### 2.6 Period, day and match progression [C, `FUN_0040b7b2`]

- data+0x20 holds the period (0 before the first, then 1 = breakfast, 2 = lunch, 4 = dinner) and data+0x21 the day.
- **Normal modes:** periods go 1 → 2 → 4. After dinner, the day goes up and the period returns to 1. data+0x2c flips between 1 = inside and 2 = outside on each period (`FUN_00405538`: 2 shows "Outside", any other value "Inside"). The match is over when the day goes past data+0x22.
- **Extreme mode:** picks one random period and a random inside/outside. The second call ends the match.
- **End of a period:** the clock (game+0x407d8, `FUN_0044214e`, `FUN_004421dd`; 2 minutes, from data+0x26 = 2) runs out, or every object on the table has been delivered (object-manager flag copied to game+0x407b8) [C].
- **Period label:** a banner reads "<meal> Inside/Outside Day <n>" (`FUN_00405538`). The meal labels in the exe are "Breakfast", "Lunch" and "Diner" [C].
- **After each period** (`FUN_0040b922`): if the period was dinner or the mode is extreme → bonus round; otherwise → shops → bourse → next period. The game is over when it is dinner of the last day, or the mode is extreme (`FUN_0040b94c`) [C].

## 3. Input [C, `FUN_00407241`; DirectInput keyboard, no mouse or joystick]

| Role | Up | Down | Left | Right | Action | Goodie select / use |
|---|---|---|---|---|---|---|
| Fly | Up arrow | Down arrow | Left arrow | Right arrow | Enter (grab or drop an object) | Right Shift cycles the goodie; Enter uses it [C, `FUN_00404198`] |
| Farmer | T | G | F | H | Tab (swing) | the key at DIK scancode 0x10 (Q on QWERTY, A on AZERTY) [C, `FUN_004043c1`] |
| Menus | arrows | | | | Enter to select | Esc to go back or open the quit dialog [C] |

- App fields: Up +0x44, Down +0x48, Right +0x4c, Left +0x50, H +0x54, F +0x58, T +0x5c, G +0x60, Enter +0x64, Esc +0x6c, Tab +0x74, scancode 0x10 at +0x78, Right Shift +0x7c [C].
- `FUN_0040735a` detects a fresh press of Esc or Enter. Screens use it to skip, so that a held key does not skip several screens [C].
- The shops use the keys of the side whose turn it is (`FUN_00408799`) [C].
- The in-game key help text lists D, F, R, G for the swatter, but the code reads T, F, G, H (see Open questions).

## 4. Game objects

The game object's update is `FUN_0040326c`. Each tick it receives the fly's five keys, the farmer's five keys, the two goodie keys, the frame time and the FPS [C]. Unless a subsection says otherwise, everything below runs once per 65 Hz tick.

### 4.1 World and camera [C, `FUN_004416b1`, `FUN_00441baf`]

- The world is 4000 px wide (the width of the main scenery layer) and one screen (600 px) tall.
- The camera scrolls horizontally only; its offset is game+0x407c0. Each layer scrolls at its own parallax factor (section 6.2).
- The farmer and the collision tests wrap around the world width (`FUN_0043cea5`, `FUN_0042f6de`) [C].
- Ambient sound: animal noises (slots 14–21) play at random intervals (`FUN_00441b3c`) [C].

### 4.2 Fly (game+0x40; init `FUN_00436df6`, physics `FUN_00437547`, update `FUN_00433ddd`, render `FUN_004388ca`)

**States** [C for alive/stunned/dead/respawn; I for the exact sub-states inside `FUN_00433ddd`]:

| State | Entered when | Behaviour | Leaves when |
|---|---|---|---|
| flying | spawn or respawn | the movement model below | hit, zapped or splatted |
| carrying | Enter pressed within 1000 px of an object (`FUN_0042e704`) | as flying; the object's weight slows climbing (section 4.4) | Enter again (drop), or delivery at the lift |
| stunned / ricochet | swatter or fan damage with a running total of 100 or less | thrown away from the hit; decelerates over 25 ticks (`FUN_00437e99`) | 25 ticks pass |
| dying | running damage total over 100 | crash animation; falls toward (850, 850) (`FUN_0043a158`); sound 29 at frequency 5000 Hz | off-screen |
| respawn | after dying | reappears at a random x, y with the respawn sound (37/38) and a fresh survival timer (`FUN_00438643`, `FUN_004385c6`, `FUN_004387d5`) | immediately back to flying |

**Movement constants** [C, `FUN_00437547`, `FUN_00436df6`; floats resolved in the disassembly]:

| Quantity | Value | Unit |
|---|---|---|
| max speed per axis | 550 + 60 × speed level (data+0x88), plus the turbo bonus | px/s |
| acceleration while a direction is held | 0.1 × max speed | px/s per tick |
| climb while carrying | 75 × f + 0.1 × max / 1.6, where f = 0.2 × strength level (data+0x86); with SuperFly f = min(f + 0.5, 1) | px/s per tick |
| friction with no key held | 9 toward 0 | px/s per tick |
| position | += velocity × dt, clamped to 0..800−w by 0..600−h on screen | px |
| idle jitter | ± random × 2 | px per tick |
| edge scrolling | inside one sprite width of a screen edge, the camera moves at max × overlap / (w/2) | px/s |

**Damage and health** [C, `FUN_00437e99`; fan and zapper callers in section 4.6]:

- Health is 100. Each hit adds to a running damage total (fly+0x2cc).
- The new total is 100 or less → ricochet. On each axis the kick is random × 500 if that axis's velocity was 0, otherwise |v| × (damage / 50) × random + 800 px/s, always away from the swatter. Sound 28.
- The new total is over 100 → dying. On a kill, `FUN_0040bc5e` records the kill type from the swatter's hit count (1 hit → type 3, 2–3 hits → type 2, more → type 1) [C].
- SuperFly divides incoming swatter damage by 2.5 [C, `FUN_0043728f`].

**Assets** [C, `FUN_00435cd4`]: `Face` and the eight `transi-*` strips (heading sprites), `crash-face0001..0004` and `crash-face0011`, `eclatement_fly`, `blood`, `Aspi` (grab effect), `onde_active`, `stardust`, `DUST`. Sounds: the buzz (slot 7) is positional (`FUN_0040d005`: pan from x around screen centre 400; frequency 10000 + random 0–999 + a speed term; quieter off-screen) [C].

### 4.3 Farmer / swatter (game+0x13d20; init `FUN_0043c5f2`, move `FUN_0043cea5`, swing `FUN_0043d510`, hit test `FUN_0043d1a0`)

L = farmer level, data+0x7c (0..5) [C].

| Quantity | Value | Unit |
|---|---|---|
| max horizontal speed | 650 + 60L | px/s |
| horizontal acceleration | ±400 | px/s per tick |
| horizontal stop | instant when the key is released | |
| vertical speed change | ±200, clamped to ±400 | px/s per tick, px/s |
| swing cooldown | 35 − 5L | ticks (field +0xc8) |
| hit power | L, or 5 while Nuclear is active | level |
| hit radius R | swatter width / 2 + fly width / 2 | px |
| damage | int((40 + 6L) × (1 − distance / R)); Nuclear uses 74 instead of 40 + 6L | HP |

- The swatter is drawn larger or smaller with depth (scale field +0xb8) [C].

**Swing states** [C, `FUN_0043d510`]:

| Frame | What happens |
|---|---|
| idle | waits for Tab while the cooldown is 0 and the swatter is allowed to hit (the mystery goodie can block it, field +0x154) |
| 1 | swing sound (slot 27) at a random frequency of 35000–49999 Hz (40000 Hz for Triple Hit); 20% chance of the voice line in slot 34 |
| 2 | wind-up |
| 3 | active: the hit test runs from this frame on |
| 4 | impact: shockwave sprite (`FUN_0043df24`); with Nuclear, a 70% chance of sparks. Then the swatter retracts and the cooldown starts |

- Voice lines in slots 30–33 (win, hurry, miss, lose) are chosen in `FUN_0043c730` [I for the exact triggers].
- **AI (1P):** `FUN_0043ea40` (init `FUN_0043edb1`, `FUN_0043ee0b`, at game+0x14e48) drives the same key inputs as a human farmer. It has three levels (Easy, Normal, Inteligent) [C]. Its decision rules were not read in detail [I].
- **Assets** [C, `FUN_0043c061`, `FUN_00448802`]: `tapette-1`, `-3`, `-5`, `-7`, `-8` (swing frames), `onde_frappe` (shockwave), `fleche_tap` (position arrow), `etincelle` (sparks), `plaque_tap` (HUD plate). `piecetap` is used by the bonus-round swatter [I].

### 4.4 Table objects (game+0x3df60 manager `FUN_0042c888`; instances in `DAT_0049bd20`, stride 0x874)

- **Object set:** 22 normal objects, plus 3 that appear only in extreme mode. Each has a sprite (`div*.irc`), a `.bor` outline (section 6.4), a weight (short at object +0x90 / +0xb0) and a base price [C, `FUN_0040ae51`].
- **Spawning:** objects are placed on the table with collision spacing (`FUN_0040bd61`). Three random x positions in 0..760 px, at least 36 px apart, are chosen for drops (`FUN_0040bf70`) [C].
- **Object states:**

  | State | Behaviour |
  |---|---|
  | resting | on the table |
  | carried | follows the fly; adds its weight to the fly's vertical velocity each tick [C] |
  | falling | on release, falls with the fly's vx / 50; the fly pops up 20 px [C] |
  | delivered | dropped onto the lift (`FUN_00431115` returns −1); plays g_money (slot 11), adds to the saved count per type (+0x1f0) and to the stats [C] |
  | pushed | a fan can blow it sideways (`FUN_0042754d`) [C] |

- **Base prices** [C, `FUN_0040ae51`]: can1 60, can2 15, can3 12, chips1 5, chips2 5, clope 22, saucisson 90, saucisson1 20, cacahuete 10, zippo 100, lait 60, mandarine 21, pomme 43, sucre 30, toaster 100, toast 18, poulet 60, poivriere 10, tomate 30, muffin 40, cornichon 8, mais 65. Extreme mode adds extreme1 80, extreme2 50, extreme3 20.
- **Price fluctuation** [C, `FUN_0040ba40`]: each price moves by ± (random 0–29)% of itself, with a floor of 1.0. The result is shown on the bourse page. That it is applied once per period is [I].
- **Footsteps:** slots 22–26 play from `FUN_0042dc21` [I].

### 4.5 Lift / winch (`DAT_0049bba0`; `FUN_00430e48`, `FUN_00431115`, `FUN_00431931`, `FUN_0042a760`)

- The lift is the delivery point; the fly drops carried objects on it [C, `FUN_00431115`].
- It is drawn with `treuil` and a `treuil_corde` rope. The winch sound (slot 47) plays while it moves [C].
- Its movement timing was not read in detail [I].

### 4.6 Traps (game+0x3ffb0 manager, update `FUN_00427270`, render back `FUN_00427a27`, render front `FUN_00427b54`)

The farmer buys traps in the traps shop with trap tokens (section 5.6). The manager keeps three arrays [C, `FUN_00427270`]:

- flypapers: `DAT_0046c948`, stride 0x874, up to 7
- fans: `DAT_00470478`, stride 0x7568, up to 5
- zappers: `DAT_00494f80`, stride 0x3610, up to 2

| Trap | Assets | Effect on the fly | Sound |
|---|---|---|---|
| Flypaper (`FUN_0042663b`) | `colle_mouche1`, `colle_mouche2` | while the fly overlaps the strip (`FUN_0042825c`), both velocity components are divided by 1.4 each tick [C] | none found |
| Fan, left- or right-facing (`FUN_0042a075`, `FUN_00429605`, `FUN_00427954`) | `ventilo_gauche`, `ventilo_droite`, `vent` (wind particles); `palesventi_gc`/`_dt` and `piedventi_gc`/`_dt` outlines | collision with the blades (`FUN_0042f6de`), at most once per 100 ms: pushes the fly sideways by the fan strength (+0x91c) and up by strength + 100 px/s; adds damage of 40–59, or 20–29 when the protection flag fly+0x24c is set (likely SuperFly [I]); a total of 100 or less stuns, over 100 splatters the fly into 60 + 11 particles (`FUN_004397c0`) [C] | slot 44, loop at −400 or −100 (louder when blowing); positional when there is a single fan; slot 45 on impact at 20000–25999 Hz [C] |
| Zapper (`FUN_004288fe`, `FUN_00428b47`, `FUN_0042699d`, `FUN_00428540`) | `tue_mouche`, `eclaires`, `etincelle` | a fly inside the zapper's column whose top edge overlaps the zapper body (within height − 80 px of the zapper's top edge) is grilled: instant death (`FUN_00439f29`) and slot 43. An arc (`FUN_00428b47`, sound 39) also pushes the fly up 180 px/s and adds 1 damage per tick, dying past 100 [C] | slot 40, a loop at −500 idle or −200 while arcing; −450 positional when there is a single zapper [C] |

- **Placement:** traps go into up to 15 slots at data+0x2a2. Positions are checked by `FUN_0040bd61`. Placing one plays p_build (slot 13) [C, `FUN_00425456`].
- **Per-type caps:** at data+0x298..0x29e [C]. The values 7, 5, 5, 2 and 7 next to the trap names at data+0xa1 are probably those caps [I].

### 4.7 Fly goodies [C, `FUN_00404198`]

- The goodie selector (game+0x408a8) cycles through 0..3 with Right Shift; Enter uses the selected one if its count is above 0.
- Using a goodie plays j_goodie (slot 37/38) and a stardust effect.

| # | Goodie | Count field | Effect | Duration | Sound |
|---|---|---|---|---|---|
| 1 | Turbo (`FUN_00436fc1`) | data+0x8a | +50% of base max speed, with a velocity kick on the dominant axis; afterwards max speed decays by 5% of base per tick | 1500 ms | 35 |
| 2 | SuperFly (`FUN_0043728f`) | data+0x8c | swatter damage ÷ 2.5; better carrying climb (section 4.2) | 8000 ms | 36 |
| 3 | Midges (`FUN_00404caf`) | data+0x8e | see section 4.9 | until they leave | 48 |

### 4.8 Farmer goodies [C, `FUN_004043c1`]

- The selector is game+0x408aa; the goodie key (scancode 0x10) cycles and uses it.

| # | Goodie | Count field | Effect | Duration |
|---|---|---|---|---|
| 1 | Triple Hit (`FUN_0043c8e2`) | data+0x80 | the next swings form a triple strike (flag +0xdc); higher swing pitch | [I] |
| 2 | Nuclear Hit (`FUN_0043cb71`) | data+0x82 | hit power 5, damage base 74, sparks | 5000 ms (20000 ms from a mystery goodie) |
| 3 | Friendly Boot | data+0x84 | calls the boot (section 4.10) | one stomp |

### 4.9 Midges (game+0x178a0, 300 slots; spawn `FUN_00404caf`, render `FUN_00404ef3`)

- **Spawn** [C]: min(100, 300 − active midges) midges. Each starts at x = 800 + random(0 .. world width − 811), y = random 0–599, with a speed of ±(200..399) px/s and a size of 30–34 px.
- **Behaviour:** each flies toward a random point on the swatter [C]. The intent is to get in the farmer's way (from the hint strings); the exact gameplay effect was not read [I].
- **Assets:** `moucheron`, `moucheron1` [C].

### 4.10 Friendly boot (game+0x3dc60; `FUN_0043aabe`, `FUN_0043abc7`, `FUN_0043b157`, `FUN_0043b370`, render `FUN_0043b41b`)

- When called, a giant boot (`i_botte`) stomps down. The active flag is game+0x3df58. The boom (slot 6) plays [C].
- Whether it damages the fly or only scares it was not traced [I].

### 4.11 Flying goodies (game+0x15000; `FUN_00432684` spawn, `FUN_00432dfb` update, `FUN_0043309b` catch, render `FUN_00432e7b`)

- Two flyers can be on screen: one for the fly (sub-object +0x1c8) and one for the farmer (+0x18). They are drawn with `goodiesmystere`/`goodies` icons and `ailesgoodies` wings [C].
- **Spawning** [C for the timer values, I for the full probability formula]: a spawn check is re-armed every 8000 ms plus a random term. After a spawn, the next check waits 16000 ms (fly side) or 20000 ms (farmer side). A roll below 3 out of 100 chooses a rarer type. The flyer spawns at a random (x 0–799, y 0–499) and wanders slowly.
- **Catching** [C, `FUN_0043309b`]:
  - Fly catches its flyer: type below 10 → +1 to one fly goodie count (data+0x84 + 2 × type). Type 10 or above → mystery: 50% inverted controls (result 1; the fly's key pairs are swapped on both axes), 50% Turbo for 25000 ms (result 2).
  - Farmer catches its flyer: type below 10 → +1 to one farmer goodie count (data+0x7a + 2 × type). Type 10 or above → mystery: 50% "swatter cannot hit" (result 0x0b, sets +0x154), 50% Nuclear for 20000 ms (result 0x0c).
  - Every catch plays slot 42. A mystery effect arms a 15000 ms timer; the flags game+0x40898 and game+0x40899 hold the active mystery state.

### 4.12 Bonus round (`FUN_00444da0`, `FUN_004452a1`, update `FUN_00446030` and `FUN_004472df`, render `FUN_00446993`)

| Step | What happens |
|---|---|
| intro [C] | a big "3, 2, 1" countdown at 1000 ms steps, in the `JOINPD__.TTF` face, with b_321 (slot 52); music_bonus (`FUN_00446e7d`); background `bckbonusfond` with the bar `bckbonusbarre` |
| play [C for the counters, I for the duration] | coins (`piece`) fall. The fly scores by collecting them (counter +0xd864); the farmer scores by swatting with the bonus swatter `FUN_00449203` (counter +0xd868; swing slot 27, voice 34). The value 35000 is stored at +0xd8a0, probably the round length in ms [I]. A handicap for each side is set from the balance meter (`FUN_0040c086`, section 5.5) |
| result [C] | fly money and score += 2 × fly coins; farmer money and score += 10 × farmer hits. A two-line tally shows "Score the Fly" and "Score the Farmer". Slot 53 plays if the farmer total is larger, otherwise slot 51 |

### 4.13 Statistics (`FUN_0040d3e0`, `FUN_0040d4ea`; page `FUN_0044bf60`)

- Counters [C]: periods played (`FUN_0040c230`), objects saved, average per period, max and average fly survival time (GetTickCount based, `FUN_0040db31`, `FUN_0040db52`), average object price, swatter hits, flies killed (`FUN_0040c250`), kills per period, hit ratio and average damage.

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
| first menu intro wait | 900 ms |
| Turbo | 1500 ms (25000 ms from a mystery goodie) |
| SuperFly | 8000 ms |
| Nuclear | 5000 ms (20000 ms from a mystery goodie) |
| mystery effect timer | 15000 ms |
| fly ricochet | 25 ticks |
| fan re-hit guard | 100 ms |
| bonus countdown step | 1000 ms |

### 5.3 Scoring [C]

- **Fly score** (data+0x30) and **fly money** (data+0x92): at the shop payout after each period (`FUN_00422223`) both go up by the sum over object types of saved count × current price (`FUN_0040ba40`). The bonus round adds 2 per coin.
- **Farmer score** (data+0x38) and **farmer money** (data+0x90): +42 per kill (data+0x15c, `FUN_0040bbe5`), paid at the same shop payout. The bonus round adds 10 per hit.
- **High score:** the table records the fly score (data+0x30) [C, `FUN_0040abb0` caller].
- **Winner:** decided by comparing the two scores [I, `FUN_0044b431`].

### 5.4 Difficulty knobs [C]

- Fly speed and strength levels 0–5, and farmer level 0–5, are all bought in the shop.
- AI level 0–2 sets the AI farmer's skill [I].
- Period length 2 minutes; match length 2, 4 or 6 days (3 in highscore mode).

### 5.5 Balance meter [C, `FUN_0040c086`]

- diff = fly saved value − (farmer base + 42 × kills), clamped to ±1000.
- Each side's ratio = (1 − |diff| / 1000) / 2, mirrored by side.
- It drives the bonus-round handicap (section 4.12).

### 5.6 Shop prices [C, `FUN_0040b005`, `FUN_00422223`, `FUN_00421e6a`]

Buying beyond the cap, or without enough money, plays gp_error (slot 12). A purchase plays g_money (slot 11).

| Side | Item | Price | Cap | Price increase per purchase |
|---|---|---|---|---|
| Farmer | level / strength | 200 | 5 | +200 |
| Farmer | trap tokens (data+0x7e) | 55 | 14 | 0 |
| Farmer | Triple Hit | 30 | 99 | 0 |
| Farmer | Nuclear Hit | 40 | 99 | 0 |
| Farmer | Friendly Boot | 65 | 99 | 0 |
| Fly | strength | 100 | 5 | +100 |
| Fly | speed | 100 | 5 | +100 |
| Fly | Turbo | 20 | 99 | 0 |
| Fly | SuperFly | 55 | 99 | 0 |
| Fly | Midges | 60 | 99 | 0 |

- The farmer's level item has a special case two below the cap [C, `FUN_00422223`]; its effect was not decoded [I].
- When the farmer finishes shopping with trap tokens left, the traps shop opens; otherwise the game moves on [C].
- **Trap token costs** [C, `FUN_00424a6f`, `FUN_00424c83`, `FUN_00424db1`, `FUN_00425456`]: flypaper 1, fan (left or right) 2, zapper 4.

### 5.7 Starting inventories [C, `FUN_0040b3c7`, `FUN_0040b50c`]

| Setup | Fly: strength, speed, turbo, superfly, midges | Fly money, score | Farmer: level, tokens, triple, nuclear, boot | Farmer money, score |
|---|---|---|---|---|
| Normal | 0, 0, 5, 3, 0 | 0, 0 | 0, 0, 3, 1, 0 | 0, 0 |
| Extreme | 4, 4, 20, 20, 0 | 0, 0 | 4, 0, 20, 15, 0 | 0, 0 |
| Tutorial | 3, 3, 50, 50, 50 | 1000, 4200 | 3, 6, 50, 50, 50 | 400, 3000 |

## 6. Rendering

### 6.1 Display [C, `FUN_00406803`, `FUN_004070ef`]

- DirectDraw 7, full-screen 800 × 600 at 16-bit colour.
- A primary surface with one back buffer, flipped each tick. On a lost surface it restores.
- A Direct3D 7 device is created; its use was not traced [I].
- Text is drawn with GDI TextOut in two TrueType faces loaded from `data\rsc`: `casual.ttf` (menus and HUD [I]) and `JOINPD__.TTF` (dialogs and the bonus round [C, `FUN_00446030`]).

### 6.2 In-game scene layers [C, `FUN_004416b1`]

- Layers 1–3 use magenta (0xFF00FF) as the transparent colour key.
- The scene variant is chosen by data+0x2c: 1 = inside, 2 = outside, and extreme mode uses the arena.

| Layer | Parallax | Inside (1) | Outside (2) | Extreme |
|---|---|---|---|---|
| 0 sky | 0.25 | `Paysage`, y 0 | `paysage2`, y 0 | `paysage2`, y 0 |
| 1 middle | 0.75 inside, 0.5 otherwise | `Jardin`, y 205 | `collines`, y 280 | `collines`, y 280 |
| 2 main (sets the 4000 px world width) | 1.0 | `Maison`, y 0 | `dehors`, y 230 | `arene`, y 123 |
| 3 table front | 1.25 | `Table`, y 525 | `table2`, y 525 | `table2`, y 525 |

### 6.3 Draw order per frame [C for the order, I for the role of the unnamed effects; `FUN_00403e36`]

1. Back scene layers (`FUN_00441baf`).
2. Effect layer at game+0x3fd90.
3. Dust (game+0x158b0).
4. Swatter shadow / back part (`FUN_0043d74a`).
5. Midges (`FUN_00404ef3`).
6. Front scene layers (second `FUN_00441baf` pass).
7. Table objects.
8. Boot (back).
9. Traps (back).
10. Table-object manager (`FUN_0042d2e3`).
11. Effect at game+0x15740.
12. Fly (`FUN_004388ca`).
13. Boot (front).
14. HUD (`FUN_00404640`: clock `horloge`, goodie panel `croix_goodies`, swatter plate).
15. Flying goodies.
16. Swatter (`FUN_0043db53`).
17. Traps (front).
18. Tutorial bubbles.
19. Period label (`FUN_00405538`).
20. End-of-period banner (game+0x3fe40).

### 6.4 Sprite sheets and outlines

- **Sprite sheets** [I, from sizes and `FUN_00413b73`]: animated sprites are horizontal strips cut into frames. For example, `Face` and each `transi-*` is 1200 × 94, and `moucheron` is 162 × 27. The frame count is passed in code at load time. Other animations are separate files (`tapette-*`, `crash-face*`).
- **`.bor` outlines** [C for the structure, I for the purpose]: `FUN_0042e040` reads a `.bor` as 500 rows, each a list of 16-bit values ended by −1. They are kept as per-row linked lists on the object. The collision code (`FUN_0042f6de`, `FUN_0042dda4`, `FUN_00426398`) does a bounding-box test first and then walks these rows. So `.bor` files are per-scanline collision outlines for objects and fan parts, not render masks. Byte layout: `docs/formats/bor.md`.

## 7. Save files

Both files are opened relative to the working directory: read with "rb" (`FUN_0040ab80`), written by `FUN_0040fe40`. See `docs/formats/btt.md` for byte layouts [C].

### 7.1 `scoot.btt`: high-score table [C, `FUN_00410a69`, `FUN_00410ad3`, `FUN_00410c24`, `FUN_00410cbd`, `FUN_00410f04`]

- **Layout:** ten 48-byte records, then an 8-byte trailer.
  - Each record holds a name (12 bytes, NUL padded) at +0, the score (signed 32-bit) at +0x0c, and a checksum (64-bit float) at +0x28. Bytes +0x10..+0x27 are not written deliberately; they hold whatever was on the stack.
  - The trailer is the checksum of the sum of all scores.
- **Checksum c(s):** x = 5.365799563 × s − sqrt(5.365799563); if x < 0, x = 0.1835; c = sqrt(x).
  - Empty slots get a random filler checksum (random 0–7999 / 5.63).
  - If the score sum is below 1, the trailer gets random 0–7999 / 6.8899.
- **Validation** (`FUN_00410ad3`): every non-zero score must match its checksum, and the trailer must match.
- **Insert** (`FUN_00410cbd`): finds the first slot whose score is lower, shifts the rest down, then rewrites every checksum. `FUN_00410f04` sets the name.
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

| Slot | File | Event (function) |
|---|---|---|
| 0 | i_melody | menu intro / ambience (`FUN_0041d3cc`, `FUN_0041e598`) [C call, I meaning] |
| 1 | i_Cowboy | menu animation (`FUN_0041e598`) [C call, I meaning] |
| 2 | i_pig | menu pig animation (`FUN_0041e1be`, `FUN_0041e63c`) [C] |
| 3 | i_ha | menu selection (`FUN_0041be05`) [C] |
| 4 | i_fucking_pig | menu pig animation (`FUN_0041e598`) [C] |
| 5 | i_kickpig | menu pig kicked (`FUN_0041e63c`) [C] |
| 6 | z_boom | menu (`FUN_0041d3cc`, `FUN_0041eb60`); friendly-boot stomp (`FUN_0043b370`) [C] |
| 7 | z_Bzz | fly buzz, positional (`FUN_0040d005`); menu (`FUN_0041d3cc`) [C] |
| 8 | g_blatte | goodies shop cockroach (`FUN_0042123c`, `FUN_00423be2`, `FUN_00423f2e`) [C] |
| 9 | g_tapis | goodies shop conveyor (`FUN_0042123c`, `FUN_00421e6a`, `FUN_0042322f`) [C] |
| 10 | g_machine | goodies shop machine (`FUN_0042123c`, `FUN_0042322f`) [C] |
| 11 | g_money | purchase; object delivered (`FUN_0042c888`); stats tally (`FUN_0044c73d`) [C] |
| 12 | gp_error | purchase refused (shops) [C] |
| 13 | p_build | trap placed (`FUN_00425456`) [C] |
| 14–21 | j_birds1, j_Birds2, j_sheep, j_dog1, j_dog2, j_cow, j_rooster, j_chicken | random ambience (`FUN_00441b3c`) [C] |
| 22–26 | j_pas1..5 | footsteps (`FUN_0042dc21`) [I] |
| 27 | j_frappe_tap | swatter swing (`FUN_0043d510`, `FUN_00449203`) [C] |
| 28 | j_ricoche_fly | fly hit, ricochet (`FUN_00437e99`) [C] |
| 29 | j_crash_fly | fly killed (`FUN_00437e99`) [C] |
| 30–33 | j_fvoix_gagne, _hate, _rate, _perdu | farmer voice lines (`FUN_0043c730`; 33 also `FUN_0044a9a8`) [C call, I trigger] |
| 34 | j_fvoixhaaa | swing shout, 20% chance (`FUN_0043d510`, `FUN_00449203`) [C] |
| 35 | j_turbo | Turbo used (`FUN_00436fc1`) [C] |
| 36 | j_super_fly | SuperFly used; high-score screen (`FUN_00419904`) [C] |
| 37, 38 | j_goodie | goodie used; fly respawn; high scores [C] |
| 39 | jpt_eclair | zapper arc (`FUN_00428b47`) [C] |
| 40 | jpt_tuemouc | zapper hum (`FUN_00427270`) [C] |
| 41 | j_aspi | fly grabs an object (`FUN_00433ddd`) [C] |
| 42 | j_goovolant | flying goodie caught (`FUN_0043309b`); bonus round [C] |
| 43 | jpt_grillemouc | fly grilled by the zapper (`FUN_004288fe`) [C] |
| 44 | jpv_ventilo | fan running (`FUN_00427270`) [C] |
| 45 | jpv_chocventilo | fly hits a fan (`FUN_0042a075`, `FUN_0042754d`) [C] |
| 46 | j_splash | fly splatter (`FUN_004397c0`, `FUN_0044a641`) [C] |
| 47 | j_treuil | winch / lift moving (`FUN_00431931`, `FUN_0042a760`) [C] |
| 48 | j_moucherons | Midges goodie (`FUN_00404caf`) [C] |
| 49, 50 | b_ring | preloaded for bourse, winner and stats (`FUN_0040c9dc`, `FUN_0040cbf1`, `FUN_0040cd8b`) [C load, I event] |
| 51 | b_iha | bonus result, fly ahead; winner; high scores [C] |
| 52 | b_321 | bonus countdown; game start (`FUN_004015c8`) [C] |
| 53 | b_ihatap | bonus result, farmer ahead; winner [C] |
| 54 | w_glingcup | winner trophy (`FUN_0044b431`) [C] |
| 55–58 | w_vibrofly1/2, w_vibrofarmer1/2 | winner screen celebration (`FUN_0044b431`) [C] |

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
| casual.ttf | font registered at init (`FUN_00406653`) |
| JOINPD__.TTF | font face "Joint by PizzaDude": dialogs and bonus round (`FUN_00446030`) |

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

## 10. Not read in detail

These were identified but not traced to the line. A reimplementer should treat their behaviour as open:

- the fly's full update, including the grab animation and the crash sub-states (`FUN_00433ddd`)
- the lift timing (`FUN_00430e48`, `FUN_00431115`)
- the bourse animation (`FUN_00420476`)
- AI decision rules (`FUN_0043ea40`, `FUN_004414ea`, `FUN_00425ba2`)
- winner-screen choreography (`FUN_0044b431`)
- the stats page layout (`FUN_0044c17a`)
- the flying-goodie spawn probability formula (`FUN_00432684`)

## Open questions

1. **Swatter keys:** the in-game help text names D, F, R, G, but the code polls T, F, G, H (DIK 0x14, 0x21, 0x22, 0x23). Is the text a leftover from an AZERTY or earlier layout? The reimplementation should follow the code.
2. **Inside/outside:** data+0x2c = 1 shows "Inside" and uses the house scenery (`FUN_00405538`, `FUN_004416b1`). It is unclear whether any other value than 1 or 2 ever occurs.
3. **Unreferenced files:** `charbon`, `g_error.wav`, `j_sifflet.wav` and the extensionless `silence` have no reference in the exe. Are they leftovers?
4. **High-score records:** bytes +0x10..+0x27 of each `scoot.btt` record hold uninitialised stack data. Does any version read them?
5. **Mystery-goodie odds:** the rare-type roll (3 in 100) and the exact spawn chance per check were not fully decoded (`FUN_00432684`).
6. **AI:** its behaviour per level is unknown beyond it using the farmer's controls, shop and trap placement.
7. **Bonus round:** is 35000 at +0xd8a0 the round length in ms, and how exactly does the balance meter handicap each side?
8. **Farmer level item:** what is the special case two below the cap in `FUN_00422223`?
9. **Friendly boot:** does it damage the fly or only shake the screen?
10. **Midges:** what is their exact effect on the farmer?
11. **Winner rule:** the higher score is assumed to win; the comparison in `FUN_0044b431` was not read.
12. **Screenshots:** none were taken, so screen layouts come from code and asset sizes only.

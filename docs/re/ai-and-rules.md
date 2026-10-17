# The Fly 1.2: farmer, AI, traps, goodies, economy and match rules

Phase 1c clean-room description, written for the Rust engine workers. It extends `game-spec.md`, and where the two disagree this file wins. That file covers the state machine, assets, rendering and the fly. This file covers the farmer and swatter, both AIs, shops, traps, goodies, scoring and progression, at the level of exact rules and numbers.

Tags: **[C]** means read directly from code (the cited function). **[I]** means inferred: a name, a purpose, or a value derived without reading every path. Addresses are virtual addresses in `thefly.exe`. Offsets such as "data+0x7c" are byte offsets into the shared game-data object (the one `game-spec.md` calls data), "game+…" into the in-game scene object, "farmer+…" into the farmer object.

---

## 0. Conventions used by every rule below

| Item | Rule |
|---|---|
| Tick | 65 Hz. One update per tick. `dt` is measured wall-clock seconds (see game-spec §3) [C] |
| Truncation | every float→int conversion truncates toward zero (x87 `ftol` semantics). Written `trunc(·)` below [C] |
| Modulus | `a % b` is C remainder: the result has the sign of `a`. `rand` is never negative, so `rand % b` with a negative `b` equals `rand % |b|`. Division by zero is not guarded anywhere it is noted [C] |
| RNG | `rand()` is the MSVC LCG: `seed = seed·214013 + 2531011 (mod 2³²)`, returns `(seed >> 16) & 0x7fff` (0..32767) [C, `FUN_0044f260`]. `srand(time(NULL))` runs in several constructors (`FUN_00416eb0`, `FUN_0042ac27`, `FUN_0042bdf8`, `FUN_00430e48`, `FUN_00436df6`) [C], so the original is not replayable. The port only needs the same distributions; it does not need the same RNG call order |
| "rand odd" | used as a coin flip: `rand % 2 == 1` [C] |
| Timers | a timer has a deadline and a period. *Arm(ms)*: deadline = now + ms, period = ms. *Expired(rearm)*: true once now ≥ deadline; with rearm the next deadline is now + period [C, `FUN_004194a9`, `FUN_00419506`] |
| AI timer helper | *Tick(timer, ms)*: if the timer was never armed, arm it with `ms` and return false; otherwise return Expired(rearm = true) [C, `FUN_0043e757`] |
| Sides | in the balance meter and the flying goodies, side 1 = fly and side 2 = farmer [C] |

---

## 1. Farmer and swatter

### 1.1 Constants and initial state [C, `FUN_0043c061`, `FUN_0043c5f2`]

| Name | Value | Notes |
|---|---|---|
| level `L` | data+0x7c | 0..5, bought in the shop |
| maxLevel | 5 | used by Nuclear as "power" |
| maxSpeedX | 650 + 60·L px/s | |
| cooldownLimit | 35 − 5·L ticks | |
| hitbox width | 130 px | hard-coded. The hit radius uses this, not the 181-px sprite |
| hitbox height | 154 px | |
| yTop | 351 = 529 − 178 | 178 = tapette sprite height |
| yBottom | 472 | |
| ySpan | 121 = 178 − 57 | |
| reachMax | 375 = 529 − 154 | |
| world ring | 4000 px | farmer+0x70 |
| start x | rand % 800 | |
| start y | 351 | |
| voice timer | armed 10000 ms | |

### 1.2 Movement, per tick [C, `FUN_0043cea5`]

Inputs: four direction keys (right, left, up, down), `dt`, and the camera scroll delta for this tick (game+0x407c0, px). The delta is written by the fly's edge-scroll code, so the farmer's x is relative to the screen on a 4000-px ring.

1. If the "ignore scroll once" flag is set, treat the delta as 0 and clear the flag.
2. Choose the speed cap. When slowed by midges (§6.3), the cap is `slowSpeed` and `vy` is forced to 0. Otherwise it is `maxSpeedX`.
3. Right held: `vx = min(vx + 400, cap)`. Left held: `vx = max(vx − 400, −cap)`. Neither held: `vx = 0`. (The +400 is a per-tick step, not scaled by dt.)
4. Up held: `vy = max(vy − 200, −400)`. Down held: `vy = min(vy + 200, 400)`. Neither held: `vy = 0`.
5. `x += trunc(vx·dt − scrollDelta)`; `y += trunc(vy·dt)`.
6. Wrap x: if `x < 1`, add 4000; if `x ≥ 4000`, subtract 4000.
7. Clamp y to [351, 472].
8. `reach = (1 − (472 − y)/121) · 375`, a float from 0 at the top to 375 at the bottom.

The AI calls the same routine with its own four key flags (§2.4 step 11) [C].

### 1.3 Swing state machine, per tick [C, `FUN_0043d1a0`, `FUN_0043d510`]

State: `frame` (0..4), `swinging`, `retracting`, `cooldown` (ticks), `active` (hit active this tick), `blocked` (set by the farmer's mystery "cannot hit", game-spec §4.11), `triple`, `nuclear`, and `missStreak` (ticks with no hit).

Edge input: for a human, the swing key counts only on a press edge: the current value is ≥ 1 and the previous stored value (game+0x407cc) was < 0 [C]. Using a farmer goodie sets that stored value to 10, so the key must be released before the next swing [C, `FUN_004043c1`]. The AI passes its swing request in directly as the edge [C, `FUN_0043f611`].

Per tick:

1. If `triple`, run the Triple step (§1.6). Otherwise, if `nuclear`, run the Nuclear step (§1.7).
2. If `retracting` and not `swinging` and not `nuclear`: `cooldown += 1`. If `cooldown > cooldownLimit`, then `retracting = false` and `cooldown = 0`.
3. Run the swing step when any of these holds:
   - `triple` and `swinging`
   - `nuclear` and `swinging`
   - (`edge` or `swinging`) and `cooldown == 0`
4. **Swing step.** If not `retracting`:
   1. Set `swinging` and do `frame += 1`.
   2. On frame 1:
      - Increment the stats swing counter (§8.7).
      - Set sound 27's frequency to 35000 + rand % 15000 Hz, or 40000 with `triple`, and play it positionally.
      - If `rand % 100 < 20`, also play voice 34.
   3. If `frame > 2`, the hit is active.
   4. If `frame > 4`:
      - Set `frame = 4` and `retracting = true`.
      - Unless `blocked`, compute `power = L`. With `nuclear`, `power = 5`, plus sparks when `rand % 10 < 7`.
      - Spawn a shockwave at (x + 60, …) with display scale `power·70 + 150` (10 slots, cosmetic) [C, `FUN_0043df24`].

   If `retracting` instead: `frame −= 1` and keep `swinging`. When `frame < 1`, set `frame = 0` and `swinging = false`.
5. While not active, `missStreak += 1`. `blocked` forces `active = false`.

Timeline of one press at cooldown 0:
- Ticks 1–2 wind up.
- Ticks 3, 4 and 5 have the hit active. The shockwave appears on tick 5.
- Ticks 6–9 retract to frame 0.
- `cooldownLimit + 1` cooldown ticks follow. A fresh edge on the tick the cooldown clears starts the next swing.

The minimum swing period is therefore 9 + 36 − 5L ticks: 45 ticks (0.69 s) at L 0 and 20 ticks (0.31 s) at L 5 [C].

### 1.4 Hit test and damage, on each active tick [C, `FUN_0043d510`]

1. `missStreak += 1`.
2. Swatter centre: `cx = x + 65`, `cy = trunc(77 + reach)`.
3. Fly centre: (flyX + flyW/2, flyY + flyH/2), in screen space (fly+0x1c4, fly+0x1cc).
4. `d = sqrt(dx² + dy²)`. `R = 65 + flyW/2`.
5. If `d ≤ R`: `f = 1 − d/R` and `missStreak = 0`. Otherwise `f = 0`.
6. `bonus = trunc(6·L)`, or 34 with Nuclear.
7. `damage = trunc((40 + bonus) · f)`. The peak is 40..70 by level, and 74 with Nuclear.
8. Direction hints for the fly's knock-back:
   - x hint = 0 if the fly's right edge < cx, 1 if cx < flyX, 2 if they overlap.
   - y hint is computed the same way.

On the fly side [C, `FUN_00433ddd` → `FUN_00437e99`]:
- Damage applies only when `damage ≥ 1` and the fly is not already in its hit state, so a swing lands at most once.
- SuperFly divides it: `damage = trunc(damage / 2.5)`.
- The running total decides the result: ≤ 100 is a ricochet, > 100 kills the fly.
- Sound 29 plays with its volume set to 5000.
- Each landed hit increments the fly's hit count, which sets the kill type (§7.6).
- The stats hit counter and damage sum update when the fly enters the hit state [C, `FUN_00439156`].

### 1.5 Voices and footsteps (side effects only) [C, `FUN_0043c730`]

All play positionally.

- Voice timer expired and `rand % 100 < 7`: re-arm the timer to 12000 + rand % 7000 and play slot 31.
- `missStreak > 7` and `rand % 4000 < 5`: slot 32.
- Rising edge of "objects left on the table" with `rand % 100 < 15`: slot 33.
- Rising edge of "fly dead" with `rand % 100 < 10`: slot 30.
- Footsteps:
  - While `vx ≠ 0`, a step counter increments. When it exceeds 20, it resets to 0 and a random one of 5 step sounds plays.
  - When `vx = 0` and the counter ≠ 0, the counter jumps to 30, so the next movement steps at once.

### 1.6 Triple Hit [C, `FUN_0043c8e2`, `FUN_0043c960`]

**Activation** (using a Triple Hit goodie): Nuclear turns off.
- If Triple was off, it turns on with the burst counter at 0 and the burst-swing counter at 0, and the swing, retract and cooldown state reset.
- If Triple was already on, only the burst counter resets to 0, so the goodie is refreshed.

**Per tick, while Triple is on:**

1. A key edge while idle, with cooldown 0 and burst-swing counter 0, starts a swing.
2. If ((not swinging and cooldown ≠ 0) or not retracting) and burst counter > 6: Triple turns off.
3. If retracting and not swinging and cooldown == 0: `burstSwing += 1`.
   - If `burstSwing < 3`, re-swing at once with jitter:
     - `reach' = max(0, reach − 20 + rand % 40)`.
     - `y = trunc(472 − (1 − reach'/375)·121)`.
     - `x += rand % 40 − 20`.
     - The swing starts with no cooldown.
   - Otherwise: `burstSwing = 0`, `burst += 1`, `cooldown = 1`, and the normal cooldown follows.

Effect: each press gives a burst of 3 swings that jitter in position. Triple ends once more than 6 bursts have completed, which is 7 bursts or 21 swings [C for the counter, I for the exact boundary tick].

### 1.7 Nuclear Hit [C, `FUN_0043cb71`, `FUN_0043cc1a`]

1. Activation with duration `ms` (−1 means 5000 ms; the mystery goodie passes 20000):
   - Triple turns off and Nuclear turns on, and the Nuclear timer is armed.
   - If Nuclear was newly switched on, `swinging` is set, so it swings at once.
2. Per tick while the timer runs, whenever the swatter is retracting and not swinging:
   - It teleports: `reach' = rand % 375`, y comes from `reach'` as in §1.6, and `x = rand % 800`.
   - Cooldown and retract are cleared and `swinging` is set. It swings again with no cooldown.
3. When the timer expires, Nuclear turns off. Ending the mystery effect also turns it off [C, `FUN_0043cd0b`].

Nuclear swings use power 5, damage base 74 and shockwave sparks (§1.3, §1.4).

---

## 2. AI farmer (in-game) [C unless tagged]

### 2.1 Creation

- The AI exists when data+0x23 (number of players) is 1. The human is then the fly [C, `FUN_0043ea40`].
- There is no fly AI. The fly is always human-controlled; only the farmer (and the bonus-round swatter, §8.3) has an AI [C, `FUN_0043ea40`].
- It holds pointers to the farmer, data, fly, objects manager, boot and flying goodies [C, `FUN_0043edb1`].
- Init steps [C, `FUN_0043ee0b`]:
  1. Read the level from data+0x25 (0 Easy, 1 Normal, 2 Intelligent).
  2. Load the parameter table (§2.2).
  3. Create the wander, Triple, Nuclear, frenzy and boot timers.
  4. Point the target controller at the fly.
  5. Roll a fresh aim offset.
  6. Initialise the three goodie budgets (§2.6).

### 2.2 Per-level parameters [C, `FUN_0043f0f4`]

| Name (offset) | Meaning | Easy | Normal | Intelligent |
|---|---|---|---|---|
| alignTol (+0x4c) | px within which an axis counts as aligned | 60 | 40 | 30 |
| aimSpread (+0x50) | max random aim offset per axis, px; = trunc(181·k) | 271 | 181 | 120 |
| aimSpreadCut (+0xcc) | subtracted from aimSpread while Triple is active | 90 | 60 | 51 |
| dwellMs (+0x58) | x aligned this long → swing even if y isn't | 1500 | 1000 | 700 |
| reactBase / reactRand (+0x5c/+0x60) | x-direction change delay = base + rand % rand | 195 / 150 | 130 / 100 | 130 / 100 |
| wanderCheckMs / wanderMs (+0x64/+0x68) | wander check period / wander length | 1000 / 1300 | 1500 / 700 | 2200 / 450 |
| frenzyOffBase / frenzyOffRand (+0x6c/+0x70) | ms | 6000 / 6000 | 5000 / 5000 | 3800 / 4000 |
| frenzyOnBase / frenzyOnRand (+0x74/+0x78) | ms | 2000 / 2000 | 3000 / 3000 | 4000 / 3000 |
| escapeWaitMs (+0xd0) | midge-escape wait | 9000 | 6000 | 4500 |
| escapeMs (+0xd4) | midge-escape length | 2200 | 3500 | 3500 |
| chaseOffMs / chaseOnMs (+0xf4/+0xf8) | goodie-chase check period / chase length | 4500 / 1000 | 3000 / 2000 | 2000 / 1300 |
| tripleIdleMs / tripleArmedMs (+0xd8/+0xdc) | Triple decision periods | 12000 / 4500 | 9000 / 3000 | 7000 / 3000 |
| nukeWaitMs / nukeFarMs (+0xe0/+0xe4) | Nuclear decision periods | 12000 / 3000 | 9000 / 2000 | 7000 / 1200 |
| bootBaseMs / bootLevelK / bootTrapK (+0xe8/+0xec/+0xf0) | boot interval terms (§2.6) | 18000 / 10000 / 4000 | 11000 / 7000 / 4000 | 9000 / 6000 / 2000 |
| pWander (+0x7c) | % per wander check | 19 | 13 | 10 |
| pFrenzy (+0x7d) | % per frenzy check | 17 | 30 | 37 |
| pTriple (+0xfc) | % | 17 | 30 | 40 |
| pNuke (+0xfd) | % | 17 | 30 | 40 |
| pNukeEscape (+0xfe) | extra % for the midge-escape Nuclear | 15 | 20 | 30 |
| pEscape (+0xff) | % for the midge-escape move | 30 | 40 | 39 |
| pBoot (+0x100) | % | 20 | 30 | 40 |
| pChase (+0x101) | % to chase its flying goodie | 20 | 30 | 45 |

### 2.3 Target controller [C, `FUN_0043b4a0`–`FUN_0043bbfe`; I for the exact reference points]

- The controller tracks one target: the fly, a flying goodie, a dummy point, or (bonus round) a coin.
- The **aim point** is the target position plus an offset `(ox, oy)`. Each offset is `±(rand % S)` with an independent random sign [C, `FUN_0043baa4`].
- `S` depends on the state:
  - 700 while wandering.
  - Otherwise `aimSpread`, or `aimSpread − aimSpreadCut` while Triple is active.
  - The offset is re-rolled when the mode changes and after every alignment swing [C, `FUN_0043f8b1`].
- `dx`:
  1. Take the signed distance from the swatter's centre x to the aim point's x, on a ring whose width is passed in when the target is set. For the fly the value is 4392 [C, `FUN_0043b845`].
  2. If the way round the ring is shorter, that path is used and the sign flips.
- `dy` is the aim point's y minus the swatter reference y [I].
- `distance` (used by the Nuclear decision) is the same horizontal ring distance without the offset [C, `FUN_0043b791`].

### 2.4 Per-tick update [C, `FUN_0043f611`]

The update runs only when the farmer and fly exist and the AI is not paused. It pauses outside play [C, `FUN_0043e6d8`].

1. Refresh the controller from the target's current position.
2. **Dwell expiry:** if the dwell timer expired: reset speed, request a swing, destroy the dwell timer.
3. **Wander toggle:** the wander timer is Tick(wanderCheckMs). When it expires, and neither escape nor goodie chase is running:
   - If not wandering, wander with probability `pWander`% and re-arm with `wanderMs`.
   - If wandering, stop and re-arm with `wanderCheckMs`.
   - The aim spread follows: 700 while wandering, else `aimSpread` (or the Triple value).
4. **Midge escape:** this runs only while the escape timer exists (created in §2.5 Nuclear), when that timer expires and neither wandering nor chasing.
   - If not already escaping and `rand % 100 < pEscape`: re-arm with `escapeMs`, set escaping, and aim at a dummy point (farmerX + 90, 430).
   - Otherwise stop escaping and re-target the fly.

   While escaping, the dummy point is updated each tick.
5. **Goodie chase:** the chase timer is Tick(chaseOffMs). When it expires and neither escaping nor wandering:
   - If the farmer's flying goodie exists and is active and `rand % 100 < pChase`: target it, re-arm with `chaseOnMs` and set chasing.
   - When a chase ends: re-arm with `chaseOffMs` and target the fly.

   While chasing, if the goodie disappears, return to the fly.
6. **Frenzy toggle:** the frenzy timer is Tick. When it expires:
   - If frenzy is off, with probability `pFrenzy`% turn it on for `frenzyOnBase + rand % m`. Here `m = 1` when L = 0, else `m = 1 − trunc(frenzyOnRand / L)`.
     - That divisor is negative, so the effective range is `0 .. trunc(frenzyOnRand/L) − 2`.
     - When `trunc(frenzyOnRand/L) = 1`, the modulus is by zero and the original crashes. That cannot happen with the table values [C].
   - If frenzy is on, turn it off for `frenzyOffBase + rand % n`. Here `n = frenzyOffRand` when L = 0, else `trunc(frenzyOffRand / L)`.
   - Note: `L` here is the farmer's bought level (data+0x7c), not the AI level [C, `FUN_0043e885`]. The first Tick arms the frenzy timer with `frenzyOffBase + frenzyOffRand` ms [C].
   - While frenzy is on and Triple is not active, the AI requests a swing every tick.
7. **Alignment** [C, `FUN_0043e306`, `FUN_0043e4e9`]:
   1. Horizontal: if `|dx| > alignTol`, the x direction is `sign(dx)`. Otherwise x is aligned: with `rand % 5 < 2` the direction becomes 0, else the previous one is kept.
   2. Vertical: if `|dy| > alignTol`, the y direction is `sign(dy)`. Otherwise it is 0 and y is aligned.
   3. **Reaction delay:**
      - A change of x direction does not apply at once. It starts the reaction timer at `reactBase + rand % reactRand` ms, and the old key state is held until the timer expires.
      - The y direction applies at once [C for the timer, I for the hold detail].
   4. If both axes are aligned: request a swing, re-roll the aim offset, destroy the dwell timer.
   5. If only x is aligned and no dwell timer exists: create the dwell timer at `dwellMs`, which leads to step 2.
8. **Goodie decisions** (§2.5).
9. Move: call §1.2 with the AI's right, left, up and down flags.
10. Swing: call §1.3 with the swing request, then clear the request.
11. If AI+0x08 is set, zero the damage output [C; the flag's meaning is not identified].

The goodie request (1 Triple, 2 Nuclear, 3 Boot) is collected by the game each tick [C, `FUN_0043f885`] and spent through the farmer goodie routine `FUN_004043c1` (AI path) [C], with the selector set from the request [I].

### 2.5 Goodie decisions, per tick [C, `FUN_0043f930`]

Flags read from the farmer [C, `FUN_004407b0`, `FUN_004407d0`, `FUN_004407f0`]:
- tripleOn = farmer+0xdc
- midgesOn = farmer+0x118
- slowed = farmer+0x11c
- nukeOn = farmer+0xe4

From the objects manager [C, `FUN_00440810`, `FUN_00440830`]:
- flyCarrying [I name], and the type of the carried object.

Steps:

1. When tripleOn falls, re-arm the Triple timer with `tripleIdleMs`.
2. When midgesOn falls while the escape flag is set: clear the flag, end any escape, destroy the escape timer.
3. If tripleOn or nukeOn, stop here.
4. **Triple** (only if data+0x80 > 0):
   - Fly not carrying:
     - If the armed flag is set, re-arm the timer with `tripleIdleMs` and clear the flag.
     - Otherwise, when the timer expires, reason = 2.
   - Fly carrying:
     - If the armed flag is clear, arm with `tripleArmedMs`, set the flag, reason = 1.
     - Otherwise, on expiry, clear the flag and reason = 1.
   - Roll when reason ≠ 0:
     - `p = max(0, pTriple − budgetTriple)`.
     - If reason = 1 and the carried object's price > 70.0, add `trunc(pTriple · 0.3)`.
     - With reason = 1 and price ≤ 70, the original adds an uninitialised value. Port it as +0 [C bug, I fix].
     - If `rand % 100 < p`, request Triple and bump `budgetTriple` (§2.6).
5. **Nuclear** (only if data+0x82 > 0 and nothing requested this tick):
   1. If the Nuclear timer (`nukeWaitMs`) has not expired:
      - Let `d` = distance (§2.3).
      - If `d < 801`, and midgesOn, the escape flag is clear and slowed: reason = 3, set the escape flag, create the escape timer at `escapeWaitMs`.
      - If `d ≥ 801`, the far timer is Tick(nukeFarMs); on expiry, reason = 2.
   2. If the Nuclear timer has expired, reason = 1.
   3. Roll when reason ≠ 0:
      - `p = max(0, pNuke − budgetNuke)`, plus `pNukeEscape` when reason = 3.
      - If `rand % 100 < p`, request Nuclear and bump the budget.
   4. If `d < 800`, destroy the far timer.
6. **Boot** (if the boot is not active, data+0x84 > 0, nothing requested and the boot timer has expired):
   - `p = pBoot − budgetBoot`. If `rand % 100 < p`, request the boot.
   - Then re-arm the boot timer with the interval below, whether or not it was requested.

### 2.6 Boot interval and goodie budgets

**Boot interval** [C, `FUN_00440068`]:

    a = trunc((L − (flyStrength + flySpeed)/2) · bootLevelK / 5), then a = min(a, 0)
    b = trunc(trapCount · bootTrapK / 14)       (14 = caps of trap types 2+3+4: 5+2+7)
    interval = bootBaseMs − a − b;  if interval ≤ 0 then interval = bootBaseMs

`a ≤ 0`, so a farmer level below the fly's average upgrade level delays the boot. More placed traps bring it sooner. The fly's levels are data+0x86 and data+0x88, and trapCount is data+0x2fc.

**Budget bump** [C, `FUN_0044021a`]:
- The bump applies to items 2..4 only: Triple, Nuclear and Boot, whose owned counts are data+0x80, data+0x82 and data+0x84.
- It runs at init and after each request. When `rand` is even and owned > 1:
  - `k = trunc((pItem − 5) / owned)`.
  - If `k > 0`, then `budget += rand % k + 1`.
- Budgets only grow during a period, so each successive request of the same goodie gets less likely [I: whether budgets reset per period was not traced].

---

## 3. AI shopping [C, `FUN_00421b51`, `FUN_00440952`, `FUN_00440b92`, `FUN_00441414`, `FUN_0044151b`, `FUN_00421cbc`]

### 3.1 Plan (computed once on entering the farmer's shop)

Shop item ids for the AI: 1 level, 2 trap tokens, 3 Triple, 4 Nuclear, 5 Boot. Shop button = id − 1, and button 5 is "done".

| Profile byte | Easy | Normal | Intelligent |
|---|---|---|---|
| reservePct | 70 | 80 | 95 |
| reserveJitter | 15 | 10 | 5 |
| tokenPct | 70 | 80 | 85 |
| tokenJitter | 15 | 20 | 5 |
| goodie split Triple/Nuclear/Boot (%) | 34/33/33 | 40/30/30 | 50/25/25 |

[C, `FUN_00440ac9`]

Steps:

1. **Budget** `B` = farmer money (data+0x90) + 42 × kills. The plan counts the pending payout before it is paid.
2. **lastShop** = (period == lunch and days ≤ day). This is the shop before the final dinner.
3. **Levels** (if level < 5):
   - Loop while all of these hold:
     - `B / price ≥ 1.0`
     - `level + planned < 5`
     - this is the first purchase, or `B / price > 1.0` strictly
   - Each pass: `planned += 1`, `B −= price`, `price += 200`.
   - The plan ignores the 999 special case (§7.2).
4. **Reserve** (if `level + planned < 5` and not lastShop). Money is set aside and nothing is bought:
   1. `j = rand % (reserveJitter + (5 if planned > 0 else 0))`.
   2. `sgn = −1` if rand is odd, else +1.
   3. `base = (B · reservePct) / 100` (integer).
   4. `R = base + sgn · (j · base) / 100`. If `B < R`, then `R = base`.
   5. `B −= R`.
5. **Tokens** (if tokens < 14 and 55 < B):
   1. `j = rand % tokenJitter`. Pick `sgn` the same way.
   2. `base = (B · tokenPct) / 100`, or `B` on lastShop.
   3. `spend = max(0, base + sgn · (j · base)/100)`.
   4. If level < 5 and spend > 0: `spend = max(0, spend − trunc(spend/5) · (5 − level))`.
   5. `n = trunc(spend / 55)`.
   6. Clamp: if `n ≥ 14`, `n = 14`. Else if `n + level > 14`, `n = 14 − level`. (The second test uses the farmer *level*, not owned tokens. It is harmless because the purchase rule refuses past the cap.)
   7. `B −= n · 55`.
6. **Goodies** (if any of the prices 30/40/65 is < B): repeat up to 1000 times while at least one is affordable (`B ≥ price`):
   1. `r = rand % 100`. r < T picks Triple, r < T + N picks Nuclear, otherwise Boot.
   2. If the pick is affordable: `B −= price` and its count +1.

   There is no cap check and prices do not rise.

### 3.2 Execution

1. **Order:** a fresh random permutation of ids 1..5 per visit. For slots 1..5, draw `rand % 5` until an unused id comes up [C].
2. **Timing:** the timer is armed at 700 ms, and the AI acts once per expiry with a 700-ms period:
   - Skip ids whose planned count is < 1.
   - Then press that item's button and decrement its planned count, whether or not the purchase succeeds.
3. After the last id, move the cursor to "done". 700 ms later, press it.
4. If the farmer has kills to tally, the AI starts paused and is unpaused when the payout finishes [C, `FUN_00421e6a`, `FUN_0044151b`].

---

## 4. Traps

### 4.1 Types, costs and caps [C, `FUN_0040ad51`-area init, `FUN_00424db1`, `FUN_00425e6b`]

| Type | Asset | Token cost | Cap | Width for spacing (px) | Stored y |
|---|---|---|---|---|---|
| 0 | flypaper (colle_mouche1) | 1 | 7 | 8 | 0 |
| 1 | fan facing left (ventilo_gauche) | 2 | 5, shared with type 2 | 34 | 600 |
| 2 | fan facing right (ventilo_droite) | 2 | shared | 34 | 600 |
| 3 | zapper (tue_mouche) | 4 | 2 | 97 | 0 |
| 4 | flypaper variant (colle_mouche2) | – | 7 | 8 | 0 |

The caps are at data+0x298..0x2a0. Type 4 is not placed by the AI. How a human gets one was not traced [I]. The trap effects are as in game-spec §4.6.

### 4.2 Lifetime [C]

- All slots are cleared when the traps shop opens [C, `FUN_004244e2`], and also when the farmer leaves the goodies shop with 0 tokens [C, `FUN_00421e6a`].
- **Traps therefore last exactly one period.** Unspent tokens carry over. Each placement deducts its cost, with a floor of 0 [C, `FUN_00425456`].
- The three winch slots are re-rolled at the same two points and in extreme mode [C, `FUN_0040bf70`, `FUN_0040b994`].

### 4.3 Coordinates and the placement validator [C, `FUN_0040bd61`, `FUN_0040bf70`]

- The shop's x range is 0..760.
- Stored x = `trunc(x / 0.152)`. 760 maps to 5000, so the stored value is world space [I].
- Checks convert back with `trunc(stored · 0.152)`.

**Placing** type `t` at shop x `x`:
1. For each existing trap `j` (type `tj`, shop position `sj`): reject if `sj − width(t) ≤ x ≤ sj + width(tj)`.
2. For each of the 3 winch slots `w`: reject if `w − width(t) ≤ x ≤ w + 36`.
3. Otherwise store the type, x and y (600 for fans, else 0), increment trapCount, and accept.
4. There is no 15-slot guard. The caps keep the count at 14 or fewer.

Human-shop affordability [C, `FUN_00424db1`]:
- Flypaper needs tokens ≥ 1 and fewer than 7 placed.
- A fan needs `tokens/2 ≥ 1` and fewer than 5 fans.
- A zapper needs `tokens/4 ≥ 1` and fewer than 2 zappers.

**Winch slots** [C, `FUN_0040bf70`]: three draws `rand % 761`. Each must be outside `[prev − gap, prev + gap]` of every earlier draw; on a conflict the draw is redone and all comparisons restart. The loop is unbounded.
- `gap` is 36 at reset and at shop exit. In the traps shop it is the winch icon width.
- Each slot is stored as `trunc(x / 0.152)`.

### 4.4 AI trap placement [C, `FUN_00425a01`, `FUN_00425ae6`, `FUN_00425dc2`, `FUN_00425e6b`, `FUN_00425ba2`, `FUN_00424a6f`]

1. **Gate.** Let `c` = 30 / 50 / 70 by AI level and `tokens` = owned tokens.
   - `threshold = c − trunc(tokens · (c − c/2) / 14)`.
   - Place traps if `rand % 100 > threshold` or on lastShop (§3.1).
   - Otherwise press "done" and keep the tokens.
2. **Plan types:**
   1. Start with `tokens` and per-type counts at 0. Existing traps are not counted, but the slots are empty here anyway (§4.2).
   2. The affordable set is the cheapest prefix of [flypaper 1, fan-left 2, fan-right 2, zapper 4] whose cost ≤ tokens.
   3. Pick uniformly from the affordable set.
   4. If the pick is under its cap (the fans share one), count it and deduct its cost.
   5. Otherwise, if only flypaper is affordable, stop. Otherwise pick again.
   6. Stop when tokens reach 0.

   With 14 tokens at most, the caps cannot all be hit at once, so the retry always ends.
3. **Place**, zappers first, then right fans, left fans and flypapers:
   - For each planned trap, `x = rand % (760 − width)`, validated by §4.3, with up to 200 attempts.
   - If 200 attempts fail, all remaining placements are abandoned.
4. **Replay:** the accepted traps are copied out, the slots are cleared again, and the shop replays them as if typed:
   1. 800 ms after the shop opens, select the trap's button (both fans share button 1).
   2. 500 ms later, press it.
   3. 1000 ms later, the trap is placed, its tokens are deducted and slot 13 plays.
   4. 800 ms later, the next trap is selected.
   5. A final sentinel step presses "done".

---

## 5. Farmer goodies (corrections to game-spec §4.8) [C, `FUN_004043c1`]

- **Selector:**
  - A press edge of the goodie key steps the selector 0 → 1 → 2 → 3 → 0.
  - The use key with selector > 0 uses the selected goodie, if its count is > 0.
  - Afterwards the selector returns to 0 and the swing-edge memory is set to 10 (§1.3).
- **Triple and Nuclear** can be used only while Nuclear is not active (farmer+0xe4 = 0). They can be used while Triple is active, which refreshes it or switches to Nuclear.
- **Boot** can always be used.
- Every use plays slot 37 and puts a stardust effect on the swatter.

---

## 6. Other goodie and hazard rules

### 6.1 Friendly boot [C, `FUN_0043aabe`, `FUN_0043abc7`, `FUN_0043b157`, `FUN_0043734c`, `FUN_004397c0`; motion curve I]

1. **Spawn:**
   - x = `rand % (W − 900) + 810`, where W is the scene width, 4000 [I].
   - y = 100 − bootHeight, mostly off the top of the screen.
   - The boot starts in follow mode with stomp counter 0. Its re-target timer is armed at 1000 ms.
2. **Follow:**
   - Every 1500 + rand % 1000 ms it re-rolls a horizontal offset `rand % (bootW + 100 + flyW)`.
   - Its target is `flyX − (bootW + 50) + offset`, taken around the ring.
   - Horizontal speed toward the target comes from a truncated float expression [I]. Vertically it bobs on a curve of x/10 [I].
   - At random it holds still for 500–999 ms: entered when `rand % 100 > 97`, left early when `rand % 100 ≥ 91`.
3. **Stomp:**
   - When the horizontal distance is < 11 px, it stops.
   - With `rand % 100 > 85` (14%), it stomps: it falls with acceleration 100.
   - At the floor it bounces up with acceleration 150 and spawns an impact at its bottom centre. The boom (slot 6) plays [C, `FUN_0043b370`].
   - Each impact adds 1 to the stomp counter. If the fly is dead at impact, the counter is set to 15 (16 after the increment).
   - Once back at hover height (y < 80 − bootH), it returns to follow mode.
4. **Leave:** when the stomp counter is > 9, it rises at 80 px/s and stops moving sideways. It counts as active while in follow mode or while any part is on screen [C, `FUN_0043b33b`].
5. **Collision with the fly** (only while the fly is alive):
   - The test uses the fly's box inset by 30 px on the left and 25 px on top, against the boot's box, with ring wrap.
   - The fly is pushed out along the axis of smaller overlap, or along both on a tie, so it ends up 20 or 25 px outside the boot's edge.
   - **If it is pushed down** (the boot is above the fly), the fly's y > 249, and the fly is not already splatted, the fly is **splatted**:
     - Same routine as a fan overkill, kind 12.
     - Slot 46 plays and the fly's hit count goes up by 1.
     - The fly drops its object, and SuperFly is cancelled.
   - The fly's y is then clamped to ≤ 600 − flyH.
   - So the boot kills by crushing a low fly [C], and that counts as a kill through the normal death path [I].

### 6.2 Flying goodies spawn [C, `FUN_00432684`]

Each side has a check timer. When it expires:

1. `m = meter(side)` (§8.2). If `rand % 100 ≥ m · 100`, there is no spawn; re-arm with `trunc((1 − m) · 8000) + 8000` ms.
2. Otherwise spawn at (rand % 800, rand % 500):
   - With `rand % 100 < 3` it is a mystery goodie (type 15).
   - Otherwise `type = 3 + rand % N`, with N from the goodie icon sheet's frame count (fly: half the frames − 2) [C formula; N's value is open].
   - If the side is trailing, `t = trunc(m·100 − 50) > 0`; with `rand % 100 ≤ t·100/50`, the type is forced to 3 + rand % 2.
   - So a trailing side gets more Turbo/SuperFly (fly) or Triple/Nuclear (farmer) [I for the farmer mapping].
   - The flyer lives 8000 ms (fly) or 12000 ms (farmer).
   - Re-arm with `trunc((1 − m) · 8000) + 16000` ms (fly) or `+ 20000` ms (farmer).

Catch effects are as in game-spec §4.11.

### 6.3 Midges slow the farmer [C, `FUN_0043cd23`, called from `FUN_00404ef3`]

1. Each tick, `stuck` = midges touching the swatter (`FUN_00442e51`) and `total` = midges spawned in this swarm. `r = stuck / total`, or 0 when total is 0.
2. The pulse uses the farmer timer +0x120:
   - **Not slowed**, timer expired: re-arm `trunc(r · 400 + 150)` ms. If `r ≠ 0`, become slowed with `slowSpeed = trunc(maxSpeedX · (1 − r))`.
   - **Slowed**, timer expired: become unslowed and re-arm `trunc((1 − r) · 500 + 200)` ms.
3. While slowed:
   - `vy = 0`, so the swatter cannot move vertically.
   - Horizontal speed is capped at `slowSpeed`.
   - A swing in its forward phase is forced into retract.
4. When the swarm is gone, both flags clear.
5. Midges that touch a trap get ±180 px/s added to each velocity component, keeping its sign [C, `FUN_00427885`].

---

## 7. Shop, prices and payout

### 7.1 Prices and fluctuation [C, `FUN_0040b005`, `FUN_0040b9d8`, `FUN_0040ba40`]

- Item prices, caps and increments are as in game-spec §5.6.
- Food prices: 22 float base prices (data+0x160..). In extreme mode, a 3-entry table (80, 50, 20) is used instead.
- A type is present in the current period when data+0x1c4[type] ≥ 0. It is set to 0 or −1 per period by the objects manager [C, `FUN_0042ac27`].

**Fluctuate(type)**, valid only for a present type:
1. `old = price`.
2. `sgn = −1` if `rand % 10 < 5`, else +1.
3. `price = old + sgn · ((rand % 30) / 100) · old`, in float32.
4. If `price < 1.0`, set `price = 1.0`.
5. Return `trunc(old)`.

### 7.2 Purchase [C, `FUN_00422223`]

1. The side is the shop's current side (0 farmer, else fly).
2. Refuse (sound 12) if money < price or owned ≥ cap.
3. Otherwise:
   - Deduct the money and play sound 11.
   - Start the conveyor animation. The owned count goes up only when the item reaches the bottom of the animation [C, `FUN_00423047`].
4. The price rises at press time, based on the owned count before the press:
   - Farmer level: when owned == cap − 2 (= 3), the price rises by increment − 1 (+199).
   - Otherwise, when owned < cap − 1, it rises by the increment.
5. Paid sequences:
   - Farmer level: 200, 400, 600, 800, 999.
   - Fly strength and speed: 100, 200, 300, 400, 500.
   - Everything else is flat.
6. "Done" (item 5): a farmer with tokens < 1 re-rolls the winch slots, clears the traps and leaves. With tokens, the traps shop opens.

### 7.3 Payout at the shop [C, `FUN_00421e6a`, `FUN_0042322f`, `FUN_00423be2`]

- **Saved goods:** data+0x1f0[type] counts deliveries this period. It is reset to 0 for present types and −1 for absent ones at the start of each period [C, `FUN_0042ac27`].
- **Fly:**
  - For each type in order: if saved ≥ 0, `value = Fluctuate(type)` (the old price), and the side earns `value × saved`.
  - Types that are present but have nothing saved are still fluctuated.
  - The total is added to fly money (data+0x92) and fly score (data+0x30).
- **Farmer:** 42 per kill (data+0x15c) is added to money (data+0x90) and score (data+0x38). Then kills = 0.
- **Animated tally:** it shows one type, or one kill, per conveyor cycle, and credits as each cycle completes. A key press skips to the instant result. **Both paths produce identical totals**, so the port needs one payout function plus an optional animation.
- After the farmer's payout the AI shops (§3.2).

### 7.4 Bourse [C, `FUN_00420476`]

The bourse page reads prices and uses `rand` for visuals only. It writes no game data.

### 7.5 Delivery [C, `FUN_0042c888`]

When the fly drops an object on the winch:
1. Play sound 11.
2. Increment stats delivered, and add `trunc(price)` to the stats price total.
3. `saved[type] += 1`, and free the object.
4. If no object with stock remains on the table, set the "table cleared" flag, which ends the period early (§9.2).

### 7.6 Kills [C, `FUN_0040bc5e`]

1. `kills += 1` and stats kills += 1.
2. A kill type is stored at list index `kills` (index 0 unused):
   - 3 when the fly took 1 hit.
   - 2 for 2–3 hits.
   - 1 for 4 or more.

The list holds 50 entries and is not bounds-checked [C].

---

## 8. Scoring

### 8.1 Points per event [C]

| Event | Fly | Farmer |
|---|---|---|
| shop payout | money and score += Σ old price × saved | money and score += 42 × kills |
| bonus coin collected | +2 money and score | – |
| bonus target swatted | – | +10 money and score |
| end of match (winner screen) | score += unsold saved value at current prices | score += 42 × untallied kills |

### 8.2 Balance meter [C, `FUN_0040c086`]

    flyValue    = flyScore + Σ over saved > 0 of (price × saved), accumulated as total = trunc(total + price·saved)
    farmerValue = farmerScore + 42 × kills
    diff = clamp(flyValue − farmerValue, −1000, 1000)
    meter(side) = 0.5 if diff == 0
                  else r = (1 − |diff|/1000) / 2, and use 1 − r when (diff > 0 and side == farmer) or (diff < 0 and side == fly)

The trailing side gets the value above 0.5. The meter drives the flying-goodie spawns (§6.2) and the bonus-round handicaps (§8.3).

### 8.3 Bonus round [C, `FUN_004452a1`, `FUN_00446030`, `FUN_004472df`, `FUN_00447cc0`, `FUN_00446993`]

1. **Handicaps:** `h_fly = 1 − meter(fly)` and `h_farmer = 1 − meter(farmer)`. A meter of exactly 0.0 gives h = 0.99, and exactly 1.0 gives h = 0.001. The leader has the larger h.
2. **Intro:** a counter starts at 3 and steps down every 1000 ms. When it goes below 0, play begins and the round timer is armed at **35000 ms** [C].
3. **Fly coins** (20 slots):
   - Every 50 ms, if a slot is free, one coin spawns.
   - Before each spawn, with `rand % 100 < h_fly·8 + 4`, the coin pattern rate is re-rolled:
     - If `h_fly > 0.5`: `b = trunc(58·h_fly − 24)` and `rate = (b + rand % (60 − b)) / 100`.
     - Otherwise: `rate = (1 + rand % trunc(100·h_fly + 10)) / 100`.
     - The initial rate is `(1 + rand % 60)/100`.
   - The rate times a spawn counter feeds a math-library call that shapes where the coin enters [I: likely trigonometric].
   - The fly collects a coin on box overlap. Slot 49 or 50 plays, at 21500 + rand % 500 Hz.
4. **Farmer targets** (10 slots): one spawns every `trunc(h_farmer · 300 + 200)` ms if a slot is free. The bonus swatter hits them on box overlap, and slot 42 plays.
5. **Counting:** coins and hits count only while the 35-s timer has not expired. Every 50th coin plays slot 51, and every 10th hit plays slot 53.
6. **End**, when the timer expires:
   - The AI pauses and both spawners stop.
   - Fly money and score += 2 × coins. Farmer money and score += 10 × hits.
   - 2000 ms later, slot 53 plays if 2·coins < 10·hits, else slot 51.
   - Then the result dialog waits for a key.
7. **Bonus AI** (players == 1) [C, `FUN_004495e0`, `FUN_0044976c`, `FUN_00449923`, `FUN_00449a24`, `FUN_00449bb1`]:
   - It uses the same alignment and reaction machinery as §2.4, with its own parameters (table below).
   - It targets the nearest active farmer target by |Δx|, using the target ring 920.
   - Frenzy:
     - On: with probability `pFrenzy + trunc(pFrenzy · (1 − 2·h_farmer))`, it stays on for `onBase + rand % max(1, trunc(onRand · (1 − h_farmer)))`.
     - Off: `offBase + rand % max(1, trunc(offRand · h_farmer))`.
     - While on, it swings every tick.
   - It swings whenever both axes align, then re-targets.

| Bonus AI | Easy | Normal | Intelligent |
|---|---|---|---|
| alignTol | 30 | 20 | 10 |
| aim spread = bonus swatter width × | 1.0 | 0.5 | 0.35 |
| reactBase / reactRand | 195 / 150 | 130 / 100 | 100 / 70 |
| offBase / offRand | 750 / 1200 | 500 / 1000 | 300 / 600 |
| onBase / onRand | 1500 / 2200 | 2000 / 3000 | 2500 / 4000 |
| pFrenzy | 15 | 30 | 45 |

### 8.4 Winner [C, `FUN_0044aedb`]

On entering the winner screen:
1. Fly score += unsold saved value at current prices, with no fluctuation (`FUN_0040bb54`).
2. Farmer score += 42 × kills (`FUN_0040bbe5`).
3. **The farmer wins only if fly score < farmer score. A tie goes to the fly.**

The displayed final scores include the top-up.

### 8.5 High score [C, `FUN_0040a48a`, `FUN_0040abb0`]

- A score is recorded only with 1 player in high-score mode (`FUN_0040aac0` == 2).
- The fly score after the top-up is inserted at the first of 10 rows whose score is ≤ the new score, so ties rank above older entries.
- No entry is made if no such row exists.

### 8.6 Period clock [C, `FUN_0044214e`, `FUN_004421dd`, `FUN_00405ed0`]

- The limit is minutes × 60 s. Minutes = 3 in tutorial mode (data+0x304), else data+0x26 (2).
- `elapsed = trunc((now − start − pausedTime) / 1000)` s. Pause time is added back.
- The period ends when `elapsed ≥ limit` **or** the table is cleared (§7.5).

### 8.7 Statistics counters [C, `FUN_0040d3e0`, `FUN_0040d4ea`]

All counters are reset per match.

| Counter | Incremented by |
|---|---|
| periods | period advance, not when the match ends (`FUN_0040b7b2`) |
| delivered | delivery |
| available | objects placed on the table per period (`FUN_0042ac27`) |
| price total | trunc(price) per delivery |
| swings | swing frame 1 (`FUN_0043d510`) |
| hits, damage sum | fly enters the hit state (`FUN_00439156`) |
| kills | each kill |
| survival total / max (ms) | fly life timer: starts at spawn, stops at death |

Page values:
- delivered
- delivered / available × 100 (percent) [C, float constant at 0x45debc]
- max survival (s)
- average survival = total / (kills + 1) (s)
- average price = price total / delivered
- swings
- kills
- kills / periods
- hit ratio = hits / swings × 100 (percent)
- average damage = damage / swings

A zero denominator shows 0.

---

## 9. Period, day and match progression

### 9.1 Advance [C, `FUN_0040b7b2`, `FUN_0040b922`, `FUN_0040b94c`, `FUN_0040b994`]

- **Normal:**
  - Period 0 → 1 (breakfast) with day = 1. Then 1 → 2 (lunch) → 4 (dinner) → 1 with day + 1.
  - Inside/outside goes 0 → 1, then alternates 1 ↔ 2.
  - The advance reports "over" when day > days. The stats period counter increments only when not over.
- **Extreme** (2 players and mode 2):
  - From period 0: period = one of {1, 2, 4} by `rand % 3`, day 1, inside/outside = 1 if rand is even else 2.
  - Any later advance reports over, so a match is one period.
  - The winch slots are re-rolled and the trap count set to 0.
- A bonus round follows dinner, and always follows in extreme.
- The match is over after the bonus round when extreme, or when the period was dinner and days < day + 1.

### 9.2 Period end order [C, `FUN_0040326c`]

1. The clock limit or the table-cleared flag sets the period-over flag, and the end banner shows.
2. If a bonus round is due (§9.1), it runs.
3. If the match is not over, the shop follows: payout, then shopping and traps.
4. Otherwise the winner screen, the stats page and, if eligible, the high-score table follow.

---

## Corrections to game-spec.md

1. **§4.6 traps and §5.6:**
   - Traps last one period: all slots are cleared when the traps shop opens and when the farmer leaves with 0 tokens. Unspent tokens carry over.
   - The caps (7 flypaper, 5 fans total, 2 zappers, 7 for the flypaper variant) are [C], not [I].
2. **§4.8:** the Triple Hit duration is not timed. It is a 3-swing burst per press, and it ends after more than 6 bursts. Triple and Nuclear cannot be activated while Nuclear is active. Nuclear auto-teleports and re-swings with no cooldown.
3. **§4.9 midges:** the effect is decoded (§6.3): pulsed slowdown, no vertical movement, and forced retract while slowed.
4. **§4.10 boot:** the boot does kill. Pushing a low fly (y > 249) downward splats it. Otherwise it is a solid obstacle that pushes the fly out. It leaves after 10 stomps.
5. **§4.11 flying goodies:** the spawn probability is the balance meter, and the re-arm delay is `(1 − m)·8000 + 8000/16000/20000` ms. It is not a plain random term.
6. **§4.12 bonus round:**
   - The round length is 35000 ms [C].
   - The handicap is applied through the coin pattern rate (fly) and the target spawn interval `h·300 + 200` ms (farmer).
   - The 1P bonus round has its own AI.
7. **§5.3 scoring:**
   - Payout pays the **old** price, then fluctuates it.
   - Fluctuation runs once per shop payout, for every type present that period.
   - The winner is [C]: the farmer wins only on fly < farmer after an end-of-match top-up of unsold goods and kills.
   - High score ties rank above older entries.
8. **§5.5 balance meter:**
   - Both scores are included.
   - It returns exactly 0.5 at diff 0.
   - The trailing side gets more than 0.5.
9. **§5.6:**
   - The farmer level special case gives the sequence 200/400/600/800/**999**.
   - The owned count increments when the conveyor animation ends, not at the press.
10. **§3 / RNG:** MSVC `rand`, reseeded with `time()` in several constructors, so the game is not replayable.
11. **Swatter hit test:**
    - The radius uses the fixed 130-px hit box (65 + flyW/2), not the sprite width.
    - Damage peaks at 40 + 6·L, or 74 with Nuclear, and lands at most once per swing.
    - Sound 29's 5000 is a volume, not a frequency.
12. **AI (§5.4, open item 6):** decoded in §2–§4.
13. **Statistics:**
    - The swing counter is the stats object, not an AI counter.
    - The period counter does not increment on the final advance.
14. **Period end:** a period also ends early once every table object has been delivered.

## Still open

1. **Key order.** The human farmer's key array order (T/G/F/H → right/left/up/down) in `FUN_00407241` is not confirmed against `FUN_0043cea5`'s parameter order.
2. **Controller details.** The exact reference points for the AI controller's `dy` (`FUN_0043b756`, `FUN_0043bbfe`). The x-direction "hold" encoding during the reaction delay.
3. **Budget lifetime.** Whether AI goodie budgets reset each period.
4. **Flying goodie types.** The value of N, the per-side type range (it needs the frame count of the goodie icon sheet). The exact farmer type mapping.
5. **Math call.** Which libc function `FUN_00450734` is (it shapes the boot bob and the bonus coin entry); likely `sin` or `pow`.
6. **Boot.** Its horizontal speed expression, and whether a boot crush increments kills through the normal death path.
7. **Midges.** The midge–swatter contact box (`FUN_00442e51`).
8. **Trap type 4** (flypaper variant): how a human places it.
9. **Coordinate scale.** What the 0.152 trap/winch x scale means beyond "shop px → stored units".
10. **AI+0x08,** the flag that zeroes AI damage output.
11. **Conveyor double-press.** Whether a second purchase press during the conveyor animation can exceed a cap. The owned count lags the press.
12. **Bonus intro.** Its exact displayed sequence (the counter shows 3..0 before play starts).

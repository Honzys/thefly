# Closed gaps

This document closes the gameplay gaps left open in `game-spec.md`, `fly-and-world.md` and `ai-and-rules.md`. It uses the same conventions:

- [C] means the rule was read in the binary at the cited function. [I] means it is inferred and flagged as such.
- `R(n)` is `rand() % n` with the MSVC generator. `trunc` rounds toward zero. The binary's float-to-int helper always truncates, including for negative values.
- `fps` is the measured frame rate the FPS-based movers use (nominally 65).
- Offsets such as fly+0x2a4 or game+0x407c0 are given only so a reviewer can check them against the dump.

Where this file disagrees with an older document, this file wins (see §10).

---

## 1. Keyboard map [C]

### 1.1 Scancodes

Keys are read through DirectInput by scancode (`FUN_0040f5be` registers them, `FUN_00407241` polls them). Every key uses the same "raw counter" mode:

- The stored value is −1 while the key is up.
- While the key is down it is ≥ 1. The counter goes up by 2 per poll, because the poll reads the device state twice.
- A **press edge** means "now ≥ 1 and the previous stored value < 0".

| Scancode | Key (QWERTY / AZERTY) | App field | Used for |
|---|---|---|---|
| 0xC8 | Up arrow | +0x44 | fly up; menus up |
| 0xD0 | Down arrow | +0x48 | fly down; menus down |
| 0xCD | Right arrow | +0x4c | fly right; menus right |
| 0xCB | Left arrow | +0x50 | fly left; menus left |
| 0x1C | Enter | +0x64 | fly action; menu confirm |
| 0x01 | Esc | +0x6c | back / pause |
| 0x36 | Right Shift | +0x7c | fly goodie cycle |
| 0x14 | T | +0x5c | farmer up |
| 0x21 | F | +0x58 | farmer left |
| 0x22 | G | +0x60 | farmer down |
| 0x23 | H | +0x54 | farmer right |
| 0x0F | Tab | +0x74 | farmer swing |
| 0x10 | Q / A | +0x78 | farmer goodie |

The physical key at scancode 0x10 is "A" on the French AZERTY layout the game was made for.

### 1.2 Gameplay [C, `FUN_00408d5c`, `FUN_0040326c`, `FUN_0043cea5`]

The farmer key order is confirmed twice. The tick passes the farmer keys to `FUN_0043cea5` as above. Separately, the traps shop (`FUN_00408acd`) builds its key array as [T, F, G, H] for a menu widget that expects [Up, Left, Down, Right].

**Farmer (human):**

| Key | Effect per tick |
|---|---|
| T held | vy = max(vy − 200, −400), toward y 351 (the far edge, short reach) |
| G held | vy = min(vy + 200, 400), toward y 472 (the near edge, full reach) |
| F held | vx = max(vx − 400, −cap) |
| H held | vx = min(vx + 400, cap) |
| neither of a pair | that velocity component is 0 |
| Tab | swing, on a press edge only. The previous value is stored at game+0x407cc every tick. Using a farmer goodie writes 10 there, so Tab must be released before the next swing. |
| 0x10 key | farmer goodie select and use (`FUN_004043c1`) |

**Fly (human):**

- The arrows steer (held, not edge).
- Enter is the action (grab, carry). It counts while held, not only on the edge.
  - After a fly goodie is used, game+0x407c8 blocks the action until Enter is released.
- Right Shift cycles the goodie selector.
- Inverted controls (fly mystery goodie) swap Left with Right and Up with Down, in fly physics only.

**Global:**

- While game+0x408ac is set, every key reads as released.
- The tutorial and pause dialogs consume Esc (§1.3).

### 1.3 Menus and screens [C]

**Menu widget** (`FUN_00415599`). The widget receives the keys as [Up, Left, Down, Right], plus Enter and Esc. All movement is on press edges.

- List mode:
  - Uses Left/Right when the widget flag +0x9f8 is 0, otherwise Up/Down.
  - Previous = −1, next = +1. The selection wraps at both ends.
- Grid mode:
  - Up/Down move by row and Left/Right by column.
  - Cells marked −1 or −2 are skipped.
- Enter confirms the item, or cycles a toggle item.
- Esc (edge) goes back one page (`FUN_0041c327(page, −1)`).
- `FUN_0041be05` ignores Enter while the counter +0x2a6e is ≥ 10. This is the input lock right after a page opens.

**Per-screen keys:**

| Screen | Keys |
|---|---|
| Main and option menus | arrows, Enter, Esc |
| Dialogs (`FUN_00411b8f`) | arrows, Enter |
| Goodies shop, farmer's turn (`FUN_00408799`) | T / F / G / H move, Tab confirms, Esc is passed through |
| Goodies shop, fly's turn | arrows move, Enter confirms, Esc is passed through |
| Traps shop (`FUN_00424a6f`) | T / F / G / H, Tab, Esc |
| Bonus round (`FUN_00446030`) | fly: arrows. Bonus swatter: H (right), F (left), T (up), G (down), Tab (swing, edge) |
| Bonus result dialog | arrows, Enter |
| Skip screens (`FUN_0040735a`) | a fresh press of Esc or Enter |

**Name entry** (`FUN_0040f8d9`):

- QWERTY letters a–z and the top-row digits 1..0 are accepted.
- Enter ends the entry and Backspace deletes. Space and Esc are also handled.
- Every other key is ignored.

---

## 2. Splat and respawn timing [C unless marked]

### 2.1 The splat itself [C, `FUN_004397c0`; callers: boot `FUN_0043734c`, fans `FUN_0042a075`]

`FUN_004397c0(minX, minY, rangeX, rangeY, signX, signY, –, g)` launches 60 blood particles, then 11 body pieces. The fly is at (fx, fy) with size w × h.

**Per blood particle,** in this rand order:

1. x = fx + R(w − 20)
2. y = fy + R(h − 20)
3. frame = R(frameCount − 3) + 1, or + 4 for a zapper death (fly+0x2a8)
4. a sign roll R(10). It is drawn even when signX is 0. sx = −1 if it is < 6 and signX is set, else +1.
5. vx = (R(rangeX + 1) + minX) × sx
6. a sign roll R(10). sy = −1 if it is < 4 and signY is set.
7. vy = (R(rangeY + 1) + minY) × sy

Each blood particle gets gravity g, with acceleration on.

**Per body piece:** the same steps without the frame roll. The x sign test is R(10) < 4 instead of < 6. Gravity is g.

C remainder rule: a negative range gives R(n) with n < 0, and `rand % n` equals `rand % |n|`, which is never negative.

**Bursts:**

| Cause | Arguments | vx | vy | g |
|---|---|---|---|---|
| Boot (`FUN_0043734c`) | 0, 0, 280, 200, 1, 0, –, 12 | −280..280 (negative 40% of the time for pieces, 60% for blood) | 0..200 | 12 |
| Fan whose fan+0x918 == 1 (`FUN_0042a075`) | 550, −550, 400, −400, 0, 0, –, 20 | 550..950 | −550..−152 | 20 |
| Other fan | −850, −550, −700, −400, 0, 0, –, 20 | −850..−152 | −550..−152 | 20 |

**Splat side effects:**

- splatted = 1 and hits += 1
- sound 46
- a 200 ms timer at fly+0x13b38

**Zapper death:** a zapper kills through `FUN_00439f29` (arguments 50, 80, 50, 100, 1, 1, 1, 13). It adds 40 fading blood particles and sets fly+0x2a8 = 1.

Body pieces use the FPS mover with gravity. The fly setup (`FUN_00435cd4`) gives each piece:

- frame i + 1 of the 34 × 34 `eclatement_fly` sheet
- acceleration on for both axes; the x acceleration is 0
- the "true gravity" flag. With the flag set, g is always added to v. Without it, g is added in the direction of v, so a rising sprite would speed up.

Each tick:

1. v += g.
2. y += sign(v) × floor(|v| / fps).

Because v changes every tick, the remainder carry of the FPS mover never builds up, so the step is just the floor.

### 2.2 When the splat ends (`FUN_00439c2b`, run from the fly render)

- **Boot or fan:** the splat ends on the first tick on which all 11 pieces have y ≥ 600.
- **Zapper** (fly+0x2a8 set): the splat ends when all 100 blood particles (60 + 40) have faded out.
  - Each fades from alpha 255 to 0 over R(100) + 850 ms.
  - The tween's per-frame step is trunc(255 / (fps × ms / 1000)).
  - At fps 65 that step is 4, so every particle takes 64 ticks. The splat lasts 64 ticks, about 0.98 s.
  - At fps 60 the step is 5 when ms = 850 (51 ticks) and 4 otherwise (64 ticks). The splat lasts 64 ticks, because the slowest particle decides.

**Fall times for boot and fan splats.** These were computed from §2.1 with the integer rule above. They are the number of ticks until the slowest piece passes y 600.

| Case | fps 65 | fps 60 |
|---|---|---|
| Boot, worst case (fly y 250, piece vy 0) | 64 | 61 |
| Boot, fast piece (fly y + 73 = 323, vy 200) | 43 | 41 |
| Boot, fly low (y 506, vy 0) | 35 | 33 |
| Fan, fly at top (y 0), slowest piece (vy −550) | 96 | 94 |
| Fan, fly at y 250, slowest piece | 83 | 81 |
| Fan, fly low (y 506), slowest piece | 64 | 64 |

So a boot splat lasts about 0.5–1.0 s, and a fan splat about 1.0–1.5 s. The engine must simulate the 11 pieces. The duration is not a constant.

### 2.3 After the splat (`FUN_0043857d`, `FUN_00438643`, `FUN_004387d5`)

1. **Same tick:** the end of the splat calls `FUN_0043857d`. This records the kill through the normal kill path (`FUN_0040bc5e`) and requests a respawn.
   - So **a boot or fan splat counts as a kill** for the farmer.
2. **Next tick,** tick step 9: the fly position is re-rolled (at most 1000 tries, `fly-and-world.md` §3.7), then the appear animation starts (`FUN_00438643(1)`).
3. **Appear animation:**
   - a 150 × 150 sprite (2 × 1 frames) at the fly centre
   - scale tweens from 3.0 to 0.1 over 500 ms, and alpha from 0 to 255 over 500 ms
   - at fps 65 the scale tween takes ceil(2.9 / (2.9 / 32.5)) = 33 ticks; the alpha step is trunc(255 / 32.5) = 7, which reaches 255 in 37 ticks
   - [I] The fly becomes alive when the tween reports done. That is 33–37 ticks, about 0.5–0.57 s.
4. **Alive** (`FUN_004387d5`): the appear tween ending makes the fly alive and plays sound 38.

**Total splat-to-control time** = fall or fade time + 1 tick + appear time. At 65 fps:

- boot: about 1.1–1.6 s
- fan: about 1.5–2.0 s
- zapper: about 1.5 s

The dying path (swatter damage ≥ 101) is different, and is already specified in `fly-and-world.md` §3.6 D: the crash animation, then a 1500 ms timer, then the kill and the respawn request.

---

## 3. Stack push and object-on-object physics [C, `FUN_0042e28a`]

The object–object step (`FUN_0042ec96`), the per-tick motion (`FUN_0042eb13`) and the bounce are in `fly-and-world.md` §4. This section adds the missing push formula.

**Inputs:** the pushed object P, a mode (1 = slide into, 2 = drop onto, 3 = carried into), the pusher's vx and vy (px/s), and the pusher's weight m (the pusher's obj+0x90).

**Steps:**

1. **Riders first.** For each object Q in P's resting-on-top list (obj+0x86c, front to back):
   - add Q's weight to P's total M (M starts at P's own weight)
   - if Q is collectible (obj+0x92 > 0), apply this whole procedure to Q, recursively, with the same mode, vx, vy and m
   - so each object in a stack is pushed using its own weight plus the weights of its direct riders, depth first
2. **Scale.** In modes 2 and 3: vx = trunc(vx / 1.5) and vy = trunc(vy / 1.5).
3. **Mean speed.** s = (|vx| + |vy|) / 2, in integer division.
4. **Energy.** E = |trunc(m × s × s)|.
5. **Speed.** q = E / (2 × M), in integer division; speed = sqrt(q).
6. If speed ≤ 50, nothing changes.
7. Otherwise P's state = 1 (sliding), and:
   - pusher vx > 0: P.vx = trunc(speed)
   - pusher vx < 0: P.vx = −trunc(speed)
   - pusher vx = 0, mode 2: draw R(10). If it is < 5, P.vx = −trunc(speed), otherwise +trunc(speed).
   - pusher vx = 0, other modes: P's state = 0 and P.vx = 0

The sign test reads the vx left by step 2. So in modes 2 and 3, a pusher with |vx| < 2 counts as vx = 0. In mode 2 it takes the R(10) branch.

**RNG note:** in mode 2 with vx = 0, each object in the stack whose speed exceeds 50 draws one `rand`, in depth-first order.

**Callers:**

- mode 1: a sliding object hits another one (`FUN_0042ec96`)
- mode 2: a falling object lands on another one (`FUN_0042ec96`)
- mode 3: the fly carries an object into another one (`FUN_0042f3b1`)

Each caller pushes only collectible targets (obj+0x92 > 0).

**Friction and bounce, re-confirmed:**

- Friction per tick is trunc(weight / 3.0) (`FUN_0042eb13`, constant 3.0).
- The pusher's rebound after a mode-1 push is ±10 × trunc(weight / 3) − vx. It stops at the sign flip (`FUN_0042ec96`).

---

## 4. Camera and parallax [C, `FUN_004416b1`, `FUN_00441baf`, `FUN_00403e36`]

### 4.1 Who moves the camera

- Only the fly's edge-scroll moves the camera (`fly-and-world.md` §3.4 step 11).
- The fly is always human (there is no fly AI), so the camera always follows the fly. It never follows the farmer.
- The per-tick delta D (game+0x407c0, a double) is assigned each tick by fly physics (`FUN_00437547`). It is reset to 0 at period start.
- The farmer, objects, traps, the boot and the flyers subtract D (scaled by their parallax) from their own x. Nothing else writes D.

### 4.2 Layer table

Four slots, set up per scene by `FUN_004416b1`. "House" means the indoor scene; "outside" and "arena" are the other two.

| Slot | Image (house / outside / arena) | Parallax f | Screen y (house / outside / arena) | Initial accumulator | Colour key |
|---|---|---|---|---|---|
| 0 sky | `paysage` / `paysage2` / `paysage2` (arena copy) | 0.25 | 0 | 0 | no |
| 1 hills | `Jardin` / `collines` / `collines` (arena copy) | 0.75 house, else 0.5 | 205 / 280 / 280 | 50 | yes |
| 2 main | `maison` / `dehors` / `arene` | 1.0 | 0 / 230 / 123 | 0 | yes |
| 3 foreground | `table` / `table2` / `table2` | 1.25 | 525 | 0 | yes |

- Image widths from the assets: `maison`, `dehors` and `arene` are 4000; `collines` 2000; `paysage2` 1000; `table` and `table2` 900; `paysage` and `Jardin` 800.
- **W = the main layer image width = 4000.** The scene's world width (game+0x1a) is that image's width. W × 1.25 = 5000 is the objects' wrap.
- The hills layer is the one that starts at 50.

### 4.3 Per-frame layer update

Layers are drawn in two calls. Slots 0–2 are drawn before the sprites; slot 3 is drawn after the objects and the fly. Each slot is updated only when it is drawn, which is once per tick.

For each drawn slot, with image width Wi, height Hi, factor f and accumulator A:

1. A += trunc(D).
2. src = trunc(A × f).
3. Wrap right: if src ≥ Wi, then A = trunc((src − Wi) / f) and src = trunc(A × f).
4. Wrap left: else, if A < 0, then A = trunc(Wi / f − |A|) and src = trunc(A × f).
5. The visible height is Hi, cut so that y + height ≤ 600.
6. Blit:
   - If src > Wi − 800: draw image [src, Wi) at screen x 0, then image [0, 800 − (Wi − src)) at screen x Wi − src.
   - Otherwise: draw image [src, src + 800) at screen x 0.

**Notes:**

- D is truncated before it is added. Edge-scroll deltas below 1 px per tick do not move the backgrounds at all, while sprites may still move. Reproduce this.
- Because of the truncations, the wrap is not an exact modulo. Use the steps as written.

---

## 5. Swatter catch of the farmer's flying goodie [C, `FUN_0043309b`, call site in the tick function `FUN_0040326c`, step 16, `FUN_00406020`]

### 5.1 Gate and box

**Gate:** the farmer can catch only while the swatter is in the active part of its swing and is not blocked:

- the swing frame (farmer+0x8c, 0..4, `FUN_0043d510`) is ≥ 3
- and the "cannot hit" mystery flag (farmer+0x154) is 0

**Swatter box,** in screen coordinates:

| | Value |
|---|---|
| x | farmer x (farmer+0x90) |
| y | trunc(reach), where reach is farmer+0xb8, 0..375 (`ai-and-rules.md` §1.2 step 8) |
| w | swatter image width − 50 = 181 − 50 = 131 (the width of `tapette-1`, stored at farmer+0x74) |
| h | 154 |

**Overlap:** with the flyer's screen x gx, y gy and frame size gw × gh, the test is gx ∈ [x − gw, x + w] and gy ∈ [y − gh, y + h], bounds inclusive (`FUN_00431e58`).

**Fly box,** for comparison: (fly x + 50, fly y + 40, fly w − 100, fly h − 80), with no gate.

### 5.2 Tests and effects

- Each side can catch only its own flyer.
  - The fly-side flyer (sub-object +0x1c8) is tested against the fly box.
  - The farmer-side flyer (+0x18) is tested against the swatter box.
- Both are tested every tick, in that order.
- Flying goodie kinds: `fly-and-world.md` §6.2 is correct. N = 3 normal kinds per side (3, 4, 5), plus the mystery kind 15.
  - The fly-side kinds are Turbo, SuperFly and Midges.
  - The farmer-side kinds are Triple, Nuclear and Boot.
  - One flyer per side can exist at a time.
- **Small difference** (`FUN_00432684`) in the "trailing side" re-roll (`fly-and-world.md` §6.2 step 4.2):
  - The fly side forces kind 3 or 4 when R(100) ≤ 2q.
  - The farmer side forces it when R(100) < 2q, strictly less.

---

## 6. AI budgets and AI+0x08 [C]

### 6.1 Budgets reset every period

- The gameplay object, which includes the farmer AI, is created on entering the gameplay state 0x32 (`FUN_00408c2a` → `FUN_004015c8` → `FUN_0043ea40`, `FUN_0043edb1`, `FUN_0043ee0b`).
- It is destroyed in state 0x3b (`FUN_00408f09`).
- Every period passes through 0x32: the price board's Enter leads to 0x32 (`game-spec.md` §2).
- So the AI's level parameters, timers and all three goodie budgets are re-initialised at the start of each period.
- Budgets grow only within a period.

### 6.2 AI+0x08 is always 0

- The only store to AI+0x08 is the base-class constructor (`FUN_0043e100`), which writes 0.
- No other function stores to that field. This was checked across the AI class and every use of game+0x14e48. The setter `FUN_00440937` belongs to the shop AI, which is a different object.
- So the "zero the damage output" step (`ai-and-rules.md` §2.4 step 11) never fires.
- An engine may omit it.
- [I] An aliased write cannot be ruled out completely by static reading, but none was found.

---

## 7. Boot and the math function [C]

### 7.1 The math function is `sin`

- `FUN_00450734` is the C runtime `sin`. It uses the x87 sine instruction with argument reduction, and its error-handler name string is "sin".
- `FUN_004507e4` is `cos`.
- Both take radians.

### 7.2 Boot

- **Hover** (`FUN_0043abc7`): y = trunc(base + sin(trunc(screenX / 10)) × 5.0). The argument is an integer.
  - screenX is the boot's screen x (sprite +0xc).
  - base is the hover y, 80 − h.
  - This confirms `fly-and-world.md` §7.2 step 8.
- **Horizontal speed:** `fly-and-world.md` §7.2 steps 3–6 (speed = trunc(dist × 2000 / W + 250)) is confirmed. W = 4000 (§4.2).
- **Kill:** a boot stomp that splats the fly records a kill (§2.3).

### 7.3 Bonus coin entry (`FUN_004472df`, `FUN_00447cc0`, setup `FUN_004452a1`)

**Spawning:**

- A spawn timer fires every 50 ms while spawning is enabled (+0x2768).
- On each firing:
  - the counter n (+0x2764, 16-bit) goes up by 1
  - at most one coin is spawned, into the first free slot of 20

**Coin entry,** for each spawned coin:

1. **Change the wave.** With probability R(100) < 8 × h + 4, re-roll the wave frequency ω (`FUN_00447cc0`).
2. **Spawn the coin:**
   - frame R(frameCount)
   - x = 810, just off the right edge of the screen
   - y = trunc((sin(n × ω) + 1) × A) + bgY + coinH
   - lifetime 25000 ms
   - animation delay R(30) + 15
   - velocity 0 at spawn

**Terms:**

- h = 1 − balance(fly) (`FUN_0040c086(side 1)`). The setup clamps it to 0.99 or 0.001 at the extremes.
- **Wave frequency ω** = (R(c) + lo) / 100:
  - If h ≤ 0.5: lo = 1 and c = trunc(100h + 10).
  - If h > 0.5: lo = trunc(((h − 0.5) / 0.5) × 29 + 5) and c = 60 − lo.
  - At setup, ω = (R(60) + 1) / 100.
- **Amplitude** A = trunc((bgH − 2.5 × coinH) / 2). It is set once at setup. bgH and bgY are the bonus backdrop (`bckbonusfond`) sprite's height and y; coinH is the coin frame height (45).

[I] How the coins then move left is driven by the coins' own mover settings in `FUN_004452a1` (parallax 1.25, wrap 4000), which were not re-read here.

---

## 8. Midges and flypaper type 4 [C]

### 8.1 Midge contact with the swatter (`FUN_00404ef3`, `FUN_00442bb3`, `FUN_00442e51`)

The goodie midges (game+0x178a0) and the ambient midges (game+0x15a20) use the same box and gate as the flyer catch (§5.1): (farmer x, trunc(reach), 131, 154), active only when the swing frame is ≥ 3 and the farmer is not blocked.

**Overlap test:** midge screen x mx and y my, frame w × h:

- mx ∈ [bx − w, bx + bw]
- my ∈ [by − h, by + bh]
- bounds inclusive

**Midge contact routine** (`FUN_00442bb3`). It takes the gate, the swatter box and the zapper result. In order:

1. **Respawn.** The midge respawns when either of these holds:
   - it is in the hit state, its 700 ms timer has expired, and it is not falling
   - it is falling and its y is greater than the world height

   A respawn clears the hit and falling states, restores flight, draws x = R(500) + 1000 and y = R(H − h) + 20 (two `rand` calls), and returns 0.
2. **Swatted.** Otherwise, if the gate is open and the midge overlaps the swatter box, it enters the hit state:
   - motion stops
   - the 700 ms timer restarts, so a midge held under the swatter stays stunned
3. **Zapped.** Otherwise, if the zapper result is set, the midge starts falling: falling state +0x204, y acceleration 30.
4. **Return value.** The routine returns the hit-state flag. A midge stunned on an earlier tick still returns 1, even with no new overlap.

**Per tick, goodie midges** (`FUN_00404ef3`):

1. **Retarget roll.** Draw R(10) once. If it is < 3, retargeting is on for this tick.
2. **Per live midge,** in list order:
   1. While the counter k < 20:
      - the zapper test (`FUN_004278f3` → `FUN_00428b47`) at the midge's (x, y + 7); a touch adds 1 to k
      - the contact routine above; a return of 1 adds 1 to k
   2. Once k reaches 20, the remaining midges skip both tests for the rest of the tick. They also skip the respawn check.
      - k counts zapper touches plus midges in the hit state, whether new or from an earlier tick. It is not a count of new hits.
      - So stunned midges at the front of the list use up slots for their 700 ms. This is the "up to 20" in `fly-and-world.md` §8.2.
   3. **Retarget,** if on. Four `rand` calls, in this order: tolerance A = R(30), tolerance B = R(30), y offset = R(farmerH − 60), x offset = R(181). The new homing point is (x offset, y offset) with tolerances (B, A).
   4. **Slowdown box.** Each live midge is tested against (farmer x, farmer y (farmer+0x94), 181, farmer frame-0 height). This is the full swatter width, with no −50. Each hit adds to the slowdown count.
   5. **Fans:** a midge in a fan gets each velocity component pushed 180 further from zero.

The ambient midges (game+0x15a20) use the same contact routine and box.

### 8.2 Flypaper type 4 is cosmetic, but uses `rand`

- No player places a type 4 trap.
  - The human traps shop places only types 0 (flypaper), 1 or 2 (fan) and 3 (zapper) (`FUN_00424c83`, `FUN_00425456`).
  - The AI plans only those types too.
  - The only store into the trap type list (`FUN_0040bd61`) takes the caller's type.
- **Fan direction for a human:** the fan starts as type 1 (facing left).
  - Moving the placement cursor right turns it into type 2 (facing right). Moving it left turns it back into type 1 (`FUN_0042570b`).
  - So the fan faces the way the cursor last moved.
  - **Cursor speed:** each held tick adds ±1 to a signed speed capped at ±10, and x += speed. Releasing both keys resets the speed to 0. x is clamped to [20, 780 − w].
- **Variant flypaper:** at period start the trap manager setup (`FUN_0042699d`, called from the gameplay object init `FUN_004015c8`) walks the stored trap list in order.
  - Caps: at most 7 flypapers, 5 fans and 2 zappers are built. Traps past a cap are skipped.
  - Each flypaper (stored type 0) built while fewer than 7 exist draws R(10).
  - If the draw is > 4 (a 50% chance), it uses the `colle_mouche2` sprite (the "type 4" asset). Otherwise it uses `colle_mouche1`.
  - The behaviour is identical. The stored type stays 0, and the token cost of type 0 is 1.
- **This is one `rand` call per built flypaper per period.** An engine must make it to stay in RNG step with the original.

---

## 9. Other simulation gaps closed [C]

1. **AI controller dy** (`FUN_0043b756`, `FUN_0043bbfe`, `FUN_0043b7bb`, `FUN_0043b814`):
   - The swatter reference y is trunc(reach + 154 / 2) = trunc(reach + 77). 154 is farmer+0xb0.
   - The target reference y is the target's top y. With the "centre" flag it is top y + h / 2 (integer).
   - dy = targetRefY − swatterRefY for the distance test. The aim version uses the centre plus the aim offset oy.
   - The ring tag 4392 (0x1128) selects the in-game swatter. 920 (0x398) selects the bonus-round swatter, which uses its own fields.
2. **Conveyor double press** (`FUN_00421b51` area, `FUN_00422223`): while the conveyor animation runs (shop state +0x2308 == 1), a confirm does not buy anything. So the cap cannot be exceeded by pressing twice.
   - [I] Whether a confirm pressed during the animation is latched and applied afterwards was not traced.

---

## 10. Corrections to older documents

| Document | Claim | Correction |
|---|---|---|
| `screens-and-rendering.md` §4.2 | the layer accumulator adds round(delta); srcX = round(acc × factor) | Both are **truncations** (§4.3). The binary's float-to-int helper sets the x87 rounding mode to chop. |
| `ai-and-rules.md` §1.3 (still open 1) | key order unconfirmed | T = up, G = down, F = left, H = right (§1.2) |
| `ai-and-rules.md` §2.6 | budget lifetime [I] | budgets reset every period (§6.1) |
| `ai-and-rules.md` §4.1 | type 4 "not traced" | cosmetic flypaper variant chosen by R(10) > 4 (§8.2) |
| `fly-and-world.md` §6.3 | farmer catch flag open | gate and box in §5.1 |
| `fly-and-world.md` Still open 1–4 | stack push, camera, farmer catch, splat duration open | §3, §4, §5, §2 |
| `fly-and-world.md` §8.2 | swatter hits up to 20 midges per tick | the 20 slots count zapper touches and midges in the hit state, including ones stunned earlier (§8.1) |

---

## 11. Still open

These items were not resolved in this pass. Each has a best guess.

1. **AI x-direction hold** (`ai-and-rules.md` Still open 2, second half). The encoding of the x-direction hold during the reaction delay was not traced. [I] Keep the last chosen x direction until the delay expires.
2. **Outline quirk** (`FUN_0042f6de`, `fly-and-world.md` Still open 5). Not re-read. [I] Implement the quirk as the existing doc describes. Do not "fix" it.
3. **Price statistic** (`FUN_0042d8a0`, `fly-and-world.md` Still open 6). Not decoded beyond "+ trunc(price)". [I] Accumulate trunc(price) per delivery.
4. **The 0.152 scale** (`ai-and-rules.md` Still open 9). [I] It converts shop-screen x to world x for traps and the winch. It is a plain scale factor with no gameplay rule behind it.
5. **Bonus round sequence** (`FUN_004452a1`, `FUN_00446993`, `FUN_00446030`). Only the coin entry and the catch boxes are covered (§7.3). [I] The coin leftward motion comes from the coin mover settings in `FUN_004452a1`.
6. **Conveyor latch** (§9.2). [I] A confirm pressed during the conveyor animation is dropped, not queued.
7. **Appear end** (§2.3). [I] The fly becomes alive when the scale tween ends (33 ticks at 65 fps), not the alpha tween (37 ticks).

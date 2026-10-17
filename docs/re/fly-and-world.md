# The Fly 1.2: fly, physics and world rules (phase 1c)

This document refines `game-spec.md` §4 for the parts an engine needs to reproduce frame for frame: the RNG, the tick order, the fly, collision, table objects, the lift, flying goodies, the friendly boot and the midges. It follows the conventions of `game-spec.md`:

- [C] means the deciding code was read; [I] means it was inferred.
- `FUN_`/`DAT_` addresses refer to the local dump.
- `game+`, `data+` and `app+` are the same objects as in the spec.

It contains no decompiled code. Object-local offsets (`fly+0x…`, `obj+0x…`, `lift+0x…`) are given only so a reviewer can check them; the engine does not need them.

**Number rules used throughout** [C]:

- `trunc(x)` is conversion toward zero (the compiler's float-to-int helper).
- `a / b` on integers is C integer division, which also truncates toward zero.
- `a % b` on integers is C remainder; it takes the sign of `a`.
- Float literals marked `f` are single precision; everything else is computed in x87 double or extended precision.
- `0.1f` is exactly 0.100000001490116.

---

## 1. RNG and seeding

**Generator** [C, `FUN_0044f260`, `FUN_0044f250`]: the MSVC C runtime generator.

- state = state × 214013 + 2531011 (mod 2³²)
- result = (state >> 16) & 0x7FFF, so values lie in 0..32767
- One global state (at 0x468e2c) serves the whole game.

**Seeding** [C]: srand(time()), where time() gives wall-clock seconds (`FUN_004504f0`), runs at each of these points:

| Where | Function |
|---|---|
| every sprite or effect constructor | `FUN_00416eb0` (many instances are built when the game object is constructed) |
| object placement, normal mode | `FUN_0042ac27` |
| object placement, extreme mode | `FUN_0042bdf8` |
| lift init | `FUN_00430e48` |
| fly init | `FUN_00436df6` |

**Consequences:**

- The stream restarts from wall-clock seconds at each period start.
- Bit-exact replay needs the seed and the exact call order.
- A reimplementation that wants determinism should seed once per period from a recorded value and keep the same call order. The engine does not need to reproduce the repeated reseeding unless replays of real sessions are a goal [I].

**Idioms used below** [C]:

| Name | Definition |
|---|---|
| `R(N)` | rand() % N |
| `U` | rand() / 32767.0, a double in [0, 1] |
| sign roll | a fresh rand(); negative when trunc(U × 6) < 3, i.e. when rand ≤ 16383 |

---

## 2. Tick order (`FUN_0040326c`) [C]

**Inputs.** The caller (`FUN_00408d5c`) passes:

- fly keys: Up, Left, Down, Right, Enter
- farmer keys: T, F, G, H, Tab
- goodie keys: Right Shift and scancode 0x10
- dt: the measured frame time in seconds (app+0x88)
- FPS (app+0xb8)

If game+0x408ac is set, every key reads as released.

**Steps, once per 65 Hz tick:**

1. **Fly action.** action = Enter is down AND game+0x407c8 < 0.
   - game+0x407c8 becomes −1 when Enter is up.
   - It is set to 10 after a goodie use or an unpause.
   - So the action is "Enter held". After a goodie use it stays blocked until Enter is released.
2. **Farmer edge flags.** The same scheme, for the farmer's swing and Tab.
3. **Fly goodie** (`FUN_00404198`).
   - A fresh Right Shift press advances the selector 0 → 1 → 2 → 3 → 0.
   - If the selector > 0 and Enter is down, the selected goodie is used when its count is above 0: 1 Turbo, 2 SuperFly, 3 Midges.
   - On a successful use: a stardust effect at the fly centre and sound 38.
   - In every case: the selector returns to 0, game+0x407c8 = 10, and the fly action is cancelled for this tick.
4. **Farmer goodie** (`FUN_004043c1`).
5. **Fly physics** (`FUN_00437547`, §3.4).
   - Arguments: right, left, up, down, dt, and the camera delta (game+0x407c0).
   - While inverted controls are active (game+0x40898 == 1), the keys are swapped: left ↔ right and up ↔ down.
6. **Farmer.**
   - Human: move (`FUN_0043cea5`), then the hit test (`FUN_0043d1a0`), which yields this tick's damage to the fly.
   - Otherwise: the AI.
7. **Boot.** If the boot is no longer active (`FUN_0043b33b`), clear game+0x3df58. If it is active and the game is not paused, update it (§7).
8. **Fly update** (`FUN_00433ddd`, §3.5), with the UN-inverted keys, the damage from step 6 and the action flag.
9. **Respawn placement.** If the fly's respawn request (fly+0x2d0, read and cleared) is set:
   - re-roll the fly position (`FUN_004385c6`) while it overlaps an object (`FUN_0042d459`), at most 1000 tries
   - then start the appear animation (`FUN_00438643` with 1)
10. **Boot collision**, only while the fly is alive.
    - Test the box (flyX + 30, flyY + 25, flyW − 30, flyH − 25) against the boot (`FUN_0043b157`).
    - On a hit, push the fly (`FUN_0043734c`, §7.3).
11. **Fly not alive.** If the fly is not alive (`FUN_0043885d` ≠ 0): both goodie selectors = 0 and the inverted-controls flag = 0.
12. **Table objects** (`FUN_0042c888`, §5.5).
    - When every object has been delivered (`FUN_004060b0`), the end-of-period flag game+0x407b8 is set.
    - The period clock (`FUN_004421dd`) can also end the period.
13. **Traps** (`FUN_00427270`).
14. **Ambient midges.** Each ambient midge (game+0x15a20, count game+0x17898, stride 0x208) is updated with the swatter's data and the fan push.
15. **Mystery expiry.**
    - Fly side (game+0x40898): when its 15000 ms timer has run out, end Turbo if the flag is 2, then clear the flag.
    - Farmer side (game+0x40899): 1 → the swatter can hit again; 2 → end Nuclear.
16. **Flying goodies.** Spawn (`FUN_00432684`), update (`FUN_00432dfb`), then catch (`FUN_0043309b`), all in §6.
17. **Ambient sounds, farmer voice, tutorial** (`FUN_00441b3c`, `FUN_0043c730`, `FUN_00443772`).

**Render-driven state** [C]. After the update, the render pass (`FUN_00403e36`) advances two animations that change gameplay state:

- the appear animation finishing makes the fly alive (`FUN_004387d5`)
- the splat particles finishing request a respawn (`FUN_0043857d`)

Both therefore complete during a render, between update ticks.

---

## 3. The fly (game+0x40)

### 3.1 Coordinates [C]

- Fly x and y are **screen** pixels: the top-left corner of the frame, with 0 ≤ x ≤ 800 − w and 0 ≤ y ≤ 600 − h.
- The fly never leaves the screen. Moving near a screen edge scrolls the camera instead (§3.4, step 11).
- The frame is w × h; the face strips have 10 frames of 120 × 94 [I, from the asset size].

### 3.2 Fields that matter for gameplay

| Field | Offset | Meaning |
|---|---|---|
| x, y | +0x1c4, +0x1cc | screen position [C] |
| previous x, y | +0x1d4, +0x1d8 | saved before each move [C] |
| vx, vy | +0x1dc, +0x1e0 | integer px/s [C] |
| maxX, maxY | +0x204, +0x208 | 550 + 60 × speed level [C] |
| scroll | +0x20c | camera scroll speed, integer px/s [C] |
| turbo active, timer | +0x210, +0x218 | [C] |
| boostX, boostY | +0x248, +0x24a | turbo boost, 16-bit [C] |
| SuperFly flag, timer | +0x24c, +0x250 | [C] |
| strength factor f | +0x280 | 0.2 × strength level, a float [C] |
| dying, stunned, splatted | +0x288, +0x2a0, +0x2a4 | [C] |
| hit in progress, phase, hit frame | +0x2ac, +0x2b0, +0x284 | [C] |
| damage total | +0x2cc | [C] |
| hits this life | +0x2d4 | [C] |
| ricochet counter, kick x/y | +0x310, +0x2d8, +0x2dc | [C] |
| aspirating, previous action, carrying | +0x324, +0x328, +0x32c | [C] |
| grab-animation lock, counter | +0x330, +0x318 | [C] |
| drop pop | +0x334 | [C] |
| respawn request | +0x2d0 | [C] |
| appearing | +0x13cd8 | [C] |

### 3.3 Init (`FUN_00436df6(strength, speed)`) [C]

1. srand(time()).
2. x = R(800 − 2w) + w.
3. y = trunc((600 − h) / 2.0).
4. maxX = maxY = 550 + 60 × speed.
5. f = strength × 0.2.

### 3.4 Physics (`FUN_00437547`), skipped while appearing [C]

1. **SuperFly tick.** While its timer runs, keep its visual effect. When it expires: flag = 0 and stop sound 36.
2. **Turbo tick**, only while turbo is active.
   - If the timer has expired: switch to the decay phase and do nothing else this step.
   - In the decay phase, every tick:
     - boostX = trunc(boostX − maxX × 0.05)
     - boostY = trunc(boostY − maxY × 0.05)
     - a boost below 0 becomes 0
     - turbo ends when both boosts are 0
3. **Effective maxima.** MX = maxX + boostX; MY = maxY + boostY.
4. **Climb step.**
   - Not carrying: climb = trunc(MY × 0.1f).
   - Carrying:
     - c = f, plus 0.5 with SuperFly; c is capped at 1.0
     - climb = trunc(75.0 × c + MY × 0.1f / 1.6)
5. **Normal steering**, when not dying and the ricochet counter is 0:
   - Right held: vx = trunc(vx + MX × 0.1f), capped at MX.
   - Left held: vx = trunc(vx − MX × 0.1f), floored at −MX.
   - Up held: vy = vy − climb, floored at −MY.
   - Down held: vy = trunc(vy + MY × 0.1f), capped at MY.
   - If the drop-pop flag is set: vy = −(|vy| + 300), then clear the flag.
   - No horizontal key held: vx moves 9 toward 0 without overshooting. The same applies to vy when no vertical key is held.
6. **Ricochet**, when the ricochet counter > 0:
   - Decelerations:
     - dx = kickX / 25, or trunc(kickX / 37.5) while Left or Right is held
     - dy = kickY / 25
   - Increment the counter first.
   - While the new counter is below 25: move vx toward 0 by dx and vy toward 0 by dy. The kick sets the counter to 1, so this gives 23 deceleration ticks.
   - When the counter reaches 25:
     - counter = 0
     - if x + w > 800 − w/2, then x = 800 − w/2 − w
     - else if x < w/2, then x = w/2
   - While dying with the counter at 0, no steering or friction runs at all.
7. **Idle jitter.** Only when the heading is FACE and the fly is not dying, not ricocheting and not carrying.
   - jx = trunc(U × 2), negated on a sign roll. Then jy the same way. That is four rand() calls in total.
   - So jx and jy are 0 or ±1, and ±2 only when rand() returns 32767.
   - A clamp against "previous + 2" exists but can never trigger.
8. **Move.**
   - Save the previous position.
   - x += trunc((vx − scroll) × dt) + jx
   - y += trunc(vy × dt) + jy
   - `scroll` is the value from the previous tick's step 11.
9. **Clamp.** x to [0, 800 − w], y to [0, 600 − h].
10. **Buzz.** If alive, update the positional buzz (slot 7, `FUN_0040d005` with x, vx, vy, w and −700); otherwise stop slot 7.
11. **Edge scroll.**
    - Right edge, when x + w ≥ 800 − w:
      - o = x + w − (800 − w)
      - camera delta = MX × o × dt / (w/2)
      - scroll = trunc(MX × o / (w/2))
    - Left edge, else when x ≤ w: the same with o = x − w, which is negative.
    - Otherwise: delta = 0 and scroll = 0.
    - The camera delta (game+0x407c0, a double) is **assigned** each tick, not accumulated. It is the world shift for this tick in fly-layer pixels.

### 3.5 Update (`FUN_00433ddd`) [C]

Runs only if the fly is not splatted and not appearing.

1. **Dying with the respawn timer armed.** When the timer expires:
   - record the kill from this life's hit count (`FUN_0040bc5e`) and the stats
   - hits = 0
   - request a respawn (`FUN_004385c6`, §3.7)
   - disarm the timer
   - reset the 60 particles
2. **Damage path.** Else, if damage ≥ 1, or dying, or stunned: run the damage machine (§3.6). If dying, aspirating = 0.
3. **Normal path.** Otherwise:
   - **Grab toggle,** on a fresh action (action now, not on the previous tick):
     - not aspirating → sound 41 and aspirating = 1
     - aspirating and carrying → aspirating = 0 (release)
     - aspirating and not carrying → nothing
   - Previous action = action.
   - If aspirating, not locked and not carrying: lock = 1.
   - **Grab animation**, while locked. The frame runs 0..9 with loop frames 4..9.
     - Turning in: frame++. Past 4, hold 4 and the turn-in ends.
     - Else, not turning out: frame++ (past 9 wraps to 4) and counter++. Past 50, start turning out.
     - Turning out: frame−−. Below 0:
       - frame = 0 and heading = FACE
       - counter = 0 and lock = 0
       - aspirating = 0 unless carrying
     - So an unsuccessful grab lasts 5 + 51 + 10 ticks, after which aspirating drops [I, from counting].
   - **Heading animation**, when not locked. It is visual only.
     - Keys R, L, U, D choose a target heading: E, W, N, S or a diagonal (NE, SE, NW, SW). No key means FACE.
     - A turn-in plays frames 0 → 4, then the loop frames 4..9 cycle.
     - Changing to a heading that shares no direction flag first turns out (frames back to 0); a heading that shares a flag switches directly at frame 4.
     - With no key held and the heading not FACE, the fly turns out to FACE. At FACE, frames cycle 0..9.

**Sprite choice** [C, `FUN_004388ca`]:

| Fly state | Strip |
|---|---|
| FACE | Face |
| E, W, N, S | transi-* strip for that direction |
| NE, SE, NW, SW | transi-* strip for that diagonal |
| grab lock | Aspi |
| stunned | crash-face frame (hit frame 0..3), centred |
| dying | crash animation |

### 3.6 Damage, stun and death (`FUN_00437e99`) [C]

Skipped while splatted. Damage that arrives while a hit is in progress is ignored.

**A. New hit** (damage ≥ 1, no hit in progress):

1. Save the heading flags and add 1 to hits.
2. effective = damage, or trunc(damage / 2.5) with SuperFly. Add it to the total.
3. If the total ≤ 100: stunned.
4. Otherwise dying:
   - start the crash animation at the fly's position
   - clear grab, lock, counter and carrying
   - turbo off, then one turbo tick
   - SuperFly off and stop sound 36
   - the crash falls toward (850, 850) (`FUN_0043a158`)
   - set sound 29's volume to +5000 (`FUN_0040cea9` is SetVolume, see §9), then play it. DirectSound volume runs −10000..0, so +5000 is out of range; the call most likely fails and the volume stays unchanged [I]
5. k = damage / 50, an integer from the raw damage. Hit frame = 0; hit in progress = 1.

**B. Stunned, phase 0.** Each tick, hit frame += 1. When it passes 3: hit frame = 3 and phase = 1. On that tick, unless SuperFly is active (SuperFly cancels the knockback):

1. For each axis, x then y: r = U.
   - If v == 0: kick = trunc(r × 500).
   - Otherwise: kick = trunc(|v| × k × r + 800). rand() is called before the absolute value is taken.
2. Apply the kick by the swatter's direction code for that axis:
   - 0 → v −= kick
   - 1 → v += kick
   - 2 → a sign roll picks the sign, then add
3. kickX = |vx| and kickY = |vy| (after the kick).
4. Ricochet counter = 1, sound 28.

**C. Stunned, phase 1.** Each tick, hit frame −= 1. When it drops below 0:

- hit frame = 0, stunned = 0, hit in progress = 0
- restore the heading flags
- set the post-stun flag (fly+0x314)
- phase = 0

The ricochet (§3.4, step 6) runs independently of B and C.

**D. Dying, phase 0, crash animation finished:**

- hit frame = 3, v = 0, phase = 1
- arm the respawn timer for 1500 ms; when it expires, §3.5 step 1 runs

**Splat** (`FUN_004397c0`, from the boot or a fan). A splat is a different death from dying:

- splatted = 1 and hits += 1
- v = 0
- grab and carrying cleared
- Turbo and SuperFly off
- sound 46 and 60 + 11 particles

While splatted, the fly update does nothing. When the particles finish (render, `FUN_00439c2b`), the kill is recorded and a respawn is requested (`FUN_0043857d`) [C].

### 3.7 Respawn [C]

1. **Request** (`FUN_004385c6`):
   - respawn request = 1, v = 0
   - x = R(800 − w)
   - y = trunc((600 − h) / 2.0 − 20)
2. **Placement:** tick step 9 re-rolls while the fly overlaps an object (`FUN_0042d459`), at most 1000 tries. At period start (`FUN_004015c8`) the cap is 3000.
3. **Reset** (`FUN_00438643`): clears hit, stun, damage total, splat, grab, carrying, Turbo and SuperFly.
   - With argument 0, the fly is alive immediately.
   - With argument 1, appearing = 1 and the appear animation plays at the fly centre.
4. **Alive** (`FUN_004387d5`): clears phase, dying, splatted and appearing; sound 38; starts the survival-time statistic.

Life state (`FUN_0043885d`): 0 alive, 1 dying or appearing, 2 splatted.

### 3.8 Turbo and SuperFly [C]

**Turbo** (`FUN_00436fc1(ms)`, where −1 means 1500):

1. Arm the timer and the flag.
2. boostX = boostY = trunc(maxX × 0.5). Both use maxX.
3. Kick on the dominant axis:
   - if |vx| < |vy|: vy += boostY when vy ≥ 0, else vy −= boostY
   - otherwise: vx gets the same treatment with boostX
4. Sound 35.

Decay after expiry is in §3.4, step 2.

**SuperFly** (`FUN_0043728f`):

- flag on, 8000 ms timer, visual effect, looping sound 36
- effects:
  - swatter damage ÷ 2.5, truncated
  - no knockback kick
  - c + 0.5 in the carrying climb
  - smaller fan damage (§7.4)

---

## 4. Coordinates and collision

### 4.1 Layers and the camera [C]

The common sprite base (`FUN_00416eb0`, `FUN_00417c8f`, `FUN_00417fab`) has these fields:

| Field | Offset | Meaning |
|---|---|---|
| layer x | +4 | |
| y | +8 | |
| screen x | +0xc | trunc(layerX × parallax), wrapped |
| parallax | +0x28 | float |
| wrap width | +0x2c | trunc(W × parallax), in screen units |
| wrap flag | +0x30 | |
| size | +0x54/+0x56 | w × h |

**Integrators:**

- **Time-based** (`FUN_00417db6(dt, delta)`):
  - layerX += trunc(vx × dt − delta)
  - y += trunc(vy × dt)
  - layer x wraps modulo wrapWidth / parallax
  - screen x is recomputed
- **FPS-based** (`FUN_004180af(fps, delta)`):
  - moves exactly |v| px over each run of `fps` frames: the quotient each frame, plus 1 on frames where the accumulated remainder share exceeds the base share
  - the run restarts when v changes or the frame count reaches `fps`
  - then x += trunc(−delta)

**Constants:**

- W = the scene background width (game+0x1a, scene field +6); world height = game+0x18 [C].
- Table objects, the lift and the boot use parallax 1.25, so they shift 1.25 × delta screen pixels per tick of camera delta [C for objects (`FUN_0042a85a` passes 1.25)].

### 4.2 Outlines (`.bor`) and the origin [C]

Each object carries 500 row lists (obj+0x9c onward), loaded from its `.bor` by `FUN_0042e040`:

- **Row index** = y relative to the sprite's top edge.
- **Values** = outline x positions in **2-pixel units** relative to the sprite's left edge. This matches `docs/formats/bor.md`: values run 1..229, while the widest object (toaster, grip x 234) is about 460 px wide.
- The loader pushes each value onto the front of a list, so the stored order is the reverse of the file order (ascending x). The order does not affect hit results.

**Fly outline** [C, `FUN_0042d67c`]. The fly has no `.bor`. The manager builds a fixed rectangle outline (manager+0x24) in the same units:

- rows 29 and 84 hold every value 15..45 (top and bottom edges)
- rows 30..83 hold 15 and 45 (the sides)
- so the fly's solid box is pixels x 30..90 by y 29..84 of its 120 × 94 frame [I for the pixel conversion]

Mask-derived outlines (`FUN_0042dda4`, edge pixels at half width) are used for the zapper (`FUN_00428540`), not the fly.

### 4.3 Outline overlap test (`FUN_0042f6de`) [C]

The test takes object A, with its own position and outline, and sprite B, given as position (x, y), size and outline.

1. **Bounding boxes** first. When either box straddles A's wrap width, any x below 800 gets + wrapWidth before comparing. No box overlap means no hit.
2. **Rows.** For every row index both sprites cover (in world rows):
   - hit if some value a in A's row and some value b in B's row satisfy a + Ax / 2 == b + Bx / 2, using integer division of the screen x
   - so outlines collide only where an outline point of one coincides with an outline point of the other at half-x resolution
   - an object fully inside another, with no edge crossing, does not register
3. Quirk: in the case where one sprite's vertical span contains the other's, both passes use the original y argument rather than the shifted one [I; it reads like a bug, so reproduce it only if it changes outcomes].

### 4.4 Fly–object contact (`FUN_0043076b`) [C]

Runs each tick for each visible object that is not falling (state ≠ 2), not carried and not on the lift (§5.5, step 2). The fly's outline is the rectangle from §4.2.

1. If the outlines do not overlap, go to step 6.
2. **Push direction.**
   - Fly vx < 0 and vy ≤ 0 → push left.
   - Fly vx > 0 and vy ≤ 0 → push right.
   - Otherwise there is no push.
3. **Blocking.** Shift the object 1 px in the push direction and test it against every other object's outline. Another object blocks only if its rank (obj+0x94, the placement table's rank column) is greater than this object's.
4. **Push.** If there is a push direction, nothing rests on the object and nothing blocks:
   - v = flyVx / max(1, weight / 10)
   - if |v| > 99 and the object is collectible: object state = sliding, object vx = v
5. **Separate.**
   1. Restore the object's and the fly's previous positions.
   2. While they still overlap, at most 1000 times: move the fly 1 px in x against its vx and re-test. If still overlapping, move it 1 px in y against its vy.
   3. Then fly vx = −trunc(vx / 1.4) and vy = −trunc(vy / 1.4).
6. **Grab candidate.** Return "candidate" when all of these hold:
   - flyX in [objX′ − flyW, objX′ + objW], where objX′ is the screen x, minus wrapWidth when x ≥ wrapWidth − w
   - flyY in [objY − 250, objY + gripY)
   - nothing rests on the object
   - the object is collectible

### 4.5 Respawn overlap (`FUN_0042d459`) [C]

The fly's inner box (x + 50, y + 40, w − 100, h − 80) is tested against each visible object's bounding box. This is a box test, not an outline test.

---

## 5. Table objects (manager game+0x3df60, instances at 0x49bd20, stride 0x874)

### 5.1 Type table [C, `FUN_0042ac27`, type order `FUN_0040ae51`]

Columns:

- **grip x, grip y:** the point the fly's bottom-centre locks onto, relative to the sprite's top-left, in px.
- **weight:** obj+0x90.
- **rank:** obj+0x94; used for blocking in §4.4.
- **meal:** bits 1 breakfast, 2 lunch, 4 dinner. A type is eligible when meal & period ≠ 0.
- **stock:** the maximum copies per period.

| # | Type | grip x | grip y | weight | rank | meal | stock | price (spec) |
|---|---|---|---|---|---|---|---|---|
| 0 | can1 | 74 | 20 | 75 | 3 | 4 | 2 | 60 |
| 1 | can2 | 111 | 20 | 35 | 3 | 4 | 3 | 15 |
| 2 | can3 | 64 | 20 | 35 | 3 | 4 | 3 | 12 |
| 3 | chips1 | 75 | 20 | 15 | 2 | 4 | 3 | 5 |
| 4 | chips2 | 75 | 20 | 15 | 2 | 4 | 3 | 5 |
| 5 | clope | 91 | 20 | 18 | 1 | 7 | 2 | 22 |
| 6 | saucisson | 150 | 40 | 100 | 5 | 4 | 1 | 90 |
| 7 | saucisson1 | 70 | 20 | 20 | 2 | 6 | 3 | 20 |
| 8 | cacahuete | 60 | 20 | 15 | 1 | 4 | 6 | 10 |
| 9 | zippo | 88 | 50 | 60 | 3 | 4 | 1 | 100 |
| 10 | lait | 95 | 20 | 110 | 3 | 1 | 1 | 60 |
| 11 | mandarine | 85 | 20 | 45 | 3 | 1 | 5 | 21 |
| 12 | pomme | 100 | 20 | 60 | 3 | 1 | 4 | 43 |
| 13 | sucre | 48 | 20 | 25 | 3 | 1 | 5 | 30 |
| 14 | toaster | 234 | 20 | 130 | 3 | 1 | 1 | 100 |
| 15 | toast | 117 | 20 | 30 | 3 | 1 | 6 | 18 |
| 16 | poulet | 120 | 20 | 75 | 3 | 2 | 3 | 60 |
| 17 | poivriere | 58 | 20 | 60 | 3 | 2 | 2 | 10 |
| 18 | tomate | 88 | 20 | 60 | 3 | 2 | 4 | 30 |
| 19 | muffin | 82 | 20 | 25 | 3 | 2 | 4 | 40 |
| 20 | cornichon | 66 | 20 | 25 | 3 | 2 | 6 | 8 |
| 21 | mais | 203 | 20 | 90 | 3 | 2 | 2 | 65 |

**Extreme mode** (data+0x28 set, `FUN_0042bdf8`) uses only these three [C]:

| Type | grip x | grip y | weight | rank | meal | stock | price |
|---|---|---|---|---|---|---|---|
| extreme1 | 137 | 20 | 135 | 3 | – | 3 | 80 |
| extreme2 | 107 | 20 | 100 | 2 | – | 5 | 50 |
| extreme3 | 74 | 20 | 70 | 1 | – | 7 | 20 |

Notes:

- A further column of y-like values (569, 576, 593, 545, 575, 595) was not seen in use [C that it exists, I that it is unused].
- "Food value" is the per-type price (`FUN_0040b9d8`): data+0x160[type] as a float, or data+0x1b8[type] in extreme mode. It is the base price after fluctuation; see the spec §4.4 [C].

### 5.2 Placement at period start (`FUN_0042ac27`) [C]

1. For each type: if eligible for this meal, set its delivered count (data+0x1f0) and placed count (data+0x1c4) to 0; otherwise set both to −1.
2. srand(time()).
3. Repeat until 15 objects are placed or 2000 placement failures have accumulated:
   1. Pick t = R(22) until t is eligible with stock ≥ 1, at most 5001 tries.
   2. Load (or share) its sprite. y = 580 − h. Record the type and add 1 to its placed count.
   3. Up to 1000 tries for x:
      - x = R(trunc(W × 1.25)), in screen units
      - reject if [x − wNew, x + wNew] overlaps any placed object's [xj, xj + wj] (inclusive)
      - reject if x + wNew reaches the limit
      - reject if the trap check (`FUN_00427bb4`) refuses the span
   4. On success: load the outline and parameters, place the object, stock −= 1, collectible = 1, landing sound 6.
   5. On failure: undo the slot and count one failure.
4. Each fan stand becomes an extra, non-collectible object with the piedventi outline (`FUN_0042a85a`).

The spec's "three x positions in 0..760 at least 36 apart" (`FUN_0040bf70`) are the **lift anchors** (§5.6), not object positions.

### 5.3 Object fields and states [C]

| Field | Offset |
|---|---|
| screen x | +0xc |
| y | +8 |
| rest y | +0x1c |
| vx, vy | +0x20, +0x24 |
| active | +0x60 |
| landing flag (read and cleared; a dust puff of size weight × 20 / 130) | +0x70 |
| held | +0x74 |
| state | +0x78 |
| drop vx | +0x7c |
| supported-by list | +0x88 |
| resting-on-top list | +0x86c |
| grip x, grip y | +0x8c, +0x8e |
| weight | +0x90 |
| collectible | +0x92 |
| rank | +0x94 |
| on-lift flag | +0x98 |

| State | Meaning |
|---|---|
| 0 | at rest |
| 1 | sliding (pushed) |
| 2 | falling |
| held | +0x74 = 1; follows the fly (§5.4) |

### 5.4 Object motion [C]

**Per tick, for an active object that is not held** (`FUN_0042eb13`, then `FUN_0042ea74`):

1. **By state:**
   - State 0: vy = 0.
   - State 1: vy = 0. Friction fr = trunc(weight / 3.0); vx moves toward 0 by fr. At 0, state = 0.
   - State 2: vx += drop vx; vy += 50.
2. If the object is not collectible: v = 0.
3. Integrate with the time-based integrator (dt and the camera delta).
4. In state 0, save the position as the previous position.
5. Clamp vx to ±500.

**Object–object interaction**, only for collectible objects with weight > 0 (`FUN_0042ec96`):

- **Sliding into another object:**
  - push it, propagating through its stack (`FUN_0042e28a`, below)
  - bounce: vx = ±10 × trunc(weight / 3) − vx, stopping when the sign flips
  - the side test (`FUN_0042fb1c`) then zeroes or reflects v
  - if the object has no support underneath and is free: state = 2, drop vx = trunc(vx / 50)
- **Falling, no collision:** when y ≥ rest y:
  - y = rest y, state = 1, v = 0
  - landing flag = 1, which gives the dust puff and landing sound 6
- **Falling into another object:** side tests with up to 10 retries.
  - Landing on top: append to both stack lists, state = 1.
  - Otherwise: push the other object, reverse drop vx and vx, vy = 0.

**Stack push** (`FUN_0042e28a`): moves the whole stack resting on the pushed object, using summed weights and a square root. Its exact formula is open (see Still open).

**Fan push** (`FUN_0042754d`) applies to collectible objects, including the carried one.

### 5.5 Manager tick (`FUN_0042c888`) [C]

1. **Per active object.**
   - Landing dust.
   - Build the lists:
     - visible
     - on screen: 0 ≤ x ≤ wrapWidth, excluding the carried object
     - collectible
   - Collectible objects with weight > 0 run the object–object interaction and the fan push.
   - Objects that are not held run the motion in §5.4.
2. **Fly contact.** For each visible object that is not falling, not carried and not on the lift, run §4.4. Collect the candidates only when the manager is not carrying.
3. **Grab state.** grab = fly aspirating, forced to 0 when the fly is not carrying but the manager thinks it is.
4. **Unlock.** grab off and a target locked → unlock.
5. **Lock.** grab on and nothing locked or carried. For each candidate:
   - d = Euclidean distance from the fly's bottom-centre (x + w/2, y + h) to the object's grip point (objX′ + grip x, objY + grip y)
   - if d < 1000.0, lock this object
   - the LAST candidate in list order that qualifies wins
6. **Attach,** with grab on and a target locked (`FUN_0042e704`):
   1. If not collectible: fail.
   2. cx = flyX + flyW / 2 (integer); tol = trunc(objW / 2.0).
   3. **Attach test:** |cx − (objX′ + grip x)| < tol AND objY < flyY + flyH.
      - Pass:
        - set objY = flyY + flyH − (grip y + 14) and test for object–object collision (`FUN_0042fe10`)
        - no collision: detach from any support, held = 1, state = 0, success
        - collision: restore y; side test; push = weight × 100 / 20; vx = ±push away from the side; state = 1; fail
   4. **Magnet,** else when cx is within ±100 of the grip x (inclusive):
      - d = |flyX − gripXabs| + 10. This uses the fly's left edge, not the centre.
      - fly vx += d if cx < grip x, else vx −= d
      - fly vy += |flyY − (objY + grip y)| / 2
   - On success: manager carrying = 1, carried = target, fly carrying = 1.
7. **Drop,** with grab off while carrying:
   - object: held = 0, state = 2, drop vx = flyVx / 50, v = 0, back on the visible list
   - fly: drop pop = 1 and y −= 20 (the pop applies vy = −(|vy| + 300) on the next physics tick); carrying = 0
   - carried = none (1000)
8. **Carry,** with grab on while carrying (`FUN_0042f3b1`), then the fan push, then fly vy += weight:
   1. Save the object's previous position. cx = trunc(flyW / 2.0 + flyX); off = grip y + 14.
   2. Object screen x = cx − grip x. If ≤ 0, x = wrapWidth − |x|.
   3. Object y = flyY + flyH − off; object v = fly v.
   4. **Table:** no collision and object y ≥ rest y:
      - flyY = restY − flyH + off
      - fly vy = trunc(−(vy / 1.7) − (weight + 20))
      - landing flag when it moved more than 5 px
   5. **Another object k:** collision with k:
      - push k (mode 3) if collectible
      - flyX = trunc(objX′ + grip x − flyW / 2.0)
      - flyY = objY − flyH + off
      - fly vx = −trunc(vx / 1.4)
      - fly vy = trunc(−(vy / 1.4) − (weight + 20))
9. **Lift** (§5.6; `FUN_00431115` with the objects and the carried index). The result is one of:
   - **Delivered (−1):**
     - sound 11
     - delivered statistic + 1 and price statistic + trunc(price)
     - data+0x1f0[type] += 1
     - the object becomes inactive and the lift slot is cleared
     - "all delivered" = no active collectible object remains
   - **None (1000):** lift loaded = 0.
   - **An index:** lift loaded = 1; that object's state = 0 and v = 0.

Scoring: the fly's score and money rise at the shop payout by Σ delivered × price (spec §5.3). Delivery itself only counts.

### 5.6 Lift (`DAT_0049bba0`) [C]

The lift is a hook on a rope that comes down to meet a carried object and hoists it off the top of the screen. `FUN_0042a760` is the manager destructor (it stops sound 47), not lift logic.

**Anchors** (data+0x2fe, three values, `FUN_0040bf70(760, 36, 0.152)`):

- draw v = R(760) three times, rejecting and redrawing until each pair differs by more than 36
- store trunc(v / 0.152): positions on the minimap scale of 0.152, so x runs 0..5000 in lift-layer units

**Fields:**

| Field | Offset |
|---|---|
| anchor index | +0x6c |
| state: 1 idle, 2 tracking, 3 hoisting | +0x6e |
| waiting object | +0x70 |
| hooked object | +0x72 |
| approaching | +0x74 |
| moved this tick | +0x78 |
| counter, period | +0x80, +0x84 |
| vy | +0x88 |
| read and write indices of a 60-entry y ring | +0x8c, +0x8e |
| max y = rope bitmap height | +0x90 |
| hook offset `off` (constructor argument) | +0x94 |

**Reset** (`FUN_00430fe7`; init `FUN_00430e48` calls srand(time()) first):

- anchor = R(3), x = anchors[anchor], y = −h
- vy = R(150) + 50
- ring cleared, state 1, counter 0, period 102
- rope sprite x = x + 114

**Move** (`FUN_00431826`, all states):

1. y += trunc(vy × dt).
2. If y ≥ max: y = max and vy = −trunc(vy / 1.5).
3. In state 1, y < 1 → y = 0.

**Update** (`FUN_00431115(objects, carried)`). First the transitions:

1. **Fly not carrying, state ≠ 3:**
   - approaching = 0
   - a waiting object becomes the hooked object, and waiting = none
   - if the hooked object's effective state is below 2 (a held object counts as 3), the fly dropped it short: hooked = none, state 1. Otherwise state 2.
2. **Fly carrying:**
   - waiting and hooked both empty: waiting = carried, state 2, approaching = 1, counter = 0
   - else, if waiting is set: state 2
3. **State 3** with hooked = none: reset.

Then by state:

- **State 1 (idle bob):**
  1. counter += 1.
  2. If counter > period:
     - period = R(51) × 2
     - vy = R(250) + 50, or 0 if that is below 100
     - counter = 0
     - if R(10) < 5, vy = −vy
  3. Accelerate: vy > 0 → vy += 7; vy < 0 → vy += 3, stopping at 0.
  4. Move.
- **State 2 (tracking the object):**
  1. target = objY + objH − off + 30, clamped to [0, max], pushed into the ring.
  2. Not approaching: counter = period + 5.
  3. Approaching, with bottom = objY + objH:
     - vy = +450 if y + off < bottom
     - vy = −450 if y + off ≥ bottom + 50
     - move without acceleration
     - if bottom < y + off < bottom + 70: approaching = 0, moved = 0, read = write − 1
  4. **Hook test,** only when hooked ≠ none. All of these must hold:
     - objX ≥ liftX − (objW − 35)
     - objX + 35 ≤ liftX + liftW
     - bottom < y + off < bottom + 30
  5. On a hook:
     - state 3, vy = −300, move
     - objY = y + off − objH + 10, on-lift flag = 1
     - return the hooked index
- **State 3 (hoisting):**
  1. vy = −300, move, and carry the object along.
  2. When y + h < 0: hooked = none and return −1 (delivered).

**Follow** (`FUN_00431931`, after the update):

1. Integrate the lift and rope sprites with the camera.
2. In state 2 when not approaching: if |write − read| == 30, read += 1 (mod 60).
3. If the lift did not move this tick: y = ring[read], so the hook trails the object's height by 30 ticks.
4. Rope y = y − rope height.
5. **Winch sound 47:**
   - stopped while vy == 0
   - otherwise positional, with frequency trunc(vy / 400 × 3000) + 18000 Hz

The other two anchors are drawn as idle hooks at y = 50 − h.

**In practice** [I]: carry an object under the hook column, let go, and the hook (tracking 30 ticks behind) must be within its 30 px window under the object's bottom while the object overlaps the hook horizontally with 35 px of margin.

---

## 6. Flying goodies (game+0x15000)

**Functions:** spawn `FUN_00432684`, launch `FUN_00431c64`, update `FUN_00432dfb`, catch `FUN_0043309b`.

### 6.1 Luck (`FUN_0040c086(side)`) [C]

1. A = data+0x30. Then, for each type t with delivered count data+0x1f0[t] > 0: A = trunc(A + count × price(t)).
2. B = data+0x38 + data+0x15c × data+0x21c. The spec reads this as the farmer base plus 42 × kills; the meaning of data+0x21c is [I].
3. d = clamp(A − B, −1000, 1000).
4. If d == 0: p = 0.5. Otherwise:
   - p_fly = (1 − |d| / 1000) / 2 when d > 0, and 1 − that when d < 0
   - p_farmer is the mirror image
   - so the side that is behind gets the higher p

### 6.2 Spawn [C]

**Timers:**

- initial: farmer (+0x378) first, then fly (+0x3a8); each R(7000) + 8000 ms
- each side has one goodie slot; a new spawn replaces the active one

**When a side's timer fires:**

1. p = luck(side); roll = R(100).
2. **No spawn,** when p × 100 ≤ roll: re-arm trunc((1 − p) × 8000) + 8000 ms and stop.
3. **Mystery,** when R(100) < 3: kind 15, mystery-sheet frame 1 (fly) or 2 (farmer).
4. **Normal,** otherwise. With n = the goodies sheet's frame count = 5 × 2 = 10 [C, the 51 × 50 frame layout passed to the constructor `FUN_004321f8`, frame count computed by `FUN_00414093`] and m = trunc(n/2 − 2) = 3:
   1. kind = R(m) + 3, giving 3, 4 or 5.
   2. q = trunc(p × 100 − 50). If q > 0 and R(100) ≤ q × 100 / 50: kind = R(2) + 3. A side that is far behind gets Turbo or SuperFly instead of Midges or Boot more often.
   3. Fly frame = kind; farmer frame = n/2 + kind.
5. **Launch** at x = R(800), y = R(500), screen space.
   - Wander mover: re-roll every R(500) ms; target speed (R(30) + 65) × direction on each axis; same-direction counts 8 (x) and 6 (y); acceleration 5.
   - Bounds:
     - right: x ≤ wrapWidth + 100 − w
     - left: not enforced (see below)
     - vertical: y in [0, 500 − h]
   - Lifetime 8000 ms (fly) or 12000 ms (farmer).
   - Spawn effect and sound 42.
6. Re-arm trunc((1 − p) × 8000) + 16000 ms (fly) or + 20000 ms (farmer).

**Wander mover** (`FUN_004172a8`, `FUN_004174c0`) [C]:

- each re-roll picks a new target velocity per axis and a direction run
- the speed magnitude ramps toward the target by the acceleration each tick
- bug: the left x bound is combined with an "or" that is always true, so it never applies

### 6.3 Update and catch [C]

**Update:** run the wander mover, then x += trunc(−delta) so the goodie scrolls with the fly layer.

**Catch test:**

- Fly box: (x + 50, y + 40, w − 100, h − 80).
- The farmer catches with the swatter's box, and only when a swatter flag passed in is set (Still open).
- Hit when goodie x ∈ [px − gw, px + pw] and goodie y ∈ [py − gh, py + ph].

**Catch results:**

| Catcher | Kind | Effect |
|---|---|---|
| fly | below 10 | data+0x84 + 2 × kind += 1: kind 3 → Turbo (data+0x8a), 4 → SuperFly (0x8c), 5 → Midges (0x8e) |
| farmer | below 10 | data+0x7a + 2 × kind += 1: kind 3 → Triple (data+0x80), 4 → Nuclear (0x82), 5 → Boot (0x84) |
| fly | mystery | R(100) < 50 → inverted controls (result 1); else Turbo for 25000 ms (result 2) |
| farmer | mystery | R(100) < 50 → swatter cannot hit (result 0x0b); else Nuclear for 20000 ms (result 0x0c) |

- A mystery arms that side's 15000 ms timer. Turbo from a mystery is cut short when that timer ends (tick step 15), so it effectively lasts 15 s plus the decay.
- Sound 42 stops on a catch.

---

## 7. Friendly boot (game+0x3dc60)

**Functions:** spawn `FUN_0043aabe`, update `FUN_0043abc7`, hit `FUN_0043b157`, stomp effect `FUN_0043b370`.

### 7.1 Spawn [C]

- bounds: x in [−1000, 9000], y in [−h, 570]
- 1000 ms retarget timer
- x = R(W − 900) + 810, where W is the boot's wrap width; y = 100 − h
- v = 0, chasing = 1, stomp count = 0

### 7.2 Update [C]

1. **Retire.** If the stomp count > 9 and chasing:
   - chasing = 0
   - vy = −80, y acceleration 80, vertical motion on
   - vx = 0, horizontal motion off
   - the boot rises away
2. **Retarget.** target = flyX − (bootW + 50) + offset. When the timer fires: re-arm R(1000) + 1500 ms and offset = R(bootW + 100 + flyW).
3. **Wrap-aware distance.** q = (bootX − target + W) / (W / 2.0).
   - q ∈ [0, 1): dist = bootX + W − target
   - q ∈ [3, 4): dist = target + W − bootX
   - otherwise: dist = |bootX − target|
4. speed = trunc(dist × 2000 / W + 250) px/s.
5. **Bursts,** only when not stomping.
   - If accel ≥ 1: when R(100) ≥ 91, accel = 0.
   - Otherwise: when R(100) > 97, accel = R(500) + 500.
   - The first roll happens only when accel ≥ 1.
6. **Direction,** when not stomping and horizontal motion is on:
   - q ∈ [1, 2) or [3, 4): vx = +speed
   - q ∈ [0, 1) or [2, 3): vx = −speed
7. **Stomp trigger.** If dist < 11, not stomping, chasing and the boot's ready flag (+0x178) is set:
   - stop
   - if R(100) > 85, stomp: y acceleration 100, vy = 0, vertical motion on
8. **Hover.** While not stomping and chasing: y = trunc((80 − h) + sin(x / 10) × 5), with integer x / 10 and sin in radians.
9. **Floor.** When the stomp reaches y == 570 − h:
   - vy = −1, y acceleration 150
   - if the fly is splatted, count = 15, which ends the boot after this stomp
   - count += 1
   - stomp effect and sound 6, after setting its volume to +8000 (out of range, as for sound 29 in §3.6) [C for the call, I for the effect]
10. **Ceiling.** Rising above 80 − h: y = 80 − h; reset vy, the acceleration and the stomping flag.

### 7.3 Effect on the fly — yes, it can kill [C]

**Contact sides** (`FUN_0043b157`). The fly box is shrunk to (x + 30, y + 25, w − 30, h − 25).

- horizontal side: 3 if bootX < boxX (the fly is pushed right), else 1
- vertical side: 2 if bootY < boxY (the fly is below the boot), else 0
- side1 = the axis with the smaller penetration; a tie reports both

**Push** (`FUN_0043734c`, mode 2 from the tick). Place the fly against the boot by side1:

| side1 | Placement |
|---|---|
| 0 | y = bootY − (h − 20) |
| 1 | x = bootX − (w − 25) |
| 2 | y = bootY + bootH − 20 |
| 3 | x = bootX + bootW − 25 |

Then:

- mode 1 would also divide v by 1.4
- y is capped at 600 − h

**Splat.** If y > 249 and side1 == 2 and the fly is not already splatted, the fly is splatted (§3.6). That means the fly is under the boot and pressed into the lower part of the screen.

- No damage total is involved: the splat is an instant death.
- The kill is recorded when the particles end.

### 7.4 Fan contact, for comparison (`FUN_0042a075`) [C]

- At most once per 100 ms.
- vx is pushed by the fan strength (+0x91c); vy −= strength + 100.
- damage = R(20) + 40, or R(10) + 20 with SuperFly. It is added to the damage total directly, not through §3.6 A.
  - total ≤ 100 → the post-stun flag only
  - total > 100 → splat, with particles thrown in the fan's direction
- Sound 45.

---

## 8. Midges (the fly's goodie)

### 8.1 Spawn (`FUN_00404caf`) [C]

1. Effect on (game+0x407d0).
2. Add min(100, 300 − active) midges in free slots (game+0x178a0, stride 0x208). Each midge gets:
   - a wander mover: re-roll R(150) ms, speeds 200/200, runs 5/3, acceleration 30
   - x = R(W − 810) + 800, y = R(600)
   - vx = ±(R(200) + 200), with the sign from a sign roll
   - a size of R(5) + 30
   - a homing target on the farmer:
     - x offset R(farmerW), y offset R(farmerH − 60)
     - tolerances R(30) and R(30)
3. Sound 48 is played by the goodie use (`FUN_00404198`), not here.

### 8.2 Per frame (`FUN_00404ef3`, called from the render pass `FUN_00403e36`, so once per tick) [C]

1. Each midge homes toward its point on the farmer (`FUN_00442f95`). Each tick there is a 30% chance to re-roll the target.
2. A swatter strike kills up to 20 midges that it touches.
3. A fan pushes midges by ±180.
4. inBox = the number of midges inside the farmer's box.
5. **Farmer slowdown** (`FUN_0043cd23(farmer, active, inBox, total)`). With r = inBox / total, the farmer alternates two timers:
   - **Slowed** for trunc(r × 400 + 150) ms:
     - horizontal speed cap = trunc(maxSpeed (farmer+0x9c) × (1 − r))
     - vertical speed zeroed every tick
   - **Free** for trunc((1 − r) × 500 + 200) ms.
6. The effect ends when no midges remain.

The ambient midges (game+0x15a20, tick step 14) are a separate, cosmetic population that reacts to the swatter and fans [I for "cosmetic"].

---

## 9. Sound helpers [C]

The two per-slot helpers call the DirectSound buffer directly, with no rescaling:

| Helper | Buffer method | Evidence |
|---|---|---|
| `FUN_0040cea9` (via `FUN_0040dfe5`) | SetVolume (vtable +0x3c), hundredths of a dB, valid −10000..0 | callers pass −100, −500, −1000 and similar |
| `FUN_0040ced5` (via `FUN_0040e008`) | SetFrequency (vtable +0x44), Hz | callers pass values from 10000 up to 49999 |

So the winch (47) and fan impact (45) values are frequencies. The +5000 for sound 29 and +8000 for sound 6 are out-of-range volumes, which most likely leave the volume unchanged [I].

## Corrections to game-spec.md

1. **§4.1:**
   - The camera field game+0x407c0 is a **per-tick delta** in fly-layer pixels, assigned each tick, not an offset.
   - The fly lives in screen space; objects, the lift and the boot live on a 1.25-parallax layer whose wrap width is trunc(W × 1.25).
2. **§4.2 carrying:**
   - Not "Enter within 1000 px of an object". Holding Enter starts aspirating (a 66-tick animation).
   - The manager locks the last candidate whose grip point is within Euclidean distance 1000 of the fly's bottom-centre.
   - Attaching then needs the fly centre within half the object's width of the grip x; a 100 px magnet pulls the fly in.
   - Enter is a held action, blocked after a goodie use until it is released.
3. **§4.2 stun:**
   - The sequence is a 4-tick hit-in, then the knockback kick, then 4 ticks back out.
   - The ricochet runs independently: 23 deceleration ticks at kick / 25, or kick / 37.5 while a horizontal key is held, then an x clamp.
   - SuperFly cancels the kick entirely.
4. **§4.2 dying:** crash animation → 1500 ms timer → kill recorded → respawn request → appear animation. The "falls toward (850, 850)" part is the crash effect.
5. **§4.2 respawn:**
   - y is not random: y = trunc((600 − h)/2 − 20) and x = R(800 − w).
   - Re-rolled while overlapping an object, up to 1000 tries (3000 at period start).
6. **§4.2 movement:**
   - Climb while carrying is trunc(75 × c + MY × 0.1f / 1.6), with MY including the turbo boost.
   - Friction is 9 per tick toward 0 per axis without overshoot.
   - The jitter is 0/±1 (±2 only at rand 32767), only when idle at FACE and not carrying.
   - Turbo's +50% uses maxX for both axes; the decay is 5% of the base maximum per tick, per axis.
7. **§4.4:**
   - Up to 15 objects per period, chosen by meal mask and stock.
   - x = R(trunc(W × 1.25)), with spacing and trap checks.
   - The 0..760 / 36 / 0.152 draw is the **lift anchors**.
   - Weight is +0x90 and +0x92 is the collectible flag.
   - Drop: vx / 50 drift plus +50 px/s per tick of gravity. The fly is moved up 20 px and gets vy = −(|vy| + 300).
   - The states are rest, sliding, falling and held; objects stack.
8. **§4.5:** the lift is a three-state machine (idle bob, tracking with a 30-tick ring-buffer lag, hoisting at −300 px/s) with explicit hook windows. `FUN_0042a760` is the manager destructor.
9. **§4.10 / Open question 9:** the boot **can kill** the fly. Contact from above, with the fly's y > 249, splats it instantly. Other contacts only shove the fly. The boot leaves after 10 floor hits, or immediately after it splats the fly.
10. **§4.9 / Open question 10:** midges slow the farmer in bursts; the slowdown is proportional to the share of midges on him (§8.2). The spawn size is R(5) + 30 [C].
11. **§4.11 / Open question 5:**
    - Spawn chance per check = luck p (§6.1).
    - Mystery 3%; otherwise kind 3–5, biased toward 3–4 for a side far behind.
    - Re-arm times scale with 1 − p.
    - The three normal kinds map to Turbo, SuperFly, Midges for the fly and Triple, Nuclear, Boot for the farmer (the spec's +2 × type indexing is right).
    - The 25 s mystery Turbo is cut at 15 s.
12. **§4.6 fan:**
    - The protection flag fly+0x24c is SuperFly [C].
    - Fan damage bypasses the stun animation: total ≤ 100 only sets the post-stun flag.
13. **§6.4 / §9.6:**
    - `.bor` x values are in 2-pixel units from the sprite's left edge; rows are y from the top.
    - A hit needs coincident outline points at half-x resolution.
    - The fly uses a fixed rectangle outline (x 30..90, y 29..84 of its frame).
14. **§4.2 dying sound:** sound 29 gets a volume of +5000 (out of range), not a frequency of 5000 Hz (§9).
15. **§10:** the fly update, the lift timing and the flying-goodie formula are now decoded here.

## Still open

1. **Stack push** (`FUN_0042e28a`): how a push spreads through a stack (summed weights, square root) is not decoded to the formula. The side-test helper `FUN_0042fb1c` is decoded only by its use.
2. **Camera accumulation** (`FUN_00441baf`, `FUN_004416b1`): how the per-tick delta becomes each layer's offset, and the exact wrap of W, were not re-read. W itself (the background width) was not measured from the asset; the spec says 4000.
3. **Farmer catch condition:** the swatter flag that gates the farmer's catch (`FUN_0043309b`) is not identified.
4. **Splat particle duration** (`FUN_00439c2b`): how long a splat lasts before the respawn request is not timed.
5. **Outline quirk:** whether the "contained span" y quirk in `FUN_0042f6de` ever changes a result.
6. **Price statistic:** the per-delivery price-statistic formula (`FUN_0042d8a0` and its neighbours) was not decoded beyond "+ trunc(price)".
7. **Reseeding:** whether exact replays need the wall-clock reseeding reproduced (§1). This is a project decision, not an RE gap.

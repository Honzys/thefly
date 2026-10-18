# Answers to the `// shortcut:` guesses

Each entry gives the code location and its guess, what the original does, the evidence (function
address, [C] confirmed from the dump or [I] inferred), and a verdict. Sections are sorted by
verdict: guess wrong first, then guess correct, then port decisions (one line each).

Conventions used below:

- `R(n)` is `rand() % n` on the MSVC generator; draws are listed in the order they consume rand.
- "parity sign" means −1 when the raw rand is even, +1 when odd. "R(10) sign" means −1 when
  `R(10) < 5`. Neither is the port's `rng.sign()` (−1 when rand ≤ 16383).
- A timer is "expired" when its deadline is strictly before now. A timer that is created but never
  armed has its deadline at creation time, so it reads as expired on the first check.
- "Screen x" is a sprite's +0xc field; "wrap" is its +0x2c field (5000 on the 1.25 layer).

## Guess wrong

### Farmer goodie use cancels the same tick's swing
`crates/thefly-sim/src/world/goodies.rs:136`

Guess: the goodie key cycles the selector and a held swing key uses it; the swing still happens.

Original (`FUN_004043c1`, called from the farmer tick `FUN_0040326c`) [C]:

1. The selector (game +0x408aa, values 0..3) advances by one on the goodie key's press edge
   (previous state < 0, now > 0) and wraps from 3 to 0.
2. A use happens when the selector is > 0 and the swing key is held (level, not edge):
   - 1 = Triple: needs Nuclear off (farmer +0xe4 == 0) and a Triple in stock (data +0x80).
   - 2 = Nuclear, 5000 ms: same Nuclear-off gate, stock at data +0x82.
   - 3 = Boot: stock at data +0x84; spawns the boot (`FUN_0043aabe`) and sets game +0x3df58 = 1.
   - On success: an effect at (farmer x + 181/2, farmer y + frame-0 height/2) and sound 37.
3. Whenever step 2 is entered, whether or not the stock or gate allowed it: selector = 0,
   +0x407cc = 10, and the function returns 1. The caller then clears this tick's swing edge, so
   no swat starts on the tick a goodie was used (or attempted).

AI path: the AI's chosen goodie comes from `FUN_0043f885` (AI +0x48, cleared on read) and is fed
as `FUN_004043c1(game, 0, 1)`, after fly physics and before the AI update `FUN_0043f611`.

Verdict: guess wrong. Keep the cycle and held-swing use, but a use attempt must consume the swing
edge for that tick and reset the selector to 0 even when nothing is in stock. The effect y is
farmer y + frame height/2, not swatter y (presentation).

### AI aim offset: sign before magnitude, parity sign
`crates/thefly-sim/src/world/ai.rs:226`

Guess: offset = magnitude then sign per axis, using `rng.sign()`.

Original (`FUN_0043baa4`) [C]:

1. Refresh the target's cached x/y (`FUN_0043b654`).
2. x sign by parity, then `ox = R(S) × sign`.
3. y sign by parity, then `oy = R(S) × sign`.

Spread S comes from the callers `FUN_0043e716` and `FUN_0043f8b1`: 700 while wandering; when not
wandering, the aim spread with Triple off (and then the offset is only re-rolled when the fly
state `FUN_0043885d` is 0), or aim spread − cut with Triple on.

Verdict: guess wrong. Draw order is sign, magnitude per axis (x first), and the sign is parity.

### Boot: distance metric, spawn x and step order
`crates/thefly-sim/src/world/boot.rs:142`

Guess: ring distance on layer x with W = 4000; integrate, then hover, floor, ceiling.

Original (`FUN_0043abc7`) [C]:

- Positions are screen x (+0xc) and the wrap is the boot's own +0x2c (5000), not layer x / 4000.
- target = fly x (fly +0x1c4) − (boot width + 50) + offset.
- q = (boot x − target + Wb) / (Wb / 2.0); speed = trunc(dist × 2000 / Wb + 250).
- Spawn x is a screen x: `R(Wb − 900) + 810`.
- Steering does not check the horizontal-on flag, so a retired boot still steers.
- Hover, floor and ceiling run before the mover integrates, not after.
- The ceiling reset only applies while stomping and chasing, and also zeroes the burst.
- The integrator is the shared FPS mover (`FUN_004180af`), not a DT step.
- Gone = not chasing and y + h < 1. The mover clamps y ≥ −h, so the port's `y < −h` test may
  never fire.

Verdict: guess wrong on the metric, the spawn coordinate space and the step order.

### Boot burst draws
`crates/thefly-sim/src/world/boot.rs:200`

Guess: burst rolls only when not stomping.

Original (`FUN_0043abc7`) [C]: applying the burst as speed + burst is right (the mover adds
sign × accel once per tick while +0xa8 is set). The rolls are:

```
if accel < 1 or R(100) < 91 or stomping:
    if R(100) > 97 and not stomping: accel = R(500) + 500
else:
    accel = 0
```

So the rolls are drawn even while stomping, and a second R(100) follows whenever the first
condition is reached through `R(100) < 91`.

Verdict: guess wrong on the draw pattern.

### Boot contact tie
`crates/thefly-sim/src/world/boot.rs:248`

Guess: a tie picks one side and may splat.

Original (`FUN_0043b157`, `FUN_0043734c`) [C]:

- Overlap is inclusive and uses the frame size (+0x68/+0x6a). x is unwrapped by subtracting the
  wrap when x ≥ wrap − frame width.
- Penetration per side is |boot edge − box edge| (not min/max).
- On a tie, side 1 is the horizontal side and side 2 the vertical side; the push applies both
  placements.
- The splat needs side 1 == 2, so a tie never splats.

Verdict: guess wrong.

### Wander mover internals
`crates/thefly-sim/src/world/flying_goodies.rs:57`

Guess: a simplified wander with bounce at the bounds.

Original (`FUN_004174c0`, shared mover `FUN_004172a8`) [C]. Per axis, x first then y, only when
that axis's timer has expired and its accel flag (+0xa8 / +0xac) is 0:

1. Re-arm the axis timer with R(reroll).
2. d = R(10).
   - d < 5: if v < 0 and run ≤ max, then run += 1 and dir = +1; otherwise run = 1 and dir = −1.
   - d ≥ 5: if v < 0 or run > max, then run = 1 and dir = −1; otherwise run += 1 and dir = +1.
3. v = (R(speed) + 65) × dir, set directly (not added).

Then every tick while wander is on (+0xa4): v += sign(v) × accel (or + accel when true gravity
+0x130 is set). Bounds (+0x138 on; +0x13c right, +0x13e left, +0x140 top, +0x142 bottom) clamp
the position and leave the velocity untouched; there is no bounce. The left x bound is never
applied.

Verdict: guess wrong: clamp, not bounce; the draw order is timer, R(10), R(speed).

### Fan strength
`crates/thefly-sim/src/world/traps.rs:30` (`FAN_STRENGTH`, O9)

Guess: 400.

Original (fan constructor `FUN_00429410`) [C]: +0x91c = 500 and +0x920 = 200.0, set once and never
rewritten. The hit (`FUN_0042a075`) adds ±500 to vx and subtracts 600 from vy.

Also in the original, and missing from the port: every tick before the hit test, the blow field
`FUN_00429cbc` pushes the fly by its centre (cx, cy) [C]. The branch is chosen by the internal fan
type at +0x918: 1 pushes right and 2 pushes left. This is the inverse of the shop/trap kind index.
Kind 1 (`ventilo_gauche`) is built as internal type 2, and kind 2 (`ventilo_droite`) as internal
type 1 (see "Fan type mapping" under Guess correct). With fan screen x `X` and fan y `Y`:

- Type 1 (right): cone base `a = X`, mouth `m = X + 135`, rows `Y + 42 .. Y + 249`.
  Inside when `a < cx < m + 500`.
- Type 2 (left): base `a = X + 222`, mouth `m = X + 93`, rows `Y + 43 .. Y + 279`.
  Inside when `m − 500 < cx < a`. Type 2 also unwraps cx near the wrap edge (rule in "Fan blow
  field: type-2 unwrap").
- With `d = |a − cx|`, cy must satisfy `top − 0.5·d < cy < bottom − 0.35·d` (the cone slants up).
- Push `P = trunc((1 − |m − cx| / 500) × 200)`; then `vx ±= trunc(0.75·P)` (sign = facing) and
  `vy −= trunc(0.25·P)`.

The hit (`FUN_0042a075`, 100 ms re-hit timer) adds the damage roll to an accumulator (+0x2cc);
while it is ≤ 100 it sets +0x314, above 100 it starts the knockback `FUN_004397c0`. The fan skips
the fly entirely while fly +0x2a4 is set.

Verdict: guess wrong: use 500. The blow field is a separate missing behaviour.

### Zapper arc reach and schedule (O9)
`crates/thefly-sim/src/world/traps.rs:389`, `crates/thefly-sim/src/world/traps.rs:414`

Guess: an arc reaches the whole column; arcs follow a fixed 3000 ms idle / 400 ms arc schedule.

Original (`FUN_00428b47` per tested target, called from the zapper/fly step `FUN_004288fe`) [C].
There is no fixed schedule. Each tick, for each target tested against each zapper:

1. Zapper screen x, unwrapped (x − wrap when x ≥ wrap − zapper width).
2. Column: target x − zapper width ≤ zapper x ≤ target x + target width; otherwise no arc.
3. No arc if one is already active on this zapper, or if the retry timer (+0x948) has not expired.
4. Reach: the target y must be ≤ zapper y + zapper height + 250; otherwise no arc.
5. d = |target y − (zapper y + zapper height)|; p = trunc((1 − d / 250) × 100).
6. If R(100) > p: re-arm +0x948 for 100 ms, no arc.
7. Otherwise the arc sprite (`j_eclaires`, 125×156 frames, 4 frames, at y 78) is fitted first.
   It is scaled uniformly by 1 + (target y − 234) / 156, or by 0.02 when that ratio is ≤ −1.
   Its x is trunc(cx − scaled w / 2.0), with cx = trunc(target x + target w / 2.0). With
   L = zapper x + 55 and Rt = zapper x − 30 + zapper w (amended in round 2, [C]):
   - the arc lies within [L, Rt]: frame R(4) + 1. This is one more draw.
   - it overhangs L by at most half its width: frame 3, no draw.
   - otherwise, if it overhangs Rt by at most half its width: frame 2, no draw.
   - otherwise nothing starts: no further draws, and +0x948 is not re-armed, so R(100) is rolled
     again on the next tick.
8. When a frame was chosen, the arc starts. It lasts R(80) + 30 ms, the retry timer is re-armed
   for 100 ms, and sound 39 plays at R(800) + 11025 Hz. Draw order on a start: R(100), [R(4)],
   R(80), R(800).

The fly is only tested when fly +0x2a4, +0x2a8 and +0x288 are all 0. The fly's target is
(fly x, fly y + 45, fly w), not the fly rect (round 2, `FUN_004288fe` [C]). An arc on the fly pushes
vy by −180 and adds 1 damage (the port's values are right). The hum volume uses the "any zapper
on screen" test `FUN_00418b40`: −200 when any is visible, else −500; a single zapper plays
positionally at −450 (manager `FUN_00427270`).

Verdict: guess wrong. Arcs are a per-tick probability that falls linearly to 0 at 250 px below
the zapper bottom, with a 100 ms retry and an R(80)+30 ms arc. This consumes rand. Round 2: the
port's `Zapper::arc_at` (traps.rs:244) skips the arc fit in step 7, so it misses the R(4) draw
and the no-start case. It should also test the fly at y + 45.

### Midge spawn draw order
`crates/thefly-sim/src/world/midges.rs:111`

Original (`FUN_00404caf`) [C], in rand order:

1. x = R(W − 810) + 800, with W the scene width (game +0x1a); amended in round 3, it is
   neither raw nor wrapped
2. sign: R(10) sign
3. |vx| = R(200) + 200
4. size = R(5) + 30
5. y = R(600)
6. a timer R(500) + 500 (inside `FUN_00442806`)
7. tolA = R(30), then tolB = R(30)
8. home y = R(frame-0 height − 60), then home x = R(farmer frame width) (round 3: the
   modulus is read from the farmer sprite, not a literal 181)

x tolerance (+0x180) = tolB, y tolerance (+0x182) = tolA. The x tolerance is not used by homing.

Verdict: guess wrong where the port differs from this order or swaps the tolerances.

### Midge homing
`crates/thefly-sim/src/world/midges.rs:151`

Original (`FUN_00442f95`) [C], every tick for every midge that is neither stunned nor falling
(amended in round 3; a stunned or falling midge makes no homing draws), after the contact step:

1. Ring q of the midge against farmer x + home x.
2. If R(100) > 50: new speeds sx = R(200) + |v0| and sy = R(60) + |v0 − 180|.
3. x is always steered when q ∈ [0, 4) (no tolerance).
4. y is steered only outside ±y tolerance; inside it, y wanders.

Verdict: guess wrong where the port applies an x tolerance or a different order.

### Goodie-midge removal
`crates/thefly-sim/src/world/midges.rs:258`

Original [C]: when a goodie midge is caught, the respawn draws R(500) and R(H − h) still happen.
The midge is then marked dead but still runs the rest of the tick (homing, retarget, box, fan,
update) and is dropped at the next tick's compaction.

Verdict: guess wrong if the port removes it immediately or skips those draws.

### Ambient midge population (O2)
`crates/thefly-sim/src/world/midges.rs:239`

Original [C]: count = R(15) at period load, after slot 0 (see "Midges: spawn x, homing gates,
ambient update and constructor", round 3). Per tick and per ambient midge: arc test, contact,
update, then the fan push. Inside the update the fan push decays by 40 per tick. The jitter (if
R(100) > 70 then R(15), then R(10) decides the sign, vx = sign × m − vx) runs once per expiry of
the midge's timer, not every tick (amended in round 3). Ambient midges never home and are never
removed.

Verdict: guess wrong if the port uses a fixed count or homes them.

### Trap loop volumes are driven by visibility, not by a blow/arc schedule
`crates/thefly-sim/src/world/traps.rs:43` and the zapper volumes (O9)

Original (trap manager update `FUN_00427270`) [C]. Each tick:

- Fans: with exactly one fan, its loop (slot 44) plays positionally at −100. With more fans, the
  loop is set to −100 when any fan is on screen (`FUN_00418b40`: screen x + width ≥ wrap, or
  screen x ≤ 800), else −400.
- Zappers: with exactly one zapper, slot 40 plays positionally at −450. With more, −200 when any
  zapper is on screen, else −500.

Verdict: guess wrong. "Blowing" is "on screen"; the zapper hum level does not follow arcs.

### Flying goodies: the fly side is checked before the farmer side
`crates/thefly-sim/src/world/flying_goodies.rs:231`

Guess: farmer first, then fly.

Original (`FUN_00432684`, one call per tick handles both sides) [C]: the fly side (timer +0x3a8,
luck for side 1, 8000 ms lifetime, re-arm + 16000) is checked and spawned first, then the farmer
side (timer +0x378, luck for side 2, 12000 ms lifetime, re-arm + 20000). Per side, in rand order:
the luck roll R(100); on a spawn, R(100) < 3 for the special goodie, else the kind draws; then
x = R(800), y = R(500); then the launch (no rand, see the next entry).

Verdict: guess wrong: check the fly side first.

### Flying goodie launch draws no speed
`crates/thefly-sim/src/world/flying_goodies.rs:360`

Guess: the launch draws x, y, then one target speed R(30) + 65.

Original (launch `FUN_00431c64`) [C]: x = R(800) and y = R(500) are drawn by the caller (x
first). The launch itself draws nothing. It turns wander on, clears both accel flags, and sets the
wander parameters: reroll 500 ms, speed range 30 on each axis, run caps 8 (x) and 6 (y), accel 5
on both axes, velocity 0. Bounds are on: right = wrap + 100, left −100 (unused, see Wander),
top 0, bottom 500. The lifetime timer is armed (8000 or 12000 ms).

The R(30) + 65 speed is drawn later, per axis, by the wander step (see the Wander entry) when that
axis's timer expires. The axis timers are not armed at launch, so the first wander tick draws for
x and then y [I].

Verdict: guess wrong: remove the launch speed draw; the speed comes from the wander step.

### Animal sounds: a per-tick 1-in-150 chance, weighted choice
`crates/thefly-sim/src/world/ambient.rs:52` (O11)

Guess: one of slots 14..21 by R(8), every 15000 + R(15000) ms.

Original (`FUN_00441b3c`, called every farmer-scene tick from `FUN_0040326c`) [C]:

1. Draw rand; if `rand % 150 != 0`, nothing happens (one rand per tick, always).
2. Otherwise R(100): if < 71, index = R(3); else index = rand % 8 (one more draw).
3. Play the slot from the table {14, 15, 16, 17, 18, 19, 20, 21} at that index.

There is no timer. Slots 14..16 are heavily favoured (71% plus their share of the 29%).

Verdict: guess wrong.

### Kill type with no hit
`crates/thefly-sim/src/match_state.rs:354`

Guess: hits ≤ 1 give type 3.

Original (`FUN_0040bc5e`) [C]: the kill counter (+0x21c) and stats kills always increment. Then the
type at the new index is written as 3 for exactly 1 hit, 2 for 2..3 hits, 1 for 4 or more. For 0
(or negative) hits nothing is written, so that entry keeps its previous value (0 after a fresh
match). There is no cap in this function.

Verdict: guess wrong: a 0-hit kill records type 0 (no type), not 3.

### Farmer voice 33: the winch hooks an object (amended in round 2)
`crates/thefly-sim/src/world/ambient.rs:38`

Guess: "objects left on the table" = any collectible object still active. Round 2 guess
(ambient.rs:39): the flag is the fly's carried index, which a plain drop clears.

Original (caller `FUN_0040326c`, flag `FUN_004060e0`, manager `FUN_0042c888`, winch
`FUN_00431115`, voices `FUN_0043c730`) [C]: the flag passed in is "object manager +0x1a ≠ 1000".
Round 1 read +0x1a as the fly's grabbed object, which is wrong. The fly's grab is +0x10. Field
+0x1a is written in only three places:

- 1000 at the manager build.
- The winch's return value, each tick it returns a real object index. The winch (`j_treuil`,
  0x49bba0) returns the hooked index on the tick it hooks an object the fly let go of, and on
  every hoisting tick after that (state 3).
- 1000 when the winch returns −1. That happens when its top passes y < 0: the object is
  delivered, sound 11 plays, and the fly's saved count for that type goes up by 1.

A plain fly drop (fly +0x32c = 0, +0x334 = 1, fly y −= 20, manager +0x10 = 1000) never touches
+0x1a. So the flag means "the winch is hoisting an object", and its rising edge is the hook tick.
Voice 33 plays with R(100) < 15 on that edge. The other voices in the same function, in rand
order each tick:

1. Timer +0x18 expired and R(100) < 7: re-arm R(7000) + 12000 ms, voice 31.
2. If farmer +0x48 > 7 and R(4000) < 5: voice 32.
3. Hoist flag rising and R(100) < 15: voice 33.
4. Fly-state flag (`FUN_0043885d` != 0) rising and R(100) < 10: voice 30.
5. While farmer +0x84 is set, a counter counts ticks; every 21st tick it resets and plays one of
   five voices from the table at 0x4680f0 by R(5). When +0x84 clears and the counter is non-zero,
   the counter is set to 30.

The R(100) draws in steps 3 and 4 only happen on a rising edge.

Verdict: guess wrong, in both rounds: the edge is the winch hooking an object, not the fly
grabbing one. Feed `Lift::state == Hoisting` (from the hook tick to delivery) instead of
`objects.carried`.

### Goodies shop: purchase and tally are motion-driven, not a fixed 600 ms (O4)
`crates/thefly-sim/src/shop.rs:22` (`CONVEYOR_MS`)

Guess: 600 ms from purchase to grant, and one tally cycle of 600 ms.

Original (shop loop `FUN_00422571`; purchase `FUN_00422223`; drop `FUN_00423047`; tally
`FUN_0042322f`; input `FUN_00421e6a` and its caller) [C]:

- **Shop state** (+0x2308): 0 = buying, 1 = an item is dropping, 2 = tally running.
- **Purchase:** on success, sound 11 and the item sprite is placed at the button's x, y = 5,
  vy = 650 px/s (FPS mover). State 1. Each tick it falls; while y < 270 its scale grows by 0.05 per
  tick, after that it shrinks by 0.05 (presentation). When y > 541 it snaps to y 542, hides, and the
  item count is incremented (farmer data +0x7c[i], fly data +0x86[i]). That is the grant, about
  537 / 650 s ≈ 825 ms after the purchase [I for the exact tick, which depends on the integrator].
  A failed purchase plays sound 12 and nothing drops.
- **Presses during a drop are latched, not lost:** the button bar is still fed during state 1 and
  latches a press on its rising edge (bar +0x9f4); the latch is only read back in state 0, so the
  press executes on the first tick after the drop ends. A second press during the same drop
  overwrites the latch.
- **Farmer tally (kills):** one cycle per kill. The dead-fly sprite (`g_flymort`, frame R(3) + 1,
  one rand per kill) starts at x 805, y 239 − h, vx −500. Once x + w < 372 it falls with
  vy += 50 per tick (sound 10 while 0 < vy < 80). When it reaches the target sprite's y it hides,
  the per-kill price (data +0x15c) is added to the money (+0x90) and stats (+0x38), the kill
  counter drops by 1, and the next cycle starts. The cycle length is therefore set by the motion,
  not a timer.
- **Fly tally (saved types):** the same, per saved type with count ≥ 1 (types 0..21 at data
  +0x1f0), using that type's image, vx −250, stopping at centre x < 588 for the "x N" label (timer
  +0x2470, created at 2000 ms), and paying price × count.
- **Skip:** an Enter rising edge during the tally pays everything left at once.
- The shop AI is paused (`FUN_0044151b(ai, 1)`) while a farmer tally runs and released (re-armed
  700 ms) when it ends.
- The shaking machine (+0xac8) calls the shared mover every tick, but its wander flag (+0xa4)
  is off except right after a tally landing, so it does not draw rand all through the shop.
  Round 1 said it did; that was wrong. See "Shop tally: target, food sizes and machine shake"
  for when it does draw.

Verdict: guess wrong. Replace the timer with the drop (≈ 825 ms) and per-item tally motion, and
latch a press made during a drop.

### Goodies shop side order and exit
`crates/thefly-sim/src/shop.rs:222`

Guess: farmer, then fly, then leave; "no money left" ends early.

Original (init `FUN_0042123c` → `FUN_00421ac4`; switch `FUN_00421b51`; done in `FUN_00421e6a`)
[C]:

1. The shop opens on the **fly** side, always with the fly tally (state 2).
2. The fly's done (button 5) fades out; when the fade ends the shop switches to the **farmer**
   side (`FUN_00421b51`). The farmer starts with the kill tally if kills > 0, else buying. The
   shop AI (one-player games) is created here, for the farmer only.
3. The farmer's done checks the farmer's trap tokens (data +0x7e), not money:
   - tokens < 1: fade out, and on the press tick re-roll the winch slots (`FUN_0040bf70`, range
     760, gap 36) and clear the trap layout, then leave without the traps shop;
   - tokens ≥ 1: go to the traps shop after the fade (music slot 9 volume step).

There is no "no money left" exit.

Verdict: guess wrong: fly side first, then farmer. The winch re-roll happens on the done press,
before the fade (the port does it after; this changes the RNG order because of the machine
wander draws during the fade).

### Traps shop: carrying a trap
`crates/thefly-sim/src/shop.rs:29` (start x 380, speed cap 10) and `crates/thefly-sim/src/shop.rs:372`

Original (carry start in the traps shop input, carry step `FUN_0042570b`) [C]:

- Start: x = 350 (screen x, which is also the stored trap x); y = 486 − h for a fan, else 396.
  A fan starts as type 1 (facing left).
- Each tick:
  - Left held (whether or not right is held): speed −= 1, floored at −10; x += speed; a type 2
    fan becomes type 1.
  - Else right held: speed += 1, capped at 10; x += speed; a type 1 fan becomes type 2.
  - Else: speed = 0.
  - So a reversal decelerates through 0 rather than resetting, and left wins when both are held.
- Clamp: if x + w ≥ 780 then x = 780 − w; else if x < 21 then x = 20 (w = the carried trap's frame
  width). The range is [20, 780 − w].
- Place (Enter): store (x, type) with `FUN_0040bd61`; failure plays sound 12. Success spends
  tokens: flypaper 1, fan 2, zapper 4 (floored at 0), sound 13.

Verdict: guess wrong: start 350 not 380, range [20, 780 − w] in stored units, no reset when
both keys are held (left wins). The fan facing rule (shop.rs:372) is correct.

### AI dwell expiry re-rolls the aim offset
`crates/thefly-sim/src/world/ai.rs:479`

Guess: "reset speed" means clearing the held direction keys.

Original (AI update `FUN_0043f611` step 2 → `FUN_0043e7bf`) [C]: when the dwell timer (+0x14)
expires, the AI re-rolls the aim offset (`FUN_0043e716` → `FUN_0043baa4`: spread 700 while
wandering, else the aim spread +0x50; parity sign then R(S) per axis, x first), requests a swing
(+0x34 = 1) and destroys the dwell timer. The direction keys and the pending reaction are not
touched.

Verdict: guess wrong: re-roll the offset (four rand draws) instead of clearing the keys.

### Bourse in-play columns come from a fixed meal mask (O18)
`crates/thefly-sim/src/bourse.rs:63`

Guess: a column is in play when its type was present in the last period.

Original (bourse init `FUN_0042008e`, draw `FUN_00420476`) [C]:

- Class bit c from the period field data +0x20: 1 (breakfast) gives c = 2, 2 (lunch) gives c = 4,
  anything else (4 = dinner, or 0 before the first period) gives c = 1. The bits read as
  1 = breakfast, 2 = lunch, 4 = dinner, so the board shows the foods of the next meal [I for
  that reading; the computation is C].
- Column i (0..21, the same index as the column permutation at 0x4672b8) is in play when
  `mask[i] & c != 0`, with mask (at 0x4672e8, by column):
  `4, 4, 2, 4, 2, 4, 4, 1, 6, 1, 7, 1, 2, 2, 1, 2, 1, 4, 2, 4, 1, 4`.
- The leader value is the highest price over the in-play columns, computed once at init; on a tie
  the later column counts (`≤`). The blink timer is created at 200 ms and armed at 300 ms.

Verdict: guess wrong: use the fixed mask against the next meal's bit, not the present types.

### Bonus round movers, boxes and target spawns (O3)
`crates/thefly-sim/src/bonus.rs:126`, `crates/thefly-sim/src/bonus.rs:131`,
`crates/thefly-sim/src/bonus.rs:150`

Guess: scroll 4 px, move 6 px, fly box 60×44, swatter box 80×80, five fixed target rows,
spawn x 810.

Original (setup `FUN_004452a1`, update `FUN_00446030`, items `FUN_004472df`, launch
`FUN_00431c64`, hit test `FUN_00431e58`) [C unless marked]:

- **Backdrop:** `bckbonusfond` on a 0.7 layer and `bckbonusbarre` on a 1.0 layer, both with
  vx = −600 px/s. When the round ends, both vx += 2 per tick until vx > −81, then 0.
- **Setup rand order:** coin ω R(60); 99 stardust delays R(100) + 700; 10 target-wing delays
  R(10) + 10; then the ω re-roll `FUN_00447cc0` with the fly handicap (one more draw).
- **Coins** (20 slots, `piece`, 45×45 frames, 1.25 layer): at each 50 ms firing, at most one coin:
  re-roll check, frame R(frame count), x = 810, y from the sine rule (gaps §7.3), lifetime
  25000 ms, animation delay R(30) + 15. Wander and bounds are off and the code never sets the
  coin velocity. A coin is removed when x + w < 0.
- **Coin motion (round 2):** each coin is moved in the bonus draw pass (`FUN_004472df`, from
  the draw near 0x446d18) by the generic mover `FUN_004180af`. Its camera delta is the negated
  per-tick step of `bckbonusbarre` (bar +0x70) [C]. So each tick the coin's layer x changes by
  trunc(bar step), and screen x = trunc(layer x × 1.25) [C].
  - The bar step is the bar's 600 px/s spread over the ticks of a second (`FUN_004135c2`,
    layer factor 1.0). That is about 600/fps layer px per tick, about 750 screen px/s,
    leftward. The per-tick split is the FPS integrator's (round 3, "FPS integrator: per-axis
    runs and percent rounding") [C]: −9 or −10 px at 65 fps.
  - The bar writes +0x70 only while its vx ≠ 0. After the scroll stops, coins keep moving at
    the last step [C].
- **Coin pickup:** the coin is caught when its (x, y) lies in
  [fly x + 50 − coin w, fly x + 50 + (fly w − 100)] × [fly y + 40 − coin h, fly y + 40 + (fly h − 80)],
  inclusive. That is, the fly box is (fly x + 50, fly y + 40, fly w − 100, fly h − 80), tested
  against the coin's top-left grown by the coin's frame size. Sound: 49 or 50 (whichever is free;
  if both are free, 49 is started) at R(500) + 21500 Hz.
- **Targets** (10 slots, `piecetap`, 48×44, 1.0 layer): at each firing of the auto-re-arming
  spawn timer, at most one target, with draws in this order: y = bg y + R(bg h), then
  x = R(150) + 810. It launches with wander on, bounds top = bg y + 15 and bottom =
  bg y + bg h − 15, right = wrap + 100, and lifetime 25000 ms. The wander parameters are then
  overwritten: reroll 500 ms, x speed range 1 (so |vx| = 65 at each reroll), y speed range
  trunc(500 × h_farmer + 1), run caps 6 (x) and 3 (y), accel 5. So targets drift and bob under
  the Wander rule; there are no fixed rows. The wings (`ailesgoodies`) follow at
  (target x − 24, target y − 12). A target is removed when wing x + wing w < 0, or when its
  lifetime ends.
- **Target hit:** only while the bonus swatter is in its hitting state (`FUN_00448110`); box =
  (swatter x + 30, trunc(swatter reach), swatter w − 60, 154), with the same top-left-grown test.
  Sound 42, then the effect `FUN_00447d7d`.
- **Swatter on fly:** box (fly x + 85, fly y, fly w − 115, fly h) knocks the fly (`FUN_004485ed`).
- **Bonus fly (round 2; setup `FUN_004481c2`, mover `FUN_00448302`)** [C]:
  - Sprite `j_transi_est`, frame 120×94. Start x = R(300), y = 150, v = 0. Speed caps
    890 (x) and 670 (y).
  - A held key adds 0.1 × cap per tick: v = trunc(v ± 89.0000013) for x, so +90 when coming up
    from below −89. y uses 67.000001. Speeds are capped at ±cap. Right is +x and up is −y.
  - A released axis decays by 9 per tick toward 0.
  - Then x += trunc(vx · dt + bar step) and y += trunc(vy · dt) [dt split I].
  - x is clamped to [0, 680] (x < 1 gives 0). y is clamped to
    [bg y − 15, bg y + bg h + 15 − 94].
  - The buzz is positional (`FUN_0040d005`).
- **Bonus swatter (round 2; setup `FUN_00448802`, mover `FUN_00448c31`)** [C]:
  - `j_tapette_1..5`, frame 181×178, start y 351.
  - Right/left set vx ±400 per tick, capped at ±900. Up/down set vy ∓200 per tick, capped at
    ±400. A released axis is zeroed at once.
  - The same integration with the bar step. x < 1 gives 0, x > 799 gives 800. y is clamped to
    [351, 472].
  - Reach (+0x98) = (1 − (472 − y) / 121) × 375, so 0..375.
  - Then `FUN_0044936a` plays a move sound from slots 22–26 by R(5), every 31 ticks, or after
    21 ticks while vx ≠ 0.
- **Swing (round 2; `FUN_004490be`, `FUN_00449203`)** [C]:
  - The frame counter (+0x74) climbs 1..4, one per tick.
  - At frame 1: sound 27 at R(15000) + 35000 Hz, then R(100) < 30 adds sound 34.
  - It hits (`FUN_00448110`) while frame > 2.
  - Past 4 it clamps, sets the retract flag and spawns the shockwave (`FUN_00449428`) at
    x − 5 + 65. It retracts one frame per tick.
  - Then a cooldown counter (+0xac) must pass 20 before the next swing.
- **Swatter boxes:** on targets (x + 30, trunc(reach), 121, 154); on the fly, the fly's
  (layer x + 85, y, w − 115, h) against the same swatter span (`FUN_0044915e`).
- **Wings:** the `ailesgoodies` sheet is cut at 100×44 (`FUN_00413ce0`), so 3 frames;
  animation delay R(10) + 10 [C].

Verdict: guess wrong on the boxes, the target rows and the target spawn x (round 1). Round 2:
- `SCROLL_PX` 4 is wrong: coins move by the bar's per-tick step, about 600/fps layer px.
- `MOVE_PX` 6 is wrong: both movers are velocity-based, with the accelerations, caps and decay
  above.
- `SWATTER_BOX` 80×80 is wrong: the swatter is 181×178, hits with (x + 30, reach, 121, 154),
  and its reach is 0..375 from y.
- `WING_W` 100 is correct.

### Winner trophy: gravity, start, crush and hops
`crates/thefly-sim/src/winner.rs:46`

Guess: gravity 0.5, start y −50, a hop every 1000 + R(5000) ms.

Original (init `FUN_0044aedb`, choreography `FUN_0044b431`) [C]:

- **Start:** the trophy is created at (430, −50) (the create call also passes 2000 and 600, read
  as a lifetime/frame pair [I]); x = 50 instead when the fly won. Mover flag +0x130 = 1, only the
  y accel flag is set, y accel (+0x12c) = 90, vx = vy = 0. In the shared mover `FUN_004172a8`,
  +0x130 = 1 turns off the "accelerate along the sign of v" rule: the accel is always added. So
  vy += 90 px/s every tick, then the FPS integrator moves it. +0x130 is never cleared, so during
  the hops below the wander accel 5 is also always added (+5 to vx and vy per tick, never −5).
- **Crush:** while falling, once the trophy bottom passes the loser's y + trunc(loser h × 50 /
  100), i.e. the loser's mid-height, and the loser is visible, the loser is hidden.
- **Landing:** when trophy y + h ≥ 588: y = 587 − h, vy = 0, accel flags off. Dust burst at
  (trophy x − 20, 310) spread 480. Slot 54 at volume 2000, then 53 (farmer won) or 51 (fly won).
- **Hop setup:** wander parameters: reroll 100 ms, speed ranges 10 (x) and 50 (y), run caps 1
  (x) and 2 (y), accel 5; wander off; bounds on: left = x − 2, right = x + w + 2, top = y − 5,
  bottom 587. First rest R(1000) + 1000 ms. Keys active after 1500 ms.
- **Hop loop:** when the rest expires: wander on, hop for R(500) + 500 ms, and a cheer: slot
  {55, 56, 57, 58}[parity + 2 × winner flag] at R(2000) + 20500 Hz (three rand draws: hop time,
  parity, frequency). When the hop expires: wander off, vx = vy = 0, y = 587 − h, rest
  R(5000) + 1000 ms, slot 54 at volume −550, small dust burst (alpha 140) at (x − 20, 310)
  spread 537 × 50.

Verdict: guess wrong on the gravity model (+90 px/s per tick, FPS-integrated) and the hop rules;
start y −50 is right.

### Winner count-up tick slot and rand
`crates/thefly-sim/src/winner.rs:113`

Original (`FUN_0044b431` state 1) [C]: each tick the loser's counter goes up by 10 (capped at
its score), then the winner's (capped at its score; reaching it moves to the trophy fall). The
tick plays slot 49 by default. `FUN_0040cf55` returns 1 when the slot is **not** playing
(game-spec.md:323 calls it an is-playing test; amended in round 3). So the tick moves to 50 when 49
is busy and 50 is free; when both are busy, 49 is stopped and replayed. The frequency
R(500) + 21500 is drawn and set **only while the loser's counter is
still below the loser's score**; after that the slot plays with no new rand.

Verdict: guess wrong on the rand rule (the port draws every tick); RING_A as slot 49 is right.

### Quit dialog: Esc
`crates/thefly-sim/src/screen.rs:354` (O15)

Guess: Esc answers No.

Original (quit state 0x3f `FUN_00409495`, dialog input `FUN_00411b8f`, key map `FUN_00407241`) [C]:
the dialog is fed only Up (+0x44), Left (+0x50), Down (+0x48), Right (+0x4c) and Enter (+0x64,
DIK 0x1c). Left moves the selection −1 and Right +1, both on the press edge and wrapping
around the item count. The Enter press edge commits the highlighted item. Esc (+0x6c, DIK 0x01)
is never read in state 0x3f, so a press does nothing. Result 0 (Yes): leave the previous state,
then go to the farewell (0x82) if the dialog was opened from the menu (0x1a), else to the menu
(0x14). Result 1 (No): return to the previous state.

Verdict: guess wrong. Esc is ignored in the quit dialog.

### Farewell length
`crates/thefly-sim/src/screen.rs:146` (O5, `FAREWELL_MS` 3000)

Guess: a fixed 3000 ms, then exit.

Original (init `FUN_0040a663` → `FUN_00449fae`, run state 0x85 `FUN_0040a749`, update
`FUN_0044a641`, close `FUN_0044a9a8`, done `FUN_0044aa3c`) [C]:

- **Entry:** Esc/Enter edges are consumed, and the Enter "previous" is set to 10 so a held Enter
  must be released first. A 1000 ms delay timer starts. The logo (`logo`) starts at scale 0.045.
- **Logo zoom:** after the 1000 ms, a step timer starts at 25 ms. At each step the next interval
  is trunc(40 − 40 × scale + 2) ms (computed from the scale before the step), then scale += 0.05
  and a counter += 14. Once scale ≥ 1.0: scale = 1, the counter is reset, the second layer's
  150 ms effect starts and slot 46 plays. That takes 20 steps, about 25 + 418 ms, so the logo is
  full about 1.45 s after entry.
- **Exit is key-driven; there is no timeout.** A press edge of Enter or Esc, once the zoom is done
  and the second layer's effect is no longer running, starts a 700 ms fade (alpha 0 → 255) and
  plays slot 33. When that fade finishes, the state goes to 0x46 (quit). Presses before then
  are consumed and ignored.

Verdict: guess wrong. The farewell waits for Enter/Esc, then takes 700 ms more.

### Stats breakdown: tick slot, kill timing, keys
`crates/thefly-sim/src/stats.rs:161`

Guess: ring slot for the tick; kills start 1500 ms after screen entry; draws only while counting.

Original (`FUN_0044c73d`, tick `FUN_0044c9d4`, leave `FUN_0044c6e2`) [C]:

- **Tick:** slot 49, with the same 49/50 pick as the winner count-up. The frequency is
  R(500) + 21500 for points and R(500) + 18500 for kills. One rand is drawn on every tick that
  plays.
- **Points phase:** every tick, points += 10 (capped at the score), kills shown = 0, result =
  points, and the tick plays. That includes the capping tick, and a score ≤ 0 still gives one
  tick. On reaching the score, the kill timer is armed at **1500 ms from that moment**, not from
  screen entry.
- **Kill phase:** when the kill timer fires, if kills shown = 0 it re-arms at 30 ms. Then kills
  shown += 1 and result −= 10 per kill. On reaching the kill total, kills shown = total, slot 11
  plays, and the counting ends. The kill tick plays on every step. With 0 kills there is still
  one step: one 18500-band tick, slot 11, and a result change of 0.
- **Keys (level, not edge):** while Esc or Enter is **held**: if counting, the numbers jump to
  final, slot 11 plays, and a 500 ms timer is armed. Once counting is done, a held key starts the
  fade after that timer expires (once).

Verdict: guess wrong on the kill start (1500 ms after the points finish), the 0-score and 0-kill
steps, the slot-11 end sound, and the held-key skip. RING_A (49) is right.

### Name entry: letter case
`crates/thefly-sim/src/highscores.rs:65`

Guess: lowercase letters.

Original (key table `FUN_0040f8d9`, key read `FUN_0040fc61`, entry `FUN_0041a8d9`) [C]: the table
maps DirectInput scancodes to lowercase `a`..`z` and `1`..`0`. Each stored character then goes
through CRT `toupper` (`FUN_00450620`), so names are stored and shown in **uppercase**. One
character is read per tick, from the last key event.

Verdict: guess wrong. Uppercase.

### Name entry: Space, Esc, Enter, length
`crates/thefly-sim/src/highscores.rs:118` (O17)

Guess: Space and Esc are ignored; the port's limit is 11.

Original (same functions) [C]:

- **Space** (DIK 0x39) types a space character. It counts toward the length like a letter.
- **Esc** (DIK 0x01) maps to the ignore code: it does nothing.
- **Backspace** (DIK 0x0e): position −1 (not below 0), and that cell is overwritten with a space.
- **Enter** (DIK 0x1c) commits **only when at least one character has been typed**. With an empty
  name it is ignored.
- **Length:** a character is stored only while the position is < 10, so names hold at most
  **10** characters.
- Other keys are ignored.

Verdict: guess wrong on Space (it types a space) and on the empty-Enter and 10-character rules.
Esc ignored is right.

### Score file trailer when the score sum is below 1
`crates/thefly-sim/src/saves.rs:69`

Guess: skip the trailer check when the sum is below 1.

Original (validator `FUN_00410ad3`, checksum `FUN_00410a69`, startup check `FUN_004101f9`) [C]:

- Every non-zero score must have checksum == c(score), compared as exact floats.
- Then the trailer must equal c(sum of all scores), **always**, with no special case for
  sum < 1. The writer stores R(8000) / 6.8899 when the sum < 1, so such a file fails validation
  (unless the random value happens to equal c(sum)).
- On failure the startup check fails, a "File Error" message box shows, and the game does not
  start.

Verdict: guess wrong as a model of the original, which has a self-bricking bug here. Keeping
the port tolerant is a reasonable port decision; if so, say so in the comment instead of
"the original doesn't say".

### Credits: end and rand
`crates/thefly-sim/src/menu.rs:119`

Guess: five pairs of 7000 ms each, then back to the top page; the slide is cosmetic.

Original (menu input `FUN_0041be05` pages 7/8/9, slide `FUN_0041eb60`, credits text in the menu
render around the +0x35f8 pair index) [C]:

- **The credits never end on their own.** After the fifth pair, the pair index goes from 10
  back to 0 and the text re-arms at 7000 ms, so the pairs loop. In credits page 9 the only input
  read is the Esc press edge. It starts the slide-out (page 8), and then the menu returns to the
  top page (0). Enter does nothing.
- **Rand:** the two scene sprites (menu +0x3310 at x 100 and +0x3480 at x 400, both at y −300)
  have wander turned on during the slide. Their parameters: reroll 500, speeds 30/30, run caps
  8/6, accel 0 while sliding, then 5. So the credits consume rand through the shared Wander
  rule for as long as they are shown.

Verdict: guess wrong on the end (loop until Esc). If the port keeps one rand stream from the
menu into the match, the Wander draws matter too.

### Object fan push (round 2)
`crates/thefly-sim/src/world/objects.rs:242`, `crates/thefly-sim/src/world/objects.rs:614`

Guess: the fan does not push objects; the carried object's push is skipped.

Original (manager `FUN_0042c888`, push `FUN_0042754d`, outline test `FUN_0042f6de`) [C]:

- **Manager order.** Pass 1 walks the objects in ascending index. For each active object it
  runs the landing dust and the list building, then the motion (`FUN_0042eb13`) unless the
  object is held.
- Pass 2 walks a list built by prepending, so the order is descending index. It visits only
  objects that are moving (state +0x78 ≠ 0) and collectible (+0x92 > 0). For each it runs the
  object–object step (`FUN_0042ec96`), then the fan push when there is at least one fan.
  - A resting object is never pushed.
  - FW §5.5's "weight > 0" gate is wrong.
- **Hit test.** Fans are tried in fan order with the pixel-outline test. The object's outline
  (at its screen x, y and frame size) is tested against the fan's blade outline: `palesventi_dt`
  or `palesventi_gc`, loaded into fan +0x9c by `FUN_00429605`, at the fan's screen x, y and frame
  size. The first fan hit ends the loop.
- **Object on the table (mode 0)**, on a hit:
  1. If fan x ≥ wrap − fan w, then fan x −= wrap.
  2. s = −1 when object x + w/2 ≤ fan x + fan w/2, else +1. The push is away from the fan's
     centre, whatever the fan type or cone.
  3. vx = (|vx| + R(60) + 90) × s.
  4. drop vx (+0x7c) = (drop vx + 50 + R(70)) × s, with no absolute value.
  5. vy = −(vy + R(60) + 90), with no absolute value.
  6. One more draw, R(100) < 50, picks a sign that is never used.
- **Carried object (mode 1)** runs inside the carry step, after `FUN_0042f3b1` and before fly vy
  += weight. On a hit, fly vx = −(vx + 200·s), where s = −1 when vx < 0, else +1. Fly vy gets the
  same rule with its own sign.
- Both modes then play sound 45 at R(6000) + 20000 Hz.
- Draws on a hit, in order:
  - table object: R(60), R(70), R(60), R(100), R(6000)
  - carried object: R(6000)

Verdict: guess wrong. Objects bounce off the blades: there is no field, decay or cone for
objects. The port should also take the two-pass order. It currently runs dust, interaction and
motion together per object, in ascending order.

### Fan stands, fan y and trap x (round 2)
`crates/thefly-sim/src/world/objects.rs:219`, `crates/thefly-sim/src/world/traps.rs` (`Traps::from_layout`)

Guess:
- stand: size from the outline extent, y = 580 − h
- fan: y = stored 600 − h
- trap screen x = trunc(trunc(stored / 1.25) × 1.25)

Original (trap build `FUN_0042699d`; stands `FUN_00427954`, called from `FUN_0042a85a`) [C]:

- **Fan y** = 570 − fan frame h. Flypapers and zappers get y 0. The stored y is never read.
- **Trap x:** the stored x goes straight to the screen-x setter, so screen x = stored and layer
  x = trunc(stored / 1.25). This holds for all three kinds. The first mover step then gives
  trunc(layer × 1.25) [I], so the port's value differs only before that step.
- **Stands** are built only when there are 1..5 fans, one per fan in fan order:
  - size = the fan's frame w × h
  - screen x = the fan's screen x
  - y = the fan's y
  - layer 1.25, wrap 4000 × 1.25, active
  - collectible 0, which makes `FUN_0042db36` clear held, state, v and drop vx
  - outline `piedventi_dt` when the internal fan type is 1 (blows right), else `piedventi_gc`

Verdict: guess wrong on the stand size and y, and on fan y (570 − h). The outline choice
(`_dt` for a right-facing fan) is correct.

### Fan blow field: type-2 unwrap (round 2)
`crates/thefly-sim/src/world/traps.rs:207`

Guess: add the wrap when the cone's base is past it and the point is left of the wrapped base.

Original (`FUN_00429cbc`) [C for the structure; I for the exact float compares]:

- Type 2 (blows left) unwraps when fan x + 222 ≤ 500 (the +0x91c reach) and the point x ≥
  wrap − point w. Then cx = point w / 2 − |wrap − point x|, which is the point centre moved by
  −wrap.
- Type 1 computes |wrap − fan x| when fan x ≥ wrap − 500 and cx < 800, then discards it. So
  type 1 has no unwrap.

Verdict: guess wrong on the trigger: a fan near the left screen edge and a point in the last
point-width of the ring, with the point moved instead of the base.

### Banner: the whole world update runs, paused (round 2)
`crates/thefly-sim/src/screen.rs:216` (O16)

Guess: only the render-driven fly step (18b) runs during the countdown.

Original (game update `FUN_0040326c`, pause `FUN_00403de0`) [C]: while the banner is up (game
+0x408ac ≠ 0), every fly and farmer key array and the H/F action keys are zeroed. Then the full
update, steps 1–17, runs. The pause (game +0x407bc = 1) gates only these:

- the AI tick `FUN_0043f611` (AI +0x3c)
- boot update (step 7) and boot collision (step 10)
- the trap manager `FUN_00427270`, which is called with active = 0:
  - trap motion and the loop volumes still run
  - flypaper `FUN_0042663b`, fan hit and blow `FUN_0042a075`, and the zapper/fly step
    `FUN_004288fe` are skipped
- the clock, which is frozen

Still running, and drawing rand:

- fly physics `FUN_00437547` (jitter) and fly update `FUN_00433ddd`
- objects
- midges, including the ungated arc test (`FUN_004278f3` → `FUN_00428b47`) and the fan field
  `FUN_00427885`
- flying goodies: the spawn `FUN_00432684` runs on a real-time timer (+0x3a8) and rolls R(100)
  on expiry
- animal sounds `FUN_00441b3c`: R(150) every tick
- farmer voices `FUN_0043c730`
- the tutorial `FUN_00443772`

Verdict: guess wrong. Run steps 1–17 with zeroed input and the pause gates above.

### Shop tally: target, food sizes and machine shake (round 2)
`crates/thefly-sim/src/shop.rs:53`, `crates/thefly-sim/src/shop.rs:56`, `crates/thefly-sim/src/shop.rs:5`

Guess:
- target y = 470 (money panel)
- food sprites 100×75
- the machine wanders every tick

Original (tally `FUN_0042322f`, shop loop and build around `FUN_00422571`) [C]:

- **Target.** The tally falls onto the `g_Machine_cache` sprite (+0xc38). Every shop tick it is
  placed at machine x + 46, machine y + 156. The machine (`g_Machine`, +0xac8) rests at (7, 143),
  so the target y is 299 at rest.
  - The drop ends on the first tick where the food's top y ≥ the target's top y. That test runs
    only once food x + w < 372.
  - The target's position is set after the tally runs, so the tally compares against the
    previous tick's position.
- **Machine shake.** The machine's wander flag is off until a tally landing, and each landing
  does three things:
  - turns wander on
  - starts the dust effect (+0xca0)
  - arms a 4000 ms timer (+0x24a0)

  When that timer expires, wander goes off, the machine snaps back to (7, 143) and its v is
  zeroed. While it shakes:
  - Wander parameters: reroll 20 ms, speed ranges 140 (x) and 150 (y), run caps 2 and 4,
    accel 5.
  - The y bounds are 139..147, so the target y is 295..303.
  - The machine draws rand only inside these 4000 ms windows (back-to-back kills keep
    re-arming the timer).
- **Food size.** The image's full size is used as one frame, then scaled uniformly to fit:
  - s = 1, or s = 150 / h when h > 150.
  - That is overridden by s = 200 / w when w > 200 and w > h.
  - s is a float32, and size = trunc(w × s) × trunc(h × s) with the product in x87 precision.

  The dead fly (`g_flymort`) stays 100×75 with frame R(3) + 1. Start: x 805, y 239 − scaled h.
  The scaled food sizes, from the assets (`irc_dims.py`):

  | type | image | tally size |
  |---|---|---|
  | can1 | 152×217 | 105×150 |
  | can2 | 220×135 | 200×122 |
  | can3 | 148×107 | 148×107 |
  | chips1 | 160×48 | 160×48 |
  | chips2 | 160×49 | 160×49 |
  | clope | 180×43 | 180×43 |
  | saucisson | 340×157 | 200×92 |
  | saucisson1 | 140×60 | 140×60 |
  | cacahuete | 107×53 | 107×53 |
  | zippo | 140×160 | 131×150 |
  | lait | 209×300 | 104×150 |
  | mandarine | 160×132 | 160×132 |
  | pomme | 201×182 | 199×181 |
  | sucre | 100×50 | 100×50 |
  | toaster | 468×270 | 200×115 |
  | toast | 240×76 | 199×63 |
  | poulet | 280×109 | 200×77 |
  | poivriere | 118×171 | 103×149 |
  | tomate | 180×165 | 163×150 |
  | muffin | 164×154 | 159×149 |
  | cornichon | 134×61 | 134×61 |
  | mais | 430×130 | 200×60 |

Verdict: guess wrong on all three:
- the target y is 299, or 295..303 while the machine shakes
- food sprites are the scaled image sizes above
- the machine wanders only in the 4000 ms after each landing

### Zapper arc on the fly: death, sparks, sound and burst order (round 2)
`crates/thefly-sim/src/world/fly.rs:450`, `crates/thefly-sim/src/world/fly.rs:353`,
`crates/thefly-sim/src/world/traps.rs` (`zapper_hits`)

Guess:
- an arc over 100 runs the zapper grill, sound 43 included
- the grill is the common splat, then 40 extra particles, then 100 R(100) fade draws
- the grill check runs over all zappers before any arc

Original (zapper/fly step `FUN_004288fe`, run per zapper in index order from `FUN_00427270`
after that zapper's own update `FUN_0042ea74`; burst `FUN_00439f29`; splat `FUN_004397c0`) [C]:

For each zapper, only while fly +0x2a4, +0x2a8 and +0x288 are all 0 at entry:

1. Grill column: fly x ≤ zapper x + zapper w − 60 and fly x + fly w ≥ zapper x + 60. This is
   a span overlap, not the port's centre-in-span test.
   - Before this test, a zapper with x ≥ wrap − zapper w is moved left by wrap (round 3).
   - Outside the column, the solid step `FUN_00428003` runs instead (round 3: "Zapper and
     flypaper: solid push-out").
   - Inside, if fly y ≤ zapper y + zapper h − 80: the burst `FUN_00439f29(50, 80, 50, 100, 1,
     1, 1, 13)`, then sound 43. There is no return.
2. The arc test for this same zapper still runs on the same tick, with target (fly x, fly y + 45,
   fly w). The entry gate is not tested again after the burst (checked in the disassembly: no
   flag compare between the burst call and the arc-test call). So a grilled fly can still take
   an arc, its sparks and a second burst on that tick.
3. If an arc starts:
   1. fly vy −= 180
   2. 10 sparks (`FUN_00428ffc`) at (trunc(arc x + arc w / 2), arc y + arc h). Each takes up to
      10 free slots of 30 and makes 8 draws: R(5), R(5), R(frames + 1), R(10) < 6 sign,
      R(181) + 70, R(10) < 4 sign, R(181) + 70, then R(60) + 100 ms life.
   3. damage +1
   4. If the total is ≤ 100, +0x314 is set. Above 100, the same burst runs
      (`FUN_00439f29(50, 80, 50, 100, 1, 1, 1, 13)`), with no sound 43.

Inside the burst, in this order:

1. Set +0x2a8 (zapper death).
2. 40 extra particles. Each makes 8 draws:
   - x = R(fly w − 20) + fly x
   - y = R(fly h − 20) + fly y
   - frame R(frames − 3) + 4
   - R(10) < 6 picks the x sign; vx = (R(51) + 50) × sign
   - R(10) < 4 picks the y sign; vy = (R(101) + 80) × sign
   - accel = 7 + R(6), from k = max(1, trunc(13 / 2.0)) = 6 and 13 − k + R(k)
3. The common splat: 60 blood particles (frame R(frames − 3) + 4, because +0x2a8 is set), then
   11 pieces, then sound 46.

No fade lengths are drawn at death:
- the 40 extras' fades (R(100) + 850 ms) are drawn once, at fly construction (`FUN_00435cd4`)
- the 60 common fades are drawn at construction and again, 60 × R(100), only when the crash
  respawn timer +0x2e0 expires in the fly update `FUN_00433ddd`. That timer is armed only by
  the crash path, so a splat death never re-rolls them.

Verdict: guess wrong in four ways:
- the arc death has no sound 43
- the sparks and their draws come before the death
- the 40 extras come first, each with an R(6) and no R(100)
- the grill and the arc run per zapper, with no early return

The death kind (the zapper burst) is correct.

### Object side test `FUN_0042fb1c` (round 2)
`crates/thefly-sim/src/world/objects.rs:287`, `:324`, `:560`

Guess:
- after the slide bounce, v is left as is
- a falling object lands when its bottom is in k's upper half, stepping up to settle
- a blocked attach pushes the object away from k's centre

Original [C]:

**The probe.** `FUN_0042fb1c(k, d, mode)` returns three flags against object k (pixel-outline
test, `FUN_0042f6de`):
- "right": still touching with the object's layer x moved by +d (screen x recomputed)
- "left": still touching at −d
- "below": still touching with screen y moved by +d

Mode 0 probes both right and left. Mode 1 probes only the side vx points to (none when vx = 0),
and probes below only when vy ≠ 0.

**The touching list** (`FUN_0042fe10`):
- For each candidate the object rests on (its support list +0x88), the object is moved down 1 px
  for the test.
- The last such candidate is the support k.
- "Still supported" means it overlapped k after the move. Supports are not added as touches.

**Sliding (state 1, `FUN_0042ec96`).** w = trunc(weight / 3), from gaps §3.
1. If anything touches: restore the previous position, push the picked object
   (`FUN_0042e28a` mode 1) if it is collectible, and bounce:
   - vx < 0: vx = −10w − vx, and stop if the result is < 1
   - vx > 0: vx = 10w − vx, and stop if the result is ≥ 0
2. Then, with a support k:
   - Not still supported: start falling (state 2), leave the support lists (`FUN_0042e5a3`) and
     set drop vx.
   - Still supported: probe k in mode 1 with d = 1.
     - Only right is set: vx = 10w − vx, and stop if the result is ≥ 0.
     - Only left is set: vx = −10w − vx, and stop if the result is < 1.
     - Neither is set: nothing changes.

**Falling (state 2).**
- Nothing touches and y ≥ floor y (+0x1c): y = floor y and the object lands (state 1, landed).
- Otherwise:
  1. Pick k and probe it in mode 1 with d = 1, then restore the previous position.
  2. If no flag is set, retry with d = 2, 3, … up to 10 more times. If still none, force "left
     only".
  3. Below only (no side flag): the object lands on k. It goes into k's list and k into its own,
     state 1, vy = 0.
  4. Any side flag, with or without below: k is pushed (mode 2) if collectible, then drop vx and
     vx are negated and vy = 0.

**Blocked attach (`FUN_0042e704`).**
1. The object is placed at grip height. If anything touches, it goes back to its y and k is the
   first touching object.
2. Probe k in mode 0 with d = 5. p = trunc(weight × 100 / 20).
   - right is set: vx = −p, state 1.
   - else left is set: vx = +p, state 1.
   - neither is set: no push.

Verdict: guess wrong on all three:
- sliding has a second bounce against the support, plus the fall-off transition
- landing is decided by the probes, not by the upper half; the retry widens the probe, and the
  fallback is a side hit
- the attach push follows the ±5 probes and can be none

The magnitude weight × 100 / 20 is right.

### Crash animation length (round 2)
`crates/thefly-sim/src/world/fly.rs:33` (O1)

Guess: 1000 ms placeholder.

Original (dying in `FUN_00437e99`; crash sprite +0x48 built in `FUN_00435cd4`; tween
`FUN_0041787c`; end test in the fly step near the +0x2e0 arm) [C]:

- The crash is the +0x48 sprite: 339×244, 1 frame. It is placed at the fly's x, y and started
  with a scale tween from 0.3 to 1.0 over 200 ms, with alpha held at 255.
- The tween advances in the fly render pass: each frame the scale goes up by 0.7 / (fps × 0.2).
  It ends on the frame the scale passes 1.0, so about 200 ms plus up to one tick. Round 3 [C]
  (`FUN_0041787c`): d = float(fps × 200 / 1000.0), step = float((1.0 − 0.3f) / d), the scale
  accumulates in float32 and ends when it is strictly above 1.0. That is 13 render frames at 60
  and at 65 fps (14 at 70, 15 at 75, 20 at 100). With the hit on update T, renders T..T+12
  advance the tween, and the end step runs on update T+13 and arms the 1500 ms respawn.
- When the tween has ended (and +0x288 is set, +0x2b0 clear), the fly sets +0x284 = 3, zeroes v,
  sets +0x2b0 and arms the 1500 ms respawn timer +0x2e0.
- FW §3.6's "falls toward (850, 850)" misreads `FUN_0043a158`. That call is a 60-particle blood
  spray with speed ranges 0..850, not the crash motion. Each particle makes 8 draws:
  - one discarded rand (`FUN_00436d4f` mode 0, which also sets a 200 ms scale-in)
  - x = R(fly w − 20) + fly x
  - y = R(fly h − 20) + fly y
  - frame R(frames − 3) + 1
  - R(10) < 6 picks the x sign; vx = R(851) × sign + fly vx
  - R(10) < 4 picks the y sign; vy = R(851) × sign
  - accel 0

Verdict: guess wrong: the crash lasts 200 ms (render-driven), then 1500 ms to the respawn. The
port's `start_dying` also lacks the 60-particle spray and its 480 draws.

### Period load order (round 2)
`crates/thefly-sim/src/world/mod.rs:131`, `crates/thefly-sim/src/world/mod.rs:152`

Guess: traps, placement, fly respawn, AI, then the ambient midges last.

Original (`FUN_004015c8`) [C]. The order:

1. loading backdrop R
2. clock
3. animal sounds object
4. fly (`FUN_00435cd4`, `FUN_00436df6`, which calls srand(time))
5. farmer
6. AI (`FUN_0043edb1`)
7. ambient midges (`j_moucheron`, with their draws)
8. goodie sprites
9. flying goodies (`FUN_004321f8`)
10. boot (`FUN_0043a958`)
11. traps (`FUN_0042699d`)
12. objects (`FUN_0042a85a`), in this order:
    1. winch constructor `FUN_00430e48` (srand(time))
    2. placement `FUN_0042ac27` or `FUN_0042bdf8` (srand(time) again)
    3. fan stands
13. a 500 ms wait
14. fly re-roll loop (`FUN_004385c6` while the fly overlaps an object, at most 3000 times;
    amended in round 3, it is not unbounded)
15. fly reset(0)
16. tutorial
17. banner timer 1000 ms, sound 52, pause

Because the generator is reseeded from the clock at steps 12.1 and 12.2, every draw before them
is on a seed the port cannot reproduce. Only the order from placement onward could matter, and
the port already has placement → stands → fly re-roll.

Verdict: guess wrong on the order, but immaterial; see the port decision for these lines.

### FPS integrator: per-axis runs and percent rounding (round 3)
`crates/thefly-sim/src/mover.rs:10` (`FpsRun`), and through it `crates/thefly-sim/src/bonus.rs:129`

Guess: one run shared by both axes. It restarts when v changes or after FPS frames, and each
frame moves |v| div FPS plus a Bresenham carry of the remainder.

Original (`FUN_004180af`) [C]. y is processed first, then x. Each axis keeps its own frame count
n, moved total m and last v. Per axis, only when |v| > 0 and the frame rate F > 0:

1. If F ≤ n, or v differs from that axis's last v: n = 0 and m = 0. Store v as the last v.
2. r0 = trunc(100 × (|v| mod F) / F), the target remainder in percent.
3. q, r = divmod(|v| − m, F − n), then r1 = trunc(100 × r / (F − n)).
4. step = q + 1 when r1 > r0, else q. Move by sign(v) × step, then m += step and n += 1.

F is the game's measured frame rate (game +0xb8), not a constant. A change in vx does not
restart the y run, and the reverse also holds.

Example, 600 px/s at F = 65: the original moves 9 px a tick, with 10 at ticks 4, 8, 12, 16, 21,
25, 29, 33, 37, 41, 46, 50, 54, 58 and 62. The port's carry gives 10 at ticks 5, 9, 13, 18, 22,
26, 31, 35, 39, 44, 48, 52, 57, 61 and 65. Both sum to 600, but the positions differ tick by
tick.

This settles the bonus coin split: the coin layer x moves by −trunc(bar step), so −9 or −10 on
the ticks above, and screen x = trunc(layer x × 1.25).

Verdict: guess wrong on the rounding rule and on the shared reset. The fixed F = 65 is a separate
matter (see Port decisions).

### Shop machine shake: only after a landing (round 3)
`crates/thefly-sim/src/shop.rs:5`

Guess: the machine runs Wander every shop tick; its parameters are not documented.

Original (shop loop around `FUN_00422571`, tally `FUN_0042322f`) [C]. Round 2 ("Shop tally:
target, food sizes and machine shake") already has the parameters: reroll 20 ms, speed ranges
140 (x) and 150 (y), run caps 2 and 4, accel 5. The bounds are on: right x ≤ 11 and y in
[139, 147]. The left bound 3 is never applied (the Wander clamp skips the left x bound). Wander is
on only for the 4000 ms after each tally landing. When that timer expires, the machine snaps to
(7, 143) with v = 0. The tally target is placed at (machine x + 46, machine y + 156) after the
tally step.

Verdict: guess wrong. The machine draws no rand outside those 4000 ms windows.

### Bonus round: target right bound and the tick order (round 3)
`crates/thefly-sim/src/bonus.rs:504`

Guess: the right bound (wrap + 100) is not applied, and the target drift runs after the spawns
and before the scroll.

Original (setup `FUN_004452a1`, update `FUN_00446030`, draw pass with items `FUN_004472df`) [C]:

- **Bounds.** The targets' 1.0 layer has wrap 4000, so the right bound is 4100. The clamp keeps
  x + w ≤ 4100, i.e. x ≤ 4052 for the 48 px target. The left bound (−100) is never applied, as
  in every Wander clamp. Top = bg y + 15 and bottom = bg y + bg h − 15.
- **Update pass** (`FUN_00446030`), in order:
  1. the end-of-round check
  2. the layer scroll
  3. the fly mover
  4. the swatter mover, or the swing while swinging
  5. the swatter-on-fly test
- **Draw pass**, in order:
  1. the layers
  2. the swatter
  3. outside the countdown only, `FUN_004472df`:
     1. coin spawn (the 50 ms firing)
     2. target spawn: y = bg y + R(bg h), then x = R(150) + 810; launch, then the parameter
        override (reroll 500, speeds 1 and trunc(500 × h + 1), run caps 6 and 3, accel 5)
     3. targets in slot order: lifetime check, mover (Wander, then the FPS integrator with the
        camera delta), wings, then the swatter hit test
     4. coins in slot order: mover, removal when off screen, then the pickup with
        R(500) + 21500 Hz
  4. the fly

So the target drift, the coins and both hit tests run in the draw pass, after the whole update.
The spawns come before the movers within that pass.

Verdict: guess wrong. The bound applies (x ≤ 4052), and the order is update (scroll, fly,
swatter), then draw (spawns, targets with their hits, coins with their pickups). It is not the
port's drift → scroll → collect → swat.

### Winner trophy: mover before the hop timers (round 3)
`crates/thefly-sim/src/winner.rs:188`

Guess: the mover steps after the choreography, so a hop's first Wander draws follow the cheer's.

Original (`FUN_0044b431`) [C]: each tick the trophy's mover (Wander when on, then the FPS
integrator) runs first. Only then does the choreography test the rest timer and the hop timer.
So the hop that a rest expiry turns on gets its first Wander draws on the next tick, after that
tick's cheer draws. A hop expiry zeroes v after that tick's move.

Verdict: guess wrong on both edges. The move comes before the timer checks in the same tick.

### Credits: slide-in, page 9, slide-out and Esc (round 3)
`crates/thefly-sim/src/menu.rs:500`

Guess: Esc returns to the top page at once; the slide-out is not modelled.

Original (menu input `FUN_0041be05` pages 7/8/9, slide `FUN_0041eb60`) [C]. The page state
machine runs in the input step. The render step then moves the layers, then the photos (seb,
then gaga).

- **Page 7 (slide-in).**
  - Layer vy starts at 520 and drops by 5 per tick.
  - The slide stops when layer 0's y ≥ −200 or vy ≤ 0. At 60 fps that is tick 86 (y −200,
    vy 100); at 65 fps tick 106 (y −228).
  - Photos: Wander parameters 500 / 30 / 30 / 8 / 6, accel 0, accel flags x 0 and y 1. They
    start at (100, −300) and (400, −300).
  - The letterbox does not grow in page 7. Esc is ignored.
- **Page 9 (shown).**
  - Photo bounds: right 820 (x ≤ 620), top = letterbox − 20, bottom = 620 − letterbox.
  - The letterbox grows by 10 per tick up to 120.
  - The text pairs cycle as in "Credits: end and rand".
  - The Esc press edge goes to page 8. Nothing else is read.
- **Page 8 (slide-out).**
  1. On entry: layer vy = −20, the photos' y accel flag set with accel 5. The letterbox
     shrinks by 10 per tick to 0.
  2. When layer 0's y + h < 601: the layers snap to 600 − h (0), +450 (1), +610 (2) and
     600 − h3 (3 and 4). Sound 6 plays at volume 300.
  3. Then, if |vy| < 61: stop, photos off, page 0 (top). Otherwise vy = −(vy / 2), a bounce.
  4. The layer accel is −5 in the first stage and −40 after the first bounce.
  Esc is ignored in page 8.

Verdict: guess wrong. Esc in page 9 starts the slide-out, and Esc is ignored in pages 7 and 8.
The menu reaches the top page only when a bounce's |vy| drops below 61.

### Midges: spawn x, homing gates, ambient update and constructor (round 3)
`crates/thefly-sim/src/world/midges.rs:137`, `:196`, `:238`, `:350`

Guess:
- :137, the goodie midge's x is the raw rand wrapped at the fly layer's W
- :196, the ring q is undefined, the first speeds come from R = 0, and a stunned or falling midge
  draws, then keeps its v
- :238, the push is kept apart from the jitter and their sum moves the midge
- :350, each ambient midge is placed by the respawn draws and starts at rest

Original [C]:

- **Goodie spawn (`FUN_00404caf`).**
  - x = R(W − 810) + 800, where W is the scene width (game +0x1a). It is not raw and not
    wrapped.
  - Then, as in round 1: R(10) sign; |vx| = R(200) + 200; size R(5) + 30; y = R(600); timer
    R(500) + 500; y tol = R(30); x tol = R(30); home y = R(farmer h − 60); home x =
    R(farmer w).
  - Accel 0 with accel flags x 1 and y 0. x never re-rolls under Wander.
- **Homing (`FUN_00442f95`)** runs only when the midge is neither stunned nor falling. A stunned
  or falling midge makes no homing draws.
  - R(100) > 50 sets sx = R(200) + |v0| and sy = R(60) + |v0 − 180|. Otherwise the speeds stay
    as they are. There is no seeded speed before the first re-roll.
  - The x steer is gated by the ring q as decoded in round 1, with no tolerance. y uses the y
    tolerance.
- **Goodie midge tick order:**
  1. retarget check R(10) < 3
  2. contact
  3. homing
  4. the retarget draws
  5. the fan push `FUN_00427885`: save v, then add ±180 (+180 when v > 0, else −180)
  6. update `FUN_00442960`
- **Jitter.** It runs once per timer expiry, not every tick, and only for ambient midges, with
  R(100) > 70. The fan push decays by 40 per tick toward the saved velocity, so the push and
  the jitter act on the same v. They are not two terms that get summed.
- **Ambient tick order:** arc test, contact, update, then the fan push.
- **Ambient constructor.**
  - Slot 0 first: x 900, vx +60, size R(5) + 30, y = R(600), timer R(500) + 800.
  - Then count = R(15). For each slot 1..count − 1: x = R(W), R(10) sign, |vx| = R(30) + 45,
    size R(5) + 30, y = R(600), timer R(500) + 800.
  - Parameters 100 / 10 / 50 / 50 / 30 / 0, bounds x −1000..9000 and y 0..600.
  - So there are max(count, 1) midges, all moving from the start.

Verdict: guess wrong on all four lines.

### Zapper and flypaper: solid push-out (round 3)
`crates/thefly-sim/src/world/traps.rs:460`; amends "Zapper arc on the fly" (round 2)

Guess: the trap step has no solid contact. The flypaper drag uses the full fly rectangle with a
strict overlap.

Original (zapper/fly step `FUN_004288fe`, solid step `FUN_00428003`, flypaper `FUN_0042663b`,
box test `FUN_0042825c`) [C]:

- **Column wrap.** Before the grill column test, if zapper x ≥ wrap − zapper w, then
  zapper x −= wrap. Outside the column means fly x > zx + zw − 60, or fly x + fly w < zx + 60.
- **Solid step, run outside the column.** The trap's outline is tested against the fly's
  rectangle outline. On a hit:
  1. The trap goes back to its previous x, y (+0x10, +0x14), and its screen x is recomputed.
  2. The fly goes back to its previous x, y (+0x1d4, +0x1d8).
  3. Up to 1000 times while they still overlap: step the fly's x by 1 against sign(vx) and
     retest. If vy ≠ 0 and they still overlap, step y by 1 against sign(vy) and retest.
  4. Then vx = trunc(−vx / 1.4) and vy = trunc(−vy / 1.4).
- **Flypaper.** `FUN_0042663b` runs the same solid step first. The drag then uses the inclusive
  box test with the fly box (x + 50, y + 25, w − 100, h − 25), and sets vx, vy = trunc(v / 1.4).

Verdict: guess wrong (missing behaviour). Zappers and flypapers are solid outside the zapper's
column, and the flypaper's catch box is the inset box above, inclusive.

### Lift: `moved` is cleared every tick (round 3)
`crates/thefly-sim/src/world/lift.rs:293`

Guess: Move sets `moved`, and only the end of the approach clears it.

Original (update `FUN_00431115`, move `FUN_00431826`, winch tick `FUN_00431931`) [C]:
- The update clears `moved` (+0x78) every tick after the state transitions. It clears it again
  on entering the state-2 body.
- Move sets it, and the approach end clears it.
- Move(1) in state 1 adds the acceleration: +7 when vy > 0, and +3 when vy < 0, clamped at 0.
  The port already has this.

So if the fly drops the object mid-approach, state 2 carries on without approaching, and `moved`
is false on those ticks. The port keeps it true and skips the ring-y step.

Verdict: guess wrong.

### Fly reset(0) and SuperFly sparkles (round 3)
`crates/thefly-sim/src/world/fly.rs:507`; settles `crates/thefly-sim/src/screen.rs:216`

Guess: argument 0 clears the dying and appearing state silently.

Original (`FUN_00438643`, `FUN_004387d5`, sparkles `FUN_00439450`, SuperFly `FUN_0043728f` and
`FUN_00437302`) [C]:

- **reset(0)** runs `FUN_004387d5`, in this order:
  1. clears the phase, dying, splat and appearing flags
  2. plays sound 38
  3. starts the survival stat (`FUN_0040db31`)
  4. forces a 45-sparkle burst, `FUN_00439450(0, 45)`
- **Each sparkle** takes a free slot of 90 and makes 7 draws: delay R(60) + 20; R(10) sign;
  x = R(w − 20) + fly x; y = R(h − 20) + fly y; vx = R(160) × sign; R(10) sign;
  vy = R(160) × sign.
- **The only caller with 0** is the period start (step 15 of "Period load order"). The port
  calls reset(true) there, an appear animation.
- **SuperFly.** Start (`FUN_0043728f`): 5 sparkles, sound 36 looped, the 8000 ms timer (+0x250)
  and a 40 ms timer (+0x9a50). While SuperFly is active, `FUN_00437302` emits 5 sparkles per
  40 ms firing. The port has neither.
- **screen.rs:216.** `FUN_00405538` is the banner draw and countdown, and the render pass runs
  normally during it. At its end, with music on, an even/odd rand picks track 0 or 3; then the
  tutorial starts and the game unpauses. Slot 38 and the survival start belong to reset(0) at
  period load, so once that fix lands, the port's start of both before the countdown matches.

Verdict: guess wrong. Argument 0 is not silent: sound 38, the survival start and 45 sparkles
(315 draws), and the period start uses it, not the appear path.

### Fly survival stat: the period end also stops it (round 3)
`crates/thefly-sim/src/world/fly.rs:537`

Guess: survival stops when the kill is recorded.

Original [C]: survival stops through `FUN_0040db52`, which adds the life to the total and
updates the maximum. It is called in two places:
- the kill record and respawn request `FUN_0043857d`
- the world destructor `FUN_00433904` at period end

Verdict: guess correct on the kill point. Wrong at period end: the port drops the life in
progress, but the original adds it.

### Fly buzz: pitch, pan and volume (round 3)
`crates/thefly-sim/src/world/fly.rs:675`

Guess: the R(1000) is drawn to keep the stream; the pitch and pan are open (O14); the first alive
tick starts the loop.

Original (`FUN_0040d005(x, vx, vy, w, −700)`) [C]. The rand is drawn first, on every call.
- **Pan:**
  - x ≤ 400 − w: −trunc((1 − x / (400 − w)) × 2000)
  - else x ≥ 400: trunc((x − 400) / 400 × 2000)
  - else: 0
- **Frequency:** 10000 + R(1000) + (vy × 1500) / 700 + (|vx| × 2500) / 700. Both divisions are
  C integer divisions, and vy is signed.
- **Volume:** −700 − off. off = |x| when x < 0, x − 800 when x > 800, else 0.
- The looped play starts whenever the slot is not playing.

Verdict: the loop start is correct. The pitch, pan and volume are now settled and differ from
the port's open-item stand-in.

### Fly hit: a new hit does not run B in the same call (round 3)
`crates/thefly-sim/src/world/fly.rs:754`

Guess: a new hit (A) runs B in the same call, so the hit frame reads 1 on the hit tick.

Original (`FUN_00437e99`) [C]: A and B are exclusive branches of one call. The hit frame stays 0
on the hit tick, and B first runs on the next call.

Verdict: guess wrong. The stun runs one tick later than in the port.

### Fly hits and damage stats: counted in the render, fatal hits excluded (round 3)
`crates/thefly-sim/src/world/fly.rs:775`

Guess: the damage sum takes the effective (SuperFly-cut) damage, counted at the hit (A).

Original (fly render `FUN_00439156`) [C]:
- The stat adds the effective damage (+0x2c8).
- The hits and damage stats are recorded in the fly render, when +0x314 == 1 and the fly is not
  dying. That flag is set:
  - at a swat stun's end
  - by a non-fatal zapper arc (damage 1)
  - by a fan hit: R(20) + 40, or R(10) + 20 with SuperFly
- A fatal hit is never counted.

Verdict: guess correct on the effective damage. Wrong on when and what: the port counts at A,
includes fatal hits and misses arc and fan hits.

### Fly heading machine: mid-turn reversals and opposite keys (round 3)
`crates/thefly-sim/src/world/fly.rs:896`, `crates/thefly-sim/src/world/fly.rs:946`

Guess:
- one frame per tick
- a turn-in finishes before it reacts, and a turn-out lands on the target held when it ends
- with opposite keys, Right and Up win

Original (fly update `FUN_00433ddd`) [C]. Flags: E +0x1ec, W +0x1f0, N +0x1f4, S +0x1f8, FACE
+0x1fc; frame +0x200; turn-in +0x1e4 and turn-out +0x1e8. It runs only while the lock +0x330 is
0, at one frame per tick.

It matches the exact key patterns none, R, L, U, D, RU, RD, LU and LD. Any other combination
(two opposite keys, or three or more keys) does nothing that tick: no flag or frame change.

**Single key** (target t, opposite o, perpendiculars p):
- Steady at FACE: frame 0, turn-in on, FACE off, t on.
- Steady with o set: frame 3, turn-out on.
- Steady with a p set: clear p, set t, frame 4.
- Steady at t only: frame + 1, wrapping past 9 to 4.
- Turn-in with t not set: switch to turn-out, frame − 1. Below 0: frame 0, turn-in, flags = {t}.
- Turn-in with t set: frame + 1. Past 4: frame 4, turn-in ends.
- Turn-out with t not set: frame − 1. Below 0: frame 0, turn-in, flags = {t}.
- Turn-out with t set: switch back to turn-in, frame + 1, capped at 4.

**Diagonal** (a horizontal, b vertical):
- Steady at FACE: start the turn-in with a and b on.
- Steady with either opposite set: frame 3, turn-out.
- Steady with b not set (only a): stay on a, looping. Quirk: E held with RU stays E.
- Steady with b set: a and b on, frame + 1, looping.
- Turning: as for a single key, with "both set" in place of "t set", and flags = {a, b} on
  underflow.

**No key:**
- Steady with no flags: FACE, frames 0..9 looping.
- Steady with a heading: turn-out, frame − 1.
- Turning: turn-out, frame − 1. Below 0: frame 0, flags cleared, FACE.

Verdict:
- :896 is correct on one frame per tick, and wrong mid-turn: a turn-in reverses at once when its
  target is released.
- :946 is wrong: opposite keys (and any three keys) freeze the heading for that tick, and E held
  with RU stays E.

### Extreme placement: sequential types and a shared try counter (round 3)
`crates/thefly-sim/src/world/objects.rs:197`

Guess: the §5.2 loop over the three extreme types, with the type drawn by R(3).

Original (`FUN_0042bdf8`) [C]:
1. All 22 type counts are set to −1, then types 0..2 to 0. Then srand(time).
2. A fixed 3-entry table, with sprite index, four sizes and stock:
   - entry 0: 0x89, 0x14, 0x87, 3; stock 3
   - entry 1: 0x6b, 0x14, 100, 2; stock 5
   - entry 2: 0x4a, 0x14, 0x46, 1; stock 7
3. The loop runs until 15 are placed or 2000 failures. The type is taken in order, not drawn:
   the index advances when the current entry's stock is below 1. So the order is type 0 three
   times, type 1 five times, then type 2 seven times. No R(3) is drawn.
4. y base 0x244 (580), so y = 580 − h.
5. x = R(trunc(width × factor)), with the same overlap, right-limit and fan checks as §5.2. A
   success decrements the stock and plays landing sound 6.
6. The x-try counter is reset only on a failure, not on a success. So successive successes share
   one budget of 1001 draws until a failure resets it.

The normal placement (`FUN_0042ac27`) resets its counter before every x loop. Its bound is 1001
draws, not the port's `PLACEMENT_X_TRIES` = 1000. That matters only on the fail path, where it
changes the rand consumed.

Verdict: guess wrong. There is no type draw, the types run 0 ×3, 1 ×5, 2 ×7, and the x-try budget
is shared across successes.

### Positional sound curve (O14) (round 3)
`crates/thefly/src/mixer.rs:14`, `crates/thefly/src/mixer.rs:17`

Guess: a linear pan from the screen centre reaching 2000 cb at the edges; off-screen sources lose
3 cb per px, down to −3000.

Original (`FUN_0040d1b2(slot, loop, vol, wrap, x, w)`, used for slots 39, 40, 42, 44, the winch
47 and the farmer voices; x is the source's screen x in [0, wrap)) [C]:

1. half = (wrap + 800) / 2.0. Then d (0..100) and a side:
   - 800 < x ≤ half: d = trunc((x − 800) / (half − 800) × 100), side −1.
   - half < x ≤ wrap − w: d = trunc((1 − (x − half) / ((wrap − w) − half)) × 100), side +1.
   - Otherwise (on screen, or past wrap − w): no pan and no volume change.
2. With a side:
   - d < 50: p = trunc(d × 100 / 50 × 30 × side), volume offset 0.
   - d ≥ 50: p = trunc((1 − (d − 50) / 50) × 3000 × side), volume offset =
     −trunc((d − 50) / 50 × 1500).
3. SetVolume(vol + offset), then SetPan(−p).
4. loop = 1: play looped only if the slot is not playing. loop = 0: play once.

So a source just right of the screen pans right, up to +3000 at d = 50. Further away the pan
returns to 0 at d = 100 while the volume drops by 1500 (it is "behind" the camera). Sources
wrapped to the left of the screen mirror this with a left pan. A source on screen is centred at
full volume.

Verdict: guess wrong on both constants. There is no on-screen pan, and the off-screen loss is the
d-curve above, at most 1500 cb, not 3 cb/px.

### Carried object collision: restore, one step back, then the fly (round 3)
`crates/thefly-sim/src/world/objects.rs:614`

Guess: on a collision the object goes back to its previous position before the fly is placed on
it.

Original (carry `FUN_0042f3b1`, resolve `FUN_0042ffc6`, touching list `FUN_0042fe10`) [C unless
marked]:
- **Placement.** The object is placed under the fly first, as in the port: screen x from the fly's
  centre minus grip x, wrapped when ≤ 0; y = fly y + fly h − (grip y + 14); v = fly v. Fly +0x1c4
  and +0x1cc are the fly's x and y (the respawn `FUN_004385c6` writes x = R(800 − w) there).
- **No touch, and y ≥ floor:** fly y = floor − fly h + off, and vy = trunc(−vy / 1.7 −
  (weight + 20)). The object is landed when |prev y − y| > 5. The object's own y is not clamped
  on this tick; the port's `o.y = o.rest_y` is not in the original.
- **Touch, resolve.** The object goes back to its previous position. Then:
  - v ≠ 0 and the stuck counter (+0x80, bumped per call while +0x74 is 0) is below 31: the
    restored position is retested against the touching list. If it still touches some k, the
    object steps 1 px against sign(vx) and 1 px against sign(vy), prev is set to it, and k is
    the result. Otherwise k is the first touching object. The loop allows 100 retests, but the
    position only changes on a hit, so it does at most one step.
  - The counter at 31 (reset to 0), or v = 0: `FUN_004303e5` runs with v forced to (0, 15) or
    vy to 5. Not read (open).
- **Then:** k is pushed (mode 3, the object's v and weight) if collectible. Fly x =
  trunc(object screen x (minus wrap when it straddles) + grip x − fly w / 2.0); fly y = object y
  − fly h + off; vx = trunc(−vx / 1.4); vy = trunc(−vy / 1.4 − (weight + 20)).

Verdict: guess correct on the restore and on the fly placement and speeds. It misses the one-px
step back when the restored position still touches, and the stuck-counter path. The port also
clamps the object's y on the table, which the original does not.

## Guess correct

### Boot spawns chasing
`crates/thefly-sim/src/world/boot.rs:102` (O23)

Original (`FUN_0043aabe`) [C]: +0x178 = 1 at spawn. Verdict: guess correct. (The spawn x is a screen
x; see the boot.rs:142 entry.)

### AI timers armed at creation
`crates/thefly-sim/src/world/ai.rs:380`

Original (`FUN_00440068`) [C]: Triple idle (+0xd8), Nuclear wait (+0xe0) and the boot interval are
armed at creation, as are the wander timer (+0x64), frenzy (+0x6c + +0x70, no rand) and chase
(+0xf4). Verdict: guess correct.

### AI divisor is a float division by 2.0
`crates/thefly-sim/src/world/ai.rs:756`

Original (`FUN_00440068`, at 0x4400b6) [C]: float division by 2.0. The level divisor is data +0x54
(a short) and the trap divisor is the sum of data +0x29c, +0x29e and +0x2a0. Verdict: guess
correct, provided the port's constants 5 and 14 equal those data values.

### Farmer pulse timer
`crates/thefly-sim/src/world/farmer.rs:431`

Original [C] (round 3): the farmer setup `FUN_0043c5f2`, called from the period build, creates
the pulse timer (+0x120) with period 2000 and its deadline at build time (timer class
`FUN_004193df` / `FUN_00419506`). It is never armed before the first pulse (`FUN_0043cd23`), so it
reads as expired there. Verdict: guess correct.

### Extreme draw order
`crates/thefly-sim/src/match_state.rs:288`

Original (`FUN_0040b7b2`) [C]: from period 0, the meal is drawn first (R(3): 0 breakfast, 1 lunch,
2 dinner), then the side by parity (even = 1, inside; odd = 2, outside), day = 1. The winch slots
are not drawn in this function, so they come after both. Verdict: guess correct.

### Shop AI: cursor to "done" on the expiry after the last press
`crates/thefly-sim/src/shop_ai.rs:127`

Original (AI step `FUN_00441414`, read-out `FUN_004414ea`, resume `FUN_0044151b`) [C]. On each
expiry of the 700 ms auto-re-arming timer, the AI skips item ids whose plan is < 1 until it finds
one with plan ≥ 1 (press it, plan − 1) or reaches id 6. Reaching id 6 moves the cursor to "done"
without pressing (read-out 10). From then on the timer is checked without re-arm; on the next
expiry (700 ms later) the AI's done flag (+0x28) is set and the shop presses done. So "done" is
selected on the expiry after the last press and pressed one expiry later. Verdict: guess correct.

### Triple swing skips the frequency draw
`crates/thefly-sim/src/world/farmer.rs:278`

Original (`FUN_0043d510`) [C]: on swing frame 1, with Triple off (+0xdc == 0) slot 27 plays at
R(15000) + 35000 Hz; with Triple on it plays at 40000 Hz and no rand is drawn. Then R(100) < 20
plays voice 34 (always drawn). Verdict: guess correct.

### Carried object's kind is the price index
`crates/thefly-sim/src/world/ai.rs:793`

Original (`FUN_0043f930` → `FUN_00440830`, price `FUN_0040b9d8`) [C]: the carried object's type is
the object table's per-object type (+0x806[i]), and the price is the active food table at that
index (22 normal types, 3 in extreme), 0.0 when out of range. The value is only compared when
the fly is carrying (reason 1, price > 70.0). Verdict: guess correct, provided the port's `kind`
is the active type-table index (its doc says so).

### Placement test against fans (O22)
`crates/thefly-sim/src/world/traps.rs:252`

Guess: only a fan's span blocks, inclusive, using the stored width.

Original (`FUN_00427bb4`) [C]: with no fans the span is always free. Otherwise, for each fan, the
span [x, x + w] is blocked when `fan x − w ≤ x ≤ fan x + fan width` (fan screen x +0xc, fan width
+0x54, inclusive), or when `x + w ≥ fan wrap (+0x2c) × fan layer factor (+0x28)`. Zappers and
flypapers are not tested.

The second clause is 5000 × 1.25 = 6250 for the 1.25 layer [I], so it never blocks a placement
inside the 5000 range. The fan width at +0x54 is the frame width (222) rather than the stored
223 [I].

Verdict: guess correct in logic; use the frame width.

### Tutorial period length
`crates/thefly-sim/src/match_state.rs:337`

Guess: 2 minutes in tutorial too (as observed), with `TUTORIAL_PERIOD_MINUTES` = 3 kept.

Original (`FUN_00405ed0`; data constructor at 0x40ac78) [C]: minutes = 3 if data +0x304 ≠ 0,
else data +0x26. +0x26 is always set to 2, and the tutorial path also sets it to 2. The only
write to data +0x304 in the binary is the constructor's 0. That flag also gives 1000 $ and
preset inventories, so it is a dormant debug flag, not the tutorial. AR §8.6 and game-spec §2.4 step 4 both mislabel it.
That nothing bulk-loads it is [I].

Verdict: guess correct. The period is always 2 min, so `TUTORIAL_PERIOD_MINUTES` and the
`TUTORIAL_AS_OBSERVED` switch can go.

### Loading backdrop pick before the world build
`crates/thefly-sim/src/screen.rs:189` (O16)

Original (`FUN_00408c2a` → game init `FUN_004015c8`) [C]: the first rand of game init is the
parity pick: even gives `loading`, odd gives `loading2`. It comes before every world draw. The
loading screen is drawn synchronously at progress steps during the init. It is not a timed
screen of its own.

Verdict: guess correct (odd → alt).

### Period clock start
`crates/thefly-sim/src/screen.rs:262` (O16)

Original (`FUN_004015c8`, clock `FUN_0044214e`/`FUN_004421dd`, banner `FUN_00405538`, pause
`FUN_00403de0`) [C]:

- The clock is created during the init, right after `j_horloge` loads, with start = now.
- At the end of the init the game is paused (game +0x407bc = 1, clock +0xb8 = 1, and the AI too),
  the banner counter is 3 with a 1000 ms timer, and slot 52 plays.
- While paused, each clock read adds the time since the previous read back onto the start. The
  first read after pausing adds 0. So the banner time is not counted, and only the rest of
  the load plus at most one tick is counted.
- The world update runs during the banner, with all player input zeroed.
- At the end of the banner: if music is on, game music track 0 (even) or 3 (odd) by parity, one
  rand; the tutorial hook `FUN_0044375d` runs; then the game unpauses.

Verdict: guess correct (equivalent within one tick). Port note: the music parity draw happens
at the banner end.

### Menu: Esc on the top page
`crates/thefly-sim/src/menu.rs:138`

Original (`FUN_0041be05` page 0) [C]: the top page reads only the arrows and Enter. The Esc
"previous" is still updated every tick. Esc is read only on the sub-pages:
back one page, credits out (page 9), or close the dialog (page 10). Verdict: guess correct.

### Fan type mapping (round 2)
`crates/thefly-sim/src/world/traps.rs` (`TRAP_TABLE`, `TrapKind`), `crates/thefly-sim/src/shop.rs` (`BUTTON_KIND`)

Guess: kind 1 = `ventilo_gauche` = blows left, kind 2 = `ventilo_droite` = blows right.

Original (trap build `FUN_0042699d`, name table near decompiled line 5304, shop icons, carry
`FUN_0042570b`) [C]:
- Kind 1 (`j_ventilo_gauche`, shop icon `p_icone_ventilo_gauche`) is built as internal fan type 2:
  - outline `palesventi_gc`
  - cone base 222, mouth 93, rows 43..279
  - vx −= push, so it blows left
- Kind 2 (`j_ventilo_droite`) is built as internal type 1:
  - outline `palesventi_dt`
  - base 0, mouth 135, rows 42..249
  - vx +=, so it blows right
- Carrying in the traps shop: moving left turns kind 2 into 1, and moving right turns 1 into 2.

Verdict: guess correct. The port's FanLeft uses CONE_LEFT, and ai-and-rules agrees. Only round
1's "type 1 pushes right" needed the internal/kind note, now added to "Fan strength".

### Shop tally: kill count drops one per cycle (round 2)
`crates/thefly-sim/src/shop.rs` (farmer tally)

Original (`FUN_0042322f` state 3) [C]:
- Each completed farmer cycle lowers data +0x21c by 1. The tally ends when it is < 1 (it is
  clamped to 0).
- With 0 kills on entry, the first call returns "done" at once.
- The fly path walks the saved types with a pre-increment from −1, so type 0 is included. It
  ends after index 21.

Verdict: guess correct.

### Credits: first hold is 7000 ms (round 2)
`crates/thefly-sim/src/menu.rs:123` (`CREDIT_HOLD_MS`)

Original (menu build around the credits fonts; credits draw, pair state +0x35f8) [C]:
- The 5000 ms value is the hold timer's creation value, and that timer is left disabled.
- On each even pair state (0, 2, 4, 6, 8), the draw re-arms the timer to 7000 ms and enables it,
  places the lines at x 800 and x −290, and starts the slide-in.
- So every hold, the first included, is 7000 ms, counted from the start of the slide-in.

Verdict: guess correct.

### Winch return while hoisting (round 2)
`crates/thefly-sim/src/world/lift.rs:152`

Original (`FUN_00431115` state 3) [C]:
- Every hoisting tick returns the hooked index and sets the object's y to winch y + rope length
  (+0x94) − object h + 10.
- It returns −1 once the winch's y + h < 0. It then clears its hooked index, and the next call
  resets the winch (`FUN_00430fe7`).
- On every tick that returns an index, the manager zeroes the object's state and v.

Verdict: guess correct.

### Screen-x setter (round 2)
`crates/thefly-sim/src/world/objects.rs:750`, `crates/thefly-sim/src/world/boot.rs:118`,
`crates/thefly-sim/src/world/lift.rs:136`

Original (`FUN_00417d0d` → `FUN_00417fab`; mover `FUN_004180af`) [C]:
- The setter stores the screen x unchanged and, for 0 ≤ x < wrap, sets layer x = trunc(x / factor).
- The next mover step adds the camera delta to the layer x and sets screen x = trunc(layer x ×
  factor).
- So the exact value lasts only until that step.

Verdict: guess correct (the same value from the next step on).

### Outline collision: the "shifted y" quirk never changes a result (round 3)
`crates/thefly-sim/src/world/collision.rs:66`

Guess: every case runs the plain row test.

Original (`FUN_0042f6de`) [C]:
- **Straddle.** A sprite straddles the wrap when x ≤ wrap and x + w ≥ wrap, inclusive. The port
  uses x + w > wrap. They differ only when x + w == wrap exactly (negligible).
- **Box pre-test.** The box test is inclusive, but the zero-length span rejects that follow make
  it strict in effect. The port's strict test matches.
- **The quirk.** In the first pass, the contained case tests rows using B's original y and A's
  height. That branch is reached only when A lies vertically inside B, and there the two forms
  give the same rows, so it never changes the outcome. FW §4.3 step 3 misdescribes it.
- **Edge case.** When B strictly contains A vertically, the row loop runs past A's height into
  A's 500-row outline table. Rows past h − 1 are nulled when the outline is built, so this has no
  effect unless the span exceeds 500 rows.

Verdict: guess correct.

### Winch sound: loop start, frequency and volume (round 3)
`crates/thefly-sim/src/world/lift.rs:324`

Guess: looped on the first tick vy ≠ 0 after a stop, then moved and retuned.

Original (the winch tick `FUN_00431931`, run from `FUN_00431115`; positional `FUN_0040d1b2`) [C]:
every tick the frequency is set to trunc(vy / 400 × 3000) + 18000. If vy (+0x88) == 0,
the slot is stopped. Otherwise slot 47 plays positionally at volume −200 with loop = 1, which
starts the looped play whenever the slot is not playing (curve in "Positional sound curve (O14)").

Verdict: guess correct. Starting the loop on the first tick with vy ≠ 0 is equivalent.

## Port decisions

- `crates/thefly-platform/src/audio.rs:292` MIDI loop point: port decision.
- `crates/thefly-platform/src/render.rs:186` text raster cache: port decision.
- `crates/thefly-sim/src/menu.rs:115` PIG_H fallback: port decision; the real value is the
  asset height (`irc_dims.py`).
- `crates/thefly-sim/src/menu.rs:165` Menu::new defaults: port decision.
- `crates/thefly-sim/src/screen.rs:381` intro video: port decision.
- `crates/thefly-sim/src/sim.rs:112` intro video: port decision.
- `crates/thefly-assets/src/btt.rs:96` stats not written: port decision.
- `crates/thefly-sim/src/bonus.rs:135` COIN_FRAMES 16: asset lookup, not a code constant. The frame is R(frame count), with the count taken from the `piece` sheet at 45×45 frames (`FUN_004472df`) [C]; read the sheet width / 45.
- `crates/thefly-sim/src/world/ambient.rs:36` tutorial bubbles (O12): port decision. `FUN_00443488`/`FUN_00443772`/`FUN_00443d54` draw no rand, play no sound and write no game state [C], so leaving them out does not desync anything.
- `crates/thefly-sim/src/world/mod.rs:131` period-load order: port decision. The
  original reseeds from the clock inside the objects build ("Period load order"), so no draw
  order before placement is reproducible.
- `crates/thefly-sim/src/world/mod.rs:138` breakfast fallback: port decision.
- `crates/thefly-sim/src/menu.rs:120`,
  `crates/thefly-sim/src/winner.rs:177`, `crates/thefly-sim/src/bonus.rs:473` shared wander mover
  not modelled: port decision until the mover lands. The parameters are settled (credits, winner
  and bonus: round 1; the shop machine is answered in "Shop machine shake", round 3).
- `crates/thefly-sim/src/winner.rs:56` loser heights: port decision; asset heights
  (`irc_dims.py`).
- `crates/thefly-sim/src/world/boot.rs:297` duplicated FPS mover, `:374` tie placement in the
  caller: port decisions (code structure).
- `crates/thefly-sim/src/highscores.rs:122` lowest scancode wins in a multi-key tick: port
  decision.
- `crates/thefly-sim/src/stats.rs:202` slot 50 alternation: port decision (no slot state in the
  sim). `FUN_0040cf55` is "not playing", not "playing" (see "Winner count-up tick slot and rand").
- `crates/thefly-sim/src/clock.rs:10` fixed 65 Hz: port decision. The original's FPS integrator
  uses the measured frame rate (game +0xb8, starting at 60), which no fixed rule reproduces; the
  rounding rule itself is answered in "FPS integrator: per-axis runs and percent rounding".
- `crates/thefly-sim/src/world/fly.rs`, `lift.rs`, `midges.rs`, `collision.rs`, `farmer.rs` and
  the zapper's `FUN_00428003`: settled in round 3 (sections above), no longer port decisions.
- Render-only, no sim state: `crates/thefly/src/assets.rs:170`,
  `crates/thefly/src/draw/between.rs:41`, `:124`, `:166`, `:333`,
  `crates/thefly/src/draw/front.rs:16`, `:189`, `:204`, `:223`, `:236`, `:305`, `:310`,
  `crates/thefly/src/draw/play.rs:104`, `:338`, `:358`, `:391`, `:457`, `:483`: port decisions
  (draw anchors, fonts and text not decoded; none draws rand).
- `crates/thefly-sim/src/world/mod.rs:152`: covered by "Midges: spawn x, homing gates, ambient
  update and constructor" (slot 0, then R(15)).
- `crates/thefly-sim/src/world/ambient.rs:39`: covered by "Farmer voice 33"; `traps.rs:207` by
  "Fan blow field: type-2 unwrap"; `objects.rs:219`, `:242`, `:287`, `:324`, `:560` by the
  round-2 object sections; `objects.rs:750`, `boot.rs:117`, `lift.rs:136` by "Screen-x setter";
  `lift.rs:152` by "Winch return while hoisting"; `fly.rs:33`, `fly.rs:353`, `fly.rs:450`,
  `crates/thefly/src/fx.rs:24` by the round-2 crash and zapper sections; `shop.rs:56`, `:59` by
  "Shop tally"; `bonus.rs:134`, `:161` by "Bonus round movers"; `winner.rs:61` and
  `highscores.rs:122` by the port decisions above.
- Open after round 3:
  - `FUN_004303e5`, the carried object's stuck-counter path (`objects.rs:614`)
  - who resets the carry stuck counter (+0x80) and what +0x74 means

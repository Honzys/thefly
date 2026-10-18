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
`FUN_00429cbc` pushes the fly by its centre (cx, cy) [C]. Per fan type (+0x918; 1 pushes right,
2 pushes left), with fan screen x `X` and fan y `Y`:

- Type 1 (right): cone base `a = X`, mouth `m = X + 135`, rows `Y + 42 .. Y + 249`.
  Inside when `a < cx < m + 500`.
- Type 2 (left): base `a = X + 222`, mouth `m = X + 93`, rows `Y + 43 .. Y + 279`.
  Inside when `m − 500 < cx < a`. Type 2 also unwraps cx near the wrap edge.
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
7. Otherwise an arc starts, aimed at the target centre (target x + w/2). It lasts R(80) + 30 ms,
   and sound 39 plays at R(800) + 11025 Hz.

The fly is only tested when fly +0x2a4, +0x2a8 and +0x288 are all 0. An arc on the fly pushes
vy by −180 and adds 1 damage (the port's values are right). The hum volume uses the "any zapper
on screen" test `FUN_00418b40`: −200 when any is visible, else −500; a single zapper plays
positionally at −450 (manager `FUN_00427270`).

Verdict: guess wrong. Arcs are a per-tick probability that falls linearly to 0 at 250 px below
the zapper bottom, with a 100 ms retry and an R(80)+30 ms arc. This consumes rand.

### Midge spawn draw order
`crates/thefly-sim/src/world/midges.rs:111`

Original (`FUN_00404caf`) [C], in rand order:

1. x = rand (raw, then wrapped)
2. sign: R(10) sign
3. |vx| = R(200) + 200
4. size = R(5) + 30
5. y = R(600)
6. a timer R(500) + 500 (inside `FUN_00442806`)
7. tolA = R(30), then tolB = R(30)
8. home y = R(frame-0 height − 60), then home x = R(181)

x tolerance (+0x180) = tolB, y tolerance (+0x182) = tolA. The x tolerance is not used by homing.

Verdict: guess wrong where the port differs from this order or swaps the tolerances.

### Midge homing
`crates/thefly-sim/src/world/midges.rs:151`

Original (`FUN_00442f95`) [C], every tick for every midge, after the contact step:

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

Original [C]: count = R(15) at period load. Per tick and per ambient midge: arc test, contact,
update (jitter: if R(100) > 70 then R(15), then R(10) decides the sign, vx = sign × m − vx; the
fan push decays by 40 per tick), then the fan push. Ambient midges never home and are never
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

### Farmer voice 33: the fly picks up an object
`crates/thefly-sim/src/world/ambient.rs:38`

Guess: "objects left on the table" = any collectible object still active.

Original (caller `FUN_0040326c`, voices `FUN_0043c730`) [C]: the flag passed in is "the fly holds
an object": object manager +0x1a is the index of the object the fly grabbed (set when a grab
succeeds in the delivery/grab step `FUN_0042c888`, 1000 = none, reset to 1000 on delivery).
Whether a plain drop also resets it was not checked [I]. Voice 33 plays with R(100) < 15 on the
rising edge of that flag. The other
voices in the same function, in rand order each tick:

1. Timer +0x18 expired and R(100) < 7: re-arm R(7000) + 12000 ms, voice 31.
2. If farmer +0x48 > 7 and R(4000) < 5: voice 32.
3. Carry flag rising and R(100) < 15: voice 33.
4. Fly-state flag (`FUN_0043885d` != 0) rising and R(100) < 10: voice 30.
5. While farmer +0x84 is set, a counter counts ticks; every 21st tick it resets and plays one of
   five voices from the table at 0x4680f0 by R(5). When +0x84 clears and the counter is non-zero,
   the counter is set to 30.

The R(100) draws in steps 3 and 4 only happen on a rising edge.

Verdict: guess wrong: the edge is the fly grabbing an object.

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
- The shaking machine (+0xac8) runs the shared wander mover every tick, so it draws rand all
  through the shop, including during fades [C]. The port should model those draws to stay in RNG
  step.

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

### Bonus round movers, boxes and target spawns (O3), partly resolved
`crates/thefly-sim/src/bonus.rs:126`

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
  coin velocity, so the leftward motion comes from the mover and layer [I, unresolved].
  A coin is removed when x + w < 0.
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
- The fly and swatter movers themselves (`FUN_00448302`, `FUN_00448c31`) were not read.

Verdict: guess wrong on the boxes, the target rows and the target spawn x. Unresolved: the
coin horizontal speed and the fly/swatter move speeds.

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
tick plays slot 49 by default; a slot-state check (`FUN_0040cf55`) switches to 50 when 49 reads 0
and 50 does not. The frequency R(500) + 21500 is drawn and set **only while the loser's counter is
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

Original [I]: a created but unarmed timer reads as expired on the first check (timer class
`FUN_004193df` / `FUN_00419506`). Verdict: guess correct [I]; the farmer's own timer init was
not checked.

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

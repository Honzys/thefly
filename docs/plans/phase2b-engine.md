# Phase 2b: gameplay engine

Goal: the gameplay of The Fly 1.2, matching the original rule for rule, with better presentation. The rules live in a pure crate, `crates/thefly-sim`. The frontend in `crates/thefly` renders them.

Sources (clean room): `docs/re/*.md`, `docs/formats/*.md` and the existing crates. Where a detail doc's "Corrections to game-spec.md" disagrees with `game-spec.md`, the correction wins. Gaps are listed under [Open items](#open-items) and are never filled by guessing. A worker who must pick a value names it as a constant, cites the open item and adds a test.

The skeleton is in place: every module, type, field and signature exists, and bodies are `todo!("W? §ref")`. Each worker replaces the `todo!`s in the files it owns.

## Decisions

### Fixed by the brief

1. **Pure and deterministic.** No SDL, OS clock or I/O in `thefly-sim`. Its only dependency is `thefly-assets`. The entry point is `Sim::tick(&mut self, keys: &KeyState) -> Vec<Event>`, called once per 65 Hz tick.
2. **One clock.** `now_ms = tick * 1000 / 65`, truncated (`clock::ms_at`). Every timer of the original (GetTickCount, `FUN_004193df`/`FUN_004194a9`) reads `Ctx::now`, which is constant within a tick. `Timer` fires on the first tick with `now ≥ deadline`.
   - **Accepted divergence:** durations round up to whole ticks. A 700 ms AI period is 45.5 ticks, so it fires on ticks 46, 92 and 138 (`clock` test). The original, on a wall clock, jitters around the same values.
   - `DT = 1/65` is fixed. The original measured dt per frame. Its 65 Hz loop makes the two equal in the steady state; a fixed dt is part of this divergence.
   - Floats are `f64` (`f32` where the doc says float32: the 0.1f constants, fluctuation, prices). The original used x87 80-bit intermediates. Rare last-bit differences are accepted.
3. **RNG.** `MsvcRand`, the MSVC LCG (fly-and-world §1), injected with a seed. The stream is seeded **once** per `Sim`. The original reseeded from `time()` at each period start; replaying a real session is not a goal. Every rule calls `cx.rng` in the spec's order, including the visual-only calls that share the stream (bourse, saves).
4. **The 17-step tick order** of `FUN_0040326c` is the literal body of `World::tick` (`world/mod.rs`): one call per step, in order. Workers fill steps in their own files and never reorder or merge them.
5. **Input.** A per-tick `KeyState` of USB HID scancodes (`input::Scancode(u8)`), with held, pressed and released sets. `FlyPad::read` and `FarmerPad::read` apply the key map of `input::keys`. The farmer uses T/F/G/H with Tab and Q. The frontend converts SDL scancodes with `u8::try_from(sc as i32)`. `KeyState::from_parts` passes the platform's edges through, so a tap within one tick is kept.
6. **Outputs are events.** `Event::{Sound, Music, Click, Effect, ScreenChanged, Save}`. `event::SOUND_FILES: [&str; 59]` is the only table that maps slots to files.
7. **UB guards are named constants** (see [UB guards](#ub-guards)).
8. **One `Screen` enum**, plus match and period progression in `MatchState`.
9. **Every struct field is declared up front.** A worker who needs a field adds it in its own file and records it in its summary.
10. **Scenario harness** (`scenario.rs`), with passing examples: seed 1 gives 41, 18467, 6334, and the logo timing.

### Made while sketching

- **`GameData`**, the asset-derived numbers (frame sizes and `.bor` outlines), is passed to `Sim::new`. The frontend builds it from the real data. Tests use `GameData::synthetic()`. `GameData::check()` lists anything missing.
- **Render-driven state moves into the tick.** Two state changes ran in the original's render pass (`FUN_00403e36`, once per tick after the update): the appear animation finishing makes the fly alive, and the splat particles finishing records the kill and requests a respawn. The midge swarm update (`FUN_00404ef3`) also ran there. They become port step 18: **18a** `midges_step`, then **18b** `fly_render_driven`. The order inside the pass is an open item. Their durations are sim timers.
- **The camera delta and the layer accumulators live in the sim** (`world::camera`), because placement and collision use layer x.
- **The quit dialog is an overlay**: `State.dialog: Option<QuitDialog>` sits beside `State.screen`. The screen below keeps ticking with `KeyState::NONE`. A gameplay screen is paused while the dialog is open.
- **Typed in-match screens.** `Screen::Match(Box<InMatch { m: MatchState, screen: MatchScreen }>)`. Screens that need a match can only be reached with one, so there is no `Option<MatchState>` to unwrap. In-match screens return `MatchFlow { Stay, Next(MatchScreen), Leave(Flow) }`; the other screens return `Flow { Stay, Go(Screen), Menu, StartMatch, Quit }`. The keys page is in both: from the menu it returns to the menu; from match start it goes on to the tutorial page.
- **The world is rebuilt per period** (`World::new(m, cx)`). `MatchState` lasts for the whole match: config, progress, economy, stats, trap layout and winch slots.
- **Interpolation by clone.** `State` and `World` derive `Clone`. Before each tick the frontend keeps a clone and draws `lerp(prev, cur, alpha)`. It must handle the wrap (4000 px fly ring, trunc(4000 × 1.25) object ring) and skip the lerp across teleports (respawn, screen change). The sim stores no previous-position fields for rendering. The `prev_x`/`prev_y` fields that exist are gameplay state from the spec.
- **Saves go out as `Event::Save { file, bytes }`.** Writing the score file draws rand for the filler checksums, so encoding runs in the tick with `cx.rng`. The frontend decodes `scoot.btt`/`boite.btt` into `saves::Persisted` before `Sim::new`.
- **The menu's cosmetics stay in the frontend.** Clouds and dust puffs belong to W4. The intro script lives in the sim (W3), because it gates input and emits sounds.

### Architecture: two shapes considered

The `pstack:architect` arena would run several sketchers. This run could not spawn agents, so both shapes were sketched inline.

- **Shape A: subsystem structs with free functions.** Each subsystem (`Fly`, `Farmer`, …) owns its state, and each step is a free function taking explicit borrows of the parts it touches (`fn traps_step(traps: &mut Traps, fly: &mut Fly, midges: &mut Swarm, cx)`). This is explicit, but the signatures churn every time a step needs one more part, and most steps touch three to five of them.
- **Shape B: `World` as a plain-data bag, one step method per spine step.** `impl World { pub fn traps_step(&mut self, cx: &mut Ctx) }`, written in the owner's file. A step reads anything and writes only its owner's state. It reaches other owners through their public methods (for example `self.fly.splat(dir, cx)`). Results flow between steps as explicit arguments and return values (`action`, `swing_edge`, `hit: SwatHit`).

**Synthesis decision: B.** The spine in `world/mod.rs` reads like the spec's tick table. The interfaces are one method per step, which gives deep modules and a small surface. Data flow between steps stays visible because results pass as values. A's precision is kept where it matters: the cross-owner effects are a short, named API (below), and the ownership rule is stated in `world/mod.rs`. A contributor who opens only one file sees the rule in its doc and the step's signature. Copying the nearest step gives a correct new step.

`Ctx<'a> { now, rng, data, events }` is built per tick from disjoint `Sim` fields. Events are pushed through `cx.emit` instead of a `&mut Vec` parameter.

## Module map

All paths are under `crates/thefly-sim/src/` unless noted. Spec references: **FW** fly-and-world, **AR** ai-and-rules, **GS** game-spec, **SR** screens-and-rendering.

| Owner | Files | Spec sections |
|---|---|---|
| Shared (done; changes need the orchestrator) | `lib.rs`, `clock.rs`, `rng.rs`, `input.rs`, `event.rs`, `ctx.rs`, `data.rs`, `sim.rs`, `scenario.rs`, `world/mod.rs` (spine and `World` fields) | FW §1, §2; GS §2.2, §3, §8 |
| **W1** fly and world physics | `world/fly.rs`, `world/collision.rs`, `world/camera.rs`, `world/objects.rs`, `world/lift.rs`, plus `World::new` in `world/mod.rs` | FW §3 (fly), §4 (layers, outlines, contact), §5 (objects, placement, motion, manager tick, lift); AR §7.5 (delivery); SR §4.2 |
| **W2** farmer side and hazards | `world/farmer.rs`, `world/ai.rs`, `world/traps.rs`, `world/goodies.rs`, `world/boot.rs`, `world/midges.rs`, `world/flying_goodies.rs`, `world/ambient.rs` | AR §1 (farmer), §2 (AI), §4.1 (trap table), §5 (farmer goodies), §6 (boot, flying goodies, midges); FW §2 steps 3, 4, 13–17, §6, §7, §8; GS §4.6, §4.7 |
| **W3** match flow and screens | `match_state.rs`, `economy.rs`, `stats.rs`, `screen.rs`, `menu.rs`, `shop.rs`, `shop_ai.rs`, `bourse.rs`, `bonus.rs`, `winner.rs`, `highscores.rs`, `saves.rs`; the `scoot.btt` codec in `crates/thefly-assets/src/btt.rs` | GS §2 (state machine, menu, modes, progression), §5.6, §5.7, §7; AR §3, §4.2–§4.4, §7, §8, §9; SR §1.4–§5.4 |
| **W4** frontend | `crates/thefly/` | SR (all render numbers), GS §6, §8; phase 2a platform API |

### W4 scope (frontend, `crates/thefly/`)

- Build `GameData` from the data dir and call `check()`. Decode the saves into `Persisted`. Seed from the wall clock, or from `--seed`.
- Run `Sim::tick` on the platform's fixed 65 Hz timestep. Keep the previous `State` and render the lerp with the frame alpha. Use sub-pixel motion via alpha and the linear-filter option. Letterbox widescreen: the 800×600 logical view stays centred, and the side bands show extended scenery or black, which is W4's call.
- Map events to audio, music, effects and file writes through `event::SOUND_FILES` and `SaveFile::file()`.
- Render every screen from `State`, following SR. Owns the menu clouds and dust, particle effects, and positional sound mixing. Positional audio is an open item.
- The platform has no frequency/pan API and the renderer has no scale API (open item 31). W4 adds them in `thefly-platform`, or reports that it can't.

### Cross-owner API (the only writes across owners)

| Caller | Callee (owner) | Purpose |
|---|---|---|
| W2 boot, traps, farmer | `Fly::splat`, `Fly::grill`, `Fly::add_raw_damage`, `Fly::push_by_boot` (W1) | hazards hurting the fly |
| W2 goodies | `Fly::start_turbo`, `Fly::end_turbo`, `Fly::start_superfly` (W1) | fly goodies and mysteries |
| W2 goodies | `Farmer::start_triple`, `start_nuclear`, `end_nuclear` (W2), `Swarm::spawn`, `Boot::spawn` | farmer goodies, Midges |
| W1 fly update | `SwatHit` from `farmer_step` (step 6 → 8) | damage and kick direction |
| W1 fly / objects | `MatchState::record_kill`, `record_delivery`, `Stats::record_life` (W3) | economy and stats |
| W2 farmer | `Stats` swing counter (W3 type, W2 increments it on frame 1) | stats |
| W2 flying goodies, W3 bonus | `Economy::meter` (W3) via `flying_goodies::luck` | balance meter |
| W1 objects placement | `Traps::span_free` (W2) | objects avoid traps |
| W2 traps | `Traps::from_layout(&m.traps)` (W2 reads the W3 layout) | the period's traps |
| W3 shops, match | `world::lift::draw_anchors` (W1) | winch slots (`WINCH_REROLL_CAP`) |
| W3 bonus | `world::ai::{Aligner, Controller}` (W2) | bonus AI |
| W3 gameplay screen | `World::new`, `World::tick`, `World.period_over`, `World.paused` | the period |

## Acceptance

All workers: `nix-shell --run 'cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test'` is clean, and no `todo!` is left in owned files except at listed open items. Each item below is a scenario test (`scenario::Scenario`), or a unit test on the owner's type built with `GameData::synthetic()` and a fixed seed. Golden values that come from the rand stream are computed by hand from the seed-1 sequence in the test, not copied from the implementation's output.

### W1

1. **Init** (FW §3.3): on a fresh `MsvcRand::new(1)` with the synthetic Face size (w, h), `Fly::new(0, 0)` has x = 41 % (800 − 2w) + w, y = trunc((600 − h)/2), max_x = max_y = 550, f = 0.0. With speed 5, max = 850.
2. **Steering** (FW §3.4): holding Right from rest gives vx = trunc(MX × 0.1f) per tick up to MX. Releasing it lets vx fall by 9 per tick to exactly 0. Up uses climb = trunc(MY × 0.1f). Carrying with f = 0.6 uses climb = trunc(75 × 0.6 + MY × 0.1f / 1.6).
3. **Turbo** (FW §3.8): `start_turbo(None)` boosts both axes by trunc(maxX × 0.5) and kicks the dominant axis. The boost holds for 1500 ms (`first_tick_at`), then decays by trunc(max × 0.05) per tick to 0. Mystery Turbo lasts 25000 ms.
4. **Hit and stun** (FW §3.6): damage 40 at total 0 stuns the fly. The hit frame goes 0→3, then the kick, sound 28 and a ricochet of 23 deceleration ticks, then back to 0. A second damage during the hit is ignored. SuperFly turns damage 40 into trunc(40/2.5) = 16 and skips the kick.
5. **Death and respawn** (FW §3.5–§3.7): a total above 100 starts dying. Once the crash ends, the respawn timer fires 1500 ms later; the kill is recorded with its kill type (1 hit → 3, 2–3 → 2, 4+ → 1); respawn y = trunc((600 − h)/2 − 20); placement stops after 1000 tries (3000 at period start).
6. **Edge scroll** (FW §3.4 step 11): x + w ≥ 800 − w sets the camera delta to MX × o × dt / (w/2) for that tick only. Away from the edge it is 0.
7. **Placement** (FW §5.2): with seed 1, at most 15 objects, no two overlapping, none on a trap span, the count per type within its stock. The same seed gives the same layout twice.
8. **Object motion** (FW §5.4): a dropped object falls at 50 px/tick² to rest y 580 − h, and |vx| ≤ 500.
9. **Lift and delivery** (FW §5.6, AR §7.5): three anchors drawn by `draw_anchors(36)` are pairwise more than 36 apart. Dropping a carried object on the winch plays slot 11, adds 1 to `saved[type]` and stats delivered, adds trunc(price) to the price total, and sets `period_over` when no stock is left.
10. **Spine order** (FW §2): a scenario where the boot splats the fly in step 10 sees step 11 reset both goodie selectors on the same tick.

### W2

1. **Farmer constants** (AR §1.1): level L gives maxSpeedX 650 + 60L and a cooldown of 35 − 5L ticks. The first voice timer is 10000 ms. `Farmer::new` on a fresh `MsvcRand::new(1)` gives x = 41 % 800 = 41 and y = 351.
2. **Movement** (AR §1.2): vx steps by ±400 per tick; vy by ±200, clamped to ±400. y stays in 351..472; reach = (1 − (472 − y)/121) × 375.
3. **Swing and hit** (AR §1.3–§1.4): a Tab press edge starts a swing. A press at cooldown 0 winds up on ticks 1–2, is active on ticks 3–5 and retracts on ticks 6–9. The minimum swing period is 9 + 36 − 5L ticks: 45 at L 0, 20 at L 5. Hit test: centre (x + 65, trunc(77 + reach)), radius 65 + flyW/2. Damage is trunc((40 + 6L) × f), or 34 with Nuclear. Holding Tab does not repeat.
4. **Triple and Nuclear** (AR §1.6–§1.7): Triple bursts 3 swings and ends after more than 6 bursts. Nuclear lasts 5000 ms from the goodie and 20000 ms from the mystery, and Triple turns it off.
5. **AI parameters** (AR §2.2): `AI_PARAMS[1]` has align 40, spread 181, cut 60, dwell 1000. A one-player scenario, seed 1, 30 s with no input: the farmer moves toward the fly within alignTol, swings, and starts no goodie it does not own.
6. **AI timing** (AR §2.4): AI decisions run on the 700 ms (and listed) timers, at ticks 46, 92 and 138 for 700 ms.
7. **Traps** (GS §4.6, FW §7.4): flypaper divides v by 1.4 per overlapping tick. A fan damages at most once per 100 ms. A zapper grills the fly.
8. **Goodies** (GS §4.7, AR §5): Right Shift cycles the selector 0→1→2→3→0; Enter with count 0 does nothing; with count > 0 it uses one, plays slot 38 and cancels this tick's action. A mystery lasts 15000 ms.
9. **Boot** (FW §7): after 9 stomps the boot leaves; a stomp that splats the fly sets the count to 15. The splat records a kill.
10. **Midges** (FW §8): one use adds min(100, 300 − active); a swat kills at most 20; the farmer slows while midges touch him.
11. **Flying goodies** (FW §6): first checks at 8000 + R(7000) ms; lifetimes 8000 (fly) and 12000 (farmer) ms; kind 15 (mystery) when R(100) < 3. The trailing side, by `luck`, spawns more often in a long seeded run.

### W3

1. **Progression** (AR §9.1): normal 2 days goes B1, L1, D1, B2, L2, D2, then over. Inside/outside goes 1, 2, 1, 2, … The stats period counter does not count the over call. In extreme, `advance` on a fresh `MsvcRand::new(1)` picks {1, 2, 4}[41 % 3] = dinner and, since 18467 is odd, outside (2); the second advance is over.
2. **Bonus due / over** (AR §9.1): the bonus round follows dinner, and always in extreme. Normal: over after D of the last day.
3. **Period clock** (AR §8.6): with a 2-minute limit the period ends at elapsed = trunc((now − start − paused)/1000) ≥ 120; 3 minutes in tutorial mode. Time paused under the quit dialog is added back.
4. **Prices** (GS §5.6, AR §7.2): the farmer level costs 200, 400, 600, 800, 999, and the sixth purchase is refused with slot 12. Fly strength costs 100 … 500. Tokens are capped at 14. A purchase plays slot 11, and the owned count rises only when the conveyor ends.
5. **Fluctuate** (AR §7.1): with seed 1, price 60.0: sgn = (41 % 10 < 5) ? −1 : +1 = −1, price = 60 − (17/100) × 60 = 49.8 in float32, and the return is 60. The floor is 1.0.
6. **Payout** (AR §7.3): the fly earns Σ old price × saved, fluctuating every present type. The farmer earns 42 × kills, then kills = 0. The animated tally and the skip give the same totals.
7. **Meter** (AR §8.2): diff 0 gives 0.5. diff +500 (fly leads) gives meter(fly) = 0.25 and meter(farmer) = 0.75. The clamp is at ±1000.
8. **Starting inventories** (GS §5.7): the three setups match the table, including tutorial money 1000/400 and score 4200/3000.
9. **AI shopping** (AR §3): with seed 1 and a fixed budget, the plan matches a hand computation. Presses come 700 ms apart in the drawn order.
10. **AI traps** (AR §4.4): gate threshold c − trunc(tokens × (c − c/2)/14); zappers are placed first; 200 attempts each; replay at 800/500/1000/800 ms with slot 13.
11. **Bonus** (AR §8.3): the countdown goes 3, 2, 1 at 1000 ms; the round lasts 35000 ms; coins spawn every 50 ms into 20 slots; targets every trunc(h × 300 + 200) ms. Fly +2 per coin, farmer +10 per hit. 2000 ms after the end: slot 53 if 2·coins < 10·hits, else slot 51.
12. **Winner** (AR §8.4): after the top-up, a tie goes to the fly.
13. **Screens** (GS §2.2, SR): the logo passes (done). The menu defaults are 2 days, 1P, mode 1, AI 1; Highscore mode forces AI 1 and 3 days. The quit dialog defaults to NO; YES from the menu goes to the farewell and YES in a match goes to the menu. A 500 ms fade ends on tick 33.
14. **High scores** (AR §8.5, GS §7.1): insert at the first row with a score ≤ the new one. Checksum c(1) = 1.74625. The shipped `scoot.btt` round-trips through decode and encode, with the filler draws taken from the rand stream.

### W4

1. Runs the real data from the boot screen to the main menu, with the logo fade and the "Release 1.2" label. Screenshot checked.
2. A 2P scenario played by script (Xvfb) reaches gameplay, renders the period start banner and plays sounds. Screenshot checked.
3. Interpolation: a fly moving at constant vx renders at monotonic x across frames at 144 Hz, including across the 4000-px wrap.
4. The frontend's sound map test checks that every `SoundSlot` resolves to a file in `data/sons`.

## Integration order

1. **Skeleton** (this commit): everything compiles; the logo scenario passes.
2. **W1, W2 and W3 in parallel**, each in its own worktree on top of the skeleton. W3 first lands `PeriodClock`, `MatchState::new`, `advance` and `Economy::new`/`meter`, which W1 and W2 call. Until then, their tests build `MatchState` through a W3-provided `MatchState::new` stub, or test their own types directly.
3. **W1 + W2 merge**: `World::new` and the first full period scenario (fly alive, AI farmer swinging, objects delivered, period over).
4. **W3 merge**: the full match flow, menu → period → shop → bourse → … → winner → stats.
5. **W4** starts against the skeleton at once, rendering the logo and the menu, and follows each merge.
6. **Cross-review**: W1 reviews W2's use of the Fly API, W2 reviews W1's `SwatHit` consumer and placement, and W3 reviews the stats and economy calls. Then the orchestrator checks the acceptance list.

## UB guards

| Constant | Where | Why |
|---|---|---|
| `WINCH_REROLL_CAP = 10_000` | `world/lift.rs` | the winch-slot redraw loop (AR §4.3) is unbounded |
| `FRENZY_DIVISOR_FLOOR = 1` | `world/ai.rs` | `rand % (1 − trunc(onRand/L))` divides by zero when the quotient is 1 |
| `TRIPLE_CHEAP_OBJECT_BONUS = 0` | `world/ai.rs` | the Triple roll adds an uninitialised value when the reason is 1 and the price ≤ 70 |
| `KILL_LOG_CAP = 50` | `match_state.rs` | the kill-type list is not bounds-checked |
| `TRAP_PICK_CAP`, `ORDER_DRAW_CAP = 10_000` | `shop_ai.rs` | retry loops that end by argument, not by a bound |
| `RESPAWN_TRIES`, `PLACEMENT_*`, `GOODIE_LOOPS`, `TRAP_X_ATTEMPTS` | various | the original's own caps, named |

## Deferred

- **Intro video** (`Intro.avi`, state 0x8c). `Screen::IntroVideo` exists, but Boot goes straight to the logo. `shortcut: the video is skipped (Esc skips it in the original anyway); upgrade when the frontend can decode Intro.avi.`
- **Sound manager boot step** (state 0x00): no-op in the sim.

## Open items

Each one names where its constant or `todo!` lives. "Proposal" items are a worker's call, which must be recorded in that worker's summary.

1. **Stack push formula** (FW open 1, `FUN_0042e28a`): how a push spreads through a stack. W1, `objects.rs`.
2. **Camera accumulation and wrap; W = 4000** (FW open 2): `world::W` is not measured. W1 measures it from the asset's width in `GameData` and checks the wrap.
3. **Farmer catch flag** (FW open 3): the swatter flag gating the farmer's flying-goodie catch. W2.
4. **Splat particle duration** (FW open 4): the sim timer for step 18b. W1 `fly.rs`.
5. **Crash animation duration**: when "crash finished" fires (FW §3.6 D). W1.
6. **Outline quirk** (FW open 5): whether the contained-span y quirk ever changes a result. W1 ports it as is.
7. **Price statistic** (FW open 6): the per-delivery formula beyond + trunc(price). W3 `record_delivery`.
8. **Reseeding** (FW open 7): decided as seeding once per `Sim`.
9. **Farmer key order** (AR open 1): GS §3 gives T up, G down, F left, H right, Tab swing and DIK 0x10 for the goodie, which is positional, as USB HID Q (20) is. AR still lists the T/G/F/H-to-direction order as unconfirmed, and the in-game help text says D, F, R, G. `input::keys` follows GS §3.
10. **AI controller `dy` reference points and the reaction-hold encoding** (AR open 2). W2 `ai.rs`.
11. **AI budget lifetime**: per period here, because the AI is rebuilt with the world. Confirm. W2.
12. **Flying goodies: N and the farmer kind mapping** (FW §6.2). W2.
13. **The math call** (likely sin) behind the boot bob (AR open 5) and the bonus coin entry (AR §8.3). W2, W3.
14. **Boot speed expression, and whether a boot crush counts as a kill** (FW §7.3 says it can kill; the kill record path must be confirmed). W2.
15. **Midge contact box** (AR open 7). W2 `midges.rs`.
16. **Ambient midges** (game+0x15a20): count and spawn rule. W2.
17. **Trap type 4** (`colle_mouche2`): how a human places it. W3/W2.
18. **The 0.152 scale**: stored trap x = trunc(x / 0.152) "is world space [I]". W2 maps stored x to layer x and records the mapping.
19. **AI+0x08**, the flag that zeroes damage. W2 `AiOutput::zero_damage`.
20. **Conveyor**: the delay between purchase and grant, and a double press during the animation. W3 `shop.rs`.
21. **Bonus round**: the intro sequence, coin trajectory, layout, and how the bonus fly and swatter move. The proposal reuses the world's fly physics and farmer movement on the bonus field. W3.
22. **Goodies and traps shop render; human trap placement controls.** W3/W4.
23. **Farewell timing and choreography** (SR open). W3 `Farewell`.
24. **`color_shade` cell colours and the 1P keys-page fill.** W4.
25. **`scoot.btt` layout conflict**: `docs/formats/btt.md` and `thefly_assets::btt` read a 40-byte name and an f64 score at +40; GS §7.1 says name[12], an i32 score at +0x0c and a checksum f64 at +0x28. Since +40 = +0x28, the existing parser most likely reads the checksum as the score. W3 settles it against the shipped file and adds the writer in `thefly-assets`.
26. **`boite.btt` missing**: the default options. W3.
27. **Lift hook offset `off`** (`world/lift.rs`): its value. W1.
28. **Winch draw R(760) vs rand % 761**: FW §5.6 says R(760); AR §4.3 says rand % 761. AR is the more detailed doc, so `draw_anchors` documents both; W1 picks one and cites it.
29. **Fan strength (+0x91c) and blow schedule; zapper arc schedule** (`FUN_00428b47`). W2 `traps.rs`.
30. **Grill death path**: what `Fly::grill` does beyond the death. W1.
31. **Platform gaps**: the audio API has no frequency or pan control, and the renderer has no scale. W4.
32. **Ambient animal sounds** (`FUN_00441b3c`): interval and choice. W2 `ambient.rs`.
33. **Tutorial bubbles** (`FUN_00443772`, `FUN_00443488`): the sequence and triggers. W2.
34. **Which steps freeze while paused**, besides the boot and the AI. W1/W2.
35. **Positional sound curve** (`FUN_0040d005`, `FUN_0040d1b2`): `Pan::ScreenX` carries x and w, and the frontend owns the curve. W4.
36. **Sound file extension case** on case-sensitive file systems. W4.
37. **Quit dialog**: Esc behaviour, and the first-show button offset (render only). W3/W4.
38. **Loading**: the step count of 9 is inferred; which world timers are armed before the 2.3 s of waits (the original builds the world during them); whether the world ticks under the "3, 2, 1, Go!" banner. W3 `screen.rs`.
39. **High-score name entry**: text input from scancodes (the allowed characters and the length). W3.
40. **Fade float type**: the alpha step is computed in f64. Only the completion tick matters for gameplay, and it is tick 33 at 500 ms either way. W3.
41. **Stats page constant** at 0x45debc (the percentage). W3.
42. **Bourse class mask** (`0x4672e8`). W3/W4.
43. **Tutorial with 2P Extreme**: GS §5.7 has three inventory rows (Normal, Extreme, Tutorial) and does not say which applies when both are on. W3 `Economy::new(setup, extreme)`.
44. **Transition tick**: a new screen first ticks on the tick after the one that created it, including after a quit-dialog YES (`State::tick` returns early). The original runs separate init, run and cleanup states, so it may differ by a tick per transition. W3 checks this against GS §2.2.

# Phase 2b: gameplay engine

Goal: the gameplay of The Fly 1.2, matching the original rule for rule, with better presentation. The rules live in a pure crate, `crates/thefly-sim`. The frontend in `crates/thefly` renders them.

Sources (clean room): `docs/re/*.md`, `docs/formats/*.md` and the existing crates. `docs/re/gaps.md` is authoritative: where it (§10) or a detail doc's corrections disagree with `game-spec.md` or another detail doc, it wins. Gaps are listed under [Open items](#open-items) and are never filled by guessing. A worker who must pick a value names it as a constant, cites the open item and adds a test.

The skeleton is in place: every module, type, field and signature exists, and bodies are `todo!("W? §ref")`. Each worker replaces the `todo!`s in the files it owns.

The constructors are done, so a worker can test without waiting for the others: `MatchState::new`, `Economy::new`, `PeriodClock`, `Fly::new`, `Farmer::new`, `Boot::new`, `Lift::new` with `draw_anchors`, `Camera::new`, `FlyingGoodies::new`, `Objects::empty` and `World::bare(&m, cx)`. `World::bare` builds a two-player world with no objects, traps or AI; it panics with one player. `World::new`, every step body and the tick scenario (`#[ignore = "needs W1/W2"]` in `world/mod.rs`) wait for W1 and W2. Remove the `ignore` when they land.

## Decisions

### Fixed by the brief

1. **Pure and deterministic.** No SDL, OS clock or I/O in `thefly-sim`. Its only dependency is `thefly-assets`. The entry point is `Sim::tick(&mut self, keys: &KeyState) -> Vec<Event>`, called once per 65 Hz tick.
2. **One clock.** `now_ms = tick * 1000 / 65`, truncated (`clock::ms_at`). Every timer of the original (GetTickCount, `FUN_004193df`/`FUN_004194a9`) reads `Ctx::now`, which is constant within a tick. `Timer` fires on the first tick with `now ≥ deadline`.
   - **Accepted divergence:** durations round up to whole ticks. A 700 ms AI period is 45.5 ticks, so it fires on ticks 46, 92 and 138 (`clock` test). The original, on a wall clock, jitters around the same values.
   - `DT = 1/65` is fixed. The original measured dt per frame. Its 65 Hz loop makes the two equal in the steady state; a fixed dt is part of this divergence.
   - Floats are `f64` (`f32` where the doc says float32: the 0.1f constants, fluctuation, prices). The original used x87 80-bit intermediates. Rare last-bit differences are accepted.
3. **RNG.** `MsvcRand`, the MSVC LCG (fly-and-world §1), injected with a seed. The stream is seeded **once** per `Sim`. The original reseeded from `time()` at each period start; replaying a real session is not a goal. Every rule calls `cx.rng` in the spec's order, including the visual-only calls that share the stream (bourse, saves).
4. **The 17-step tick order** of `FUN_0040326c` is the literal body of `World::tick` (`world/mod.rs`): one call per step, in order. Workers fill steps in their own files and never reorder or merge them.
5. **Input.** A per-tick `KeyState` of USB HID scancodes (`input::Scancode(u8)`), with held, pressed and released sets. `FlyPad::read` and `FarmerPad::read` apply the key map of `input::keys`. The farmer uses T/F/G/H with Tab and Q (confirmed, gaps §1.1, §1.2). The frontend converts SDL scancodes with `u8::try_from(sc as i32)`. `KeyState::from_parts` passes the platform's edges through, so a tap within one tick is kept.
6. **Outputs are events.** `Event::{Sound, Music, Click, Effect, ScreenChanged, Save}`. `event::SOUND_FILES: [&str; 59]` is the only table that maps slots to files.
7. **UB guards are named constants** (see [UB guards](#ub-guards)).
8. **One `Screen` enum**, plus match and period progression in `MatchState`.
9. **Every struct field is declared up front.** A worker who needs a field adds it in its own file and records it in its summary.
10. **Scenario harness** (`scenario.rs`), with passing examples: seed 1 gives 41, 18467, 6334, and the logo timing.

### Made while sketching

- **`GameData`**, the asset-derived numbers (frame sizes and `.bor` outlines), is passed to `Sim::new`. The frontend builds it from the real data. Tests use `GameData::synthetic()`. `GameData::check()` lists anything missing.
- **Render-driven state moves into the tick.** Two state changes ran in the original's render pass (`FUN_00403e36`, once per tick after the update): the appear animation finishing makes the fly alive, and the splat particles finishing records the kill and requests a respawn. The midge swarm update (`FUN_00404ef3`) also ran there. They become port step 18: **18a** `midges_step`, then **18b** `fly_render_driven`. The order inside the pass is open item O21. Their durations are sim timers.
- **The camera delta and the layer accumulators live in the sim** (`world::camera`), because placement and collision use layer x.
- **The quit dialog is an overlay**: `State.dialog: Option<QuitDialog>` sits beside `State.screen`. The screen below keeps ticking with `KeyState::NONE`. A gameplay screen is paused while the dialog is open.
- **Typed in-match screens.** `Screen::Match(Box<InMatch { m: MatchState, screen: MatchScreen }>)`. Screens that need a match can only be reached with one, so there is no `Option<MatchState>` to unwrap. In-match screens return `MatchFlow { Stay, Next(MatchScreen), Leave(Flow) }`; the other screens return `Flow { Stay, Go(Screen), Menu, StartMatch, Quit }`. The keys page is in both: from the menu it returns to the menu; from match start it goes on to the tutorial page.
- **The world is rebuilt per period** (`World::new(m, cx)`). `MatchState` lasts for the whole match: config, progress, economy, stats, trap layout and winch slots.
- **Interpolation by clone.** `State` and `World` derive `Clone`. Before each tick the frontend keeps a clone and draws `lerp(prev, cur, alpha)`. It must handle the wrap (4000 px fly ring, trunc(4000 × 1.25) object ring) and skip the lerp across teleports (respawn, screen change). The sim stores no previous-position fields for rendering. The `prev_x`/`prev_y` fields that exist are gameplay state from the spec.
- **Saves go out as `Event::Save { file, bytes }`.** Writing the score file draws rand for the filler checksums, so encoding runs in the tick with `cx.rng`. The frontend decodes `scoot.btt`/`boite.btt` into `saves::Persisted` before `Sim::new`.
- **The menu's cosmetics stay in the frontend.** Clouds and dust puffs belong to W4. The intro script lives in the sim (W3), because it gates input and emits sounds.


### Reconciled against the reverse-engineering docs

These replace the open items of the first plan. [C] means read in the binary, [I] inferred; `gaps.md` is cited by section.

| Was | Decision | Source |
|---|---|---|
| Stack push formula | W1 implements `FUN_0042e28a` as written: riders first and recursive, mode 2 and 3 scale v by 1/1.5, E = |trunc(m s²)|, speed = sqrt(E / 2M), nothing at speed ≤ 50. Mode 2 with vx = 0 draws R(10) per pushed object, depth first. | gaps §3 [C] |
| Camera and W | W = 4000 (`Maison`, `dehors`, `arene`). The layer accumulator adds trunc(delta), the source x is trunc(acc × factor), and the wraps are the two steps as written. Deltas under 1 px move no background. `GameData::check` rejects another width. | gaps §4.2, §4.3 [C]; corrects SR §4.2 |
| Farmer catch flag | The farmer catches only with swing frame ≥ 3 and the "cannot hit" flag clear, against the box (x, trunc(reach), 131, 154), bounds inclusive. The fly side has no gate. The trailing-side re-roll is `R(100) ≤ 2q` for the fly and `< 2q` for the farmer. | gaps §5 [C] |
| Splat duration | Not a constant: the 11 body pieces fall under `v += g` and the splat ends when all are at y ≥ 600; a zapper splat lasts 64 ticks. The kill is recorded in the same tick the splat ends, then the respawn runs next tick. A boot or fan splat counts as a kill. | gaps §2.2, §2.3 [C] |
| Fly alive after the appear animation | When the scale tween ends: 500 ms, tick 33. | gaps §2.3, §11.7 [I] |
| Grill death path | The splat path with kind 13, sound 43. | FW §7.4; gaps §2.1 [C] |
| Outline quirk | Reproduce the contained-span y quirk as written. | gaps §11.2 [I] |
| Price statistic | `record_delivery` adds trunc(price). | GS open 18; gaps §11.3 [I] |
| Farmer keys | T up, G down, F left, H right, Tab swing, scancode 0x10 (USB Q) goodie. The in-game help text naming D, F, R, G is cosmetic. | gaps §1.1, §1.2 [C], §10 |
| AI `dy` | Swatter reference y = trunc(reach + 77); the target's top y, plus h/2 with the centre flag. | gaps §9.1 [C] |
| AI x-direction hold | Keep the last chosen x direction until the reaction delay expires. | gaps §11.1 [I] |
| AI budgets | Reset every period, because the AI is rebuilt with the world. | gaps §6.1 [C] |
| AI+0x08 | Always 0. The "zero the damage" step is omitted; `zero_damage` is removed from `AiOutput` and `FarmerAi`. | gaps §6.2 [C] |
| Math call | `sin`, for the boot hover (argument trunc(screenX / 10), amplitude 5) and the bonus coin entry. | gaps §7.1, §7.2, §7.3 [C] |
| Boot | Horizontal speed trunc(dist × 2000 / W + 250); a stomp that splats the fly records a kill. | gaps §7.2, §2.3 [C] |
| Midge contact | One routine and one box for goodie and ambient midges; the 20 slots count zapper touches and midges in the hit state, including ones stunned earlier. The slowdown box is the full 181 px swatter width. | gaps §8.1 [C]; corrects FW §8.2 |
| Trap type 4 | No such trap kind. Every flypaper built under the cap of 7 draws R(10); above 4 it uses the `colle_mouche2` sprite (`Flypaper::variant2`). Cost and stored type stay those of flypaper. `TrapKind::Flypaper2` is removed. | gaps §8.2 [C]; corrects AR §4.1 |
| Human trap placement | Cursor speed ±1 per held tick capped at ±10, x clamped to [20, 780 − w]; a fan faces the way the cursor last moved. | gaps §8.2 [C] |
| 0.152 scale | Stored x = trunc(shop x / 0.152) is the object-layer x. A plain scale factor. | gaps §11.4; GS open 27 [I] |
| Winch draw | `rand % 761` (0..760) per slot, gap 36 at reset and at shop exit, drawn once in the match reset (`MatchState::new`, before the music coin). Extreme re-rolls them in `advance` (AR §9.1). GS §2.4 step 2 also names `FUN_0040b994` for extreme setup; whether that draws again at match start is not stated, and the port assumes it does not [I]. FW §5.6 and AR §4.3 both say `% 761`. | FW §5.6; AR §4.2, §4.3, §9.1 [C] |
| Conveyor double press | A confirm during the conveyor animation is dropped, not queued, so the cap cannot be exceeded. | gaps §9.2, §11.6 [I] |
| Bonus coin entry | n counts spawns every 50 ms into 20 slots; wave re-roll when R(100) < 8h + 4; x 810; y = trunc((sin(n ω) + 1) A) + bgY + coinH; lifetime 25000 ms. | gaps §7.3 [C] |
| Name entry | a to z and 1 to 0 type, Enter ends, Backspace deletes. The name field is `name[12]`, so at most 11 characters. | gaps §1.3; GS §7.1 [C] |
| `scoot.btt` layout | name[12], i32 score at +0x0c, f64 checksum at +0x28, trailer c(sum). `thefly_assets::btt` and `docs/formats/btt.md` now match; the old parser read the checksum as the score. | GS §7.1 [C], checked against the shipped file |
| Stats percentage | delivered / available × 100. | AR §8.7 [C] |
| Starting inventories | `MatchState::new` picks Tutorial if the option is on, else Extreme if the match is extreme, else Normal. (O19 for the tutorial plus extreme case.) | GS §5.7 |
| Music pick | One `rand % 2` after the winch draws, only when music is on [I]; odd picks `music_game2`. | GS §2.4 step 3 |
| Scene | Extreme always uses the arena; otherwise inside before the first period and then the period's flag. | GS §6.2; AR §9.1 |
| Fade alpha type | f64. Only the completion tick matters and it is tick 33 at 500 ms either way. | SR |
| Sound file extension case | The frontend resolves it case-insensitively (`thefly_assets::files_with_ext`); W4 acceptance 4 covers every slot. | W4 |

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
- Render every screen from `State`, following SR. Owns the menu clouds and dust, particle effects, and positional sound mixing. Positional audio is open item O14.
- The platform has no frequency/pan API and the renderer has no scale API (open item O10). W4 adds them in `thefly-platform`, or reports that it can't.

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
11. **Stack push** (gaps §3): a pusher of weight 60 moving at vx 300 into a resting object of weight 30 with one 20-weight rider gives the rider and the object speeds from E = |trunc(m s²)|, q = E / (2M), hand-computed in the test. Speed 50 or less changes nothing. Mode 2 with vx 0 draws one R(10) per pushed object, depth first.
12. **Camera** (gaps §4.3): a delta of 0.9 moves no background; a delta of 3.7 adds 3. The right wrap rewrites the accumulator to trunc((src − Wi)/f); the left wrap to trunc(Wi/f − |A|).
13. **Splat end** (gaps §2.2): a boot splat ends when all 11 pieces are at y ≥ 600 (not a constant); a zapper splat after 64 ticks. The kill is recorded the tick it ends and the respawn runs on the next.
14. **Appear** (gaps §2.3): the fly is alive on tick 33 after the appear animation starts.

### W2

1. **Farmer constants** (AR §1.1): level L gives maxSpeedX 650 + 60L and a cooldown of 35 − 5L ticks. The first voice timer is 10000 ms. `Farmer::new` on a fresh `MsvcRand::new(1)` gives x = 41 % 800 = 41 and y = 351.
2. **Movement** (AR §1.2): vx steps by ±400 per tick; vy by ±200, clamped to ±400. y stays in 351..472; reach = (1 − (472 − y)/121) × 375.
3. **Swing and hit** (AR §1.3–§1.4): a Tab press edge starts a swing. A press at cooldown 0 winds up on ticks 1–2, is active on ticks 3–5 and retracts on ticks 6–9. The minimum swing period is 9 + 36 − 5L ticks: 45 at L 0, 20 at L 5. Hit test: centre (x + 65, trunc(77 + reach)), radius 65 + flyW/2. Damage is trunc((40 + 6L) × f), or 34 with Nuclear. Holding Tab does not repeat.
4. **Triple and Nuclear** (AR §1.6–§1.7): Triple bursts 3 swings and ends after more than 6 bursts. Nuclear lasts 5000 ms from the goodie and 20000 ms from the mystery, and Triple turns it off.
5. **AI parameters** (AR §2.2): `AI_PARAMS[1]` has align 40, spread 181, cut 60, dwell 1000. A one-player scenario, seed 1, 30 s with no input: the farmer moves toward the fly within alignTol, swings, and starts no goodie it does not own.
6. **AI timing** (AR §2.4): AI decisions run on the 700 ms (and listed) timers, at ticks 46, 92 and 138 for 700 ms.
7. **Traps** (GS §4.6, FW §7.4): flypaper divides v by 1.4 per overlapping tick. A fan damages at most once per 100 ms. A zapper grills the fly (splat kind 13, sound 43). Building the traps draws R(10) for each flypaper under the cap of 7, and a draw above 4 sets `variant2` (gaps §8.2); the seed-1 draw is hand-computed.
8. **Goodies** (GS §4.7, AR §5): Right Shift cycles the selector 0→1→2→3→0; Enter with count 0 does nothing; with count > 0 it uses one, plays slot 38 and cancels this tick's action. A mystery lasts 15000 ms.
9. **Boot** (FW §7): after 9 stomps the boot leaves; a stomp that splats the fly sets the count to 15. The splat records a kill.
10. **Midges** (FW §8): one use adds min(100, 300 − active); a swat kills at most 20; the farmer slows while midges touch him.
11. **Flying goodies** (FW §6): first checks at 8000 + R(7000) ms; lifetimes 8000 (fly) and 12000 (farmer) ms; kind 15 (mystery) when R(100) < 3. The trailing side, by `luck`, spawns more often in a long seeded run. The re-roll is `R(100) <= 2q` on the fly side and `< 2q` on the farmer side. The farmer catches only at swing frame 3 or later, against the box (x, trunc(reach), 131, 154), bounds inclusive (gaps §5).

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
14. **High scores** (AR §8.5, GS §7.1): insert at the first row with a score ≤ the new one. Checksum c(1) = 1.74625. The shipped `scoot.btt` round-trips through decode and encode, with the filler draws taken from the rand stream. The parser side is done and tested against the shipped file (`thefly-assets` `release.rs`); names hold at most 11 characters.

### W4

1. Runs the real data from the boot screen to the main menu, with the logo fade and the "Release 1.2" label. Screenshot checked.
2. A 2P scenario played by script (Xvfb) reaches gameplay, renders the period start banner and plays sounds. Screenshot checked.
3. Interpolation: a fly moving at constant vx renders at monotonic x across frames at 144 Hz, including across the 4000-px wrap.
4. The frontend's sound map test checks that every `SoundSlot` resolves to a file in `data/sons`.

## Integration order

1. **Skeleton and constructors** (done): everything compiles; the logo scenario and the two-player construction scenario (`World::bare`) pass.
2. **W1, W2 and W3 in parallel**, each in its own worktree on top of the skeleton. `MatchState::new`, `Economy::new` and `PeriodClock` already exist, so W1 and W2 build a `MatchState` and a bare world in their tests without waiting for W3's `advance`, `meter` and the screens.
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

Reconciled against `docs/re/game-spec.md` and `docs/re/gaps.md` (which wins, §10). The earlier list had 44 items: 24 are closed (answered or turned into a decision above, with citations) and 20 remain, narrowed to what the docs still do not say. Three more came up in the code and are numbered O21 to O23. Each item names where its constant or `todo!` lives. A worker who must pick a value names it as a constant, cites the item and adds a test.

| # | What is open | Doc reference | Where |
|---|---|---|---|
| O1 | How long the crash animation runs (the dying path: "crash falls toward (850, 850)", `FUN_0043a158`), so when "crash finished" fires. | FW §3.6 A, D | W1 `fly.rs` `crash_timer` |
| O2 | Count and spawn rule of the ambient midges (game+0x15a20, count game+0x17898). The contact routine and the box are decoded. | FW §2 step 14, §8.2; gaps §8.1 | W2 `midges.rs` |
| O3 | Bonus round: the intro (counter 3..0), the play-field layout, the coins' leftward motion ([I] from `FUN_004452a1`), and how the bonus fly and swatter move. Coin entry and the rules are decoded. | GS open 30; gaps §7.3, §11.5; AR §8.3 | W3 `bonus.rs` |
| O4 | Goodies and traps shop render: counter text, the belt/machine/cockroach sequence, the side-switch animation, icon positions and font, whether the backdrop slides in. The conveyor delay between a purchase and its grant has no number either. | GS open 32, 33; SR §3.1, §3.2 | W3 `shop.rs`, W4 |
| O5 | Farewell timing and choreography. | GS open 34; SR §5.4 | W3 `screen.rs` |
| O6 | `color_shade` cell colours (cell 1 assumed black) and the 1P keys-page fill. | GS open 35 | W4 |
| O7 | What the original does when `boite.btt` is missing. The shipped file holds `1, 1`. `Options::default()` is music on, tutorial off, which is a guess. | GS §7.2 | W3 `saves.rs` |
| O8 | The lift hook offset `off` (constructor argument). `HOOK_OFF = 0` is a placeholder; the hook test needs the real value. | FW §5.6 | W1 `lift.rs` |
| O9 | Fan strength (+0x91c) and blow schedule, the zapper arc schedule (`FUN_00428b47`), and whether arcs really kill midges [I]. | FW §7.4, §8.2; GS open 37 | W2 `traps.rs` |
| O10 | Platform gaps: the audio API has no frequency or pan control, and the renderer has no scale. | GS §8.1 (helpers `FUN_0040ced5`, `FUN_0040cf01`) | W4, `thefly-platform` |
| O11 | Ambient animal sounds (slots 14 to 21): interval and choice rule. | GS §4.1; `FUN_00441b3c` | W2 `ambient.rs` |
| O12 | Tutorial hint bubbles: the sequence and the triggers. | GS §2.5; `FUN_00443772`, `FUN_00443488` | W2 `ambient.rs` |
| O13 | Which steps freeze while the quit dialog is open, besides the boot (FW §2 step 7), the AI (AR §2) and the period clock (AR §8.6). The plan freezes the whole `World::tick`. | GS §2.2 | W1/W2/W3 |
| O14 | The positional sound curve (`FUN_0040d005`, `FUN_0040d1b2`). `Pan::ScreenX` carries x and w; the frontend owns the curve. | GS §8.1 | W4 |
| O15 | Quit dialog: Esc behaviour and the first-show button offset (render only). | GS §2.2 rows 3c/3f/42 | W3 `screen.rs`, W4 |
| O16 | Loading: the step count of 9 is inferred; which world timers are armed before the 2.3 s of waits (the original builds the world during them); whether the world ticks under the "3, 2, 1, Go!" banner. | SR §4.1 | W3 `screen.rs` |
| O17 | High-score name entry: what Space and Esc do. Letters, digits, Enter and Backspace are decoded, and the name holds at most 11 characters. | gaps §1.3; GS §7.1 | W3 `highscores.rs` |
| O18 | Bourse class mask (`0x4672e8`) beyond "object in play this scene". | GS open 36; SR §3.3 | W3 `bourse.rs` |
| O19 | Tutorial together with 2P Extreme: GS §5.7 lists three inventory rows and does not say which wins. `MatchState::new` uses the Tutorial row. | GS §5.7; AR §7.7 | W3 `match_state.rs` |
| O20 | Transition tick: a new screen first ticks on the tick after the one that created it, including after a quit-dialog YES. The original runs separate init, run and cleanup states, so it may differ by a tick per transition. | GS §2.2 | W3 `screen.rs` |
| O21 | The order of ports 18a and 18b inside the original's render pass `FUN_00403e36`. The port runs `midges_step`, then `fly_render_driven`. | GS §6.3 | `world/mod.rs` |
| O22 | The exact span test of the trap check `FUN_00427bb4` that placement uses. | FW §5.2 step 3 | W2 `traps.rs` `span_free` |
| O23 | What sets the boot's ready flag (+0x178); the stomp reads it. | FW §7.2 step 7 | W2 `boot.rs` |

Also open and not blocking: GS open 3 (unreferenced files), 12 (no screenshots; layouts are [C]/[I], not observed) and 31 (the 65 Hz rate is from a timer constant, not measured).

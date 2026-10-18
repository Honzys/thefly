//! W3. Match statistics counters and the stats page values (ai-and-rules §8.7). Reset per match.

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{Event, PlayParams, SoundOp, SoundSlot};
use crate::highscores::{HighScoreScreen, HighScoreTable};
use crate::input::{KeyState, Scancode};
use crate::match_state::MatchState;
use crate::screen::{FADE_END_MS, Fade, Flow, MatchFlow, Screen};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub periods: i32,
    pub delivered: i32,
    pub available: i32,
    pub price_total: i32,
    pub swings: i32,
    pub hits: i32,
    pub damage: i32,
    pub kills: i32,
    pub survival_total_ms: u64,
    pub survival_max_ms: u64,
}

/// The values the stats page shows; a zero denominator shows 0.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatsPage {
    pub delivered: i32,
    pub delivered_percent: f64,
    pub max_survival_s: f64,
    pub avg_survival_s: f64,
    pub avg_price: f64,
    pub swings: i32,
    pub kills: i32,
    pub kills_per_period: f64,
    pub hit_ratio_percent: f64,
    pub avg_damage: f64,
}

impl Stats {
    /// Fly life ended: add to the total and the max.
    pub fn record_life(&mut self, ms: u64) {
        self.survival_total_ms += ms;
        self.survival_max_ms = self.survival_max_ms.max(ms);
    }

    pub fn page(&self) -> StatsPage {
        let ratio = |num: f64, den: i32| if den == 0 { 0.0 } else { num / f64::from(den) };
        let average_price = if self.delivered == 0 {
            0
        } else {
            self.price_total / self.delivered
        };
        StatsPage {
            delivered: self.delivered,
            delivered_percent: ratio(f64::from(self.delivered), self.available) * 100.0,
            max_survival_s: self.survival_max_ms as f64 / 1000.0,
            avg_survival_s: self.survival_total_ms as f64 / 1000.0 / f64::from(self.kills + 1),
            // Shown as a whole number (screens-and-rendering §5.2).
            avg_price: f64::from(average_price),
            swings: self.swings,
            kills: self.kills,
            kills_per_period: ratio(f64::from(self.kills), self.periods),
            hit_ratio_percent: ratio(f64::from(self.hits), self.swings) * 100.0,
            avg_damage: ratio(f64::from(self.damage), self.swings),
        }
    }
}

/// The statistics screen (0xaa), with the 1P highscore breakdown (screens-and-rendering §5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct StatsScreen {
    pub page: StatsPage,
    pub fade: Fade,
    pub leaving: bool,
    /// The fade-out runs once this has passed and a key is pressed.
    pub min_show: Timer,
    /// Highscore mode: points counted up by 10 per tick, then kills one per 30 ms.
    pub breakdown: Option<Breakdown>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Breakdown {
    pub points: i32,
    pub kills: i32,
    /// Unarmed while the points count; armed 1500 ms when they finish, then 30 ms per kill.
    pub kill_timer: Timer,
    /// The counting is over (naturally, or skipped by a held key).
    pub finished: bool,
    /// Armed 500 ms by the skip; created expired, so a natural finish leaves at once.
    pub leave_after: Timer,
}

pub const MIN_SHOW_MS: u64 = 3000;
pub const BREAKDOWN_KILLS_AFTER_MS: u64 = 1500;
pub const BREAKDOWN_KILL_STEP_MS: u64 = 30;

/// The recorded high score loses this per kill (ai-and-rules §8.5).
pub const KILL_PENALTY: i32 = 10;
/// The second key's fade-out is armed this long after the first (SR §5.2).
pub const SECOND_KEY_MS: u64 = 500;

impl StatsScreen {
    pub fn new(m: &MatchState, cx: &mut Ctx) -> Self {
        let breakdown = m.config.highscore().then(|| Breakdown {
            points: 0,
            kills: 0,
            kill_timer: Timer::UNARMED,
            finished: false,
            leave_after: Timer::armed(cx.now, 0),
        });
        Self {
            page: m.stats.page(),
            fade: Fade::new(false, FADE_END_MS),
            leaving: false,
            min_show: Timer::armed(cx.now, MIN_SHOW_MS),
            breakdown,
        }
    }

    /// The breakdown's result: score − 10 × kills (ai-and-rules §8.5).
    pub fn recorded(m: &MatchState) -> i32 {
        m.economy.fly.score - KILL_PENALTY * m.stats.kills
    }

    /// Highscore mode with a qualifying score → high scores with name entry, else → menu.
    pub fn tick(
        &mut self,
        keys: &KeyState,
        m: &MatchState,
        table: &HighScoreTable,
        cx: &mut Ctx,
    ) -> MatchFlow {
        let faded = self.fade.step();
        if self.leaving {
            return if faded {
                MatchFlow::Leave(exit(m, table, cx))
            } else {
                MatchFlow::Stay
            };
        }
        let leave = match &mut self.breakdown {
            Some(b) => b.tick(keys, m.economy.fly.score, m.stats.kills, cx),
            None => keys.skip() && self.min_show.expired(cx.now),
        };
        if leave {
            self.leaving = true;
            self.fade = Fade::new(true, FADE_END_MS);
        }
        MatchFlow::Stay
    }
}

impl Breakdown {
    /// `FUN_0044c73d`: count, then the keys, read held (answers.md "Stats breakdown"). True
    /// when the fade-out should start.
    fn tick(&mut self, keys: &KeyState, score: i32, kills: i32, cx: &mut Ctx) -> bool {
        self.count(score, kills, cx);
        if !keys.held(Scancode::ESCAPE) && !keys.held(Scancode::RETURN) {
            return false;
        }
        if self.finished {
            return self.leave_after.expired(cx.now);
        }
        (self.points, self.kills, self.finished) = (score, kills, true);
        cx.emit(Event::play(SoundSlot::MONEY));
        self.leave_after.arm(cx.now, SECOND_KEY_MS);
        false
    }

    /// Points +10 per tick (the capping tick and a score ≤ 0 included), then from 1500 ms after
    /// they finish one kill per 30 ms, every step with a tick; slot 11 at the end. With no kills
    /// there is still one kill step.
    fn count(&mut self, score: i32, kills: i32, cx: &mut Ctx) {
        if self.finished {
            return;
        }
        if !self.kill_timer.is_armed() {
            self.points = (self.points + 10).min(score);
            ring(21_500, cx);
            if self.points >= score {
                self.kill_timer.arm(cx.now, BREAKDOWN_KILLS_AFTER_MS);
            }
            return;
        }
        if !self.kill_timer.expired_rearm(cx.now) {
            return;
        }
        if self.kills == 0 {
            self.kill_timer.arm(cx.now, BREAKDOWN_KILL_STEP_MS);
        }
        self.kills += 1;
        ring(18_500, cx);
        if self.kills >= kills {
            self.kills = kills;
            self.finished = true;
            cx.emit(Event::play(SoundSlot::MONEY));
        }
    }
}

/// The count tick: slot 49 at `base` + R(500). shortcut: the original switches to slot 50 when
/// 49 has stopped and 50 has not (`FUN_0040cf55`); the sim has no slot state, so always 49.
fn ring(base: u32, cx: &mut Ctx) {
    let freq = base + cx.rng.r(500) as u32;
    cx.sound(SoundSlot::RING_A, play_at(freq));
}

fn play_at(freq: u32) -> SoundOp {
    SoundOp::Play(PlayParams {
        freq_hz: Some(freq),
        ..PlayParams::default()
    })
}

/// ai-and-rules §8.5: qualify on the fly score; the entry stores the lower recorded value.
fn exit(m: &MatchState, table: &HighScoreTable, cx: &mut Ctx) -> Flow {
    let row = table.qualifies(m.economy.fly.score);
    match row.filter(|_| m.config.highscore()) {
        Some(row) => {
            let entry = Some((row, StatsScreen::recorded(m)));
            Flow::Go(Screen::HighScores(Box::new(HighScoreScreen::new(
                entry, cx,
            ))))
        }
        None => Flow::Menu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lives_add_to_the_total_and_the_max() {
        let mut s = Stats::default();
        s.record_life(4000);
        s.record_life(1500);
        assert_eq!((s.survival_total_ms, s.survival_max_ms), (5500, 4000));
    }

    /// AR §8.7 page values.
    #[test]
    fn the_page_divides_with_zero_for_an_empty_denominator() {
        assert_eq!(Stats::default().page(), StatsPage::default());
        let s = Stats {
            periods: 4,
            delivered: 6,
            available: 24,
            price_total: 200,
            swings: 40,
            hits: 10,
            damage: 300,
            kills: 3,
            survival_total_ms: 60_000,
            survival_max_ms: 30_500,
        };
        let p = s.page();
        assert_eq!(p.delivered_percent, 25.0);
        assert_eq!(p.max_survival_s, 30.5);
        assert_eq!(p.avg_survival_s, 15.0); // 60 s / (3 + 1)
        assert_eq!(p.avg_price, 33.0); // 200 / 6, whole
        assert_eq!(p.kills_per_period, 0.75);
        assert_eq!(p.hit_ratio_percent, 25.0);
        assert_eq!(p.avg_damage, 7.5);
    }

    use crate::match_state::{MatchConfig, Mode, Players};
    use crate::rng::MsvcRand;
    use crate::screen::rig::Rig;

    /// A Highscore match with the given fly score and kills, and a seed-1 stream for the screen.
    fn highscore(rig: &mut Rig, score: i32, kills: i32) -> MatchState {
        let config = MatchConfig {
            players: Players::One,
            mode: Mode::Special,
            ..MatchConfig::default()
        };
        let mut m = rig.cx(|cx| MatchState::new(config, false, cx));
        m.economy.fly.score = score;
        m.stats.kills = kills;
        rig.rng = MsvcRand::new(1);
        m
    }

    /// (now, slot, freq) of every sound the breakdown plays.
    fn sounds(
        rig: &mut Rig,
        s: &mut StatsScreen,
        m: &MatchState,
        ticks: usize,
        held: &[Scancode],
    ) -> Vec<(u64, u8, Option<u32>)> {
        let table = HighScoreTable::default();
        let mut out = Vec::new();
        for _ in 0..ticks {
            let now = rig.now();
            let before = rig.events.len();
            rig.step(held, |k, cx| s.tick(k, m, &table, cx));
            for e in &rig.events[before..] {
                if let Event::Sound {
                    slot,
                    op: SoundOp::Play(p),
                } = e
                {
                    out.push((now, slot.0, p.freq_hz));
                }
            }
        }
        out
    }

    /// answers.md "Stats breakdown": +10 per tick with a 21500-band tick each (seed 1: 41, 18467,
    /// 6334 → 21541, 21967, 21834); the kill timer is armed when the points finish (tick 2, 30 ms)
    /// so the first kill fires at 1538 ms (tick 100), the next 30 ms later (tick 102, 1569 ms) in
    /// the 18500 band (26500, 19169 → 18500, 18669), then slot 11.
    #[test]
    fn the_breakdown_counts_kills_from_the_end_of_the_points() {
        let mut rig = Rig::new(1);
        let m = highscore(&mut rig, 25, 2);
        let mut s = rig.cx(|cx| StatsScreen::new(&m, cx));
        let got = sounds(&mut rig, &mut s, &m, 200, &[]);
        assert_eq!(
            got,
            [
                (0, 49, Some(21_541)),
                (15, 49, Some(21_967)),
                (30, 49, Some(21_834)),
                (1538, 49, Some(18_500)),
                (1569, 49, Some(18_669)),
                (1569, 11, None),
            ]
        );
        let b = s.breakdown.unwrap();
        assert_eq!((b.points, b.kills, b.finished), (25, 2, true));
    }

    /// A score ≤ 0 and no kills still give one tick each, then slot 11.
    #[test]
    fn a_zero_breakdown_still_ticks_once_per_phase() {
        let mut rig = Rig::new(1);
        let m = highscore(&mut rig, 0, 0);
        let mut s = rig.cx(|cx| StatsScreen::new(&m, cx));
        let got = sounds(&mut rig, &mut s, &m, 200, &[]);
        // 1500 ms after tick 0: tick 98 (1507 ms).
        assert_eq!(
            got,
            [
                (0, 49, Some(21_541)),
                (1507, 49, Some(18_967)),
                (1507, 11, None)
            ]
        );
        assert_eq!(s.breakdown.map(|b| (b.points, b.kills)), Some((0, 0)));
    }

    /// Keys are read held: a held Enter jumps to the final numbers with slot 11, then the fade
    /// starts once the 500 ms timer has run, with the key still held.
    #[test]
    fn a_held_key_skips_the_count_then_leaves_after_500_ms() {
        let mut rig = Rig::new(1);
        let m = highscore(&mut rig, 500, 3);
        let mut s = rig.cx(|cx| StatsScreen::new(&m, cx));
        let got = sounds(&mut rig, &mut s, &m, 1, &[Scancode::RETURN]);
        assert_eq!(got, [(0, 49, Some(21_541)), (0, 11, None)]);
        assert_eq!(
            s.breakdown.map(|b| (b.points, b.kills, b.finished)),
            Some((500, 3, true))
        );
        let table = HighScoreTable::default();
        while rig.now() < 500 {
            rig.step(&[Scancode::RETURN], |k, cx| s.tick(k, &m, &table, cx));
            assert!(!s.leaving, "{}", rig.now());
        }
        rig.step(&[Scancode::RETURN], |k, cx| s.tick(k, &m, &table, cx));
        assert!(s.leaving);
    }
}

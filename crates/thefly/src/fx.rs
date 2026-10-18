//! Cosmetics the sim leaves to the frontend: menu clouds and dust, and the particle effects of
//! [`Effect`]. They run per tick on their own random stream, never the sim's.

use thefly_platform::Error;
use thefly_sim::MsvcRand;
use thefly_sim::event::Effect;

use crate::gfx::Gfx;
use crate::lerp::Lerp;

const DT: f32 = 1.0 / 65.0;

const CLOUDS: usize = 15;
const CLOUD_W: f32 = 317.0;
/// Live particles at most; a burst past this drops its extra particles.
const MAX_PARTICLES: usize = 600;
const MENU_DUST: usize = 15;
const STARDUST_BURST: usize = 30;
const SPLAT_BLOOD: usize = 60;
const SPLAT_PIECES: usize = 11;
const SHOCK_SPARKS: usize = 10;
/// SR §4.6: the expanding ring tweens over 600 ms.
const RING_MS: f32 = 600.0;
/// answers.md "Crash animation length": the 339 × 244 crash sprite scales 0.3 → 1.0 over 200 ms
/// (13 ticks at 65 fps), and a 60-particle `blood` spray launches at ±0..850 px/s.
const CRASH_MS: f32 = 200.0;
const CRASH_SCALE: f32 = 0.3;
/// shortcut: when the crash sprite goes is not in the docs; it stays until the 1500 ms respawn
/// (fly-and-world §3.6 D). Revisit if a capture shows the dying fly.
const RESPAWN_MS: f32 = 1500.0;
const CRASH_SPRAY: usize = 60;
/// The fly frame the spray is spread over (the event carries only the top-left).
const FLY_SIZE: (i32, i32) = (120, 94);
const SPRAY_SPEED: i32 = 850;

/// A menu cloud (SR §2.2): drifts and wraps around the screen.
#[derive(Debug, Clone, Copy)]
struct Cloud {
    x: f32,
    prev_x: f32,
    y: f32,
    vx: f32,
    frame: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Particle {
    sprite: &'static str,
    frame: u32,
    frames: u32,
    frame_ms: f32,
    age_ms: f32,
    life_ms: f32,
    x: f32,
    y: f32,
    prev: (f32, f32),
    vx: f32,
    vy: f32,
    /// px/s added to vy every tick (gaps §2.1 "true gravity").
    gravity: f32,
    scale: f32,
    scale_step: f32,
    alpha: f32,
    alpha_step: f32,
    /// The scale stops here (the crash tween ends when it passes 1.0).
    scale_max: f32,
    /// Drawn and scaled from its top-left, as the D3D path does (SR §1.1), not centred.
    top_left: bool,
}

impl Particle {
    fn new(sprite: &'static str, x: f32, y: f32) -> Self {
        Self {
            sprite,
            frame: 0,
            frames: 1,
            frame_ms: 50.0,
            age_ms: 0.0,
            life_ms: f32::INFINITY,
            x,
            y,
            prev: (x, y),
            vx: 0.0,
            vy: 0.0,
            gravity: 0.0,
            scale: 1.0,
            scale_step: 0.0,
            alpha: 255.0,
            alpha_step: 0.0,
            scale_max: f32::INFINITY,
            top_left: false,
        }
    }

    fn step(&mut self) {
        self.prev = (self.x, self.y);
        self.age_ms += DT * 1000.0;
        self.vy += self.gravity;
        self.x += self.vx * DT;
        self.y += self.vy * DT;
        self.scale += self.scale_step;
        if self.scale > self.scale_max {
            self.scale = self.scale_max;
            self.scale_step = 0.0;
        }
        self.alpha += self.alpha_step;
    }

    fn alive(&self) -> bool {
        self.age_ms < self.life_ms && self.alpha > 0.0 && self.scale > 0.0 && self.y < 900.0
    }
}

pub struct Fx {
    rng: MsvcRand,
    clouds: Vec<Cloud>,
    particles: Vec<Particle>,
}

impl Fx {
    pub fn new(seed: u32) -> Self {
        let mut rng = MsvcRand::new(seed ^ 0x5eed_f00d);
        let clouds = (0..CLOUDS)
            .map(|_| {
                let x = (rng.r(1000) - 100) as f32;
                let y = (rng.r(750) - 50) as f32;
                let speed = (rng.r(20) + 10) as f32;
                Cloud {
                    x,
                    prev_x: x,
                    y,
                    vx: speed * rng.sign() as f32,
                    frame: rng.r(3) as u32,
                }
            })
            .collect();
        Self {
            rng,
            clouds,
            particles: Vec::new(),
        }
    }

    fn r(&mut self, lo: i32, hi_incl: i32) -> f32 {
        (self.rng.r(hi_incl - lo + 1) + lo) as f32
    }

    pub fn step(&mut self) {
        for c in &mut self.clouds {
            c.prev_x = c.x;
            c.x += c.vx * DT;
            if c.x > 800.0 {
                c.x = -CLOUD_W;
                c.prev_x = c.x;
            } else if c.x < -CLOUD_W {
                c.x = 800.0;
                c.prev_x = c.x;
            }
        }
        for p in &mut self.particles {
            p.step();
        }
        self.particles.retain(Particle::alive);
    }

    pub fn clear(&mut self) {
        self.particles.clear();
    }

    fn push(&mut self, p: Particle) {
        if self.particles.len() < MAX_PARTICLES {
            self.particles.push(p);
        }
    }

    /// The pig's landing dust (SR §2.2): x 120..569 at least 10 px apart, y 560..599, growing from
    /// 0.1 by (3..6)/100 per tick while alpha falls from 200 − 2s by s per tick.
    pub fn menu_dust(&mut self) {
        const MIN_GAP: f32 = 10.0;
        const TRIES: usize = 20;
        let mut xs: Vec<f32> = Vec::with_capacity(MENU_DUST);
        for _ in 0..MENU_DUST {
            let x = (0..TRIES)
                .map(|_| self.r(120, 569))
                .find(|x| xs.iter().all(|o| (o - x).abs() >= MIN_GAP));
            let Some(x) = x else { continue };
            xs.push(x);
            let y = self.r(560, 599);
            self.puff(x, y, 200.0);
        }
    }

    /// One growing, fading `DUST` puff centred at `(x, y)`.
    fn puff(&mut self, x: f32, y: f32, alpha_max: f32) {
        let s = self.r(3, 6);
        let frame = self.rng.r(3) as u32;
        self.push(Particle {
            frame,
            scale: 0.1,
            scale_step: s / 100.0,
            alpha: alpha_max - 2.0 * s,
            alpha_step: -s,
            ..Particle::new("DUST", x, y)
        });
    }

    /// `count` puffs spread over a `w × h` box at `(x, y)` (winner trophy, boot stomp).
    pub fn dust_burst(&mut self, x: f32, y: f32, w: i32, h: i32, count: usize, alpha_max: f32) {
        for _ in 0..count {
            let px = x + self.r(0, w.max(1) - 1);
            let py = y + self.r(0, h.max(1) - 1);
            self.puff(px, py, alpha_max);
        }
    }

    /// `stardust` sparkles over a box: ±(30..79) px/s, 20–79 ms frames, fading in 400–499 ms
    /// (SR §5.3, used for every stardust burst).
    pub fn stardust(&mut self, x: f32, y: f32, w: i32, h: i32, count: usize) {
        for _ in 0..count {
            let px = x + self.r(0, w.max(1) - 1);
            let py = y + self.r(0, h.max(1) - 1);
            let vx = self.r(30, 79) * self.rng.sign() as f32;
            let vy = self.r(30, 79) * self.rng.sign() as f32;
            let life = self.r(400, 499);
            let frame_ms = self.r(20, 79);
            self.push(Particle {
                frames: 3,
                frame_ms,
                vx,
                vy,
                life_ms: life,
                alpha_step: -255.0 / (life / 1000.0 * 65.0),
                ..Particle::new("stardust", px, py)
            });
        }
    }

    pub fn effect(&mut self, e: Effect) {
        match e {
            Effect::Stardust { x, y } => {
                self.stardust(x as f32 - 40.0, y as f32 - 40.0, 80, 80, STARDUST_BURST)
            }
            Effect::Dust { x, y, size } => {
                let n = usize::try_from(size.clamp(1, 8)).unwrap_or(1);
                self.dust_burst(x as f32 - 20.0, y as f32 - 10.0, 40, 20, n, 200.0)
            }
            Effect::Stomp { x, y } => {
                self.dust_burst(x as f32 - 60.0, y as f32 - 20.0, 120, 30, 10, 200.0)
            }
            Effect::GoodieSpawn { x, y } => {
                self.stardust(x as f32 - 25.0, y as f32 - 25.0, 50, 50, 10)
            }
            Effect::Shockwave {
                x,
                y,
                scale,
                sparks,
            } => self.shockwave(x, y, scale, sparks),
            Effect::Crash { x, y } => self.crash(x, y),
            Effect::Splat { x, y, dir } => self.splat(x as f32, y as f32, dir),
            // Drawn by the frontend owner; the sim emits them from the zapper arc and the fly.
            Effect::Spark { .. } | Effect::Sparkles { .. } => {}
        }
    }

    /// The crash sprite at the fly's top-left and its blood spray (answers.md "Crash animation
    /// length"). The spray's rand is the sim's; this one only looks the same: positions over the
    /// fly frame, frames 1..=3, speeds ±0..850 with no gravity, each fading in R(100) + 850 ms
    /// (gaps §2.2).
    fn crash(&mut self, x: i32, y: i32) {
        let ticks = CRASH_MS / 1000.0 * 65.0;
        self.push(Particle {
            life_ms: CRASH_MS + RESPAWN_MS,
            scale: CRASH_SCALE,
            scale_step: (1.0 - CRASH_SCALE) / ticks,
            scale_max: 1.0,
            top_left: true,
            ..Particle::new("crash-face0011", x as f32, y as f32)
        });
        for _ in 0..CRASH_SPRAY {
            let px = (x + self.rng.r(FLY_SIZE.0 - 20)) as f32;
            let py = (y + self.rng.r(FLY_SIZE.1 - 20)) as f32;
            let frame = self.rng.r(3) as u32 + 1;
            let vx = self.r(0, SPRAY_SPEED) * self.rng.sign() as f32;
            let vy = self.r(0, SPRAY_SPEED) * self.rng.sign() as f32;
            let life = self.r(850, 949);
            self.push(Particle {
                frame,
                vx,
                vy,
                life_ms: life,
                alpha_step: -255.0 / (life / 1000.0 * 65.0),
                ..Particle::new("blood", px, py)
            });
        }
    }

    /// SR §4.6: `onde_frappe` from scale 0.1 to its display scale (percent) with alpha 255 → 0.
    fn shockwave(&mut self, x: i32, y: i32, scale_percent: i32, sparks: bool) {
        let ticks = RING_MS / 1000.0 * 65.0;
        let end = scale_percent as f32 / 100.0;
        self.push(Particle {
            scale: 0.1,
            scale_step: (end - 0.1) / ticks,
            alpha_step: -255.0 / ticks,
            life_ms: RING_MS,
            ..Particle::new("onde_frappe", x as f32, y as f32)
        });
        if !sparks {
            return;
        }
        for _ in 0..SHOCK_SPARKS {
            let vx = self.r(100, 300) * self.rng.sign() as f32;
            let vy = -self.r(100, 400);
            self.push(Particle {
                frames: 3,
                vx,
                vy,
                gravity: 20.0,
                life_ms: 800.0,
                ..Particle::new("etincelle", x as f32, y as f32)
            });
        }
    }

    /// gaps §2.1: 60 blood particles and 11 body pieces. `dir` 0 is the boot burst, otherwise the
    /// fan's push direction.
    fn splat(&mut self, x: f32, y: f32, dir: i32) {
        let (vx_lo, vx_hi, vy_lo, vy_hi, g) = match dir.signum() {
            0 => (-280, 280, 0, 200, 12.0),
            1 => (550, 950, -550, -152, 20.0),
            _ => (-850, -152, -550, -152, 20.0),
        };
        for i in 0..SPLAT_BLOOD + SPLAT_PIECES {
            let px = x + self.r(0, 99);
            let py = y + self.r(0, 73);
            let vx = self.r(vx_lo, vx_hi);
            let vy = self.r(vy_lo, vy_hi);
            let base = if i < SPLAT_BLOOD {
                Particle {
                    frame: self.rng.r(6) as u32,
                    ..Particle::new("blood", px, py)
                }
            } else {
                Particle {
                    frame: (i - SPLAT_BLOOD) as u32,
                    ..Particle::new("eclatement_fly", px, py)
                }
            };
            self.push(Particle {
                vx,
                vy,
                gravity: g,
                ..base
            });
        }
    }

    pub fn draw_clouds(&self, g: &mut Gfx, l: Lerp, dy: f32) -> Result<(), Error> {
        for c in &self.clouds {
            let x = l.f(Some(f64::from(c.prev_x)), f64::from(c.x));
            g.spr("i_nuage", x, c.y + dy, c.frame)?;
        }
        Ok(())
    }

    pub fn draw_particles(&self, g: &mut Gfx, l: Lerp) -> Result<(), Error> {
        for p in &self.particles {
            let x = l.f(Some(f64::from(p.prev.0)), f64::from(p.x));
            let y = l.f(Some(f64::from(p.prev.1)), f64::from(p.y));
            let frame = p.frame + (p.age_ms / p.frame_ms) as u32 % p.frames.max(1);
            let alpha = p.alpha.clamp(0.0, 255.0);
            if p.top_left {
                g.spr_scaled(p.sprite, x, y, frame, p.scale, alpha)?;
            } else {
                g.spr_centred(p.sprite, x, y, frame, p.scale, alpha)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Fx {
        fn crash_scale(&self) -> Option<f32> {
            self.particles.iter().find(|p| p.top_left).map(|p| p.scale)
        }
    }

    #[test]
    fn menu_dust_grows_and_fades_out() {
        let mut fx = Fx::new(1);
        fx.menu_dust();
        assert!(!fx.particles.is_empty());
        let mut xs: Vec<f32> = fx.particles.iter().map(|p| p.x).collect();
        xs.sort_by(f32::total_cmp);
        assert!(xs.windows(2).all(|w| w[1] - w[0] >= 10.0));
        assert!(fx.particles.iter().all(|p| (120.0..=569.0).contains(&p.x)));
        // alpha ≤ 194 falls by ≥ 3 a tick: gone within 65 ticks
        for _ in 0..65 {
            fx.step();
        }
        assert!(fx.particles.is_empty());
    }

    #[test]
    fn boot_splat_falls_off_screen() {
        let mut fx = Fx::new(1);
        fx.effect(Effect::Splat {
            x: 300,
            y: 250,
            dir: 0,
        });
        assert_eq!(fx.particles.len(), SPLAT_BLOOD + SPLAT_PIECES);
        const LIMIT: usize = 400;
        for _ in 0..LIMIT {
            fx.step();
        }
        assert!(fx.particles.is_empty());
    }

    #[test]
    fn a_crash_scales_in_from_its_top_left_and_sprays_blood_that_fades() {
        let mut fx = Fx::new(1);
        fx.effect(Effect::Crash { x: 300, y: 200 });
        let crash: Vec<&Particle> = fx.particles.iter().filter(|p| p.top_left).collect();
        assert_eq!(crash.len(), 1);
        assert_eq!(
            (crash[0].sprite, crash[0].x, crash[0].y),
            ("crash-face0011", 300.0, 200.0)
        );
        assert!((crash[0].scale - 0.3).abs() < 1e-6);
        let blood = fx.particles.iter().filter(|p| p.sprite == "blood").count();
        assert_eq!(blood, CRASH_SPRAY);
        // answers.md "Crash animation length": the scale passes 1.0 on the 13th step at 65 fps
        for _ in 0..12 {
            fx.step();
        }
        assert!(fx.crash_scale().is_some_and(|s| s <= 1.0));
        fx.step();
        assert_eq!(fx.crash_scale(), Some(1.0));
        // the blood fades in at most 949 ms, the crash stays until the 1500 ms respawn
        for _ in 0..62 {
            fx.step();
        }
        assert!(fx.particles.iter().all(|p| p.top_left));
        for _ in 0..50 {
            fx.step();
        }
        assert!(fx.particles.is_empty());
    }

    #[test]
    fn clouds_wrap_around_the_screen() {
        let mut fx = Fx::new(7);
        const TEN_MINUTES: usize = 65 * 600;
        for _ in 0..TEN_MINUTES {
            fx.step();
        }
        assert!(fx.clouds.iter().all(|c| (-CLOUD_W..=800.0).contains(&c.x)));
    }
}

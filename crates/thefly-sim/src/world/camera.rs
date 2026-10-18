//! W1. The camera: a per-tick delta written by the fly's edge scroll (fly-and-world §3.4 step 11,
//! spec correction 1) and the four layer scroll accumulators the renderer reads. Every float to
//! int step truncates (gaps §4.3, which corrects screens-and-rendering §4.2).

use crate::match_state::Scene;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    /// Parallax factor: 0.25, 0.5 or 0.75, 1.0, 1.25 (gaps §4.2).
    pub factor: f64,
    /// Accumulated trunc(delta) per tick, fly-layer pixels. The wrap rewrites it (gaps §4.3).
    pub acc: i64,
    /// Image width, for the wrap.
    pub width: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    /// game+0x407c0: this tick's world shift in fly-layer pixels, assigned (not accumulated).
    pub delta: f64,
    /// Sky, hills, main, table front.
    pub layers: [Layer; 4],
}

/// The images of the four layers for a scene (gaps §4.2): sky, hills, main, table front.
/// The main layer's width is [`super::W`].
pub const fn layer_images(scene: Scene) -> [&'static str; 4] {
    match scene {
        Scene::Inside => ["Paysage", "Jardin", "Maison", "Table"],
        Scene::Outside => ["paysage2", "collines", "dehors", "table2"],
        Scene::Arena => ["paysage2", "collines", "arene", "table2"],
    }
}

impl Camera {
    /// Layers for the scene (gaps §4.2): factors 0.25, 0.75 (inside) or 0.5, 1.0, 1.25; the
    /// hills start at accumulator 50, the others at 0. `widths` are the [`layer_images`] widths.
    pub fn new(scene: Scene, widths: [i32; 4]) -> Self {
        let hills = if scene == Scene::Inside { 0.75 } else { 0.5 };
        let layer = |factor, acc, width| Layer { factor, acc, width };
        Self {
            delta: 0.0,
            layers: [
                layer(0.25, 0, widths[0]),
                layer(hills, 50, widths[1]),
                layer(1.0, 0, widths[2]),
                layer(1.25, 0, widths[3]),
            ],
        }
    }

    /// Per layer, once per tick at the end of step 5 (gaps §4.3): acc += trunc(delta), then the
    /// right wrap (src ≥ width) or the left wrap (acc < 0) rewrites acc. A delta under 1 px
    /// moves no background.
    pub fn scroll(&mut self) {
        let step = self.delta as i64;
        for l in &mut self.layers {
            l.scroll(step);
        }
    }

    /// The source x of layer `i` for the renderer: trunc(acc × factor), after [`Camera::scroll`]'s
    /// wrap.
    pub fn source_x(&self, i: usize) -> i32 {
        self.layers[i].source_x()
    }
}

impl Layer {
    fn source_x(&self) -> i32 {
        (self.acc as f64 * self.factor) as i32
    }

    /// gaps §4.3 steps 1–4, truncations as written (not an exact modulo).
    fn scroll(&mut self, step: i64) {
        self.acc += step;
        let src = self.source_x();
        if src >= self.width {
            self.acc = (f64::from(src - self.width) / self.factor) as i64;
        } else if self.acc < 0 {
            self.acc = (f64::from(self.width) / self.factor - self.acc.abs() as f64) as i64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::world::W;

    #[test]
    fn the_main_layer_is_the_world_width_in_every_scene() {
        let data = GameData::synthetic();
        for scene in [Scene::Inside, Scene::Outside, Scene::Arena] {
            let names = layer_images(scene);
            assert_eq!(data.size(names[2]).w, W, "{scene:?}");
        }
    }

    #[test]
    fn layers_start_as_the_layer_table_says() {
        let c = Camera::new(Scene::Inside, [800, 800, 4000, 900]);
        assert_eq!(c.layers.map(|l| l.factor), [0.25, 0.75, 1.0, 1.25]);
        assert_eq!(c.layers.map(|l| l.acc), [0, 50, 0, 0]);
        assert_eq!(Camera::new(Scene::Arena, [0; 4]).layers[1].factor, 0.5);
    }

    fn sky_at(acc: i64, delta: f64) -> Camera {
        let mut c = Camera::new(Scene::Inside, [800, 800, 4000, 900]);
        c.layers[0].acc = acc;
        c.delta = delta;
        c.scroll();
        c
    }

    /// Acceptance W1 12: D is truncated before it is added.
    #[test]
    fn a_delta_under_one_pixel_moves_no_background() {
        let c = sky_at(100, 0.9);
        assert_eq!(c.layers.map(|l| l.acc), [100, 50, 0, 0]);
        let c = sky_at(100, -0.9);
        assert_eq!(c.layers[0].acc, 100);
        let c = sky_at(100, 3.7);
        assert_eq!(c.layers.map(|l| l.acc), [103, 53, 3, 3]);
        assert_eq!(c.source_x(0), 25);
        assert_eq!(c.source_x(1), 39);
    }

    #[test]
    fn the_right_wrap_rewrites_the_accumulator_from_the_overshoot() {
        // A = 3215, src = trunc(3215 × 0.25) = 803 ≥ 800 → A = trunc(3 / 0.25) = 12, src 3.
        let c = sky_at(3210, 5.0);
        assert_eq!((c.layers[0].acc, c.source_x(0)), (12, 3));
    }

    #[test]
    fn the_left_wrap_rewrites_the_accumulator_from_the_width() {
        // A = 2 + trunc(−5.2) = −3 → A = trunc(800 / 0.25 − 3) = 3197, src = trunc(799.25) = 799.
        let c = sky_at(2, -5.2);
        assert_eq!((c.layers[0].acc, c.source_x(0)), (3197, 799));
        // Main layer, f 1.0: −5 → trunc(4000 − 5) = 3995.
        assert_eq!((c.layers[2].acc, c.source_x(2)), (3995, 3995));
    }
}

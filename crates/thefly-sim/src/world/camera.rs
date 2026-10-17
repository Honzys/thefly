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
        todo!("W1 gaps §4.3")
    }

    /// The source x of layer `i` for the renderer: trunc(acc × factor), after [`Camera::scroll`]'s
    /// wrap.
    pub fn source_x(&self, i: usize) -> i32 {
        let _ = i;
        todo!("W1 gaps §4.3")
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
}

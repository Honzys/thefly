//! W1. The camera: a per-tick delta written by the fly's edge scroll (fly-and-world §3.4 step 11,
//! spec correction 1) and the four layer scroll accumulators the renderer reads
//! (screens-and-rendering §4.2). Open item: the exact accumulation and wrap of
//! `FUN_00441baf`/`FUN_004416b1` (fly-and-world "Still open" 2).

use crate::match_state::Scene;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    /// 0.25, 0.5 or 0.75, 1.0, 1.25.
    pub factor: f64,
    /// Accumulated round(delta) per tick, fly-layer pixels.
    pub acc: i64,
    /// Image width, for the wrap.
    pub width: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    /// game+0x407c0: this tick's world shift in fly-layer pixels, assigned (not accumulated).
    pub delta: f64,
    /// Sky, middle, main, table front.
    pub layers: [Layer; 4],
}

impl Camera {
    /// Layers for the scene (game-spec §6.2). Widths come from the layer images; the main layer
    /// is [`super::W`]. Layer 1 starts at offset 50.
    pub fn new(scene: Scene, widths: [i32; 4]) -> Self {
        let _ = (scene, widths);
        todo!("W1 game-spec §6.2, screens §4.2")
    }

    /// Adds this tick's delta to every layer accumulator (called at the end of step 5).
    pub fn scroll(&mut self) {
        todo!("W1 screens §4.2")
    }

    /// The source x of layer `i` for the renderer: round(acc × factor), wrapped into the width.
    pub fn source_x(&self, i: usize) -> i32 {
        let _ = i;
        todo!("W1 screens §4.2")
    }
}

//! Keyboard and mouse state as seen by one simulation tick.

use std::collections::HashSet;

pub use sdl3::keyboard::Scancode as Key;
pub use sdl3::mouse::MouseButton;

#[derive(Debug, Default, Clone)]
pub struct Input {
    keys_held: HashSet<Key>,
    keys_pressed: HashSet<Key>,
    keys_released: HashSet<Key>,
    buttons_held: HashSet<MouseButton>,
    buttons_pressed: HashSet<MouseButton>,
    buttons_released: HashSet<MouseButton>,
    mouse: (f32, f32),
}

impl Input {
    /// Went down since the previous tick.
    pub fn pressed(&self, k: Key) -> bool {
        self.keys_pressed.contains(&k)
    }
    pub fn held(&self, k: Key) -> bool {
        self.keys_held.contains(&k)
    }
    /// Went up since the previous tick.
    pub fn released(&self, k: Key) -> bool {
        self.keys_released.contains(&k)
    }
    pub fn mouse_pressed(&self, b: MouseButton) -> bool {
        self.buttons_pressed.contains(&b)
    }
    pub fn mouse_held(&self, b: MouseButton) -> bool {
        self.buttons_held.contains(&b)
    }
    pub fn mouse_released(&self, b: MouseButton) -> bool {
        self.buttons_released.contains(&b)
    }
    /// Mouse position in logical coordinates, clamped to the logical frame.
    pub fn mouse(&self) -> (f32, f32) {
        self.mouse
    }

    pub(crate) fn key_down(&mut self, k: Key) {
        if self.keys_held.insert(k) {
            self.keys_pressed.insert(k);
        }
    }
    pub(crate) fn key_up(&mut self, k: Key) {
        if self.keys_held.remove(&k) {
            self.keys_released.insert(k);
        }
    }
    pub(crate) fn button_down(&mut self, b: MouseButton) {
        if self.buttons_held.insert(b) {
            self.buttons_pressed.insert(b);
        }
    }
    pub(crate) fn button_up(&mut self, b: MouseButton) {
        if self.buttons_held.remove(&b) {
            self.buttons_released.insert(b);
        }
    }
    pub(crate) fn set_mouse(&mut self, pos: (f32, f32)) {
        self.mouse = pos;
    }
    /// Releases everything, e.g. when the window loses focus and key-ups would be missed.
    pub(crate) fn release_all(&mut self) {
        self.keys_released.extend(self.keys_held.drain());
        self.buttons_released.extend(self.buttons_held.drain());
    }
    /// Edges are seen by exactly one tick, even when a frame runs several catch-up ticks.
    pub(crate) fn end_tick(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.buttons_pressed.clear();
        self.buttons_released.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_is_seen_by_one_tick_hold_persists() {
        let mut i = Input::default();
        i.key_down(Key::Space);
        i.key_down(Key::Space); // OS key repeat
        assert!(i.pressed(Key::Space) && i.held(Key::Space));
        i.end_tick();
        assert!(!i.pressed(Key::Space) && i.held(Key::Space));
        i.key_up(Key::Space);
        assert!(i.released(Key::Space) && !i.held(Key::Space));
        i.end_tick();
        assert!(!i.released(Key::Space));
    }

    #[test]
    fn press_and_release_within_one_frame_both_register() {
        let mut i = Input::default();
        i.button_down(MouseButton::Left);
        i.button_up(MouseButton::Left);
        assert!(i.mouse_pressed(MouseButton::Left));
        assert!(i.mouse_released(MouseButton::Left));
        assert!(!i.mouse_held(MouseButton::Left));
    }
}

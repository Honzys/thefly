//! Keyboard and mouse state as seen by one simulation tick.
//!
//! Window events are queued as edges (down or up). Each tick takes edges off the queue, so a
//! press is seen as `pressed` by exactly one tick even when a frame runs several, and edges
//! that arrive between ticks are never lost.

use std::collections::{HashSet, VecDeque};

pub use sdl3::keyboard::Scancode as Key;
pub use sdl3::mouse::MouseButton;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Control {
    Key(Key),
    Button(MouseButton),
}

#[derive(Debug, Default, Clone)]
pub struct Input {
    /// Held according to the events received so far, ahead of the tick state below.
    os_held: HashSet<Control>,
    /// Edges not yet seen by a tick: `(control, went_down)`.
    queue: VecDeque<(Control, bool)>,
    held: HashSet<Control>,
    pressed: HashSet<Control>,
    released: HashSet<Control>,
    mouse: (f32, f32),
}

impl Input {
    /// Went down since the previous tick.
    pub fn pressed(&self, k: Key) -> bool {
        self.pressed.contains(&Control::Key(k))
    }
    pub fn held(&self, k: Key) -> bool {
        self.held.contains(&Control::Key(k))
    }
    /// Went up since the previous tick.
    pub fn released(&self, k: Key) -> bool {
        self.released.contains(&Control::Key(k))
    }
    pub fn mouse_pressed(&self, b: MouseButton) -> bool {
        self.pressed.contains(&Control::Button(b))
    }
    pub fn mouse_held(&self, b: MouseButton) -> bool {
        self.held.contains(&Control::Button(b))
    }
    pub fn mouse_released(&self, b: MouseButton) -> bool {
        self.released.contains(&Control::Button(b))
    }
    /// Mouse position in logical coordinates, clamped to the logical frame.
    pub fn mouse(&self) -> (f32, f32) {
        self.mouse
    }

    pub(crate) fn key_down(&mut self, k: Key) {
        self.push(Control::Key(k), true);
    }
    pub(crate) fn key_up(&mut self, k: Key) {
        self.push(Control::Key(k), false);
    }
    pub(crate) fn button_down(&mut self, b: MouseButton) {
        self.push(Control::Button(b), true);
    }
    pub(crate) fn button_up(&mut self, b: MouseButton) {
        self.push(Control::Button(b), false);
    }
    pub(crate) fn set_mouse(&mut self, pos: (f32, f32)) {
        self.mouse = pos;
    }
    /// Releases everything, e.g. when the window loses focus and key-ups would be missed.
    pub(crate) fn release_all(&mut self) {
        let held: Vec<Control> = self.os_held.iter().copied().collect();
        for c in held {
            self.push(c, false);
        }
    }

    /// Queues an edge. OS key repeats and releases of controls never seen going down are dropped.
    fn push(&mut self, c: Control, down: bool) {
        if self.os_held.contains(&c) == down {
            return;
        }
        if down {
            self.os_held.insert(c);
        } else {
            self.os_held.remove(&c);
        }
        self.queue.push_back((c, down));
    }

    /// Starts a tick: takes queued edges in order. A second press (or release) of the same control
    /// stays queued for the next tick, so quick taps are never merged.
    pub(crate) fn begin_tick(&mut self) {
        self.pressed.clear();
        self.released.clear();
        while let Some(&(c, down)) = self.queue.front() {
            let seen = if down {
                &mut self.pressed
            } else {
                &mut self.released
            };
            if !seen.insert(c) {
                break;
            }
            if down {
                self.held.insert(c);
            } else {
                self.held.remove(&c);
            }
            self.queue.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `n` ticks, returning what each saw of `k` as `(pressed, held, released)`.
    fn ticks(i: &mut Input, k: Key, n: usize) -> Vec<(bool, bool, bool)> {
        (0..n)
            .map(|_| {
                i.begin_tick();
                (i.pressed(k), i.held(k), i.released(k))
            })
            .collect()
    }

    #[test]
    fn press_is_seen_by_exactly_one_of_several_ticks_hold_persists() {
        let mut i = Input::default();
        i.key_down(Key::Space);
        i.key_down(Key::Space); // OS key repeat
        assert_eq!(
            ticks(&mut i, Key::Space, 3),
            [
                (true, true, false),
                (false, true, false),
                (false, true, false)
            ]
        );
        i.key_up(Key::Space);
        assert_eq!(
            ticks(&mut i, Key::Space, 2),
            [(false, false, true), (false, false, false)]
        );
    }

    #[test]
    fn press_and_release_between_two_ticks_still_registers() {
        let mut i = Input::default();
        i.key_down(Key::Space);
        i.key_up(Key::Space);
        assert_eq!(
            ticks(&mut i, Key::Space, 2),
            [(true, false, true), (false, false, false)]
        );
    }

    #[test]
    fn repeated_taps_in_one_frame_each_get_their_own_tick() {
        let mut i = Input::default();
        for _ in 0..3 {
            i.key_down(Key::Space);
            i.key_up(Key::Space);
        }
        let seen = ticks(&mut i, Key::Space, 4);
        assert_eq!(seen.iter().filter(|t| t.0).count(), 3);
        assert_eq!(seen.iter().filter(|t| t.2).count(), 3);
        assert_eq!(seen[3], (false, false, false));
    }

    #[test]
    fn edges_wait_when_no_tick_runs() {
        let mut i = Input::default();
        i.key_down(Key::Space);
        assert!(!i.pressed(Key::Space)); // not yet part of any tick
        assert_eq!(ticks(&mut i, Key::Space, 1), [(true, true, false)]);
    }

    #[test]
    fn release_without_press_is_ignored() {
        // e.g. the key-up of F11, whose key-down the loop swallowed
        let mut i = Input::default();
        i.key_up(Key::F11);
        assert_eq!(ticks(&mut i, Key::F11, 1), [(false, false, false)]);
    }

    #[test]
    fn focus_loss_releases_held_keys_and_buttons() {
        let mut i = Input::default();
        i.key_down(Key::Left);
        i.button_down(MouseButton::Left);
        i.begin_tick();
        i.release_all();
        i.begin_tick();
        assert!(i.released(Key::Left) && !i.held(Key::Left));
        assert!(i.mouse_released(MouseButton::Left) && !i.mouse_held(MouseButton::Left));
    }

    #[test]
    fn mouse_buttons_follow_the_same_edges() {
        let mut i = Input::default();
        i.button_down(MouseButton::Left);
        i.button_up(MouseButton::Left);
        i.button_down(MouseButton::Left);
        i.begin_tick();
        assert!(i.mouse_pressed(MouseButton::Left) && i.mouse_released(MouseButton::Left));
        i.begin_tick();
        assert!(i.mouse_pressed(MouseButton::Left) && i.mouse_held(MouseButton::Left));
        i.begin_tick();
        assert!(!i.mouse_pressed(MouseButton::Left) && i.mouse_held(MouseButton::Left));
    }
}

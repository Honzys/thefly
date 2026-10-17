//! Per-tick keyboard state and the original key map (`game-spec.md` §3, from the code, not the
//! in-game help text).
//!
//! The sim does not depend on SDL. [`Scancode`] holds the USB HID usage id, which is the value
//! SDL3 uses for its scancodes, so the frontend converts with `u8::try_from(sc as i32)` and drops
//! keys outside 0..=255 (no key the game reads is out there).

/// A keyboard key by physical position (USB HID usage id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Scancode(pub u8);

impl Scancode {
    /// DIK 0x10, the farmer's goodie key: Q on QWERTY, A on AZERTY. Positional, like DirectInput.
    pub const Q: Self = Self(20);
    pub const F: Self = Self(9);
    pub const G: Self = Self(10);
    pub const H: Self = Self(11);
    pub const T: Self = Self(23);
    pub const RETURN: Self = Self(40);
    pub const ESCAPE: Self = Self(41);
    pub const TAB: Self = Self(43);
    pub const RIGHT: Self = Self(79);
    pub const LEFT: Self = Self(80);
    pub const DOWN: Self = Self(81);
    pub const UP: Self = Self(82);
    pub const RSHIFT: Self = Self(229);
}

/// The key map the original reads (`FUN_00407241`).
pub mod keys {
    use super::Scancode;

    pub const FLY_UP: Scancode = Scancode::UP;
    pub const FLY_DOWN: Scancode = Scancode::DOWN;
    pub const FLY_LEFT: Scancode = Scancode::LEFT;
    pub const FLY_RIGHT: Scancode = Scancode::RIGHT;
    /// Grab or drop (held), and goodie use.
    pub const FLY_ACTION: Scancode = Scancode::RETURN;
    pub const FLY_GOODIE: Scancode = Scancode::RSHIFT;

    // Open item: ai-and-rules "Still open" 1, the T/G/F/H to direction order is unconfirmed.
    pub const FARMER_UP: Scancode = Scancode::T;
    pub const FARMER_DOWN: Scancode = Scancode::G;
    pub const FARMER_LEFT: Scancode = Scancode::F;
    pub const FARMER_RIGHT: Scancode = Scancode::H;
    pub const FARMER_SWING: Scancode = Scancode::TAB;
    pub const FARMER_GOODIE: Scancode = Scancode::Q;

    pub const MENU_UP: Scancode = Scancode::UP;
    pub const MENU_DOWN: Scancode = Scancode::DOWN;
    pub const MENU_LEFT: Scancode = Scancode::LEFT;
    pub const MENU_RIGHT: Scancode = Scancode::RIGHT;
    pub const MENU_OK: Scancode = Scancode::RETURN;
    pub const MENU_BACK: Scancode = Scancode::ESCAPE;
}

const WORDS: usize = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct KeySet([u64; WORDS]);

impl KeySet {
    fn get(&self, k: Scancode) -> bool {
        self.0[usize::from(k.0 / 64)] & (1 << (k.0 % 64)) != 0
    }

    fn set(&mut self, k: Scancode) {
        self.0[usize::from(k.0 / 64)] |= 1 << (k.0 % 64);
    }

    fn of(keys: impl IntoIterator<Item = Scancode>) -> Self {
        let mut s = Self::default();
        keys.into_iter().for_each(|k| s.set(k));
        s
    }

    fn map2(a: Self, b: Self, f: impl Fn(u64, u64) -> u64) -> Self {
        Self(std::array::from_fn(|i| f(a.0[i], b.0[i])))
    }
}

/// Keys for one tick: held now, plus the edges against the previous tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyState {
    held: KeySet,
    pressed: KeySet,
    released: KeySet,
}

impl KeyState {
    /// Every key up, no edges. What the original reads while game+0x408ac is set.
    pub const NONE: KeyState = KeyState {
        held: KeySet([0; WORDS]),
        pressed: KeySet([0; WORDS]),
        released: KeySet([0; WORDS]),
    };

    /// The state for this tick from the keys held now and the previous tick's state.
    pub fn next(prev: &KeyState, held_now: impl IntoIterator<Item = Scancode>) -> Self {
        let held = KeySet::of(held_now);
        Self {
            held,
            pressed: KeySet::map2(held, prev.held, |now, before| now & !before),
            released: KeySet::map2(held, prev.held, |now, before| !now & before),
        }
    }

    /// Builds a state from explicit sets. The frontend uses it to pass the platform's edges
    /// through unchanged, so a press and release inside one tick is not lost.
    pub fn from_parts(
        held: impl IntoIterator<Item = Scancode>,
        pressed: impl IntoIterator<Item = Scancode>,
        released: impl IntoIterator<Item = Scancode>,
    ) -> Self {
        Self {
            held: KeySet::of(held),
            pressed: KeySet::of(pressed),
            released: KeySet::of(released),
        }
    }

    pub fn held(&self, k: Scancode) -> bool {
        self.held.get(k)
    }

    /// Went down since the previous tick (`FUN_0040735a` for Esc and Enter).
    pub fn pressed(&self, k: Scancode) -> bool {
        self.pressed.get(k)
    }

    pub fn released(&self, k: Scancode) -> bool {
        self.released.get(k)
    }

    /// A fresh Esc or Enter, the screen-skip test.
    pub fn skip(&self) -> bool {
        self.pressed(Scancode::ESCAPE) || self.pressed(Scancode::RETURN)
    }
}

/// The fly's controls for one tick, read from the key map.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlyPad {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    /// Enter held.
    pub action: bool,
    /// Fresh Right Shift press.
    pub goodie_cycle: bool,
}

impl FlyPad {
    pub fn read(keys: &KeyState) -> Self {
        Self {
            up: keys.held(keys::FLY_UP),
            down: keys.held(keys::FLY_DOWN),
            left: keys.held(keys::FLY_LEFT),
            right: keys.held(keys::FLY_RIGHT),
            action: keys.held(keys::FLY_ACTION),
            goodie_cycle: keys.pressed(keys::FLY_GOODIE),
        }
    }

    /// Mystery "inverted controls": left and right swap, up and down swap.
    pub fn inverted(self) -> Self {
        Self {
            up: self.down,
            down: self.up,
            left: self.right,
            right: self.left,
            ..self
        }
    }
}

/// The farmer's controls for one tick. A human fills it from the keys; the AI writes its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FarmerPad {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    /// Tab held. The swing edge is derived in tick step 2.
    pub swing: bool,
    /// Goodie key held. Its edge cycles the selector (ai-and-rules §5).
    pub goodie: bool,
    /// Fresh goodie-key press.
    pub goodie_pressed: bool,
}

impl FarmerPad {
    pub fn read(keys: &KeyState) -> Self {
        Self {
            up: keys.held(keys::FARMER_UP),
            down: keys.held(keys::FARMER_DOWN),
            left: keys.held(keys::FARMER_LEFT),
            right: keys.held(keys::FARMER_RIGHT),
            swing: keys.held(keys::FARMER_SWING),
            goodie: keys.held(keys::FARMER_GOODIE),
            goodie_pressed: keys.pressed(keys::FARMER_GOODIE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_follow_the_previous_tick() {
        let a = KeyState::next(&KeyState::NONE, [Scancode::RETURN, Scancode::RSHIFT]);
        assert!(a.pressed(Scancode::RETURN) && a.held(Scancode::RSHIFT) && a.skip());
        let b = KeyState::next(&a, [Scancode::RETURN]);
        assert!(b.held(Scancode::RETURN) && !b.pressed(Scancode::RETURN));
        assert!(b.released(Scancode::RSHIFT) && !b.skip());
    }

    #[test]
    fn pads_read_the_code_key_map() {
        let k = KeyState::next(&KeyState::NONE, [Scancode::T, Scancode::LEFT, Scancode::Q]);
        let farmer = FarmerPad::read(&k);
        assert!(farmer.up && farmer.goodie_pressed && !farmer.left);
        let fly = FlyPad::read(&k);
        assert!(fly.left && fly.inverted().right && !fly.inverted().left);
    }
}

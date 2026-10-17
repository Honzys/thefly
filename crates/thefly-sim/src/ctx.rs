//! What every step receives besides its own state: the time, the one RNG stream, the event sink
//! and the immutable asset numbers. Steps never read a clock or a seed from anywhere else.

use crate::data::GameData;
use crate::event::{Event, SoundOp, SoundSlot};
use crate::rng::MsvcRand;

pub struct Ctx<'a> {
    /// `now_ms` of this tick ([`crate::clock::Clock::now_ms`]); constant for the whole tick.
    pub now: u64,
    pub rng: &'a mut MsvcRand,
    pub data: &'a GameData,
    events: &'a mut Vec<Event>,
}

impl<'a> Ctx<'a> {
    pub fn new(
        now: u64,
        rng: &'a mut MsvcRand,
        data: &'a GameData,
        events: &'a mut Vec<Event>,
    ) -> Self {
        Self {
            now,
            rng,
            data,
            events,
        }
    }

    pub fn emit(&mut self, e: Event) {
        self.events.push(e);
    }

    pub fn sound(&mut self, slot: SoundSlot, op: SoundOp) {
        self.emit(Event::Sound { slot, op });
    }
}

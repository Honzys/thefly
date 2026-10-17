//! Pure, deterministic gameplay engine for The Fly 1.2.
//!
//! No SDL, no OS clock, no I/O. [`Sim::tick`] runs once per 65 Hz tick with that tick's
//! [`KeyState`] and returns the [`Event`]s the frontend turns into sound, music, effects and
//! file writes. The frontend renders from `&Sim` plus the interpolation alpha.
//!
//! Plan, ownership and acceptance criteria: `docs/plans/phase2b-engine.md`.

pub mod bonus;
pub mod bourse;
pub mod clock;
pub mod ctx;
pub mod data;
pub mod economy;
pub mod event;
pub mod highscores;
pub mod input;
pub mod match_state;
pub mod menu;
pub mod rng;
pub mod saves;
pub mod scenario;
pub mod screen;
pub mod shop;
pub mod shop_ai;
pub mod sim;
pub mod stats;
pub mod winner;
pub mod world;

pub use data::GameData;
pub use event::Event;
pub use input::{KeyState, Scancode};
pub use rng::MsvcRand;
pub use sim::Sim;

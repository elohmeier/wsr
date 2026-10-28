//! Wall Street Raider engine core: a port of the game logic in WSR.EXE (PowerBASIC 8).
//!
//! The crate has no UI or OS dependencies so it builds for native tests and the `wasm32-unknown-unknown` browser target.
//! All user interaction goes through the [`ui::Ui`] trait (see `wsr-wasm` for the browser binding).

pub mod logic;
pub mod content;
pub mod pb;
pub mod save;
pub mod snapshot;
pub mod state;
pub mod ui;
pub mod validation;

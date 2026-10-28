//! Differential tests of logic::assets (player asset actions and start-ups) against the original
//! (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod common;

#[path = "assets/script.rs"]
mod script;

#[path = "assets/buy.rs"]
mod buy;
#[path = "assets/offers.rs"]
mod offers;
#[path = "assets/sell.rs"]
mod sell;
#[path = "assets/startup.rs"]
mod startup;

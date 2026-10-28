//! Tests of logic::commod_ai: differential tests of the AI commodity/futures routines, physical
//! commodities, the commodity and leveraged ETFs and the exchange limits against the original
//! (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default, clippy::manual_is_multiple_of)]

mod diff;

#[path = "sim_entity/common.rs"]
mod sim_common;

#[path = "commod_ai/common.rs"]
mod common;

#[path = "commod_ai/limits.rs"]
mod limits;
#[path = "commod_ai/physical.rs"]
mod physical;
#[path = "commod_ai/futures_ai.rs"]
mod futures_ai;

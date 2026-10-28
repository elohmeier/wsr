//! Tests of logic::corp_ai: differential tests of the scheduled corporate AI routines against the
//! original (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

/// The recording Ui, the random world and the `run` helper of the sim_entity tests.
#[path = "sim_entity/common.rs"]
mod common;

#[path = "corp_ai/capital.rs"]
mod capital;
#[path = "corp_ai/distress.rs"]
mod distress;
#[path = "corp_ai/misc.rs"]
mod misc;
#[path = "corp_ai/spinoff.rs"]
mod spinoff;

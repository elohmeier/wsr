//! Tests of logic::outlook: differential tests of BuildIndustryOutlookText (0x56f33b) and its
//! commentary builders against the original (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod common;

#[path = "outlook/world.rs"]
mod world;

#[path = "outlook/commentary.rs"]
mod commentary;
#[path = "outlook/build.rs"]
mod build;

//! Tests of logic::events: differential tests of the random corporate events, merger/takeover events,
//! antitrust and lawsuit processing against the original (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod common;

#[path = "events/check.rs"]
mod check;

#[path = "events/antitrust.rs"]
mod antitrust;
#[path = "events/antitrust_suit.rs"]
mod antitrust_suit;
#[path = "events/lawsuits.rs"]
mod lawsuits;
#[path = "events/merger.rs"]
mod merger;
#[path = "events/random_news.rs"]
mod random_news;

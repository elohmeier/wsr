//! Tests of logic::turn: differential tests of the turn loop (SimScheduledEvents, TickerSimSteps,
//! EndTurn and helpers) and the scenario events against the original (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
#[allow(unused)]
mod world;

#[path = "turn/common.rs"]
mod common;

#[path = "turn/scenario_event.rs"]
mod scenario_event;
#[path = "turn/scheduled.rs"]
mod scheduled;
#[path = "turn/story.rs"]
mod story;
#[path = "turn/small.rs"]
mod small;
#[path = "turn/turn_loop.rs"]
mod turn_loop;

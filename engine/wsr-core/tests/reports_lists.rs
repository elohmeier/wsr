//! Tests of logic::reports_lists: unit tests and differential tests against the original (emulator
//! harness, tests/diff): report listbox lines, state and RND state.

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "reports_lists/common.rs"]
mod common;

#[path = "reports_lists/alerts.rs"]
mod alerts;
#[path = "reports_lists/futures_pos.rs"]
mod futures_pos;
#[path = "reports_lists/industry.rs"]
mod industry;
#[path = "reports_lists/pfs.rs"]
mod pfs;
#[path = "reports_lists/portfolio.rs"]
mod portfolio;
#[path = "reports_lists/projections.rs"]
mod projections;
#[path = "reports_lists/standings.rs"]
mod standings;

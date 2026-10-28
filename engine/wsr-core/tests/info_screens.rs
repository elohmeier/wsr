//! Tests of logic::info_screens: differential tests of the database search, ownership diagram, holder
//! lists, price charts, insider tips and the cheat menu against the original (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod common;

#[path = "trading/script.rs"]
mod script;

#[path = "info_screens/chart.rs"]
mod chart;
#[path = "info_screens/cheat.rs"]
mod cheat;
#[path = "info_screens/db_search.rs"]
mod db_search;
#[path = "info_screens/holders.rs"]
mod holders;
#[path = "info_screens/insider.rs"]
mod insider;
#[path = "info_screens/ownership.rs"]
mod ownership;

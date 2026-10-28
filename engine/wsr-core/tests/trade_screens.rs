//! Tests of logic::trade_screens: differential tests of the trading dialogs (options station, option
//! strategies, swaps, government bonds, commodity desk, crypto, alerts, bank asset allocation) against the
//! original (emulator harness with scripted dialog actions, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod common;

#[path = "trading/script.rs"]
mod script;

#[path = "trade_screens/option_station.rs"]
mod option_station;
#[path = "trade_screens/option_misc.rs"]
mod option_misc;
#[path = "trade_screens/ir_swaps.rs"]
mod ir_swaps;
#[path = "trade_screens/commod_crypto.rs"]
mod commod_crypto;
#[path = "trade_screens/alerts_alloc.rs"]
mod alerts_alloc;

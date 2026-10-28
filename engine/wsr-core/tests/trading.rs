//! Tests of logic::trading: differential tests of the stock trade primitives, the purchase financing,
//! the selection routines and the trading menus against the original (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod common;

#[path = "trading/script.rs"]
mod script;

#[path = "trading/stock.rs"]
mod stock;
#[path = "trading/borrow.rs"]
mod borrow;
#[path = "trading/select.rs"]
mod select;
#[path = "trading/short_menu.rs"]
mod short_menu;
#[path = "trading/buy_menu.rs"]
mod buy_menu;
#[path = "trading/futures_trade.rs"]
mod futures_trade;
#[path = "trading/options_menu.rs"]
mod options_menu;
#[path = "trading/options_sell.rs"]
mod options_sell;
#[path = "trading/bonds.rs"]
mod bonds;

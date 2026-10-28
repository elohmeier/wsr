//! Tests of logic::restructure: differential tests of mergers, liquidations of subsidiaries, LBO tender
//! offers, stock-for-stock mergers and capital contributions against the original (emulator harness,
//! tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod common;

#[path = "restructure/capital.rs"]
mod capital;
#[path = "restructure/liquidate_sub.rs"]
mod liquidate_sub;
#[path = "restructure/menus.rs"]
mod menus;
#[path = "restructure/merge.rs"]
mod merge;
#[path = "restructure/tender.rs"]
mod tender;

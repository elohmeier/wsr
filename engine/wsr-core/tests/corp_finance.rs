//! Tests of logic::corp_finance: differential tests of the corporate-finance actions against the
//! original (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "corp_finance/common.rs"]
mod common;

#[path = "corp_finance/bank.rs"]
mod bank;
#[path = "corp_finance/bonds.rs"]
mod bonds;
#[path = "corp_finance/lending.rs"]
mod lending;
#[path = "corp_finance/loans.rs"]
mod loans;
#[path = "corp_finance/misc.rs"]
mod misc;
#[path = "corp_finance/stock.rs"]
mod stock;

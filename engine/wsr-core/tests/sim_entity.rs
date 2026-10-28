//! Tests of logic::sim_entity: differential tests of SimProcessNextEntity (0x4c6bc1) over consecutive
//! ticks and of its callees against the original (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod common;

/// The scripted Ui of the trading tests (pick lists, symbol dialog).
#[path = "trading/script.rs"]
mod script;

#[path = "sim_entity/acquisitions.rs"]
mod acquisitions;
#[path = "sim_entity/etf.rs"]
mod etf;
#[path = "sim_entity/liquidation.rs"]
mod liquidation;
#[path = "sim_entity/mgmt.rs"]
mod mgmt;
#[path = "sim_entity/ai_corp.rs"]
mod ai_corp;
#[path = "sim_entity/asset_sales.rs"]
mod asset_sales;
#[path = "sim_entity/bonds.rs"]
mod bonds;
#[path = "sim_entity/corp_ops.rs"]
mod corp_ops;
#[path = "sim_entity/dividends.rs"]
mod dividends;
#[path = "sim_entity/fin_inst.rs"]
mod fin_inst;
#[path = "sim_entity/quarter_start.rs"]
mod quarter_start;
#[path = "sim_entity/transactions.rs"]
mod transactions;
#[path = "sim_entity/tick.rs"]
mod tick;

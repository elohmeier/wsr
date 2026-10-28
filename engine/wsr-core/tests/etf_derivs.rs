//! Tests of logic::etf_derivs: differential tests against the original (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod common;

#[path = "etf_derivs/corp_options.rs"]
mod corp_options;
#[path = "etf_derivs/etf_mgmt.rs"]
mod etf_mgmt;
#[path = "etf_derivs/etf_ops.rs"]
mod etf_ops;
#[path = "etf_derivs/option_expiry.rs"]
mod option_expiry;
#[path = "etf_derivs/swaps_ai.rs"]
mod swaps_ai;

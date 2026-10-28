//! Tests of logic::ai_player: differential tests of AiComputerPlayerTurn (0x5c3172) — the whole turn and
//! each strategy step (the other steps skipped on both sides) — and of FireManagement, ElectSelfCeo,
//! CheckCeoOptionForfeiture, AiFindAffordableBuyer and AddRecentOffering against the original
//! (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

/// The recording Ui, the random world and the `run` helper of the sim_entity tests.
#[path = "sim_entity/common.rs"]
mod sim_common;

#[path = "ai_player/common.rs"]
mod common;

#[path = "ai_player/actions.rs"]
mod actions;
#[path = "ai_player/buyer.rs"]
mod buyer;
#[path = "ai_player/invest.rs"]
mod invest;
#[path = "ai_player/merge.rs"]
mod merge;
#[path = "ai_player/mgmt.rs"]
mod mgmt;
#[path = "ai_player/portfolio.rs"]
mod portfolio;
#[path = "ai_player/position.rs"]
mod position;
#[path = "ai_player/turn.rs"]
mod turn;

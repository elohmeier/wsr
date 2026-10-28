//! Tests of `logic::advice`: differential tests of BuildMenuAdviceText (0x557bbf) per menu mode and of
//! its helpers against the original (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod sim_common;

#[path = "advice/common.rs"]
mod common;

#[path = "advice/helpers.rs"]
mod helpers;
#[path = "advice/player.rs"]
mod player;
#[path = "advice/corp.rs"]
mod corp;
#[path = "advice/menus.rs"]
mod menus;
#[path = "advice/economy.rs"]
mod economy;

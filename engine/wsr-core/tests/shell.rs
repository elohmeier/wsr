//! Tests of logic::shell: differential tests of the menu dispatcher, the action menu, the trading desk,
//! the settings menus, startup / ticker / save-file helpers and the shell dialogs against the original
//! (emulator harness, tests/diff).

#![allow(clippy::field_reassign_with_default)]

mod diff;

#[path = "sim_entity/common.rs"]
mod common;

#[path = "trading/script.rs"]
mod script;

#[path = "shell/support.rs"]
mod support;

#[path = "shell/menu.rs"]
mod menu;
#[path = "shell/action_menu.rs"]
mod action_menu;
#[path = "shell/desk.rs"]
mod desk;
#[path = "shell/settings.rs"]
mod settings;
#[path = "shell/app.rs"]
mod app;
#[path = "shell/dialogs.rs"]
mod dialogs;
#[path = "shell/leftovers.rs"]
mod leftovers;
#[path = "shell/credit.rs"]
mod credit;
#[path = "shell/quotes.rs"]
mod quotes;

#[path = "shell/startup.rs"]
mod startup;

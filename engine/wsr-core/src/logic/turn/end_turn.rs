//! `EndTurn` (0x424fd6) with its GOSUBs `EndTurn_CheckPlayersAlive` (0x42608c) and
//! `EndTurn_FixOutLabels` (0x4263d6), `EndTurn_RestoreActiveEntity` (0x4262a6), `EndTurn_Stub`
//! (0x4265f6), the END TURN button `CbEndTurn` (0x411de0), `EnableMainControls` (0x424754) and
//! `UpdateSettingsMenuText` (0x473796).


use super::prelude::*;
use super::Env;
use crate::logic::corp_ai::distress::ai_boost_leaders_rival;
use crate::logic::economy::close_all_dialogs;
use crate::logic::market::{
    rebuild_streaming_quotes_panel, refresh_selection_labels, select_target_corp, update_market_display,
    update_select_last_button,
};
use crate::logic::report::show_quick_message;
use crate::logic::reports_lists::show_player_standings;

/// The locals of EndTurn shared with its GOSUBs.
#[derive(Default)]
struct Locals {
    /// -0xa4
    msg_text: PbStr,
    /// -0xa0
    use_msg_box: i32,
    /// -0x9c
    prev_player_name: PbStr,
    /// -0x98
    saved_sel_entity: i32,
    /// -0x90
    out_count: i32,
    /// -0x8c
    player_count: i32,
    /// -0x88
    order_idx: i32,
    /// EndTurn_FixOutLabels' copy of the selection label (control 169).
    sel_label: PbStr,
}

/// Port of EndTurn (0x424fd6).
/// Ends the current player's turn and starts the next one:
///
/// * In the last year (`GameEndYear = CurYear`) the game ends (`GameEndFlag = "Y"`); otherwise, in the
///   middle of a quarter (`SimCoCount` 12..1589), one more tick runs (`TickerSimSteps(1)`).
/// * A lone human (`SoloHumanMode = 1`) just gets `TurnActionCount = 0` and continues.
/// * The running ticker is stopped (`ClockState = "STOP"`, `TickerOn = 0`; not at the very start of a
///   game with computer players).
/// * `AiBoostLeadersRival`, then the loop: `TurnActionCount = 0`, count the players and humans still in
///   (EndTurn_CheckPlayersAlive; no human left ends the game with a message), remember the human's
///   selections (`PlayerSelectLastCorp`, `PlayerLastStock`, `PlayerLastActiveEntity`); at the game's end
///   the final message (once, `GameOverMsgShown`), a hint and the standings; else the next player in
///   `PlayerTurnOrder` who is not out ("All players are broke" ends the game). A computer player plays
///   its whole turn (`AiComputerPlayerTurn`; "... has finished a turn" news) and the loop continues; for
///   a human the selections are restored (last stock via `SelectTargetCorp`, active entity, the
///   select-last button), the market display updated, the settings menu loaded from the player's
///   preferences and — when the player changed — "<name>'s turn begins now..." is shown (a message box
///   when `TurnActionCount` was 7 or in the first quarter, else a popup).
///
/// Skipped UI: dialogs, labels, the ticker button, window states, `SHELL PIX.EXE` (end-of-game picture),
/// focus.
pub fn end_turn(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    end_turn_skipping(g, ui, rng, now, &[]);
}

/// [`end_turn`] with routines (game addresses) skipped like the test harness does (`--skip`).
#[doc(hidden)]
pub fn end_turn_skipping(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, skip: &[u32]) {
    end_turn_env(g, ui, rng, &Env { now, skip });
}

pub(super) fn end_turn_env(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x426582 (ShowRuntimeError; not ported)
    let ts = time_secs(env.now);
    let mut l = Locals::default();
    // (UI) DIALOG END DlgWhoOwnsWhat
    if g.game_end_year == g.cur_year {
        set(&mut g.game_end_flag, "Y");
        set(&mut g.ticker_stop_req, "Y");
        g.solo_human_mode = 0;
    } else if 11 < g.sim_co_count && g.sim_co_count < 1590 && !env.skips(0x41ef3f) {
        super::ticker::ticker(g, ui, rng, 1, env);
    }
    if g.game_end_flag == "N" && g.solo_human_mode == 1 {
        g.turn_action_count = 0;
        end_turn_stub();
        return;
    }
    // (UI) "START TICKER" caption when the main window is in front
    if g.ticker_on == 1 {
        let keep_running = g.cur_year == g.start_year
            && g.cur_quarter == 1
            && g.has_cpu_players == "Y"
            && g.sim_co_count < 10;
        if !keep_running {
            set(&mut g.clock_state, "STOP");
            // (UI) DIALOG END RunLoopDlg
            g.ticker_on = 0;
        }
    }
    l.use_msg_box = if g.turn_action_count == 7 { 1 } else { 0 };
    if !env.skips(0x494506) {
        ai_boost_leaders_rival(g, ui, rng);
    }
    loop {
        g.turn_action_count = 0;
        check_players_alive(g, ui, &mut l);
        if g.players_alive == -1 {
            if !env.skips(0x446484) {
                show_player_standings(g, ui, rng);
            }
            end_turn_stub();
            return;
        }
        if g.players_alive == 1 && g.humans_alive == 1 {
            g.solo_human_mode = 1;
        }
        l.prev_player_name = g.ent_name[g.cur_player].clone();
        if g.is_computer_player[g.cur_player] == 0 {
            if 10 < g.last_sel_entity {
                g.player_select_last_corp[g.cur_player] = g.last_sel_entity;
            }
            if 0 < g.sel_entity {
                g.player_last_stock[g.cur_player] = g.sel_entity;
            }
            if 0 < g.active_entity {
                g.player_last_active_entity[g.cur_player] = g.active_entity;
            }
        }
        if g.game_end_year == g.cur_year {
            set(&mut g.game_end_flag, "Y");
            set(&mut g.ticker_stop_req, "Y");
        }
        if g.game_end_flag == "Y" {
            g.turn_action_count = 5;
            enable_main_controls(g);
            l.msg_text = PbStr::new();
            close_all_dialogs(g, ui, rng, 1);
            if g.players_alive != 0 && 0 < g.humans_alive && g.game_over_msg_shown == 0 {
                g.game_over_msg_shown = 1;
                let text = s(__content.string(12884)) + "\r" + "\r" + __content.string(12885) + &l.msg_text;
                ui.msgbox(&text, 8256, &s(__content.string(12886)));
            }
            // (UI) label 141 "January 1, <GameEndYear> -- Game Over"; DIALOG SHOW STATE hDlgMain, 0
            let mut hint = s(__content.string(12887))
                + __content.string(12888);
            show_quick_message(g, ui, &mut hint);
            if !env.skips(0x446484) {
                show_player_standings(g, ui, rng);
            }
            // (UI) DIALOG SHOW STATE hDlgMain, 5
            end_turn_stub();
            return;
        }
        l.out_count = 0;
        loop {
            l.player_count = g.num_players;
            l.order_idx = 1;
            let last = l.player_count;
            while l.order_idx <= last {
                if g.cur_player == g.player_turn_order[l.order_idx] && l.order_idx < l.player_count {
                    g.cur_player = g.player_turn_order[l.order_idx + 1];
                    break;
                }
                if g.cur_player == g.player_turn_order[l.order_idx] && l.player_count == l.order_idx {
                    g.cur_player = g.player_turn_order[1];
                }
                l.order_idx += 1;
            }
            l.order_idx = g.cur_player;
            g.cur_player_name = g.player_name[l.order_idx].clone();
            g.active_entity = 0;
            if g.is_computer_player[g.cur_player] == 0 {
                end_turn_restore_active_entity(g);
                if g.active_entity == 0 {
                    g.active_entity = g.cur_player;
                    g.active_entity_name = g.cur_player_name.clone();
                }
            }
            if g.player_ceo_of[l.order_idx] == 0 {
                g.player_ceo_since_year[l.order_idx] = 0;
            }
            if g.cur_player_name == "...OUT..." {
                l.out_count += 1;
            }
            if !(g.cur_player_name == "...OUT..." && l.out_count <= g.num_players) {
                break;
            }
        }
        if g.num_players <= l.out_count || g.players_alive == 0 {
            l.msg_text = s(__content.string(12889))
                + "\r"
                + "\r"
                + __content.string(12890);
            let title = g.app_title.clone();
            ui.msgbox(&l.msg_text, 8256, &title);
            // (UI) DIALOG SHOW STATE hDlgMain, 0
            l.msg_text = s("PZI|");
            // (UI) SHELL DataPath + "PIX.EXE " + msgText + DataPath
            set(&mut g.game_end_flag, "Y");
            close_all_dialogs(g, ui, rng, 1);
            // (UI) DIALOG END hDlgMain
        }
        if g.is_computer_player[g.cur_player] != 1 {
            human_turn(g, ui, rng, env, &mut l, ts);
            return;
        }
        // a computer player's turn
        g.ticker_pause_until = 0;
        g.clock_state = PbStr::new();
        g.ticker_on = 0;
        set(&mut g.ticker_stop_req, "N");
        // 0x4258ff loads the INTEGER zero at 0x75e098.
        super::ticker::pause_ticker(g, env.now, 0.0f32);
        l.saved_sel_entity = g.sel_entity;
        g.sel_entity = g.cur_player;
        close_all_dialogs(g, ui, rng, 1);
        if !env.skips(0x5c3172) {
            crate::logic::ai_player::ai_computer_player_turn_skipping(g, ui, rng, env.now, env.skip);
        }
        set(&mut g.action_code, "SUPP");
        set(&mut g.ticker_stop_req, "Y");
        set(&mut g.clock_state, "STOP");
        g.sel_entity = l.saved_sel_entity;
        g.ticker_on = 0;
        g.action_code = PbStr::new();
        enable_main_controls(g);
        g.news_msg = s("... ") + &g.cur_player_name + __content.string(12891);
        post_news(g, ui, 0, 0);
        // (UI) CONTROL SET FOCUS hDlgMain, 168
    }
}

/// The human player's part of EndTurn (0x425a1f..0x426072).
fn human_turn(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, l: &mut Locals, ts: i32) {
    let __content = crate::content::current();
    close_all_dialogs(g, ui, rng, 1);
    if g.sel_entity < 12 {
        let last_stock = g.player_last_stock[g.cur_player];
        if 10 < last_stock && g.entity_status[last_stock] == 1 {
            g.sel_entity = g.player_last_stock[g.cur_player];
            g.sel_corp_name = g.ent_name[g.sel_entity].clone();
        } else {
            g.sel_entity = g.cur_player;
            g.sel_corp_name = g.cur_player_name.clone();
        }
    } else {
        enable_main_controls(g);
    }
    refresh_selection_labels(g);
    if !env.skips(0x4400d7) {
        update_market_display(g, ui, rng, ts);
    }
    if !(g.game_end_flag == "Y") {
        // (UI) label 140 "Player:  " + CurPlayerName
        if g.has_cpu_players == "Y" {
            l.msg_text = PbStr::new();
            if g.is_computer_player[g.cur_player] == 0 && g.humans_alive == 1 {
                end_turn_restore_active_entity(g);
                rebuild_streaming_quotes_panel(g, ui, 0);
                update_settings_menu_text(g);
                fix_out_labels(g, l);
                end_turn_stub();
                return;
            }
            if 1 < g.players_alive && 0 < g.humans_alive && g.is_computer_player[g.cur_player] == 0 {
                end_turn_restore_active_entity(g);
                rebuild_streaming_quotes_panel(g, ui, 0);
                update_settings_menu_text(g);
                if g.humans_alive == 1 {
                    end_turn_stub();
                    return;
                }
            }
        }
        if !(l.prev_player_name == g.cur_player_name) {
            if g.cur_quarter == 1 && g.start_year == g.cur_year {
                l.use_msg_box = 1;
            }
            if l.use_msg_box == 1 {
                let text = l.msg_text.clone() + &g.cur_player_name + __content.string(12892);
                let title = g.app_title.clone();
                ui.msgbox(&text, 8192, &title);
            } else {
                let mut text = g.cur_player_name.clone() + __content.string(12892);
                show_quick_message(g, ui, &mut text);
            }
            let last_stock = g.player_last_stock[g.cur_player];
            if last_stock < 11 || g.entity_status[last_stock] != 1 {
                g.cur_player_name = g.ent_name[g.cur_player].clone();
                g.sel_corp_name = g.cur_player_name.clone();
                g.active_entity_name = g.cur_player_name.clone();
                g.sel_entity = g.cur_player;
                g.active_entity = g.cur_player;
                refresh_selection_labels(g);
            } else {
                g.target_corp = g.player_last_stock[g.cur_player];
                g.tmp_symbol = g.ent_symbol[g.target_corp].clone();
                g.sel_entity = 0;
                select_target_corp(g, ui, rng, ts);
                g.lookup_entity_name = PbStr::new();
                if 10 < g.sel_entity && g.cur_player == g.corp_controller[g.sel_entity] {
                    g.active_entity = g.sel_entity;
                    g.active_entity_name = g.ent_name[g.sel_entity].clone();
                    g.sel_corp_name = PbStr::new();
                }
                refresh_selection_labels(g);
            }
            let last_corp = g.player_select_last_corp[g.cur_player];
            if 10 < last_corp && g.last_sel_entity != last_corp {
                update_select_last_button(g, last_corp);
                let last_corp = g.player_select_last_corp[g.cur_player];
                if 10 < last_corp && g.entity_status[last_corp] != 1 {
                    g.player_select_last_corp[g.cur_player] = 0;
                }
            }
        }
    }
    end_turn_restore_active_entity(g);
    rebuild_streaming_quotes_panel(g, ui, 0);
    update_settings_menu_text(g);
    fix_out_labels(g, l);
    end_turn_stub();
}

/// Port of EndTurn_CheckPlayersAlive (0x42608c), GOSUB of EndTurn.
/// `PlayersAlive` / `HumansAlive` = the players (humans) not "...OUT...". If only computer players
/// remain, the game ends: `PlayersAlive = -1`, `GameEndFlag = "Y"` and a mocking message box (and the
/// end-of-game picture, `SHELL PIX.EXE`, UI).
fn check_players_alive(g: &mut Globals, ui: &mut dyn Ui, l: &mut Locals) {
    let __content = crate::content::current();
    g.players_alive = 0;
    g.humans_alive = 0;
    l.order_idx = 1;
    let last = g.num_players;
    while l.order_idx <= last {
        if !(g.player_name[l.order_idx] == "...OUT...") {
            g.players_alive += 1;
            if g.is_computer_player[l.order_idx] == 0 {
                g.humans_alive += 1;
            }
        }
        l.order_idx += 1;
    }
    if g.humans_alive == 0 && 0 < g.players_alive {
        g.players_alive = -1;
        set(&mut g.game_end_flag, "Y");
        enable_main_controls(g);
        l.msg_text = s(__content.string(12893))
            + __content.string(12894)
            + "\r"
            + "\r"
            + __content.string(12895)
            + __content.string(12896)
            + "\r"
            + "\r";
        let title = g.app_title.clone() + __content.string(12897);
        ui.msgbox(&l.msg_text, 8256, &title);
        l.msg_text = s("PZI|");
        // (UI) SHELL DataPath + "PIX.EXE " + msgText + DataPath
    }
}

/// Port of EndTurn_FixOutLabels (0x4263d6), GOSUB of EndTurn.
/// If the selection label (control 169) shows "...OUT...", `SelCorpName` is reset to `EntName(SelEntity)`
/// (the labels are UI). The port takes the label's text to be `SelCorpName` (what the game puts there);
/// it is not read when the game is over.
fn fix_out_labels(g: &mut Globals, l: &mut Locals) {
    if !(g.game_end_flag == "Y") {
        l.sel_label = g.sel_corp_name.clone();
    }
    if l.sel_label == "...OUT..." {
        g.sel_corp_name = g.ent_name[g.sel_entity].clone();
        // (UI) labels 169 (SelCorpName), 202 (symbol), 173 ("Se&lect " + last symbol)
    }
}

/// Port of EndTurn_RestoreActiveEntity (0x4262a6).
/// For a human current player: `ActiveEntity` = the company the player last acted for
/// (`PlayerLastActiveEntity`) if it is still active and controlled by the player, else the player;
/// `ActiveEntityName` accordingly.
pub fn end_turn_restore_active_entity(g: &mut Globals) {
    if g.is_computer_player[g.cur_player] == 1 {
        return;
    }
    let last = g.player_last_active_entity[g.cur_player];
    if 10 < last && g.entity_status[last] == 1 && g.cur_player == g.corp_controller[last] {
        g.active_entity = g.player_last_active_entity[g.cur_player];
    } else {
        g.active_entity = g.cur_player;
    }
    g.active_entity_name = g.ent_name[g.active_entity].clone();
}

/// Port of EndTurn_Stub (0x4265f6): the shared (empty) exit of EndTurn.
pub fn end_turn_stub() {}

/// Port of CbEndTurn (0x411de0), the END TURN button: closes the dialogs, ends the turn
/// ([`end_turn`], with `TickerStopReq = "Y"` meanwhile) and — unless the ticker runs or the quarter is
/// nearly over (`SimCoCount >= 1570`) — runs 20 suppressed ticks (`ActionCode = "SUPP"`).
pub fn cb_end_turn(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    let env = Env { now, skip: &[] };
    close_all_dialogs(g, ui, rng, 1);
    set(&mut g.ticker_stop_req, "Y");
    end_turn_env(g, ui, rng, &env);
    set(&mut g.ticker_stop_req, "N");
    if !(g.clock_state == "RUN") && g.sim_co_count < 1570 {
        g.ticker_pause_until = 0;
        g.clock_state = PbStr::new();
        g.ticker_on = 0;
        set(&mut g.action_code, "SUPP");
        super::ticker::ticker(g, ui, rng, 20, &env);
        g.action_code = PbStr::new();
    }
}

/// Port of EnableMainControls (0x424754): enables/disables the main window's buttons (UI only; no
/// game state).
pub fn enable_main_controls(g: &mut Globals) {
    let _ = g;
}

/// Port of UpdateSettingsMenuText (0x473796), state effects only.
/// For a human current player 1..NumPlayers loads the player's preferences into the current settings:
/// `SuppressPopups`, `CurSuppressEarnings`, `CurSuppressCashWarn`, `CurAutoSave`, `CurExerciseOptions`,
/// `CurMakeDelivery`, `CurTakeDelivery`, `AutoAddStreamList`, `CurSweepCash` (the menu captions are UI).
pub fn update_settings_menu_text(g: &mut Globals) {
    let p = g.cur_player;
    if g.is_computer_player[p] == 1 || p < 1 || g.num_players < p {
        return;
    }
    g.suppress_popups = g.player_suppress_popups[p];
    g.cur_suppress_earnings = g.player_suppress_earnings[p];
    g.cur_suppress_cash_warn = g.player_suppress_cash_warn[p];
    g.cur_auto_save = g.player_auto_save[p];
    g.cur_exercise_options = g.player_exercise_options[p];
    g.cur_make_delivery = g.player_make_delivery[p];
    g.cur_take_delivery = g.player_take_delivery[p];
    g.auto_add_stream_list = g.player_auto_add_stream[p];
    // (UI) AutoPilot(CurPlayer), PlayerGrowthThrottle(CurPlayer): menu captions only
    g.cur_sweep_cash = g.sweep_cash_to_loan[p];
}

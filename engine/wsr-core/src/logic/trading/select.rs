//! Selecting companies: report links (`FollowReportLink`), the acting entity (`SelectActingEntity`),
//! symbol lookup (`LookupSymbol`) and entry (`InBox` mode 3), the pick lists `ShowCompanyPickList` and
//! `PickFromStreamingList`, and `GetSelectedPlayerEtf`.

// Keep the original's statement shapes.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::if_same_then_else,
    clippy::manual_range_contains
)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::control::update_corp_control;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::get_largest_holding_id;
use crate::logic::market::{
    refresh_selection_labels, select_target_corp, show_next_earnings_date, toggle_streaming_quote,
    update_market_display, update_select_last_button,
};
use crate::logic::newgame::LocalTime;
use crate::logic::newgame_helpers::calc_loan_sale_price_pct;
use crate::logic::report::describe_defunct_entity;
use crate::logic::reports_lists::{pb_array_sort, update_control_chain};
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::turn::end_turn;
use crate::logic::valuation::calc_analyst_rating;
use crate::pb::string::compare;

/// Port of FollowReportLink (0x46c9f4).
/// Follows the link code of a clicked report line (`"@C0123"` / `"@I0042"`, or the whole line: only its
/// last 4 characters are read). Returns 1 when the link was handled (the caller closes the report).
///
/// * `@I<industry>` (capped at 71): with `EntityPickMode` `"GetInd"`/`"ChangeInd"` it only sets
///   `SelIndustry`; otherwise a different industry is selected (`SelIndustry`, the industry combo).
///   Handled whenever the industry is > 0.
/// * `@C<id>` (ids over 1600 are ignored): `TargetCorp = id`. `"GetCorp"` mode: handled. A player id
///   other than the current player is refused before the game's end. `"ChangeCorp"` mode: the current
///   player becomes the acting entity (`SelectActingEntity(1)`), at the game's end another player is
///   switched to (`SelectActingEntity(-id)`), a company is looked up by its symbol (`LookupSymbol`) and
///   selected (`SelectTargetCorp`) unless already selected.
pub fn follow_report_link(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, link_text: &PbStr) -> i32 {
    let __content = crate::content::current();
    let mut handled = 0;
    let mut industry: i16 = 0;
    if 0 < instr(1, link_text.as_bytes(), b"@I") {
        let tail = right(link_text.as_bytes(), 4);
        industry = to_integer(val(tail.as_bytes()));
        if 71 < industry {
            industry = 71;
        }
        if g.entity_pick_mode == "GetInd" || g.entity_pick_mode == __content.string(10154) {
            g.sel_industry = cast::<i32, _>(industry);
            return 1;
        }
        if 0 < industry && cast::<i32, _>(industry) != g.sel_industry {
            g.sel_industry = cast::<i32, _>(industry);
            select_industry_combo_item(g);
        }
    }
    if 0 < industry {
        return 1;
    }
    if instr(1, link_text.as_bytes(), b"@C") < 1 {
        return handled;
    }
    let tail = right(link_text.as_bytes(), 4);
    let mut entity_id = to_long(val(tail.as_bytes()));
    if 1600 < entity_id {
        entity_id = 0;
    }
    if entity_id < 1 {
        return handled;
    }
    g.target_corp = entity_id;
    if g.entity_pick_mode == "GetCorp" {
        return 1;
    }
    if entity_id < 11 && g.cur_player != entity_id && !(g.game_end_flag == "Y") {
        return handled;
    }
    if !(g.entity_pick_mode == __content.string(6093)) {
        return handled;
    }
    if g.cur_player == entity_id {
        let now = ui.local_time();
        select_acting_entity(g, ui, rng, 1, &now);
        return 1;
    }
    if entity_id < 11 && g.game_end_flag == "Y" {
        let now = ui.local_time();
        select_acting_entity(g, ui, rng, -entity_id, &now);
        return 1;
    }
    let symbol = g.ent_symbol[entity_id].clone();
    g.tmp_symbol = PbStr::new();
    lookup_symbol(g, ui, rng, &symbol);
    // (UI) CONTROL SET TEXT hDlgMain, 202, TmpSymbol
    if g.tmp_symbol.is_empty() || g.sel_entity == g.target_corp {
        handled = 0;
    } else {
        let seconds = ui.local_time().time_secs();
        select_target_corp(g, ui, rng, seconds);
        handled = 1;
    }
    handled
}

/// Port of SelectIndustryComboItem (0x46df33), state effects only: selects the entry of `SelIndustry` in
/// the main window's industry combo box and refreshes the main controls (UI; no state changes).
pub fn select_industry_combo_item(_g: &mut Globals) {
    // (UI) COMBOBOX SELECT hDlgMain, 234, <index of SelIndustry in IndComboItems()>; EnableMainControls
}

/// Port of SelectActingEntity (0x4721dc).
/// Chooses the entity the player acts as (`ActiveEntity`) / looks at (`SelEntity`):
///
/// * `mode = 1`: the current player. `mode < 0` after the game's end: switches to player `-mode`
///   (`CurPlayer`, `CurPlayerName`, market display) unless that player is out (obituary message), then
///   as mode 1. (`mode < 0` before the end is the same as mode 1.)
/// * `mode = 2`: counts the companies the player controls (after `UpdateCorpControl`); none or only the
///   active one → message; else the company pick list ([`show_company_pick_list`], `"ChangeCorp"`).
/// * `mode = 3`: the symbol entry dialog ([`in_box_symbol`], `ActionCode = "CHANGESYM"`).
///
/// Modes 2/3 then select the chosen company (`SelectTargetCorp`) and add it to the streaming quotes
/// (`AutoAddStreamList`). The ticker pause (`PauseTicker`) is UI timing and not ported. `now` is the
/// local time (only used when the pick list ends a computer player's turn, see
/// [`show_company_pick_list`]).
pub fn select_acting_entity(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, mode: i32, now: &LocalTime) {
    let __content = crate::content::current();
    if 0 < g.symbol_dlg_open {
        // (UI) DIALOG END DlgSymbolEntry
        g.symbol_dlg_open = 0;
    }
    let prev_sel = g.sel_entity;
    if mode == 1 || mode < 0 {
        if mode < 1 && g.game_end_flag == "Y" {
            let entity_id = to_long((cast::<Ext, _>(mode)).abs());
            if g.player_name[entity_id] == "...OUT..." {
                describe_defunct_entity(g, ui, entity_id);
                let obit_text = s(__content.string(12704))
                    + &ucase(g.ent_name[entity_id].as_bytes(), CaseMode::Ansi)
                    + ":"
                    + "\r"
                    + "\r"
                    + __content.string(12705)
                    + "\r"
                    + "\r"
                    + __content.string(12706);
                ui.msgbox(&obit_text, 8192, &s(__content.string(12707)));
                return;
            }
            g.cur_player = entity_id;
            g.cur_player_name = g.ent_name[g.cur_player].clone();
            update_market_display(g, ui, rng, now.time_secs());
            // (UI) label 140: "Player:  " + CurPlayerName
        }
        g.sel_corp_name = g.cur_player_name.clone();
        g.active_entity_name = g.cur_player_name.clone();
        g.sel_entity = g.cur_player;
        g.active_entity = g.cur_player;
        g.lookup_entity_name = PbStr::new();
        refresh_selection_labels(g);
        if g.ticker_on == 1 {
            g.clock_state = s("RUN");
        }
        if prev_sel != g.sel_entity {
            update_select_last_button(g, prev_sel);
        }
        return;
    }
    g.tmp_symbol = PbStr::new();
    g.input_dlg_label = PbStr::new();
    g.input_dlg_prompt = PbStr::new();
    g.target_corp = 0;
    if mode == 2 {
        let mut owned_count = 0;
        for entity_id in 11..=1600 {
            if g.entity_status[entity_id] == 1 && g.cur_player == g.corp_controller[entity_id] {
                update_corp_control(g, ui, rng, entity_id);
                if g.cur_player == g.corp_controller[entity_id] {
                    owned_count = owned_count + 1;
                }
            }
        }
        if owned_count == 0 {
            let msg_text = s(__content.string(12708));
            let title = g.app_title.clone();
            ui.msgbox(&msg_text, 4112, &title);
            return;
        }
        if owned_count == 1 && g.sel_entity == g.active_entity && 10 < g.active_entity {
            let msg_text = s(__content.string(12709))
                + "\r"
                + "\r"
                + __content.string(12710)
                + &g.ent_name[g.active_entity]
                + __content.string(12711)
                + __content.string(12712);
            let title = g.app_title.clone();
            ui.msgbox(&msg_text, 8208, &title);
            return;
        }
        g.input_dlg_prompt = s(__content.string(12713))
            + __content.string(12714)
            + __content.string(3392)
            + __content.string(3393)
            + "\r\n"
            + "\r\n";
        g.entity_pick_mode = s(__content.string(6093));
        show_company_pick_list(g, ui, rng, now);
    }
    if mode == 3 {
        g.lookup_filter = PbStr::new();
        g.action_code = s("CHANGESYM");
        g.input_dlg_title = s(__content.string(12715));
        in_box_symbol(g, ui, rng);
    }
    show_next_earnings_date(g);
    if mode == 3 {
        // (UI) CONTROL SET TEXT hDlgMain, 202, TmpSymbol
    }
    if g.tmp_symbol.is_empty() || g.sel_entity == g.target_corp {
        if prev_sel != g.sel_entity {
            update_select_last_button(g, prev_sel);
        }
        // (UI) PauseTicker(0)
    } else {
        select_target_corp(g, ui, rng, now.time_secs());
        // (UI) PauseTicker(0)
        if 10 < g.sel_entity && !(g.game_end_flag == "Y") && g.auto_add_stream_list == 1 {
            toggle_streaming_quote(g, ui, rng, 1, 0);
        }
    }
}

/// Port of LookupSymbol (0x422b3b) with its GOSUBs LookupSymbol_Clear (0x42308f) and LookupSymbol_Exit
/// (0x42314f, the common exit).
/// Resolves an entered stock symbol (or entity number) into `TargetCorp`, `TmpSymbol` (the trimmed
/// entry, or the symbol of an entered number) and `LookupEntityName`. Another player's number is refused
/// with a popup (after the game's end it switches to that player: `SelectActingEntity(-id)`); a
/// liquidated company is described (`DescribeDefunctEntity`); an unknown symbol gives a popup (unless it
/// is "CANCEL"). On failure TargetCorp, TmpSymbol and LookupEntityName are cleared.
pub fn lookup_symbol(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, symbol_ref: &PbStr) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    g.tmp_symbol = trim(symbol_ref.as_bytes());
    if g.tmp_symbol.is_empty()
        || 0 < instr(1, g.tmp_symbol.as_bytes(), b"N/A")
        || mid(g.tmp_symbol.as_bytes(), 1, 3) == "..."
    {
        lookup_symbol_clear(g);
        return;
    }
    let entity_id = to_long(val(g.tmp_symbol.as_bytes()));
    if 0 < entity_id && entity_id <= g.num_players && g.cur_player != entity_id {
        if g.game_end_flag == "Y" {
            let now = ui.local_time();
        select_acting_entity(g, ui, rng, -entity_id, &now);
            return;
        }
        if g.player_name[entity_id] == "...OUT..." {
            describe_defunct_entity(g, ui, entity_id);
        } else {
            let mut m = s(__content.string(12716))
                + &str_s(cast::<Ext, _>(entity_id))
                + " ("
                + &g.ent_name[entity_id]
                + ")";
            show_quick_message(g, ui, &mut m);
        }
        lookup_symbol_clear(g);
        return;
    }
    if 10 < entity_id && entity_id < 1601 {
        g.tmp_symbol = g.ent_symbol[entity_id].clone();
        if g.entity_status[entity_id] == 1 {
            g.target_corp = entity_id;
            g.lookup_entity_name = g.ent_name[entity_id].clone();
            return;
        }
        if g.entity_status[entity_id] == 2 {
            describe_defunct_entity(g, ui, entity_id);
        } else {
            let mut m =
                g.ent_name[entity_id].clone() + __content.string(12717);
            show_quick_message(g, ui, &mut m);
        }
        lookup_symbol_clear(g);
        return;
    }
    for entity_id in 11..=1600 {
        if g.tmp_symbol == g.ent_symbol[entity_id] {
            let ent_status = g.entity_status[entity_id];
            if ent_status == 1 {
                g.target_corp = entity_id;
                g.lookup_entity_name = g.ent_name[entity_id].clone();
            } else if ent_status == 2 {
                describe_defunct_entity(g, ui, entity_id);
                lookup_symbol_clear(g);
            } else {
                let mut m = g.ent_name[entity_id].clone()
                    + __content.string(12718);
                show_quick_message(g, ui, &mut m);
                lookup_symbol_clear(g);
            }
            return;
        }
    }
    if !(ucase(g.tmp_symbol.as_bytes(), CaseMode::Ansi) == "CANCEL") {
        let mut m = s(__content.string(12719)) + &g.tmp_symbol;
        show_quick_message(g, ui, &mut m);
    }
    lookup_symbol_clear(g);
}

/// Port of LookupSymbol_Clear (0x42308f): clears `TmpSymbol`, `TargetCorp`, `LookupEntityName`.
fn lookup_symbol_clear(g: &mut Globals) {
    g.tmp_symbol = PbStr::new();
    g.target_corp = 0;
    g.lookup_entity_name = PbStr::new();
}

/// Port of InBox (0x423170) in mode 3, the stock-symbol entry used by the trading menus and
/// `SelectActingEntity`, with the state effects of CreateSymbolEntryDialog (0x419674), InBox_SetupPrompt
/// (0x42351f) and the OK / Cancel buttons SymbolEntry_OkBtn (0x41a85f) / SymbolEntry_CancelBtn
/// (0x41a961).
///
/// The dialog is asked through [`Ui::inputbox`]: prompt = `InputDlgPrompt` (or, when empty, the "currently
/// [runtime resource wording] " text) + "Click 'OK' to enter.", title = `InputDlgTitle` (or `AppTitle`),
/// default = the prefilled symbol (CreateSymbolEntryDialog's choice by `ActionMenuMode` / `ActionCode`:
/// the selected or last selected company, a bank for "CHANGEBANK", …). An empty answer is Cancel
/// (`SymbolDlgText = "Cancel"`, TargetCorp / TmpSymbol / LookupEntityName cleared); otherwise the entry
/// goes through [`lookup_symbol`]. `ActionCode` "CHANGEBANK" / "ANTITRUST" / "CALLLOAN" / "ETF_TRADES"
/// is cleared, `InputDlgTitle` too. With `ActionCode` "CHANGESYM" (or "GETDATA") a found company is
/// selected (`SelectTargetCorp`).
///
/// The dialog's LOOK UP button (SymbolEntry_LookUpBtn 0x41a79f, the alphabetical company list
/// ShowCompanyListDialog 0x41ce26) is not offered: the port's host shows its own lists.
///
/// An empty answer of [`Ui::inputbox`] is the Cancel button: the `Ui` can't tell OK with an empty entry
/// box from Cancel (the original then leaves `SymbolDlgText = ""` instead of `"Cancel"`).
pub fn in_box_symbol(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mode_ref = 3;
    g.in_box_mode = mode_ref;
    let mut dlg_mode = mode_ref;
    if !(g.action_code == "CHANGESYM") && !(g.action_code == "GETDATA") {
        dlg_mode = mode_ref + 2;
    }
    g.symbol_dlg_text = PbStr::new();
    if g.symbol_dlg_open == 1 {
        // (UI) DIALOG END DlgSymbolEntry
        g.symbol_dlg_open = 0;
    }
    // CreateSymbolEntryDialog(0)
    let mut dlg_title = g.app_title.clone();
    if !g.input_dlg_title.is_empty() {
        dlg_title = g.input_dlg_title.clone();
    }
    let default_text = symbol_entry_prefill(g, ui, rng);
    // InBox_SetupPrompt (mode 3)
    let mut prompt_text = g.input_dlg_prompt.clone();
    if g.input_dlg_prompt.is_empty() {
        prompt_text = s(__content.string(12720));
        prompt_text = prompt_text + "is:  " + &g.sel_corp_name + "\r\n" + "\r\n";
        prompt_text = prompt_text + __content.string(12721);
        prompt_text = prompt_text + __content.string(12722);
        prompt_text = prompt_text + __content.string(12723);
        prompt_text = prompt_text + __content.string(12724);
        prompt_text = prompt_text + __content.string(12725);
    }
    let prompt = prompt_text + __content.string(6717);
    g.input_dlg_title = PbStr::new();
    g.symbol_dlg_open = 1;
    let answer = ui.inputbox(&prompt, &dlg_title, &default_text);
    if answer.is_empty() {
        // SymbolEntry_CancelBtn
        g.symbol_dlg_text = s("Cancel");
        g.target_corp = 0;
        g.lookup_entity_name = PbStr::new();
        g.tmp_symbol = PbStr::new();
    } else {
        // SymbolEntry_OkBtn
        g.symbol_dlg_text = answer;
        if g.in_box_mode == 3 {
            let symbol_text = g.symbol_dlg_text.clone();
            lookup_symbol(g, ui, rng, &symbol_text);
            if g.target_corp == 0 {
                g.lookup_entity_name = PbStr::new();
                g.tmp_symbol = PbStr::new();
            }
        }
    }
    g.symbol_dlg_open = 0;
    if g.ticker_on == 1 {
        g.clock_state = s("RUN");
    }
    g.symbol_dlg_open = 0;
    if dlg_mode == 3 {
        // (UI) CONTROL SET TEXT hDlgMain, 202, SymbolDlgText
        if 0 < g.target_corp {
            let seconds = ui.local_time().time_secs();
        select_target_corp(g, ui, rng, seconds);
        }
    }
    g.in_box_mode = 0;
}

/// CreateSymbolEntryDialog (0x419674): the symbol prefilled in the entry box (state effects included:
/// the "CHANGEBANK" search updates control and financials; some action codes are cleared).
fn symbol_entry_prefill(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> PbStr {
    let mut text = PbStr::new();
    let mut default_entity = g.sel_entity;
    if g.sel_entity == g.active_entity {
        default_entity = g.last_sel_entity;
    }
    let mut prefill_symbol = 0;
    if g.action_menu_mode == 3
        && g.action_code == "ADVANCE"
        && g.cur_player == g.active_entity
        && default_entity != g.active_entity
    {
        if 10 < default_entity && g.cur_player == g.corp_controller[default_entity] {
            text = g.ent_symbol[default_entity].clone();
        }
        prefill_symbol = 1;
    }
    let _ = prefill_symbol;
    if 3 < g.action_menu_mode && 10 < default_entity {
        prefill_symbol = 1;
        if g.action_code == "BUYSELLJUNK" && g.bonds_issued[default_entity] < 1 {
            prefill_symbol = 0;
        }
        if g.action_code == "CHANGEBANK" {
            let ae = g.active_entity;
            if 10 < g.last_sel_entity && g.last_sel_entity < 86 && cast::<i32, _>(g.lending_bank[ae]) != g.last_sel_entity {
                default_entity = g.last_sel_entity;
            }
            if 10 < g.sel_entity && g.sel_entity < 86 && cast::<i32, _>(g.lending_bank[ae]) != g.sel_entity {
                default_entity = g.sel_entity;
            }
            if default_entity < 11 || 85 < default_entity || cast::<i32, _>(g.lending_bank[ae]) == default_entity {
                prefill_symbol = 0;
            }
            if prefill_symbol == 0 {
                let mut bank_id = 11;
                loop {
                    let ae = g.active_entity;
                    if g.entity_status[bank_id] == 1
                        && cast::<i32, _>(g.lending_bank[ae]) != bank_id
                        && g.cur_player == g.corp_controller[bank_id]
                        && {
                            update_corp_control(g, ui, rng, bank_id);
                            g.cur_player == g.voting_controller
                        }
                    {
                        let loan_price_pct = calc_loan_sale_price_pct(g, ui, rng, ae);
                        let mut bank_liquidity = g.t_bills[bank_id];
                        bank_liquidity = g.cash[bank_id] + bank_liquidity;
                        if bank_liquidity < Ext::from_f64(0.0) {
                            bank_liquidity = Ext::from_f64(0.0);
                        }
                        let ae = g.active_entity;
                        if sf(0.01f32) * cast::<Ext, _>(loan_price_pct) * g.bank_debt[ae] <= bank_liquidity + g.cash[ae]
                            || g.bank_debt[ae] <= sf(0.01f32)
                        {
                            default_entity = bank_id;
                            prefill_symbol = 1;
                            let mut skip_rest = false;
                            if g.exchange_rate * Ext::from_f64(3000.0) < g.cash[ae] {
                                calc_entity_financials(g, ui, rng, bank_id);
                                if g.total_assets * sf(0.2f32) < g.cash[ae] && g.bank_debt[ae] * sf(1.5f32) < g.cash[ae] {
                                    default_entity = 0;
                                    prefill_symbol = 0;
                                    skip_rest = true;
                                }
                            }
                            if !skip_rest {
                                bank_id = 85;
                            }
                        }
                    }
                    bank_id = bank_id + 1;
                    if 85 < bank_id {
                        break;
                    }
                }
            }
        }
        if g.action_code == "ASSETS" {
            if g.active_entity < 131 {
                prefill_symbol = 0;
            } else {
                if default_entity < 131 || 69 < g.industry_num[default_entity] {
                    prefill_symbol = 0;
                }
                if g.industry_num[g.active_entity] < 70
                    && g.industry_num[default_entity] != g.industry_num[g.active_entity]
                {
                    prefill_symbol = 0;
                }
            }
        }
        if g.action_code == "ANTITRUST" && 130 < g.active_entity {
            if default_entity < 131 {
                prefill_symbol = 0;
            } else if g.industry_num[g.active_entity] != g.industry_num[default_entity] {
                prefill_symbol = 0;
            }
        }
        if g.action_code == "CALLLOAN" && 10 < default_entity && default_entity < 86 {
            prefill_symbol = 0;
        }
        if g.action_code == "CHANGESYM" {
            prefill_symbol = 0;
        }
        if default_entity == g.active_entity {
            prefill_symbol = 0;
        } else if 10 < default_entity && g.entity_status[default_entity] != 1 {
            prefill_symbol = 0;
        }
        if g.action_code == "ETF_TRADES"
            && 1580 < default_entity
            && g.sel_entity == default_entity
            && get_etf_adviser_human_status(g, ui, rng, default_entity) == 2
        {
            if g.last_sel_entity < 11 || default_entity == g.last_sel_entity {
                prefill_symbol = 0;
            } else {
                default_entity = g.last_sel_entity;
                prefill_symbol = 1;
            }
        }
        if g.action_code == "ETF_TRADES"
            && get_selected_player_etf(g, ui, rng) == default_entity
            && g.sel_entity == g.active_entity
        {
            default_entity = g.active_entity;
            if g.active_entity < 11 {
                prefill_symbol = 0;
            }
        }
        if prefill_symbol == 1 {
            text = g.ent_symbol[default_entity].clone();
        }
    }
    if g.action_code == "CHANGEBANK" || g.action_code == "ANTITRUST" || g.action_code == "CALLLOAN" {
        g.action_code = PbStr::new();
    }
    if g.action_code == "ETF_TRADES" {
        g.action_code = PbStr::new();
    }
    text
}

/// Port of GetSelectedPlayerEtf (0x652b17): the selected entity (or else the last selected one) if it is
/// a fu [runtime resource wording] of the current player (`GetEtfAdviserHumanStatus = 2`), else 0.
pub fn get_selected_player_etf(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> i32 {
    let sel = g.sel_entity;
    if get_etf_adviser_human_status(g, ui, rng, sel) == 2 {
        return g.sel_entity;
    }
    let last = g.last_sel_entity;
    if get_etf_adviser_human_status(g, ui, rng, last) == 2 {
        return g.last_sel_entity;
    }
    0
}

/// Port of PickFromStreamingList (0x5de64f).
/// Shows the current player's streaming-quotes list (`WatchList`) as a pick list and returns the clicked
/// company (`TargetCorp`, set by [`follow_report_link`]), or 0. `mode = 0`: pick a stock to remove
/// ("GetCorp"); `mode = 1`: pick the new active entity ("ChangeCorp"; an empty list gives a popup).
pub fn pick_from_streaming_list(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, mode: i32) -> i32 {
    let __content = crate::content::current();
    let mut result = 0;
    if g.cur_player == 0 || g.game_in_progress == 0 || g.game_end_flag == "Y" {
        return result;
    }
    let mut menu_lines: Vec<PbStr> = vec![PbStr::new(); 91];
    g.entity_pick_mode = s("GetCorp");
    menu_lines[1] = s(__content.string(12726));
    menu_lines[2] = s("---------------------------------------------------------------------");
    if mode == 1 {
        g.entity_pick_mode = s(__content.string(6093));
        menu_lines[1] = s(__content.string(12727));
        menu_lines[2] = s("-----------------------------------------------------------------");
        if g.watch_list[(1, g.cur_player)] == 0 {
            let mut m = s(__content.string(12728));
            show_quick_message(g, ui, &mut m);
            return result;
        }
    }
    let mut line_idx = 2;
    for slot in 1..=15 {
        let corp_id = g.watch_list[(slot, g.cur_player)];
        if corp_id != 0 {
            line_idx = line_idx + 1;
            menu_lines[line_idx] = g.ent_name[corp_id].clone() + " (" + &g.ent_symbol[corp_id] + ")";
            let id_text = link_id(corp_id);
            menu_lines[line_idx] = menu_lines[line_idx].clone() + &string_fill(85, b" ") + "@C" + &id_text;
        }
    }
    menu_lines[line_idx + 4] = s("@END");
    for line_idx in 1..=90 {
        g.text_lines[line_idx] = menu_lines[cast::<usize, _>(line_idx)].clone();
    }
    g.text_box_title = s(__content.string(12729));
    g.target_corp = 0;
    show_text_report(g, ui, rng);
    if mode == 1 && 10 < g.target_corp {
        // (UI) CONTROL SET TEXT hDlgMain, 202, EntSymbol(TargetCorp)
    }
    result = g.target_corp;
    result
}

/// Port of ShowCompanyPickList (0x5825fc).
/// Report " [runtime resource wording] BY <PLAYER> ===": every company the current player controls
/// (after updating the control chains of the player's holdings of 20% and more), with earnings per share
/// (prior year quarter, last quarter, projected), credit rating, analyst rating ("NOT TRADED" without a
/// public float), growth rate, R&D/marketing spending and autopilot, sorted by name (`ARRAY SORT`), with
/// `@C` links in `"ChangeCorp"`/`"GetCorp"` pick mode. A click goes through [`follow_report_link`].
///
/// When the current or acting player is a computer player (before the game's end) the list is not shown;
/// instead the player's turn is ended (`TurnActionCount = 5`, market display, `EndTurn`; `now` is the
/// local time).
pub fn show_company_pick_list(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    let __content = crate::content::current();
    // DIM listLines(1 TO 1000). Past 1000 lines (bug #241, ~990 controlled companies) the original
    // raises error 9; the port's list is longer so that case does not panic.
    let mut list_lines: Vec<PbStr> = vec![PbStr::new(); 1700];
    let dash_line = string_fill(71, b"-");
    let pad_spaces = string_fill(30, b" ");
    let num_fmt = "#####.00";
    list_lines[1] = s(__content.string(12730));
    if g.entity_pick_mode == __content.string(6093) {
        list_lines[1] = list_lines[1].clone() + __content.string(12731) + "\"" + __content.string(6112) + "\"";
    }
    list_lines[2] = s("           --------------------------");
    if g.entity_pick_mode == __content.string(6093) {
        list_lines[2] = list_lines[2].clone() + &string_fill(23, b"-");
    }
    list_lines[4] = dash_line;
    list_lines[5] = s(__content.string(12732));
    list_lines[6] = s(__content.string(12733));
    list_lines[7] = s(__content.string(12734));
    list_lines[8] = s(__content.string(12735));
    list_lines[9] = s("------------------------- -------- -------- -------- ------ ----------- ------ ------ -----");
    let mut line_count: i32 = 9;
    let mut is_computer = 0;
    if 0 < g.cur_player && g.is_computer_player[g.cur_player] == 1 {
        is_computer = 1;
    }
    if 0 < g.active_entity && g.active_entity <= g.num_players && g.is_computer_player[g.active_entity] == 1 {
        is_computer = 1;
    }
    if is_computer == 1 && !(g.game_end_flag == "Y") {
        g.turn_action_count = 5;
        update_market_display(g, ui, rng, now.time_secs());
        g.ticker_stop_req = s("Y");
        end_turn(g, ui, rng, now);
        if !(g.game_end_flag == "Y") {
            g.ticker_stop_req = s("N");
        }
        return;
    }
    pack_holding_arrays(g, ui);
    for slot in 1..=15 {
        let mut chain_steps = 0;
        if 19 < g.hold_stock_pct[(slot, g.cur_player)] {
            let chain_corp_id = cast::<i32, _>(g.hold_stock_id[(slot, g.cur_player)]);
            update_control_chain(g, ui, rng, chain_corp_id);
            let mut largest_holder = cast::<i16, _>(get_largest_holding_id(g, chain_corp_id));
            while 10 < largest_holder && chain_steps < 100 {
                chain_steps = chain_steps + 1;
                let holder_id = cast::<i32, _>(largest_holder);
                if 10 < holder_id {
                    update_corp_control(g, ui, rng, holder_id);
                }
                largest_holder = cast::<i16, _>(get_largest_holding_id(g, holder_id));
            }
        }
    }
    let mut idx = 0;
    for corp_id in 11..=1600 {
        if g.entity_status[corp_id] != 1 {
            continue;
        }
        if g.cur_player == g.corp_controller[corp_id] {
            update_corp_control(g, ui, rng, corp_id);
        }
        if g.cur_player != g.corp_controller[corp_id] {
            continue;
        }
        line_count = line_count + 1;
        let mut line_text = g.ent_name[corp_id].clone() + &pad_spaces;
        line_text = mid(line_text.as_bytes(), 1, 26);
        let mut proj_text = parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 4);
        let mut earnings_val = val(proj_text.as_bytes());
        let mut field_text = pad_spaces.clone()
            + &fmt((sf(0.01f32) * earnings_val) / g.shares_outstanding[corp_id], num_fmt)
            + "  ";
        field_text = right(field_text.as_bytes(), 9);
        if earnings_val == Ext::from_f64(0.0) {
            field_text = s(__content.string(8615));
        }
        proj_text = parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 3);
        earnings_val = val(proj_text.as_bytes());
        let mut last_qtr_text = pad_spaces.clone()
            + &fmt((sf(0.01f32) * earnings_val) / g.shares_outstanding[corp_id], num_fmt)
            + "  ";
        last_qtr_text = right(last_qtr_text.as_bytes(), 9);
        if earnings_val == Ext::from_f64(0.0) {
            last_qtr_text = s(__content.string(8615));
        }
        proj_text = pad_spaces.clone()
            + &fmt((g.proj_qtr_earnings[corp_id] * sf(0.01f32)) / g.shares_outstanding[corp_id], num_fmt)
            + "  ";
        proj_text = right(proj_text.as_bytes(), 9);
        if g.proj_qtr_earnings[corp_id] == Ext::from_f64(0.0) {
            proj_text = s(__content.string(8615));
        }
        line_text = line_text + &field_text + &last_qtr_text + &proj_text;
        calc_entity_financials(g, ui, rng, corp_id);
        field_text = s("    ") + &g.credit_rating_str;
        field_text = right(field_text.as_bytes(), 5) + "  ";
        line_text = line_text + &field_text;
        update_corp_control(g, ui, rng, corp_id);
        if g.public_pct < 1 {
            field_text = s(__content.string(8618));
        } else {
            idx = calc_analyst_rating(g, ui, rng, corp_id);
            field_text = match idx {
                1 => s(__content.string(8617)),
                2 => s(__content.string(12736)),
                3 => s(__content.string(12737)),
                4 => s(__content.string(12738)),
                _ => s(__content.string(6179)),
            };
        }
        line_text = line_text + &field_text;
        if g.industry_num[corp_id] < 70 {
            field_text = s("     ") + &str_s(cast::<Ext, _>(g.growth_rate[corp_id])) + "% ";
            field_text = right(field_text.as_bytes(), 7);
        } else {
            field_text = s("   N/A ");
        }
        line_text = line_text + &field_text;
        if g.industry_num[corp_id] < 70 {
            field_text = s("     ") + &str_s(cast::<Ext, _>(g.disc_spend_pct[corp_id])) + "% ";
            field_text = right(field_text.as_bytes(), 7);
        } else {
            field_text = s("   N/A ");
        }
        line_text = line_text + &field_text;
        field_text = s("  NO");
        if g.auto_pilot[corp_id] == 1 {
            field_text = s("  YES");
        }
        list_lines[cast::<usize, _>(line_count)] = line_text + &field_text;
        if g.entity_pick_mode == __content.string(6093) || g.entity_pick_mode == "GetCorp" {
            let id_text = link_id(corp_id);
            list_lines[cast::<usize, _>(line_count)] =
                list_lines[cast::<usize, _>(line_count)].clone() + &string_fill(70, b" ") + "@C" + &id_text;
        }
        if line_count == 995 {
            // BUG (original, #241): meant to end the loop (`corpId = 1600`) but sets `idx`.
            idx = 1600;
        }
    }
    let _ = idx;
    // ARRAY SORT listLines(10) FOR lineCount - 9
    let n = cast::<usize, _>((line_count - 9).max(0));
    pb_array_sort::<PbStr, ()>(&mut list_lines[10..10 + n], None::<&mut [()]>, false, &|a: &PbStr, b: &PbStr| {
        compare(a.as_bytes(), b.as_bytes()) == std::cmp::Ordering::Less
    });
    if line_count == 995 {
        line_count = 996;
        list_lines[cast::<usize, _>(line_count)] = s(__content.string(8853));
    }
    line_count = line_count + 1;
    list_lines[cast::<usize, _>(line_count)] = list_lines[9].clone();
    line_count = line_count + 1;
    list_lines[cast::<usize, _>(line_count)] = s("  ");
    line_count = line_count + 1;
    list_lines[cast::<usize, _>(line_count)] = s("  ");
    line_count = line_count + 2;
    list_lines[cast::<usize, _>(line_count)] = s("@END");
    let last_line = line_count;
    for idx in 1..=last_line {
        g.text_lines[idx] = list_lines[cast::<usize, _>(idx)].clone();
    }
    g.text_box_title = s(__content.string(12739))
        + &ucase(g.cur_player_name.as_bytes(), CaseMode::Ansi)
        + " ===";
    g.text_box_mode = s("120");
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}

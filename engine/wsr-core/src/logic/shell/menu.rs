//! The action-menu dispatcher: `ExecuteMenuCommand` (0x553664) and its caller `DoTransactionMenuAction`
//! (0x552f0b).
//!
//! # How a host drives the action menus
//!
//! The original has seven "action menus" (`ActionMenuMode`, see [`mode`]), each a dialog with numbered
//! buttons built by `ShowActionMenu` (0x4131ea, [`super::action_menu`]). The button captions depend on
//! the selected entity (`SelEntity`, research menus) or the acting entity (`ActiveEntity`, transaction
//! menus), so the dispatcher first maps the button number to a command. A click on button `n` is
//! `ActionMenu_Btn<n>` → [`do_transaction_menu_action`]`(n)` → [`execute_menu_command`]`(n)`.
//!
//! A host opens a menu with `TmpExt1 = mode` + `show_action_menu` (or the trading-desk buttons that do so)
//! and calls [`do_transaction_menu_action`] with the clicked button number (1-based, in caption order).
//! Calling [`execute_menu_command`] directly skips the turn bookkeeping around it (ticker steps, advice
//! text, end of turn).
//!
//! # Button → command mapping
//!
//! Notation: `n → m` remaps button `n` to command `m` (before dispatch). "corp" = `ActiveEntity` 11..1600,
//! "player" = 1..10, "bank" = 11..85 (industry 1), "insurer" = 86..130 (industry 2), ind = `IndustryNum`.
//!
//! **Mode 1, entity research** (for `SelEntity`). Remaps: player: 1→2, 2→5, 3→6, 4→7, 5→10, 6→11, 7→13,
//! 8→14, 9→15, 10→16, 11→12; bank: 11→13, 12→16; corp ≥ 86 with ind 3..69: 11→13, 12→14, 13→15, 14→16;
//! ind 70: 11→14, 12→15, 13→16; 1600: the same; funds 1584/1585: 10→14, 11→16; funds < 1584: 10→16;
//! other ind 71: 11→16; ind 2: 11→14, 12→16; then (all ≥ 86 except ind 71 below 1586) 10→13; then 9→10.
//! Commands: 1 research report, 2 balance sheet (players: personal financial statement), 3 earnings
//! report, 4 shareholder list, 5 portfolio listing (10: tax basis), 6 ownership diagram, 7 credit info
//! (symbol dialog; defunct: description), 8 industry summary, 9 bank loan customers / industry projection /
//! option positions, 11 advances to corps, 12 "My Corporations" (select acting entity), 13 option
//! positions, 14 futures positions (a picked own contract opens its trade), 15 physical commodities
//! (a picked own lot is sold), 16 cash-flow projection.
//!
//! **Mode 2, general research**: 1 market share, 2 growth rates, 3 industry projection, 4 industry
//! summary, 5 economic statistics, 6 interest rates, 7 news headlines, 8 standings, 9 most cash, 10
//! largest tax losses, 11 largest market cap, 12 database search, 13 who owns what.
//!
//! **Mode 3, miscellaneous**: 1 borrow, 2 repay loan, 3 advance to corp / stock split, 4 recall advance /
//! reverse split, 5 prepay tax / name change, 6 offer stock for sale, 7 price alert (operating corps:
//! offer assets), 8 T-bills (operating corps: price alert), 9 T-bills.
//!
//! **Mode 4, buy/sell**: the button is mapped to a command 1..14 — corps > 130: n > 2 → n + 3, 6 → 9; ind
//! 70: 7..11 → n − 1, 11 → 14, 12 → 11, 13 → 12, 14 → 13; else 7 → 10, 8..11 → n − 2, 12 → 14, 13 → 11,
//! 14 → 12, 15 → 13. Banks/insurers (11..130): 3..5 → n + 3, 6..8 → n − 3, ind < 3 also 9..12 → n − 3,
//! insurers 13 → 11. Players: 10 → 14. Commands: 1 buy stock (players short of anything: cover short), 2
//! sell stock (players: sell short), 3/4 buy/sell corporate bonds, 5 government bonds, 6 buy calls
//! (corps: merger), 7 sell calls (corps: greenmail), 8 buy puts (corps: LBO), 9 sell puts (corps: buy
//! business assets), 10 sell business assets, 11 futures desk, 12 physical commodities desk, 13 crypto,
//! 14 advanced options station. (For corps the option commands are re-checked with the original button
//! number, see the code.)
//!
//! **Mode 5, financing**: 1 start-up, 2 capital contribution, 3/4 public/private stock offering, 5 issue
//! bonds, 6 buy back / call bonds, 7 extraordinary dividend, 8/9 tax-free/taxable liquidation, 10 spin-off.
//! Corps 11..130: 9 → 10.
//!
//! **Mode 6, management**: 1 elect me CEO, 2 resign as CEO, 3 change managers, 4 dividend payout, 5
//! productivity spending (ind 70: autopilot), 6 growth rate, 7 restructure (banks: asset allocation,
//! insurers: ETF adviser), 8 antitrust suit (ind < 3: autopilot), 9 autopilot, 10 ETF adviser fee.
//!
//! **Mode 7, other**: 1 harassing lawsuit, 2 rumors, 3/4 exercise call/put, 5 change bank (banks: call in
//! loan), 6..11 by entity kind: swaps, for-sale items, bank-loan trading, freeze credit line, contingency
//! reserve (decrease/increase earnings), subprime mortgages.
//!
//! The ON ERROR handler (0x557adc) shows `ShowRuntimeError("ExecuteMenuCommand …")` for a PB runtime
//! error; the port raises none, so it is not ported. Every exit goes through `ExecuteMenuCommand_ExitSub`
//! (0x557b6f, the frame release; ported as [`crate::logic::corp_finance::bonds::execute_menu_command_exit_sub`])
//! or its thunk `ExecuteMenuCommand_ExitSubThunk` (0x557ad7, a `JMP` to it): both are plain returns here.
use crate::pb::real::{Ext, cast};


use super::prelude::*;

use crate::logic::{
    assets, commod_ai, control, corp_ai, corp_finance, earnings, economy, etf_derivs, finance, reports_corp,
    reports_lists, research, restructure, sim_entity, trading, turn,
};
use crate::logic::report::{describe_defunct_entity, show_economic_stats, show_interest_rates, show_shareholder_list};

/// `ActionMenuMode` values: which action menu is open (set by `ShowActionMenu` from `TmpExt1`).
pub mod mode {

    /// Entity research menu (reports about `SelEntity`).
    pub const ENTITY_RESEARCH: i32 = 1;
    /// General research menu (market-wide reports).
    pub const GENERAL_RESEARCH: i32 = 2;
    /// Miscellaneous functions (loans, splits, name change, alerts, T-bills).
    pub const MISC: i32 = 3;
    /// Buy / sell transactions (stocks, bonds, options, assets, commodities, crypto).
    pub const BUY_SELL: i32 = 4;
    /// Financing transactions (start-ups, offerings, bonds, dividends, liquidations, spin-offs).
    pub const FINANCING: i32 = 5;
    /// Management transactions (CEO, managers, policies, restructuring, antitrust, autopilot).
    pub const MANAGEMENT: i32 = 6;
    /// Other transactions (lawsuits, rumors, option exercise, banks, swaps, for-sale items).
    pub const OTHER: i32 = 7;
}

/// `IndustryNum(e)`.
fn ind(g: &Globals, e: i32) -> i32 {
    cast::<i32, _>(g.industry_num[e])
}

/// `VAL(RIGHT$(TIME$, 2))`.
fn tsecs(env: &Env) -> i32 {
    env.now.second
}

/// Port of ExecuteMenuCommand (0x553664), with its exits ExecuteMenuCommand_ExitSubThunk (0x557ad7) and
/// ExecuteMenuCommand_ExitSub (0x557b6f): runs button `menu_choice` of the open action menu
/// (`ActionMenuMode`), see the module docs for the mapping. Window plumbing (control texts, `DIALOG SHOW
/// STATE`, `DIALOG END`) is skipped; `InfoDlgReopen` and all game state are exact.
pub fn execute_menu_command(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, menu_choice: i32) {
    let saved_active = g.active_entity;
    let saved_sel = g.sel_entity;
    let saved_ind = g.sel_industry;
    if !env.skips(0x49117c) {
        sim_entity::quarter_start::pack_holding_arrays(g, ui);
    }
    let mut mc = menu_choice;
    if g.sel_entity < 11 && g.action_menu_mode == 1 {
        mc = match mc {
            1 => 2,
            2 => 5,
            3 => 6,
            4 => 7,
            5 => 10,
            6 => 11,
            7 => 13,
            8 => 14,
            9 => 15,
            10 => 16,
            11 => 12,
            x => x,
        };
    }
    if g.action_menu_mode == 1 {
        remap_entity_research(g, &mut mc);
    }
    if g.action_menu_mode == 5 && 10 < g.active_entity && g.active_entity < 131 && mc == 9 {
        mc = 10;
    }
    if 3 < g.action_menu_mode && 3 < g.turn_action_count && !env.skips(0x493e2b) {
        economy::close_all_dialogs_skipping(g, ui, rng, 2, env);
    }
    if 10 < g.active_entity {
        if !env.skips(0x41f71b) {
            let e = g.active_entity;
            control::update_corp_control(g, ui, rng, e);
        }
        if g.cur_player != g.voting_controller {
            g.active_entity = g.cur_player;
            g.active_entity_name = g.cur_player_name.clone();
        }
    }
    match g.action_menu_mode {
        1 => {
            mode_entity_research(g, ui, rng, env, mc, saved_sel);
        }
        2 => {
            mode_general_research(g, ui, rng, env, mc, saved_sel, saved_ind);
        }
        3 => mode_misc(g, ui, rng, env, mc),
        4 => {
            if mode_buy_sell(g, ui, rng, env, mc) {
                buy_sell_tail(g, ui, rng, env, saved_active, saved_sel);
            }
        }
        5 => mode_financing(g, ui, rng, env, mc),
        6 => mode_management(g, ui, rng, env, mc),
        7 => mode_other(g, ui, rng, env, mc),
        _ => {}
    }
}

/// The entity-kind remapping of the entity research menu (`ActionMenuMode = 1`), 0x55392e..0x553ec5.
fn remap_entity_research(g: &Globals, mc: &mut i32) {
    let se = g.sel_entity;
    if 85 < se {
        let i = ind(g, se);
        if i != 2 && i <= 69 {
            // ind 3..69
            if *mc == 14 {
                *mc = 16;
            }
            if *mc == 13 {
                *mc = 15;
            }
            if *mc == 12 {
                *mc = 14;
            }
            if *mc == 11 {
                *mc = 13;
            }
            return;
        }
        if i == 70 || (i == 71 && se == 1600) {
            if *mc == 13 {
                *mc = 16;
            }
            if *mc == 12 {
                *mc = 15;
            }
            if *mc == 11 {
                *mc = 14;
            }
        } else if i == 71 && 1583 < se && se < 1586 {
            if *mc == 11 {
                *mc = 16;
            }
            if *mc == 10 {
                *mc = 14;
            }
        } else if i == 71 && se < 1584 {
            if *mc == 10 {
                *mc = 16;
            }
        } else if i == 71 {
            if *mc == 11 {
                *mc = 16;
            }
        } else if i == 2 {
            if *mc == 12 {
                *mc = 16;
            }
            if *mc == 11 {
                *mc = 14;
            }
        }
        if !(ind(g, se) == 71 && se < 1586) && *mc == 10 {
            *mc = 13;
        }
        if *mc == 9 {
            *mc = 10;
        }
    } else if 10 < se {
        if *mc == 11 {
            *mc = 13;
        }
        if *mc == 12 {
            *mc = 16;
        }
    }
}

/// `ActionMenuMode = 1`: the entity research menu (0x553f96..0x554bb0).
fn mode_entity_research(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, mc: i32, saved_sel: i32) {
    let __content = crate::content::current();
    let tm = tsecs(env);
    match mc {
        1 => {
            set(&mut g.entity_pick_mode, "");
            if !env.skips(0x448ed3) {
                let e = g.sel_entity;
                research::show_research_report(g, ui, rng, e);
            }
        }
        2 => {
            set(&mut g.entity_pick_mode, "");
            if g.sel_entity < 11 {
                if !env.skips(0x43987e) {
                    reports_lists::show_personal_financial_statement(g, ui, rng, 0);
                }
            } else if !env.skips(0x430898) {
                let e = g.sel_entity;
                reports_corp::show_balance_sheet(g, ui, rng, e);
            }
        }
        3 => {
            set(&mut g.entity_pick_mode, "");
            if !env.skips(0x462f1e) {
                let e = g.sel_entity;
                reports_corp::show_earnings_report(g, ui, rng, e);
            }
        }
        4 => {
            let entity_arg = g.sel_entity;
            set(&mut g.entity_pick_mode, __content.string(6093));
            if !env.skips(0x421544) {
                show_shareholder_list(g, ui, rng, entity_arg, 1);
            }
        }
        5 | 10 => {
            if !list_portfolio(g, ui, rng, env, mc) {
                return; // (never: every path continues at 0x554b95)
            }
        }
        6 => {
            if !env.skips(0x54eb71) { crate::logic::info_screens::ownership::show_ownership_diagram(g, ui, rng, env.now); }
            g.dlg_open_holdings_diagram = 0;
        }
        7 => {
            let se = g.sel_entity;
            if g.entity_status[se] == 2 {
                if !env.skips(0x480a70) {
                    describe_defunct_entity(g, ui, se);
                }
                return; // ExecuteMenuCommand_ExitSub (InfoDlgReopen untouched)
            }
            set(&mut g.action_code, "CHANGESYM");
            set(&mut g.input_dlg_label, "");
            set(&mut g.input_dlg_prompt, "");
            g.input_dlg_title = s(__content.string(10151)) + &g.ent_name[se];
            if !env.skips(0x423170) { super::credit::show_credit_info(g, ui, rng); }
        }
        8 => {
            let mut entity_arg = g.sel_industry;
            if 10 < g.sel_entity {
                entity_arg = ind(g, g.sel_entity);
            }
            set(&mut g.entity_pick_mode, __content.string(6093));
            if !env.skips(0x47f17e) {
                reports_lists::show_industry_report(g, ui, rng, entity_arg);
            }
        }
        9 => {
            let entity_arg = g.sel_entity;
            if 10 < entity_arg {
                if ind(g, entity_arg) == 1 {
                    set(&mut g.entity_pick_mode, __content.string(6093));
                    if !env.skips(0x461796) {
                        reports_lists::show_bank_loan_customers(g, ui, rng, 0);
                    }
                } else if 2 < ind(g, entity_arg) && ind(g, entity_arg) < 70 {
                    let ea = ind(g, g.sel_entity);
                    set(&mut g.entity_pick_mode, __content.string(6093));
                    if !env.skips(0x460058) {
                        reports_lists::show_industry_projection(g, ui, rng, ea, tm);
                    }
                } else if !env.skips(0x5f51fc) {
                    let e = g.sel_entity;
                    reports_lists::show_option_positions(g, ui, rng, e, 0, 0);
                }
            }
        }
        11 => {
            set(&mut g.entity_pick_mode, __content.string(6093));
            if !env.skips(0x499f8e) {
                let e = g.sel_entity;
                reports_lists::show_my_advances(g, ui, rng, e);
            }
        }
        12 => {
            if !env.skips(0x4721dc) {
                trading::select_acting_entity(g, ui, rng, 2, env.now);
            }
        }
        13 => {
            if !env.skips(0x5f51fc) {
                let e = g.sel_entity;
                reports_lists::show_option_positions(g, ui, rng, e, 0, 0);
            }
        }
        14 => {
            let rec_idx = if !env.skips(0x62cbaf) {
                let e = g.sel_entity;
                reports_lists::show_futures_positions(g, ui, rng, e, 1)
            } else {
                0
            };
            if g.game_end_flag != "Y" && 0 < rec_idx && cast::<Ext, _>(rec_idx) <= val(g.futures_recs[0].as_bytes()) {
                let rec = g.futures_recs[rec_idx].clone();
                if val(parse_any(rec.as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(g.active_entity) {
                    // (UI) DIALOG END hDlgTransMenu; DIALOG SHOW STATE hDlgMain, 0
                    g.action_menu_open = 0;
                    g.cur_commodity = trim(parse_any(rec.as_bytes(), b"|", 1).as_bytes());
                    let ae = g.active_entity;
                    if Ext::from_f64(0.0) <= val(parse_any(rec.as_bytes(), b"|", 3).as_bytes()) {
                        if !env.skips(0x620f39) {
                            commod_ai::desk::futures_sell_or_short(g, ui, rng, 2, ae, rec_idx);
                        }
                    } else if !env.skips(0x61cebb) {
                        commod_ai::desk::futures_buy_or_cover(g, ui, rng, 2, ae, rec_idx);
                    }
                    if !env.skips(0x424754) {
                        turn::enable_main_controls(g);
                    }
                }
            }
        }
        15 => {
            let rec_idx = if !env.skips(0x65c08f) {
                let e = g.sel_entity;
                reports_lists::list_phys_commod_holdings(g, ui, rng, e, 0)
            } else {
                0
            };
            if g.game_end_flag != "Y" && 0 < rec_idx && cast::<Ext, _>(rec_idx) <= val(g.phys_commod_rec[0].as_bytes()) {
                let rec = g.phys_commod_rec[rec_idx].clone();
                if val(parse_any(rec.as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(g.active_entity) {
                    // (UI) DIALOG END hDlgTransMenu; DIALOG SHOW STATE hDlgMain, 0
                    g.action_menu_open = 0;
                    g.cur_commodity = trim(parse_any(rec.as_bytes(), b"|", 1).as_bytes());
                    let ae = g.active_entity;
                    if !env.skips(0x65db62) {
                        commod_ai::physical::sell_phys_commod(g, ui, rng, ae, rec_idx, 0, 0.0f32, 0);
                    }
                    if !env.skips(0x424754) {
                        turn::enable_main_controls(g);
                    }
                }
            }
        }
        16 => {
            if !env.skips(0x6a20e0) {
                let e = g.sel_entity;
                crate::logic::cashflow::show_cash_flow_projection(g, ui, rng, e, 0);
            }
        }
        _ => return, // CASE ELSE: ExecuteMenuCommand_ExitSub
    }
    // 0x554b95
    if g.sel_entity != saved_sel {
        g.info_dlg_reopen = 1;
    }
}

/// Entity research commands 5 (List Portfolio) and 10 (Tax Basis Info), 0x554114..0x5543e0. Always
/// continues at the common end (returns true).
fn list_portfolio(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, mc: i32) -> bool {
    let __content = crate::content::current();
    let entity_arg = g.sel_entity;
    g.tmp_ext1 = Ext::from_f64(3.0);
    if mc == 10 {
        g.tmp_ext1 = -Ext::from_f64(3.0);
    }
    set(&mut g.entity_pick_mode, __content.string(6093));
    if !env.skips(0x426610) {
        finance::calc_entity_financials(g, ui, rng, entity_arg);
    }
    let mut show_own = true;
    if g.tmp_ext1.abs() == Ext::from_f64(3.0) {
        let n = if !env.skips(0x5f0cb9) { control::find_option_positions(g, ui, entity_arg, 0, 0, 0) } else { 0 };
        if n == 0
            && g.hold_stock_id[(0, entity_arg)] == 0
            && g.tmp_gov_bonds_value == Ext::from_f64(0.0)
            && g.tmp_corp_bonds_value == Ext::from_f64(0.0)
            && 10 < entity_arg
        {
            let cp = g.cur_player;
            let n2 = if !env.skips(0x5f0cb9) { control::find_option_positions(g, ui, cp, 0, 0, 0) } else { 0 };
            let player_has_nothing =
                n2 == 0 && g.hold_stock_id[(0, cp)] == 0 && g.hold_bond_id[(0, cp)] == 0;
            if !player_has_nothing {
                let msg = g.ent_name[entity_arg].clone()
                    + __content.string(10152)
                    + "\r"
                    + "\r"
                    + __content.string(10153);
                let answer = ui.msgbox(&msg, 8484, &s(__content.string(10132)));
                if answer == id::YES {
                    if !env.skips(0x4422a4) {
                        reports_lists::show_portfolio_listing(g, ui, rng, cp, 0, 0);
                    }
                } else {
                    g.tmp_ext1 = Ext::from_f64(0.0);
                }
                show_own = false;
            }
        }
    }
    if show_own && !env.skips(0x4422a4) {
        reports_lists::show_portfolio_listing(g, ui, rng, entity_arg, 0, 0);
    }
    true
}

/// `ActionMenuMode = 2`: the general research menu (0x554bb5..0x5550bc).
fn mode_general_research(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    env: &Env,
    mc: i32,
    saved_sel: i32,
    saved_ind: i32,
) {
    let __content = crate::content::current();
    match mc {
        1 | 9 | 10 | 11 => {
            let list_type = match mc {
                1 => 1,
                9 => 3,
                10 => 2,
                _ => 4,
            };
            if !env.skips(0x49e21f) {
                reports_lists::show_top_companies_list(g, ui, rng, list_type);
            }
        }
        2 => {
            let entity_arg = g.sel_industry;
            set(&mut g.entity_pick_mode, __content.string(10154));
            if !env.skips(0x45f1af) {
                reports_lists::show_industry_growth_projections(g, ui, rng);
            }
            if g.sel_industry != entity_arg && 0 < g.sel_industry && !env.skips(0x46df33) {
                trading::select_industry_combo_item(g);
            }
        }
        3 => {
            set(&mut g.entity_pick_mode, __content.string(6093));
            if !env.skips(0x460058) {
                let i = g.sel_industry;
                reports_lists::show_industry_projection(g, ui, rng, i, tsecs(env));
            }
        }
        4 => {
            set(&mut g.entity_pick_mode, __content.string(6093));
            if !env.skips(0x47f17e) {
                let i = g.sel_industry;
                reports_lists::show_industry_report(g, ui, rng, i);
            }
        }
        5 => {
            if !env.skips(0x47dd94) {
                show_economic_stats(g, ui, rng);
            }
        }
        6 => {
            set(&mut g.entity_pick_mode, "");
            if !env.skips(0x45c781) {
                show_interest_rates(g, ui, rng);
            }
        }
        7 => {
            let mut count = 0;
            for k in 1..=60 {
                if instr(1, g.news_headline[k].as_bytes(), b".............") < 1 {
                    count += 1;
                    g.text_lines[count] = s(" - ") + &g.news_headline[k];
                }
            }
            set(&mut g.text_lines[count + 1], "@END");
            set(&mut g.text_box_title, __content.string(10155));
            g.text_has_links = 1;
            set(&mut g.text_box_mode, "84");
            if !env.skips(0x41dfaa) {
                show_text_report(g, ui, rng);
            }
        }
        8 => {
            if !env.skips(0x446484) {
                reports_lists::show_player_standings(g, ui, rng);
            }
        }
        12 => {
            let prev_sel = g.sel_entity;
            if !env.skips(0x493e2b) {
                economy::close_all_dialogs_skipping(g, ui, rng, 2, env);
            }
            if !env.skips(0x5472ee) { crate::logic::info_screens::db_search::show_db_search_dialog(g, ui, rng); }
            if g.sel_entity == prev_sel {
                if g.clock_state == "RUN" || 1589 < g.sim_co_count {
                    if !env.skips(0x46e001) {
                        turn::pause_ticker(g, &ui.local_time(), 0.0f32);
                    }
                } else {
                    suppressed_ticker_steps(g, ui, rng, env, 4);
                }
            } else {
                g.tmp_ext1 = Ext::from_f64(1.0);
                if !env.skips(0x4131ea) {
                    super::action_menu::show_action_menu(g, ui, rng, env);
                }
            }
        }
        13 => {
            if !env.skips(0x66f1a8) {
                super::dialogs::show_who_owns_what_dialog(g, ui, rng, env);
            }
        }
        _ => return, // CASE ELSE: ExecuteMenuCommand_ExitSub
    }
    // 0x55507c
    if g.sel_entity != saved_sel || g.sel_industry != saved_ind {
        g.info_dlg_reopen = 1;
        if g.sel_entity == saved_sel {
            g.info_dlg_reopen = 2;
        }
    }
}

/// `TickerPauseUntil = 0: ClockState = "": TickerOn = 0: ActionCode = "SUPP": TickerSimSteps(n):
/// ActionCode = ""` — a batch of suppressed ticks.
fn suppressed_ticker_steps(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, n: i32) {
    g.ticker_pause_until = 0;
    set(&mut g.clock_state, "");
    g.ticker_on = 0;
    set(&mut g.action_code, "SUPP");
    if !env.skips(0x41ef3f) {
        turn::ticker_sim_steps(g, ui, rng, n, env.now);
    }
    set(&mut g.action_code, "");
}

/// `ActionMenuMode = 3`: miscellaneous functions (0x5550c1..0x555383).
fn mode_misc(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, mc: i32) {
    let tm = tsecs(env);
    let ae = g.active_entity;
    match mc {
        1 => {
            if !env.skips(0x46ef51)
                && let Some(cfg) = corp_finance::loans::borrow_from_bank(g, ui, rng, tm)
            {
                env.files.borrow_mut().write(b"CFIG.WSR", cfg);
            }
        }
        2 => {
            if 10 < ae {
                if !env.skips(0x426610) {
                    finance::calc_entity_financials(g, ui, rng, ae);
                }
                if Ext::from_f64(0.0) < g.liquid_funds && Ext::from_f64(0.0) < g.tmp_player_advances && !env.skips(0x46feda) {
                    corp_finance::loans::repay_bank_loan(g, ui, rng, tm, 2);
                }
            }
            if !env.skips(0x46feda) {
                corp_finance::loans::repay_bank_loan(g, ui, rng, tm, 1);
            }
        }
        3 => {
            if ae < 11 {
                if !env.skips(0x5bdaaf) {
                    corp_finance::lending::advance_funds_to_corp(g, ui, rng);
                }
            } else if !env.skips(0x5bc3ff) {
                corp_finance::bonds::stock_split(g, ui, 1);
            }
        }
        4 => {
            if ae < 11 {
                if !env.skips(0x5be3f5) {
                    corp_finance::lending::demand_advance_repayment(g, ui, rng);
                }
            } else if !env.skips(0x5bc3ff) {
                corp_finance::bonds::stock_split(g, ui, 2);
            }
        }
        5 => {
            if ae < 11 {
                if !env.skips(0x5c1974) {
                    corp_finance::loans::prepay_income_tax(g, ui, rng, tm);
                }
            } else if !env.skips(0x4715f3) {
                corp_finance::misc::change_company_name(g, ui, rng, tm);
            }
        }
        6 => {
            if !env.skips(0x6699ec) {
                assets::offers::offer_assets_for_sale(g, ui, rng, 1);
            }
        }
        7 => {
            if ae < 11 || !(130 < ae && ind(g, ae) <= 69) {
                if !env.skips(0x6828ca) { crate::logic::trade_screens::alerts_alloc::price_alerts::set_price_alert(g, ui, rng, env.now); }
            } else if !env.skips(0x6699ec) {
                assets::offers::offer_assets_for_sale(g, ui, rng, 2);
            }
        }
        8 => {
            if 130 < ae && ind(g, ae) <= 69 {
                if !env.skips(0x6828ca) { crate::logic::trade_screens::alerts_alloc::price_alerts::set_price_alert(g, ui, rng, env.now); }
            } else if !env.skips(0x69af92) {
                trading::bonds::trade_t_bills(g, ui);
            }
        }
        9 => {
            if !env.skips(0x69af92) {
                trading::bonds::trade_t_bills(g, ui);
            }
        }
        _ => {}
    }
}

/// `ActionMenuMode = 4`: buy/sell transactions (0x555388..0x556723). Returns true when the routine
/// continues at the common end 0x556723 ([`buy_sell_tail`]), false for an `ExitSub`.
fn mode_buy_sell(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, mc: i32) -> bool {
    let __content = crate::content::current();
    let tm = tsecs(env);
    let ae = g.active_entity;
    let mut mapped = mc;
    if ae < 131 {
        if 10 < ae && ind(g, ae) < 3 && 8 < mc && mc < 13 {
            mapped = mc - 3;
        }
    } else {
        if 2 < mc {
            mapped = mapped + 3;
        }
        if mc == 6 {
            mapped = 9;
        }
        if ind(g, ae) == 70 {
            if 6 < mc && mc < 12 {
                mapped = mc - 1;
            }
            if mc == 11 {
                mapped = 14;
            }
            if mc == 12 {
                mapped = 11;
            }
            if mc == 13 {
                mapped = 12;
            }
            if mc == 14 {
                mapped = 13;
            }
        } else {
            if mc == 7 {
                mapped = 10;
            }
            if 7 < mc && mc < 12 {
                mapped = mc - 2;
            }
            if mc == 12 {
                mapped = 14;
            }
            if mc == 13 {
                mapped = 11;
            }
            if mc == 14 {
                mapped = 12;
            }
            if mc == 15 {
                mapped = 13;
            }
        }
    }
    if 10 < ae && ae < 131 {
        if 2 < mc && mc < 6 {
            mapped = mc + 3;
        }
        if 5 < mc && mc < 9 {
            mapped = mc - 3;
        }
        if ind(g, ae) == 2 && mc == 13 {
            mapped = 11;
        }
    }
    if ae < 11 && mc == 10 {
        mapped = 14;
    }
    let pack = |g: &mut Globals, ui: &mut dyn Ui| {
        if !env.skips(0x49117c) {
            sim_entity::quarter_start::pack_holding_arrays(g, ui);
        }
    };
    let cp = g.cur_player;
    match mapped {
        1 => {
            let mut cover = false;
            if g.active_entity < 11 && g.skip_margin_prompt != "Y" {
                let n = if !env.skips(0x5df7c4) { crate::logic::shorts::find_short_positions(g, ui, rng, 0, cp) } else { 0 };
                cover = n == 3;
            }
            if cover {
                if !env.skips(0x5e2654) {
                    trading::short_menu::cover_short_position(g, ui, rng);
                }
                set(&mut g.action_code, "");
                if !env.skips(0x5df7c4) {
                    crate::logic::shorts::find_short_positions(g, ui, rng, 0, cp);
                }
                // (UI) <> 3: CONTROL SET TEXT 461, "&Buy Stock"
            } else {
                set(&mut g.skip_margin_prompt, "");
                g.tmp_ext1 = Ext::from_f64(0.0);
                if !env.skips(0x58d471) {
                    trading::buy_menu::buy_stock_menu(g, ui, rng, env.now);
                }
                set(&mut g.action_code, "");
                if !env.skips(0x481079) {
                    let a = g.active_entity;
                    reports_corp::check_cross_ownership(g, ui, rng, a, 1);
                }
            }
            pack(g, ui);
        }
        2 => {
            if g.active_entity < 11 && g.skip_margin_prompt != "Y" {
                if !env.skips(0x5df7c4) {
                    crate::logic::shorts::find_short_positions(g, ui, rng, 0, cp);
                }
                if !env.skips(0x5e0ea7) {
                    trading::short_menu::sell_short(g, ui, rng, tm);
                }
                set(&mut g.action_code, "");
                if !env.skips(0x5df7c4) {
                    crate::logic::shorts::find_short_positions(g, ui, rng, 0, cp);
                }
                // (UI) changed: CONTROL SET TEXT 461, "&Cover Short Position"
            } else {
                set(&mut g.skip_margin_prompt, "");
                if !env.skips(0x5ab449) {
                    trading::sell_stock(g, ui, rng, tm);
                }
                set(&mut g.action_code, "");
            }
            pack(g, ui);
        }
        3 | 4 => {
            let side = mapped - 2;
            g.player_recent_corp = 0;
            if !env.skips(0x5b6dbe) {
                trading::bonds::trade_corp_bonds(g, ui, rng, 1, side, tm);
            }
            if 10 < g.player_recent_corp {
                if !env.skips(0x556858) {
                    corp_finance::bonds::menu_cmd_place_bonds_with_bank(g, ui, rng);
                }
                if side == 2 {
                    g.takeover_candidate_id = cast::<i32, _>(g.player_recent_corp);
                }
            }
            if side == 2 {
                set(&mut g.action_code, "");
            }
            pack(g, ui);
        }
        5 => {
            if 130 < g.active_entity {
                if !env.skips(0x5b6cfe) {
                    trading::bonds::msg_non_financial_no_bond_trading(g, ui);
                }
                return false;
            }
            if !env.skips(0x5b94df) {
                super::dialogs::show_govt_bond_dialog(g, ui, rng, env);
            }
        }
        6 => {
            let a = g.active_entity;
            if a < 11 {
                buy_opts(g, ui, rng, env, 1);
            } else if mc != 7 && mc != 8 && mc != 9 {
                if !env.skips(0x5a8815) {
                    restructure::menus::stock_for_stock_merger(g, ui, rng, env.now);
                }
                if !env.skips(0x481079) {
                    let a = g.active_entity;
                    reports_corp::check_cross_ownership(g, ui, rng, a, 1);
                }
            } else if (ind(g, a) == 70 && mc == 7) || (ind(g, a) < 3 && mc == 9) {
                buy_opts(g, ui, rng, env, 1);
            } else if mc == 8 && 130 < a && ind(g, a) != 70 {
                buy_opts(g, ui, rng, env, 1);
            }
            pack(g, ui);
        }
        7 => {
            let a = g.active_entity;
            if a < 11 {
                sell_opts(g, ui, rng, env, 1);
            } else if mc != 8 && mc != 9 && mc != 10 {
                if !env.skips(0x59e01f) {
                    corp_finance::stock::stock_buyback_menu(g, ui, rng, env.now, 1);
                }
            } else if (ind(g, a) == 70 && mc == 8) || (mc == 10 && ind(g, a) < 3) {
                sell_opts(g, ui, rng, env, 1);
            } else if mc == 9 && 130 < a && ind(g, a) < 70 {
                sell_opts(g, ui, rng, env, 1);
            }
            pack(g, ui);
        }
        8 => {
            let a = g.active_entity;
            if a < 11 {
                buy_opts(g, ui, rng, env, 2);
                pack(g, ui);
            } else if mc != 9 && mc != 10 && mc != 11 {
                if !env.skips(0x59e01f) {
                    corp_finance::stock::stock_buyback_menu(g, ui, rng, env.now, 2);
                }
            } else if (ind(g, a) == 70 && mc == 9) || (mc == 11 && ind(g, a) < 3) {
                buy_opts(g, ui, rng, env, 2);
            } else if mc == 10 && 130 < a && ind(g, a) < 70 {
                buy_opts(g, ui, rng, env, 2);
            }
        }
        9 => {
            let a = g.active_entity;
            if a < 11 {
                sell_opts(g, ui, rng, env, 2);
                pack(g, ui);
                return false;
            }
            if mc == 10 || mc == 11 || mc == 12 {
                let sell = (ind(g, a) == 70 && mc == 10)
                    || (mc == 12 && ind(g, a) < 3)
                    || (mc == 11 && 130 < a && ind(g, a) < 70);
                if sell {
                    sell_opts(g, ui, rng, env, 2);
                    pack(g, ui);
                    return false;
                }
            }
            g.tmp_ext1 = Ext::from_f64(0.0);
            let prev_industry = ind(g, a);
            if !env.skips(0x5d2f04) {
                assets::buy::buy_business_assets(g, ui, rng, 0, env.now);
            }
            let a = g.active_entity;
            if ind(g, a) != prev_industry && g.turn_action_count < 5 {
                reopen_action_menu(g, ui, rng, env);
            }
        }
        10 => {
            let a = g.active_entity;
            if a < 11 {
                if !env.skips(0x5b8512) {
                    crate::logic::cashflow::msg_corp_only_function(g, ui);
                }
                return false;
            }
            let prev_industry = ind(g, a);
            if !env.skips(0x5d8ab9) {
                assets::sell::sell_business_assets(g, ui, rng, env.now);
            }
            let a = g.active_entity;
            if ind(g, a) != prev_industry && g.turn_action_count < 5 {
                reopen_action_menu(g, ui, rng, env);
            }
        }
        11 => {
            set(&mut g.commod_trade_mode, "FUTURES");
            let a = g.active_entity;
            if 10 < a && ind(g, a) == 2 {
                g.chart_entity = 0;
                g.tmp_ext1 = -Ext::from_f64(999.0);
            }
            if !env.skips(0x627a84) { crate::logic::trade_screens::commod_crypto::commod_desk::show_commodity_desk(g, ui, rng, 0); }
        }
        12 => {
            set(&mut g.commod_trade_mode, "PHYSICAL");
            if !env.skips(0x627a84) { crate::logic::trade_screens::commod_crypto::commod_desk::show_commodity_desk(g, ui, rng, 0); }
        }
        13 => {
            if !env.skips(0x6b4357) { crate::logic::trade_screens::commod_crypto::crypto::show_dlg_crypto(g, ui, rng, env.now, 0); }
        }
        14 => {
            let etf_id = if !env.skips(0x652b17) { trading::get_selected_player_etf(g, ui, rng) } else { 0 };
            if 1585 < etf_id && g.sel_entity == etf_id {
                let msg = g.ent_name[etf_id].clone()
                    + " ("
                    + &g.ent_symbol[etf_id]
                    + __content.string(3017)
                    + __content.string(10156)
                    + &g.ent_name[g.fund_adviser[etf_id]]
                    + ", but "
                    + __content.string(10157)
                    + __content.string(10158)
                    + __content.string(10159)
                    + &g.active_entity_name
                    + "."
                    + "\r"
                    + "\r"
                    + __content.string(3089)
                    + &g.active_entity_name
                    + __content.string(10160)
                    + __content.string(10161)
                    + &g.ent_name[etf_id]
                    + "?";
                let answer = ui.msgbox(&msg, 8483, &s(__content.string(10162)));
                if answer != id::YES {
                    return true;
                }
            }
            if !env.skips(0x60b44b) { crate::logic::trade_screens::option_station::station::show_advanced_options_station(g, ui, rng, env.now, 0); }
        }
        _ => return false, // CASE ELSE: ExecuteMenuCommand_ExitSub
    }
    true
}

/// `BuyOptions(optType)`.
fn buy_opts(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, opt_type: i32) {
    if !env.skips(0x5e340d) {
        trading::buy_options(g, ui, rng, opt_type, tsecs(env));
    }
}

/// `SellOptions(optType)`.
fn sell_opts(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, opt_type: i32) {
    if !env.skips(0x5e95d8) {
        trading::options_sell::sell_options(g, ui, rng, opt_type, tsecs(env));
    }
}

/// `TmpExt1 = ActionMenuMode: ShowActionMenu(0)`.
fn reopen_action_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    g.tmp_ext1 = cast::<Ext, _>(g.action_menu_mode);
    if !env.skips(0x4131ea) {
        super::action_menu::show_action_menu(g, ui, rng, env);
    }
}

/// The common end of the buy/sell menu (0x556723): when the menu was opened for a selected company the
/// player controls but another entity acted, the selected company becomes the acting entity again and
/// the menu is reopened (while transactions remain).
fn buy_sell_tail(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, saved_active: i32, saved_sel: i32) {
    if saved_active != saved_sel
        && 10 < saved_sel
        && g.cur_player == g.corp_controller[saved_sel]
        && g.turn_action_count < 5
    {
        g.active_entity = saved_sel;
        g.active_entity_name = g.ent_name[saved_sel].clone();
        if !env.skips(0x493e2b) {
            economy::close_all_dialogs_skipping(g, ui, rng, 1, env);
        }
        // (UI) label 112 "Transactions Remaining:  " + STR$(MAX(0, 5 - TurnActionCount)); DIALOG SHOW STATE
        reopen_action_menu(g, ui, rng, env);
    }
}

/// `ActionMenuMode = 5`: financing transactions (0x556ac6..0x556ec8).
fn mode_financing(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, mc: i32) {
    let __content = crate::content::current();
    let tm = tsecs(env);
    let corp_only = |g: &mut Globals, ui: &mut dyn Ui| -> bool {
        if g.active_entity < 11 {
            if !env.skips(0x5b8512) {
                crate::logic::cashflow::msg_corp_only_function(g, ui);
            }
            return true;
        }
        false
    };
    let etf = |g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng| -> i32 {
        if !env.skips(0x652b17) { trading::get_selected_player_etf(g, ui, rng) } else { 0 }
    };
    match mc {
        1 => {
            if !env.skips(0x5af020) {
                assets::startup::startup_company(g, ui, rng, tm);
            }
        }
        2 => {
            if !env.skips(0x596b50) {
                restructure::capital::contribute_capital_to_sub(g, ui, rng, tm);
            }
            if !env.skips(0x481079) {
                let a = g.active_entity;
                reports_corp::check_cross_ownership(g, ui, rng, a, 1);
            }
        }
        3 | 4 => {
            if !env.skips(0x5b0f50) {
                corp_finance::stock::stock_offering_menu(g, ui, rng, env.now, mc - 2);
            }
        }
        5 => {
            if 1580 < etf(g, ui, rng) {
                if !env.skips(0x5a40a2) {
                    corp_finance::bonds::issue_bonds_menu(g, ui, rng, tm);
                }
            } else {
                if corp_only(g, ui) {
                    return;
                }
                let a = g.active_entity;
                if g.entity_status[a] != 1 {
                    return;
                }
                if 0 < g.bonds_issued[a] && !env.skips(0x5a0aa1) {
                    corp_finance::bonds::bond_buyback_menu(g, ui, rng, 1, tm);
                }
                let a = g.active_entity;
                if g.bonds_issued[a] == 0 && !env.skips(0x5a40a2) {
                    corp_finance::bonds::issue_bonds_menu(g, ui, rng, tm);
                }
            }
        }
        6 => {
            if 1580 < etf(g, ui, rng) {
                if !env.skips(0x5a0aa1) {
                    corp_finance::bonds::bond_buyback_menu(g, ui, rng, 0, tm);
                }
            } else {
                if corp_only(g, ui) {
                    return;
                }
                let a = g.active_entity;
                if g.entity_status[a] != 1 {
                    return;
                }
                if g.bonds_issued[a] == 0 {
                    let n = g.active_entity_name.clone();
                    ui.msgbox(
                        &(n.clone() + __content.string(10163)),
                        8208,
                        &(n + __content.string(10164)),
                    );
                    return;
                }
                if !env.skips(0x5a0aa1) {
                    corp_finance::bonds::bond_buyback_menu(g, ui, rng, 0, tm);
                }
            }
        }
        7 => {
            if g.active_entity < 11 && etf(g, ui, rng) < 1581 {
                if !env.skips(0x5b8512) {
                    crate::logic::cashflow::msg_corp_only_function(g, ui);
                }
                return;
            }
            if !env.skips(0x5b3dae) {
                corp_finance::stock::extraordinary_dividend(g, ui, rng, env.now);
            }
        }
        8 | 9 => {
            if corp_only(g, ui) {
                return;
            }
            if !env.skips(0x5a7387) {
                restructure::menus::liquidate_subsidiary(g, ui, rng, mc - 7, env.now);
            }
            if !env.skips(0x49117c) {
                sim_entity::quarter_start::pack_holding_arrays(g, ui);
            }
        }
        10 => {
            if !env.skips(0x5bfb37) {
                let a = g.active_entity;
                corp_ai::spinoff::spin_off_subsidiary_menu(g, ui, rng, a, 0, tm);
            }
            if !env.skips(0x49117c) {
                sim_entity::quarter_start::pack_holding_arrays(g, ui);
            }
            if !env.skips(0x481079) {
                let a = g.active_entity;
                reports_corp::check_cross_ownership(g, ui, rng, a, 1);
            }
        }
        _ => {}
    }
}

/// `ActionMenuMode = 6`: management transactions (0x556ecd..0x5576d5).
fn mode_management(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, mc: i32) {
    let corp_only = |g: &mut Globals, ui: &mut dyn Ui| -> bool {
        if g.active_entity < 11 {
            if !env.skips(0x5b8512) {
                crate::logic::cashflow::msg_corp_only_function(g, ui);
            }
            return true;
        }
        false
    };
    let policy = |g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, code: &str| {
        if !env.skips(0x5acf48) {
            corp_finance::lending::change_corp_policy(g, ui, rng, &s(code));
        }
    };
    let autopilot = |g: &mut Globals, ui: &mut dyn Ui| {
        if !env.skips(0x5b65b9) {
            corp_finance::lending::toggle_auto_pilot(g, ui);
        }
    };
    let etf_adviser = |g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng| {
        let a = g.active_entity;
        let n = if !env.skips(0x64d4fe) { earnings::get_funds_advised_by(g, a) } else { 0 };
        if n == 0 {
            if !env.skips(0x6501b3) {
                corp_finance::misc::seek_become_etf_adviser(g, ui, rng);
            }
        } else if !env.skips(0x5c1dae) {
            corp_finance::misc::set_etf_advisory_fee(g, ui);
        }
    };
    match mc {
        1 => {
            if !env.skips(0x59ca81) {
                corp_finance::misc::become_or_resign_ceo(g, ui, rng);
            }
        }
        2 => resign_as_ceo(g, ui, env),
        3 => {
            if !env.skips(0x4844cb) {
                let a = g.active_entity;
                crate::logic::ai_player::fire_management(g, ui, rng, a);
            }
        }
        4 => policy(g, ui, rng, "DV"),
        5 => {
            let a = g.active_entity;
            if 10 < a && ind(g, a) == 70 {
                autopilot(g, ui);
            } else {
                policy(g, ui, rng, "RD");
            }
        }
        6 => {
            if corp_only(g, ui) {
                return;
            }
            policy(g, ui, rng, "GR");
        }
        7 => {
            if corp_only(g, ui) {
                return;
            }
            let a = g.active_entity;
            if ind(g, a) == 1 {
                if !env.skips(0x69f0a2) { crate::logic::trade_screens::alerts_alloc::bank_alloc::show_dlg_bank_asset_alloc(g, ui, rng, 0); }
            } else if ind(g, a) == 2 {
                etf_adviser(g, ui, rng);
            } else if !env.skips(0x5aa186) {
                corp_finance::stock::restructuring_write_off(g, ui, rng, env.now);
            }
        }
        8 => {
            if corp_only(g, ui) {
                return;
            }
            let a = g.active_entity;
            if ind(g, a) < 3 {
                autopilot(g, ui);
            } else if !env.skips(0x588f0f) {
                corp_finance::misc::file_antitrust_suit(g, ui, rng);
            }
        }
        9 => autopilot(g, ui),
        10 => etf_adviser(g, ui, rng),
        _ => {}
    }
}

/// Management command 2, "Resign As CEO" (0x556f0d..0x5573e4).
fn resign_as_ceo(g: &mut Globals, ui: &mut dyn Ui, env: &Env) {
    let __content = crate::content::current();
    let cp = g.cur_player;
    if g.player_ceo_of[cp] < 11 {
        let mut m = s(__content.string(10165));
        if !env.skips(0x46e851) {
            show_quick_message(g, ui, &mut m);
        }
        return;
    }
    let ceo = g.player_ceo_of[cp];
    let r = if !env.skips(0x5f1dae) {
        crate::logic::ai_player::check_ceo_option_forfeiture(g, ui, cp, ceo, 0)
    } else {
        0
    };
    if r == 1 {
        return;
    }
    if 0 < g.player_ceo_since_year[cp] && g.player_ceo_since_year[cp] < g.cur_year {
        let mut msg = s(__content.string(10166))
            + __content.string(10167)
            + &g.ent_symbol[ceo]
            + __content.string(1069)
            + __content.string(10168)
            + "\r"
            + "\r";
        if g.cur_player != g.corp_controller[ceo] {
            msg = msg
                + __content.string(10169)
                + "of "
                + &g.ent_name[ceo]
                + __content.string(10170)
                + "\r"
                + "\r";
        }
        msg = msg + __content.string(10171);
        if ui.msgbox(&msg, 8484, &s(__content.string(10172))) != id::YES {
            return;
        }
    }
    let msg = g.cur_player_name.clone()
        + __content.string(10173)
        + &g.ent_name[g.player_ceo_of[cp]]
        + __content.string(10174)
        + __content.string(10175);
    ui.msgbox(&msg, 8256, &s(__content.string(10176)));
    g.news_msg = g.ent_symbol[cp].clone() + __content.string(10177) + &g.ent_symbol[g.player_ceo_of[cp]];
    if !env.skips(0x41e7d5) {
        post_news(g, ui, 0, 0);
    }
    g.turn_action_count = g.turn_action_count + 1;
    g.player_ceo_of[cp] = 0;
    g.player_ceo_since_year[cp] = 0;
    // (UI) CONTROL SET TEXT 462, "Resig&n As CEO"
    if !env.skips(0x5fd937) {
        sim_entity::quarter_start::revoke_ex_ceo_options(g, ui);
    }
    g.auto_pilot[ceo] = g.auto_pilot[cp];
    let state = if g.auto_pilot[ceo] == 1 { s("OFF") } else { s("ON") };
    if !env.skips(0x5b6b90) {
        corp_finance::lending::set_auto_pilot_menu_text(g, ceo, &state);
    }
}

/// `ActionMenuMode = 7`: other transactions (0x5576da..0x557ac8).
fn mode_other(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, mc: i32) {
    let __content = crate::content::current();
    let ae = g.active_entity;
    let cross = |g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng| {
        if !env.skips(0x481079) {
            let a = g.active_entity;
            reports_corp::check_cross_ownership(g, ui, rng, a, 1);
        }
    };
    let sale_offers = |g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng| {
        if !env.skips(0x666a27) {
            assets::offers::show_asset_sale_offers(g, ui, rng, env.now);
        }
        cross(g, ui, rng);
    };
    let reserve = |g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, m: i32| {
        if !env.skips(0x59fcdd) {
            corp_finance::stock::adjust_contingency_reserve(g, ui, rng, m);
        }
    };
    let loans = |g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, t: i32| {
        if !env.skips(0x58a220) {
            corp_finance::loans::buy_sell_loans(g, ui, rng, t);
        }
    };
    let swaps = |g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng| {
        if !env.skips(0x63b281) { crate::logic::trade_screens::ir_swaps::show_ir_swap_dialog(g, ui, rng, 0); }
    };
    match mc {
        1 => {
            if !env.skips(0x59e9e5) {
                corp_finance::misc::harassing_lawsuit(g, ui, rng);
            }
        }
        2 => {
            if 0 < g.rumor_victim_id && g.player_notoriety[g.cur_player] < 7 {
                let mut m = s(__content.string(10178))
                    + __content.string(10179);
                if !env.skips(0x46e851) {
                    show_quick_message(g, ui, &mut m);
                }
                return;
            }
            if !env.skips(0x5aac8f) {
                corp_finance::misc::spread_rumors(g, ui, rng);
            }
        }
        3 => {
            if !env.skips(0x600b42) {
                trading::options_sell::exercise_options_menu(g, ui, rng, 1);
            }
            cross(g, ui, rng);
        }
        4 => {
            if !env.skips(0x600b42) {
                trading::options_sell::exercise_options_menu(g, ui, rng, 2);
            }
        }
        5 => {
            if ae < 11 || 85 < ae {
                if !env.skips(0x59bd8a) {
                    corp_finance::loans::change_lending_bank(g, ui, rng);
                }
            } else if !env.skips(0x595994) {
                corp_finance::lending::call_in_loan(g, ui, rng);
            }
        }
        6 => {
            if 85 < ae {
                reserve(g, ui, rng, 6);
            } else if ae < 11 {
                swaps(g, ui, rng);
            } else {
                loans(g, ui, rng, 0);
            }
        }
        7 => {
            if 85 < ae {
                reserve(g, ui, rng, 7);
            } else if ae < 11 {
                sale_offers(g, ui, rng);
            } else if !env.skips(0x59d28a) {
                corp_finance::lending::freeze_credit_line(g, ui, rng);
            }
        }
        8 => {
            if 10 < ae && ae < 86 {
                reserve(g, ui, rng, 6);
            } else if 85 < ae {
                swaps(g, ui, rng);
            }
        }
        9 => {
            if 10 < ae && ae < 86 {
                swaps(g, ui, rng);
            } else if 85 < ae {
                sale_offers(g, ui, rng);
            }
        }
        10 => {
            if 10 < ae && ae < 86 {
                sale_offers(g, ui, rng);
            } else if 85 < ae && ae < 131 {
                loans(g, ui, rng, 4);
            }
        }
        11 => loans(g, ui, rng, 8),
        _ => {}
    }
}

/// Port of DoTransactionMenuAction (0x552f0b): the click on action-menu button `btn_num`
/// (`ActionMenu_Btn1..15`). A computer player's turn (current player or acting player is a computer
/// player) is ended instead; after the turn's last transaction the click only advances the ticker.
/// Otherwise runs [`execute_menu_command`] and the bookkeeping after it: holdings packed, market display,
/// for transaction menus (mode > 2) the control check of the acting entity, the advice text
/// (`BuildMenuAdviceText`) and a batch of suppressed ticker steps, or — once the turn's
/// transactions are used up — `TurnActionCount = 7`, the ticker stop (two-player mode) and a ticker step.
/// Window plumbing (showing/hiding dialogs, enabling buttons) is skipped.
pub fn do_transaction_menu_action(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, btn_num: i32) {
    let tm = tsecs(env);
    let mut computer = false;
    if 0 < g.cur_player && g.is_computer_player[g.cur_player] == 1 {
        computer = true;
    }
    if 0 < g.active_entity && g.active_entity <= g.num_players && g.is_computer_player[g.active_entity] == 1 {
        computer = true;
    }
    let market = |g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng| {
        if !env.skips(0x4400d7) {
            crate::logic::market::update_market_display(g, ui, rng, tm);
        }
    };
    if computer {
        if g.game_end_flag != "Y" {
            g.turn_action_count = 5;
            market(g, ui, rng);
            set(&mut g.ticker_stop_req, "Y");
            if !env.skips(0x424fd6) {
                turn::end_turn(g, ui, rng, env.now);
            }
            if g.game_end_flag != "Y" {
                set(&mut g.ticker_stop_req, "N");
            }
            return;
        }
    } else if 4 < g.turn_action_count && g.action_code != "AUTO" && g.game_end_flag != "Y" {
        // (UI) DIALOG END hDlgTransMenu
        market(g, ui, rng);
        if !env.skips(0x41ef3f) {
            turn::ticker_sim_steps(g, ui, rng, 1, env.now);
        }
        return;
    }
    // (UI) DIALOG END DlgWhoOwnsWhat
    let saved_ind = g.sel_industry;
    let saved_active = g.active_entity;
    if 0 < g.active_entity {
        g.active_entity_name = g.ent_name[g.active_entity].clone();
    }
    if 2 < g.action_menu_mode {
        if !env.skips(0x4242c6) {
            restructure::merge::disable_main_controls(g);
        }
        if !env.skips(0x4245e3) {
            super::action_menu::disable_action_menu_controls(g);
        }
    }
    if !env.skips(0x5e319d) {
        etf_derivs::corp_options::force_insider_short_cover(g, ui, rng);
    }
    // (UI) DIALOG SHOW STATE hDlgTransMenu, 0; mode > 2: hDlgMain hidden, DIALOG END hChartDlg (mode 3:
    // hReportDlg)
    if !env.skips(0x553664) {
        execute_menu_command(g, ui, rng, env, btn_num);
    }
    // (UI) DIALOG SHOW STATE hDlgMain / hDlgTransMenu, 5
    if !env.skips(0x49117c) {
        sim_entity::quarter_start::pack_holding_arrays(g, ui);
    }
    if g.sel_industry != saved_ind && 10 < g.active_entity && !env.skips(0x46df33) {
        trading::select_industry_combo_item(g);
    }
    market(g, ui, rng);
    if g.action_menu_mode <= 2 {
        if 0 < g.info_dlg_reopen {
            // (UI) DIALOG END hDlgTransMenu
            g.action_menu_open = 0;
        }
        return;
    }
    set(&mut g.help_text, "");
    // (UI) CONTROL SET TEXT 460, HelpText
    if 10 < g.active_entity && !env.skips(0x41f71b) {
        let a = g.active_entity;
        control::update_corp_control(g, ui, rng, a);
    }
    let a = g.active_entity;
    if g.entity_status[a] == 1 && g.cur_player == g.corp_controller[a] && saved_active == a {
        // (UI) DIALOG SHOW STATE hDlgMain, 5; CONTROL ENABLE 461..478, 1; focus
    } else if !env.skips(0x493e2b) {
        economy::close_all_dialogs_skipping(g, ui, rng, 1, env);
    }
    if !env.skips(0x424754) {
        turn::enable_main_controls(g);
    }
    if !env.skips(0x46e001) {
        turn::pause_ticker(g, &ui.local_time(), 0.0f32);
    }
    if g.turn_action_count < 5 {
        if !env.skips(0x557bbf) { crate::logic::advice::build_menu_advice_text(g, ui, rng, env.now.second); }
        // (UI) CONTROL SET TEXT 460, HelpText
        if g.clock_state != "RUN" && 85 < g.sim_co_count && g.sim_co_count < 1580 {
            suppressed_ticker_steps(g, ui, rng, env, 5);
        }
    } else {
        if g.solo_human_mode == 1 && !env.skips(0x493e2b) {
            economy::close_all_dialogs_skipping(g, ui, rng, 1, env);
        }
        g.turn_action_count = 7;
        if g.solo_human_mode == 0 {
            set(&mut g.clock_state, "STOP");
            g.ticker_on = 0;
        }
        if g.game_end_flag != "Y" {
            if g.clock_state != "RUN" {
                g.ticker_pause_until = 0;
                set(&mut g.clock_state, "");
                g.ticker_on = 0;
                set(&mut g.action_code, "SUPP");
                set(&mut g.action_code, "");
            }
            if !env.skips(0x41ef3f) {
                turn::ticker_sim_steps(g, ui, rng, 1, env.now);
            }
        }
        market(g, ui, rng);
    }
}

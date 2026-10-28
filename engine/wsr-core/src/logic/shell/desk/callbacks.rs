//! Trading desk control callbacks. The host supplies the control id and Win32 notification code.
use crate::pb::real::{Ext, cast};


use super::ctl;
use super::super::prelude::*;
use crate::logic::{economy, info_screens, market, reports_corp, reports_lists, trading, turn};

fn close(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if !env.skips(0x493e2b) { economy::close_all_dialogs_skipping(g, ui, rng, 1, env); }
}
fn cross(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, entity: i32) -> i32 {
    if env.skips(0x481079) { 0 } else { reports_corp::check_cross_ownership(g, ui, rng, entity, 1) }
}
fn menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, mode: i32) {
    g.tmp_ext1 = cast::<Ext, _>(mode);
    if !env.skips(0x4131ea) { super::super::action_menu::show_action_menu(g, ui, rng, env); }
}
fn select(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, mode: i32) {
    if !env.skips(0x4721dc) { trading::select::select_acting_entity(g, ui, rng, mode, env.now); }
}
fn target(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if !env.skips(0x4227d5) { market::select_target_corp(g, ui, rng, env.now.second); }
}
fn sim(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, steps: i32) {
    g.ticker_pause_until = 0;
    set(&mut g.clock_state, "");
    g.ticker_on = 0;
    set(&mut g.action_code, "SUPP");
    if !env.skips(0x41ef3f) { turn::ticker_sim_steps_skipping(g, ui, rng, steps, env.now, env.skip); }
    set(&mut g.action_code, "");
}
fn chart(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, entity: i32) {
    g.chart_entity = entity;
    if !env.skips(0x6141b1) { info_screens::chart::show_price_chart(g, ui, rng, env.now); }
}

/// Dispatch a trading desk control notification. Text entry and combo selection have separate
/// host methods [`desk_go`] and [`desk_industry_selection`], since their values belong to the UI.
pub fn desk_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, control: i32, notify: i32) {
    let __content = crate::content::current();
    if control == ctl::LOGO {
        if notify == 1 && !env.skips(0x67383a) { info_screens::cheat::cheat_menu(g, ui, rng, env.now.second); }
        return;
    }
    if control == ctl::SYMBOL_BOX {
        if notify == 256 { close(g, ui, rng, env); }
        return;
    }
    if notify != 0 { return; }
    match control {
        ctl::GO => {
            // The host may set TmpSymbol before clicking, or call desk_go with the input explicitly.
            let value = g.tmp_symbol.clone();
            desk_go(g, ui, rng, env, &value);
        }
        ctl::ENTITY_INFO | ctl::GENERAL => {
            let entity = g.sel_entity;
            cross(g, ui, rng, env, entity);
            g.info_dlg_reopen = if control == ctl::ENTITY_INFO { 1 } else { 2 };
            while if control == ctl::ENTITY_INFO { g.info_dlg_reopen == 1 } else { g.info_dlg_reopen > 0 } {
                let mode = g.info_dlg_reopen;
                g.info_dlg_reopen = 0;
                menu(g, ui, rng, env, cast::<i32, _>(mode));
            }
        }
        ctl::MISC_MENU => menu(g, ui, rng, env, 3),
        ctl::BUY_STOCK | ctl::SELL_STOCK | ctl::BUY_SELL_MENU | ctl::FINANCING_MENU | ctl::MANAGEMENT_MENU | ctl::OTHER_TRANS_MENU => {
            let selected = g.sel_entity;
            let active = g.active_entity;
            if cross(g, ui, rng, env, selected) > 0 || cross(g, ui, rng, env, active) > 0 { return; }
            if control == ctl::BUY_STOCK || control == ctl::SELL_STOCK {
                g.action_menu_mode = 4;
                if active < 11 { set(&mut g.skip_margin_prompt, "Y"); }
                close(g, ui, rng, env);
                if !env.skips(0x552f0b) {
                    super::super::menu::do_transaction_menu_action(g, ui, rng, env, if control == ctl::BUY_STOCK { 1 } else { 2 });
                }
            } else {
                let mode = match control { ctl::BUY_SELL_MENU => 4, ctl::FINANCING_MENU => 5, ctl::MANAGEMENT_MENU => 6, _ => 7 };
                menu(g, ui, rng, env, cast::<i32, _>(mode));
            }
        }
        ctl::SELECT_PLAYER | ctl::SELECT_CORP | ctl::MY_CORPORATIONS => {
            close(g, ui, rng, env);
            let mode = match control { ctl::SELECT_PLAYER => 1, ctl::SELECT_CORP => 3, _ => 2 };
            select(g, ui, rng, env, mode);
            if control == ctl::SELECT_CORP && g.clock_state != "RUN" && g.sim_co_count < 1580 { sim(g, ui, rng, env, 5); }
            if control == ctl::MY_CORPORATIONS {
                if g.clock_state == "RUN" || g.sim_co_count > 1589 { turn::pause_ticker(g, &ui.local_time(), 0.0f32); } else { sim(g, ui, rng, env, 4); }
            }
        }
        ctl::SELECT_LAST => {
            if g.sel_entity != g.last_sel_entity {
                g.symbol_dlg_open = 0;
                let prev = g.sel_entity;
                let last = g.last_sel_entity;
                if last < 11 { g.last_sel_entity = prev; }
                if g.last_sel_entity > 10 && g.entity_status[g.last_sel_entity] != 1 {
                    if last > 10 && g.entity_status[last] != 1 && !env.skips(0x480a70) { crate::logic::report::describe_defunct_entity(g, ui, last); }
                    g.last_sel_entity = 0;
                    return;
                }
                if g.sel_entity != g.last_sel_entity {
                    close(g, ui, rng, env);
                    g.target_corp = g.last_sel_entity;
                    g.tmp_symbol = g.ent_symbol[g.last_sel_entity].clone();
                    target(g, ui, rng, env);
                    if prev > 10 { g.last_sel_entity = prev; }
                }
            }
        }
        ctl::CONTROLLER_LABEL => {
            let selected = g.sel_entity;
            cross(g, ui, rng, env, selected);
            let controller = g.corp_controller[selected];
            if selected > 10 && controller > 10 {
                close(g, ui, rng, env); g.target_corp = controller; target(g, ui, rng, env);
            } else if selected > 10 && controller > 0 && g.game_end_flag == "Y" {
                close(g, ui, rng, env); select(g, ui, rng, env, -controller);
            }
        }
        ctl::ACTING_LABEL => {
            if g.sel_entity != g.active_entity {
                let active = g.active_entity;
                if active < 11 && active == g.cur_player { close(g, ui, rng, env); select(g, ui, rng, env, 0); }
                else if active > 10 { close(g, ui, rng, env); g.target_corp = active; target(g, ui, rng, env); }
            }
        }
        ctl::STREAM_ADD_DELETE => {
            if !env.skips(0x5dda89) { market::toggle_streaming_quote(g, ui, rng, 0, 0); }
            close(g, ui, rng, env);
        }
        ctl::STREAM_CLEAR => { close(g, ui, rng, env); if !env.skips(0x5dea80) { info_screens::holders::clear_streaming_quotes_list(g, ui); } }
        ctl::STREAM_FILL => { close(g, ui, rng, env); if !env.skips(0x5dec84) { info_screens::holders::fill_streaming_list_with_holdings(g, ui, rng); } }
        ctl::MY_NEWS => {
            close(g, ui, rng, env);
            if !env.skips(0x49d62b) { reports_lists::show_my_headlines(g, ui, rng); }
            if g.clock_state == "RUN" || g.sim_co_count > 1589 { turn::pause_ticker(g, &ui.local_time(), 0.0f32); } else { sim(g, ui, rng, env, 9); }
        }
        ctl::END_TURN => {
            close(g, ui, rng, env); set(&mut g.ticker_stop_req, "Y");
            if !env.skips(0x424fd6) { turn::end_turn_skipping(g, ui, rng, env.now, env.skip); }
            set(&mut g.ticker_stop_req, "N");
            if g.clock_state != "RUN" && g.sim_co_count < 1570 { sim(g, ui, rng, env, 20); }
        }
        ctl::TICKER_ON_OFF => {
            g.ticker_on = i32::from(g.ticker_on != 1);
            if g.ticker_on == 0 {
                set(&mut g.clock_state, "STOP");
                if !env.skips(0x41e749) { turn::refresh_news_listbox(g); }
            } else {
                g.ticker_pause_until = 0; set(&mut g.clock_state, "RUN");
                if !env.skips(0x41d9c2) { super::super::app::run_ticker_loop(g, ui, rng, env); }
            }
        }
        ctl::CHART => { let entity = g.sel_entity; if !env.skips(0x40fcff) { super::super::dialogs::show_stock_chart(g, ui, rng, env, entity); } }
        ctl::CHEAT => if !env.skips(0x67383a) { info_screens::cheat::cheat_menu(g, ui, rng, env.now.second); },
        ctl::CHART_STOCK_INDEX => chart(g, ui, rng, env, 0),
        ctl::CHART_CRUDE => chart(g, ui, rng, env, 6),
        ctl::CHART_GOLD => chart(g, ui, rng, env, 7),
        ctl::CHART_SILVER => chart(g, ui, rng, env, 8),
        ctl::CHART_WHEAT => chart(g, ui, rng, env, 9),
        ctl::CHART_CORN => chart(g, ui, rng, env, 10),
        ctl::NET_WORTH => { let p = g.cur_player; chart(g, ui, rng, env, p); }
        ctl::CHART_PRIME_RATE => chart(g, ui, rng, env, 1601),
        ctl::CHART_LONG_BOND => chart(g, ui, rng, env, 1602),
        ctl::CHART_SHORT_BOND => chart(g, ui, rng, env, 1603),
        ctl::CHART_GDP_GROWTH => chart(g, ui, rng, env, 1604),
        ctl::EARNINGS_REPORT | ctl::RESEARCH_REPORT | ctl::LIST_SHAREHOLDERS => {
            close(g, ui, rng, env);
            let mut entity = g.sel_entity;
            if entity < 11 && g.last_sel_entity > 10 && g.entity_status[g.last_sel_entity] == 1 { entity = g.last_sel_entity; }
            if control == ctl::LIST_SHAREHOLDERS {
                set(&mut g.entity_pick_mode, __content.string(6093));
                if !env.skips(0x421544) { crate::logic::report::show_shareholder_list(g, ui, rng, entity, 0); }
            } else {
                set(&mut g.entity_pick_mode, "");
                if control == ctl::EARNINGS_REPORT && !env.skips(0x462f1e) { reports_corp::show_earnings_report(g, ui, rng, entity); }
                if control == ctl::RESEARCH_REPORT && !env.skips(0x448ed3) { crate::logic::research::show_research_report(g, ui, rng, entity); }
            }
        }
        ctl::LIST_OPTIONS => { close(g, ui, rng, env); let entity = g.sel_entity; if !env.skips(0x5f51fc) { reports_lists::show_option_positions(g, ui, rng, entity, 0, 0); } }
        ctl::RECALL_DB_SEARCH => {
            g.dlg_open_who_owns_what = 0;
            let previous = g.sel_entity;
            close(g, ui, rng, env);
            let mut dlg = info_screens::db_search::DbSearchDlg::default();
            let mut mode = 3;
            let result = if env.skips(0x54b072) { 0 } else { info_screens::db_search::run_db_search(g, ui, rng, &mut dlg, &mut mode) };
            if result == 0 && !env.skips(0x5472ee) { info_screens::db_search::show_db_search_dialog(g, ui, rng); }
            if g.sel_entity != previous { menu(g, ui, rng, env, 1); }
            else if g.clock_state == "RUN" || g.sim_co_count > 1589 { turn::pause_ticker(g, &ui.local_time(), 0.0f32); }
            else { sim(g, ui, rng, env, 0); }
        }
        ctl::LIST_FUTURES => {
            g.dlg_open_who_owns_what = 0;
            if g.turn_action_count > 4 && g.action_code != "AUTO" && g.game_end_flag != "Y" {
                if !env.skips(0x4400d7) { market::update_market_display(g, ui, rng, env.now.second); }
                if !env.skips(0x41ef3f) { turn::ticker_sim_steps_skipping(g, ui, rng, 0, env.now, env.skip); }
                return;
            }
            let p = g.cur_player;
            let count = if p != g.sel_entity { -1 } else if env.skips(0x633d45) { 0 } else { crate::logic::futures::count_futures_positions(g, p, 0) };
            if p == g.sel_entity && count == 0 {
                let phys = if env.skips(0x65b912) { Ext::from_f64(0.0) } else { crate::logic::finance::get_phys_commod_value(g, p, &s("")) };
                if phys > Ext::from_f64(0.0) {
                    let msg = s(__content.string(10130));
                    if ui.msgbox(&msg, 8227, &s(__content.string(8539))) == id::YES {
                        let idx = if env.skips(0x65c08f) { 0 } else { reports_lists::list_phys_commod_holdings(g, ui, rng, p, 0) };
                        if g.game_end_flag != "Y" && idx > 0 && cast::<Ext, _>(idx) <= val(g.phys_commod_rec[0].as_bytes()) {
                            let rec = g.phys_commod_rec[idx].clone();
                            if val(parse_any(rec.as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(p) {
                                g.action_menu_open = 0;
                                g.cur_commodity = trim(parse_any(rec.as_bytes(), b"|", 1).as_bytes());
                                if !env.skips(0x65db62) { crate::logic::commod_ai::physical::sell_phys_commod(g, ui, rng, p, idx, 0, 0.0f32, 0); }
                                if !env.skips(0x424754) { turn::enable_main_controls(g); }
                            }
                        }
                    }
                    return;
                }
            }
            let entity = g.sel_entity;
            let idx = if env.skips(0x62cbaf) { 0 } else { reports_lists::show_futures_positions(g, ui, rng, entity, 1) };
            if g.game_end_flag != "Y" && idx > 0 && cast::<Ext, _>(idx) <= val(g.futures_recs[0].as_bytes()) {
                let rec = g.futures_recs[idx].clone();
                let active = g.active_entity;
                if val(parse_any(rec.as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(active) {
                    g.action_menu_open = 0;
                    close(g, ui, rng, env);
                    g.cur_commodity = trim(parse_any(rec.as_bytes(), b"|", 1).as_bytes());
                    if val(parse_any(rec.as_bytes(), b"|", 3).as_bytes()) >= Ext::from_f64(0.0) {
                        if !env.skips(0x620f39) { crate::logic::commod_ai::desk::futures_sell_or_short(g, ui, rng, 2, active, idx); }
                    } else if !env.skips(0x61cebb) { crate::logic::commod_ai::desk::futures_buy_or_cover(g, ui, rng, 2, active, idx); }
                    if !env.skips(0x424754) { turn::enable_main_controls(g); }
                }
            }
        }
        ctl::LIST_PORTFOLIO => {
            close(g, ui, rng, env);
            let entity = g.sel_entity;
            if !env.skips(0x426610) { crate::logic::finance::calc_entity_financials(g, ui, rng, entity); }
            g.tmp_ext1 = Ext::from_f64(3.0);
            set(&mut g.entity_pick_mode, __content.string(6093));
            let has_options = if env.skips(0x5f0cb9) { 0 } else { crate::logic::control::find_option_positions(g, ui, entity, 0, 0, 0) };
            if has_options == 0 && g.hold_stock_id[(0, entity)] == 0 && g.tmp_gov_bonds_value == Ext::from_f64(0.0)
                && g.tmp_corp_bonds_value == Ext::from_f64(0.0) && entity > 10 {
                let p = g.cur_player;
                let player_options = if env.skips(0x5f0cb9) { 0 } else { crate::logic::control::find_option_positions(g, ui, p, 0, 0, 0) };
                if player_options != 0 || g.hold_stock_id[(0, p)] != 0 || g.hold_bond_id[(0, p)] != 0 {
                    let msg = g.ent_name[entity].clone() + __content.string(10131);
                    if ui.msgbox(&msg, 8484, &s(__content.string(10132))) == id::YES && !env.skips(0x4422a4) {
                        reports_lists::show_portfolio_listing(g, ui, rng, p, 0, 0);
                    }
                    return;
                }
            }
            if !env.skips(0x4422a4) { reports_lists::show_portfolio_listing(g, ui, rng, entity, 0, 0); }
        }
        ctl::STREAM_QUOTE_FIRST..=ctl::STREAM_QUOTE_LAST => {
            g.target_corp = g.watch_list[(control - 800, g.cur_player)];
            if g.target_corp != 0 && g.sel_entity != g.target_corp { close(g, ui, rng, env); target(g, ui, rng, env); }
        }
        ctl::FINANCIAL_PROFILE => {
            close(g, ui, rng, env); set(&mut g.entity_pick_mode, "");
            let entity = g.sel_entity;
            if entity < 11 { if !env.skips(0x43987e) { reports_lists::show_personal_financial_statement(g, ui, rng, 0); } }
            else if !env.skips(0x430898) { reports_corp::show_balance_sheet(g, ui, rng, entity); }
        }

        _ => {}
    }
}

/// Process the symbol box's Go button using the text supplied by the host (CbGoButton, 0x411a45).
pub fn desk_go(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, text: &PbStr) {
    close(g, ui, rng, env);
    set(&mut g.tmp_symbol, "");
    if text.is_empty() && g.sel_entity > 10 { return; }
    let player = to_long(val(text.as_bytes()));
    if g.game_end_flag == "Y" && player > 0 && player <= g.num_players {
        select(g, ui, rng, env, -player);
    } else {
        if !env.skips(0x422b3b) { trading::select::lookup_symbol(g, ui, rng, text); }
        if !g.tmp_symbol.is_empty() && g.sel_entity != g.target_corp {
            target(g, ui, rng, env);
            close(g, ui, rng, env);
            if g.sel_entity > 10 && g.game_end_flag != "Y" && g.auto_add_stream_list == 1 && !env.skips(0x5dda89) {
                market::toggle_streaming_quote(g, ui, rng, 1, 0);
            }
        }
    }
}

/// Process a chosen industry combo caption (CbIndustryCombo, 0x411cf0).
pub fn desk_industry_selection(g: &mut Globals, caption: &PbStr, notify: i32) {
    match notify {
        9 => g.sel_industry = to_long(val(right(caption.as_bytes(), 2).as_bytes())),
        10 => trading::select::select_industry_combo_item(g),
        _ => {}
    }
}

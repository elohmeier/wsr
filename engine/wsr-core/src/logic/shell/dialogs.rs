//! Dialogs opened by shell commands, represented as host choices.
use crate::pb::real::{Ext, cast};


use super::prelude::*;

/// Government bond trading desk. A choice selects the transaction and executes its Go action.
pub fn show_govt_bond_dialog(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.dlg_open_gov_bond_desk = 1;
    let long = mid(g.long_bond_desc.as_bytes(), 1, 4);
    let short = mid(g.short_bond_desc.as_bytes(), 1, 4);
    let options = [s(__content.string(10133)) + &long, s(__content.string(10134)) + &long,
        s(__content.string(10135)) + &short, s(__content.string(10136)) + &short];
    let mut selected = None;
    while let Some(choice) = ui.select(&s(__content.string(10137)), &[], &options, selected, &s("Go")) {
        selected = Some(choice);
        g.tmp_ext1 = cast::<Ext, _>(choice + 1);
        if g.sim_co_count != g.popup_req_co_cnt { g.gov_bond_desk_action = 0; }
        g.popup_req_co_cnt = g.sim_co_count;
        g.gov_bond_desk_action = cast::<i32, _>(choice + 1);
        if !env.skips(0x5ba0ee) {
            crate::logic::trade_screens::option_misc::trade_govt_bonds(g, ui, rng, cast::<i32, _>(choice + 1));
        }
        if g.turn_action_count >= 5 { break; }
    }
    g.dlg_open_gov_bond_desk = 0;
}

/// Select and display a position-holder list.
pub fn show_who_owns_what_dialog(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    g.tmp_ext1 = Ext::from_f64(0.0);
    if !env.skips(0x49117c) { crate::logic::sim_entity::quarter_start::pack_holding_arrays(g, ui); }
    g.dlg_open_who_owns_what = 1;
    let options = [s(__content.string(10138)), s(__content.string(10139)), s("Interest rate swaps"), s(__content.string(10140)), s("Stock cross-holdings"), s(__content.string(10141))];
    let mut selected_choice = Some(0);
    while let Some(choice) = ui.select(&s(__content.string(10142)), &[], &options, selected_choice, &s("Go")) {
        selected_choice = Some(choice);
        let selected = g.sel_entity;
        g.tmp_ext1 = Ext::from_f64(0.0);
        use crate::logic::info_screens::holders;
        match choice {
            0 | 1 => holders::list_futures_or_phys_holders(g, ui, rng, cast::<i32, _>(choice + 1)),
            2 => holders::list_swap_holders(g, ui, rng),
            3 => holders::list_option_holders(g, ui, rng),
            4 => holders::list_stock_cross_holdings(g, ui, rng),
            5 => holders::list_etf_advisers(g, ui, rng),
            _ => break,
        }
        g.tmp_ext1 = cast::<Ext, _>(choice + 1);
        if g.sel_entity != selected { break; }
    }
    g.dlg_open_who_owns_what = 0;
    g.tmp_ext1 = Ext::from_f64(0.0);
}

/// Select an entity and show its stock chart if sufficient history is available.
pub fn show_stock_chart(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, entity_id: i32) {
    let __content = crate::content::current();
    g.chart_entity = entity_id;
    let mut msg = PbStr::new();
    if g.cur_player == g.chart_entity && g.cur_player > 0 {
        if g.last_sel_entity < 11 || g.entity_status[g.last_sel_entity] != 1 {
            msg = s(__content.string(10143));
        } else { g.chart_entity = g.last_sel_entity; }
    } else if g.chart_entity > 10 && g.cap_hist_close[(3, g.chart_entity)] == Ext::from_f64(0.0) && g.price_hist_high[(0, g.chart_entity)] == Ext::from_f64(0.0) {
        let new = g.earnings_yr2_ago[g.chart_entity] == Ext::from_f64(0.0) && g.earnings_yr3_ago[g.chart_entity] == Ext::from_f64(0.0);
        let reason = if new { __content.string(10144) } else { __content.string(10145) };
        msg = s(__content.string(10146)) + reason + __content.string(10147) + if new { "new" } else { __content.string(10148) } + __content.string(10149);
    }
    if !msg.is_empty() { show_quick_message(g, ui, &mut msg); return; }
    if g.chart_entity > 10 && g.entity_status[g.chart_entity] != 1 { g.chart_entity = -1; }
    if g.chart_entity > 0 && g.chart_entity < 6 && (g.num_players < g.chart_entity || g.player_name[g.chart_entity] == "...OUT...") { g.chart_entity = -1; }
    g.valuation_mode = 0;
    if g.chart_entity > 10 && !env.skips(0x42c72a) {
        crate::logic::valuation::calc_stock_price(g, ui, rng, g.chart_entity, env.now.second);
    }
    if g.chart_entity == -1 { g.chart_entity = 0; }
    else if !env.skips(0x6141b1) { crate::logic::info_screens::chart::show_price_chart(g, ui, rng, env.now); }
}

//! The station dialog: `ShowAdvancedOptionsStation` (0x60b44b), `DlgProcAdvOptions` (0x60e344) and the
//! `AdvOpt_*` control callbacks.
use crate::pb::real::{Ext, cast};


use super::order_sheet::{clear_option_grid_fields, process_option_order_sheet, strike_range};
use super::strategy::auto_build_option_strategy;
use super::{ctl, OptionStation, StationEnv, ROWS};
use crate::logic::trade_screens::option_misc::{close_all_option_positions, show_option_size_limits};
use crate::logic::control::update_corp_control;
use crate::logic::earnings::{balance_cash_t_bills, calc_cur_month_of_year};
use crate::logic::economy::close_all_dialogs;
use crate::logic::finance::calc_entity_financials;
use crate::logic::newgame::LocalTime;
use crate::logic::report::number_to_words;
use crate::logic::reports_lists::show_option_positions;
use crate::logic::sim_entity::asset_sales::calc_group_stake_pct;
use crate::logic::trading::prelude::*;
use crate::logic::turn::ticker_sim_steps_skipping;
use crate::pb::string::mcase;
use crate::pb::numeric::fix;
use std::collections::VecDeque;

/// One user action on the open station dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StationAction {
    /// A button (control id, see [`ctl`]) was clicked.
    Click(i32),
    /// The text of grid box `id` was changed (no callback; `CONTROL SET TEXT`).
    SetText(i32, PbStr),
    /// The user left the buy/sell box `1320 + row` (EN_KILLFOCUS: `AdvOpt_ValidateBuySellRow<row>`).
    KillFocus(i32),
    /// The window was closed (WM_CLOSE: `DlgProcAdvOptions` warns about pending orders, then the dialog
    /// ends).
    Close,
    /// The dialog was destroyed without any message.
    End,
}

/// Port of ShowAdvancedOptionsStation (0x60b44b), driven through [`Ui`] (see
/// [`show_advanced_options_station_with`] for the logic). Hosts can supply an editable sheet through
/// [`Ui::option_station`]. Otherwise the station's buttons are offered with
/// [`Ui::choose`] (the order sheet in the prompt lines), together with "Edit line <n>" entries that ask for
/// the four fields of a line with `inputbox` (the current text as default). Cancelling the choice closes
/// the window.
pub fn show_advanced_options_station(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, h_parent: i32) {
    let env = StationEnv::new(now);
    let mut pending: VecDeque<StationAction> = VecDeque::new();
    let mut next = |st: &OptionStation, ui: &mut dyn Ui| -> StationAction { ui_next_action(st, ui, &mut pending) };
    show_advanced_options_station_with(g, ui, rng, &env, h_parent, &mut next);
}

/// Button captions offered by [`show_advanced_options_station`], with their control ids.
#[allow(non_snake_case)]
fn BUTTONS(__content: &crate::content::ContentPack) -> [(i32, &str); 9] { [
    (ctl::SUBMIT, "SUBMIT"),
    (ctl::CALC, "CALC"),
    (ctl::CLEAR_ALL, "CLEAR ALL"),
    (ctl::AUTO_TRADE, "AUTO-TRADE"),
    (ctl::SIZE_LIMITS, "SHOW SIZE LIMITS"),
    (ctl::LIST_OPTIONS, __content.string(11680)),
    (ctl::CLOSE_ALL_OPTIONS, ""),
    (ctl::HELP, __content.string(11681)),
    (ctl::CLOSE, "CLOSE"),
] }

/// The next action of the [`Ui`]-driven station (see [`show_advanced_options_station`]).
fn ui_next_action(st: &OptionStation, ui: &mut dyn Ui, pending: &mut VecDeque<StationAction>) -> StationAction {
    let __content = crate::content::current();
    if let Some(a) = pending.pop_front() {
        return a;
    }
    if let Some(actions) = ui.option_station(st) {
        pending.extend(actions);
        return pending.pop_front().unwrap_or(StationAction::Close);
    }
    let mut prompt = vec![
        st.stock_label.clone(),
        s("Line  Type  B/S  ") + &st.strike_header + "  " + &st.exp_header + "  Size %  Option Price",
    ];
    for row in 1..=cast::<i32, _>(ROWS) {
        let k = cast::<usize, _>(row - 1);
        prompt.push(
            s(&format!("{row}     {}  ", if row <= 4 { "PUT " } else { "CALL" }))
                + &st.grid.side[k]
                + "  "
                + &st.grid.strike[k]
                + "  "
                + &st.grid.exp[k]
                + "  "
                + &st.grid.qty[k]
                + "  "
                + &st.grid.price[k],
        );
    }
    if !st.status.is_empty() {
        prompt.push(st.status.clone());
    }
    let mut options: Vec<PbStr> = BUTTONS(&__content)
        .iter()
        .map(|&(id, cap)| if id == ctl::CLOSE_ALL_OPTIONS { remove(st.close_all_caption.as_bytes(), b"&") } else { s(cap) })
        .collect();
    for row in 1..=ROWS {
        options.push(s(&format!("Edit line {row} ({})", if row <= 4 { "PUT" } else { "CALL" })));
    }
    match ui.choose(&st.title, &prompt, &options) {
        None => StationAction::Close,
        Some(k) if k < BUTTONS(&__content).len() => StationAction::Click(BUTTONS(&__content)[k].0),
        Some(k) => {
            let row = cast::<i32, _>(k - BUTTONS(&__content).len() + 1);
            let kk = cast::<usize, _>(row - 1);
            let title = s(&format!("{__wsr_text_11682}{row}",__wsr_text_11682=__content.string(11682)));
            let side = ui.inputbox(&s(__content.string(11683)), &title, &st.grid.side[kk]);
            pending.push_back(StationAction::KillFocus(ctl::SIDE + row));
            let strike = ui.inputbox(&st.strike_header, &title, &st.grid.strike[kk]);
            pending.push_back(StationAction::SetText(ctl::STRIKE + row, strike));
            let exp = ui.inputbox(&st.exp_header, &title, &st.grid.exp[kk]);
            pending.push_back(StationAction::SetText(ctl::EXP + row, exp));
            let qty = ui.inputbox(&s(__content.string(11684)), &title, &st.grid.qty[kk]);
            pending.push_back(StationAction::SetText(ctl::QTY + row, qty));
            StationAction::SetText(ctl::SIDE + row, side)
        }
    }
}

/// Port of ShowAdvancedOptionsStation (0x60b44b).
///
/// Entry checks (each ends with a message): a company acting needs credit rating `BBB` (7) or better; at
/// most 990 option records; the option stock is `SelEntity` unless the current player controls it (or it
/// is the player), else `LastSelEntity` if not controlled by the player — asking YES/NO/CANCEL when
/// switching (CANCEL ends; NO goes on with stock -1, BUGS.md #361); at least 10% publicly traded
/// (`UpdateCorpControl`); share price 10..1000; not in dire condition (rating 0, or rating 1 with no
/// projected earnings or a bank); no insider of a pending merger target (5% incl. `TmpExt1`, which is
/// cleared); some cash/T-bills or credit line (after `BalanceCashTBills`). ETFs get a warning. Then the
/// dialog opens ([`OptionStation`]) and `next` supplies the user's actions until it ends.
pub fn show_advanced_options_station_with(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    env: &StationEnv,
    _h_parent: i32,
    next: &mut dyn FnMut(&OptionStation, &mut dyn Ui) -> StationAction,
) -> Option<OptionStation> {
    let Some(mut st) = open_station(g, ui, rng) else {
        return None;
    };
    // DIALOG SHOW MODAL hDlgOptions CALL DlgProcAdvOptions
    while st.open {
        match next(&st, ui) {
            StationAction::Click(id) => click(g, ui, rng, env, &mut st, id, next),
            StationAction::SetText(id, t) => st.grid.set_text(id, &t),
            StationAction::KillFocus(id) => {
                if (ctl::SIDE + 1..=ctl::SIDE + 8).contains(&id) {
                    adv_opt_validate_buy_sell_row(g, ui, &mut st, id - ctl::SIDE);
                }
            }
            StationAction::Close => {
                dlg_proc_adv_options_close(g, ui, &st);
                st.open = false;
            }
            StationAction::End => st.open = false,
        }
    }
    Some(st)
}

/// The entry checks and dialog setup of ShowAdvancedOptionsStation (0x60b44b): `Some` when the dialog
/// opens.
fn open_station(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> Option<OptionStation> {
    let __content = crate::content::current();
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    if !(g.active_entity < 11 || 6 < g.credit_rating_num[g.active_entity]) {
        let m = s(__content.string(11685))
            + __content.string(11686)
            + __content.string(11687)
            + &g.active_entity_name
            + __content.string(11688);
        ui.msgbox(&m, 8208, &s(__content.string(11689)));
        return None;
    }
    if Ext::from_f64(990.0) <= val(g.option_recs[0].as_bytes()) {
        let mut m = g.app_title.clone()
            + __content.string(11690)
            + __content.string(11691)
            + __content.string(11692);
        show_quick_message(g, ui, &mut m);
        return None;
    }
    let cp = g.cur_player;
    let mut stock_id = g.sel_entity;
    if 10 < g.sel_entity && cp == g.corp_controller[g.sel_entity] {
        stock_id = -1;
    }
    if g.cur_player == stock_id {
        stock_id = -1;
    }
    let mut msg_text;
    if stock_id == -1 {
        if 10 < g.last_sel_entity && g.cur_player != g.corp_controller[g.last_sel_entity] {
            stock_id = g.last_sel_entity;
        }
        if g.last_sel_entity != stock_id || g.sel_entity != g.cur_player {
            msg_text = s(__content.string(11693))
                + __content.string(11694)
                + __content.string(11695);
            if stock_id == -1 {
                msg_text = trim(msg_text.as_bytes())
                    + "\r\n"
                    + "\r\n"
                    + __content.string(11696);
                let mut own_corp_id = 0;
                if g.sel_entity < 11 || g.cur_player != g.corp_controller[g.sel_entity] {
                    if 10 < g.last_sel_entity && g.cur_player == g.corp_controller[g.last_sel_entity] {
                        own_corp_id = g.last_sel_entity;
                    }
                } else {
                    own_corp_id = g.sel_entity;
                }
                if 10 < own_corp_id {
                    msg_text = msg_text + __content.string(5132) + &g.ent_name[own_corp_id];
                }
                show_quick_message(g, ui, &mut msg_text);
                return None;
            }
            if 10 < g.sel_entity {
                msg_text = msg_text
                    + __content.string(11697)
                    + "on "
                    + &g.sel_corp_name
                    + __content.string(11698)
                    + "\""
                    + __content.string(11699)
                    + "\""
                    + "\r"
                    + "\r";
            }
            msg_text = msg_text
                + __content.string(11700)
                + &g.ent_name[stock_id]
                + __content.string(11701)
                + __content.string(11702)
                + "\r"
                + "\r"
                + __content.string(11703)
                + &g.active_entity_name
                + " ON "
                + __content.string(11704)
                + &g.ent_name[stock_id]
                + "?";
            let title = s(__content.string(11705)) + &g.ent_symbol[stock_id] + " Now?";
            let answer = ui.msgbox(&msg_text, 8483, &title);
            if answer == 2 {
                return None;
            }
            if answer == 7 {
                // NO goes on with stock -1 (BUGS.md #361)
                stock_id = -1;
            }
        }
    }
    update_corp_control(g, ui, rng, stock_id);
    if g.public_pct < 10 {
        msg_text = s(__content.string(11706))
            + &g.ent_name[stock_id]
            + __content.string(11707)
            + __content.string(11708);
        if g.public_pct == 0 {
            msg_text = s(__content.string(1093))
                + &g.ent_name[stock_id]
                + __content.string(11709);
        }
        show_quick_message(g, ui, &mut msg_text);
        return None;
    }
    let tmp_int = to_long(fix(g.market_cap[stock_id] / g.shares_outstanding[stock_id]));
    if tmp_int < 10 || 1000 < tmp_int {
        let mut month_text = s(__content.string(11710))
            + "\r\n"
            + "\r\n"
            + __content.string(11711);
        if 4 < tmp_int && tmp_int < 1000 {
            month_text = month_text + __content.string(11712) + &g.ent_name[stock_id] + __content.string(11713);
        }
        if 1000 < tmp_int {
            month_text = s(__content.string(11714));
        }
        show_quick_message(g, ui, &mut month_text);
        return None;
    }
    calc_entity_financials(g, ui, rng, stock_id);
    let mut tradable = true;
    msg_text = PbStr::new();
    if g.credit_rating_num[stock_id] == 1 && g.proj_qtr_earnings[stock_id] <= Ext::from_f64(0.0) {
        tradable = false;
    }
    if g.credit_rating_num[stock_id] == 1 && g.industry_num[stock_id] == 1 {
        tradable = false;
    }
    if g.credit_rating_num[stock_id] == 0 {
        tradable = false;
    }
    if !tradable && msg_text.is_empty() {
        msg_text = s(__content.string(11715))
            + &g.ent_name[stock_id]
            + __content.string(11716)
            + __content.string(11717);
    }
    if 10 < g.merger_target && stock_id == g.merger_target && 10 < g.merger_acquirer {
        let cp = g.cur_player;
        let stake = calc_group_stake_pct(g, cp, stock_id);
        let pct = to_long(g.tmp_ext1 + cast::<Ext, _>(stake));
        if 4 < pct {
            let words = number_to_words(pct, 0, 1);
            msg_text = s(__content.string(11718))
                + &g.ent_name[stock_id]
                + __content.string(11719)
                + __content.string(11720)
                + &g.ent_symbol[stock_id]
                + __content.string(11721)
                + __content.string(11722)
                + &g.ent_name[g.merger_acquirer]
                + "."
                + "\r"
                + "\r"
                + __content.string(11723)
                + "\r"
                + "\r"
                + __content.string(11724)
                + __content.string(11725)
                + &g.ent_symbol[stock_id]
                + __content.string(11726)
                + __content.string(11727)
                + "\r"
                + "\r"
                + __content.string(11728)
                + &words
                + ".";
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
    }
    let ae = g.active_entity;
    balance_cash_t_bills(g, ae);
    calc_entity_financials(g, ui, rng, ae);
    if g.unused_credit_line == Ext::from_f64(0.0) && g.cash[ae] + g.t_bills[ae] <= Ext::from_f64(0.0) {
        msg_text = s(__content.string(11729))
            + __content.string(11730);
        if 10 < ae {
            msg_text = replace(msg_text.as_bytes(), __content.bytes(1506), (g.ent_name[ae].clone() + " has").as_bytes());
        }
    }
    if !msg_text.is_empty() {
        ui.msgbox(&msg_text, 8208, &s(__content.string(11731)));
        return None;
    }
    if g.industry_num[stock_id] == 71 {
        let mut m = s(__content.string(11732))
            + &g.ent_name[stock_id]
            + __content.string(11733)
            + __content.string(11734)
            + __content.string(11735);
        show_quick_message(g, ui, &mut m);
    }
    // DIALOG NEW hParent, ... TO hDlgOptions; AdjustDialogFrameSize; CONTROL ADD ... (window plumbing:
    // the texts the dialog shows are kept in the station)
    let mut st = OptionStation {
        stock_id,
        title: s(__content.string(11736)) + &g.ent_symbol[g.active_entity],
        open: true,
        ..OptionStation::default()
    };
    let price_text = fmt_trim(g.market_cap[stock_id] / g.shares_outstanding[stock_id], "* ###.00");
    st.stock_label = s(__content.string(11737)) + &g.ent_name[stock_id] + " / " + &price_text;
    let share_price = to_long(fix(g.market_cap[stock_id] / g.shares_outstanding[stock_id]).abs());
    let (min_strike, max_strike) = strike_range(share_price);
    st.strike_header = s(__content.string(11738)) + &str_trim(min_strike) + " to" + &str_s(max_strike) + ")";
    let progress = g.sim_co_count;
    let mut month = 1 + to_long(ceil(calc_cur_month_of_year(g, progress)));
    let mut year_text = str_trim(cast::<Ext, _>(g.cur_year));
    if month == 13 {
        month = 1;
        year_text = str_trim(cast::<Ext, _>(1 + g.cur_year));
    }
    let mut range = s("0") + &str_trim(cast::<Ext, _>(month));
    range = right(range.as_bytes(), 2) + "/" + &year_text + " to ";
    let mut max_month = month - 1;
    if max_month == 0 {
        max_month = 12;
    }
    let max_month_text = right((s("0") + &str_trim(cast::<Ext, _>(max_month))).as_bytes(), 2);
    let max_year_text = str_trim(cast::<Ext, _>(2 + g.cur_year));
    range = range + &max_month_text + "/" + &max_year_text;
    st.exp_header = s(__content.string(11739)) + &range;
    st.close_all_caption = s(__content.string(11740)) + &g.ent_symbol[stock_id] + __content.string(11741);
    Some(st)
}

/// The button callbacks of the station: AdvOpt_ClearAllClick (0x610275), AdvOpt_HelpClick (0x610358),
/// AdvOpt_CalcClick (0x6103e3), AdvOpt_ShowSizeLimitsClick (0x6106d5), AdvOpt_AutoTradeClick (0x610770),
/// AdvOpt_ListOptionsClick (0x61054a), AdvOpt_CloseAllOptionsClick (0x6105ff), AdvOpt_SubmitClick
/// (0x6102d9) and AdvOpt_CloseClick (0x610438).
fn click(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    env: &StationEnv,
    st: &mut OptionStation,
    id: i32,
    next: &mut dyn FnMut(&OptionStation, &mut dyn Ui) -> StationAction,
) {
    match id {
        ctl::CLEAR_ALL => adv_opt_clear_all_click(st),
        ctl::HELP => adv_opt_help_click(),
        ctl::CALC => adv_opt_calc_click(g, ui, rng, env, st),
        ctl::SIZE_LIMITS => adv_opt_show_size_limits_click(g, ui, env),
        ctl::AUTO_TRADE => adv_opt_auto_trade_click(g, ui, rng, env, st),
        ctl::LIST_OPTIONS => adv_opt_list_options_click(g, ui, rng, env, st, next),
        ctl::CLOSE_ALL_OPTIONS => adv_opt_close_all_options_click(g, ui, rng, env, st),
        ctl::SUBMIT => adv_opt_submit_click(g, ui, rng, env, st),
        ctl::CLOSE => adv_opt_close_click(g, ui, st),
        _ => {}
    }
}

/// Port of AdvOpt_ClearAllClick (0x610275): `ClearOptionGridFields` and an empty status line.
fn adv_opt_clear_all_click(st: &mut OptionStation) {
    clear_option_grid_fields(&mut st.grid);
    st.status = PbStr::new();
}

/// Port of AdvOpt_HelpClick (0x610358): `SHELL WSRHELP.EXE |ADV|` (the help viewer; not ported, UI).
/// `DlgProcAdvOptions` does the same on WM_HELP (F1).
fn adv_opt_help_click() {}

/// Port of AdvOpt_CalcClick (0x6103e3): `ProcessOptionOrderSheet(0)` (price the sheet without executing).
fn adv_opt_calc_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &StationEnv, st: &mut OptionStation) {
    process_option_order_sheet(g, ui, rng, env, st, 0);
}

/// Port of AdvOpt_SubmitClick (0x6102d9): `ProcessOptionOrderSheet(1)`; from the fifth action of the turn
/// the dialog ends and `TickerSimSteps(5)` runs (again, if the execution already did so and the turn did
/// not end).
fn adv_opt_submit_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &StationEnv, st: &mut OptionStation) {
    process_option_order_sheet(g, ui, rng, env, st, 1);
    if 4 < g.turn_action_count {
        st.open = false;
        if !env.skips(0x41ef3f) {
            ticker_sim_steps_skipping(g, ui, rng, 5, env.now, env.skip);
        }
    }
}

/// Port of AdvOpt_CloseClick (0x610438): with an order line started (a non-empty B/S box) asks "Close
/// [runtime resource wording] " (YES ends the dialog), else ends it.
fn adv_opt_close_click(g: &mut Globals, ui: &mut dyn Ui, st: &mut OptionStation) {
    let __content = crate::content::current();
    for row in 1..=8 {
        let t = st.grid.text(ctl::SIDE + row);
        if !trim(t.as_bytes()).is_empty() {
            let m = s(__content.string(11742));
            if ui.msgbox(&m, 8483, &s(__content.string(11743))) == 6 {
                dlg_proc_adv_options_close(g, ui, st);
                st.open = false;
            }
            return;
        }
    }
    st.open = false;
}

/// Port of AdvOpt_ListOptionsClick (0x61054a): `CloseAllDialogs(1)`, the active entity's option positions
/// (`ShowOptionPositions`); if a link there changed `SelEntity`, the station is closed and reopened.
fn adv_opt_list_options_click(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    env: &StationEnv,
    st: &mut OptionStation,
    next: &mut dyn FnMut(&OptionStation, &mut dyn Ui) -> StationAction,
) {
    let prev_sel = g.sel_entity;
    close_all_dialogs(g, ui, rng, 1);
    // (UI) DIALOG SHOW STATE hDlgOptions, 0
    if !env.skips(0x5f51fc) {
        let ae = g.active_entity;
        show_option_positions(g, ui, rng, ae, 0, 0);
    }
    // (UI) DIALOG SHOW STATE hDlgOptions, 5
    if g.sel_entity != prev_sel {
        st.open = false;
        show_advanced_options_station_with(g, ui, rng, env, 0, next);
    }
}

/// The option stock of the button callbacks: `SelEntity`, or `LastSelEntity` when `SelEntity` is the
/// current player or controlled by them.
fn callback_stock(g: &Globals) -> i32 {
    if g.cur_player == g.sel_entity || g.cur_player == g.corp_controller[g.sel_entity] {
        g.last_sel_entity
    } else {
        g.sel_entity
    }
}

/// Port of AdvOpt_CloseAllOptionsClick (0x6105ff): `CloseAllOptionPositions(stock)`; when it returns 1,
/// `TurnActionCount + 1` and from the fifth action the dialog ends and `TickerSimSteps(5)` runs.
fn adv_opt_close_all_options_click(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    env: &StationEnv,
    st: &mut OptionStation,
) {
    let stock_id = callback_stock(g);
    let closed = if env.skips(0x611ab6) { 0 } else { close_all_option_positions(g, ui, rng, stock_id) };
    if closed == 1 {
        g.turn_action_count = g.turn_action_count + 1;
        if 4 < g.turn_action_count {
            st.open = false;
            if !env.skips(0x41ef3f) {
                ticker_sim_steps_skipping(g, ui, rng, 5, env.now, env.skip);
            }
        }
    }
}

/// Port of AdvOpt_ShowSizeLimitsClick (0x6106d5): `ShowOptionSizeLimits(stock)`.
fn adv_opt_show_size_limits_click(g: &mut Globals, ui: &mut dyn Ui, env: &StationEnv) {
    let stock_id = callback_stock(g);
    if !env.skips(0x612d58) {
        show_option_size_limits(g, ui, stock_id);
    }
}

/// Port of AdvOpt_AutoTradeClick (0x610770): `AutoBuildOptionStrategy(stock)` (the station is hidden
/// meanwhile); when a strategy was built, `ProcessOptionOrderSheet(0)` prices it and the status line shows
/// "Strategy selected: <Name>".
fn adv_opt_auto_trade_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &StationEnv, st: &mut OptionStation) {
    let __content = crate::content::current();
    // (UI) DIALOG SHOW STATE hDlgOptions, 0
    let stock_id = callback_stock(g);
    let strategy_name = auto_build_option_strategy(g, ui, &mut st.grid, stock_id);
    if !strategy_name.is_empty() {
        process_option_order_sheet(g, ui, rng, env, st, 0);
        st.status = s(__content.string(11744)) + &mcase(strategy_name.as_bytes(), CaseMode::Ansi);
    }
    // (UI) DIALOG SHOW STATE hDlgOptions, 5
}

/// Port of AdvOpt_ValidateBuySellRow1..8 (0x60f101, 0x60f234, 0x60f367, 0x60f49a, 0x60f5cd, 0x60f700,
/// 0x60f833, 0x60f966): on leaving the B/S box of line `row` (EN_KILLFOCUS), anything but "", "B" or "S"
/// (exact, no trimming) shows "Must enter 'B' or 'S' ..." and empties the box.
pub fn adv_opt_validate_buy_sell_row(g: &mut Globals, ui: &mut dyn Ui, st: &mut OptionStation, row: i32) {
    let __content = crate::content::current();
    let entry_text = st.grid.text(ctl::SIDE + row);
    if !entry_text.is_empty() && !(entry_text == "B") && !(entry_text == "S") {
        show_quick_message(g, ui, &mut s(__content.string(11745)));
        st.grid.set_text(ctl::SIDE + row, &PbStr::new());
    }
}

/// Port of AdvOpt_StrikeFieldRow1Cb..8Cb (0x60fa95, 0x60fae9, 0x60fb3d, 0x60fb91, 0x60fbe5, 0x60fc39,
/// 0x60fc8d, 0x60fce1), AdvOpt_ExpDateFieldRow1Cb..8Cb (0x60fd35, 0x60fd89, 0x60fddd, 0x60fe31, 0x60fe85,
/// 0x60fed9, 0x60ff2d, 0x60ff81) and AdvOpt_PctFieldRow1Cb..8Cb (0x60ffd5, 0x610029, 0x61007d, 0x6100d1,
/// 0x610125, 0x610179, 0x6101cd, 0x610221): the callbacks of the strike, expiry and size boxes do nothing.
pub fn adv_opt_field_cb() {}

/// Port of DlgProcAdvOptions (0x60e344), its one game-visible message: on WM_CLOSE, if a B/S box holds
/// "B" or "S" (trimmed), " [runtime resource wording] ".
/// The rest is window plumbing (control colours `ColorTable`/`BrushHandles`, tooltips, WM_HELP starting
/// the help viewer) and not ported.
pub fn dlg_proc_adv_options_close(g: &mut Globals, ui: &mut dyn Ui, st: &OptionStation) {
    let __content = crate::content::current();
    for row in 1..=8 {
        let t = trim(st.grid.text(ctl::SIDE + row).as_bytes());
        if t == "B" || t == "S" {
            show_quick_message(g, ui,
                &mut s(__content.string(11746)),
            );
            break;
        }
    }
}

//! `ProcessOptionOrderSheet` (0x608b95) with its GOSUBs, and `ClearOptionGridFields` (0x6130f7).
use crate::pb::real::{Ext, cast};


use super::{ctl, OptionGrid, OptionStation, StationEnv};
use crate::logic::earnings::calc_cur_month_of_year;
use crate::logic::finance::calc_entity_financials;
use crate::logic::options::{
    add_or_remove_option_rec, calc_option_price, calc_spread_factor, get_max_options_pct, merge_matching_option_rec,
};
use crate::logic::trading::prelude::*;
use crate::logic::turn::ticker_sim_steps_skipping;
use crate::pb::ext80::Ext80;
use crate::pb::numeric::format_ext;
use crate::pb::numeric::str_num_ext;
use crate::pb::string::parse_count;
use crate::pb::numeric::{fix, Cux};

/// `STR$(x)` of an 80-bit value.
fn str_e(x: Ext80) -> PbStr {
    PbStr::from(str_num_ext(x, 15).as_str())
}

/// `RTRIM$(LTRIM$(FORMAT$(x, mask)))` of an 80-bit value.
fn fmt_trim_e(x: Ext80, mask: &str) -> PbStr {
    trim(&format_ext(x, mask.as_bytes()))
}

/// `RTRIM$(LTRIM$(CONTROL GET TEXT))`.
fn get(grid: &OptionGrid, id: i32) -> PbStr {
    trim(grid.text(id).as_bytes())
}

/// The strike range of the order sheet for a share price: `FIX(0.75 * price)` (at least 5) to
/// `CEIL(1.25 * price)` (at most 999).
pub(super) fn strike_range(share_price: i32) -> (Ext, Ext) {
    let mut min_strike = fix(cast::<Ext, _>(cast::<f32, _>(share_price)) * Ext::from_f64(0.75));
    if min_strike < Ext::from_f64(5.0) {
        min_strike = Ext::from_f64(5.0);
    }
    let mut max_strike = ceil(cast::<Ext, _>(cast::<f32, _>(share_price)) * Ext::from_f64(1.25));
    if Ext::from_f64(999.0) < max_strike {
        max_strike = Ext::from_f64(999.0);
    }
    (min_strike, max_strike)
}

/// Locals of ProcessOptionOrderSheet shared with its GOSUBs.
#[derive(Default)]
struct Frame {
    order_recs: [PbStr; 9],
    msg_text: PbStr,
    hint_text: PbStr,
    caution_text: PbStr,
    net_cost: Ext80,
    borrow_amt: Ext80,
    leg_count: i32,
    excess_pct: i32,
    stock_id: i32,
}

/// Port of ProcessOptionOrderSheet (0x608b95) with its GOSUBs OptionOrderSheet_ClearRow (0x60ad7e),
/// OptionOrderSheet_Execute (0x60af36), OptionOrderSheet_ClearAll (0x60ae51), OptionOrderSheet_MsgTooMany
/// (0x60b1ab) and ProcessOptionOrderSheet_Exit (0x60b40f).
///
/// Reads the eight order lines of the station: the option stock is `SelEntity`, or `LastSelEntity` when
/// the current player controls `SelEntity`. A line counts when all of B/S, strike, expiry and size are
/// filled (a line with an empty field is cleared); the first invalid field (side not "B"/"S", strike
/// outside `FIX(0.75 price) (>= 5) .. CEIL(1.25 price) (<= 999)`, expiry not "m/yy(yy)" within the next
/// 24 months, size outside 1..10) shows a message, clears that field and ends. Each line is priced
/// (`CalcOptionPrice`, the ask side x `CalcSpreadFactor` for purchases, at least 0.01), its price per share
/// written to the price column, and an option record prepared. Size limits per kind
/// (`GetMaxOptionsPct`) end with a "Too Many ..." message. Then the net cost is reported: with
/// `execute_mode = 1` the player is asked to execute (borrowing what cash and T-bills don't cover); YES
/// executes: bank debt/cash adjusted, records merged or added, " [runtime resource wording] ", the sheet
/// cleared, `TurnActionCount + 1`, news for a multi-leg position, and after the fifth action the dialog
/// ends and `TickerSimSteps(5)` runs.
///
/// Note: the size limit of short calls asks `GetMaxOptionsPct` with "s" instead of "S", which counts
/// long calls too (BUGS.md #360). The routine's ON ERROR handler (ShowRuntimeError on array-bound
/// errors) is not ported.
pub fn process_option_order_sheet(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    env: &StationEnv,
    st: &mut OptionStation,
    execute_mode: i32,
) {
    let __content = crate::content::current();
    let mut f = Frame::default();
    let mut max_pcts = [0i32; 5];
    let mut stock_id = g.sel_entity;
    if g.cur_player == g.corp_controller[g.sel_entity] {
        stock_id = g.last_sel_entity;
    }
    f.stock_id = stock_id;
    let share_price = to_long(fix(g.market_cap[stock_id] / g.shares_outstanding[stock_id]).abs());
    let (min_strike, max_strike) = strike_range(share_price);
    max_pcts[1] = get_max_options_pct(g, stock_id, 2, &s("L"), 1);
    max_pcts[2] = get_max_options_pct(g, stock_id, 2, &s("S"), 1);
    max_pcts[3] = get_max_options_pct(g, stock_id, 1, &s("L"), 1);
    // "s": the original's typo for "S" (BUGS.md #360)
    max_pcts[4] = get_max_options_pct(g, stock_id, 1, &s("s"), 1);
    let progress = g.sim_co_count;
    let mut min_month = 1 + to_long(ceil(calc_cur_month_of_year(g, progress)));
    let mut min_year = g.cur_year;
    if min_month == 13 {
        min_month = 1;
        min_year = g.cur_year + 1;
    }
    let mut max_month = min_month - 1;
    let mut max_year = min_year + 2;
    if max_month == 0 {
        max_month = 12;
        max_year = min_year + 1;
    }
    let min_exp_date: Ext = cast::<Ext, _>(min_month) / Ext::from_f64(12.0) + cast::<Ext, _>(min_year);
    let max_exp_date: Ext = cast::<Ext, _>(max_month) / Ext::from_f64(12.0) + cast::<Ext, _>(max_year);
    let mut long_put_qty = 0i32;
    let mut short_put_qty = 0i32;
    let mut long_call_qty = 0i32;
    let mut short_call_qty = 0i32;
    let mut itm_cost = Ext80::from_i64(0);
    let mut row_count = 0;
    let mut opt_type = 2;
    let grid = &mut st.grid;
    for row in 1..=8 {
        if 4 < row {
            opt_type = 1;
        }
        let side_text = get(grid, ctl::SIDE + row);
        if side_text.is_empty() {
            clear_row(grid, row);
            continue;
        }
        if !(side_text == "B") && !(side_text == "S") {
            let mut m = s(__content.string(11636))
                + &str_s(cast::<Ext, _>(row))
                + __content.string(11637);
            show_quick_message(g, ui, &mut m);
            grid.set_text(ctl::SIDE + row, &PbStr::new());
            return;
        }
        let strike_text = get(grid, ctl::STRIKE + row);
        if strike_text.is_empty() {
            clear_row(grid, row);
            continue;
        }
        if val(strike_text.as_bytes()) < min_strike || max_strike < val(strike_text.as_bytes()) {
            let mut m = s(__content.string(11638))
                + &str_s(cast::<Ext, _>(row))
                + __content.string(11639)
                + &fmt_trim(min_strike, "* ###,")
                + " and "
                + &fmt_trim(max_strike, "* ###,");
            show_quick_message(g, ui, &mut m);
            grid.set_text(ctl::STRIKE + row, &PbStr::new());
            return;
        }
        let exp_text = get(grid, ctl::EXP + row);
        if exp_text.is_empty() {
            clear_row(grid, row);
            continue;
        }
        let mut m = s(__content.string(11638))
            + &str_s(cast::<Ext, _>(row))
            + __content.string(11640);
        if parse_count(exp_text.as_bytes(), b"/") != 2 || len(exp_text.as_bytes()) < 4 || 7 < len(exp_text.as_bytes())
        {
            show_quick_message(g, ui, &mut m);
            grid.set_text(ctl::EXP + row, &PbStr::new());
            return;
        }
        let mut exp_ok = 1;
        m = s(__content.string(11638)) + &str_s(cast::<Ext, _>(row)) + __content.string(11641);
        let mut parsed_year = to_long(val(parse_any(exp_text.as_bytes(), b"/", 1).as_bytes()));
        if parsed_year < 1 || 12 < parsed_year {
            exp_ok = 0;
        }
        parsed_year = to_long(val(parse_any(exp_text.as_bytes(), b"/", 2).as_bytes()));
        if 2 < parsed_year && parsed_year < 100 {
            let year_text = str_trim(cast::<Ext, _>(g.cur_year));
            parsed_year = to_long(val(mid(year_text.as_bytes(), 1, 2).as_bytes()) * Ext::from_f64(100.0) + cast::<Ext, _>(parsed_year));
        }
        if parsed_year < g.cur_year || g.cur_year + 2 < parsed_year {
            exp_ok = 0;
        }
        let mut exp_date: Ext = cast::<Ext, _>(parsed_year);
        if exp_ok == 1 {
            exp_date = val(parse_any(exp_text.as_bytes(), b"/", 1).as_bytes()) / Ext::from_f64(12.0) + exp_date;
            if sf(0.01f32) + exp_date < min_exp_date || max_exp_date < exp_date - sf(0.01f32) {
                exp_ok = 0;
            }
        }
        if exp_ok == 0 {
            show_quick_message(g, ui, &mut m);
            grid.set_text(ctl::EXP + row, &PbStr::new());
            return;
        }
        let exp_month = to_long(val(parse_any(exp_text.as_bytes(), b"/", 1).as_bytes()));
        let exp_year = parsed_year;
        let qty_text = get(grid, ctl::QTY + row);
        if qty_text.is_empty() {
            clear_row(grid, row);
            continue;
        }
        if val(qty_text.as_bytes()) < Ext::from_f64(1.0) || Ext::from_f64(10.0) < val(qty_text.as_bytes()) {
            let mut m = s(__content.string(11638)) + &str_s(cast::<Ext, _>(row)) + __content.string(11642);
            show_quick_message(g, ui, &mut m);
            grid.set_text(ctl::QTY + row, &PbStr::new());
            return;
        }
        let q = val(qty_text.as_bytes());
        if side_text == "B" && opt_type == 2 {
            long_put_qty = to_long(q + cast::<Ext, _>(long_put_qty));
        }
        if side_text == "S" && opt_type == 2 {
            short_put_qty = to_long(q + cast::<Ext, _>(short_put_qty));
        }
        if side_text == "B" && opt_type == 1 {
            long_call_qty = to_long(q + cast::<Ext, _>(long_call_qty));
        }
        if side_text == "S" && opt_type == 1 {
            short_call_qty = to_long(q + cast::<Ext, _>(short_call_qty));
        }
        f.leg_count = f.leg_count + 1;
        let strike_cap =
            Ext80::from_ext(g.shares_outstanding[stock_id]).fmul(Ext80::from_ext(val(strike_text.as_bytes())));
        let mut premium: Ext = calc_option_price(g, ui, rng, stock_id, Ext::from(strike_cap.into_ext()), exp_year, exp_month, opt_type, 0);
        if side_text == "B" {
            let spread = calc_spread_factor(opt_type, g.market_cap[stock_id], Ext::from(strike_cap.into_ext()));
            premium = Ext::from(Ext80::from_ext(spread).fmul(Ext80::from_ext(premium)).into_ext());
        }
        if premium < sf(0.01f32) {
            premium = sf(0.01f32);
        }
        let mut qty = to_long(val(qty_text.as_bytes()));
        let mut total_prem = Cux::default();
        total_prem.set(premium);
        let mut prem_per_share = Cux::default();
        prem_per_share.set_ext(total_prem.get_ext().fdiv(Ext80::from_ext(g.shares_outstanding[stock_id])));
        grid.set_text(ctl::PRICE + row, &fmt_trim_e(prem_per_share.get_ext(), "* ###.00"));
        total_prem.set_ext(total_prem.get_ext().fmul(Ext80::from_i64(cast::<i64, _>(qty))));
        if side_text == "B" {
            f.net_cost = f.net_cost.fadd(total_prem.get_ext());
            let strike_v = val(strike_text.as_bytes());
            if cast::<Ext, _>(share_price) <= strike_v + sf(0.1f32) && opt_type == 1 {
                itm_cost = itm_cost.fadd(total_prem.get_ext());
            }
            if strike_v <= sf(0.1f32) + cast::<Ext, _>(share_price) && opt_type == 2 {
                itm_cost = itm_cost.fadd(total_prem.get_ext());
            }
        } else {
            f.net_cost = f.net_cost.fsub(total_prem.get_ext());
            qty = -qty;
        }
        let rec = str_trim(cast::<Ext, _>(stock_id))
            + "|"
            + &str_trim(cast::<Ext, _>(opt_type))
            + "|"
            + &str_trim(cast::<Ext, _>(g.active_entity))
            + "|"
            + &str_trim(cast::<Ext, _>(qty))
            + "|"
            + &trim(str_e(strike_cap).as_bytes())
            + "|"
            + &str_trim(cast::<Ext, _>(exp_month))
            + "|"
            + &str_trim(cast::<Ext, _>(exp_year))
            + "|"
            + &trim(str_e(total_prem.get_ext()).as_bytes());
        f.order_recs[cast::<usize, _>(row)] = rec;
        row_count = row_count + 1;
    }
    if row_count == 0 {
        show_quick_message(g, ui, &mut s(__content.string(11643)));
        return;
    }
    f.msg_text = PbStr::new();
    f.caution_text = PbStr::new();
    let limits = [
        (max_pcts[1], long_put_qty, __content.string(11644)),
        (max_pcts[2], short_put_qty, __content.string(3360)),
        (max_pcts[3], long_call_qty, __content.string(3359)),
        (max_pcts[4], short_call_qty, __content.string(11645)),
    ];
    for (max_pct, qty, kind) in limits {
        f.excess_pct = max_pct - qty;
        if f.excess_pct < 0 {
            f.msg_text = s(kind);
            f.caution_text = str_s(cast::<Ext, _>(max_pct));
            msg_too_many(g, ui, &mut f);
            return;
        }
    }
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    let net_cost = f.net_cost.into_ext();
    let itm = itm_cost.into_ext();
    f.msg_text = s(__content.string(11646))
        + &g.currency_symbol
        + &fmt_trim_e(f.net_cost, "* ###,")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + ", ";
    f.caution_text = s(__content.string(11647));
    if net_cost <= Ext::from_f64(0.0) {
        if net_cost < Ext::from_f64(0.0) {
            f.caution_text = f.caution_text.clone() + __content.string(11648);
        }
    } else {
        f.caution_text = f.caution_text.clone() + __content.string(11649);
    }
    f.caution_text = f.caution_text.clone()
        + __content.string(11650)
        + __content.string(11651);
    if 10 < ae {
        f.caution_text = replace(f.caution_text.as_bytes(), b"your", (g.active_entity_name.clone() + "'s").as_bytes());
    }
    let cash_eq = g.cash[ae] + g.t_bills[ae];
    let adj_nw = g.tmp_credit_net_worth - itm;
    if Ext::from_f64(0.0) < net_cost {
        if net_cost < adj_nw * Ext::from_f64(0.2) {
            f.caution_text = PbStr::new();
        }
        if net_cost < cash_eq {
            if g.bank_debt[ae] < adj_nw * Ext::from_f64(0.2) {
                f.caution_text = PbStr::new();
            }
        }
        if cash_eq < net_cost {
            if net_cost - cash_eq + g.bank_debt[ae] < adj_nw * Ext::from_f64(0.2) {
                f.caution_text = PbStr::new();
            }
        }
    }
    if net_cost < Ext::from_f64(0.0) && net_cost.abs() < adj_nw * Ext::from_f64(0.4) {
        f.caution_text = PbStr::new();
    }
    f.hint_text = s(__content.string(11652));
    if !f.caution_text.is_empty() {
        f.hint_text = f.hint_text.clone() + "\r\n" + "\r\n" + &f.caution_text;
    }
    if cash_eq < net_cost {
        f.borrow_amt = f.net_cost.fsub(Ext80::from_ext(g.cash[ae]).fadd(Ext80::from_ext(g.t_bills[ae])));
    }
    if g.liquid_funds < net_cost {
        f.msg_text = f.msg_text.clone()
            + __content.string(11653)
            + &g.currency_symbol
            + &fmt_trim(g.liquid_funds, "* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "."
            + "\r"
            + "\r"
            + __content.string(11654);
        if 10 < ae {
            f.msg_text = replace(f.msg_text.as_bytes(), b"your", (g.active_entity_name.clone() + "'s").as_bytes());
        }
        if row_count == 1 {
            f.msg_text = replace(f.msg_text.as_bytes(), __content.bytes(11655), __content.bytes(11656));
        }
        ui.msgbox(&f.msg_text, 8208, &s(__content.string(1928)));
        return;
    }
    if Ext::from_f64(0.0) < net_cost && cash_eq < net_cost && execute_mode == 1 {
        f.msg_text = f.msg_text.clone()
            + __content.string(11657)
            + &g.currency_symbol
            + &fmt_trim_e(f.borrow_amt, "* ###,")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + __content.string(11658)
            + __content.string(11659)
            + "\r"
            + "\r"
            + __content.string(11660);
        if 10 < ae {
            f.msg_text = replace(f.msg_text.as_bytes(), b"your", (g.active_entity_name.clone() + "'s").as_bytes());
        }
        if !f.caution_text.is_empty() {
            f.msg_text = f.msg_text.clone() + " **" + "\r" + "\r" + &f.caution_text;
        }
        if ui.msgbox(&f.msg_text, 8483, &s(__content.string(11661))) != 6 {
            return;
        }
        execute(g, ui, rng, env, st, &mut f);
        return;
    }
    if net_cost <= Ext::from_f64(0.0) {
        f.msg_text = s(__content.string(11662))
            + &g.currency_symbol
            + &fmt_trim_e(f.net_cost.abs(), "* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + __content.string(11663);
        if 10 < ae {
            f.msg_text = replace(f.msg_text.as_bytes(), b"You", g.active_entity_name.as_bytes());
        }
        if execute_mode == 1 {
            f.msg_text = f.msg_text.clone() + "\r" + "\r" + __content.string(11660);
            if !f.caution_text.is_empty() {
                f.msg_text = f.msg_text.clone() + " **" + "\r" + "\r" + &f.caution_text;
            }
            if ui.msgbox(&f.msg_text, 8483, &s(__content.string(11664))) != 6 {
                return;
            }
            execute(g, ui, rng, env, st, &mut f);
        } else {
            f.msg_text = f.msg_text.clone() + "\r\n" + "\r\n" + &f.hint_text;
            show_quick_message(g, ui, &mut f.msg_text);
        }
    } else {
        f.msg_text = f.msg_text.clone() + __content.string(11665);
        if 10 < ae {
            f.msg_text =
                replace(f.msg_text.as_bytes(), __content.bytes(11595), (g.active_entity_name.clone() + " has").as_bytes());
        }
        if execute_mode == 1 {
            f.msg_text = f.msg_text.clone() + "\r" + "\r" + __content.string(11660);
            if !f.caution_text.is_empty() {
                f.msg_text = f.msg_text.clone() + " **" + "\r" + "\r" + &f.caution_text;
            }
            if ui.msgbox(&f.msg_text, 8483, &s(__content.string(11664))) != 6 {
                return;
            }
            execute(g, ui, rng, env, st, &mut f);
        } else {
            if Ext80::from_i64(0).cmp_val(f.borrow_amt) == std::cmp::Ordering::Less {
                f.msg_text = replace(f.msg_text.as_bytes(), b"cash", __content.bytes(11666));
            }
            let mut m = f.msg_text.clone() + "\r\n" + "\r\n" + &f.hint_text;
            show_quick_message(g, ui, &mut m);
        }
    }
}

/// Port of OptionOrderSheet_ClearRow (0x60ad7e): empties the five boxes of line `row`.
fn clear_row(grid: &mut OptionGrid, row: i32) {
    for base in [ctl::SIDE, ctl::STRIKE, ctl::EXP, ctl::QTY, ctl::PRICE] {
        grid.set_text(base + row, &PbStr::new());
    }
}

/// Port of OptionOrderSheet_ClearAll (0x60ae51): empties every grid box (ids 1321..1368 not ending in 9
/// or 0) and `RESET orderRecs()`.
fn clear_all(grid: &mut OptionGrid, f: &mut Frame) {
    clear_option_grid_fields(grid);
    f.order_recs = Default::default();
}

/// Port of OptionOrderSheet_Execute (0x60af36).
/// Books the order: `BankDebt`/`Cash` of the active entity + the amount to borrow, `Cash - netCost`;
/// each prepared record is merged into a matching one (`MergeMatchingOptionRec`) or added
/// (`AddOrRemoveOptionRec`); " [runtime resource wording] "; the sheet cleared; `TurnActionCount + 1`;
/// "<entity> [runtime resource wording] <symbol> options" for more than one leg; from the fifth
/// action the dialog ends and `TickerSimSteps(5)` runs.
fn execute(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &StationEnv, st: &mut OptionStation, f: &mut Frame) {
    let __content = crate::content::current();
    let ae = g.active_entity;
    if Ext80::from_i64(0).cmp_val(f.borrow_amt) == std::cmp::Ordering::Less {
        g.bank_debt[ae] = Ext::from(Ext80::from_ext(g.bank_debt[ae]).fadd(f.borrow_amt).into_ext());
        g.cash[ae] = Ext::from(Ext80::from_ext(g.cash[ae]).fadd(f.borrow_amt).into_ext());
    }
    g.cash[ae] = Ext::from(Ext80::from_ext(g.cash[ae]).fsub(f.net_cost).into_ext());
    for idx in 1..=8 {
        if f.order_recs[idx].is_empty() {
            continue;
        }
        let rec = f.order_recs[idx].clone();
        if merge_matching_option_rec(g, &rec) != 1 {
            add_or_remove_option_rec(g, ui, &rec, 0);
        }
    }
    show_quick_message(g, ui, &mut s(__content.string(11667)));
    clear_all(&mut st.grid, f);
    g.turn_action_count = g.turn_action_count + 1;
    if 1 < f.leg_count {
        g.news_msg = g.active_entity_name.clone()
            + __content.string(11668)
            + &g.ent_symbol[f.stock_id]
            + __content.string(6830);
        post_news(g, ui, 0, 0);
    }
    if 4 < g.turn_action_count {
        st.open = false; // DIALOG END hDlgOptions
        if !env.skips(0x41ef3f) {
            ticker_sim_steps_skipping(g, ui, rng, 5, env.now, env.skip);
        }
    }
}

/// Port of OptionOrderSheet_MsgTooMany (0x60b1ab): the "Too Many <kind> Options" message (the order sheet
/// [runtime resource wording] by `excessPct`), then the routine ends.
fn msg_too_many(g: &Globals, ui: &mut dyn Ui, f: &mut Frame) {
    let __content = crate::content::current();
    f.msg_text = f.msg_text.clone() + __content.string(6830);
    f.hint_text = s(__content.string(11669))
        + &f.msg_text
        + __content.string(11670)
        + __content.string(2146)
        + &g.ent_name[f.stock_id]
        + " is"
        + &f.caution_text
        + __content.string(11671)
        + &str_s(cast::<Ext, _>(f.excess_pct.abs()))
        + "% "
        + __content.string(11672)
        + &f.msg_text
        + __content.string(11673)
        + &f.msg_text
        + __content.string(11674)
        + __content.string(11675)
        + "\r"
        + "\r"
        + __content.string(11676)
        + "on "
        + &g.ent_name[f.stock_id]
        + __content.string(11677)
        + __content.string(11678);
    let title = s(__content.string(11679)) + &crate::pb::string::mcase(f.msg_text.as_bytes(), CaseMode::Ansi);
    ui.msgbox(&f.hint_text, 8208, &title);
}

/// Port of ClearOptionGridFields (0x6130f7): empties every order-sheet box (controls 1321..1368 whose id
/// does not end in 9 or 0, i.e. all five columns of the eight lines).
pub fn clear_option_grid_fields(grid: &mut OptionGrid) {
    for ctl_id in 1321..=1368 {
        let id_text = str_trim(cast::<Ext, _>(ctl_id));
        if !(right(id_text.as_bytes(), 1) == "9") && !(right(id_text.as_bytes(), 1) == "0") {
            grid.set_text(ctl_id, &PbStr::new());
        }
    }
}

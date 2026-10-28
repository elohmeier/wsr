//! Creating a swap from the dialog: `IRSwap_CreateSwap` (0x63efd6) with its GOSUBs and
//! `CheckSwapPositionLimits` (0x63d93c).
use crate::pb::real::{Ext, cast};


use super::IrSwapForm;
use crate::logic::etf_derivs::swaps_ai::calc_swap_fixed_rate;
use crate::logic::finance::calc_entity_financials;
use crate::logic::swaps::{count_entity_swaps, find_free_swap_slot, sum_swap_notional};
use crate::logic::trading::prelude::*;
use crate::pb::string::mcase;
use crate::pb::numeric::fix;

/// `CurQuarter * 0.1! + CurYear` style dates (`fild q; fmul dword 0.1; fiadd y; fstp qword`: exact in
/// f64).
fn year_qtr(q: i32, y: i32) -> Ext {
    cast::<Ext, _>(q) * sf(0.1f32) + cast::<Ext, _>(y)
}

/// Port of CheckSwapPositionLimits (0x63d93c).
/// Whether the acting entity may add a swap of `notional` (millions): the position limit is the lesser
/// of 10× `NetWorth` and 10,000,000 × `ExchangeRate`; with existing swaps (total notional
/// `SumSwapNotional(ActiveEntity, 0)`) or a notional above limit + 1, the new total must stay within
/// limit + 10. Otherwise a "Excess Derivatives Risk" message explains why, control 989 gets the remaining
/// room ("No bids" below 101) and 0 is returned; else 1.
pub fn check_swap_position_limits(g: &mut Globals, ui: &mut dyn Ui, form: &mut IrSwapForm, notional: i64) -> i32 {
    let __content = crate::content::current();
    let mut result = 0;
    let entity_id = g.active_entity;
    let mut position_limit = to_quad(g.net_worth * Ext::from_f64(10.0));
    if g.exchange_rate * Ext::from_f64(10000000.0) < cast::<Ext, _>(position_limit) {
        position_limit = to_quad(g.exchange_rate * Ext::from_f64(10000000.0));
    }
    let current_notional = to_quad(sum_swap_notional(g, entity_id, 0));
    if current_notional != 0 || cast::<Ext, _>(position_limit) + Ext::from_f64(1.0) <= cast::<Ext, _>(notional) {
        let new_total = to_quad(cast::<Ext, _>(current_notional) + cast::<Ext, _>(notional));
        if (cast::<Ext, _>(new_total)).abs() <= cast::<Ext, _>(position_limit) + Ext::from_f64(10.0) {
            result = 1;
        } else {
            let mut msg_text = s(__content.string(11458));
            if g.active_entity < 11 {
                msg_text = msg_text
                    + __content.string(11459)
                    + __content.string(11460);
            } else if g.active_entity < 131 {
                msg_text = s(__content.string(11461));
                msg_text = msg_text
                    + __content.string(11462)
                    + __content.string(11463);
                if 85 < g.active_entity {
                    msg_text = replace(msg_text.as_bytes(), b"bank'", __content.bytes(11464));
                    msg_text = replace(msg_text.as_bytes(), b"banking", __content.bytes(2822));
                }
            } else {
                msg_text = msg_text
                    + &g.active_entity_name
                    + __content.string(11465)
                    + __content.string(11466);
            }
            if (g.active_entity < 11 || 130 < g.active_entity)
                && g.net_worth < g.exchange_rate * Ext::from_f64(50000.0)
                && g.exchange_rate * Ext::from_f64(99000.0) < cast::<Ext, _>(notional)
            {
                let mut risk_text = s(__content.string(11467));
                if 130 < g.active_entity {
                    risk_text = replace(risk_text.as_bytes(), __content.bytes(11468), __content.bytes(11469));
                }
                msg_text = msg_text + &risk_text;
            }
            let remaining_limit = to_quad(cast::<Ext, _>(position_limit) - (cast::<Ext, _>(current_notional)).abs());
            let max_text;
            if remaining_limit < 101 {
                max_text = s("No bids");
            } else {
                max_text = fmt_trim(cast::<Ext, _>(remaining_limit), "* ###,");
                msg_text = msg_text
                    + __content.string(11470)
                    + &max_text
                    + &g.currency_suffix
                    + ".";
            }
            form.notional = max_text;
            ui.msgbox(&msg_text, 8208, &s(__content.string(11471)));
        }
    } else {
        result = 1;
    }
    result
}

/// The dialog inputs as read by IRSwap_ValidateInputs.
struct SwapInputs {
    notional: i64,
    start_qtr: i32,
    start_year: i32,
    end_qtr: i32,
    end_year: i32,
}

/// Port of IRSwap_ValidateInputs (0x640707), GOSUB of IRSwap_CreateSwap.
/// Reads and checks the dialog inputs: rate type and position chosen; notional (control 989, `$ ,`
/// removed) between 100 × `ExchangeRate` and the position limit (+1); quarters 1..4 and years given;
/// start after the current quarter and, for more than 10,000 × `ExchangeRate`, within about a year; end
/// not before start (an end year before the start year is corrected to the start year when the start is
/// within 5 years: popup, `Err(1)`); end within 5.15 "years" (dates are `year + quarter / 10`; 5.75 in
/// Q4); for the short bond rate, before the short bond's maturity year. Invalid fields are corrected or
/// cleared in `form`. An error is shown (" [runtime resource wording] ") and `Err(1)` returned (the
/// GOSUB jumps to IRSwap_Exit with `result = 1`).
fn ir_swap_validate_inputs(g: &mut Globals, ui: &mut dyn Ui, form: &mut IrSwapForm) -> Result<SwapInputs, i32> {
    let __content = crate::content::current();
    // errMsg shares its stack slot with swapRec (still "" here)
    let mut err_msg = PbStr::new();
    let mut inp = SwapInputs { notional: 0, start_qtr: 0, start_year: 0, end_qtr: 0, end_year: 0 };
    if g.swap_rate_type.is_empty() {
        err_msg = s(__content.string(11472));
    }
    if !g.swap_position.is_empty() {
        let msg_text = form.notional.clone();
        inp.notional = to_quad(val(remove_any(msg_text.as_bytes(), b"$ ,").as_bytes()));
        let mut max_notional = to_quad(g.net_worth * Ext::from_f64(10.0));
        if g.exchange_rate * Ext::from_f64(10000000.0) < cast::<Ext, _>(max_notional) {
            max_notional = to_quad(g.exchange_rate * Ext::from_f64(10000000.0));
        }
        if inp.notional <= max_notional + 1 {
            if 1 <= inp.notional {
                if (cast::<Ext, _>(inp.notional)) < g.exchange_rate * Ext::from_f64(100.0) {
                    err_msg = s(__content.string(11473))
                        + __content.string(11474)
                        + &trim(g.currency_symbol.as_bytes())
                        + &fmt_trim(g.exchange_rate * Ext::from_f64(100.0), "* ###,")
                        + " "
                        + &g.million_prefix_lc
                        + "illion"
                        + &g.currency_suffix
                        + ".";
                    form.notional = fmt_trim(g.exchange_rate * Ext::from_f64(100.0), "* ###,");
                }
            } else {
                err_msg = s(__content.string(11475))
                    + &g.million_prefix_lc
                    + __content.string(11476)
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(g.exchange_rate * Ext::from_f64(100.0), "* ###,")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + ".)";
                if !(g.currency_code == "USA") {
                    err_msg = replace(err_msg.as_bytes(), b".)", __content.bytes(11477));
                }
                form.notional = fmt_trim(g.exchange_rate * Ext::from_f64(100.0), "* ###,");
            }
        } else {
            err_msg = s(__content.string(11478))
                + "\r"
                + "\r"
                + __content.string(11479)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(g.net_worth * Ext::from_f64(10.0), "* ###,")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + "); or"
                + "\r";
            if g.currency_code == "USA" {
                err_msg = err_msg + __content.string(11480);
            } else {
                err_msg = err_msg
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(g.exchange_rate * Ext::from_f64(10000000.0), "* ####,")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + __content.string(11481);
            }
            form.notional = fmt_trim(cast::<Ext, _>(max_notional), "* ###,");
        }
        inp.start_qtr = to_long(val(form.start_qtr.as_bytes()));
        inp.start_year = to_long(val(form.start_year.as_bytes()));
        inp.end_qtr = to_long(val(form.end_qtr.as_bytes()));
        inp.end_year = to_long(val(form.end_year.as_bytes()));
        let cur_date = year_qtr(g.cur_quarter, g.cur_year);
        let start_date = year_qtr(inp.start_qtr, inp.start_year);
        let mut end_date = year_qtr(inp.end_qtr, inp.end_year);
        if inp.start_qtr < 1 || inp.end_qtr < 1 || inp.start_year < 1 || inp.end_year < 1 {
            err_msg = s(__content.string(11482));
        }
        if start_date <= cur_date {
            err_msg = s(__content.string(11483));
        }
        if sf(1.01f32) < start_date - cur_date && g.exchange_rate * Ext::from_f64(10000.0) < cast::<Ext, _>(inp.notional) {
            err_msg = s(__content.string(11484))
                + __content.string(11485);
        }
        if end_date < start_date {
            if err_msg.is_empty() && inp.start_year - g.cur_year < 6 && inp.end_year < inp.start_year {
                let old_end_year = inp.end_year;
                inp.end_year = inp.start_year;
                end_date = year_qtr(inp.end_qtr, inp.end_year);
                let _ = end_date;
                form.end_year = str_trim(cast::<Ext, _>(inp.end_year));
                let mut m = s(__content.string(11486))
                    + &str_s(cast::<Ext, _>(old_end_year))
                    + " to"
                    + &str_s(cast::<Ext, _>(inp.end_year))
                    + __content.string(11487);
                show_quick_message(g, ui, &mut m);
                return Err(1);
            }
            if end_date < start_date {
                err_msg = s(__content.string(11488));
            }
        }
        if 4 < inp.start_qtr || 4 < inp.end_qtr {
            err_msg = s(__content.string(11489));
            if 4 < inp.start_qtr {
                form.start_qtr = PbStr::new();
            }
            if 4 < inp.end_qtr {
                form.end_qtr = PbStr::new();
            }
        }
        let term_years = end_date - cur_date;
        if sf(5.15f32) < term_years && err_msg.is_empty() {
            err_msg = s(__content.string(11490));
            if g.cur_quarter == 4 && term_years < sf(5.75f32) {
                err_msg = PbStr::new();
            }
        }
        if g.swap_rate_type == "S" {
            let maturity = mid(g.short_bond_desc.as_bytes(), 1, 4);
            if val(maturity.as_bytes()) <= fix(end_date) {
                err_msg = s(__content.string(11491))
                    + &maturity
                    + ").";
            }
        }
    }
    if g.swap_position.is_empty() {
        err_msg = s(__content.string(11492));
    }
    if err_msg.is_empty() {
        return Ok(inp);
    }
    ui.msgbox(&err_msg, 8208, &s(__content.string(11493)));
    Err(1)
}

/// Port of IRSwap_CheckCounterpartyConflict (0x6412f3), GOSUB of IRSwap_CreateSwap.
/// Drops the candidate `counterparty_id` (returns 0) when, with more than one swap on record, it is
/// already the long party of a swap and the player wants to be SHORT, or the short party and the player
/// wants LONG (scanning stops at the first such swap), or when it has any swap at all; and when it is
/// the fund 1595 (industry 71) with swaps of its own.
fn ir_swap_check_counterparty_conflict(g: &Globals, mut counterparty_id: i32) -> i32 {
    if Ext::from_f64(1.0) < val(g.swap_contracts[0].as_bytes()) {
        let swap_count = to_long(val(g.swap_contracts[0].as_bytes()));
        let mut slot = 1;
        while slot <= swap_count {
            let rec = g.swap_contracts[slot].as_bytes();
            if val(parse_any(rec, b"|", 1).as_bytes()) == cast::<Ext, _>(counterparty_id) && g.swap_position == "SHORT" {
                counterparty_id = 0;
                break;
            }
            if val(parse_any(rec, b"|", 2).as_bytes()) == cast::<Ext, _>(counterparty_id) && g.swap_position == "LONG" {
                counterparty_id = 0;
                break;
            }
            slot += 1;
        }
        if 0 < counterparty_id && 0 < count_entity_swaps(g, counterparty_id, 0) {
            counterparty_id = 0;
        }
    }
    if counterparty_id == 1595 && g.industry_num[counterparty_id] == 71 && sum_swap_notional(g, counterparty_id, 0) != Ext::from_f64(0.0) {
        counterparty_id = 0;
    }
    counterparty_id
}

/// Port of IRSwap_CreateSwap (0x63efd6) with its GOSUBs IRSwap_ValidateInputs (0x640707),
/// IRSwap_CheckCounterpartyConflict (0x6412f3) and IRSwap_Exit (0x641599, the common exit returning
/// `result`).
///
/// After [`ir_swap_validate_inputs`] and [`check_swap_position_limits`] (failures return 1), up to 2,000
/// random entities 11..1600 are tried as counterparty (the first try may be the fund 1595 with
/// probability 1/3): active, not the player's, without swaps, rating 7+ and net worth of 1,000 ×
/// `ExchangeRate` (`CalcEntityFinancials` of the candidate), an operating company only with twice the
/// notional in net worth; in tries 1..49 only brokerages (industry 37), then also banks (industry 1), then
/// industries 1..2, from try 200 anyone (companies only up to 10,000 × `ExchangeRate`); its total notional
/// must stay within 2× (5×/8×/10× for ratings 8/9/10, 2× for non-brokerage companies) its net worth and
/// 10,000,000 × `ExchangeRate`. None found: "No Takers Found", returns 0.
///
/// The fixed rate is `CalcSwapFixedRate` capped by the current rate (control 994: a LONG party receives
/// at most it, a SHORT one pays at least it). An offered rate equal to the current one is accepted at once
/// (popup); otherwise the counter-offer is shown (YES/NO; NO returns 0). The swap
/// `long|short|rate|type|start|end|notional` goes into a free slot (`FindFreeSwapSlot`, none: returns 0),
/// `SwapContracts(0)` counts it, a confirmation is shown, a news headline posted (unless the last one
/// mentions a swap) and `TurnActionCount` incremented. Returns 1 while the turn has actions left
/// (`TurnActionCount < 5` or solo mode), else 0.
pub fn ir_swap_create_swap(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, form: &mut IrSwapForm) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let inp = match ir_swap_validate_inputs(g, ui, form) {
        Ok(inp) => inp,
        Err(r) => return r,
    };
    let notional = inp.notional;
    if check_swap_position_limits(g, ui, form, notional) == 0 {
        return 1;
    }
    let mut counterparty_id = 0;
    for attempt in 1..=2000 {
        let mut candidate_id = rng.rnd_range(1600, 11);
        if attempt == 1 && g.industry_num[1595] == 71 && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
            candidate_id = 1595;
        }
        if g.entity_status[candidate_id] != 1 {
            continue;
        }
        if g.industry_num[candidate_id] == 71 && candidate_id != 1595 {
            continue;
        }
        if g.cur_player == g.corp_controller[candidate_id] {
            continue;
        }
        if 0 < count_entity_swaps(g, candidate_id, 0) {
            continue;
        }
        calc_entity_financials(g, ui, rng, candidate_id);
        if g.credit_rating_num[candidate_id] < 7 || g.net_worth < g.exchange_rate * Ext::from_f64(1000.0) {
            continue;
        }
        let cp_industry = g.industry_num[candidate_id];
        if !(candidate_id < 131 || cp_industry == 37 || cast::<Ext, _>(notional) <= g.net_worth * Ext::from_f64(2.0)) {
            continue;
        }
        if attempt < 2 && cp_industry == 71 {
            counterparty_id = candidate_id;
        } else if attempt < 50 {
            if cp_industry == 37 {
                counterparty_id = candidate_id;
            }
        } else if attempt < 100 {
            if cp_industry == 1 || cp_industry == 37 {
                counterparty_id = candidate_id;
            }
        } else if attempt < 200 {
            if cp_industry < 3 || cp_industry == 37 {
                counterparty_id = candidate_id;
            }
        } else {
            counterparty_id = candidate_id;
            if 130 < candidate_id && g.exchange_rate * Ext::from_f64(10000.0) < cast::<Ext, _>(notional) {
                counterparty_id = 0;
            }
        }
        if 10 < counterparty_id {
            counterparty_id = ir_swap_check_counterparty_conflict(g, counterparty_id);
        }
        if 10 < counterparty_id {
            let mut leverage_mult = 2;
            if g.credit_rating_num[counterparty_id] == 8 {
                leverage_mult = 5;
            }
            if g.credit_rating_num[counterparty_id] == 9 {
                leverage_mult = 8;
            }
            if g.credit_rating_num[counterparty_id] == 10 {
                leverage_mult = 10;
            }
            if 130 < counterparty_id && cp_industry != 37 {
                leverage_mult = 2;
            }
            let total = to_quad(sum_swap_notional(g, counterparty_id, 0) + cast::<Ext, _>(notional));
            if cast::<Ext, _>(total) <= g.net_worth * cast::<Ext, _>(leverage_mult) && cast::<Ext, _>(total) <= g.exchange_rate * Ext::from_f64(10000000.0) {
                break;
            }
            counterparty_id = 0;
        }
    }
    if counterparty_id == 0 {
        ui.msgbox(&s(__content.string(11494)), 8256, &s(__content.string(11495)));
        return 0;
    }
    let mut msg_text = str_trim(cast::<Ext, _>(g.active_entity));
    let mut party_label = s("you");
    if 10 < g.active_entity {
        party_label = g.ent_symbol[g.active_entity].clone();
    }
    let mut notional_text = str_trim(cast::<Ext, _>(counterparty_id));
    if g.swap_position == "SHORT" {
        std::mem::swap(&mut msg_text, &mut notional_text);
    }
    let current_rate_text = form.current_rate.clone();
    let start_date_text = str_trim(cast::<Ext, _>(inp.start_year)) + "." + &str_trim(cast::<Ext, _>(inp.start_qtr));
    let end_date_text = str_trim(cast::<Ext, _>(inp.end_year)) + "." + &str_trim(cast::<Ext, _>(inp.end_qtr));
    let mut rate_desc = str_trim(cast::<Ext, _>(notional));
    let mut swap_rec = msg_text.clone()
        + "|"
        + &notional_text
        + "|"
        + "XXXX"
        + "|"
        + &g.swap_rate_type
        + "|"
        + &start_date_text
        + "|"
        + &end_date_text
        + "|"
        + &rate_desc;
    let active = g.active_entity;
    let mut offered_rate = calc_swap_fixed_rate(g, rng, &swap_rec, active);
    if g.swap_position == "SHORT" {
        if val(offered_rate.as_bytes()) < val(current_rate_text.as_bytes()) {
            offered_rate = current_rate_text.clone();
        }
    } else if val(current_rate_text.as_bytes()) < val(offered_rate.as_bytes()) {
        offered_rate = current_rate_text.clone();
    }
    rate_desc = s(__content.string(11412));
    if g.swap_rate_type == "L" {
        rate_desc = s(__content.string(11496));
    }
    if g.swap_rate_type == "S" {
        rate_desc = s(__content.string(11497));
    }
    notional_text = s(__content.string(11498));
    if g.swap_position == "LONG" {
        notional_text = s("receive");
    }
    if val(offered_rate.as_bytes()) == val(current_rate_text.as_bytes()) {
        let mut m = s(__content.string(11499)) + &g.ent_name[counterparty_id] + "!";
        show_quick_message(g, ui, &mut m);
    } else {
        let offer_title = s(__content.string(11500)) + &g.ent_name[counterparty_id];
        msg_text = g.ent_name[counterparty_id].clone()
            + __content.string(11501)
            + __content.string(11502)
            + "\r"
            + "\r"
            + __content.string(11503)
            + __content.string(11504)
            + &party_label
            + " will "
            + &notional_text
            + __content.string(11505)
            + &trim(offered_rate.as_bytes())
            + __content.string(11506)
            + __content.string(11507)
            + &trim(current_rate_text.as_bytes())
            + "% (the "
            + &rate_desc
            + ")."
            + "\r"
            + "\r";
        let ind_a = g.industry_num[active];
        let ind_c = g.industry_num[counterparty_id];
        if 10 < active && (ind_a <= 2 || ind_a == 37) && (ind_c <= 2 || ind_c == 37) {
            let mut industry_label = lcase(g.ind_name[cast::<i32, _>(ind_a)].as_bytes(), CaseMode::Ansi);
            if ind_a == 37 {
                industry_label = s(__content.string(9071));
            }
            let base = cast::<Ext, _>(g.base_interest_rate);
            if base <= sf(5.25f32) || sf(8.75f32) <= base {
                let mut rate_level_word = s("high");
                if base < sf(5.3f32) {
                    rate_level_word = s("low");
                }
                msg_text = msg_text
                    + __content.string(11508)
                    + __content.string(11509)
                    + &industry_label
                    + __content.string(11510)
                    + __content.string(11511)
                    + &rate_level_word
                    + __content.string(11512)
                    + "\r"
                    + "\r";
            } else {
                msg_text = msg_text
                    + __content.string(11513)
                    + __content.string(11514)
                    + &industry_label
                    + __content.string(2223)
                    + "\r"
                    + "\r";
            }
        }
        let mut term_prefix = s("s ") + &start_date_text + __content.string(11515);
        if start_date_text == end_date_text {
            term_prefix = s(" ");
        }
        msg_text = msg_text
            + __content.string(11516)
            + __content.string(11517)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(cast::<Ext, _>(notional), "* #######,")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "."
            + "\r"
            + "\r"
            + __content.string(11518)
            + &term_prefix
            + &end_date_text
            + "."
            + "\r"
            + "\r"
            + __content.string(11519)
            + &g.ent_name[counterparty_id]
            + __content.string(11520);
        let answer = ui.msgbox(&msg_text, 8228, &offer_title);
        if answer == 7 {
            return 0;
        }
    }
    swap_rec = replace(swap_rec.as_bytes(), b"XXXX", trim(offered_rate.as_bytes()).as_bytes());
    let free_slot = find_free_swap_slot(g, ui);
    if free_slot == 0 {
        return 0;
    }
    g.swap_contracts[free_slot] = swap_rec;
    let count = to_long(val(g.swap_contracts[0].as_bytes())) + 1;
    g.swap_contracts[0] = str_trim(cast::<Ext, _>(count));
    notional_text = trim(g.currency_symbol.as_bytes())
        + &fmt_trim(cast::<Ext, _>(notional), "* #####,")
        + " "
        + &g.million_prefix_lc
        + "il."
        + &g.currency_suffix;
    msg_text = s(__content.string(11521))
        + "\r"
        + "\r"
        + __content.string(11522)
        + &party_label
        + " and "
        + &g.ent_name[counterparty_id];
    if right(msg_text.as_bytes(), 1) == "." {
        msg_text = msg_text + "\r" + "\r";
    } else {
        msg_text = msg_text + "." + "\r" + "\r";
    }
    msg_text = msg_text
        + __content.string(11523)
        + &right(start_date_text.as_bytes(), 1)
        + " of "
        + &mid(start_date_text.as_bytes(), 1, 4)
        + __content.string(11524)
        + __content.string(11525)
        + &right(end_date_text.as_bytes(), 1)
        + __content.string(11526)
        + &mid(end_date_text.as_bytes(), 1, 4)
        + "."
        + "\r"
        + "\r"
        + __content.string(11527)
        + &rate_desc
        + "."
        + "\r"
        + "\r"
        + __content.string(11528)
        + &notional_text
        + "\r"
        + "\r"
        + __content.string(11529)
        + &offered_rate
        + __content.string(11530)
        + __content.string(11531)
        + __content.string(11532)
        + "\r"
        + "\r"
        + &ucase(g.active_entity_name.as_bytes(), CaseMode::Ansi)
        + __content.string(11533)
        + &mcase(g.swap_position.as_bytes(), CaseMode::Ansi)
        + __content.string(11534);
    ui.msgbox(&msg_text, 8256, &(s(__content.string(11535)) + &g.active_entity_name));
    if instr(1, g.news_headline[60].as_bytes(), b"swap") == 0 {
        g.news_msg = g.ent_symbol[active].clone() + " does " + &notional_text + __content.string(11536);
        if len(g.news_msg.as_bytes()) < 72 {
            g.news_msg = g.news_msg.clone() + " with " + &g.ent_symbol[counterparty_id];
        }
        post_news(g, ui, 0, 0);
    }
    g.turn_action_count = g.turn_action_count + 1;
    if g.turn_action_count < 5 || g.solo_human_mode != 0 {
        return 1;
    }
    0
}

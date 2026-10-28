//! Interest-rate swaps: the `SwapContracts` record list and the routines that settle, value, cancel,
//! transfer, count and sort swaps.
//!
//! `SwapContracts(1 TO 1000)` records are `long|short|fixedRate|rateType|startDate|endDate|notional`:
//! the LONG party receives the fixed rate and pays the floating rate (`rateType` "P" prime rate, "L"
//! long-bond yield, "S" short-bond yield), the SHORT party the reverse; dates are `year.quarter`
//! ("2025.3"), the notional is in millions. Element 0 holds the record count as a string (maintained by
//! `PackHoldingArrays`). `SwapPnLQtr(0 TO 1600)` accumulates each entity's swap P&L of the quarter.
//!
//! Numeric conventions as in `logic::finance`: SINGLE pool constants are written `0.1f32 as f64`,
//! CURRENCY locals use [`Cur`], every float→integer store goes through `to_long`/`to_quad`, and the
//! operand order follows the machine code ("asm:" comments mark where the decompiled view differs).
//!
//! Error handling: every routine here has an `ON ERROR` handler that only calls ShowRuntimeError
//! (0x46ea48) and leaves the routine; the error paths (e.g. a record with party id 0 → error 9 on
//! `EntityStatus(0)`) are not reproduced — out-of-range subscripts read the arrays' dummy element.

// Statements keep the original BASIC shape (`x = x + y`, `a < 1 OR 5 < a`) on purpose; negated float
// comparisons (`!(a > b)`) mirror the x87 branch taken for unordered (NaN) operands.
#![allow(
    clippy::manual_range_contains,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::if_same_then_else,
    clippy::neg_multiply
)]
use crate::pb::real::{Ext, cast};


use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::pack_holding_arrays_with_news as pack_holding_arrays;
use crate::logic::news::{is_in_recent_headlines, post_news};
use crate::logic::report::show_quick_message;
use crate::pb::numeric::format;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{len, ltrim, mid, parse_any, parse_count_any, replace, rtrim};
use crate::pb::numeric::{ceil, to_long, to_quad, Cur};
use crate::state::Globals;
use crate::ui::Ui;

/// `PARSE$(rec, ANY "|", n)`.
fn field(rec: &PbStr, n: i32) -> PbStr {
    parse_any(rec.as_bytes(), b"|", n)
}

/// `VAL(PARSE$(rec, ANY "|", n))`.
fn fval(rec: &PbStr, n: i32) -> Ext {
    val(field(rec, n).as_bytes())
}

/// `RTRIM$(LTRIM$(s))`.
fn trim(s: &[u8]) -> PbStr {
    rtrim(ltrim(s).as_bytes())
}

/// `PackHoldingArrays` (0x49117c) including recapture news.
fn pack_holdings(g: &mut Globals, ui: &mut dyn Ui) {
    pack_holding_arrays(g, ui);
}

/// Port of SwapPnLAccumulator (0x6477eb).
/// `op = 1`: add `amount` to `SwapPnLQtr(entityId)` (QUAD, rounded); `op = 2`: return it;
/// `entityId = 0, op = 0`: clear the whole table. Returns 0 except for `op = 2`.
///
/// The routine starts with `DIM SwapPnLQtr(0 TO 1600) AS QUAD` (a no-op once the global array exists)
/// and clears it with `ERASE` (mode 4 = reset of a global array: the elements are zeroed).
pub fn swap_pnl_accumulator(g: &mut Globals, entity_id: i32, op: i32, amount: Ext) -> Ext {
    let mut result: i64 = 0;
    if entity_id == 0 && op == 0 {
        g.swap_pn_l_qtr.reset();
        return Ext::from_f64(0.0);
    }
    if op == 1 {
        // asm: fld amount; fild SwapPnLQtr(e); faddp; fistp qword
        g.swap_pn_l_qtr[entity_id] = to_quad(cast::<Ext, _>(g.swap_pn_l_qtr[entity_id]) + amount);
    } else if op == 2 {
        result = g.swap_pn_l_qtr[entity_id];
    }
    cast::<Ext, _>(result)
}

/// Port of FindFreeSwapSlot (0x647639).
/// Packs the record lists (`PackHoldingArrays`) and returns the first empty `SwapContracts` slot in
/// 1..995, or 0 after an error MSGBOX when there is none.
pub fn find_free_swap_slot(g: &mut Globals, ui: &mut dyn Ui) -> i32 {
    let __content = crate::content::current();
    pack_holdings(g, ui);
    let mut free_slot = 0;
    for slot in 1..=995 {
        if g.swap_contracts[slot].is_empty() {
            free_slot = slot;
            break;
        }
    }
    if free_slot == 0 {
        let msg_text = PbStr::from(
            __content.string(11204),
        );
        ui.msgbox(&msg_text, 8208, &PbStr::from(__content.string(11205)));
    }
    free_slot
}

/// Port of SortSwapContracts (0x6479fe).
/// Packs the record lists, then bubble-sorts `SwapContracts(1..count)` by start date (field 5) and, for
/// equal start dates, end date (field 6). Stops at an empty record or after 10,000 comparisons.
pub fn sort_swap_contracts(g: &mut Globals, ui: &mut dyn Ui) {
    pack_holdings(g, ui);
    if !(val(g.swap_contracts[0].as_bytes()) >= Ext::from_f64(2.0)) {
        return;
    }
    let count = to_long(val(g.swap_contracts[0].as_bytes()));
    let mut sorted = 0;
    let mut iter_guard = 0;
    while sorted == 0 {
        sorted = 1;
        for idx in 2..=count {
            iter_guard += 1;
            if iter_guard == 10000 {
                return;
            }
            let prev_idx = idx - 1;
            let prev_rec = g.swap_contracts[prev_idx].clone();
            let next_rec = g.swap_contracts[idx].clone();
            if prev_rec.is_empty() || next_rec.is_empty() {
                return;
            }
            let (p5, n5) = (fval(&prev_rec, 5), fval(&next_rec, 5));
            if !(p5 <= n5) {
                swap_records(g, idx, prev_idx);
                sorted = 0;
            } else if p5 == n5 {
                if !(fval(&prev_rec, 6) <= fval(&next_rec, 6)) {
                    swap_records(g, idx, prev_idx);
                    sorted = 0;
                }
            }
        }
    }
}

/// `SWAP SwapContracts(a), SwapContracts(b)`.
fn swap_records(g: &mut Globals, a: i32, b: i32) {
    let t = g.swap_contracts[a].clone();
    g.swap_contracts[a] = g.swap_contracts[b].clone();
    g.swap_contracts[b] = t;
}

/// Port of CountEntitySwaps (0x648575).
/// Number of swaps `entityId` is a party to (scanning slots 1..VAL(SwapContracts(0))); 0 if the list is
/// empty. With `posFilter` 1 or 2 the routine also counts long/short sides starting at 1 for the chosen
/// side and returns 0 when exactly one swap was found and both side counters are 1 (i.e. the single swap
/// is on the *other* side than `posFilter`).
pub fn count_entity_swaps(g: &Globals, entity_id: i32, pos_filter: i32) -> i32 {
    if val(g.swap_contracts[0].as_bytes()) == Ext::from_f64(0.0) {
        return 0;
    }
    let self_id = entity_id;
    let (mut long_count, mut short_count, mut count) = (0, 0, 0);
    if pos_filter == 1 {
        long_count = 1;
    } else if pos_filter == 2 {
        short_count = 1;
    }
    let n = to_long(val(g.swap_contracts[0].as_bytes()));
    for slot in 1..=n {
        let swap_rec = g.swap_contracts[slot].clone();
        if swap_rec.is_empty() {
            continue;
        }
        let long_party = to_long(fval(&swap_rec, 1));
        let short_party = to_long(fval(&swap_rec, 2));
        if self_id == long_party || self_id == short_party {
            if 0 < pos_filter {
                if self_id == long_party {
                    long_count += 1;
                }
                if self_id == short_party {
                    short_count += 1;
                }
            }
            count += 1;
        }
    }
    if 0 < pos_filter && count == 1 && long_count == 1 && short_count == 1 {
        count = 0;
    }
    count
}

/// Port of GetSoleSwapCounterparty (0x6487c6).
/// The counterparty of `entityId`'s only swap (slots 1..1000), or 0 if it has none or several.
pub fn get_sole_swap_counterparty(g: &Globals, entity_id: i32) -> i32 {
    let self_id = entity_id;
    let (mut count, mut counterparty_id) = (0, 0);
    for slot in 1..=1000 {
        let swap_rec = g.swap_contracts[slot].clone();
        if swap_rec.is_empty() {
            continue;
        }
        let long_party = to_long(fval(&swap_rec, 1));
        let short_party = to_long(fval(&swap_rec, 2));
        if self_id == long_party || self_id == short_party {
            count += 1;
            counterparty_id = if self_id == long_party { short_party } else { long_party };
        }
    }
    if count != 1 {
        counterparty_id = 0;
    }
    counterparty_id
}

/// Port of SumSwapNotional (0x646745).
/// Total notional of `entityId`'s swaps (slots 1..1000); with `netMode <> 0` long minus short.
pub fn sum_swap_notional(g: &Globals, entity_id: i32, net_mode: i32) -> Ext {
    let self_id = entity_id;
    let mut total: i64 = 0;
    for slot in 1..=1000 {
        let swap_rec = g.swap_contracts[slot].clone();
        if swap_rec.is_empty() {
            continue;
        }
        let long_party = to_long(fval(&swap_rec, 1));
        let short_party = to_long(fval(&swap_rec, 2));
        if self_id == long_party || self_id == short_party {
            if net_mode == 0 {
                total = to_quad(fval(&swap_rec, 7) + cast::<Ext, _>(total));
            } else if self_id == long_party {
                total = to_quad(fval(&swap_rec, 7) + cast::<Ext, _>(total));
            } else if self_id == short_party {
                // asm: VAL; fild total; fsubrp → total - VAL
                total = to_quad(cast::<Ext, _>(total) - fval(&swap_rec, 7));
            }
        }
    }
    cast::<Ext, _>(total)
}

/// Port of CalcSwapEstQtrProfit (0x645d1c).
/// Estimated quarterly profit of a swap position at the current floating rate: sets
/// `TmpExt1 = receive - pay` (LONG receives `fixedRate`, SHORT the floating rate; each leg is
/// `0.0025 * rate * notional`) and returns the floating rate as text (`FORMAT$(rate, "* ##.00")`,
/// trimmed; "" for an unknown rate type).
pub fn calc_swap_est_qtr_profit(
    g: &mut Globals,
    rate_type: &PbStr,
    fixed_rate: Cur,
    position: &PbStr,
    notional: i64,
) -> PbStr {
    g.tmp_ext1 = Ext::from_f64(0.0);
    let mut float_rate_text = PbStr::new();
    if *rate_type == "P" {
        float_rate_text = PbStr::from(format(cast::<Ext, _>(g.prime_rate), b"* ##.00"));
    } else if *rate_type == "L" {
        float_rate_text = PbStr::from(format(cast::<Ext, _>(g.long_bond_yield), b"* ##.00"));
    } else if *rate_type == "S" {
        float_rate_text = PbStr::from(format(cast::<Ext, _>(g.short_bond_yield), b"* ##.00"));
    }
    float_rate_text = trim(float_rate_text.as_bytes());
    let c = cast::<Ext, _>(0.0025f32);
    let (receive_amt, pay_amt);
    if *position == "LONG" {
        receive_amt = c * fixed_rate.get() * cast::<Ext, _>(notional);
        pay_amt = c * val(float_rate_text.as_bytes()) * cast::<Ext, _>(notional);
    } else {
        receive_amt = c * val(float_rate_text.as_bytes()) * cast::<Ext, _>(notional);
        pay_amt = c * fixed_rate.get() * cast::<Ext, _>(notional);
    }
    g.tmp_ext1 = receive_amt - pay_amt;
    float_rate_text
}

/// Port of CalcSwapTerminationFee (0x643336).
/// Fee `entityId` must pay to terminate swap `slot`: 0.5% of the notional per remaining year (rounded
/// up, only when more than a year is left), at least twice the estimated quarterly loss; in the swap's
/// final quarter a share of the estimated quarterly profit (or 0.1% of the notional when it is 0);
/// at least 1.
pub fn calc_swap_termination_fee(g: &mut Globals, entity_id: i32, slot: i32) -> Ext {
    let swap_rec = g.swap_contracts[slot].clone();
    let self_id = entity_id;
    let rate_type = field(&swap_rec, 4);
    let fixed_rate = Cur::from_f64(fval(&swap_rec, 3));
    let mut position = PbStr::from("LONG");
    if fval(&swap_rec, 2) == cast::<Ext, _>(self_id) {
        position = PbStr::from("SHORT");
    }
    let notional = to_quad(fval(&swap_rec, 7));
    let _profit_text = calc_swap_est_qtr_profit(g, &rate_type, fixed_rate, &position, notional);
    let est_qtr_profit = to_quad(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    let min_fee = (cast::<Ext, _>(est_qtr_profit) * Ext::from_f64(2.0)) * -Ext::from_f64(1.0);
    let mut notional_fee = cast::<Ext, _>(notional) * (cast::<Ext, _>(0.005f32));
    // asm: curDate is a DOUBLE: (CurQuarter * 0.1!) + CurYear
    let cur_date = cast::<Ext, _>(g.cur_quarter) * (cast::<Ext, _>(0.1f32)) + cast::<Ext, _>(g.cur_year);
    if fval(&swap_rec, 6) - cur_date > Ext::from_f64(1.0) {
        let years_left = ceil(fval(&swap_rec, 6) - cur_date);
        notional_fee = years_left * notional_fee;
    }
    let mut fee = notional_fee;
    if min_fee > notional_fee {
        fee = min_fee;
    }
    if trim(&format(cur_date, b"####.#")) == trim(field(&swap_rec, 6).as_bytes()) {
        if (cast::<Ext, _>(est_qtr_profit)).abs() > Ext::from_f64(0.0) {
            fee = cast::<Ext, _>(est_qtr_profit) * (cast::<Ext, _>(0.1f32));
            if g.sim_co_count < 1000 {
                fee = cast::<Ext, _>(est_qtr_profit) * (cast::<Ext, _>(0.2f32));
            }
            if g.sim_co_count < 500 {
                fee = cast::<Ext, _>(est_qtr_profit) * (cast::<Ext, _>(0.3f32));
            }
            if (cast::<Ext, _>(est_qtr_profit)) < Ext::from_f64(0.0) {
                fee = Ext::from_f64(2.0) * fee;
                if fee < cast::<Ext, _>(notional) * (cast::<Ext, _>(0.0015f32)) {
                    fee = cast::<Ext, _>(notional) * (cast::<Ext, _>(0.0015f32));
                    if 750 < g.sim_co_count {
                        fee = Ext::from_f64(0.5) * fee;
                    }
                }
            }
        } else {
            fee = cast::<Ext, _>(notional) * (cast::<Ext, _>(0.001f32));
        }
    }
    if fee < Ext::from_f64(1.0) {
        fee = Ext::from_f64(1.0);
    }
    fee
}

/// Port of CancelEntitySwaps (0x6428b0).
/// Ends every swap `entityId` is a party to (slots 1..1000).
///
/// * `|cancelMode| = 1` (bankruptcy): the records are cleared and `NewsMsg` set to "All interest rate
///   swaps of <sym> [runtime resource wording] tion" (`-1`: "<sym> is insolvent; …") — posted
///   once after the loop.
/// * otherwise (taxable liquidation): with `checkOnly <> 0` returns 1 at the first swap found. Else the
///   user is asked (MSGBOX YES/NO) to terminate each swap, without fee when both parties have the same
///   controller, else paying `CalcSwapTerminationFee` (borrowing a shortfall, PORT-TODO); NO returns 1.
///
/// After the loop: `PostNews(0, 0)` and `PackHoldingArrays`. Returns 0 unless stopped early.
pub fn cancel_entity_swaps(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
    cancel_mode: i32,
    check_only: i32,
) -> i32 {
    let __content = crate::content::current();
    let self_id = entity_id;
    let ent_name = g.ent_name[self_id].clone();
    let cr = "\r";
    for slot in 1..=1000 {
        let mut side = 0;
        if g.swap_contracts[slot].is_empty() {
            continue;
        }
        let swap_rec = g.swap_contracts[slot].clone();
        let long_party = to_long(fval(&swap_rec, 1));
        let short_party = to_long(fval(&swap_rec, 2));
        if self_id == long_party {
            side = 1;
        }
        if self_id == short_party {
            side = 2;
        }
        if !(0 < side) {
            continue;
        }
        if (cast::<Ext, _>(cancel_mode)).abs() == Ext::from_f64(1.0) {
            g.swap_contracts[slot] = PbStr::new();
            g.news_msg = PbStr::from(__content.string(11206))
                + &g.ent_symbol[self_id]
                + __content.string(11207);
            if cancel_mode == -1 {
                g.news_msg =
                    g.ent_symbol[self_id].clone() + __content.string(11208);
            }
            continue;
        }
        if check_only != 0 {
            return 1;
        }
        let other_party = if side == 1 { short_party } else { long_party };
        if g.corp_controller[self_id] == g.corp_controller[other_party] {
            let msg_text = ent_name.clone()
                + __content.string(11209)
                + "with "
                + &g.ent_name[other_party]
                + __content.string(11210)
                + cr
                + cr
                + __content.string(11211)
                + cr
                + cr
                + __content.string(11212);
            let answer = ui.msgbox(&msg_text, 8228, &PbStr::from(__content.string(11213)));
            if answer == 7 {
                return 1;
            }
            g.swap_contracts[slot] = PbStr::new();
            let mut msg_text = PbStr::from(__content.string(11214))
                + &g.ent_name[other_party]
                + " and "
                + &g.ent_name[self_id]
                + " is "
                + __content.string(11215);
            show_quick_message(g, ui, &mut msg_text);
        } else {
            let fee_raw = calc_swap_termination_fee(g, self_id, slot);
            let term_fee = to_quad(fee_raw);
            let msg_text = ent_name.clone()
                + __content.string(11209)
                + "with "
                + &g.ent_name[other_party]
                + __content.string(11210)
                + cr
                + cr
                + __content.string(11216)
                + &ent_name
                + __content.string(11217)
                + __content.string(11218)
                + &trim(g.currency_symbol.as_bytes())
                + &trim(&format(cast::<Ext, _>(term_fee), b"* ####,"))
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + "."
                + cr
                + cr
                + __content.string(11219);
            let answer = ui.msgbox(&msg_text, 8228, &PbStr::from(__content.string(11213)));
            if answer == 7 {
                return 1;
            }
            if cast::<Ext, _>(term_fee) > g.t_bills[self_id] + g.cash[self_id] {
                let mut shortfall = cast::<Ext, _>(term_fee) - (g.t_bills[self_id] + g.cash[self_id]);
                calc_entity_financials(g, ui, rng, self_id);
                let answer = crate::logic::trading::borrow_to_cover_purchase(g, ui, &mut shortfall, fee_raw, 0);
                if answer == 7 {
                    return 1;
                }
            }
            g.cash[self_id] = g.cash[self_id] - cast::<Ext, _>(term_fee);
            g.accrued_pl[self_id] = g.accrued_pl[self_id] - cast::<Ext, _>(term_fee);
            g.cash[other_party] = cast::<Ext, _>(term_fee) + g.cash[other_party];
            g.accrued_pl[other_party] = cast::<Ext, _>(term_fee) + g.accrued_pl[other_party];
            g.swap_contracts[slot] = PbStr::new();
            g.news_msg = g.ent_symbol[self_id].clone()
                + " pays "
                + &g.ent_symbol[other_party]
                + " "
                + &trim(&format(cast::<Ext, _>(term_fee), b"* ###,"))
                + " "
                + &g.million_prefix_lc
                + __content.string(11220);
            post_news(g, ui, 0, 0);
        }
    }
    post_news(g, ui, 0, 0);
    pack_holdings(g, ui);
    0
}

/// Port of TransferEntitySwaps (0x6460e0).
/// Moves every swap of `fromId` to `toId` (BYVAL args): records naming both are deleted, the others are
/// rewritten with `toId` in place of `fromId`. If any moved: news " [runtime resource wording] <from>
/// [runtime resource wording] <to>" and `SwapPnLQtr(from)` is added to `SwapPnLQtr(to)` (not cleared).
/// Ends with `PackHoldingArrays`.
pub fn transfer_entity_swaps(g: &mut Globals, ui: &mut dyn Ui, from_id: i32, to_id: i32) {
    let __content = crate::content::current();
    let old_id = from_id;
    let new_id = to_id;
    let mut any_moved = 0;
    for slot in 1..=1000 {
        if g.swap_contracts[slot].is_empty() {
            continue;
        }
        let swap_rec = g.swap_contracts[slot].clone();
        let mut long_party = to_long(fval(&swap_rec, 1));
        let mut short_party = to_long(fval(&swap_rec, 2));
        if !(old_id == long_party || old_id == short_party) {
            continue;
        }
        if new_id == long_party || new_id == short_party {
            g.swap_contracts[slot] = PbStr::new();
            continue;
        }
        any_moved = 1;
        let fixed_rate_text = field(&swap_rec, 3);
        let rate_type = field(&swap_rec, 4);
        let start_date_text = field(&swap_rec, 5);
        let end_date_text = field(&swap_rec, 6);
        let notional_text = field(&swap_rec, 7);
        if old_id == long_party {
            long_party = new_id;
        }
        if old_id == short_party {
            short_party = new_id;
        }
        let long_text = trim(str_num(cast::<Ext, _>(long_party), 15).as_bytes());
        let short_text = trim(str_num(cast::<Ext, _>(short_party), 15).as_bytes());
        g.swap_contracts[slot] = long_text
            + "|"
            + &short_text
            + "|"
            + &fixed_rate_text
            + "|"
            + &rate_type
            + "|"
            + &start_date_text
            + "|"
            + &end_date_text
            + "|"
            + &notional_text;
    }
    if any_moved == 1 {
        g.news_msg = PbStr::from(__content.string(11206))
            + &g.ent_symbol[old_id]
            + __content.string(9440)
            + &g.ent_symbol[new_id];
        post_news(g, ui, 0, 0);
        let pnl = swap_pnl_accumulator(g, old_id, 2, Ext::from_f64(0.0));
        swap_pnl_accumulator(g, new_id, 1, pnl);
    }
    pack_holdings(g, ui);
}

/// Port of ProcessSwapPayments (0x63ddc0).
/// Walks `SwapContracts(1 .. count + 100)` (at most 1000): drops malformed (< 6 fields), expired, invalid
/// ("S" swaps ending after the short bond matures) and orphaned (a party not active) records, and for
/// every running swap computes the quarterly net payment to the LONG party,
/// `0.25 * (fixed - floating) * notional / 100` (the floating rate is a CURRENCY local that keeps its
/// previous value for an unknown rate type).
///
/// * `entityId = 0` (quarterly settlement): clears `SwapPnLQtr`, caps payments by the payer's net worth
///   (plus [runtime resource wording] non-banks) — a payer rated below 2 has all its swaps canceled
///   ([`cancel_entity_swaps`] `(payer, -1, 2)`), others "default" with news —, moves cash and accrued P&L,
///   accumulates `SwapPnLQtr`, deletes swaps ending this quarter (news for slot 1 only), and may report one
///   randomly picked computer-controlled party's swap P&L in the news. Returns 0.
/// * `entityId > 0` (estimate): no payments; returns `entityId`'s net payments of the quarter (QUAD), but
///   0 for ids above `SimCoCount` unless `modeFlag = "R"`.
///
/// After a deletion the record list is packed (`PackHoldingArrays`) and the same slot is visited again.
pub fn process_swap_payments(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
    mode_flag: &PbStr,
) -> Ext {
    let __content = crate::content::current();
    let ent = entity_id;
    let mut result: i64 = 0;
    let mut reported_pnl: i64 = 0;
    let mut report_entity = 0;
    let mut float_rate = Cur(0); // persists across records (see doc)
    // asm: CURRENCY curDate = 0.1! * CurQuarter + CurYear
    let cur_date = Cur::from_f64((cast::<Ext, _>(0.1f32)) * cast::<Ext, _>(g.cur_quarter) + cast::<Ext, _>(g.cur_year));
    if ent == 0 {
        swap_pnl_accumulator(g, 0, 0, Ext::from_f64(0.0));
    }
    let mut max_slot = to_long(val(g.swap_contracts[0].as_bytes()) + Ext::from_f64(100.0));
    if 1000 < max_slot {
        max_slot = 1000;
    }
    let mut slot = 1;
    while slot <= max_slot {
        'rec: {
            if g.swap_contracts[slot].is_empty() {
                break 'rec;
            }
            let swap_rec = g.swap_contracts[slot].clone();
            if parse_count_any(swap_rec.as_bytes(), b"|") < 6 {
                g.swap_contracts[slot] = PbStr::new();
                break 'rec;
            }
            let short_bond_year = to_long(val(mid(g.short_bond_desc.as_bytes(), 1, 4).as_bytes()));
            let end_date = cast::<f32, _>(fval(&swap_rec, 6)); // SINGLE local
            let delete = if field(&swap_rec, 4) == "S" && (cast::<Ext, _>(short_bond_year)) < cast::<Ext, _>(end_date) {
                true
            } else {
                !(cur_date.get() <= cast::<Ext, _>(end_date))
            };
            if delete {
                g.swap_contracts[slot] = PbStr::new();
                pack_holdings(g, ui);
                if 0 < slot {
                    slot -= 1;
                }
                break 'rec;
            }
            if !(fval(&swap_rec, 5) <= cur_date.get()) {
                break 'rec; // not started yet
            }
            let long_party = to_long(fval(&swap_rec, 1));
            let short_party = to_long(fval(&swap_rec, 2));
            if !(g.entity_status[long_party] == 1 && g.entity_status[short_party] == 1) {
                g.swap_contracts[slot] = PbStr::new();
                pack_holdings(g, ui);
                if 0 < slot {
                    slot -= 1;
                }
                break 'rec;
            }
            if 0 < ent {
                if long_party != ent && short_party != ent {
                    break 'rec;
                }
                report_entity = ent;
            }
            if report_entity == 0 {
                report_entity = long_party;
                if rng.rnd() * Ext::from_f64(2.0) < Ext::from_f64(1.0) && 10 < g.corp_controller[report_entity] {
                    report_entity = short_party;
                }
                if 10 < g.corp_controller[report_entity]
                    || g.cur_player == report_entity
                    || g.sel_entity == report_entity
                {
                    report_entity = 0;
                }
            }
            let notional = to_quad(fval(&swap_rec, 7));
            let fixed_rate = Cur::from_f64(fval(&swap_rec, 3));
            let rate_type = field(&swap_rec, 4);
            if rate_type == "P" {
                float_rate = Cur::from_f64(cast::<Ext, _>(g.prime_rate));
            } else if rate_type == "L" {
                float_rate = Cur::from_f64(cast::<Ext, _>(g.long_bond_yield));
            } else if rate_type == "S" {
                float_rate = Cur::from_f64(cast::<Ext, _>(g.short_bond_yield));
            }
            // asm: (0.01! * fixed) * notional - (0.01! * float) * notional
            let c = cast::<Ext, _>(0.01f32);
            let mut payment = c * fixed_rate.get() * cast::<Ext, _>(notional) - c * float_rate.get() * cast::<Ext, _>(notional);
            payment = (cast::<Ext, _>(0.25f32)) * payment;
            if ent == 0 {
                if payment > Ext::from_f64(0.0) && 10 < short_party {
                    calc_entity_financials(g, ui, rng, short_party);
                    if 85 < short_party {
                        g.net_worth = g.contingency_reserve[short_party] + g.net_worth;
                    }
                    if g.net_worth < payment {
                        let mut capped_payment = (cast::<Ext, _>(0.9f32)) * g.net_worth;
                        if capped_payment < payment * (cast::<Ext, _>(0.1f32)) {
                            capped_payment = payment * (cast::<Ext, _>(0.1f32));
                        }
                        payment = capped_payment.abs();
                        if g.credit_rating_num[short_party] < 2 {
                            cancel_entity_swaps(g, ui, rng, short_party, -1, 2);
                            if 0 < slot {
                                slot -= 1;
                            }
                            break 'rec;
                        }
                        g.news_msg = g.ent_symbol[short_party].clone()
                            + __content.string(11221)
                            + &g.ent_symbol[long_party];
                        if len(g.news_msg.as_bytes()) < 74 {
                            g.news_msg = replace(g.news_msg.as_bytes(), b"swap", __content.bytes(11222));
                        }
                        post_news(g, ui, short_party, long_party);
                    }
                } else if payment < Ext::from_f64(0.0) && 10 < long_party {
                    calc_entity_financials(g, ui, rng, long_party);
                    if 85 < long_party {
                        g.net_worth = g.contingency_reserve[long_party] + g.net_worth;
                    }
                    if g.net_worth < payment.abs() {
                        let mut capped_payment = (cast::<Ext, _>(0.9f32)) * g.net_worth;
                        if capped_payment < payment.abs() * (cast::<Ext, _>(0.1f32)) {
                            capped_payment = payment * (cast::<Ext, _>(0.1f32));
                        }
                        payment = capped_payment;
                        if capped_payment > Ext::from_f64(0.0) {
                            payment = -Ext::from_f64(1.0) * capped_payment;
                        }
                        if g.credit_rating_num[long_party] < 2 {
                            cancel_entity_swaps(g, ui, rng, long_party, -1, 2);
                            if 0 < slot {
                                slot -= 1;
                            }
                            break 'rec;
                        }
                        if is_in_recent_headlines(g, __content.bytes(11223)) == 0 {
                            g.news_msg = g.ent_symbol[long_party].clone()
                                + __content.string(11224)
                                + &g.ent_symbol[short_party];
                            post_news(g, ui, short_party, long_party);
                        }
                    }
                }
            }
            if ent != 0 {
                g.news_msg = PbStr::new();
            }
            if !g.news_msg.is_empty() {
                post_news(g, ui, long_party, short_party);
            }
            if ent == 0 && payment != Ext::from_f64(0.0) {
                g.cash[long_party] = payment + g.cash[long_party];
                g.cash[short_party] = g.cash[short_party] - payment;
                g.accrued_pl[long_party] = payment + g.accrued_pl[long_party];
                g.accrued_pl[short_party] = g.accrued_pl[short_party] - payment;
                swap_pnl_accumulator(g, long_party, 1, payment);
                swap_pnl_accumulator(g, short_party, 1, -payment);
            }
            if 0 < report_entity {
                if long_party == report_entity {
                    reported_pnl = to_quad(payment + cast::<Ext, _>(reported_pnl));
                }
                if short_party == report_entity {
                    reported_pnl = to_quad(cast::<Ext, _>(reported_pnl) - payment);
                }
            }
            if ent == 0 {
                if !(fval(&swap_rec, 6) > cur_date.get()) && 5 < parse_count_any(swap_rec.as_bytes(), b"|") {
                    g.swap_contracts[slot] = PbStr::new();
                    pack_holdings(g, ui);
                    if slot == 1 {
                        g.news_msg = PbStr::from(__content.string(11214))
                            + &g.ent_symbol[long_party]
                            + " and "
                            + &g.ent_symbol[short_party]
                            + __content.string(11225);
                        if len(g.news_msg.as_bytes()) < 67 {
                            post_news(g, ui, long_party, short_party);
                        } else {
                            g.news_msg = PbStr::new();
                        }
                    }
                    if 0 < slot {
                        slot -= 1;
                    }
                }
            }
        }
        slot += 1;
    }
    if 0 < ent {
        if g.sim_co_count < ent && !(*mode_flag == "R") {
            result = 0;
        } else {
            result = reported_pnl;
        }
    } else {
        pack_holdings(g, ui);
        if 0 < report_entity {
            let threshold = Ext::from_f64(50.0) * rng.rnd() + Ext::from_f64(50.0);
            if (cast::<Ext, _>(reported_pnl)).abs() > threshold && report_entity != 1595 {
                let pl_word = if (cast::<Ext, _>(reported_pnl)) < Ext::from_f64(0.0) { "loss" } else { "profits" };
                g.news_msg = g.ent_symbol[report_entity].clone()
                    + __content.string(11226)
                    + &trim(str_num(cast::<Ext, _>(g.cur_quarter), 15).as_bytes())
                    + " "
                    + pl_word
                    + " of "
                    + &trim(g.currency_symbol.as_bytes())
                    + &trim(&format((cast::<Ext, _>(reported_pnl)).abs(), b"* ######,"))
                    + " "
                    + &g.million_prefix_lc
                    + __content.string(11227);
                post_news(g, ui, 0, 0);
            }
        }
    }
    cast::<Ext, _>(result)
}

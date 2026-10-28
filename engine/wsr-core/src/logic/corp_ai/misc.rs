//! Small scheduled bookkeeping and AI routines: government-bond interest, loan-portfolio amortisation,
//! Superfund and asbestos charges, turnaround headlines, margin erosion of young companies, the stock
//! index divisor rebase, the recent-dividend-change list, excess-loss recapture and the hot-stock pick.

#![allow(
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::assign_op_pattern,
    clippy::neg_cmp_op_on_partial_ord
)]
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;

use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::news::is_in_recent_headlines;
use crate::logic::report::show_quick_message;
use crate::logic::shorts::find_short_positions;
use crate::logic::sim_entity::quarter_start::recapture_excess_loss;
use crate::pb::string::parse_count_any;

/// Port of AccrueGovBondInterest (0x4c572b).
/// Quarterly interest on the long and short government bonds held by players 1..NumPlayers, banks,
/// insurers and funds 1581..1583 (active only): coupon `Face * 0.0025 * coupon%` (coupon from
/// `RIGHT$(…Desc, 5)`, a SINGLE) to `Cash` and `AccruedPL`, plus the straight-line amortisation of the
/// premium/discount `(Face - Cost) / quartersToMaturity` (at least 1) to the cost and `AccruedPL`.
pub fn accrue_gov_bond_interest(g: &mut Globals) {
    // ON ERROR GOTO -> ShowRuntimeError (not ported)
    for long_bond in [true, false] {
        let desc = if long_bond { g.long_bond_desc.clone() } else { g.short_bond_desc.clone() };
        let coupon = cast::<f32, _>(val(right(desc.as_bytes(), 5).as_bytes()));
        let year = to_long(val(mid(desc.as_bytes(), 1, 4).as_bytes()));
        let mut qtrs_to_maturity = (year.wrapping_sub(g.cur_year))
            .wrapping_mul(4)
            .wrapping_sub(g.cur_quarter - 1);
        if qtrs_to_maturity < 1 {
            qtrs_to_maturity = 1;
        }
        for entity_id in 1..=1583 {
            if !((entity_id <= g.num_players || 10 < entity_id) && (entity_id < 131 || 1580 < entity_id)) {
                continue;
            }
            if g.entity_status[entity_id] != 1 {
                continue;
            }
            let face = if long_bond { g.long_gov_bond_face[entity_id] } else { g.short_gov_bond_face[entity_id] };
            if !(Ext::from_f64(0.0) < cast::<Ext, _>(face)) {
                continue;
            }
            let interest = cast::<Ext, _>(face) * sf(0.0025f32) * cast::<Ext, _>(coupon);
            let cost = if long_bond { g.lt_gov_bond_cost[entity_id] } else { g.st_gov_bond_cost[entity_id] };
            let amort = (cast::<Ext, _>(face) - cost) / cast::<Ext, _>(qtrs_to_maturity);
            if long_bond {
                g.lt_gov_bond_cost[entity_id] = g.lt_gov_bond_cost[entity_id] + amort;
            } else {
                g.st_gov_bond_cost[entity_id] = g.st_gov_bond_cost[entity_id] + amort;
            }
            g.accrued_pl[entity_id] = interest + g.accrued_pl[entity_id] + amort;
            g.cash[entity_id] = g.cash[entity_id] + interest;
        }
    }
}

/// Port of TurnaroundNews (0x4c67a5).
/// For the entity `SimCoCount`: one time in three, a chronically loss-making operating company (margin
/// under 16%, losses this year and in each of the last four years, business assets at least 40x its
/// market cap, no GDP slump below -2% and no pandemic/peak-oil/subprime crisis) that is not controlled
/// by a human gets new managers: `ProfitMargin` = 11..20. Unless "profitability" is in the recent
/// headlines, one of four turnaround headlines (by `RandSeed MOD 4`) is left in `NewsMsg` (the caller
/// posts it).
pub fn turnaround_news(g: &mut Globals, rng: &mut Rng) {
    let __content = crate::content::current();
    let corp_id = g.sim_co_count;
    if !(g.entity_status[corp_id] == 1) {
        return;
    }
    if !(rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0)) {
        return;
    }
    if !(g.industry_num[corp_id] < 70
        && g.profit_margin[corp_id] < 16
        && g.earnings_ytd[corp_id] < Ext::from_f64(0.0)
        && g.earnings_yr4_ago[corp_id] < Ext::from_f64(0.0)
        && g.earnings_yr3_ago[corp_id] < Ext::from_f64(0.0)
        && g.earnings_yr2_ago[corp_id] < Ext::from_f64(0.0)
        && g.earnings_y1[corp_id] < Ext::from_f64(0.0)
        && -Ext::from_f64(2.0) <= cast::<Ext, _>(g.gdp_growth)
        && g.pandemic_state < 1
        && g.peak_oil_crisis < 1
        && g.subprime_crisis_state < 1)
    {
        return;
    }
    if !(g.market_cap[corp_id] * sf(0.4f32) * Ext::from_f64(100.0) <= g.business_assets[corp_id]) {
        return;
    }
    if get_controller_type(g, corp_id) == 1 {
        return;
    }
    g.profit_margin[corp_id] = (cast::<i16, _>(rng.rnd_range(10, 1))).wrapping_add(10);
    if is_in_recent_headlines(g, __content.bytes(2830)) == 1 {
        return;
    }
    let sym = g.ent_symbol[corp_id].clone();
    g.news_msg = PbStr::from(__content.string(2831)) + &sym + __content.string(2832);
    match g.rand_seed % 4 {
        1 => g.news_msg = PbStr::from(__content.string(2833)) + &sym,
        2 => {
            g.news_msg = PbStr::from(__content.string(2834)) + &sym + __content.string(2835)
        }
        3 => {
            g.news_msg =
                PbStr::from(__content.string(2836)) + &sym + __content.string(2837)
        }
        _ => {}
    }
}

/// Port of ChargeSuperfundCosts (0x4bbc32).
/// The Superfund company `SuperfundCorp` (cleared for a fund or an inactive company) pays its quarterly
/// environmental cleanup costs `ROUND(MarketCap * (DifficultyLevel + 2.8))` from `Cash` / `AccruedPL`,
/// with a headline (a pending `NewsMsg` is posted first). If some player is short the stock
/// (`FindShortPositions(corp, 0) = 2`), the Superfund ends a year earlier, the margin rises by 2
/// (operating company, difficulty 3) and the investor sentiment by 10 (if under 30).
pub fn charge_superfund_costs(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mut corp_id = cast::<i32, _>(g.superfund_corp);
    if !(corp_id < 11 || g.industry_num[corp_id] != 71) {
        g.superfund_corp = 0;
        return;
    }
    if 10 < corp_id && g.entity_status[corp_id] != 1 {
        g.superfund_corp = 0;
        corp_id = 0;
    }
    if !(10 < corp_id) {
        return;
    }
    let cleanup_cost = to_quad(g.market_cap[corp_id] * (cast::<Ext, _>(g.difficulty_level) + sf(2.8f32)));
    g.cash[corp_id] = g.cash[corp_id] - cast::<Ext, _>(cleanup_cost);
    g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - cast::<Ext, _>(cleanup_cost);
    let cost_text = trim(&format(cast::<Ext, _>(cleanup_cost), b"* ###,"));
    if val(cost_text.as_bytes()) == Ext::from_f64(0.0) {
        return;
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.news_msg = g.ent_symbol[corp_id].clone()
        + __content.string(2838)
        + &trim(g.currency_symbol.as_bytes())
        + &cost_text
        + " "
        + &g.million_prefix_uc
        + "il.";
    post_news(g, ui, corp_id, 0);
    if find_short_positions(g, ui, rng, corp_id, 0) == 2 {
        g.superfund_end_year = g.superfund_end_year.wrapping_sub(1);
        if g.industry_num[corp_id] < 70 && g.difficulty_level == 3 {
            g.profit_margin[corp_id] = g.profit_margin[corp_id].wrapping_add(2);
        }
        if g.investor_sentiment[corp_id] < 30 {
            g.investor_sentiment[corp_id] = g.investor_sentiment[corp_id].wrapping_add(10);
        }
    }
}

/// Port of AmortizeLoanPortfolios (0x4c6227).
/// Quarterly principal repayments of the active banks' and insurers' loan portfolios: 0.335% of
/// `SubprimeSecs` (banks and insurers), 1.7% of `BankConsumerLoans` and 0.67% of `BankMortgageLoans`
/// (banks), each added to `Cash` and taken off the (rounded QUAD) balance.
pub fn amortize_loan_portfolios(g: &mut Globals) {
    for fin_inst_id in 11..=130 {
        if g.entity_status[fin_inst_id] != 1 {
            continue;
        }
        if 0 < g.subprime_secs[fin_inst_id] {
            let paid = cast::<Ext, _>(g.subprime_secs[fin_inst_id]) * sf(0.00335f32);
            g.cash[fin_inst_id] = g.cash[fin_inst_id] + paid;
            g.subprime_secs[fin_inst_id] = to_quad(cast::<Ext, _>(g.subprime_secs[fin_inst_id]) - paid);
        }
        if fin_inst_id < 86 {
            if 0 < g.bank_consumer_loans[fin_inst_id] {
                let paid = cast::<Ext, _>(g.bank_consumer_loans[fin_inst_id]) * sf(0.017f32);
                g.cash[fin_inst_id] = g.cash[fin_inst_id] + paid;
                g.bank_consumer_loans[fin_inst_id] = to_quad(cast::<Ext, _>(g.bank_consumer_loans[fin_inst_id]) - paid);
            }
            if 0 < g.bank_mortgage_loans[fin_inst_id] {
                let paid = cast::<Ext, _>(g.bank_mortgage_loans[fin_inst_id]) * sf(0.0067f32);
                g.cash[fin_inst_id] = g.cash[fin_inst_id] + paid;
                g.bank_mortgage_loans[fin_inst_id] = to_quad(cast::<Ext, _>(g.bank_mortgage_loans[fin_inst_id]) - paid);
            }
        }
    }
}

/// Port of AsbestosReserveCharge (0x4bc147).
/// The asbestos-liability company `ABS(AsbestosCorp)` (active, net worth over 100, contingency reserve
/// under 45% of the net worth) adds 10% of its net worth to `ContingencyReserve`, charged to
/// `EquityIncomeYTD`, with a headline ("creates a" / "sets aside" / "increases" / "adds to its"), and
/// re-projects its earnings.
pub fn asbestos_reserve_charge(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let corp_id = to_long((cast::<Ext, _>(g.asbestos_corp)).abs());
    if !(10 < corp_id && corp_id < 1601 && g.entity_status[corp_id] == 1) {
        return;
    }
    calc_entity_financials(g, ui, rng, corp_id);
    if !(Ext::from_f64(100.0) < g.net_worth) {
        return;
    }
    if !(g.contingency_reserve[corp_id] < g.net_worth * sf(0.45f32)) {
        return;
    }
    let reserve_add = to_quad(g.net_worth * sf(0.1f32));
    let mut verb_text = PbStr::new();
    if g.contingency_reserve[corp_id] == Ext::from_f64(0.0) {
        verb_text = PbStr::from(__content.string(2839));
    }
    g.contingency_reserve[corp_id] = g.contingency_reserve[corp_id] + cast::<Ext, _>(reserve_add);
    if verb_text.is_empty() {
        verb_text = PbStr::from(__content.string(2840));
        let k = (g.cur_year.wrapping_add(g.cur_quarter)) % 3;
        if k == 1 {
            verb_text = PbStr::from(__content.string(2841));
        }
        if k == 2 {
            verb_text = PbStr::from(__content.string(2842));
        }
    }
    g.equity_income_ytd[corp_id] = g.equity_income_ytd[corp_id] - cast::<Ext, _>(reserve_add);
    g.news_msg = g.ent_symbol[corp_id].clone() + &verb_text + __content.string(2843);
    post_news(g, ui, corp_id, 0);
    calc_projected_earnings(g, ui, rng, corp_id);
}

/// Port of RebaseStockIndexDivisor (0x4c5fef).
/// When the stock index has left 4,850..5,150, rescales `StockIndexDivisor` so that the index reads
/// 5,000 again and restarts the index history (series 0) with three random bars around 5,000; shows
/// " [runtime resource wording] …" (ShowQuickMessage). `StockIndex` itself is not changed here.
/// The main window's index label (control 114, "   5,000.00") is UI only and not modelled.
pub fn rebase_stock_index_divisor(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if !(g.stock_index < Ext::from_f64(4850.0) || Ext::from_f64(5150.0) < g.stock_index) {
        return;
    }
    let ratio: Ext = crate::pb::numeric::double(g.stock_index / Ext::from_f64(5000.0)); // stored as DOUBLE
    g.stock_index_divisor = ratio * g.stock_index_divisor;
    // (UI) CONTROL SET TEXT GetDlgItem(hDlgMain, 114), "   5,000.00"
    g.cap_hist_close[(0, 0)] = Ext::from_f64(3.0);
    for k in 1..=3 {
        if k < 3 {
            g.cap_hist_close[(k, 0)] = rng.rnd() * Ext::from_f64(50.0) + Ext::from_f64(5000.0);
        } else {
            g.cap_hist_close[(k, 0)] = Ext::from_f64(5000.0);
        }
        g.price_hist_high[(k, 0)] = rng.rnd() * Ext::from_f64(40.0) + Ext::from_f64(5060.0);
        g.price_hist_low[(k, 0)] = Ext::from_f64(4990.0) - rng.rnd() * Ext::from_f64(100.0);
    }
    let mut msg = PbStr::from(__content.string(2844));
    show_quick_message(g, ui, &mut msg);
}

/// Port of YoungCorpMarginErosion (0x4b819b).
/// For the entity `SimCoCount` (an active operating company without earnings four years ago, in the
/// quarter `id MOD 4`, not computer-controlled): unless its d [runtime resource wording] at least
/// `RND(0..10) + 4`, the margin drops by 1 (2 at difficulty 3 without earnings three years ago), +1 without
/// earnings two years ago (difficulty > 1), +1 without last year's earnings.
pub fn young_corp_margin_erosion(g: &mut Globals, rng: &mut Rng) {
    let corp_id = g.sim_co_count;
    if !(10 < g.sim_co_count
        && g.entity_status[g.sim_co_count] == 1
        && g.industry_num[corp_id] < 70
        && corp_id % 4 == g.cur_quarter
        && g.earnings_yr4_ago[corp_id] == Ext::from_f64(0.0))
    {
        return;
    }
    if get_controller_type(g, corp_id) == 2 {
        return;
    }
    let mut erosion: i16 = 1;
    if g.earnings_yr3_ago[corp_id] == Ext::from_f64(0.0) && g.difficulty_level == 3 {
        erosion = 2;
    }
    if g.earnings_yr2_ago[corp_id] == Ext::from_f64(0.0) && 1 < g.difficulty_level {
        erosion += 1;
    }
    if g.earnings_y1[corp_id] == Ext::from_f64(0.0) {
        erosion += 1;
    }
    if (cast::<i32, _>(g.disc_spend_pct[corp_id])) < rng.rnd_range(10, 0).wrapping_add(4) {
        g.profit_margin[corp_id] = g.profit_margin[corp_id].wrapping_sub(erosion);
    }
}

/// Port of TrimRecentDividendChanges (0x4bbfb1).
/// Drops the oldest entry from `RecentDividendChangeList` (`"|"`-separated, 4-character ids plus
/// separator): the list is cleared when it has no fields or is shorter than 5; a short leading field
/// (separator before position 5) is cut first (a remaining 4-character entry gets its `"|"`), then the
/// first 5 characters are removed.
pub fn trim_recent_dividend_changes(g: &mut Globals) {
    if g.recent_dividend_change_list.is_empty() {
        return;
    }
    let count = parse_count_any(g.recent_dividend_change_list.as_bytes(), b"|");
    let mut list_len = len(g.recent_dividend_change_list.as_bytes());
    if count == 0 || list_len < 5 {
        g.recent_dividend_change_list = PbStr::new();
        return;
    }
    let sep = instr(1, g.recent_dividend_change_list.as_bytes(), b"|");
    if sep < 5 {
        g.recent_dividend_change_list = right(g.recent_dividend_change_list.as_bytes(), list_len - sep);
        list_len -= sep;
        if list_len == 4 && instr(1, g.recent_dividend_change_list.as_bytes(), b"|") == 0 {
            g.recent_dividend_change_list = g.recent_dividend_change_list.clone() + "|";
        }
    }
    g.recent_dividend_change_list = right(g.recent_dividend_change_list.as_bytes(), list_len - 5);
}

/// Port of CheckExcessLossRecapture (0x4ba8d5).
/// For an active `corp_id` (<= 1600) with holdings: every holding under 80% with a negative basis goes
/// through `RecaptureExcessLoss(stock, corp_id)`.
pub fn check_excess_loss_recapture(g: &mut Globals, ui: &mut dyn Ui, corp_id: i32) {
    if !(g.entity_status[corp_id] == 1 && corp_id < 1601) {
        return;
    }
    if g.hold_stock_id[(0, corp_id)] == 0 {
        return;
    }
    for slot in 1..=15 {
        if g.hold_stock_id[(slot, corp_id)] != 0
            && g.hold_stock_pct[(slot, corp_id)] < 80
            && g.hold_stock_basis[(slot, corp_id)] < Ext::from_f64(0.0)
        {
            // stockId is passed BYREF; RecaptureExcessLoss does not change it.
            let stock_id = cast::<i32, _>(g.hold_stock_id[(slot, corp_id)]);
            recapture_excess_loss(g, ui, stock_id, corp_id);
        }
    }
}

/// Port of PickHotStockCorp (0x4a5881).
/// Clears `TurnaroundPickCorp` in Q3 and an inactive `HotStockCorp`. While a hot stock exists, a new
/// one is only drawn at entity 11 in Q1. A random company 11..1600 (operating, active, market cap at most
/// `RND * 30 + 20`, at difficulty 3 controlled by a company) becomes both `TurnaroundPickCorp` and
/// `HotStockCorp`.
pub fn pick_hot_stock_corp(g: &mut Globals, rng: &mut Rng) {
    if g.cur_quarter == 3 {
        g.turnaround_pick_corp = 0;
    }
    if 10 < g.hot_stock_corp {
        if g.entity_status[cast::<i32, _>(g.hot_stock_corp)] != 1 {
            g.hot_stock_corp = 0;
        }
        if g.sim_co_count != 11 {
            return;
        }
        if g.cur_quarter != 1 {
            return;
        }
    }
    let i = rng.rnd_range(1600, 11);
    if g.industry_num[i] < 70 && g.entity_status[i] == 1 {
        if g.market_cap[i] <= rng.rnd() * Ext::from_f64(30.0) + Ext::from_f64(20.0) && (g.difficulty_level != 3 || 10 < g.corp_controller[i]) {
            g.turnaround_pick_corp = cast::<i16, _>(i);
            g.hot_stock_corp = g.turnaround_pick_corp;
        }
    }
}

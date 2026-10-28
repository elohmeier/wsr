//! Fund (ETF) supervision and cash management: `CoverNegativeCash`, `EtfComplianceCheck` (with its
//! GOSUBs), `EtfAdviserReview` (with its GOSUB), `AiEtfInvestCash`, `EtfLiquidateBondsOnDisqualify` and
//! `ReviveDefunctEtfs`.
//!
//! Funds are the entities 1581..1600 of industry 71 (1584/1585 are the index funds, 1600 the futures
//! fund). The `ON ERROR` handlers of these routines only call ShowRuntimeError and leave; the error paths
//! are not reproduced (out-of-range subscripts read the arrays' dummy element). SINGLE pool constants
//! (`fmul dword [..]`) are written `sf(0.4)`.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::manual_range_contains
)]
use crate::pb::real::{Ext, cast};


use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::balance_cash_t_bills;
use crate::logic::finance::{calc_entity_financials, get_managing_player};
use crate::logic::newgame_helpers::{pick_etf_holding_stock, remove_recent_offering};
use crate::logic::options::close_option_positions;
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::assign_etf_adviser;
use crate::logic::research::get_etf_largest_holding_slot;
use crate::logic::sim_entity::ai_corp::is_recent_offering;
use crate::logic::sim_entity::bonds::{bond_buyback, issue_stock};
use crate::logic::sim_entity::fin_inst::sell_corp_bonds;
use crate::logic::sim_entity::prelude::*;
use crate::logic::sim_entity::transactions::{
    etf_drop_one_adviser_contract, forced_sell_bonds, forced_sell_stocks, sell_stock_holding,
};
use crate::logic::valuation::{
    calc_stock_price, clear_price_history, price_pct_change_months, price_trend_direction,
    update_price_history_tick,
};

fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

// ------------------------------------------------------------------------------- CoverNegativeCash

/// Port of CoverNegativeCash (0x504cd1).
/// Raises cash for an active entity (1..1600, not a bank) whose `Cash` is negative after
/// [`balance_cash_t_bills`]: draws on the unused credit line (`LenderId` from `CalcEntityFinancials`),
/// then force-sells bonds, then (with `allow_wc_cut = 1`, operating companies 131+ below industry 70
/// without subsidiary stock) releases up to half of the working capital, then closes long option
/// positions (up to 100 rounds while `FindOptionPositions` reports long options) and finally
/// force-sells stocks (`ForcedSellStocks` with the managing player).
pub fn cover_negative_cash(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32, allow_wc_cut: i32) {
    let e = entity_id;
    if !(0 < e && e < 1601) {
        return;
    }
    if g.entity_status[e] != 1 || !(g.cash[e] < Ext::from_f64(0.0)) {
        return;
    }
    balance_cash_t_bills(g, e);
    if !(g.cash[e] < Ext::from_f64(0.0)) {
        return;
    }
    if 10 < e && g.industry_num[e] == 1 {
        return;
    }
    calc_entity_financials(g, ui, rng, e);
    if g.unused_credit_line > Ext::from_f64(0.0) {
        let amount = if g.unused_credit_line > -g.cash[e] { -g.cash[e] } else { g.unused_credit_line };
        g.cash[e] = g.cash[e] + amount;
        let lender = g.lender_id;
        g.cash[lender] = g.cash[lender] - amount;
        g.bank_debt[e] = g.bank_debt[e] + amount;
    }
    if !(g.cash[e] < Ext::from_f64(0.0)) {
        return;
    }
    forced_sell_bonds(g, ui, rng, e, 0, Ext::from_f64(0.0), Ext::from_f64(0.0));
    if !(g.cash[e] < Ext::from_f64(0.0)) {
        return;
    }
    if allow_wc_cut == 1 && 130 < e && g.industry_num[e] < 70 && g.tmp_subsid_stock_value == Ext::from_f64(0.0) {
        let mut amount = g.working_capital[e] * sf(0.5f32);
        if amount > g.cash[e].abs() {
            amount = g.cash[e].abs();
        }
        g.working_capital[e] = g.working_capital[e] - amount;
        g.cash[e] = g.cash[e] + amount;
        if !(g.cash[e] < Ext::from_f64(0.0)) {
            return;
        }
    }
    let opt_result = find_option_positions(g, ui, e, 0, 0, 0);
    if opt_result == 41 || opt_result == 42 {
        let mut tries = 100;
        loop {
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            if close_option_positions(g, ui, rng, e, 0, 0, 0) < 1 {
                break;
            }
            // only written (short) options left
            if find_option_positions(g, ui, e, 0, 0, 0) == -41 {
                break;
            }
            tries -= 1;
            if tries == 0 {
                break;
            }
        }
    }
    let managing_player = get_managing_player(g, e);
    let _sold_list = forced_sell_stocks(g, ui, rng, e, managing_player);
}

// ------------------------------------------------------------------------------ EtfComplianceCheck

/// Locals of `EtfComplianceCheck` (zeroed at entry; the GOSUBs run on this frame).
#[derive(Default)]
struct ComplianceFrame {
    etf_id: i32,        // -0x88
    hold_slot: i32,     // -0x8c
    opt_pos: i32,       // -0x90
    stock_id: i32,      // -0x98
    largest_slot: i32,  // -0x9c
    value: Ext,         // -0xa8 (EXT)
    pct_to_sell: i32,   // -0xac
    start_offset: i32,  // -0xb0
    sales_count: i32,   // -0xb4
    sell_flag: i32,     // -0xb8
    sell_frac: Ext,     // -0xc0 (DOUBLE)
}

/// Port of EtfComplianceCheck (0x64b0c9), ending in EmptyStub_64c309 (0x64c309, no-op).
/// Yearly-style SEC/credit compliance of the funds `1581 + offset`..1600 (offset 0, 8 for
/// `300 < SimCoCount < 800`, 13 above 800): dumps holdings of distressed / environmentally liable
/// stocks, sells holdings over 10% of a company, enforces the "300% rule" on bank debt (repaying the
/// excess, [`cover_negative_cash`] if needed), trims the largest holding when it is over 50% of the
/// assets or debt/cash/bond maturities require it, buys back cheap bonds of liquid funds, and lets
/// illiquid AI-run funds issue stock (10% chance). Stops after the third sale (`salesCount = 3`).
///
/// Bugs (reproduced): the stop tests `salesCount = 3` exactly, so a count jumping past 3 never stops
/// the scan (181); `sellFlag` is only reset after the 10%-rule sale, which therefore uses the previous
/// fund's flag (184).
pub fn etf_compliance_check(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let mut f = ComplianceFrame::default();
    if 300 < g.sim_co_count && g.sim_co_count < 800 {
        f.start_offset = 8;
    }
    if 800 < g.sim_co_count {
        f.start_offset = 13;
    }
    f.etf_id = f.start_offset + 1581;
    while f.etf_id <= 1600 {
        if compliance_one(g, ui, rng, &mut f) && f.sales_count == 3 {
            break;
        }
        f.etf_id += 1;
    }
    // EmptyStub_64c309: nothing
}

/// One iteration of the fund loop; `true` when it reaches the `salesCount = 3` test at the end (the
/// "next fund" jumps skip it).
fn compliance_one(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut ComplianceFrame) -> bool {
    let __content = crate::content::current();
    let etf = f.etf_id;
    if g.entity_status[etf] != 1 || g.industry_num[etf] != 71 {
        return false;
    }
    if g.cur_quarter == 3 && 1280 < g.sim_co_count {
        remove_recent_offering(g, etf);
    }
    dump_risky_holdings(g, ui, rng, f);
    if g.hold_stock_id[(0, etf)] < 5 && get_etf_adviser_human_status(g, ui, rng, etf) == 0 && 1585 < etf {
        return false;
    }
    f.sell_frac = crate::pb::numeric::double(Ext::from_f64(0.0));
    f.largest_slot = 0;
    if 1585 < etf {
        enforce_10pct(g, ui, rng, f);
        if f.sales_count == 3 {
            return false;
        }
        f.largest_slot = get_etf_largest_holding_slot(g, etf);
        f.sell_flag = 0;
        if f.largest_slot == 0 {
            return false;
        }
    }
    calc_entity_financials(g, ui, rng, etf);
    // the "300% rule": debt over 3x net worth
    if g.tmp_total_debt > g.exchange_rate * Ext::from_f64(10.0)
        && g.net_worth < g.tmp_total_debt * Ext::from_f64(3.0)
        && g.bank_debt[etf] > Ext::from_f64(0.0)
        && 2 <= g.credit_rating_num[etf]
    {
        let mut excess = g.tmp_total_debt - g.net_worth * sf(0.3333f32);
        if excess > g.exchange_rate * Ext::from_f64(5.0) {
            if excess > g.bank_debt[etf] {
                excess = g.bank_debt[etf];
            }
            g.cash[etf] = g.cash[etf] - excess;
            g.bank_debt[etf] = g.bank_debt[etf] - excess;
            let lender = g.lender_id;
            g.cash[lender] = g.cash[lender] + excess;
            g.news_msg = g.ent_symbol[etf].clone() + __content.string(4827);
            if g.cash[etf] >= Ext::from_f64(0.0) || g.cash[etf].abs() <= g.t_bills[etf] {
                post_news(g, ui, etf, 0);
            } else {
                // left pending in NewsMsg
                g.news_msg = g.news_msg.clone() + __content.string(4828);
            }
            if g.cash[etf] < Ext::from_f64(0.0) {
                cover_negative_cash(g, ui, rng, etf, 0);
            }
            calc_entity_financials(g, ui, rng, etf);
        }
    }
    if 1585 < etf && 0 < f.largest_slot {
        let stock = cast::<i32, _>(g.hold_stock_id[(f.largest_slot, etf)]);
        f.stock_id = stock;
        if stock < 11 {
            return false;
        }
        f.pct_to_sell = cast::<i32, _>(g.hold_stock_pct[(f.largest_slot, etf)]);
        f.value = g.market_cap[stock] * cast::<Ext, _>(f.pct_to_sell);
        if !(g.total_assets > Ext::from_f64(0.0)) || !(f.value > Ext::from_f64(0.0)) {
            return false;
        }
        f.sell_frac = crate::pb::numeric::double(f.value / g.total_assets);
        if !(sf(0.5f32) >= f.sell_frac) {
            g.news_msg = g.ent_symbol[stock].clone()
                + __content.string(4829)
                + &g.ent_symbol[etf]
                + __content.string(4830);
            sell_holding(g, ui, rng, f);
        } else if !(sf(0.2f32) >= f.sell_frac)
            && g.tmp_total_debt > Ext::from_f64(0.0)
            && get_etf_adviser_human_status(g, ui, rng, etf) == 0
        {
            let liquid = g.cash[etf] + g.t_bills[etf];
            if g.tmp_total_debt * sf(0.25f32) > liquid {
                sell_holding(g, ui, rng, f);
            } else if g.ci_bonds_outstanding > 0
                && g.cash[etf] + g.t_bills[etf] < cast::<Ext, _>(g.bonds_issued[etf]) * sf(0.8f32)
                && val(mid(g.bond_issue_info[etf].as_bytes(), 1, 4).as_bytes()) < cast::<Ext, _>(3 + g.cur_year)
            {
                g.news_msg = g.ent_symbol[etf].clone()
                    + __content.string(4831)
                    + &g.ent_symbol[stock]
                    + __content.string(4832);
                sell_holding(g, ui, rng, f);
            } else if g.bank_debt[etf] > Ext::from_f64(100.0) {
                f.sell_flag = 1;
                sell_holding(g, ui, rng, f);
            }
        } else if g.tmp_total_debt * sf(0.4f32) > g.cash[etf] + g.t_bills[etf] && g.tmp_total_debt > Ext::from_f64(0.0) {
            f.sell_flag = 1;
            if 0 < get_etf_adviser_human_status(g, ui, rng, etf) {
                let adviser = g.fund_adviser[etf];
                if 10 < adviser && g.auto_pilot[adviser] == 0 {
                    return false;
                }
            }
            sell_holding(g, ui, rng, f);
        }
    }
    // bonds: buy back cheap bonds, or raise cash with a stock offering
    let liquid = g.cash[etf] + g.t_bills[etf];
    if g.bonds_public_held > 0
        && 2 < g.credit_rating_num[etf]
        && liquid > g.exchange_rate * Ext::from_f64(200.0)
        && g.bank_debt[etf] == Ext::from_f64(0.0)
        && g.bond_price[etf].get() < Ext::from_f64(90.0)
    {
        let mut buy_back = true;
        if 0 < get_etf_adviser_human_status(g, ui, rng, etf) {
            let adviser = g.fund_adviser[etf];
            if 10 < adviser && g.auto_pilot[adviser] == 0 {
                buy_back = false;
            }
        }
        if buy_back {
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(2.0);
            g.tmp_ext3 = -Ext::from_f64(1.0);
            bond_buyback(g, ui, rng, etf);
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
        }
    } else if g.total_assets * sf(0.05f32) > g.cash[etf] + g.t_bills[etf]
        && 3 < g.credit_rating_num[etf]
        && g.credit_rating_num[etf] < 7
        && get_etf_adviser_human_status(g, ui, rng, etf) <= 0
        && etf != 1584
        && etf != 1585
        && is_recent_offering(g, etf) != 1
        && Ext::from_f64(10.0) * rng.rnd() > Ext::from_f64(9.0)
    {
        f.hold_slot = rng.rnd_range(10, 1);
        if g.recent_offerings[f.hold_slot] == 0 {
            g.recent_offerings[f.hold_slot] = etf;
        }
        let pct = rng.rnd_range(12, 3);
        g.tmp_ext1 = cast::<Ext, _>(pct + 10);
        issue_stock(g, ui, rng, etf, 1);
    }
    true
}

/// Port of EtfCompliance_SellHolding (0x64bde4), GOSUB of EtfComplianceCheck.
/// Sells part of the fund's holding `stockId` (`pctToSell` points; 1 or 2 points, or the excess over
/// 10, unless `sellFlag`) and posts `NewsMsg`. Skipped for small holdings when the fund has no debt and
/// the holding is under 30% of the assets.
///
/// Note: the covered-call guard tests the "all companies" codes -11 / 12 although `FindOptionPositions`
/// is asked about one company (codes 1 / -1 / 2), so it never applies (bug 182).
fn sell_holding(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut ComplianceFrame) {
    f.opt_pos = find_option_positions(g, ui, f.etf_id, f.stock_id, 1, 0);
    if (f.opt_pos == -11 || f.opt_pos == 12) && f.pct_to_sell < 11 {
        return;
    }
    if f.pct_to_sell < 11 && g.tmp_total_debt == Ext::from_f64(0.0) && sf(0.3f32) > f.sell_frac {
        return;
    }
    g.tmp_ext1 = cast::<Ext, _>(f.etf_id);
    g.tmp_ext2 = Ext::from_f64(1.0);
    if f.sell_flag == 0 && 3 < f.pct_to_sell {
        g.tmp_ext2 = Ext::from_f64(2.0);
    }
    if 10 < f.pct_to_sell && f.sell_flag == 0 {
        g.tmp_ext2 = cast::<Ext, _>(f.pct_to_sell - 10);
    }
    g.tmp_ext3 = Ext::from_f64(2.0);
    sell_stock_holding(g, ui, rng, f.stock_id);
    post_news(g, ui, f.etf_id, f.stock_id);
    f.sales_count += 1;
}

/// Port of EtfCompliance_Enforce10Pct (0x64bef1), GOSUB of EtfComplianceCheck.
/// Sells the first holding of 11% or more of a company (" [runtime resource wording] ").
fn enforce_10pct(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut ComplianceFrame) {
    let __content = crate::content::current();
    let etf = f.etf_id;
    f.hold_slot = 1;
    loop {
        if 11 <= g.hold_stock_pct[(f.hold_slot, etf)] {
            f.sell_frac = crate::pb::numeric::double(sf(0.5f32));
            f.stock_id = cast::<i32, _>(g.hold_stock_id[(f.hold_slot, etf)]);
            f.pct_to_sell = cast::<i32, _>(g.hold_stock_pct[(f.hold_slot, etf)]);
            g.news_msg = g.ent_symbol[etf].clone()
                + __content.string(4833)
                + &g.ent_symbol[f.stock_id]
                + __content.string(4834);
            sell_holding(g, ui, rng, f);
            f.hold_slot = 15;
        }
        f.hold_slot += 1;
        if 15 < f.hold_slot {
            break;
        }
    }
    f.sell_frac = crate::pb::numeric::double(Ext::from_f64(0.0));
}

/// Port of EtfCompliance_DumpRiskyHoldings (0x64c031), GOSUB of EtfComplianceCheck.
/// For an AI-advised fund: sells the whole first holding of the asbestos or Superfund company or of a
/// company rated 1 or lower.
///
/// Bug 180: the reason is set to "environmental liabilities" right before the test
/// `IF reason = "" THEN reason = " [runtime resource wording] situation"`, so every dump cites
/// environmental liabilities.
fn dump_risky_holdings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut ComplianceFrame) {
    let __content = crate::content::current();
    let etf = f.etf_id;
    if g.hold_stock_id[(0, etf)] == 0 || 0 < get_etf_adviser_human_status(g, ui, rng, etf) {
        return;
    }
    f.hold_slot = 1;
    loop {
        f.stock_id = cast::<i32, _>(g.hold_stock_id[(f.hold_slot, etf)]);
        if 11 <= f.stock_id {
            let stock = f.stock_id;
            let liable = (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(stock)
                || (cast::<Ext, _>(g.superfund_corp)).abs() == cast::<Ext, _>(stock);
            if liable || g.credit_rating_num[stock] <= 1 {
                let mut reason = st(__content.string(4835));
                if g.credit_rating_num[stock] < 2 && reason.is_empty() {
                    reason = st(__content.string(4836));
                }
                g.tmp_ext1 = cast::<Ext, _>(etf);
                g.tmp_ext2 = cast::<Ext, _>(g.hold_stock_pct[(f.hold_slot, etf)]);
                g.tmp_ext3 = Ext::from_f64(2.0);
                sell_stock_holding(g, ui, rng, stock);
                g.news_msg = g.ent_symbol[etf].clone()
                    + __content.string(4837)
                    + &g.ent_symbol[stock]
                    + __content.string(4838)
                    + &reason;
                post_news(g, ui, etf, stock);
                f.sales_count += 1;
                f.hold_slot = 15;
            }
        }
        f.hold_slot += 1;
        if 15 < f.hold_slot {
            break;
        }
    }
    f.stock_id = 0;
}

// -------------------------------------------------------------------------------- EtfAdviserReview

/// Port of EtfAdviserReview (0x64d7a0) with its GOSUB EtfAdviserReview_FireAdviser (0x64e4ab), ending in
/// EtfFireManager_Exit (0x64e69e, no-op).
/// In the 4th quarter, reviews the fund `SimCoCount` (1586..1600, held by its adviser for 3+ years)
/// against the stock index over the 5-year price history: fires the adviser after a decline while the
/// index rose, after overpaying a poorly performing adviser, or after a 3-year down trend; after strong
/// performance an AI-run fund issues 15..25% new stock and may raise its adviser's fee by 0.2 points;
/// after a weak 2 years against a steady index an AI fund cuts the fee by 0.1 points.
///
/// Quirk (reproduced): the " [runtime resource wording] …" headline is overwritten by
/// the dismissal headline in `FireAdviser` before anything is posted, so it never appears.
pub fn etf_adviser_review(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if g.cur_quarter != 4 {
        return;
    }
    let etf = g.sim_co_count;
    if g.industry_num[etf] != 71 || g.entity_status[etf] != 1 || etf < 1586 {
        return;
    }
    if g.cap_hist_close[(60, 0)] == Ext::from_f64(0.0) || g.cap_hist_close[(60, etf)] == Ext::from_f64(0.0) {
        return;
    }
    if g.cur_year - 3 < g.ctrl_since_year[etf] {
        return;
    }
    let mut adviser = g.fund_adviser[etf];
    if adviser < 11 || g.entity_status[adviser] != 1 {
        return;
    }
    let idx_cur = to_long(g.cap_hist_close[(0, 0)]);
    let etf_cur = to_long(g.cap_hist_close[(0, etf)]);
    let mut h = 1 + idx_cur;
    if 60 < h {
        h = 1;
    }
    let (old, cur) = (g.cap_hist_close[(h, 0)], g.cap_hist_close[(idx_cur, 0)]);
    if !(old > Ext::from_f64(0.0)) || !(cur > Ext::from_f64(0.0)) {
        return;
    }
    let idx_ratio = cur / old;
    h = 1 + etf_cur;
    if 60 < h {
        h = 1;
    }
    let (old, cur) = (g.cap_hist_close[(h, etf)], g.cap_hist_close[(etf_cur, etf)]);
    if !(old > Ext::from_f64(0.0)) || !(cur > Ext::from_f64(0.0)) {
        return;
    }
    let etf_ratio = cur / old;
    if !(sf(1.34f32) > etf_ratio) && !(etf_ratio - idx_ratio >= sf(0.12f32)) {
        return;
    }
    if !(Ext::from_f64(1.0) > etf_ratio) && sf(1.34f32) > etf_ratio {
        return;
    }
    adviser = g.fund_adviser[etf];
    let mut years_text = PbStr::new();
    if sf(0.9f32) > etf_ratio && !(sf(1.1f32) >= idx_ratio) && g.cur_year - 5 > g.ctrl_since_year[etf] {
        g.news_msg = g.ent_symbol[etf].clone() + __content.string(4839);
        post_news(g, ui, etf, 0);
        fire_adviser(g, ui, rng, etf, adviser, &mut years_text);
        return;
    }
    if sf(0.95f32) > etf_ratio && !(sf(0.7f32) >= cast::<Ext, _>(g.fund_fee_pct[etf])) && g.cur_year - 5 > g.ctrl_since_year[etf] {
        g.news_msg = g.ent_symbol[etf].clone()
            + __content.string(4840)
            + &g.ent_symbol[adviser]
            + __content.string(4841);
        fire_adviser(g, ui, rng, etf, adviser, &mut years_text);
        return;
    }
    if price_trend_direction(g, etf, 36, 6) < 0 && g.cur_year - 3 > g.ctrl_since_year[etf] {
        years_text = st("3 ");
        fire_adviser(g, ui, rng, etf, adviser, &mut years_text);
        return;
    }
    if !(price_pct_change_months(g, etf, 12) >= 10 && price_pct_change_months(g, etf, 6) >= -5) {
        if price_pct_change_months(g, etf, 24) < -10
            && price_pct_change_months(g, 0, 24) > -5
            && get_etf_adviser_human_status(g, ui, rng, etf) == 0
            && !(sf(0.21f32) >= cast::<Ext, _>(g.fund_fee_pct[etf]))
        {
            g.fund_fee_pct[etf] = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.fund_fee_pct[etf]) - sf(0.1f32)));
        }
        return;
    }
    // strong results: stock offering
    if 0 < get_etf_adviser_human_status(g, ui, rng, etf) {
        return;
    }
    calc_entity_financials(g, ui, rng, etf);
    let mut cash_ratio = sf(0.2f32);
    if etf == 1600 {
        cash_ratio = sf(0.4f32);
    }
    if !(g.total_assets * cash_ratio >= g.cash[etf] + g.t_bills[etf])
        && g.bank_debt[etf] == Ext::from_f64(0.0)
        && g.bonds_issued[etf] == 0
    {
        return;
    }
    g.tmp_ext1 = Ext::from_f64(25.0);
    if etf_ratio - idx_ratio < sf(0.6f32) {
        g.tmp_ext1 = Ext::from_f64(20.0);
    }
    if etf_ratio - idx_ratio < sf(0.24f32) {
        g.tmp_ext1 = Ext::from_f64(15.0);
    }
    g.news_msg = g.ent_symbol[etf].clone() + __content.string(4842);
    if price_trend_direction(g, etf, 48, 12) > 0 {
        g.tmp_ext1 = Ext::from_f64(25.0);
        g.news_msg = st(__content.string(4843)) + &g.ent_symbol[etf] + __content.string(4844);
    }
    post_news(g, ui, 0, 0);
    let idx_pct = to_long(Ext::from_f64(100.0) * idx_ratio - Ext::from_f64(100.0));
    let etf_pct = to_long(Ext::from_f64(100.0) * etf_ratio - Ext::from_f64(100.0));
    let mut direction = st("rise");
    if idx_pct < 0 {
        direction = st("decline");
    }
    if (g.sel_entity == etf || g.last_sel_entity == etf) && idx_pct != 0 {
        let mut m = g.ent_name[etf].clone()
            + __content.string(4845)
            + &str_s(cast::<Ext, _>(etf_pct))
            + "% "
            + __content.string(4846)
            + &str_s((cast::<Ext, _>(idx_pct)).abs())
            + "% "
            + &direction
            + __content.string(4847)
            + "\r\n"
            + "\r\n"
            + __content.string(4848)
            + &str_s(g.tmp_ext1)
            + "%.";
        show_quick_message(g, ui, &mut m);
    }
    issue_stock(g, ui, rng, etf, 1);
    if sf(0.9f32) > cast::<Ext, _>(g.fund_fee_pct[etf]) {
        let adv = g.fund_adviser[etf];
        if 85 < adv && 10 < g.corp_controller[adv] {
            if price_pct_change_months(g, etf, 24) > 20 && sf(0.21f32) > cast::<Ext, _>(g.fund_fee_pct[etf]) {
                return;
            }
            g.fund_fee_pct[etf] = cast::<f32, _>(cast::<f32, _>(sf(0.2f32) + cast::<Ext, _>(g.fund_fee_pct[etf])));
            g.news_msg = g.ent_symbol[etf].clone()
                + __content.string(4849)
                + &g.ent_symbol[g.fund_adviser[etf]]
                + __content.string(4850);
            let adv = g.fund_adviser[etf];
            post_news(g, ui, adv, etf);
        }
    }
}

/// Port of EtfAdviserReview_FireAdviser (0x64e4ab), GOSUB of EtfAdviserReview.
/// "After N  [runtime resource wording] <adviser> dismissed as <fund> fund manager" (N = `years_text`,
/// default "5 "); pops it up for a human-advised fund, posts it and assigns a new adviser.
fn fire_adviser(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, etf: i32, adviser: i32, years_text: &mut PbStr) {
    let __content = crate::content::current();
    if years_text.is_empty() {
        *years_text = st("5 ");
    }
    g.news_msg = st("After ")
        + &*years_text
        + __content.string(4851)
        + &g.ent_symbol[adviser]
        + __content.string(4852)
        + &g.ent_symbol[etf]
        + __content.string(4853);
    if 0 < get_etf_adviser_human_status(g, ui, rng, etf) {
        let mut m = g.news_msg.clone() + " of " + &g.ent_name[etf];
        show_quick_message(g, ui, &mut m);
    }
    post_news(g, ui, 0, 0);
    assign_etf_adviser(g, ui, rng, etf, adviser, 1);
}

// --------------------------------------------------------------------------------- AiEtfInvestCash

/// Port of AiEtfInvestCash (0x64e961).
/// For each AI-run fund 1586..1600 with at most 12 holdings, little debt and investable cash (cash +
/// T-bills less positive extraordinary and accrued/dividend income) of at least 40% of
/// `TmpSubsidStockValue`: picks up to 4 candidate stocks ([`pick_etf_holding_stock`]) and buys 2..10%
/// (`BuyStock`) of the first whose 20%-of-liquidity stake is 2..35% (stakes over 10% become 6..10%).
/// `time_secs`: seconds of `TIME$` (for `BuyStock`).
pub fn ai_etf_invest_cash(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let mut pct_to_buy = 0; // -0x98, kept across funds
    let mut stock_id = 0; // -0x8c
    for etf in 1586..=1600 {
        if g.entity_status[etf] != 1 || g.industry_num[etf] != 71 {
            continue;
        }
        if 0 < get_etf_adviser_human_status(g, ui, rng, etf) && g.auto_pilot[g.fund_adviser[etf]] == 0 {
            continue;
        }
        if 12 < g.hold_stock_id[(0, etf)] {
            continue;
        }
        calc_entity_financials(g, ui, rng, etf);
        if etf == 1600 && g.futures_net_value != Ext::from_f64(0.0) {
            continue;
        }
        let mut investable = to_quad(g.cash[etf] + g.t_bills[etf]);
        if g.extraordinary_pl[etf] > 0 {
            investable = to_quad(cast::<Ext, _>(investable) - cast::<Ext, _>(g.extraordinary_pl[etf]));
        }
        if g.accrued_pl[etf] + g.div_recvd2[etf] > Ext::from_f64(0.0) {
            investable = to_quad(cast::<Ext, _>(investable) - (g.accrued_pl[etf] + g.div_recvd2[etf]));
        }
        if (cast::<Ext, _>(investable)) < g.tmp_subsid_stock_value * sf(0.4f32) {
            continue;
        }
        if g.tmp_total_debt > (g.cash[etf] + g.t_bills[etf]) * sf(0.1f32) {
            continue;
        }
        let mut tries = 0;
        let budget = (g.cash[etf] + g.t_bills[etf]) * sf(0.2f32);
        let mut chosen = 0;
        loop {
            tries += 1;
            if tries == 5 {
                break;
            }
            stock_id = pick_etf_holding_stock(g, ui, rng, etf, 0);
            if stock_id < 11 {
                break;
            }
            if g.market_cap[stock_id] > Ext::from_f64(0.0) {
                pct_to_buy = to_long(budget / g.market_cap[stock_id]);
                if 2 <= pct_to_buy && pct_to_buy <= 35 {
                    if 10 < pct_to_buy {
                        pct_to_buy = rng.rnd_range(5, 1) + 5;
                    }
                    chosen = stock_id;
                }
            }
            if 10 < chosen {
                break;
            }
        }
        if 10 < chosen && 1 < pct_to_buy && pct_to_buy < 11 {
            g.tmp_ext1 = cast::<Ext, _>(pct_to_buy);
            g.tmp_ext2 = Ext::from_f64(2.0);
            crate::logic::trading::buy_stock(g, ui, rng, etf, stock_id, time_secs);
        }
    }
}

// ------------------------------------------------------------------- EtfLiquidateBondsOnDisqualify

/// Port of EtfLiquidateBondsOnDisqualify (0x656f9e).
/// A fund that lost its ETF status sells all its [runtime resource wording]  ([`sell_corp_bonds`] with
/// `TmpExt1` = face, `TmpExt2` = 2) and posts "<sym> [runtime resource wording] ".
pub fn etf_liquidate_bonds_on_disqualify(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, etf_id: i32) {
    let __content = crate::content::current();
    let etf = etf_id;
    if g.hold_bond_id[(0, etf)] == 0 {
        return;
    }
    for slot in 1..=15 {
        let bond_id = g.hold_bond_id[(slot, etf)];
        if bond_id != 0 {
            g.tmp_ext1 = cast::<Ext, _>(g.hold_bond_face[(slot, etf)]);
            g.tmp_ext2 = Ext::from_f64(2.0);
            sell_corp_bonds(g, ui, rng, etf, bond_id);
        }
    }
    g.news_msg = g.ent_symbol[etf].clone() + __content.string(4854);
    post_news(g, ui, etf, 0);
}

// ------------------------------------------------------------------------------- ReviveDefunctEtfs

/// Port of ReviveDefunctEtfs (0x64ee77).
/// Scans the funds 1581..1600: the first liquidated one (status 2, not 1584/1585) is revived with fresh
/// capital and a random adviser; a fully public, unlevered fund 1586+ that was disqualified (industry 70)
/// rejoins industry 71 ([`etf_compliance_check`], offering or cash injection, its CEO players are fired,
/// earnings reset). Both end the scan. A small fund in a down trend gets 5000 cash and a new adviser
/// (the scan continues). The revived fund then invests ([`ai_etf_invest_cash`] x5), its price history
/// is restarted, and the news is posted.
///
/// Bug 183: the manager change calls `EtfDropOneAdviserContract(etfId, 0)` with the fund's own id as the
/// adviser, so it finds no contract and the fund keeps its adviser despite the "fires investment
/// manager" headline.
pub fn revive_defunct_etfs(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let mut revived_id = 0; // -0x90
    let mut etf = 1581; // -0x88
    loop {
        if g.entity_status[etf] == 2 && etf != 1584 && etf != 1585 {
            g.entity_status[etf] = 1;
            g.shares_outstanding[etf] = Ext::from_f64(2.0);
            g.cash[etf] = g.exchange_rate * Ext::from_f64(20000.0);
            g.market_cap[etf] = g.exchange_rate * Ext::from_f64(200.0);
            revived_id = etf;
            g.ent_misc_data[etf] = PbStr::new();
            g.industry_num[etf] = 71;
            if g.lending_bank[etf] == 0 {
                g.lending_bank[etf] = cast::<i16, _>(rng.rnd_range(50, 1) + 10);
            }
            g.fund_bonus_year[etf] = 0;
            g.fund_adviser[etf] = rng.rnd_range(120, 86);
            g.ctrl_since_year[etf] = g.cur_year;
            break;
        }
        if 1586 <= etf && revive_disqualified(g, ui, rng, etf, &mut revived_id) {
            break;
        }
        etf += 1;
        if 1600 < etf {
            break;
        }
    }
    let etf = to_long((cast::<Ext, _>(revived_id)).abs());
    if 0 < etf {
        for _ in 0..5 {
            ai_etf_invest_cash(g, ui, rng, time_secs);
        }
        clear_price_history(g, etf);
        g.valuation_mode = 3;
        calc_stock_price(g, ui, rng, etf, time_secs);
        update_price_history_tick(g, ui, rng, etf);
    }
    if 0 < revived_id {
        g.news_msg = g.ent_symbol[etf].clone() + __content.string(4855);
    }
    if 0 < etf {
        post_news(g, ui, etf, 0);
    }
}

/// The checks of ReviveDefunctEtfs for an active fund 1586..1600 (inline in the original); `true` when
/// a disqualified fund was readmitted (ends the scan).
fn revive_disqualified(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, etf: i32, revived_id: &mut i32) -> bool {
    let __content = crate::content::current();
    update_corp_control(g, ui, rng, etf);
    if g.public_pct != 100 {
        return false;
    }
    find_option_positions(g, ui, 0, etf, 0, 0);
    if g.has_options_outstanding[etf] == 1 {
        return false;
    }
    calc_entity_financials(g, ui, rng, etf);
    if g.bank_debt[etf] + cast::<Ext, _>(g.ci_bonds_outstanding) + g.tmp_player_advances > g.exchange_rate * Ext::from_f64(10.0) {
        return false;
    }
    if g.industry_num[etf] == 70 {
        g.industry_num[etf] = 71;
        etf_compliance_check(g, ui, rng);
        if g.market_cap[etf] >= g.exchange_rate * Ext::from_f64(50.0) {
            g.tmp_ext1 = Ext::from_f64(25.0);
            issue_stock(g, ui, rng, etf, 1);
        } else {
            g.cash[etf] = g.exchange_rate * Ext::from_f64(5000.0) + g.cash[etf];
        }
        *revived_id = etf;
        for i in 1..=5 {
            if etf == g.player_ceo_of[i] {
                g.player_ceo_of[i] = 0;
                g.player_ceo_since_year[i] = 0;
                g.news_msg = g.ent_name[i].clone() + __content.string(4856) + &g.ent_symbol[etf];
                post_news(g, ui, etf, i);
            }
        }
        g.earnings_y1[etf] = Ext::from_f64(0.0);
        g.earnings_yr2_ago[etf] = Ext::from_f64(0.0);
        g.earnings_yr3_ago[etf] = Ext::from_f64(0.0);
        g.earnings_yr4_ago[etf] = Ext::from_f64(0.0);
        g.consol_tax_inc_ytd[etf] = Ext::from_f64(0.0);
        return true;
    }
    if g.industry_num[etf] == 71
        && g.market_cap[etf] < g.exchange_rate * Ext::from_f64(15.0)
        && price_trend_direction(g, etf, 12, 3) < 0
    {
        g.cash[etf] = g.exchange_rate * Ext::from_f64(5000.0) + g.cash[etf];
        etf_drop_one_adviser_contract(g, ui, rng, etf, 0);
        *revived_id = -etf;
        g.news_msg = g.ent_symbol[etf].clone() + __content.string(4857);
    }
    false
}

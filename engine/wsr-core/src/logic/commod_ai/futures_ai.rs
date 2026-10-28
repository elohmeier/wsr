//! Futures expiry and the AI futures traders: `SettleExpiredFutures`, the per-corporation hedger/
//! speculator `AiFuturesTrading`, the commodity fund 1600 (`CommodityEtfTrading`) and the leveraged
//! index funds 1584/1585 (`LeveragedEtfFuturesUpdate`).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::if_same_then_else,
    clippy::needless_late_init,
    clippy::manual_range_contains,
    clippy::neg_multiply,
    clippy::needless_range_loop,
    clippy::manual_clamp,
    clippy::field_reassign_with_default,
    clippy::too_many_arguments
)]
use crate::pb::real::{Ext, cast};


use super::limits::{apply_futures_trade_impact, calc_futures_position_room};
use super::physical::ai_trade_phys_commod;
use super::prelude::*;
use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::{balance_cash_t_bills, calc_cur_month_of_year};
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::futures::{
    add_or_remove_futures_rec, calc_futures_contract_value, count_futures_positions, get_commod_data,
    get_futures_price,
};
use crate::logic::newgame::LocalTime;
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::assign_etf_adviser;
use crate::logic::sim_entity::ai_corp::is_recent_offering;
use crate::logic::sim_entity::bonds::issue_stock;
use crate::logic::sim_entity::liquidation::{liquidate_all_phys_commod, liquidate_corporation};
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::sim_entity::transactions::{liquidate_futures_positions, settle_futures_contract, settle_futures_contract_at};
use crate::logic::valuation::{calc_stock_price, clear_price_history, price_pct_change_months, price_trend_direction, price_trend_streak};
use crate::pb::numeric::{Cux, ceil};

// ---------------------------------------------------------------------------------------------------------
// SettleExpiredFutures
// ---------------------------------------------------------------------------------------------------------

/// Port of SettleExpiredFutures (0x6259d2).
/// Packs the record lists, then settles every futures record whose delivery date (month/100 + year,
/// SINGLE) is not after the current one (`SettleFuturesContract` with no commission and delivery; news
/// mode 1 for players and the selected/active entity), and deletes records of inactive owners. Packs
/// again if anything was settled.
/// Reads the host clock; the scheduled loop can also supply it through [`settle_expired_futures_at`].
pub fn settle_expired_futures(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let now = ui.local_time();
    settle_expired_futures_at(g, ui, rng, &now);
}

/// [`settle_expired_futures`] with the host's clock for physical-delivery margin calls.
pub fn settle_expired_futures_at(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    pack_holding_arrays(g, ui);
    let month_in_qtr: Ext = crate::pb::numeric::double(calc_cur_month_of_year(g, g.sim_co_count)); // DOUBLE
    let cur_month = to_long(ceil(month_in_qtr));
    let cur_date = cast::<f32, _>(cast::<Ext, _>(cur_month) * sf(0.01f32) + cast::<Ext, _>(g.cur_year));
    let mut settled_any = 0;
    let mut rec_idx = 1;
    while rec_idx <= 1000 {
        let fut_rec = g.futures_recs[rec_idx].clone();
        if !fut_rec.is_empty() {
            let owner_id = to_long(field_val(&fut_rec, 2));
            if g.entity_status[owner_id] == 1 {
                let deliv_month = to_long(field_val(&fut_rec, 4));
                let deliv_year = to_long(field_val(&fut_rec, 5));
                let contract_date = cast::<f32, _>(cast::<Ext, _>(deliv_month) * sf(0.01f32) + cast::<Ext, _>(deliv_year));
                if contract_date <= cur_date {
                    let mut news_mode = 0;
                    if g.sel_entity == owner_id || g.active_entity == owner_id {
                        news_mode = 1;
                    }
                    if field_val(&fut_rec, 2) < Ext::from_f64(11.0) {
                        news_mode = 1;
                    }
                    settle_futures_contract_at(g, ui, rng, rec_idx, news_mode, 1, 1, now);
                    rec_idx -= 1;
                    settled_any = 1;
                }
            } else {
                add_or_remove_futures_rec(g, &PbStr::new(), rec_idx);
                rec_idx -= 1;
            }
        }
        rec_idx += 1;
    }
    if settled_any == 1 {
        pack_holding_arrays(g, ui);
    }
}

// ---------------------------------------------------------------------------------------------------------
// AiFuturesTrading
// ---------------------------------------------------------------------------------------------------------

/// Port of AiFuturesTrading (0x62fac5).
/// One tick of AI futures trading for corporation `SimCoCount` (86..1600): AI-run hedging industries
/// (insurers 2, oil users 7/8/11/16/23/25/52/69, food 15/28, investment 37, mining 50; fund 1600 picks
/// one of them from `RandSeed`) in quarters matching the corp's parity, with business assets of at least
/// 250 x `ExchangeRate` and few positions, decide BUY/SELL from the futures price of a random delivery
/// month (0..5 years) against price bands, targets, the economy and (index) price trends; controlled
/// corps (`GetControllerType` 2, or fund 1600 a third of the time with a random risk bias) speculate.
/// Investment companies without a decision may trade [runtime resource wording] . The trade (credit
/// rating at least 7, fund 1600 at least 9) is 40% of business assets (insurers: net worth; fund 1600:
/// cash x 0.4 + T-bills) in contracts, capped by `CalcFuturesPositionRoom`, oil crises and a BBB rating;
/// it pays a 6% margin deposit (5% to `CommodMarginBal`), borrows a cash shortfall from the lending
/// bank, adds the record and moves the market. `time_secs` is the seconds of `TIME$` (the last digit
/// flips index decisions).
pub fn ai_futures_trading(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO handler: ShowRuntimeError("AiFuturesTrading", …) only
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 501 {
        return;
    }
    if Ext::from_f64(750.0) < val(g.futures_recs[0].as_bytes()) {
        return;
    }
    let corp_id = g.sim_co_count;
    if !(85 < corp_id && corp_id < 1601) {
        return;
    }
    if g.entity_status[corp_id] != 1 {
        return;
    }
    if 1580 < corp_id && 0 < get_etf_adviser_human_status(g, ui, rng, corp_id) {
        return;
    }
    let mut industry: i16 = g.industry_num[corp_id];
    if !matches!(industry, 2 | 7 | 8 | 11 | 15 | 16 | 23 | 25 | 28 | 37 | 50 | 52 | 69 | 71) {
        return;
    }
    let mut ctrl_type = 0;
    let mut pos_count = 0;
    let mut risk_bias: Ext = crate::pb::numeric::double(Ext::from_f64(0.0)); // DOUBLE
    if industry == 71 {
        if corp_id != 1600 {
            return;
        }
        if Ext::from_f64(1.0) < Ext::from_f64(3.0) * rng.rnd() {
            ctrl_type = 2;
        }
    } else {
        if g.cur_quarter % 2 != g.sim_co_count % 2 {
            return;
        }
        if 2 < industry && !(g.business_assets[corp_id] >= g.exchange_rate * Ext::from_f64(250.0)) {
            return;
        }
        ctrl_type = get_controller_type(g, corp_id);
        if ctrl_type == 1 {
            return;
        }
        pos_count = count_futures_positions(g, corp_id, 0);
        if 1 < pos_count && 2 < industry && ctrl_type == 0 && Ext::from_f64(1.0) < Ext::from_f64(3.0) * rng.rnd() {
            return;
        }
        if 2 < pos_count && Ext::from_f64(1.0) < Ext::from_f64(3.0) * rng.rnd() {
            return;
        }
        if 5 < pos_count {
            return;
        }
    }
    let month_in_qtr: Ext = crate::pb::numeric::double(calc_cur_month_of_year(g, g.sim_co_count)); // DOUBLE
    let cur_month = to_long(ceil(month_in_qtr));
    let mut deliv_month = cur_month;
    let mut deliv_year = rng.rnd_range(0, 5) + g.cur_year;
    if industry == 2 && g.cur_year + 2 < deliv_year {
        deliv_year = g.cur_year + 2;
    }
    if g.cur_year == deliv_year {
        let base = cur_month + 3;
        deliv_month = rng.rnd_range(1, 8) + base;
        if 12 < deliv_month {
            deliv_month = deliv_month - 12;
            deliv_year += 1;
        }
    } else if g.cur_year + 5 == deliv_year {
        deliv_month = cur_month - (rng.rnd_range(1, 10) + 1);
        if deliv_month < 1 {
            deliv_month = deliv_month + 12;
            deliv_year -= 1;
        }
    }
    if industry == 71 && corp_id == 1600 {
        if 10 <= count_futures_positions(g, corp_id, 0) {
            return;
        }
        industry = 7;
        if g.rand_seed % 5 == 1 {
            industry = 11;
        }
        if g.rand_seed % 5 == 2 {
            industry = 15;
        }
        if g.rand_seed % 5 == 3 {
            industry = 28;
        }
        if g.rand_seed % 5 == 4 {
            industry = 50;
        }
        if ctrl_type == 2 {
            risk_bias = crate::pb::numeric::double(Ext::from_f64(3.0) * rng.rnd());
        }
    }
    let mut commod: PbStr;
    // futures prices as CURRENCYX locals (VAL of the price text, 2 decimals)
    let mut oil_fut = Cux(0);
    let mut corn_fut = Cux(0);
    let mut wheat_fut = Cux(0);
    let mut silver_fut = Cux(0);
    let mut gold_fut = Cux(0);
    let mut _stock_fut: Ext = Ext::from_f64(0.0);
    match industry {
        7 | 8 | 11 | 16 | 23 | 25 | 52 | 69 => {
            commod = s("OIL");
            let p = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 7);
            oil_fut.set(val(p.as_bytes()));
        }
        15 | 28 => {
            commod = s("CORN");
            let p = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 8);
            corn_fut.set(val(p.as_bytes()));
            commod = s("WHEAT");
            let p = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 9);
            wheat_fut.set(val(p.as_bytes()));
        }
        2 | 37 => {
            commod = s("STOCKS");
            let p = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 9);
            _stock_fut = val(p.as_bytes());
        }
        50 => {
            commod = s("SILVER");
            let p = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 10);
            silver_fut.set(val(p.as_bytes()));
            commod = s("GOLD");
            let p = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 11);
            gold_fut.set(val(p.as_bytes()));
        }
        _ => return,
    }
    let oil_fut = oil_fut.get();
    let corn_fut = corn_fut.get();
    let wheat_fut = wheat_fut.get();
    let silver_fut = silver_fut.get();
    let gold_fut = gold_fut.get();
    let rb = risk_bias;
    let oil = cast::<Ext, _>(g.oil_price);
    let oil_t = cast::<Ext, _>(g.oil_target_price);
    let peak = g.peak_oil_crisis;
    let mpb = cast::<Ext, _>(g.metals_price_base);
    let mut action = PbStr::new();
    // (`'decided` = the original's GOTO past the decision block)
    'decided: {
        match industry {
            7 | 8 => {
                if ctrl_type == 0 {
                    if oil_fut <= Ext::from_f64(80.0) || 1 < pos_count || 0 < peak {
                        if oil_fut < Ext::from_f64(120.0) || oil * sf(1.1f32) < oil_t || peak == 1 {
                            return;
                        }
                        if peak == -1 && Ext::from_f64(0.0) < cast::<Ext, _>(g.oil_crisis_level) && sf(2.2f32) < oil {
                            return;
                        }
                        action = s("SELL");
                    } else {
                        action = s("SELL");
                    }
                } else {
                    if Ext::from_f64(140.0) - Ext::from_f64(6.0) * rb < oil_fut && oil_t < oil * sf(1.35f32) && peak < 1 {
                        action = s("SELL");
                    }
                    if oil_fut < Ext::from_f64(4.0) * rb + Ext::from_f64(60.0) && oil * sf(0.75f32) < oil_t {
                        action = s("BUY");
                    }
                    if peak == 1 && oil < sf(1.6f32) {
                        action = s("BUY");
                    }
                }
            }
            11 | 16 | 23 | 25 | 52 | 69 => {
                if ctrl_type == 0 {
                    if Ext::from_f64(130.0) <= oil_fut || 1 < pos_count {
                        if rb + Ext::from_f64(80.0) < oil_fut || oil_t < oil * sf(0.9f32) {
                            return;
                        }
                        action = s("BUY");
                    } else {
                        action = s("BUY");
                    }
                } else {
                    if Ext::from_f64(140.0) - Ext::from_f64(6.0) * rb < oil_fut && oil_t < oil * sf(1.35f32) && peak < 1 {
                        action = s("SELL");
                    }
                    if oil_fut < Ext::from_f64(4.0) * rb + Ext::from_f64(60.0) && oil * sf(0.6f32) < oil_t {
                        action = s("BUY");
                    }
                    if peak == 1 && oil < sf(1.6f32) {
                        action = s("BUY");
                    }
                }
            }
            15 => {
                if ctrl_type == 0 {
                    commod = s("WHEAT");
                    if wheat_fut <= rb + Ext::from_f64(5.0) || 1 < pos_count {
                        if sf(7.4f32) < wheat_fut
                            && (cast::<Ext, _>(g.wheat_target_price)) < cast::<Ext, _>(g.wheat_price) * sf(1.05f32)
                            && Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd()
                        {
                            action = s("SELL");
                            break 'decided;
                        }
                        commod = s("CORN");
                        if rb + sf(4.5f32) < corn_fut && pos_count == 0 {
                            action = s("SELL");
                        }
                        if sf(7.1f32) < corn_fut && (cast::<Ext, _>(g.corn_target_price)) < cast::<Ext, _>(g.corn_price) * sf(1.05f32) {
                            action = s("SELL");
                        }
                    } else {
                        action = s("SELL");
                    }
                } else {
                    if Ext::from_f64(9.0) - rb < wheat_fut
                        && (cast::<Ext, _>(g.wheat_target_price)) < cast::<Ext, _>(g.wheat_price) * sf(1.02f32)
                        && Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd()
                    {
                        action = s("SELL");
                        break 'decided;
                    }
                    if wheat_fut < rb + sf(3.8f32) {
                        action = s("BUY");
                    } else {
                        commod = s("CORN");
                        if sf(8.5f32) - rb < corn_fut
                            && (cast::<Ext, _>(g.corn_target_price)) < cast::<Ext, _>(g.corn_price) * sf(1.02f32)
                            && Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd()
                        {
                            action = s("SELL");
                            break 'decided;
                        }
                        if corn_fut < rb + sf(3.5f32) {
                            action = s("BUY");
                        }
                    }
                }
            }
            28 => {
                commod = s("WHEAT");
                if ctrl_type == 0 {
                    if wheat_fut < rb + sf(4.5f32) && pos_count < 2 && Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                        action = s("BUY");
                        break 'decided;
                    }
                    if sf(5.2f32) <= wheat_fut || cast::<Ext, _>(g.wheat_target_price) <= cast::<Ext, _>(g.wheat_price) * sf(0.98f32) {
                        commod = s("CORN");
                        if corn_fut < rb + Ext::from_f64(4.0) && pos_count < 2 && Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                            action = s("BUY");
                        }
                        if corn_fut < sf(4.7f32) && (cast::<Ext, _>(g.corn_price) * sf(0.98f32)) < cast::<Ext, _>(g.corn_target_price) {
                            action = s("BUY");
                        }
                    } else {
                        action = s("BUY");
                    }
                } else {
                    if Ext::from_f64(9.0) - rb < wheat_fut
                        && (cast::<Ext, _>(g.wheat_target_price)) < cast::<Ext, _>(g.wheat_price) * sf(1.02f32)
                        && Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd()
                    {
                        action = s("SELL");
                        break 'decided;
                    }
                    if wheat_fut < rb + sf(3.7f32) {
                        action = s("BUY");
                    } else {
                        commod = s("CORN");
                        if corn_fut <= sf(8.5f32) - rb || cast::<Ext, _>(g.corn_price) * sf(1.02f32) <= cast::<Ext, _>(g.corn_target_price) {
                            if corn_fut < rb + sf(3.5f32) {
                                action = s("BUY");
                            }
                        } else {
                            action = s("SELL");
                        }
                    }
                }
            }
            2 | 37 => {
                commod = s("STOCKS");
                let gdp = cast::<Ext, _>(g.gdp_growth);
                let gdp_t = cast::<Ext, _>(g.gdp_growth_target);
                let rate = cast::<Ext, _>(g.base_interest_rate);
                if ctrl_type == 0 {
                    if Ext::from_f64(3.0) < gdp && pos_count < 3 {
                        action = s("BUY");
                    }
                    if Ext::from_f64(4.0) < gdp && pos_count < 4 && Ext::from_f64(2.0) * rng.rnd() < Ext::from_f64(1.0) {
                        action = s("BUY");
                    }
                    if Ext::from_f64(5.0) < gdp && pos_count < 4 && oil < Ext::from_f64(1.0) {
                        action = s("BUY");
                    }
                    if Ext::from_f64(2.0) < gdp && gdp < gdp_t && rate < Ext::from_f64(7.0) {
                        action = s("BUY");
                    }
                    if gdp < -Ext::from_f64(1.0) && pos_count == 0 {
                        action = s("SELL");
                    }
                    if gdp < -Ext::from_f64(2.0) && pos_count < 2 && Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                        action = s("SELL");
                    }
                    if gdp < -Ext::from_f64(2.0) && gdp_t < gdp && pos_count < 3 {
                        action = s("SELL");
                    }
                    if gdp < sf(0.5f32) && gdp_t < gdp && sf(7.5f32) < rate && pos_count < 3 {
                        action = s("SELL");
                    }
                    if price_pct_change_months(g, 0, 8) < -10 && 0 < price_trend_direction(g, 0, 2, 1) {
                        action = s("BUY");
                    }
                    if 10 < price_pct_change_months(g, 0, 8) && price_trend_direction(g, 0, 2, 1) < 0 {
                        action = s("SELL");
                    }
                    let time_digit = time_secs.rem_euclid(10);
                    if 7 < time_digit && action == "BUY" {
                        action = s("SELL");
                    }
                    if time_digit < 2 && action == "SELL" {
                        action = s("BUY");
                    }
                    if action.is_empty() {
                        let trend_streak = price_trend_streak(g, 0);
                        let r = rng.rnd_range(1, 3);
                        if r + 4 < trend_streak && Ext::from_f64(2.0) < gdp_t && rate < Ext::from_f64(8.0) {
                            action = s("BUY");
                        }
                        if trend_streak < -5 && gdp_t < Ext::from_f64(1.0) && Ext::from_f64(7.0) < rate {
                            action = s("SELL");
                        }
                    }
                } else if Ext::from_f64(12.0) < g.cap_hist_close[(0, 0)] || Ext::from_f64(0.0) < g.cap_hist_close[(60, 0)] {
                    let mp = g.monetary_policy;
                    let mut check_down = true;
                    if 4 < price_trend_streak(g, 0) {
                        if !(price_pct_change_months(g, 0, 8) <= 15 || gdp_t <= gdp) {
                            check_down = false;
                            if mp < 1 && rate < sf(8.5f32) {
                                action = s("BUY");
                            }
                        }
                    }
                    if check_down && price_trend_streak(g, 0) < -4 {
                        if price_pct_change_months(g, 0, 8) < -15 && gdp_t < gdp && 1 < mp && Ext::from_f64(7.0) < rate {
                            action = s("SELL");
                        }
                    }
                    if 0 < pos_count && Ext::from_f64(2.0) < Ext::from_f64(5.0) * rng.rnd() {
                        return;
                    }
                }
                // LAB_006316b2
                if industry == 2 && action == "SELL" {
                    action = PbStr::new();
                }
                if action.is_empty()
                    && Ext::from_f64(4.0) < Ext::from_f64(5.0) * rng.rnd()
                    && 7 < g.credit_rating_num[corp_id]
                    && get_controller_type(g, corp_id) != 1
                    && industry == 37
                {
                    ai_trade_phys_commod(g, ui, rng, corp_id);
                }
            }
            _ => {
                // 50
                commod = s("GOLD");
                let gold = cast::<Ext, _>(g.gold_price);
                let gold_t = cast::<Ext, _>(g.gold_target_price);
                let silver = cast::<Ext, _>(g.silver_price);
                let silver_t = cast::<Ext, _>(g.silver_target_price);
                if ctrl_type == 0 {
                    if rb * Ext::from_f64(300.0) + Ext::from_f64(1500.0) + mpb < gold_fut && pos_count == 0 && Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                        action = s("SELL");
                        break 'decided;
                    }
                    if gold_fut <= mpb + Ext::from_f64(1350.0) || gold * sf(1.02f32) <= gold_t {
                        if gold_fut - mpb < rb * Ext::from_f64(50.0) + Ext::from_f64(800.0) && Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                            action = s("BUY");
                            break 'decided;
                        }
                        commod = s("SILVER");
                        if rb * Ext::from_f64(5.0) + Ext::from_f64(17.0) < silver_fut && pos_count == 0 {
                            action = s("SELL");
                        }
                        if mpb * sf(0.01f32) + sf(24.5f32) < silver_fut && silver_t < silver * sf(1.02f32) {
                            action = s("SELL");
                        }
                        if silver_fut < mpb * sf(0.01f32) + rb * Ext::from_f64(3.0) + Ext::from_f64(10.0) && pos_count == 0 && Ext::from_f64(2.0) * rng.rnd() < Ext::from_f64(1.0) {
                            action = s("BUY");
                        }
                    } else {
                        action = s("SELL");
                    }
                } else {
                    if (Ext::from_f64(1945.0) - rb * Ext::from_f64(100.0)) + mpb < gold_fut && gold_t < gold * sf(1.05f32) && Ext::from_f64(1.0) < Ext::from_f64(3.0) * rng.rnd() {
                        action = s("SELL");
                        break 'decided;
                    }
                    if gold_fut < rb * Ext::from_f64(50.0) + Ext::from_f64(730.0) + mpb {
                        action = s("BUY");
                    } else {
                        commod = s("SILVER");
                        if silver_fut <= (mpb * sf(0.01f32) + sf(27.5f32)) - rb * Ext::from_f64(2.0) || silver * sf(1.05f32) <= silver_t {
                            if silver_fut < mpb * sf(0.01f32) + rb + sf(12.3f32) && silver * sf(0.97f32) < silver_t {
                                action = s("BUY");
                            }
                        } else {
                            action = s("SELL");
                        }
                    }
                }
            }
        }
    }
    // LAB_00631c6b
    if action.is_empty() {
        return;
    }
    let commod_data = get_commod_data(g, ui, &commod, 9);
    let commod_idx = to_quad(field_val(&commod_data, 3));
    let basis = cast::<i32, _>(g.futures_basis[cast::<i32, _>(commod_idx)]);
    if action == "BUY" {
        if 250 < basis && g.cur_year + 1 < deliv_year {
            deliv_year -= 1;
        }
    } else if 150 < basis && deliv_year < g.cur_year + 4 {
        deliv_year += 1;
    }
    let fut_price_str = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 12);
    if g.credit_rating_num[corp_id] < 7 {
        return;
    }
    calc_entity_financials(g, ui, rng, corp_id);
    if g.credit_rating_num[corp_id] < 7 {
        return;
    }
    if corp_id == 1600 && g.industry_num[1600] == 71 && g.credit_rating_num[corp_id] < 9 {
        return;
    }
    let tmp_str = commod.clone()
        + "|"
        + &str_trim(cast::<Ext, _>(corp_id))
        + "|1|"
        + &str_trim(cast::<Ext, _>(deliv_month))
        + "|"
        + &str_trim(cast::<Ext, _>(deliv_year));
    let contract_value = calc_futures_contract_value(g, ui, rng, &tmp_str);
    if contract_value == Ext::from_f64(0.0) {
        return;
    }
    let mut max_qty: i32;
    if corp_id == 1600 && g.industry_num[1600] == 71 {
        max_qty = to_long(g.cash[corp_id] * sf(0.4f32) + g.t_bills[corp_id] / contract_value);
    } else if industry == 2 {
        max_qty = to_long((g.net_worth * sf(0.4f32)) / contract_value);
    } else {
        max_qty = to_long((g.business_assets[corp_id] * sf(0.4f32)) / contract_value);
    }
    let pos_room = calc_futures_position_room(g, corp_id, &commod, &action);
    if pos_room > max_qty {
        max_qty -= 1;
    } else {
        max_qty = pos_room;
    }
    if commod == "OIL" && g.peak_oil_crisis != 0 && Ext::from_f64(0.0) < cast::<Ext, _>(g.oil_crisis_level) && 10 < max_qty {
        max_qty = 10;
    }
    if !(2 <= max_qty && g.liquid_funds >= cast::<Ext, _>(max_qty) * contract_value * sf(0.08f32)) {
        return;
    }
    let mut qty = max_qty;
    if action == "SELL" {
        qty = qty.wrapping_mul(-1);
    }
    if g.credit_rating_str == "BBB" {
        qty = to_long(sf(0.25f32) * cast::<Ext, _>(qty));
    }
    if g.peak_oil_crisis == 1 && action == "BUY" && commod == "OIL" && ctrl_type == 0 && 100 < qty {
        qty = 100;
    }
    if !((cast::<Ext, _>(qty)).abs() >= Ext::from_f64(5.0)) {
        return;
    }
    if (g.cur_year + g.sim_co_count + g.cur_quarter) % 40 == 39 {
        if industry == 37 {
            g.news_msg = g.ent_symbol[corp_id].clone() + __content.string(2541);
            let mut news_tail = PbStr::new();
            if action == "BUY" {
                news_tail = s(__content.string(2542));
            }
            if action == "SELL" {
                news_tail = s(__content.string(2543));
            }
            if Ext::from_f64(4.0) < Ext::from_f64(5.0) * rng.rnd() {
                news_tail = s("are a '") + &lc(&action) + __content.string(2544);
            }
            g.news_msg = g.news_msg.clone() + &news_tail;
        } else if 0 < pos_count {
            g.news_msg = g.ent_symbol[corp_id].clone() + __content.string(2545) + &lc(&commod) + __content.string(2546);
            if commod == "STOCKS" {
                g.news_msg = replace(g.news_msg.as_bytes(), b"stocks", __content.bytes(2547));
            }
        }
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
    }
    let total_value = (cast::<Ext, _>(qty)).abs() * contract_value;
    g.cash[corp_id] = g.cash[corp_id] - total_value * sf(0.06f32);
    g.commod_margin_bal[corp_id] = total_value * sf(0.05f32) + g.commod_margin_bal[corp_id];
    balance_cash_t_bills(g, corp_id);
    if g.cash[corp_id] < Ext::from_f64(0.0) {
        let shortfall = g.cash[corp_id].abs();
        g.bank_debt[corp_id] = shortfall + g.bank_debt[corp_id];
        let lb = cast::<i32, _>(g.lending_bank[corp_id]);
        g.cash[lb] = g.cash[lb] - shortfall;
        g.cash[corp_id] = Ext::from_f64(0.0);
    }
    let value_str = fmt_trim(total_value, b"##############.00");
    let new_rec = commod.clone()
        + "|"
        + &str_trim(cast::<Ext, _>(corp_id))
        + "|"
        + &str_trim(cast::<Ext, _>(qty))
        + "|"
        + &str_trim(cast::<Ext, _>(deliv_month))
        + "|"
        + &str_trim(cast::<Ext, _>(deliv_year))
        + "|"
        + &value_str
        + "|"
        + &fut_price_str;
    add_or_remove_futures_rec(g, &new_rec, 0);
    if g.exchange_rate * Ext::from_f64(500.0) <= g.exchange_rate * total_value {
        let direction = if action == "SELL" { -1 } else { 1 };
        apply_futures_trade_impact(g, ui, rng, &commod, direction, cast::<i64, _>(qty));
    }
}

// ---------------------------------------------------------------------------------------------------------
// CommodityEtfTrading
// ---------------------------------------------------------------------------------------------------------

/// Port of CommodityEtfTrading_OpenPosition (GOSUB 0x64c50f).
/// Opens a long position in `commod_code` (random delivery 1..12 / 2..4 years out) worth 20% of the
/// fund's liquid funds (2..100,000 contracts, 10,000 except oil), paying the 6% deposit (5% margin).
/// Returns false when the original leaves the whole routine (more than 900 futures records).
fn etf_open_position(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, etf_id: i32, commod_code: &PbStr, liquid_funds: Ext) -> bool {
    if Ext::from_f64(900.0) < val(g.futures_recs[0].as_bytes()) {
        return false;
    }
    let delivery_month = rng.rnd_range(1, 12);
    let delivery_year = rng.rnd_range(2, 4) + g.cur_year;
    let fut_price = get_futures_price(g, ui, rng, commod_code, delivery_month, delivery_year, 12);
    let mut rec_text = commod_code.clone()
        + "|"
        + &str_trim(cast::<Ext, _>(etf_id))
        + "|1|"
        + &str_trim(cast::<Ext, _>(delivery_month))
        + "|"
        + &str_trim(cast::<Ext, _>(delivery_year));
    let contract_value = calc_futures_contract_value(g, ui, rng, &rec_text);
    let mut contracts = to_long((liquid_funds * sf(0.2f32)) / contract_value);
    contracts = contracts.wrapping_sub(1);
    if contracts < 2 {
        return true;
    }
    if 100000 < contracts {
        contracts = 100000;
    }
    if *commod_code != "OIL" && 10000 < contracts {
        contracts = 10000;
    }
    let qty = contracts;
    let position_value = (cast::<Ext, _>(qty)).abs() * contract_value;
    g.cash[etf_id] = g.cash[etf_id] - position_value * sf(0.06f32);
    g.commod_margin_bal[etf_id] = position_value * sf(0.05f32) + g.commod_margin_bal[etf_id];
    if g.cash[etf_id] < Ext::from_f64(0.0) {
        balance_cash_t_bills(g, etf_id);
    }
    if g.cash[etf_id] < Ext::from_f64(0.0) {
        let shortfall = g.cash[etf_id].abs();
        g.bank_debt[etf_id] = shortfall + g.bank_debt[etf_id];
        let lb = cast::<i32, _>(g.lending_bank[etf_id]);
        g.cash[lb] = g.cash[lb] - shortfall;
        g.cash[etf_id] = Ext::from_f64(0.0);
    }
    rec_text = fmt_trim(position_value, b"##############.00");
    let fut_rec = commod_code.clone()
        + "|"
        + &str_trim(cast::<Ext, _>(etf_id))
        + "|"
        + &str_trim(cast::<Ext, _>(qty))
        + "|"
        + &str_trim(cast::<Ext, _>(delivery_month))
        + "|"
        + &str_trim(cast::<Ext, _>(delivery_year))
        + "|"
        + &rec_text
        + "|"
        + &fut_price;
    add_or_remove_futures_rec(g, &fut_rec, 0);
    true
}

/// Port of CommodityEtfTrading (0x64c333) with its GOSUB CommodityEtfTrading_OpenPosition (0x64c50f) and
/// exit stub EmptyStub_64c9b7 (0x64c9b7).
/// The commodity fund 1600 (industry 71, active, at most 500 futures records): with a human adviser it
/// sells its [runtime resource wording]  stops; in the first quarter of the game it opens long positions
/// in OIL, GOLD, SILVER, WHEAT and CORN (20% of cash + T-bills each), otherwise it runs
/// `AiFuturesTrading` three times (bumping `RandSeed` before each).
pub fn commodity_etf_trading(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let etf_id = 1600;
    if g.industry_num[1600] != 71 || g.entity_status[etf_id] != 1 {
        return;
    }
    if Ext::from_f64(500.0) < val(g.futures_recs[0].as_bytes()) {
        return;
    }
    if 0 < get_etf_adviser_human_status(g, ui, rng, etf_id) {
        liquidate_all_phys_commod(g, ui, rng, etf_id);
    }
    if 0 < get_etf_adviser_human_status(g, ui, rng, etf_id) {
        return;
    }
    if g.cur_quarter == 1 && g.start_year == g.cur_year {
        let liquid_funds = g.cash[etf_id] + g.t_bills[etf_id];
        for c in ["OIL", "GOLD", "SILVER", "WHEAT", "CORN"] {
            if !etf_open_position(g, ui, rng, etf_id, &s(c), liquid_funds) {
                return;
            }
        }
    } else {
        for _ in 0..3 {
            g.rand_seed = g.rand_seed.wrapping_add(1);
            ai_futures_trading(g, ui, rng, time_secs);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// LeveragedEtfFuturesUpdate
// ---------------------------------------------------------------------------------------------------------

/// Port of EtfLifecycle_IsSymbolUnique (GOSUB 0x6550b8).
/// 1 if no entity 11..1600 has the ticker `symbol`.
fn is_symbol_unique(g: &Globals, symbol: &PbStr) -> i32 {
    let mut is_unique = 1;
    for ent_idx in 11..=1600 {
        if g.ent_symbol[ent_idx] == *symbol {
            is_unique = 0;
        }
    }
    is_unique
}

/// Port of EtfLifecycle_ReorganizeCollapsed (GOSUB 0x6548d0).
/// Recapitalizes a collapsed leveraged fund: without options, bonds, debt and outside holders and with a
/// recent offering of an earlier control year it gets 40,000 x `ExchangeRate` new cash and a fresh price
/// history ("… [runtime resource wording]  …" / "… [runtime resource wording] …"); otherwise, if not
/// recently offered, it is put on the offering list and issues 25% new stock. Then the news, a price
/// update (`CalcStockPrice_42c72a`, not ported) and a new adviser (`AssignEtfAdviser`).
fn reorganize_collapsed(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, etf_id: i32, time_secs: i32) {
    let __content = crate::content::current();
    update_corp_control(g, ui, rng, etf_id);
    find_option_positions(g, ui, 0, etf_id, 0, 0);
    let mut handled = false;
    if g.has_options_outstanding[etf_id] == 0
        && g.public_pct == 100
        && g.bonds_issued[etf_id] == 0
        && g.bank_debt[etf_id] == Ext::from_f64(0.0)
        && is_recent_offering(g, etf_id) == 1
        && g.ctrl_since_year[etf_id] < g.cur_year
    {
        handled = true;
        g.cash[etf_id] = g.exchange_rate * Ext::from_f64(40000.0) + g.cash[etf_id];
        clear_price_history(g, etf_id);
        let mut headline_word = s(__content.string(2548));
        if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
            headline_word = replace(headline_word.as_bytes(), b"implode", __content.bytes(2549));
        }
        if g.rand_seed % 4 == 1 {
            headline_word = s(__content.string(2550));
        }
        if g.rand_seed % 4 == 2 {
            headline_word = s(__content.string(2551));
        }
        if g.rand_seed % 4 == 3 {
            headline_word = s(__content.string(2552));
        }
        g.news_msg = g.ent_symbol[etf_id].clone() + __content.string(2553) + &headline_word;
        let mut wiped_out_check = true;
        if !(g.start_year > g.ctrl_since_year[etf_id])
            && Ext::from_f64(2.0) < Ext::from_f64(3.0) * rng.rnd()
            && g.net_worth < g.exchange_rate * Ext::from_f64(5000.0)
        {
            g.news_msg = g.ent_symbol[etf_id].clone() + __content.string(2554);
            wiped_out_check = false;
        }
        if wiped_out_check && g.net_worth < g.exchange_rate * Ext::from_f64(7000.0) {
            g.news_msg = g.ent_symbol[etf_id].clone() + __content.string(2555);
            if etf_id == 1584 && price_pct_change_months(g, 0, 6) < -8 {
                g.news_msg = g.news_msg.clone() + __content.string(2556);
            } else if etf_id == 1585 && 10 < price_pct_change_months(g, 0, 6) {
                g.news_msg = g.news_msg.clone() + __content.string(2557);
            } else {
                g.news_msg = g.news_msg.clone() + __content.string(2558);
            }
            if g.start_year > g.ctrl_since_year[etf_id] {
                g.news_msg = g.news_msg.clone() + __content.string(2559);
            } else {
                g.news_msg = g.news_msg.clone() + __content.string(2560);
            }
        }
        g.extraordinary_pl[etf_id] = 0;
    }
    if !handled {
        if is_recent_offering(g, etf_id) != 0 {
            return;
        }
        let slot = rng.rnd_range(4, 8);
        if g.recent_offerings[slot] == 0 {
            g.recent_offerings[slot] = etf_id;
        } else {
            g.recent_offerings[10] = etf_id;
        }
        g.tmp_ext1 = Ext::from_f64(25.0);
        issue_stock(g, ui, rng, etf_id, 1);
        g.news_msg = g.ent_symbol[etf_id].clone() + __content.string(2561);
        if g.rand_seed % 4 == 0 {
            g.news_msg = g.news_msg.clone() + __content.string(2562);
        }
        if g.rand_seed % 4 == 1 {
            g.news_msg = g.news_msg.clone() + __content.string(2563);
        }
        if g.rand_seed % 4 == 2 {
            g.news_msg = g.news_msg.clone() + __content.string(2564);
        }
        if g.rand_seed % 4 == 3 {
            g.news_msg = g.news_msg.clone() + __content.string(2565);
        }
    }
    post_news(g, ui, 0, 0);
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, etf_id, time_secs);
    let old_adviser = g.fund_adviser[etf_id];
    assign_etf_adviser(g, ui, rng, etf_id, old_adviser, 1);
}

/// Port of LeveragedEtfFuturesUpdate (0x653409) with its GOSUBs EtfLifecycle_ReorganizeCollapsed
/// (0x6548d0), EtfLifecycle_IsSymbolUnique (0x6550b8) and exit stub EtfLifecycle_Exit (0x65517d).
/// The leveraged index funds 1584 (long, "BULL") and 1585 (short, "BEAR"): a liquidated one is restarted
/// with 35,000..45,000 x `ExchangeRate` cash and a new name (unless futures trading is halted in another
/// quarter, `EtfFuturesHaltQtr`). An active one settles non-index futures, sums its STOCKS exposure by
/// delivery month (`expiryCounts(0..10)`), and depending on its net worth stops futures trading,
/// liquidates, or is reorganized; otherwise it keeps 3x net worth exposure: above 105% it trims
/// (`LiquidateFuturesPositions` mode 4, then recounts), below 95% it adds a contract for a free month
/// 3..8 ahead (13% of the target, halved while the 6% deposit exceeds the liquid funds, at most
/// 1,000), borrowing within the credit line.
///
/// A STOCKS record whose delivery is more than 10 months ahead within next year (or already past) is an
/// out-of-range `expiryCounts` index in the original: error 9, ShowRuntimeError and the routine ends; the
/// port returns there.
///
/// `now`: the local time (`VAL(RIGHT$(TIME$, 2))` for the reorganized fund's `CalcStockPrice`; the
/// liquidation's callees).
pub fn leveraged_etf_futures_update(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &crate::logic::newgame::LocalTime) {
    let __content = crate::content::current();
    let time_secs = now.time_secs();
    // ON ERROR GOTO handler: ShowRuntimeError, then exit
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 750 {
        return;
    }
    let commod_code = s("STOCKS");
    let cur_month_raw: Ext = crate::pb::numeric::double(calc_cur_month_of_year(g, g.sim_co_count)); // DOUBLE
    let cur_month = to_long(ceil(cur_month_raw));
    let mut etf_id = 1584;
    while etf_id <= 1585 {
        'next: {
            if g.entity_status[etf_id] == 2
                && (g.etf_futures_halt_qtr < 1 || g.cur_quarter == g.etf_futures_halt_qtr)
            {
                let r = rng.rnd_range(1, 10);
                g.cash[etf_id] = g.exchange_rate * (Ext::from_f64(1000.0) * cast::<Ext, _>(r) + Ext::from_f64(35000.0));
                g.entity_status[etf_id] = 1;
                g.industry_num[etf_id] = 71;
                g.market_cap[etf_id] = Ext::from_f64(50.0) * rng.rnd() + Ext::from_f64(350.0);
                let symbol;
                if etf_id == 1584 {
                    if g.ent_name[1584] == __content.string(2566) {
                        g.ent_name[etf_id] = s(__content.string(2567));
                    } else {
                        g.ent_name[etf_id] = s(__content.string(2566));
                    }
                    symbol = s("BULL");
                } else {
                    if g.ent_name[1585] == __content.string(2568) {
                        g.ent_name[etf_id] = s(__content.string(2569));
                    } else {
                        g.ent_name[etf_id] = s(__content.string(2568));
                    }
                    symbol = s("BEAR");
                }
                if is_symbol_unique(g, &symbol) == 1 {
                    g.ent_symbol[etf_id] = symbol;
                }
                g.name_list_dirty = 1;
                g.news_msg = s(__content.string(2570))
                    + &g.ent_name[etf_id]
                    + __content.string(2571)
                    + &g.ent_symbol[etf_id];
                if Ext::from_f64(10.0) * rng.rnd() <= Ext::from_f64(9.0) {
                    post_news(g, ui, etf_id, 0);
                } else {
                    let mut m = g.news_msg.clone();
                    show_quick_message(g, ui, &mut m);
                    g.news_msg = m;
                    post_news(g, ui, 0, 0);
                }
            }
            if !(g.entity_status[etf_id] == 1 && g.industry_num[etf_id] == 71) {
                break 'next;
            }
            let direction = if etf_id == 1585 { -1 } else { 1 };
            // LAB_00653920 (re-entered after trimming)
            loop {
                let mut total_exposure: i64 = 0;
                let mut expiry_counts = [0i32; 11]; // RESET expiryCounts() (DIM 0 TO 10)
                let mut rec_idx = 1;
                while rec_idx <= 1000 {
                    let fut_rec = g.futures_recs[rec_idx].clone();
                    if !fut_rec.is_empty() && field_val(&fut_rec, 2) == cast::<Ext, _>(etf_id) {
                        if field(&fut_rec, 1) == "STOCKS" {
                            let mut delivery_month = to_long(field_val(&fut_rec, 4));
                            let delivery_year = to_long(field_val(&fut_rec, 5));
                            let mut month_offset = 0;
                            if g.cur_year == delivery_year {
                                month_offset = delivery_month - cur_month;
                            } else if g.cur_year + 1 == delivery_year {
                                delivery_month = delivery_month + 12;
                                month_offset = delivery_month - cur_month;
                            }
                            if !(0..=10).contains(&month_offset) {
                                // error 9 -> ON ERROR: ShowRuntimeError and exit
                                return;
                            }
                            expiry_counts[cast::<usize, _>(month_offset)] += 1;
                            let contract_value = calc_futures_contract_value(g, ui, rng, &fut_rec);
                            total_exposure = to_quad(contract_value.abs() + cast::<Ext, _>(total_exposure));
                        } else {
                            settle_futures_contract(g, ui, rng, rec_idx, 2, 0, 0);
                            rec_idx -= 1;
                        }
                    }
                    rec_idx += 1;
                }
                total_exposure = to_quad((cast::<Ext, _>(total_exposure)).abs());
                calc_entity_financials(g, ui, rng, etf_id);
                if g.net_worth < Ext::from_f64(0.0) {
                    break 'next;
                }
                let er = g.exchange_rate;
                let r = rng.rnd_range(1, 1000);
                if g.net_worth < er * (cast::<Ext, _>(r) + Ext::from_f64(9500.0))
                    && is_recent_offering(g, etf_id) == 1
                    && g.cur_year == g.ctrl_since_year[etf_id]
                {
                    if 0 < count_futures_positions(g, etf_id, 0) {
                        if 0 < liquidate_futures_positions(g, ui, rng, etf_id, 2, 0) {
                            g.news_msg = g.ent_symbol[etf_id].clone()
                                + __content.string(2572);
                            if g.suppress_popups == 0
                                && Ext::from_f64(3.0) < (cast::<Ext, _>(g.etf_futures_quit_year - g.cur_year)).abs()
                                && g.start_year + 3 <= g.cur_year
                            {
                                let mut m = g.news_msg.clone()
                                    + __content.string(2573)
                                    + __content.string(2574);
                                show_quick_message(g, ui, &mut m);
                                g.etf_futures_quit_year = g.cur_year;
                            }
                            post_news(g, ui, 0, 0);
                            g.etf_futures_halt_qtr = g.cur_quarter + 1;
                        }
                    } else if g.etf_futures_halt_qtr == g.cur_quarter || g.etf_futures_halt_qtr == 0 {
                        liquidate_corporation(g, ui, rng, etf_id, 2, now);
                        g.etf_futures_halt_qtr = g.cur_quarter + 2;
                    }
                    if 4 < g.etf_futures_halt_qtr {
                        g.etf_futures_halt_qtr = g.etf_futures_halt_qtr - 4;
                    }
                    break 'next;
                }
                if count_futures_positions(g, etf_id, 0) == 0
                    && is_recent_offering(g, etf_id) == 1
                    && g.net_worth < er * Ext::from_f64(8000.0)
                {
                    if g.rand_seed % 7 < 1 {
                        g.news_msg = s(__content.string(2575))
                            + &g.ent_name[etf_id]
                            + " ("
                            + &g.ent_symbol[etf_id]
                            + __content.string(2576);
                        let cp = g.cur_player;
                        post_news(g, ui, cp, 0);
                        liquidate_corporation(g, ui, rng, etf_id, 2, now);
                        g.etf_futures_halt_qtr = g.cur_quarter + 2;
                        if 4 < g.etf_futures_halt_qtr {
                            g.etf_futures_halt_qtr = g.etf_futures_halt_qtr - 4;
                        }
                    }
                    break 'next;
                }
                if g.net_worth < er * Ext::from_f64(5000.0) && 0 < count_futures_positions(g, etf_id, 0) {
                    liquidate_futures_positions(g, ui, rng, etf_id, 2, 0);
                    break 'next;
                }
                if g.net_worth < er * Ext::from_f64(10501.0) && count_futures_positions(g, etf_id, 0) == 0 && Ext::from_f64(1.0) < Ext::from_f64(4.0) * rng.rnd() {
                    break 'next;
                }
                let r = rng.rnd_range(1, 1000);
                if g.net_worth < g.exchange_rate * (cast::<Ext, _>(r) + Ext::from_f64(9500.0)) {
                    reorganize_collapsed(g, ui, rng, etf_id, time_secs);
                    calc_entity_financials(g, ui, rng, etf_id);
                    if g.net_worth < g.exchange_rate * Ext::from_f64(11000.0) {
                        break 'next;
                    }
                }
                let target_exposure = to_quad(g.net_worth * Ext::from_f64(3.0));
                if cast::<Ext, _>(target_exposure) * sf(1.05f32) < cast::<Ext, _>(total_exposure) {
                    if 0 < liquidate_futures_positions(g, ui, rng, etf_id, 4, 0) {
                        continue;
                    }
                    break 'next;
                }
                if !((cast::<Ext, _>(total_exposure)) < cast::<Ext, _>(target_exposure) * sf(0.95f32)) {
                    break 'next;
                }
                if !(val(g.futures_recs[0].as_bytes()) <= Ext::from_f64(800.0)) {
                    break 'next;
                }
                if !(g.liquid_funds >= g.exchange_rate * Ext::from_f64(500.0)) {
                    break 'next;
                }
                let mut month_offset = rng.rnd_range(3, 8);
                let mut _total_contracts = 0;
                for month_idx in 3..=8 {
                    if expiry_counts[month_idx] == 0 {
                        month_offset = cast::<i32, _>(month_idx);
                    }
                    _total_contracts += expiry_counts[month_idx];
                }
                let mut delivery_year = g.cur_year;
                let mut delivery_month = month_offset + cur_month;
                if 12 < delivery_month {
                    delivery_year = g.cur_year + 1;
                    delivery_month = delivery_month - 12;
                }
                let rec_text = commod_code.clone()
                    + "|"
                    + &str_trim(cast::<Ext, _>(etf_id))
                    + "|1|"
                    + &str_trim(cast::<Ext, _>(delivery_month))
                    + "|"
                    + &str_trim(cast::<Ext, _>(delivery_year));
                let unit_value = calc_futures_contract_value(g, ui, rng, &rec_text);
                if unit_value == Ext::from_f64(0.0) || !(g.liquid_funds >= unit_value) {
                    break 'next;
                }
                let mut target_notional = to_quad(cast::<Ext, _>(target_exposure) * sf(0.13f32));
                if g.liquid_funds < cast::<Ext, _>(target_notional) * sf(0.06f32) {
                    target_notional = to_quad(sf(0.5f32) * cast::<Ext, _>(target_notional));
                }
                if g.liquid_funds < cast::<Ext, _>(target_notional) * sf(0.06f32) {
                    target_notional = to_quad(sf(0.5f32) * cast::<Ext, _>(target_notional));
                }
                if g.liquid_funds < cast::<Ext, _>(target_notional) * sf(0.06f32) {
                    target_notional = to_quad(g.liquid_funds * Ext::from_f64(10.0));
                }
                let mut contracts = to_long(cast::<Ext, _>(direction) * (cast::<Ext, _>(target_notional) / unit_value));
                if 1000 < contracts {
                    contracts = 1000;
                } else if contracts < -1000 {
                    contracts = -1000;
                }
                let fut_price = get_futures_price(g, ui, rng, &commod_code, delivery_month, delivery_year, 12);
                let position_value = (cast::<Ext, _>(contracts)).abs() * unit_value;
                g.cash[etf_id] = g.cash[etf_id] - position_value * sf(0.06f32);
                g.commod_margin_bal[etf_id] = position_value * sf(0.05f32) + g.commod_margin_bal[etf_id];
                balance_cash_t_bills(g, etf_id);
                if g.cash[etf_id] < Ext::from_f64(0.0) && g.unused_credit_line > g.cash[etf_id].abs() {
                    let shortfall = g.cash[etf_id].abs();
                    g.bank_debt[etf_id] = shortfall + g.bank_debt[etf_id];
                    let lb = cast::<i32, _>(g.lending_bank[etf_id]);
                    g.cash[lb] = g.cash[lb] - shortfall;
                    g.cash[etf_id] = Ext::from_f64(0.0);
                } else if g.cash[etf_id] < Ext::from_f64(0.0) {
                    g.cash[etf_id] = position_value * sf(0.06f32) + g.cash[etf_id];
                    g.commod_margin_bal[etf_id] = g.commod_margin_bal[etf_id] - position_value * sf(0.05f32);
                    break 'next;
                }
                let value_str = fmt_trim(position_value, b"##############.00");
                let fut_rec = commod_code.clone()
                    + "|"
                    + &str_trim(cast::<Ext, _>(etf_id))
                    + "|"
                    + &str_trim(cast::<Ext, _>(contracts))
                    + "|"
                    + &str_trim(cast::<Ext, _>(delivery_month))
                    + "|"
                    + &str_trim(cast::<Ext, _>(delivery_year))
                    + "|"
                    + &value_str
                    + "|"
                    + &fut_price;
                add_or_remove_futures_rec(g, &fut_rec, 0);
                if g.exchange_rate * Ext::from_f64(500.0) <= g.exchange_rate * position_value {
                    apply_futures_trade_impact(g, ui, rng, &commod_code, direction, cast::<i64, _>(contracts));
                }
                break 'next;
            }
        }
        etf_id += 1;
    }
}

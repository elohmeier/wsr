//! Stock valuation and price history: the market-cap model `CalcStockPrice` (0x42c72a) with its
//! industry outlook, the monthly price-history ring buffers (`CapHistClose` / `PriceHistHigh` /
//! `PriceHistLow`, slot 0 = current month 1..60) and the indicators computed from them (trend
//! direction/streak, % change, technical signal, volatility, analyst rating), plus a few small industry
//! statistics (market share, average net margin, management margin, working-capital ratio), the stock
//! split check, the index divisor adjustment and `CountActiveCorps`.
//!
//! Numeric conventions (see the crate README and `logic::finance`): `EXT` uses software x87 arithmetic; a `SINGLE` store
//! (`fstp dword`) is rounded with `as f32`; SINGLE constants of the original (`fmul dword [0x75e…]`) are
//! written `s(0.05)` (= `0.05f32 as f64`). The association order of every float expression follows the
//! machine code, not the decompiled view. Comparisons are written so that the branch taken for unordered
//! (NaN) operands matches the x87 code: `jbe skip` → `if a > b`, `jae skip` → `if !(a >= b)`, `jb skip` →
//! `if a >= b`, `ja skip` → `if !(a > b)`.

// Statements keep the original BASIC shape on purpose (`x = x * 0.85`, `!(a >= b)` for the x87 branch
// taken on unordered operands).
#![allow(
    clippy::manual_range_contains,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::if_same_then_else,
    clippy::neg_multiply,
    clippy::too_many_lines,
    clippy::int_plus_one,
    clippy::manual_clamp
)]
use crate::pb::real::{Ext, cast};


use crate::logic::finance::{calc_entity_financials, get_managing_player};
use crate::pb::numeric::val;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, len, ltrim, parse_any, parse_count_any, rtrim};
use crate::pb::numeric::format;
use crate::pb::numeric::{ceil, int, sqr, to_integer, to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

/// A SINGLE constant of the original (`fld/fmul dword [const]`) as the x87 sees it.
#[inline]
fn s(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

/// `x = x + d` on a SINGLE local (`fadd/fsubr dword [x]; fstp dword [x]`).
#[inline]
fn add_s(x: f32, d: Ext) -> f32 {
    cast::<f32, _>(d + cast::<Ext, _>(x))
}

// ---------------------------------------------------------------------------------------------------------
// Small statistics
// ---------------------------------------------------------------------------------------------------------

/// Port of CountActiveCorps (0x4baa56).
/// `ActiveCorpCount` = number of entities 11..1600 with `EntityStatus = 1`.
pub fn count_active_corps(g: &mut Globals) {
    g.active_corp_count = 0;
    for corp_id in 11..=1600 {
        if g.entity_status[corp_id] == 1 {
            g.active_corp_count += 1;
        }
    }
}

/// Port of AdjustIndexDivisor (0x480e07).
/// Keeps the stock index continuous when `corp_id`'s market cap changes by `pct_change` % (or, with
/// `pct_change = 0`, when the cap `TmpExt3 / 100` leaves the index; `TmpExt3` is then cleared): the
/// divisor grows by the change relative to the total cap of the other active non-fund corps.
pub fn adjust_index_divisor(g: &mut Globals, corp_id: i32, pct_change: i32) {
    if !(g.stock_index_divisor > Ext::from_f64(0.0)) {
        return;
    }
    if corp_id > 1580 && g.industry_num[corp_id] == 71 {
        return;
    }
    let mut total_cap = Ext::from_f64(0.0);
    for ent_id in 11..=1600 {
        if g.entity_status[ent_id] == 1 && g.industry_num[ent_id] != 71 {
            if corp_id != ent_id || pct_change == 0 {
                total_cap = g.market_cap[ent_id] + total_cap;
            }
        }
    }
    let cap_change;
    if pct_change == 0 {
        cap_change = g.tmp_ext3 * s(0.01f32);
        total_cap = total_cap - cap_change;
        g.tmp_ext3 = Ext::from_f64(0.0);
    } else {
        cap_change = (cast::<Ext, _>(pct_change) * s(0.01f32)) * g.market_cap[corp_id];
    }
    let mut change_ratio = Ext::from_f64(0.0);
    if total_cap > Ext::from_f64(0.0) {
        change_ratio = cap_change / total_cap;
    }
    change_ratio = g.stock_index_divisor * change_ratio;
    g.stock_index_divisor = change_ratio + g.stock_index_divisor;
}

/// Port of CalcIndustryOutlook (0x458261).
/// Supply/demand outlook of an operating industry (3..69): `IndTotalAssets`, demand drift of
/// `IndDemand`, the current demand gap in % (`TmpExt1`, also stored in `IndDemandGap`), the projected
/// gap after a year of growth (`TmpExt2`), the projected capacity (`TmpExt3`); may set
/// `CrowdedIndustry` (> 22 corps), `ShortageIndustry`, and lower `IndGrowthForecast`. Returns the corp
/// with the largest business assets (0 for other industries, which return at once).
///
/// Quirk: the `ActiveCorpCount > 1250` threshold (25) is unreachable behind the `> 1150` test.
pub fn calc_industry_outlook(g: &mut Globals, rng: &mut Rng, industry: i16) -> i32 {
    // ON ERROR: ShowRuntimeError — error path ignored.
    if industry < 3 || industry > 69 {
        return 0;
    }
    let ind = cast::<i32, _>(industry);
    let mut corp_count = 0;
    let mut max_assets = Ext::from_f64(0.0);
    let mut leader_id = 0;
    let mut proj_capacity = Ext::from_f64(0.0);
    g.ind_total_assets = Ext::from_f64(0.0);
    for corp_id in 131..=1600 {
        if g.industry_num[corp_id] != industry || g.entity_status[corp_id] != 1 {
            continue;
        }
        corp_count += 1;
        if corp_count > 22 {
            g.crowded_industry = ind;
        }
        let ba = g.business_assets[corp_id];
        g.ind_total_assets = ba + g.ind_total_assets;
        if ba > max_assets {
            max_assets = ba;
            leader_id = corp_id;
        }
        let grown = (s(0.005f32) * ba) * cast::<Ext, _>(g.growth_rate[corp_id]) + ba;
        proj_capacity = grown + proj_capacity;
    }
    let demand = cast::<Ext, _>(g.ind_demand[ind]);
    if g.ind_total_assets > demand * s(1.3f32) {
        g.ind_demand[ind] = to_quad(s(1.01f32) * cast::<Ext, _>(g.ind_demand[ind]));
    } else if cast::<Ext, _>(g.ind_demand[ind]) > g.ind_total_assets * s(1.6f32) {
        g.ind_demand[ind] = to_quad(s(0.99f32) * cast::<Ext, _>(g.ind_demand[ind]));
    }
    let ratio = g.ind_total_assets / cast::<Ext, _>(g.ind_demand[ind]);
    let mut demand_gap: Ext = Ext::from_f64(100.0) * (Ext::from_f64(1.0) - ratio);
    if !(Ext::from_f64(45.0) >= demand_gap) {
        demand_gap = Ext::from_f64(45.0);
    }
    if -Ext::from_f64(20.0) > demand_gap {
        demand_gap = -Ext::from_f64(20.0);
    }
    if corp_count < 6 {
        g.shortage_industry = industry;
    } else {
        let mut gap_threshold = 18;
        if g.active_corp_count < 800 {
            gap_threshold = 15;
        } else if g.active_corp_count < 900 {
            gap_threshold = 16;
        } else if g.active_corp_count > 1150 {
            gap_threshold = 20;
        } else if g.active_corp_count > 1250 {
            gap_threshold = 25; // unreachable
        }
        if !(cast::<Ext, _>(gap_threshold) >= demand_gap) && g.crowded_industry != ind {
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                let si = g.shortage_industry;
                if si > 2 && si < 70 && !(g.ind_demand_gap[cast::<i32, _>(si)] >= demand_gap) {
                    g.shortage_industry = industry;
                }
                if g.shortage_industry == 0 {
                    g.shortage_industry = industry;
                }
            }
        }
    }
    g.tmp_ext1 = demand_gap;
    g.ind_demand_gap[ind] = cast::<f64, _>(demand_gap);
    if !(Ext::from_f64(40.0) >= demand_gap) {
        g.ind_demand[ind] = to_quad(s(0.98f32) * cast::<Ext, _>(g.ind_demand[ind]));
    }
    if !(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(25.0) >= demand_gap) {
        if !(Ext::from_f64(10.0) * rng.rnd() >= cast::<Ext, _>(g.ind_growth_forecast[ind])) {
            g.ind_growth_forecast[ind] -= 1;
        }
    }
    let demand = cast::<Ext, _>(g.ind_demand[ind]);
    let proj_demand = (cast::<Ext, _>(g.ind_growth_rate[ind]) * s(0.005f32)) * demand + demand;
    let ratio = proj_capacity / proj_demand;
    let mut proj_gap: Ext = Ext::from_f64(100.0) * (Ext::from_f64(1.0) - ratio);
    if !(Ext::from_f64(45.0) >= proj_gap) {
        proj_gap = Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(43.0);
    } else if -Ext::from_f64(30.0) > proj_gap {
        proj_gap = -Ext::from_f64(30.0) + Ext::from_f64(6.0) * rng.rnd();
    }
    g.tmp_ext2 = proj_gap;
    g.tmp_ext3 = proj_capacity;
    leader_id
}

/// Port of CalcIndustryMarketShare (0x53ecb3).
/// Percentage (rounded) of the industry's business assets owned by the controller id passed in
/// `TmpExt1` (which is cleared).
pub fn calc_industry_market_share(g: &mut Globals, industry: i16) -> i32 {
    // ON ERROR: ShowRuntimeError — error path ignored.
    let owner_id = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    let mut industry_assets: i64 = 0;
    let mut owner_assets: i64 = 0;
    for corp_id in 131..=1600 {
        if g.entity_status[corp_id] != 1 || g.industry_num[corp_id] != industry {
            continue;
        }
        industry_assets = to_quad(g.business_assets[corp_id] + cast::<Ext, _>(industry_assets));
        if g.corp_controller[corp_id] == owner_id {
            owner_assets = to_quad(g.business_assets[corp_id] + cast::<Ext, _>(owner_assets));
        }
    }
    let mut share_ratio: Ext = Ext::from_f64(0.0);
    if cast::<Ext, _>(industry_assets) > Ext::from_f64(0.0) && cast::<Ext, _>(owner_assets) > Ext::from_f64(0.0) {
        share_ratio = cast::<Ext, _>(owner_assets) / cast::<Ext, _>(industry_assets);
    }
    to_long(share_ratio * Ext::from_f64(100.0))
}

/// Port of CalcIndustryAvgNetMargin (0x4a174b).
/// Average of `ProfitMargin - DiscSpendPct` over the industry's active corps 131..1600, plus the
/// industry's projected demand gap (rounded `TmpExt2` of [`calc_industry_outlook`]); `TmpExt1..3` are
/// cleared. (`industry` is passed BYVAL.)
pub fn calc_industry_avg_net_margin(g: &mut Globals, rng: &mut Rng, industry: i16) -> f32 {
    let industry_arg = industry;
    calc_industry_outlook(g, rng, industry_arg);
    let gap = to_long(g.tmp_ext2);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    let mut margin_sum: f32 = cast::<f32, _>(Ext::from_f32(0.0f32));
    let mut corp_count = 0;
    for corp_id in 131..=1600 {
        if g.industry_num[corp_id] == industry_arg && g.entity_status[corp_id] == 1 {
            margin_sum = cast::<f32, _>((cast::<Ext, _>(g.profit_margin[corp_id]) + cast::<Ext, _>(margin_sum))
                - cast::<Ext, _>(g.disc_spend_pct[corp_id]));
            corp_count += 1;
        }
    }
    if corp_count > 0 {
        margin_sum = cast::<f32, _>(cast::<Ext, _>(margin_sum) / cast::<Ext, _>(corp_count));
    } else {
        margin_sum = 0.0f32;
    }
    cast::<f32, _>(cast::<Ext, _>(margin_sum) + cast::<Ext, _>(gap))
}

/// Port of GetMgmtMarginVsIndustry (0x46e595).
/// `mode = 1`: the midpoint of the corp's industry margin range from the last call (`IndMgmtMidpoint`).
/// Otherwise (operating industries < 70): recomputes the midpoint `min + (max - min) / 2` of the
/// industry's `ProfitMargin` range (0 if the range is 5 or less) and returns the corp's margin minus it
/// (0 if the range is 5 or less).
pub fn get_mgmt_margin_vs_industry(g: &mut Globals, corp_id: i32, mode: i32) -> i32 {
    // (DIM IndMgmtMidpoint(0 TO 71): no-op, the global array is already dimensioned.)
    if corp_id < 11 || corp_id > 1600 {
        return 0;
    }
    let ind = g.industry_num[corp_id];
    if mode == 1 {
        return g.ind_mgmt_midpoint[cast::<i32, _>(ind)];
    }
    let mut margin_diff = 0;
    if ind < 70 {
        let mut min_margin: i32 = 99;
        let mut max_margin: i32 = -99;
        for entity_id in 11..=1600 {
            if g.industry_num[entity_id] == ind && g.entity_status[entity_id] == 1 {
                let pm = cast::<i32, _>(g.profit_margin[entity_id]);
                if pm > max_margin {
                    max_margin = pm;
                }
                if pm < min_margin {
                    min_margin = pm;
                }
            }
        }
        g.ind_mgmt_midpoint[cast::<i32, _>(ind)] = 0;
        if max_margin - min_margin > 5 {
            let mid = to_long((cast::<Ext, _>(max_margin) - cast::<Ext, _>(min_margin)) / Ext::from_f64(2.0)) + min_margin;
            g.ind_mgmt_midpoint[cast::<i32, _>(ind)] = mid;
            margin_diff = cast::<i32, _>(g.profit_margin[corp_id]) - mid;
        }
    }
    margin_diff
}

/// Port of CalcWorkingCapitalRatio (0x49b2e3).
/// Working capital a corp needs as a fraction of its business: `(12 - (margin - industry midpoint) / 3)
/// %` clamped to 5..20 %; 0.12 for non-corps (< 131) and inactive corps.
pub fn calc_working_capital_ratio(g: &mut Globals, corp_id: i32) -> Ext {
    if corp_id < 131 || g.entity_status[corp_id] != 1 {
        return s(0.12f32);
    }
    let x = cast::<Ext, _>(get_mgmt_margin_vs_industry(g, corp_id, 1));
    let ratio: Ext = Ext::from_f64(12.0) - (cast::<Ext, _>(g.profit_margin[corp_id]) - x) / Ext::from_f64(3.0);
    let clamped = if !(ratio >= Ext::from_f64(5.0)) {
        Ext::from_f64(5.0)
    } else if ratio > Ext::from_f64(20.0) {
        Ext::from_f64(20.0)
    } else {
        ratio
    };
    s(0.01f32) * clamped
}

/// Port of CalcDividendYield (0x49b447).
/// Dividend yield in % of an active corp with a positive market cap (else 0): funds (industry 71) from
/// the quarterly distribution in field 12 of `EntMiscData` (x4), others from `DividendPerShare`; a yield
/// outside 0..100 gives 0 (for non-funds `DividendPerShare` is then reset to 0).
pub fn calc_dividend_yield(g: &mut Globals, corp_id: i32) -> Ext {
    if corp_id < 11 || g.entity_status[corp_id] != 1 || !(g.market_cap[corp_id] > Ext::from_f64(0.0)) {
        return Ext::from_f64(0.0);
    }
    if g.industry_num[corp_id] == 71 {
        let dist = val(parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 12).as_bytes());
        let mut yield_pct: Ext = (Ext::from_f64(4.0) * dist) / g.market_cap[corp_id];
        if !(Ext::from_f64(100.0) > yield_pct) || Ext::from_f64(0.0) > yield_pct {
            yield_pct = Ext::from_f64(0.0);
        }
        yield_pct
    } else {
        let mut stock_yield = ((Ext::from_f64(100.0) * g.dividend_per_share[corp_id]) * g.shares_outstanding[corp_id])
            / g.market_cap[corp_id];
        if !(stock_yield < Ext::from_f64(100.0)) || !(stock_yield >= Ext::from_f64(0.0)) {
            g.dividend_per_share[corp_id] = Ext::from_f64(0.0);
            stock_yield = Ext::from_f64(0.0);
        }
        stock_yield
    }
}

// ---------------------------------------------------------------------------------------------------------
// Price history
// ---------------------------------------------------------------------------------------------------------

/// Port of ClearPriceHistory (0x4a389b).
/// Zeroes slots 0..60 of the three price-history arrays of series `corp_id`.
pub fn clear_price_history(g: &mut Globals, corp_id: i32) {
    for period in 0..=60 {
        g.cap_hist_close[(period, corp_id)] = Ext::from_f64(0.0);
        g.price_hist_high[(period, corp_id)] = Ext::from_f64(0.0);
        g.price_hist_low[(period, corp_id)] = Ext::from_f64(0.0);
    }
}

/// Port of ScalePriceHistory (0x619d7d).
/// Multiplies months 1..60 of series `series_id` (close, high, low) by `(100 + pct_change) / 100`.
pub fn scale_price_history(g: &mut Globals, series_id: i32, pct_change: i32) {
    let factor: Ext = s(0.01f32) * (Ext::from_f64(100.0) + cast::<Ext, _>(pct_change));
    for month in 1..=60 {
        g.cap_hist_close[(month, series_id)] = factor * g.cap_hist_close[(month, series_id)];
        g.price_hist_high[(month, series_id)] = factor * g.price_hist_high[(month, series_id)];
        g.price_hist_low[(month, series_id)] = factor * g.price_hist_low[(month, series_id)];
    }
}

/// Port of CheckStockSplit (0x494a8e).
/// 4-for-1 splits (repeated) while an active corp's share price is 125 or more, adjusting
/// `DividendPerShare`; not for BKH/GOOG/AAPL/AMZN below 5000/2000/1750/1200, nor for player-managed corps
/// priced between 3 and 1000.
pub fn check_stock_split(g: &mut Globals, corp_id: i32) {
    if corp_id < 11 || corp_id > 1600 || g.entity_status[corp_id] != 1 {
        return;
    }
    if !(g.shares_outstanding[corp_id] > Ext::from_f64(0.0)) {
        g.shares_outstanding[corp_id] = Ext::from_f64(1.0);
    }
    if !(g.market_cap[corp_id] > Ext::from_f64(0.0)) {
        return;
    }
    let price = g.market_cap[corp_id] / g.shares_outstanding[corp_id];
    let sym = &g.ent_symbol[corp_id];
    for (name, limit) in [("BKH", Ext::from_f64(5000.0)), ("GOOG", Ext::from_f64(2000.0)), ("AAPL", Ext::from_f64(1750.0)), ("AMZN", Ext::from_f64(1200.0))] {
        if *sym == name && !(price >= limit) {
            return;
        }
    }
    if get_managing_player(g, corp_id) > 0 && price > Ext::from_f64(3.0) && !(price >= Ext::from_f64(1000.0)) {
        return;
    }
    let old_shares = g.shares_outstanding[corp_id];
    loop {
        let price = g.market_cap[corp_id] / g.shares_outstanding[corp_id];
        if price >= Ext::from_f64(125.0) {
            g.shares_outstanding[corp_id] = Ext::from_f64(4.0) * g.shares_outstanding[corp_id];
        }
        if !(price >= Ext::from_f64(125.0)) {
            break;
        }
    }
    let shares = g.shares_outstanding[corp_id];
    if shares != old_shares && shares > Ext::from_f64(0.0) {
        let ratio = old_shares / shares;
        g.dividend_per_share[corp_id] = ratio * g.dividend_per_share[corp_id];
    }
}

/// Port of UpdateMainCommodityDisplay (0x6351a5), state effects only.
/// Floors `OilPrice` at 0.15 and snaps `PrimeRate` to the nearest quarter of `BaseInterestRate` when it
/// is more than 0.365 away; the commodity/rate labels of the main window (CONTROL SET TEXT) are UI.
pub fn update_main_commodity_display(g: &mut Globals) {
    if s(0.15f32) > cast::<Ext, _>(g.oil_price) {
        g.oil_price = cast::<f32, _>(Ext::from_f32(0.15f32));
    }
    // (UI) labels 154/159/161/163/165: [runtime resource wording] corn prices
    if (cast::<Ext, _>(g.prime_rate) - cast::<Ext, _>(g.base_interest_rate)).abs() > s(0.365f32) {
        let quarters = to_integer(cast::<Ext, _>(g.base_interest_rate) * Ext::from_f64(4.0));
        g.prime_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(quarters) / Ext::from_f64(4.0)));
    }
    // (UI) labels 152/157/158 (rates), DlgCrypto 1172/1175 (bitcoin, ethereum)
}

/// The current value of a non-stock series (used by both history updates): 0 = stock index, 6..10 oil
/// (x100), gold, silver, wheat, corn, 1601..1606 prime rate, long/short bond yield, GDP growth,
/// bitcoin, ethereum. Other ids: `None` (the caller keeps its value).
fn series_value(g: &Globals, series: i32) -> Option<Ext> {
    Some(match series {
        6 => Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price),
        7 => cast::<Ext, _>(g.gold_price),
        8 => cast::<Ext, _>(g.silver_price),
        9 => cast::<Ext, _>(g.wheat_price),
        10 => cast::<Ext, _>(g.corn_price),
        1601 => cast::<Ext, _>(g.prime_rate),
        1602 => cast::<Ext, _>(g.long_bond_yield),
        1603 => cast::<Ext, _>(g.short_bond_yield),
        1605 => cast::<Ext, _>(g.bitcoin_price),
        1606 => cast::<Ext, _>(g.ethereum_price),
        _ => return None,
    })
}

/// Port of UpdatePriceHistoryTick (0x618e2a).
/// Records the current value of series `series_id` in the current month's slot (close, and high/low
/// extremes): corps 11..1600 (market cap; also repairs non-positive shares (2) / caps up to 0.2 (20) and
/// checks for a split above 10000), players 1..5 (net worth, floored at 0.01), commodities, rates, GDP
/// growth (clamped to -23..9.8 and rounded to 0.1 first), crypto, and the stock index (ids <= 0).
/// A non-positive value (except GDP) nudges the current close by +-0.2 % at random instead.
pub fn update_price_history_tick(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, series_id: i32) {
    let series = series_id;
    let mut value: Ext = Ext::from_f64(0.0);
    if series > 10 && series < 1601 {
        if g.entity_status[series] != 1 {
            return;
        }
        if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 8 {
            return;
        }
        if !(g.shares_outstanding[series] > Ext::from_f64(0.0)) {
            g.shares_outstanding[series] = Ext::from_f64(2.0);
        }
        if !(g.market_cap[series] > s(0.2f32)) {
            g.market_cap[series] = Ext::from_f64(20.0);
        }
        value = g.market_cap[series];
        if value > Ext::from_f64(10000.0) {
            check_stock_split(g, series);
        }
    } else if series > 0 && series < 6 {
        if g.player_name[series] == "...OUT..." || series > g.num_players {
            return;
        }
        calc_entity_financials(g, ui, rng, series);
        value = g.net_worth;
        if !(value >= Ext::from_f64(0.0)) {
            value = s(0.01f32);
        }
    } else if series > 5 {
        if let Some(v) = series_value(g, series) {
            value = v;
        }
        if series == 1604 {
            if !(s(9.8f32) >= cast::<Ext, _>(g.gdp_growth)) {
                g.gdp_growth = cast::<f32, _>(Ext::from_f32(9.8f32));
            }
            if -Ext::from_f64(23.0) > cast::<Ext, _>(g.gdp_growth) {
                g.gdp_growth = cast::<f32, _>(-Ext::from_f32(23.0f32));
            }
            let tenths = to_long(Ext::from_f64(10.0) * cast::<Ext, _>(g.gdp_growth));
            g.gdp_growth = cast::<f32, _>(cast::<f32, _>(s(0.1f32) * cast::<Ext, _>(tenths)));
            value = cast::<Ext, _>(g.gdp_growth);
        }
    } else {
        value = g.stock_index;
    }

    if value > Ext::from_f64(0.0) || series == 1604 {
        if g.cap_hist_close[(0, series)] == Ext::from_f64(0.0) {
            g.cap_hist_close[(0, series)] = Ext::from_f64(1.0);
            g.cap_hist_close[(1, series)] = value;
            g.price_hist_high[(1, series)] = value;
            g.price_hist_low[(1, series)] = value;
        }
        let slot = to_long(g.cap_hist_close[(0, series)]);
        record_tick(g, slot, series, value);
    } else {
        let slot = to_long(g.cap_hist_close[(0, series)]);
        let mut v = g.cap_hist_close[(slot, series)];
        if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
            v = v * s(1.002f32);
        } else {
            v = v * s(0.998f32);
        }
        record_tick(g, slot, series, v);
    }
}

/// Close = `value`, high/low widened to include it.
fn record_tick(g: &mut Globals, slot: i32, series: i32, value: Ext) {
    g.cap_hist_close[(slot, series)] = value;
    if !(g.price_hist_high[(slot, series)] >= value) {
        g.price_hist_high[(slot, series)] = value;
    }
    if g.price_hist_low[(slot, series)] > value {
        g.price_hist_low[(slot, series)] = value;
    }
}

/// Port of RollPriceHistoryMonth (0x618537).
/// Month end for every series 0..1606: records the final value of the month (like
/// [`update_price_history_tick`], with random intra-month highs up to +12 % / lows down to -10 % for
/// commodities, stocks and crypto when the recorded range is narrow), then advances the series' slot
/// (1..60, wrapping) and opens the new month at the same value. Returns at once (for all series) if the
/// stock index is not positive. Corps with a non-positive cap are re-valued (`ValuationMode = 3`) and
/// share prices below 2.5 or above 10000 trigger [`check_stock_split`]; GDP growth is clamped to
/// -19.9..6.1 (different limits than the tick!). Inactive corps and absent players are skipped.
pub fn roll_price_history_month(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let mut value: Ext = Ext::from_f64(0.0);
    for series_id in 0..=1606 {
        if series_id == 0 {
            value = g.stock_index;
            if !(value > Ext::from_f64(0.0)) {
                return;
            }
        } else if series_id < 6 {
            if g.player_name[series_id] == "...OUT..." || series_id > g.num_players {
                continue;
            }
            calc_entity_financials(g, ui, rng, series_id);
            value = g.net_worth;
            if !(value >= Ext::from_f64(0.0)) {
                value = s(0.01f32);
            }
        } else if series_id < 11 || series_id > 1600 {
            if let Some(v) = series_value(g, series_id) {
                value = v;
            }
            if series_id == 1604 {
                if !(s(6.3f32) >= cast::<Ext, _>(g.gdp_growth)) {
                    g.gdp_growth = cast::<f32, _>(Ext::from_f32(6.1f32));
                }
                if -Ext::from_f64(20.0) > cast::<Ext, _>(g.gdp_growth) {
                    g.gdp_growth = cast::<f32, _>(-Ext::from_f32(19.9f32));
                }
                value = cast::<Ext, _>(g.gdp_growth);
            }
        } else {
            if g.entity_status[series_id] != 1 {
                continue;
            }
            value = g.market_cap[series_id];
            if !(g.shares_outstanding[series_id] > Ext::from_f64(0.0)) {
                g.shares_outstanding[series_id] = Ext::from_f64(1.0);
            }
            let price = g.market_cap[series_id] / g.shares_outstanding[series_id];
            if !(g.market_cap[series_id] > Ext::from_f64(0.0)) {
                g.market_cap[series_id] = Ext::from_f64(10.0);
                g.valuation_mode = 3;
                calc_stock_price(g, ui, rng, series_id, 0);
            }
            if !(price >= Ext::from_f64(2.5)) || price > Ext::from_f64(10000.0) {
                check_stock_split(g, series_id);
            }
        }

        let sid = series_id;
        if g.cap_hist_close[(0, sid)] == Ext::from_f64(0.0) {
            g.cap_hist_close[(0, sid)] = Ext::from_f64(1.0);
            g.cap_hist_close[(1, sid)] = value;
            g.price_hist_high[(1, sid)] = value;
            g.price_hist_low[(1, sid)] = value;
        }
        let mut slot = to_long(g.cap_hist_close[(0, sid)]);
        g.cap_hist_close[(slot, sid)] = value;
        if !(g.price_hist_high[(slot, sid)] >= value) {
            g.price_hist_high[(slot, sid)] = value;
        }
        let fake_range = |sid: i32| (sid > 5 && sid < 1601, sid > 1604);
        let (stock_like, crypto) = fake_range(sid);
        for apply in [stock_like, crypto] {
            if apply && !(g.price_hist_high[(slot, sid)] >= value * s(1.02f32)) && !(Ext::from_f64(4.0) * rng.rnd() >= Ext::from_f64(3.0)) {
                let r = rng.rnd();
                g.price_hist_high[(slot, sid)] = value + value * (s(0.12f32) * r);
            }
        }
        if g.price_hist_low[(slot, sid)] > value {
            g.price_hist_low[(slot, sid)] = value;
        }
        for apply in [stock_like, crypto] {
            if apply && g.price_hist_low[(slot, sid)] > value * s(0.99f32) && !(Ext::from_f64(3.0) * rng.rnd() >= Ext::from_f64(2.0)) {
                let r = rng.rnd();
                g.price_hist_low[(slot, sid)] = value - value * (s(0.1f32) * r);
            }
        }
        slot += 1;
        if slot > 60 {
            slot = 1;
        }
        g.cap_hist_close[(0, sid)] = cast::<Ext, _>(slot);
        g.cap_hist_close[(slot, sid)] = value;
        g.price_hist_high[(slot, sid)] = value;
        g.price_hist_low[(slot, sid)] = value;
    }
}

/// Port of PriceTrendDirection (0x61a9f1).
/// +1 / -1 if series `series_id` moved steadily up / down over the last `lookback` months (< 60),
/// comparing every `step` months back from the current close; 0 if mixed, flat, or without history.
/// With `lookback = step` only the endpoints are compared.
///
/// Quirks: a repeated close ends the scan with 0 (unless it is the slot just compared); `lookback` must
/// be a multiple of `step` or the scan runs on until the direction changes.
pub fn price_trend_direction(g: &Globals, series_id: i32, lookback: i32, step: i32) -> i32 {
    let i = series_id;
    let mut result = 0;
    if lookback > 59 {
        return result;
    }
    let j = to_long(g.cap_hist_close[(0, i)]);
    let mut slot = j - lookback;
    if slot < 1 {
        slot += 60;
    }
    if g.cap_hist_close[(slot, i)] == Ext::from_f64(0.0) {
        return result;
    }
    let mut direction = 0;
    if g.cap_hist_close[(j, i)] > g.cap_hist_close[(slot, i)] {
        direction = 1;
    }
    if !(g.cap_hist_close[(j, i)] >= g.cap_hist_close[(slot, i)]) {
        direction = -1;
    }
    if lookback == step {
        return direction;
    }
    if direction == 0 {
        return result;
    }
    let mut prev_close = g.cap_hist_close[(j, i)];
    let first = direction;
    let mut offset = 0;
    let mut prev_slot = 0;
    loop {
        offset += step;
        slot = j - offset;
        if slot < 1 {
            slot += 60;
        }
        let c = g.cap_hist_close[(slot, i)];
        if c == prev_close && slot != prev_slot {
            return result;
        }
        prev_slot = slot;
        if prev_close > c {
            direction = 1;
        }
        if !(prev_close >= c) {
            direction = -1;
        }
        prev_close = c;
        if !(direction == first && lookback != offset) {
            break;
        }
    }
    if direction == first {
        result = first;
    }
    result
}

/// Port of PriceTrendStreak (0x61ad2a).
/// Length in months (3..) of the current monthly up (+) / down (-) streak of series `series_id`; 0 if
/// the last 3 months are not a steady trend.
pub fn price_trend_streak(g: &Globals, series_id: i32) -> i32 {
    let first = price_trend_direction(g, series_id, 3, 1);
    if first == 0 {
        return 0;
    }
    let mut streak = 3;
    let mut lookback = 4;
    loop {
        if price_trend_direction(g, series_id, lookback, 1) != first {
            break;
        }
        streak += 1;
        lookback += 1;
        if lookback > 60 {
            break;
        }
    }
    if first < 0 {
        streak = -streak;
    }
    streak
}

/// Port of PricePctChangeMonths (0x61ae3a).
/// % change (rounded up) of series `series_id` over the last `months` (1..60) months; 0 without positive
/// closes, for |change| > 99999, and for GDP growth (1604) while it is below 4.5.
pub fn price_pct_change_months(g: &Globals, series_id: i32, months: i32) -> i32 {
    let i = series_id;
    let mut result = 0;
    if i == 1604 && s(4.5f32) > cast::<Ext, _>(g.gdp_growth) {
        return result;
    }
    if months < 1 || months > 60 {
        return result;
    }
    let back = to_long((cast::<Ext, _>(months)).abs());
    let mut past_slot = to_long(g.cap_hist_close[(0, i)] - cast::<Ext, _>(back));
    if past_slot < 1 {
        past_slot += 60;
    }
    let past = g.cap_hist_close[(past_slot, i)];
    if past > Ext::from_f64(0.0) {
        let cur = g.cap_hist_close[(to_long(g.cap_hist_close[(0, i)]), i)];
        if !(cur > Ext::from_f64(0.0)) {
            return result;
        }
        let change = Ext::from_f64(100.0) * ((cur - past) / past);
        let mut pct = to_long(ceil(change));
        if (cast::<Ext, _>(pct)).abs() > Ext::from_f64(99999.0) {
            pct = 0;
        }
        result = pct;
    }
    result
}

/// Port of CalcTechnicalTrendSignal (0x61b01e) with its GOSUBs TrendSignal_MovingAvgCheck (0x61b221)
/// and TrendSignal_Return (0x61b396).
/// Technical "chart" signal of series `series_id` (+1 buy / -1 sell / 0 neutral), after recording the
/// current tick ([`update_price_history_tick`]) and refreshing the commodity display: a 1-month move of
/// 10 % or more, else close vs. its 2-6 month moving average (above it: +1, more than 4 % below: -1),
/// else a steady 12-month trend in 3-month steps, else a 12-month move of 7 % or more, else the sign
/// of the monthly trend streak. Rates and GDP (1601..1604) always give 0.
pub fn calc_technical_trend_signal(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, series_id: i32) -> i32 {
    let series = series_id;
    update_price_history_tick(g, ui, rng, series);
    update_main_commodity_display(g);
    if series > 1600 && series < 1605 {
        return 0; // TrendSignal_Return
    }
    let mut pct_signal = price_pct_change_months(g, series, 1);
    if !((cast::<Ext, _>(pct_signal)).abs() >= Ext::from_f64(10.0)) {
        pct_signal = 0;
    }
    let sign = |x: i32| x.signum();
    pct_signal = sign(pct_signal);
    if pct_signal != 0 {
        return pct_signal;
    }
    if let Some(signal) = trend_signal_moving_avg_check(g, series) {
        return signal;
    }
    let signal = price_trend_direction(g, series, 12, 3);
    if signal != 0 {
        return signal;
    }
    let mut pct_signal = price_pct_change_months(g, series, 12);
    if !((cast::<Ext, _>(pct_signal)).abs() >= Ext::from_f64(7.0)) {
        pct_signal = 0;
    }
    pct_signal = sign(pct_signal);
    if pct_signal != 0 {
        return pct_signal;
    }
    sign(price_trend_streak(g, series))
}

/// GOSUB TrendSignal_MovingAvgCheck (0x61b221) of [`calc_technical_trend_signal`]: `Some(signal)` where
/// the original jumps to TrendSignal_Return (ending the function), `None` for RETURN.
fn trend_signal_moving_avg_check(g: &Globals, series: i32) -> Option<i32> {
    let cur_slot = to_long(g.cap_hist_close[(0, series)]);
    let cur_close: Ext = g.cap_hist_close[(cur_slot, series)];
    let mut sum_close = Ext::from_f64(2.0) * cur_close;
    let mut moving_avg = Ext::from_f64(0.0);
    let mut k = 1;
    loop {
        let mut past_slot = cur_slot - k;
        if past_slot < 1 {
            past_slot += 60;
        }
        let past_close: Ext = g.cap_hist_close[(past_slot, series)];
        if g.cap_hist_close[(past_slot, series)] == Ext::from_f64(0.0) {
            if k < 4 {
                return None;
            }
            break;
        }
        sum_close = past_close + sum_close;
        moving_avg = sum_close / (Ext::from_f64(2.0) + cast::<Ext, _>(k));
        k += 1;
        if k > 5 {
            break;
        }
    }
    if !(moving_avg * s(1.0f32) >= cur_close) {
        return Some(1);
    }
    if moving_avg * s(0.96f32) > cur_close {
        return Some(-1);
    }
    None
}

/// Port of CalcStockVolatility (0x605fbc).
/// 2-year volatility of an active corp's stock: `(high - low) / low` in % over the current and the 23
/// previous months (after recording the current tick); -1 without 24 months of history. With
/// `save_hi_lo = 1` the 12-month high/low are left in `TmpExt1` / `TmpExt2`.
pub fn calc_stock_volatility(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32, save_hi_lo: i32) -> f32 {
    // ON ERROR: ShowRuntimeError — error path ignored.
    let corp = corp_id;
    if g.entity_status[corp] != 1 {
        return -1.0f32;
    }
    let mut hist_idx = to_long(g.cap_hist_close[(0, corp)]);
    if hist_idx <= 0 || !(g.cap_hist_close[(hist_idx, corp)] > Ext::from_f64(0.0)) || !(g.cap_hist_close[(24, corp)] > Ext::from_f64(0.0)) {
        return -1.0f32;
    }
    update_price_history_tick(g, ui, rng, corp);
    let mut lo_price = g.price_hist_low[(hist_idx, corp)];
    let mut hi_price = g.price_hist_high[(hist_idx, corp)];
    for i in 1..=23 {
        hist_idx -= 1;
        if hist_idx == 0 {
            hist_idx = 60;
        }
        if g.price_hist_high[(hist_idx, corp)] > hi_price {
            hi_price = g.price_hist_high[(hist_idx, corp)];
        }
        if !(g.price_hist_low[(hist_idx, corp)] >= lo_price) && g.price_hist_low[(hist_idx, corp)] > Ext::from_f64(0.0) {
            lo_price = g.price_hist_low[(hist_idx, corp)];
        }
        if save_hi_lo == 1 && i == 11 {
            g.tmp_ext1 = hi_price;
            g.tmp_ext2 = lo_price;
        }
    }
    if !(lo_price > Ext::from_f64(0.0)) {
        return -1.0f32;
    }
    let ratio: Ext = (hi_price - lo_price) / lo_price;
    cast::<f32, _>(Ext::from_f64(100.0) * ratio)
}

/// Port of CalcAnalystRating (0x494007).
/// Analyst rating 1 (strong buy) .. 5 (strong sell) of an active corp (0 for others). Funds: from the
/// 6-month % change and the trend streak, shifted by one depending on the quarter. Others: market cap
/// vs. a "fair value" from [`calc_stock_price`] in `ValuationMode = 4` (rounded, 0.6x for the
/// asbestos/superfund corps, then 1.5x or 0.65x depending on corp id and quarter), with earnings growth
/// and credit rating; corps rated below 2 are always 5.
pub fn calc_analyst_rating(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32) -> i32 {
    if corp_id < 11 || corp_id > 1600 || g.entity_status[corp_id] != 1 {
        return 0;
    }
    let mut rating;
    if g.industry_num[corp_id] == 71 {
        rating = 3;
        let pct = price_pct_change_months(g, corp_id, 6);
        let streak = price_trend_streak(g, corp_id);
        let small = !((cast::<Ext, _>(pct)).abs() >= Ext::from_f64(10.0));
        if small && streak > 0 {
            rating = 2;
        }
        if pct > 10 && streak == 0 {
            rating = 2;
        }
        if pct > 10 && streak > 0 {
            rating = 1;
        }
        if pct < -10 && streak == 0 {
            rating = 4;
        }
        if small && streak < 0 {
            rating = 4;
        }
        if pct < -10 && streak < 0 {
            rating = 5;
        }
        if g.cur_quarter < 3 && (g.cur_quarter + corp_id) % 2 == 0 {
            rating -= 1;
        }
        if g.cur_quarter > 2 && (g.cur_quarter + corp_id) % 2 == 0 {
            rating += 1;
        }
    } else {
        g.valuation_mode = 4;
        calc_stock_price(g, ui, rng, corp_id, 0);
        let mut fair_value = to_long(g.calc_stock_price);
        if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(corp_id) || cast::<i32, _>(g.superfund_corp) == corp_id {
            fair_value = to_long(s(0.6f32) * cast::<Ext, _>(fair_value));
        }
        if g.cur_quarter % 2 == corp_id % 2 {
            fair_value = to_long(s(1.5f32) * cast::<Ext, _>(fair_value));
        } else if g.cur_quarter % 3 == corp_id % 3 {
            fair_value = to_long(s(0.65f32) * cast::<Ext, _>(fair_value));
        }
        let fv = cast::<Ext, _>(fair_value);
        let mc = g.market_cap[corp_id];
        rating = 3;
        if !(mc >= fv * s(0.95f32)) {
            rating = 2;
        }
        if !(mc >= fv * s(0.75f32))
            && g.earnings_y1[corp_id] > g.earnings_yr2_ago[corp_id]
            && g.earnings_yr2_ago[corp_id] > g.earnings_yr3_ago[corp_id]
            && g.credit_rating_num[corp_id] > 7
        {
            rating = 1;
        }
        if mc > fv * s(1.2f32) {
            rating = 4;
        }
        if mc > fv * s(1.35f32) || g.credit_rating_num[corp_id] < 2 {
            rating = 5;
        }
    }
    rating.clamp(1, 5)
}

// ---------------------------------------------------------------------------------------------------------
// CalcStockPrice
// ---------------------------------------------------------------------------------------------------------

/// Port of CalcStockPrice (0x42c72a, `CalcStockPrice_42c72a` in tools/names.tsv, the global holding its result is `CalcStockPrice`).
/// Market valuation of corp `corp_ref` (11..1600, active; else nothing happens). The behaviour depends
/// on `ValuationMode` (always reset to 1 at the end, except for the early exits):
///
/// * 0: no valuation — only `TmpSharePrice` / `StockPriceStr` (and the history tick) from the current cap.
/// * 1 (normal) / 2 (damped): a target cap from earnings (P/E multiple from earnings trend, industry,
///   credit rating, growth, interest rates, GDP target), book value, asset floors and ceilings, investor
///   sentiment, `MarketValuation`, bubbles and fads, the hot stock, earnings surprises, P/E and P/B caps;
///   the cap then moves part of the way toward it (randomised, max 20 % of the old cap); sets
///   `TickerChangeStr`. Weak or tiny corps may be switched to 2/3 on the way.
/// * 3 (set) / 4 (fair value): the cap is set to the target (4 restores the old cap afterwards); the
///   target is also left in the global `CalcStockPrice`. Funds (industry 71) in mode 1..3 are valued at
///   ~net asset value.
///
/// Also: `TmpSharePrice`, `StockPriceStr` (floor 0.25 per share), may set `TakeoverCandidateId`,
/// adjusts `RandSeed` (by the seconds of `TIME$`, passed as `time_secs` = `VAL(RIGHT$(TIME$, 2))`),
/// clamps `InvestorSentiment` and `MarketValuation`, may clear the fad (`FadIndustry`), clears
/// `TmpExt1..3`, zeroes `EarningsYTD` of new-game corps, records the history tick while
/// `GameInProgress = 1` ([`update_price_history_tick`]).
///
/// For corps > 85 the target is scaled by the cash-flow outlook of
/// [`crate::logic::cashflow::show_cash_flow_projection`] (mode 1; it also rebalances cash/T-bills and
/// recomputes the financials): x1.05 for a positive outlook (status 1, or `TmpExt1 = -99` below 4),
/// x0.95 for status 7, x0.9 for status 8.
/// Runs `CheckPriceAlerts(corpId)` for the selected corp in modes 1..3 (price alert pop-ups; clears
/// triggered `PriceAlerts`).
pub fn calc_stock_price(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_ref: i32, time_secs: i32) {
    // ON ERROR GOTO: ShowRuntimeError(ERR, …) and exit — error path ignored.
    let corp_id = corp_ref;
    if corp_id < 11 || g.entity_status[corp_id] != 1 {
        return;
    }
    let c = corp_id;
    if g.shares_outstanding[c] == Ext::from_f64(0.0) || !(g.market_cap[c] >= Ext::from_f64(0.0)) {
        g.shares_outstanding[c] = Ext::from_f64(1.0);
        g.market_cap[c] = Ext::from_f64(10.0);
        g.valuation_mode = 3;
    }
    let mut f = Frame::default();
    if g.market_cap[c] > Ext::from_f64(0.0) {
        f.old_cap = g.market_cap[c];
    }
    if g.game_end_flag == "Y" && g.valuation_mode > 0 && g.valuation_mode < 4 {
        calc_entity_financials(g, ui, rng, c);
        g.market_cap[c] = f.old_cap;
        return;
    }
    if g.valuation_mode != 0 {
        calc_entity_financials(g, ui, rng, c);
        if g.valuation_mode == 1 || g.valuation_mode == 2 {
            if g.credit_rating_num[c] < 2 || !(f.old_cap >= g.net_worth * s(0.0068f32)) {
                g.valuation_mode = 3;
            }
        }
        f.industry = g.industry_num[c];
        f.total_assets = to_quad(g.total_assets);
        if f.industry == 1 {
            f.total_assets = to_quad(g.total_assets - (g.bank_debt[c + 1] + g.bank_largest_loan));
        }
        if f.industry < 3 {
            f.bond_loss_adj = -Ext::from_f64(1.0) * g.bond_unrealized_loss;
        }
        if f.industry == 71 && g.valuation_mode < 4 {
            g.valuation_mode = 3;
        } else {
            target_cap(g, ui, rng, c, &mut f);
        }

        if g.valuation_mode == 3 || g.valuation_mode == 4 {
            if f.industry == 71 && g.valuation_mode == 3 && g.net_worth > Ext::from_f64(10.0) {
                f.target_cap = (g.net_worth * s(0.01f32)) * s(0.98f32);
                let r = rng.rnd();
                let adj = ((g.net_worth * s(0.01f32)) * s(0.04f32)) * r;
                f.target_cap = adj + f.target_cap;
                if !(f.target_cap >= g.shares_outstanding[c] * Ext::from_f64(0.25)) {
                    f.target_cap = g.shares_outstanding[c] * Ext::from_f64(0.25);
                }
            }
            g.market_cap[c] = f.target_cap;
            g.calc_stock_price = f.target_cap;
        } else {
            move_toward_target(g, rng, c, &mut f, time_secs);
        }

        if f.est_earnings > Ext::from_f64(0.0)
            && f.est_earnings > f.part_earnings
            && f.industry != 71
            && g.net_worth + g.contingency_reserve[c] > g.market_cap[c] * s(0.012f32)
            && !(g.market_cap[c] >= Ext::from_f64(65.0))
            && g.takeover_candidate_id == 0
        {
            g.takeover_candidate_id = c;
        }
    }

    g.tmp_share_price = g.market_cap[c] / g.shares_outstanding[c];
    g.stock_price_str = rtrim(ltrim(&format(g.tmp_share_price, b"* ###.00")).as_bytes());
    if !(g.tmp_share_price >= Ext::from_f64(0.25)) {
        g.tmp_share_price = Ext::from_f64(0.25);
        g.stock_price_str = PbStr::from("0.25");
        g.ticker_change_str = PbStr::new();
        g.market_cap[c] = g.shares_outstanding[c] * Ext::from_f64(0.25);
    }
    if g.valuation_mode == 4 {
        g.market_cap[c] = f.old_cap;
    }
    if g.game_in_progress == 1 {
        update_price_history_tick(g, ui, rng, c);
        if g.valuation_mode > 0
            && g.valuation_mode < 4
            && (g.sel_entity == c || g.last_sel_entity == c)
        {
            crate::logic::reports_lists::check_price_alerts(g, ui, rng, c);
        }
    }
    if g.game_end_flag == "Y" && g.valuation_mode == 0 {
        g.market_cap[c] = f.old_cap;
    }
    g.valuation_mode = 1;
}

/// Locals of CalcStockPrice shared between its parts (PB zero-initialises locals).
#[derive(Default)]
struct Frame {
    old_cap: Ext,
    industry: i16,
    total_assets: i64,
    bond_loss_adj: Ext,
    target_cap: Ext,
    /// `partEarnings`: first a quarter-weighted share of last year's earnings, later field 1 / field 3
    /// of `EntMiscData` (estimate before / current).
    part_earnings: Ext,
    /// `estEarnings`: field 3 of `EntMiscData` (current earnings estimate).
    est_earnings: Ext,
}

/// The target-cap computation of CalcStockPrice (0x42ca74..0x42ff67), run unless the corp is a fund in
/// mode 1..3.
fn target_cap(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: i32, f: &mut Frame) {
    let industry = f.industry;
    let ta = cast::<Ext, _>(f.total_assets);

    // --- cash offset and net assets
    let mut cash_offset: i64 = 0;
    let cash_tb = g.t_bills[c] + g.cash[c];
    if !(-s(0.01f32) >= cash_tb) && g.tmp_total_debt > cash_tb {
        cash_offset = to_quad(g.cash[c] + g.t_bills[c]);
    } else if !(g.tmp_total_debt >= cash_tb) {
        cash_offset = to_quad(g.tmp_total_debt);
    }
    cash_offset = to_quad(cast::<Ext, _>(cash_offset) * s(0.5f32));
    let bonds_adv = cast::<Ext, _>(g.bonds_issued[c]) + g.tmp_player_advances;
    if !(cast::<Ext, _>(cash_offset) >= bonds_adv) {
        cash_offset = to_quad(bonds_adv);
    }
    let net_assets = to_quad(ta - cast::<Ext, _>(cash_offset));
    let na = cast::<Ext, _>(net_assets);

    let mut tc = ta * s(0.05f32);
    if industry == 1 {
        tc = ta * s(0.01f32);
    }
    if industry == 2 {
        tc = ta * s(0.02f32);
    }
    if industry < 3 && !(f.bond_loss_adj + g.tmp_credit_net_worth >= Ext::from_f64(0.0)) {
        let solvent_bank = industry == 1
            && (f.bond_loss_adj + g.tmp_credit_net_worth) + g.bad_debt_reserve[c] > Ext::from_f64(0.0);
        if !solvent_bank {
            tc = s(0.07f32) * tc;
            let mc = g.market_cap[c];
            if !(tc >= mc * s(0.7f32)) {
                let r = rng.rnd();
                tc = mc * s(0.7f32) + mc * (s(0.15f32) * r);
            }
        }
    }

    // --- P/E multiple from the earnings history
    let mut pe: f32 = cast::<f32, _>(Ext::from_f32(8.0f32));
    let y4 = g.earnings_yr4_ago[c];
    let y3 = g.earnings_yr3_ago[c];
    let y2 = g.earnings_yr2_ago[c];
    let y1 = g.earnings_y1[c];
    let up_down = |pe: f32, up: bool| add_s(pe, if up { Ext::from_f64(1.0) } else { -Ext::from_f64(1.0) });
    if y4 != Ext::from_f64(0.0) {
        pe = up_down(pe, y3 >= y4);
        pe = up_down(pe, y2 >= y4);
        pe = up_down(pe, y1 >= y4);
    }
    if y3 != Ext::from_f64(0.0) {
        pe = up_down(pe, y2 >= y3);
        pe = up_down(pe, y1 >= y3);
    }
    pe = up_down(pe, y1 >= y2 && y2 != Ext::from_f64(0.0));

    let q = g.cur_quarter;
    let mut part = Ext::from_f64(0.0);
    if q == 3 {
        part = y1 * s(0.25f32);
    }
    if q == 2 {
        part = y1 * s(0.5f32);
    }
    if q == 1 {
        part = y1 * s(0.75f32);
    }
    if !(g.sim_co_count > c) {
        part = part + y1 * s(0.25f32);
    } else if q == 4 {
        part = y1;
    }
    if q == 1 && g.sim_co_count < c && g.num_players + 1 <= g.sim_co_count {
        g.earnings_ytd[c] = Ext::from_f64(0.0);
    }
    let mut proj = part + g.earnings_ytd[c];
    if g.earnings_ytd[c] > proj {
        proj = g.earnings_ytd[c];
    }
    pe = up_down(pe, proj > y1);
    let mut raw = proj;
    let pqe = g.proj_qtr_earnings[c];
    if pqe != Ext::from_f64(0.0) {
        proj = pqe + proj * s(0.75f32);
    }
    if !(pqe > Ext::from_f64(0.0) && pqe > proj * s(0.5f32)) {
        raw = proj;
    }
    if g.credit_rating_num[c] > 4 && proj > g.net_worth * s(0.2f32) {
        let avg_net_worth = (g.corp_prior_net_worth[c] + g.net_worth) / Ext::from_f64(2.0);
        if proj > avg_net_worth * s(0.25f32) {
            proj = avg_net_worth * s(0.25f32);
        }
    }
    let mut adj_earnings = proj;

    let misc = g.ent_misc_data[c].clone();
    let mut asset_yield_pct: i16 = 0;
    if parse_count_any(misc.as_bytes(), b"|") > 29 {
        asset_yield_pct = to_integer(val(parse_any(misc.as_bytes(), b"|", 29).as_bytes()));
    }
    let ayp = cast::<Ext, _>(asset_yield_pct);
    let pm = g.profit_margin[c];
    let mut low_assets = false;
    let mut outlook_next: f32 = cast::<f32, _>(Ext::from_f32(0.0f32));
    let ctr = cast::<Ext, _>(g.corp_tax_rate);
    // after-tax yield on investable assets, capped at 15 % of the projected earnings
    let asset_income = |base: Ext, proj: Ext| {
        let mut adj = (s(0.01f32) * base) * ayp;
        adj = (Ext::from_f64(1.0) - ctr * s(0.01f32)) * adj;
        if adj > proj * s(0.15f32) {
            adj = proj * s(0.15f32);
        }
        if !(proj > Ext::from_f64(0.0)) {
            adj = Ext::from_f64(0.0);
        }
        adj
    };
    let sel = cast::<Ext, _>(industry);
    if sel == Ext::from_f64(1.0) {
        if pm > 10 {
            pe = add_s(pe, Ext::from_f64(2.0));
        }
        if pm < -8 {
            pe = add_s(pe, -Ext::from_f64(2.0));
        }
        if asset_yield_pct > 6 {
            if asset_yield_pct < 20 {
                pe = add_s(pe, ayp / Ext::from_f64(4.0));
            } else {
                pe = add_s(pe, ayp / Ext::from_f64(6.0));
            }
        } else if asset_yield_pct < 4 {
            pe = add_s(pe, -Ext::from_f64(1.0));
        }
    } else if sel == Ext::from_f64(2.0) {
        if !(Ext::from_f64(5.0) >= cast::<Ext, _>(pe)) {
            let gr = cast::<Ext, _>(g.growth_rate[c]);
            if !(cast::<Ext, _>(pe) * Ext::from_f64(2.0) >= gr) {
                if gr / Ext::from_f64(2.0) > s(1.3f32) * cast::<Ext, _>(pe) {
                    pe = cast::<f32, _>(s(1.3f32) * cast::<Ext, _>(pe));
                } else {
                    pe = cast::<f32, _>(gr / Ext::from_f64(2.0));
                }
            }
        }
        if pm > 10 {
            pe = add_s(pe, Ext::from_f64(5.0));
        }
        if pm < -8 {
            pe = add_s(pe, -Ext::from_f64(5.0));
        }
        let adj = asset_income(g.ins_reserves[c], proj);
        adj_earnings = proj + adj;
    } else if sel != Ext::from_f64(70.0) && sel != Ext::from_f64(71.0) {
        if g.business_assets[c] > ta * s(0.2f32) {
            let adj = asset_income(g.business_assets[c], proj);
            adj_earnings = proj + adj;
            calc_industry_outlook(g, rng, industry);
            let outlook_cur = cast::<f32, _>(g.tmp_ext1);
            outlook_next = cast::<f32, _>(g.tmp_ext2);
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
            pe = add_s(pe, if !(outlook_next >= outlook_cur) { Ext::from_f64(2.0) } else { -Ext::from_f64(2.0) });
            let growth = to_long(cast::<Ext, _>(g.ind_growth_forecast[cast::<i32, _>(industry)]) / Ext::from_f64(10.0));
            pe = add_s(pe, cast::<Ext, _>(growth));
            let margin = cast::<Ext, _>(to_long(cast::<Ext, _>(pm) + cast::<Ext, _>(outlook_next)));
            if margin > Ext::from_f64(30.0) {
                pe = add_s(pe, Ext::from_f64(4.0));
            } else if margin > Ext::from_f64(20.0) {
                pe = add_s(pe, Ext::from_f64(3.0));
            } else if margin >= Ext::from_f64(13.0) {
                pe = add_s(pe, Ext::from_f64(2.0));
            } else if margin > Ext::from_f64(5.0) {
                pe = add_s(pe, Ext::from_f64(1.0));
            } else if !(margin >= -Ext::from_f64(15.0)) {
                pe = add_s(pe, -Ext::from_f64(2.0));
            } else if !(margin > -Ext::from_f64(5.0)) {
                pe = add_s(pe, -Ext::from_f64(1.0));
            }
        } else if !(g.business_assets[c] >= ta * s(0.3f32)) {
            low_assets = true;
        }
    }

    // --- credit rating
    let crs = g.credit_rating_str.clone();
    if instr(1, crs.as_bytes(), b"D") > 0 {
        pe = cast::<f32, _>(s(0.7f32) * cast::<Ext, _>(pe));
    }
    if instr(1, crs.as_bytes(), b"C") > 0 {
        pe = add_s(pe, -Ext::from_f64(1.0));
    }
    if instr(1, crs.as_bytes(), b"B") > 0 {
        pe = add_s(pe, Ext::from_f64(1.0));
    }
    if crs == "BBB" {
        pe = add_s(pe, Ext::from_f64(1.0));
    }
    if instr(1, crs.as_bytes(), b"A") > 0 {
        pe = add_s(pe, Ext::from_f64(2.0));
    }
    if instr(1, crs.as_bytes(), b"AA") > 0 {
        pe = add_s(pe, Ext::from_f64(1.0));
    }

    // --- earnings estimate (EntMiscData field 3) vs. the previous one (field 1)
    if len(misc.as_bytes()) > 20 {
        f.est_earnings = val(parse_any(misc.as_bytes(), b"|", 3).as_bytes());
        f.part_earnings = val(parse_any(misc.as_bytes(), b"|", 1).as_bytes());
        pe = add_s(pe, if f.est_earnings > f.part_earnings { Ext::from_f64(2.0) } else { -Ext::from_f64(2.0) });
        if !(f.est_earnings >= Ext::from_f64(0.0)) && !(Ext::from_f64(20.0) >= cast::<Ext, _>(pe)) {
            pe = cast::<f32, _>(s(0.8f32) * cast::<Ext, _>(pe));
        }
        if pqe > Ext::from_f64(0.0) {
            if pqe > f.est_earnings {
                pe = cast::<f32, _>(s(1.2f32) * cast::<Ext, _>(pe));
            } else {
                pe = cast::<f32, _>(s(0.83f32) * cast::<Ext, _>(pe));
            }
        }
    }
    if y4 > Ext::from_f64(0.0) && y3 > Ext::from_f64(0.0) && y2 > Ext::from_f64(0.0) && y1 > Ext::from_f64(0.0) && g.earnings_ytd[c] >= Ext::from_f64(0.0) {
        pe = add_s(pe, Ext::from_f64(5.0));
    }
    let iltg = g.ind_long_term_growth[cast::<i32, _>(industry)];
    if industry > 2 && industry < 70 && !(Ext::from_f64(18.0) >= cast::<Ext, _>(pe)) && iltg < 13 {
        pe = cast::<f32, _>(s(0.8f32) * cast::<Ext, _>(pe));
        if g.ind_long_term_growth[cast::<i32, _>(industry)] < 7 {
            pe = cast::<f32, _>(s(0.75f32) * cast::<Ext, _>(pe));
        }
    } else if industry > 2 && industry < 70 && Ext::from_f64(15.0) > cast::<Ext, _>(pe) && iltg > 1 {
        pe = cast::<f32, _>((cast::<Ext, _>(g.ind_long_term_growth[cast::<i32, _>(industry)]) * s(0.02f32) + Ext::from_f64(1.0)) * cast::<Ext, _>(pe));
    }
    let bir = cast::<Ext, _>(g.base_interest_rate);
    if !(Ext::from_f64(2.0) >= bir) {
        let adj = sqr(Ext::from_f64(8.0) / bir);
        pe = cast::<f32, _>(adj * cast::<Ext, _>(pe));
    }
    let mut gdp_adj = cast::<f32, _>(s(0.5f32) * cast::<Ext, _>(g.gdp_growth_target));
    if -Ext::from_f64(5.0) > cast::<Ext, _>(gdp_adj) {
        gdp_adj = -5.0f32;
    }
    pe = add_s(pe, cast::<Ext, _>(gdp_adj));
    if Ext::from_f64(5.0) > cast::<Ext, _>(pe) {
        pe = 5.0f32;
    }
    if adj_earnings * cast::<Ext, _>(pe) > Ext::from_f64(0.0) {
        tc = adj_earnings * cast::<Ext, _>(pe);
    }
    let earnings_cap = tc;

    // --- book value
    let nw = g.net_worth;
    if nw > Ext::from_f64(0.0) {
        let mut futures_tax = Ext::from_f64(0.0);
        if c > 130 && g.futures_net_value > Ext::from_f64(0.0) {
            futures_tax = g.futures_net_value;
            if !(g.tax_loss_carry[c] >= Ext::from_f64(0.0)) {
                futures_tax = g.tax_loss_carry[c] + futures_tax;
            }
            futures_tax = (ctr * s(0.01f32)) * futures_tax;
            if !(futures_tax >= Ext::from_f64(0.0)) {
                futures_tax = Ext::from_f64(0.0);
            }
        }
        let mut book_value = (((nw + f.bond_loss_adj) + g.tax_credits[c] * s(0.5f32)) + g.contingency_reserve[c])
            - futures_tax;
        if g.tax_loss_carry[c] >= Ext::from_f64(0.0) {
            book_value = g.tax_credits[c] * s(0.25f32) + book_value;
        }
        book_value = book_value * s(0.8f32);
        if industry == 2 {
            book_value = ((g.contingency_reserve[c] + nw) + g.ins_reserves[c] * s(0.2f32)) * s(0.85f32);
        }
        if industry > 2 && industry < 70 {
            let margin = to_long(cast::<Ext, _>(pm) + cast::<Ext, _>(outlook_next));
            book_value = (s(0.01f32) * cast::<Ext, _>(margin) + Ext::from_f64(1.0)) * book_value;
        }
        if book_value > earnings_cap {
            tc = book_value;
        }
    }
    if g.last_fraud_corp == c {
        tc = s(0.65f32) * tc;
    }
    if g.valuation_mode != 4 && c > 85 {
        if g.investor_sentiment[c] > 50 {
            g.investor_sentiment[c] = 50;
        }
        if g.investor_sentiment[c] < -40 {
            g.investor_sentiment[c] = -40;
        }
        let mut sentiment_mult: Ext = cast::<Ext, _>(g.investor_sentiment[c]);
        sentiment_mult = Ext::from_f64(100.0) + sentiment_mult;
        sentiment_mult = s(0.01f32) * sentiment_mult;
        tc = sentiment_mult * tc;
    }

    // --- floors and ceiling
    let mut adj_factor = s(0.3f32);
    if industry == 2 {
        adj_factor = s(0.1f32);
    }
    if industry == 1 {
        adj_factor = s(0.05f32);
    }
    let mut cap_floor = na * adj_factor;
    if industry == 1 {
        let x = g.bad_debt_reserve[c] * s(0.9f32) + nw;
        if !(cap_floor >= x) {
            cap_floor = x;
        }
    }
    if cap_floor > tc {
        tc = cap_floor;
    }
    if !(g.tax_loss_carry[c] >= Ext::from_f64(0.0)) {
        cap_floor = ((ctr * s(0.01f32)) * s(0.35f32)) * g.tax_loss_carry[c].abs();
        if cap_floor > tc {
            tc = cap_floor;
        }
    }
    let mut cap_ceiling = g.contingency_reserve[c] + nw;
    cap_ceiling = Ext::from_f64(2.5) * (na + cap_ceiling);
    if cap_ceiling > Ext::from_f64(0.0) && tc > cap_ceiling {
        tc = cap_ceiling;
    }
    if g.market_valuation == Ext::from_f32(0.0f32) {
        g.market_valuation = cast::<f32, _>(Ext::from_f32(1.0f32));
    }
    if Ext::from_f64(0.5) > cast::<Ext, _>(g.market_valuation) {
        g.market_valuation = cast::<f32, _>(Ext::from_f32(0.5f32));
    }
    if !(Ext::from_f64(1.5) >= cast::<Ext, _>(g.market_valuation)) {
        g.market_valuation = cast::<f32, _>(Ext::from_f32(1.5f32));
    }
    tc = cast::<Ext, _>(g.market_valuation) * tc;
    if g.industry_num[c] == 69 && nw > Ext::from_f64(0.0) && g.credit_rating_num[c] > 5 && tc > nw * Ext::from_f64(2.5) {
        tc = tc * s(0.6f32);
    }

    // --- bubbles and fads
    let mut fad_hit_corp = 0;
    if g.bubble_state > 100 && !low_assets {
        let mut stage = g.bubble_state;
        if stage > 200 {
            stage -= 100;
        }
        if stage > 102 && stage < 106 {
            let sel = g.industry_num[c];
            let mut hit = false;
            if sel == 4 || sel == 21 {
                if g.bubble_state > 200 {
                    tc = s(1.7f32) * tc;
                }
                hit = true;
            } else if [10, 24, 26, 30, 35, 37, 39, 40, 61].contains(&sel) {
                if g.bubble_state < 200 {
                    tc = s(1.7f32) * tc;
                }
                hit = true;
            }
            if hit {
                if cast::<i32, _>(g.industry_num[c]) == g.fad_industry {
                    g.fad_industry = 0;
                    g.fad_valuation_pct = 0;
                }
                fad_hit_corp = c;
            }
            if stage > 104 {
                tc = s(1.5f32) * tc;
            }
        }
    } else if cast::<i32, _>(g.industry_num[c]) == g.fad_industry && !low_assets {
        if g.fad_valuation_pct > 0 {
            tc = (s(0.01f32) * cast::<Ext, _>(g.fad_valuation_pct)) * tc;
        }
        fad_hit_corp = c;
    }

    if c > 130 && g.credit_rating_num[c] < 2 && g.valuation_mode == 1 {
        if !(ta * Ext::from_f64(2.0) >= g.market_cap[c] * Ext::from_f64(100.0)) {
            tc = s(0.35f32) * tc;
            g.valuation_mode = 2;
            if !(ta >= Ext::from_f64(25.0)) {
                tc = s(0.45f32);
                g.valuation_mode = 3;
            }
        }
    }
    tc = s(0.01f32) * tc;
    if cast::<i32, _>(g.hot_stock_corp) == c {
        tc = s(1.25f32) * tc;
    }
    let field_text = if len(misc.as_bytes()) > 20 { parse_any(misc.as_bytes(), b"|", 3) } else { misc.clone() };
    f.part_earnings = val(field_text.as_bytes());
    let part = f.part_earnings;
    if pqe != Ext::from_f64(0.0) {
        if pqe > part && part != Ext::from_f64(0.0) {
            tc = s(1.1f32) * tc;
        }
        if !(pqe >= part) && part != Ext::from_f64(0.0) {
            tc = s(0.9f32) * tc;
        }
    }

    // --- special cases (GOTO LAB_0042f34b ends the chain)
    let crn = g.credit_rating_num[c];
    let equity = |g: &Globals| g.contingency_reserve[c] + g.net_worth;
    'chain: {
        if !(tc >= equity(g) * s(0.0085f32)) {
            tc = equity(g) * s(0.0087f32);
            if y1 == Ext::from_f64(0.0) && y2 == Ext::from_f64(0.0) {
                tc = equity(g) * s(0.0107f32);
            }
            if g.valuation_mode == 1 {
                g.valuation_mode = 2;
            }
            break 'chain;
        }
        if crn > 5 && industry == 2 {
            let eq = equity(g);
            let r = rng.rnd();
            if tc > eq * (s(0.004f32) * r + s(0.018f32))
                && adj_earnings > Ext::from_f64(0.0)
                && g.corp_prior_net_worth[c] > Ext::from_f64(0.0)
                && !(adj_earnings / g.corp_prior_net_worth[c] >= s(0.12f32))
            {
                let r = rng.rnd();
                tc = eq * (s(0.005f32) * r + s(0.015f32));
                break 'chain;
            }
        }
        if crn > 7 && g.industry_num[c] > 1 && !(ta * s(0.7f32) >= g.t_bills[c] + g.cash[c]) {
            tc = equity(g) * s(0.009f32);
            let r = rng.rnd();
            tc = (s(0.08f32) * r + s(1.05f32)) * tc;
            if g.valuation_mode == 1 {
                g.valuation_mode = 2;
            }
            break 'chain;
        }
        if crn > 5
            && g.industry_num[c] == 1
            && tc > (g.bad_debt_reserve[c] + nw) * s(0.027f32)
            && g.fad_industry != 1
            && g.market_cap[c] > g.exchange_rate * Ext::from_f64(100.0)
        {
            tc = (g.bad_debt_reserve[c] + nw) * s(0.025f32);
            let r = rng.rnd();
            tc = ((s(0.1f32) * r) * cast::<Ext, _>(q) + s(1.05f32)) * tc;
            for k in [s(0.35f32), s(0.25f32), s(0.15f32)] {
                if tc > k * adj_earnings {
                    tc = s(0.85f32) * tc;
                }
            }
        }
    }
    // LAB_0042f34b
    if g.million_prefix_uc == "M" && !(tc >= Ext::from_f64(0.5)) {
        tc = s(0.1f32) * rng.rnd() + s(0.4f32);
    }
    if g.million_prefix_uc == "B" && !(tc >= s(0.1f32)) {
        tc = s(0.02f32) * rng.rnd() + s(0.02f32);
    }
    if g.shares_outstanding[c] == Ext::from_f64(0.0) {
        g.shares_outstanding[c] = Ext::from_f64(1.0);
    }
    if g.valuation_mode == 1 && q == 1 && g.start_year == g.cur_year {
        g.valuation_mode = if g.sim_co_count < 1500 { 3 } else { 2 };
    }
    if g.valuation_mode == 1 && crn > 1 && !(tc >= equity(g) * s(0.0087f32)) {
        tc = equity(g) * s(0.012f32);
        g.valuation_mode = 2;
    }
    if crn > 3 {
        if g.industry_num[c] == 70 {
            if tc > nw * s(0.0112f32) || !(tc >= nw * s(0.0093f32)) {
                tc = nw * s(0.0096f32);
                tc = (cast::<Ext, _>(crn) * s(0.01f32) + s(1.01f32)) * tc;
                if pqe > part {
                    tc = tc * s(1.05f32);
                } else {
                    tc = tc * s(0.97f32);
                }
                if g.valuation_mode == 1 {
                    g.valuation_mode = 2;
                }
            }
        } else if g.industry_num[c] < 70
            && fad_hit_corp != c
            && g.rand_seed % 3 == 1
            && !(tc >= nw * s(0.015f32))
        {
            if f.est_earnings > Ext::from_f64(0.0) || pqe > Ext::from_f64(0.0) || adj_earnings > Ext::from_f64(0.0) {
                tc = (s(0.002f32) * rng.rnd() + s(0.014f32)) * nw;
            }
        }
    }

    // --- P/E and P/B caps
    if c > 85 && g.industry_num[c] < 71 && adj_earnings > Ext::from_f64(0.0) && nw > Ext::from_f64(0.0) && crn > 1 && fad_hit_corp != c {
        let d: Ext = nw / adj_earnings;
        let max_pe: i32;
        let mut max_pb: Ext;
        if !(d >= Ext::from_f64(5.0)) {
            max_pe = to_long(d * s(0.3f32) + Ext::from_f64(25.0));
            max_pb = s(1.1f32);
        } else if !(d >= Ext::from_f64(10.0)) {
            max_pe = to_long(d * s(0.2f32) + Ext::from_f64(20.0));
            max_pb = s(1.3f32);
        } else if !(d >= Ext::from_f64(15.0)) {
            max_pe = to_long(Ext::from_f64(5.0) + d);
            max_pb = s(1.6f32);
        } else if !(d >= Ext::from_f64(20.0)) {
            max_pe = to_long(d * s(0.93f32));
            max_pb = Ext::from_f64(2.0);
        } else if !(d >= Ext::from_f64(25.0)) {
            let mut m = to_long(d * s(0.9f32));
            max_pb = Ext::from_f64(2.5);
            if crn < 7 {
                max_pb = s(2.8f32);
                m = 25;
            }
            if crn < 5 {
                max_pb = Ext::from_f64(3.0);
                m = 27;
            }
            max_pe = m;
        } else if !(d > Ext::from_f64(30.0)) {
            let mut m = to_long(d * s(0.88f32));
            max_pb = s(3.2f32);
            if crn < 7 {
                max_pb = Ext::from_f64(3.5);
                m = 30;
            }
            if crn < 5 {
                max_pb = s(3.7f32);
                m = 33;
            }
            max_pe = m;
        } else {
            let mut m = to_long(d * s(0.2f32) + Ext::from_f64(30.0));
            max_pb = d * s(0.1f32) + s(3.6f32);
            if crn < 7 {
                max_pb = Ext::from_f64(0.5) + max_pb;
                m += 3;
            }
            if crn < 5 {
                max_pb = Ext::from_f64(1.0) + max_pb;
                m += 2;
            }
            max_pe = m;
        }
        pe = cast::<f32, _>((Ext::from_f64(100.0) * tc) / adj_earnings);
        let pb_ratio: Ext = (Ext::from_f64(100.0) * tc) / nw;
        if !(cast::<Ext, _>(max_pe) >= cast::<Ext, _>(pe)) && !(max_pb >= pb_ratio) {
            if crn <= 5 {
                tc = (adj_earnings * s(0.01f32)) * cast::<Ext, _>(max_pe);
            } else {
                tc = (max_pb * s(0.01f32)) * nw;
                pe = cast::<f32, _>((Ext::from_f64(100.0) * tc) / adj_earnings);
                if cast::<Ext, _>(max_pe) > cast::<Ext, _>(pe) {
                    tc = ((adj_earnings * s(0.01f32)) * cast::<Ext, _>(max_pe)) * s(0.7f32);
                }
            }
        }
    }
    if g.industry_num[c] > 1 && raw > Ext::from_f64(0.0) {
        let floor_pe = s(0.5f32) * rng.rnd() + s(3.5f32);
        if !((Ext::from_f64(100.0) * g.market_cap[c]) / raw >= floor_pe) && !(tc >= nw * s(0.027f32)) && g.rand_seed % 7 < 6 {
            let r = rng.rnd();
            tc = (raw * s(0.05f32)) * (s(0.05f32) * r + s(1.15f32));
        }
    }
    if y3 == Ext::from_f64(0.0) && y4 == Ext::from_f64(0.0) && crn > 3 && tc > nw * s(0.013f32) {
        tc = tc * s(0.8f32);
        if y1 != Ext::from_f64(0.0) {
            tc = s(1.05f32) * tc;
        }
        if y2 != Ext::from_f64(0.0) {
            tc = s(1.04f32) * tc;
        }
    }
    if c > 85 {
        let cash_flow_outlook = to_long(crate::logic::cashflow::show_cash_flow_projection(g, ui, rng, c, 1));
        if (cash_flow_outlook == 1 || g.tmp_ext1 == -Ext::from_f64(99.0)) && cash_flow_outlook < 4 {
            tc = s(1.05f32) * tc;
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
        if cash_flow_outlook == 7 {
            tc = s(0.95f32) * tc;
        } else if cash_flow_outlook == 8 {
            tc = s(0.9f32) * tc;
        }
    }
    f.target_cap = tc;
}

/// Modes 1/2 of CalcStockPrice (0x430096..0x43051f): move the cap part of the way toward the target.
fn move_toward_target(g: &mut Globals, rng: &mut Rng, c: i32, f: &mut Frame, time_secs: i32) {
    let mut tc = f.target_cap;
    if g.monetary_stance == 1 {
        tc = s(0.85f32) * tc;
    }
    if !(tc >= g.shares_outstanding[c] * Ext::from_f64(0.25)) {
        tc = g.shares_outstanding[c] * Ext::from_f64(0.25);
    }
    // RandSeed = RandSeed + VAL(RIGHT$(TIME$, 2))
    g.rand_seed = to_long(cast::<Ext, _>(time_secs) + cast::<Ext, _>(g.rand_seed));
    if g.rand_seed > 32500 {
        g.rand_seed -= 1000;
    }
    let r = rng.rnd();
    tc = tc * s(0.95f32) + tc * (s(0.1f32) * r);
    f.target_cap = tc;
    let old_cap = f.old_cap;
    let mut cap_change = tc - old_cap;
    if g.valuation_mode == 1 {
        if int(Ext::from_f64(17.0) * rng.rnd()) == Ext::from_f64(1.0) {
            cap_change = cap_change * s(0.3f32);
        } else {
            let r = rng.rnd();
            cap_change = (s(0.05f32) * r + s(0.05f32)) * cap_change;
        }
    } else if g.valuation_mode == 2 {
        cap_change = cap_change * s(0.35f32);
    }
    if cap_change.abs() > old_cap * s(0.2f32) {
        cap_change = cap_change * s(0.2f32);
    }
    if g.market_cap[c] / g.shares_outstanding[c] > Ext::from_f64(12.0) && c < 86 {
        if !(old_cap >= tc * s(0.85f32)) {
            let r = rng.rnd();
            cap_change = (s(0.09f32) * r + s(0.02f32)) * old_cap;
        }
        if old_cap > tc * s(1.2f32) {
            let r = rng.rnd();
            cap_change = -Ext::from_f64(1.0) * ((s(0.08f32) * r + s(0.05f32)) * old_cap);
        }
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
            cap_change = -Ext::from_f64(1.0) * cap_change;
        }
    } else if c > 85 && g.market_cap[c] / g.shares_outstanding[c] > Ext::from_f64(12.0) && Ext::from_f64(5.0) * rng.rnd() > Ext::from_f64(4.0) {
        cap_change = -Ext::from_f64(1.0) * cap_change;
    }
    g.market_cap[c] = old_cap + cap_change;
    let sign = if cap_change > Ext::from_f64(0.0) { "_+" } else { "_" };
    let change_text = rtrim(ltrim(&format(cap_change / g.shares_outstanding[c], b"######.00")).as_bytes());
    g.ticker_change_str = PbStr::new();
    if cap_change != Ext::from_f64(0.0) {
        g.ticker_change_str = PbStr::from(sign) + &change_text;
    }
}

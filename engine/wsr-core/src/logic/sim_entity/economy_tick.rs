//! GOSUB blocks of SimProcessNextEntity (see [`super`]).
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::Frame;
#[allow(unused_imports)]
use super::prelude::*;

use crate::logic::earnings::balance_cash_t_bills;
use crate::logic::economy::{update_economy as macro_update_economy, update_interest_rates};
use crate::logic::finance::calc_entity_financials;
use crate::logic::futures::update_metals_price_base;
use crate::logic::valuation::calc_working_capital_ratio;

/// GOSUB SimEntity_UpdateEconomy (0x4c89e0): every 290th tick (SimCoCount = 291, 581, ...) — moves
/// `MarketValuation` toward its target, advances the economic cycle, GDP and the oil price target,
/// shifts industry growth forecasts with the economy, runs `SimCorpQuarterOperations` for all entities
/// and invests the growth spending of operating corps (business assets, working capital, tax credits),
/// then (unless `EconCycleCounter > 23`) `UpdateEconomy` and `UpdateInterestRates` with a prime-rate
/// headline.
pub(super) fn update_economy(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    if g.sim_co_count % 290 != 1 || g.sim_co_count < 290 {
        return;
    }
    f.prime_rate_save = g.prime_rate;
    if g.market_valuation == Ext::from_f32(0.0f32) {
        g.market_valuation = cast::<f32, _>(Ext::from_f32(1.0f32));
    }
    if g.market_val_target == Ext::from_f64(0.0) {
        g.market_val_target = cast::<f64, _>(cast::<Ext, _>(g.market_valuation));
    }
    f.cap_change = (g.market_val_target - cast::<Ext, _>(g.market_valuation)) * sf(0.25f32);
    g.market_valuation = cast::<f32, _>(cast::<f32, _>(f.cap_change + cast::<Ext, _>(g.market_valuation)));
    if Ext::from_f64(1.0) > g.market_val_target {
        g.market_val_target = cast::<f64, _>(sf(0.03f32) + g.market_val_target);
    } else {
        g.market_val_target = cast::<f64, _>(g.market_val_target - sf(0.03f32));
    }
    if 6 < g.econ_cycle_counter && (2 < g.rand_seed % 5 || 22 < g.econ_cycle_counter) {
        g.econ_cycle_counter = 25;
    }
    g.econ_cycle_counter = g.econ_cycle_counter.wrapping_add(1);
    if 0 < g.subprime_crisis_state && Ext::from_f64(10.0) * rng.rnd() > Ext::from_f64(1.0) {
        g.econ_cycle_counter = g.econ_cycle_counter.wrapping_sub(1);
    }
    if g.peak_oil_crisis == 1 {
        g.econ_cycle_counter = g.econ_cycle_counter.wrapping_add(1);
    }
    f.amount = sf(0.01f32) * (cast::<Ext, _>(g.gdp_growth) / Ext::from_f64(20.0)) * cast::<Ext, _>(g.gdp_level);
    g.gdp_level = to_quad(f.amount + cast::<Ext, _>(g.gdp_level));
    f.amount = (cast::<Ext, _>(g.gdp_growth) - sf(3.0f32)) / Ext::from_f64(2.0);
    if -Ext::from_f64(10.0) < cast::<Ext, _>(g.gdp_growth) {
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(f.amount * (sf(0.01f32) * rng.rnd()) + cast::<Ext, _>(g.oil_target_price)));
    } else if g.peak_oil_crisis < 1 {
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_target_price) - sf(0.02f32)));
    }
    f.tmp_long = to_quad(cast::<Ext, _>(g.cur_year) - (Ext::from_f64(15.0) + cast::<Ext, _>(g.start_year)));
    if 1 < f.tmp_long {
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(f.tmp_long) * sf(0.0002f32) + cast::<Ext, _>(g.oil_target_price)));
    }
    if sf(0.26f32) > cast::<Ext, _>(g.oil_price) {
        g.oil_price = cast::<f32, _>(cast::<f32, _>(sf(0.05f32) + cast::<Ext, _>(g.oil_price)));
    }
    if sf(0.5f32) * rng.rnd() > cast::<Ext, _>(g.oil_price) {
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.1f32) * cast::<Ext, _>(g.oil_target_price)));
    }
    if g.oil_crisis_level == Ext::from_f32(0.0f32) {
        if sf(0.75f32) * rng.rnd() + sf(2.25f32) < cast::<Ext, _>(g.oil_price) {
            g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_target_price) - sf(0.13f32)));
        }
    }
    f.tmp_long = to_quad(Ext::from_f64(7.0) * rng.rnd());
    if sf(0.7f32) > cast::<Ext, _>(g.oil_price) {
        if 3 < g.ind_growth_forecast[7] {
            g.ind_growth_forecast[7] =
                cast::<i16, _>(to_quad(cast::<Ext, _>(g.ind_growth_forecast[7]) - cast::<Ext, _>(f.tmp_long)));
        }
        if 4 < g.ind_growth_forecast[8] {
            g.ind_growth_forecast[8] =
                cast::<i16, _>(to_quad(cast::<Ext, _>(g.ind_growth_forecast[8]) - cast::<Ext, _>(f.tmp_long)));
        }
    }
    f.idx = 3;
    while f.idx <= 69 {
        f.line_marker = PbStr::from(__content.string(10827));
        let gdp = cast::<Ext, _>(g.gdp_growth);
        let hit = Ext::from_f64(3.0) * rng.rnd() + -Ext::from_f64(5.0) > gdp || !(rng.rnd() + sf(3.9f32) >= gdp);
        if hit {
            demand_shift(g, ui, rng, f);
        }
        f.idx += 1;
    }
    f.idx = 1;
    loop {
        g.rand_seed += 1;
        if 32500 < g.rand_seed {
            g.rand_seed = rng.rnd_range(1000, 100);
        }
        if !f.skips(0x4fc8a9) {
            super::corp_ops::sim_corp_quarter_operations(g, ui, rng, f.idx);
        }
        let i = f.idx;
        if 131 <= i
            && g.entity_status[i] == 1
            && !(g.business_assets[i] < sf(0.01f32))
            && g.growth_rate[i] != 0
            && g.industry_num[i] <= 69
        {
            g.growth_rate[i] = g.growth_rate[i].clamp(-10, 60);
            f.growth_invest = g.business_assets[i] * cast::<Ext, _>(g.growth_rate[i]) * sf(0.0005f32);
            f.amount =
                g.business_assets[i] * calc_working_capital_ratio(g, i) - g.working_capital[i];
            if g.credit_rating_num[i] < 8 {
                f.amount = sf(0.05f32) * f.amount;
            }
            if f.amount + f.growth_invest > g.t_bills[i] + g.cash[i]
                && f.amount + f.growth_invest > Ext::from_f64(0.0)
            {
                calc_entity_financials(g, ui, rng, i);
                if f.amount > Ext::from_f64(0.0) && g.liquid_funds > Ext::from_f64(0.0) {
                    if f.amount > g.liquid_funds {
                        f.amount = g.liquid_funds;
                    }
                    g.cash[i] = g.cash[i] - f.amount;
                    g.working_capital[i] = f.amount + g.working_capital[i];
                    g.liquid_funds = g.liquid_funds - f.amount;
                    f.amount = Ext::from_f64(0.0);
                }
            }
            g.cash[i] = g.cash[i] - f.amount;
            g.working_capital[i] = f.amount + g.working_capital[i];
            g.business_assets[i] = f.growth_invest + g.business_assets[i];
            g.cash[i] = g.cash[i] - f.growth_invest;
            balance_cash_t_bills(g, i);
            if g.cash[i] < Ext::from_f64(0.0) && !f.skips(0x506842) {
                super::corp_ops::draw_credit_line_for_cash(g, ui, rng, i);
            }
            if f.growth_invest > Ext::from_f64(0.0) {
                g.tax_credits[i] = f.growth_invest * sf(0.07f32) + g.tax_credits[i];
            }
        }
        f.idx += 1;
        if 1600 < f.idx {
            break;
        }
    }
    if 23 < g.econ_cycle_counter {
        return;
    }
    if !f.skips(0x458a7a) {
        macro_update_economy(g, ui, rng);
    }
    if !f.skips(0x4af1d2) {
        update_interest_rates(g, rng);
    }
    if g.rand_seed % 3 == 1 {
        f.tmp_long = to_quad((cast::<Ext, _>(g.rand_seed) % Ext::from_f64(50.0)) + Ext::from_f64(11.0));
        let c = cast::<i32, _>(f.tmp_long);
        if g.entity_status[c] == 1 {
            g.news_msg = g.ent_name[c].clone()
                + __content.string(10828)
                + &fmt_trim(cast::<Ext, _>(g.prime_rate), b"##.00")
                + "% ";
            post_news(g, ui, 0, 0);
        }
    }
}

/// The SELECT CASE on the industry `idx` inside SimEntity_UpdateEconomy's forecast loop: a target growth
/// `otherId` by industry group, and the forecast moved toward it by up to 10.
fn demand_shift(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.sel_case = cast::<Ext, _>(f.idx);
    let gdp = cast::<Ext, _>(g.gdp_growth);
    match f.idx {
        4 | 21 => {
            f.other_id = to_long(rng.rnd() * Ext::from_f64(9.0) + Ext::from_f64(6.0));
            if Ext::from_f64(0.0) < gdp {
                f.other_id = to_long(rng.rnd() * Ext::from_f64(12.0) + cast::<Ext, _>(f.other_id));
            }
            if 199 < g.bubble_state && g.bubble_state < 207 {
                f.other_id = 35;
            }
        }
        10 | 24 | 26 | 30 | 35 | 37 | 39 | 40 | 61 => {
            f.other_id = to_long(rng.rnd() * Ext::from_f64(12.0) + Ext::from_f64(3.0));
            if Ext::from_f64(0.0) < gdp {
                f.other_id = to_long(rng.rnd() * Ext::from_f64(12.0) + cast::<Ext, _>(f.other_id));
            }
            if 99 < g.bubble_state && g.bubble_state < 107 {
                f.other_id = 30;
            }
        }
        7 | 8 => {
            if Ext::from_f64(1.0) > cast::<Ext, _>(g.oil_price) {
                f.other_id = to_long(rng.rnd() * Ext::from_f64(3.0) + Ext::from_f64(0.0));
            } else {
                f.other_id = to_long(rng.rnd() * Ext::from_f64(4.0) + Ext::from_f64(5.0));
            }
            if g.peak_oil_crisis == 1 {
                f.other_id = to_long(rng.rnd() * Ext::from_f64(12.0) + Ext::from_f64(20.0));
            }
        }
        17 | 22 | 28 | 31 | 32 | 34 | 55 | 56 | 62 | 69 => {
            f.other_id = to_long(rng.rnd() * Ext::from_f64(3.0) + Ext::from_f64(3.0));
        }
        50 => {
            f.other_id = to_long((Ext::from_f64(5.0) + cast::<Ext, _>(g.silver_price)) - Ext::from_f64(5.0) * rng.rnd());
            if (cast::<Ext, _>(g.silver_price) - cast::<Ext, _>(g.metals_price_base) * sf(0.01f32)) < Ext::from_f64(15.0) {
                let base = update_metals_price_base(g, ui);
                if base < Ext::from_f64(1130.0) {
                    f.other_id = -2;
                }
            }
        }
        _ => {
            f.other_id = to_long(-Ext::from_f64(5.0) + Ext::from_f64(10.0) * rng.rnd());
            if Ext::from_f64(0.0) < gdp {
                f.other_id = to_long(rng.rnd() * Ext::from_f64(3.0) + Ext::from_f64(3.0));
            }
        }
    }
    let i = f.idx;
    if 0 < f.other_id && Ext::from_f64(3.0) * rng.rnd() + -Ext::from_f64(10.0) > cast::<Ext, _>(g.gdp_growth) {
        f.other_id -= 5;
    }
    if Ext::from_f64(0.0) > cast::<Ext, _>(g.gdp_growth) {
        if cast::<i32, _>(g.ind_growth_forecast[i]) >= f.other_id {
            g.ind_growth_forecast[i] =
                cast::<i16, _>(to_quad(cast::<Ext, _>(g.ind_growth_forecast[i]) - Ext::from_f64(10.0) * rng.rnd()));
        }
    } else if (cast::<i32, _>(g.ind_growth_forecast[i])) < f.other_id {
        g.ind_growth_forecast[i] =
            cast::<i16, _>(to_quad(Ext::from_f64(10.0) * rng.rnd() + cast::<Ext, _>(g.ind_growth_forecast[i])));
    }
}

/// GOSUB SimEntity_AsbestosSuperfund (0x4c9b79): picks a corp with a [runtime resource wording]
/// (Q2, from the third year) or runs `ProcessAsbestosLiability`; ends the Superfund cleanup (margin boost);
/// on a few fixed ticks the AI's R&D/marketing boost, the periodic stock sales (GOSUB
/// SimEntity_SellStockHelper) and cash-rich takeovers; bond trading for banks and insurers.
pub(super) fn asbestos_superfund(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    if g.asbestos_corp == 0 {
        if g.start_year + 2 < g.cur_year && g.cur_quarter == 2 {
            if int(cast::<Ext, _>(g.long_bond_price)) * Ext::from_f64(5.0) < cast::<Ext, _>(g.sim_co_count) {
                f.tmp_long = cast::<i64, _>(rng.rnd_range(320, 181));
                if g.entity_status[cast::<i32, _>(f.tmp_long)] == 1 && g.rand_seed % 5 == 3 {
                    g.asbestos_corp = cast::<i32, _>(f.tmp_long);
                    g.news_msg = g.ent_symbol[cast::<i32, _>(f.tmp_long)].clone()
                        + __content.string(10829);
                    f.other_id = cast::<i32, _>(f.tmp_long);
                    post_news(g, ui, f.other_id, 0);
                }
            }
        }
    } else if !f.skips(0x506c63) {
        super::corp_ops::process_asbestos_liability(g, ui, rng);
    }
    if g.superfund_end_year <= g.cur_year {
        let c = cast::<i32, _>(g.superfund_corp);
        if 10 < c && g.entity_status[c] == 1 && g.industry_num[c] < 70 {
            g.profit_margin[c] = cast::<i16, _>(to_quad(Ext::from_f64(10.0) * rng.rnd() + cast::<Ext, _>(g.profit_margin[c])));
        }
        g.superfund_corp = 0;
    }
    if (g.sim_co_count == 117 || g.sim_co_count == 1213) && !f.skips(0x4e07fa) {
        super::corp_ops::ai_boost_rnd_and_marketing(g, ui, rng);
    }
    if g.sim_co_count % 210 == 1 && !f.skips(0x4dfd87) {
        sell_stock_helper(g, ui, rng, f);
    }
    if g.sim_co_count % 75 == 1 && !f.skips(0x4fb4da) {
        super::ai_corp::ai_cash_rich_takeover(g, ui, rng, f.time_secs);
    }
    if (g.industry_num[f.ent_id] == 1 || g.industry_num[f.ent_id] == 2) && !f.skips(0x4dfde0) {
        fin_inst_bond_trading(g, ui, rng, f);
    }
}

/// GOSUB SimEntity_SellStockHelper (0x4dfd87): `SellStockHolding(0)` with TmpExt3 = 2 (entId is the
/// BYREF argument and is restored to SimCoCount).
pub(super) fn sell_stock_helper(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.line_marker = PbStr::from("35900");
    f.ent_id = 0;
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(2.0);
    if !f.skips(0x4ed93b) {
        super::transactions::sell_stock_holding(g, ui, rng, f.ent_id);
    }
    f.ent_id = g.sim_co_count;
}

/// GOSUB SimEntity_FinInstBondTrading (0x4dfde0): `AiFinInstBondTrading(SimCoCount)`.
pub(super) fn fin_inst_bond_trading(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
) {
    let __content = crate::content::current();
    f.line_marker =
        PbStr::from(__content.string(10830)) + &str_s(cast::<Ext, _>(f.ent_id)) + " SYM$=" + &g.ent_symbol[f.ent_id];
    if !f.skips(0x4dff05) {
        let c = g.sim_co_count;
        super::fin_inst::ai_fin_inst_bond_trading(g, ui, rng, c);
    }
}

/// GOSUB SimEntity_CleanHoldings (0x4c9a4b): clears the stock-holding slots 1..15 of `SimCoCount` that
/// hold no valid id, 0 %, or a stock that is no longer active.
pub(super) fn clean_holdings(g: &mut Globals, f: &mut Frame) {
    f.line_marker = PbStr::from("25350");
    let c = g.sim_co_count;
    f.slot = 1;
    loop {
        f.tmp_long = cast::<i64, _>(g.hold_stock_id[(f.slot, c)]);
        if f.tmp_long <= 0
            || g.hold_stock_pct[(f.slot, c)] == 0
            || g.entity_status[cast::<i32, _>(f.tmp_long)] != 1
        {
            g.hold_stock_id[(f.slot, c)] = 0;
            g.hold_stock_pct[(f.slot, c)] = 0;
        }
        f.slot += 1;
        if 15 < f.slot {
            return;
        }
    }
}

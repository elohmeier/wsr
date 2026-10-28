//! The quarterly macro step `UpdateEconomy_52978d` (0x52978d) with its GOSUB
//! `UpdateEconomy_IndGrowthForecasts` (0x52cb42), and the industry growth helpers
//! `UpdateIndustryLongTermGrowth` (0x4a56ec) and `AdjustIndLongTermGrowth` (0x4f8f90).
//!
//! INTEGER array stores of the original are 16-bit (`mov word`): sums wrap (`as i16`) and `fistp qword`
//! results are truncated to their low word (`to_quad(x) as i16`).
use crate::pb::real::{Ext, cast};


use super::*;
use crate::logic::finance::get_managing_player;
use crate::logic::futures::update_metals_price_base;
use crate::logic::valuation::{calc_stock_price, check_stock_split, update_price_history_tick};

/// `fistp qword` + `mov word`: the low 16 bits of the rounded value.
fn q16(x: Ext) -> i16 {
    cast::<i16, _>(to_quad(x))
}

/// `IndDemand(ind) = IndDemand(ind) * factor` (SINGLE constant, `fistp qword`).
fn scale_demand(g: &mut Globals, ind: i32, factor: f32) {
    g.ind_demand[ind] = to_quad(sf(factor) * cast::<Ext, _>(g.ind_demand[ind]));
}

/// `ShockCommodityPrice name, pct` with a temporary for the BYREF percentage.
fn shock(g: &mut Globals, rng: &mut Rng, name: &str, pct: i32) {
    shock_commodity_price(g, rng, name.as_bytes(), pct);
}

/// `x \ y` (PB_IDiv): both operands rounded, the quotient truncated.
fn idiv(x: Ext, y: Ext) -> Ext {
    (crate::pb::numeric::round_half_even(x) / crate::pb::numeric::round_half_even(y)).trunc()
}

/// Port of UpdateEconomy_52978d (0x52978d), the quarterly macro step.
///
/// Resets `EconCycleCounter` and `MonetaryStance`; during a pandemic (`PandemicState` not in -1..0) only
/// runs `PandemicUpdate`. Otherwise: housing starts from `GdpLevel` and `BaseInterestRate`; a random
/// world event (`RandomWorldEvents`, whose number is also passed to `MonetaryPolicyScenario`); new
/// growth forecasts for industries 3..69 (GOSUB `UpdateEconomy_IndGrowthForecasts` plus a random swing
/// by `IndVolatility`); the macro effects of the event (interest-rate targets, GDP targets, market
/// valuation, industry demand/growth, commodity shocks, futures basis, market-wide re-pricing); clamps
/// of the industry forecasts / growth rates / long-term growth; `TechBubbleCycle` (bubble state >= 100);
/// a re-valuation of every active entity (`ValuationMode` 1, or 2 above difficulty 1) with stock splits
/// for unmanaged high-priced stocks and share-count fixes for tiny caps; `PeakOilCrisisUpdate` during
/// an oil crisis; a stance-driven nudge of `GdpGrowthTarget`; early-game interest-rate cap; the
/// `PrimeRate = 3` reset of `BaseInterestRate`.
///
/// `time_secs` is passed on to `CalcStockPrice`. The ON ERROR handler only shows ShowRuntimeError.
pub fn update_economy_52978d(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    update_economy_52978d_with(g, ui, rng, time_secs, &mut random_world_events);
}

/// [`update_economy_52978d`] with the `RandomWorldEvents` call supplied by the caller (for tests that
/// run the original event generator).
#[doc(hidden)]
pub fn update_economy_52978d_with(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    time_secs: i32,
    world_events: &mut dyn FnMut(&mut Globals, &mut dyn Ui, &mut Rng, i32) -> i32,
) {
    g.econ_cycle_counter = 0;
    if g.market_valuation == Ext::from_f32(0.0f32) {
        g.market_valuation = cast::<f32, _>(Ext::from_f32(1.0f32));
    }
    if g.market_val_target == Ext::from_f64(0.0) {
        g.market_val_target = cast::<f64, _>(cast::<Ext, _>(g.market_valuation));
    }
    g.monetary_stance = 0;
    if g.pandemic_state < -1 || 0 < g.pandemic_state {
        pandemic_update(g, ui, rng, time_secs);
        return;
    }
    g.housing_starts = to_long(cast::<Ext, _>(g.gdp_level) * sf(0.01f32) + Ext::from_f64(1000.0));
    let tmp: Ext = crate::pb::numeric::double((Ext::from_f64(7.5) - cast::<Ext, _>(g.base_interest_rate)) * sf(0.1f32) + Ext::from_f64(1.0)); // DOUBLE local
    g.housing_starts = to_long(tmp * cast::<Ext, _>(g.housing_starts));
    if g.housing_starts < 500 {
        g.housing_starts = to_long(Ext::from_f64(250.0) * rng.rnd() + Ext::from_f64(400.0));
    }
    let event = world_events(g, ui, rng, time_secs);
    let mut ent_id: i32 = event; // BYREF to MonetaryPolicyScenario
    let sel_case = event;
    monetary_policy_scenario(g, ui, rng, ent_id);
    for industry in 3..=69i32 {
        ind_growth_forecasts(g, rng, industry);
        let range: f32 = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) + Ext::from_f64(2.0) * cast::<Ext, _>(g.ind_volatility[industry]))); // SINGLE local
        let r = rng.rnd_range(cast::<i32, _>(to_quad(cast::<Ext, _>(range))), 1);
        let swing = to_long(cast::<Ext, _>(r) - idiv(cast::<Ext, _>(range), Ext::from_f64(2.0)));
        g.ind_growth_forecast[industry] = cast::<i16, _>(swing + cast::<i32, _>(g.ind_growth_forecast[industry]));
        if (cast::<i32, _>(g.ind_growth_forecast[industry])) < -30 {
            g.ind_growth_forecast[industry] = -30;
            g.ind_growth_forecast[industry] = cast::<i16, _>(swing + cast::<i32, _>(g.ind_growth_forecast[industry]));
        }
        if (cast::<i32, _>(g.ind_growth_forecast[industry])) < -5 {
            g.ind_growth_forecast[industry] = cast::<i16, _>(3 + cast::<i32, _>(g.ind_growth_forecast[industry]));
        }
        if 65 < cast::<i32, _>(g.ind_growth_forecast[industry]) {
            g.ind_growth_forecast[industry] = cast::<i16, _>(g.rand_seed % 25 + 40);
        }
        let d = cast::<Ext, _>(g.ind_growth_forecast[69]) - cast::<Ext, _>(g.ind_long_term_growth[69]);
        if Ext::from_f64(4.0) < d.abs() {
            g.ind_growth_forecast[69] = q16(
                (cast::<Ext, _>(g.ind_long_term_growth[69]) - cast::<Ext, _>(g.ind_growth_forecast[69])) * sf(0.5f32)
                    + cast::<Ext, _>(g.ind_growth_forecast[69]),
            );
        }
    }

    match sel_case {
        1 => {
            if Ext::from_f64(12.0) > cast::<Ext, _>(g.interest_rate_target) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) * rng.rnd() + cast::<Ext, _>(g.interest_rate_target)));
            } else {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.interest_rate_target)));
            }
            if Ext::from_f32(0.0f32) == g.oil_crisis_level {
                shock(g, rng, "OIL", 10);
            }
        }
        5 => {
            if 0 < g.monetary_policy {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(12.0) * rng.rnd() + sf(9.5f32)));
            } else {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) + cast::<Ext, _>(g.base_interest_rate)));
            }
            if sf(8.5f32) > cast::<Ext, _>(g.base_interest_rate) {
                g.base_interest_rate = cast::<f32, _>(8.6f32);
            } else {
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(2.0) + cast::<Ext, _>(g.base_interest_rate)) + Ext::from_f64(2.0) * rng.rnd()));
            }
            g.econ_cycle_counter = -5;
            if sf(0.85f32) > g.market_val_target {
                g.market_val_target = cast::<f64, _>(sf(0.7f32));
            } else {
                g.market_val_target = cast::<f64, _>(g.market_val_target * sf(0.92f32));
            }
            let mut odd_start = 1;
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                odd_start = 0;
            }
            ent_id = odd_start + 11;
            while ent_id <= 1598 {
                if g.entity_status[ent_id] == 1 {
                    g.valuation_mode = 2;
                    calc_stock_price(g, ui, rng, ent_id, time_secs);
                }
                ent_id += 2;
            }
        }
        6 => {
            if !(sf(0.8f32) >= cast::<Ext, _>(g.oil_price)) {
                shock(g, rng, "OIL", -20);
            }
            if g.rand_seed % 5 == 2 {
                shock(g, rng, "OIL", 10);
            }
            scale_demand(g, 14, 0.75f32);
            g.ind_growth_forecast[14] = q16(-Ext::from_f64(15.0) + Ext::from_f64(4.0) * rng.rnd());
            g.ind_growth_rate[14] = q16(sf(0.5f32) * cast::<Ext, _>(g.ind_growth_forecast[14]));
            g.market_val_target = cast::<f64, _>(sf(1.3f32));
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.1f32) * cast::<Ext, _>(g.market_valuation)));
        }
        10 => {
            for ind in [9, 14, 50] {
                if (cast::<i32, _>(g.ind_growth_forecast[ind])) < 30 {
                    g.ind_growth_forecast[ind] = q16(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(25.0));
                }
            }
            g.ind_long_term_growth[9] = q16(Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(15.0));
            g.ind_long_term_growth[14] = q16(Ext::from_f64(12.0) * rng.rnd() + Ext::from_f64(10.0));
            g.ind_long_term_growth[50] = q16(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(12.0));
            scale_demand(g, 9, 1.3f32);
            scale_demand(g, 14, 1.25f32);
            scale_demand(g, 50, 1.35f32);
            shock(g, rng, "OIL", -14);
        }
        11 => {
            if Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth) {
                g.gdp_growth = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * rng.rnd() + Ext::from_f64(3.0)));
            }
            if !(Ext::from_f64(4.0) >= cast::<Ext, _>(g.gdp_growth)) {
                g.gdp_growth_target = cast::<f32, _>(3.6f32);
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.interest_rate_target)));
            }
        }
        19 | 21 => {
            g.market_val_target = cast::<f64, _>(sf(0.6f32));
            g.futures_basis[6] = cast::<i16, _>(cast::<i32, _>(g.futures_basis[6]) - 150);
            ent_id = 11;
            while ent_id <= 1600 {
                if g.entity_status[ent_id] == 1 {
                    if g.industry_num[ent_id] == 71 {
                        calc_stock_price(g, ui, rng, ent_id, time_secs);
                    } else {
                        g.market_cap[ent_id] = sf(0.92f32) * g.market_cap[ent_id];
                        update_price_history_tick(g, ui, rng, ent_id);
                    }
                }
                ent_id += 1;
            }
        }
        20 => {
            g.econ_cycle_counter = -7;
            g.ind_growth_forecast[4] = q16(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(20.0));
            let base = Ext::from_f64(6.0) + cast::<Ext, _>(g.ind_long_term_growth[4]);
            g.ind_long_term_growth[4] = q16(base + Ext::from_f64(9.0) * rng.rnd());
            g.ind_growth_forecast[15] = 40;
            scale_demand(g, 15, 1.15f32);
            g.ind_growth_forecast[21] = 28;
            let base = Ext::from_f64(4.0) + cast::<Ext, _>(g.ind_long_term_growth[21]);
            g.ind_long_term_growth[21] = q16(base + Ext::from_f64(9.0) * rng.rnd());
        }
        22 => {
            g.gdp_growth_target = cast::<f32, _>(-4.8f32);
            if g.rand_seed % 3 == 1 {
                g.gdp_growth_target = cast::<f32, _>(-9.4f32);
            }
            if g.rand_seed % 3 == 2 {
                g.gdp_growth_target = cast::<f32, _>(1.1f32);
            }
            g.market_val_target = cast::<f64, _>(sf(0.75f32));
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.7f32) * cast::<Ext, _>(g.market_valuation)));
            g.econ_cycle_counter = -7;
            g.interest_rate_target = cast::<f32, _>(Ext::from_f32(7.0f32));
        }
        25 => {
            if (cast::<i32, _>(g.ind_growth_forecast[14])) < 30 {
                g.ind_growth_forecast[14] = q16(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(25.0));
            }
            g.ind_growth_rate[14] = q16(cast::<Ext, _>(g.ind_growth_forecast[14]) * sf(0.7f32));
            scale_demand(g, 14, 1.3f32);
            if sf(1.6f32) > cast::<Ext, _>(g.oil_price) {
                shock(g, rng, "OIL", 22);
            }
            g.futures_basis[1] = cast::<i16, _>(75 + cast::<i32, _>(g.futures_basis[1]));
            g.market_val_target = cast::<f64, _>(g.market_val_target - sf(0.2f32));
            // MarketValuation = (MarketValuation = 0.7): -1 or 0
            g.market_valuation = cast::<f32, _>(if cast::<Ext, _>(g.market_valuation) == sf(0.7f32) { -Ext::from_f32(1.0f32) } else { Ext::from_f32(0.0f32) });
        }
        26 => {
            scale_demand(g, 7, 1.2f32);
            scale_demand(g, 8, 1.25f32);
            g.futures_basis[1] = cast::<i16, _>(cast::<i32, _>(g.futures_basis[1]) - 75);
            g.ind_growth_forecast[7] = q16(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(45.0));
            g.ind_growth_forecast[8] = q16(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(45.0));
            shock(g, rng, "OIL", 25);
            if g.rand_seed % 5 == 1 {
                shock(g, rng, "OIL", -10);
            }
            g.econ_cycle_counter = -5;
            for industry in 3..=68i32 {
                if matches!(industry, 3 | 11 | 16 | 18 | 23 | 25 | 52 | 67 | 68) {
                    scale_demand(g, industry, 0.9f32);
                    g.ind_growth_forecast[industry] = q16(-Ext::from_f64(19.0) + Ext::from_f64(8.0) * rng.rnd());
                    g.ind_growth_rate[industry] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_rate[industry]) - 7);
                }
            }
            g.market_val_target = cast::<f64, _>(sf(0.85f32));
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.8f32) * cast::<Ext, _>(g.market_valuation)));
        }
        27 => {
            if !(Ext::from_f64(3.0) >= cast::<Ext, _>(g.gdp_growth)) {
                g.gdp_growth = cast::<f32, _>(cast::<f32, _>(sf(0.4f32) * rng.rnd() + sf(2.5f32)));
            }
        }
        31 => {
            g.econ_cycle_counter = -6;
            if Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(6.0) > cast::<Ext, _>(g.interest_rate_target) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.interest_rate_target)));
            }
            if sf(0.9f32) > cast::<Ext, _>(g.oil_price) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(1.0) + cast::<Ext, _>(g.interest_rate_target)) + Ext::from_f64(3.0) * rng.rnd()));
            }
            shock(g, rng, "OIL", -16);
            g.market_val_target = cast::<f64, _>(sf(0.85f32));
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.9f32) * cast::<Ext, _>(g.market_valuation)));
        }
        46 => {
            if sf(1.7f32) > cast::<Ext, _>(g.gdp_growth) {
                g.gdp_growth = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * rng.rnd() + sf(1.8f32)));
            }
        }
        56 => {
            if !(Ext::from_f64(8.0) >= cast::<Ext, _>(g.base_interest_rate)) {
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.base_interest_rate) - sf(1.4f32)));
            }
            g.econ_cycle_counter = -6;
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(1.5f32) * rng.rnd() + sf(3.75f32)));
            if g.housing_starts < 1250 {
                g.housing_starts = to_long(Ext::from_f64(250.0) * rng.rnd() + Ext::from_f64(1300.0));
            }
            scale_demand(g, 50, 0.8f32);
            scale_demand(g, 15, 0.9f32);
            g.ind_growth_forecast[50] = q16(-Ext::from_f64(25.0) + Ext::from_f64(15.0) * rng.rnd());
            g.ind_growth_forecast[15] = q16(-Ext::from_f64(15.0) + Ext::from_f64(8.0) * rng.rnd());
            g.market_val_target = cast::<f64, _>(sf(0.5f32));
            g.market_valuation = cast::<f32, _>(Ext::from_f32(0.5f32));
            g.monetary_policy = 2;
            shock(g, rng, "WHEAT", -20);
            shock(g, rng, "OIL", -20);
            shock(g, rng, "GOLD", -20);
        }
        59 => {
            g.ind_growth_forecast[7] = q16(-Ext::from_f64(25.0) + Ext::from_f64(10.0) * rng.rnd());
            g.ind_growth_forecast[8] = q16(-Ext::from_f64(25.0) + Ext::from_f64(10.0) * rng.rnd());
            g.ind_long_term_growth[7] = q16(-Ext::from_f64(10.0) - Ext::from_f64(8.0) * rng.rnd());
            g.ind_long_term_growth[8] = q16(-Ext::from_f64(10.0) - Ext::from_f64(10.0) * rng.rnd());
            scale_demand(g, 7, 0.9f32);
            scale_demand(g, 8, 0.85f32);
            g.futures_basis[1] = cast::<i16, _>(120 + cast::<i32, _>(g.futures_basis[1]));
            g.ind_long_term_growth[11] = cast::<i16, _>(5 + cast::<i32, _>(g.ind_long_term_growth[11]));
            scale_demand(g, 3, 1.2f32);
            for ind in [11, 16, 23, 25, 67, 68] {
                scale_demand(g, ind, 1.25f32);
            }
            g.market_val_target = cast::<f64, _>(sf(1.25f32));
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.1f32) * cast::<Ext, _>(g.market_valuation)));
        }
        61 | 62 => {
            shock(g, rng, "OIL", -30);
            g.futures_basis[1] = -75;
            scale_demand(g, 7, 0.85f32);
            scale_demand(g, 8, 0.9f32);
            g.ind_growth_forecast[7] = -10;
            g.ind_growth_forecast[8] = -12;
            g.ind_growth_forecast[50] = -5;
            g.econ_cycle_counter = -3;
            for (ind, add) in [(3, 7), (11, 6), (16, 10), (18, 1), (23, 10), (25, 12), (67, 8), (68, 6)] {
                g.ind_growth_forecast[ind] = cast::<i16, _>(add + cast::<i32, _>(g.ind_growth_forecast[ind]));
            }
        }
        63 => {
            g.market_val_target = cast::<f64, _>(sf(0.25f32) + g.market_val_target);
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.07f32) * cast::<Ext, _>(g.market_valuation)));
            if Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) * rng.rnd() + sf(3.5f32)));
            }
        }
        68 => {
            g.market_val_target = cast::<f64, _>(sf(0.15f32) * rng.rnd() + sf(0.8f32));
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.95f32) * cast::<Ext, _>(g.market_valuation)));
            if !(Ext::from_f64(6.0) >= cast::<Ext, _>(g.interest_rate_target)) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.interest_rate_target) - Ext::from_f64(1.0)));
            }
            if !(Ext::from_f64(3.0) >= cast::<Ext, _>(g.gdp_growth_target)) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) * rng.rnd() + sf(0.5f32)));
            } else if !(-Ext::from_f64(5.0) >= cast::<Ext, _>(g.gdp_growth_target)) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(1.0)));
            }
        }
        70 => {
            scale_demand(g, 67, 0.9f32);
            g.ind_growth_forecast[67] = -22;
            g.futures_basis[1] = cast::<i16, _>(100 + cast::<i32, _>(g.futures_basis[1]));
        }
        71 => {
            g.market_val_target = cast::<f64, _>(sf(1.5f32));
            g.market_valuation = cast::<f32, _>(1.3f32);
            g.econ_cycle_counter = -5;
            ent_id = 131;
            while ent_id <= 1580 {
                if g.entity_status[ent_id] == 1 {
                    let f = sf(0.06f32) * rng.rnd() + sf(1.04f32);
                    g.market_cap[ent_id] = g.market_cap[ent_id] * f;
                }
                ent_id += 2;
            }
        }
        75 => {
            for ind in [5, 6] {
                if (cast::<i32, _>(g.ind_growth_forecast[ind])) < 22 {
                    g.ind_growth_forecast[ind] = q16(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(20.0));
                }
            }
            g.ind_growth_rate[5] = q16(cast::<Ext, _>(g.ind_growth_forecast[5]) * sf(0.7f32));
            g.ind_growth_rate[6] = q16(cast::<Ext, _>(g.ind_growth_forecast[6]) * sf(0.7f32));
            g.ind_long_term_growth[5] = cast::<i16, _>(5 + cast::<i32, _>(g.ind_long_term_growth[5]));
            g.ind_long_term_growth[6] = cast::<i16, _>(5 + cast::<i32, _>(g.ind_long_term_growth[6]));
        }
        76 => {
            g.ind_growth_rate[4] = q16(-Ext::from_f64(25.0) + Ext::from_f64(9.0) * rng.rnd());
            g.ind_long_term_growth[4] = q16(Ext::from_f64(3.0) * rng.rnd() - Ext::from_f64(5.0));
            g.ind_growth_rate[21] = q16(-Ext::from_f64(10.0) * rng.rnd());
            g.ind_long_term_growth[21] = -3;
            g.econ_cycle_counter = -10;
        }
        77 => {
            if Ext::from_f64(1.0) > cast::<Ext, _>(g.gdp_growth) {
                g.gdp_growth = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth)));
            }
            if !(-sf(1.2f32) >= cast::<Ext, _>(g.gdp_growth_target)) {
                g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(1.5f32));
            }
            if !(sf(0.5f32) >= cast::<Ext, _>(g.oil_target_price)) {
                shock(g, rng, "OIL", -15);
            }
            g.market_val_target = cast::<f64, _>(sf(0.7f32));
            g.market_valuation = cast::<f32, _>(0.8f32);
        }
        78 => {
            if !(sf(0.9f32) >= g.market_val_target) {
                g.market_val_target = cast::<f64, _>(sf(0.8f32) * g.market_val_target);
            }
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.7f32) * cast::<Ext, _>(g.market_valuation)));
            g.futures_basis[6] = cast::<i16, _>(cast::<i32, _>(g.futures_basis[6]) - 180);
        }
        80 => {
            if Ext::from_f64(9.0) > cast::<Ext, _>(g.base_interest_rate) {
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(sf(1.15f32) * cast::<Ext, _>(g.base_interest_rate)));
            }
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(12.0) * rng.rnd() + Ext::from_f64(13.0)));
            g.econ_cycle_counter = -6;
            if 750 < g.housing_starts {
                g.housing_starts = to_long(Ext::from_f64(250.0) * rng.rnd() + Ext::from_f64(300.0));
            }
            scale_demand(g, 50, 1.3f32);
            g.ind_growth_forecast[50] = q16(Ext::from_f64(15.0) * rng.rnd() + Ext::from_f64(40.0));
            g.market_val_target = cast::<f64, _>(sf(0.6f32));
            g.market_valuation = cast::<f32, _>(Ext::from_f32(0.5f32));
            g.monetary_policy = 2;
        }
        83 => {
            for industry in 10..=40i32 {
                if matches!(industry, 10 | 13 | 24 | 26 | 35 | 40) {
                    if (cast::<i32, _>(g.ind_growth_forecast[industry])) < 25 {
                        g.ind_growth_forecast[industry] = q16(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(24.0));
                    }
                    g.ind_long_term_growth[industry] = cast::<i16, _>(3 + cast::<i32, _>(g.ind_long_term_growth[industry]));
                    scale_demand(g, industry, 1.12f32);
                }
            }
        }
        87 => {
            g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(9.0f32));
            g.econ_cycle_counter = -11;
            shock(g, rng, "OIL", -30);
        }
        88 => {
            g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(20.0f32));
            if 0 < g.monetary_policy {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(21.0f32));
            } else {
                shock(g, rng, "OIL", 50);
            }
            g.market_val_target = cast::<f64, _>(sf(0.7f32));
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * cast::<Ext, _>(g.market_valuation)));
            g.econ_cycle_counter = -13;
            for ind in 3..=69i32 {
                if ind == 7 || ind == 8 || ind == 67 || ind == 68 {
                    continue;
                }
                if ind == 23 || ind == 50 {
                    scale_demand(g, ind, 1.12f32);
                } else {
                    scale_demand(g, ind, 0.73f32);
                }
            }
        }
        90 => {
            shock(g, rng, "OIL", 23);
            g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(12.0f32));
            if check_index_hist_low(g) == 1 {
                g.gdp_growth_target = cast::<f32, _>(-4.8f32);
            }
            if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
                if !(-Ext::from_f64(3.0) >= cast::<Ext, _>(g.gdp_growth_target)) {
                    g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(5.0)));
                }
                if !(Ext::from_f64(0.0) >= cast::<Ext, _>(g.gdp_growth)) {
                    g.gdp_growth = cast::<f32, _>(cast::<f32, _>(sf(0.2f32) * cast::<Ext, _>(g.gdp_growth)));
                }
            }
            let base = update_metals_price_base(g, ui);
            if base < Ext::from_f64(3100.0) || cast::<Ext, _>(g.silver_price) - cast::<Ext, _>(g.metals_price_base) * sf(0.01f32) < Ext::from_f64(33.0) {
                shock(g, rng, "GOLD", 40);
            }
            if Ext::from_f64(11.0) > cast::<Ext, _>(g.corn_price) {
                shock(g, rng, "CORN", 22);
            }
            if sf(1.4f32) > cast::<Ext, _>(g.oil_price) {
                shock(g, rng, "OIL", 20);
            }
            for industry in 3..=69i32 {
                if industry == 7 || industry == 8 || industry == 50 {
                    continue;
                }
                g.ind_long_term_growth[industry] = q16(cast::<Ext, _>(g.ind_long_term_growth[industry]) * sf(0.75f32));
                g.ind_growth_rate_now[industry] = q16(cast::<Ext, _>(g.ind_growth_rate_now[industry]) * sf(0.8f32));
                if (cast::<i32, _>(g.ind_growth_rate_now[industry])) < 2 {
                    g.ind_growth_rate_now[industry] = 2;
                }
                if industry == 31 {
                    g.ind_long_term_growth[industry] = 5;
                }
            }
            g.interest_rate_target = cast::<f32, _>(4.8f32);
        }
        92 => {
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(17.0)));
            if Ext::from_f64(10.0) > cast::<Ext, _>(g.base_interest_rate) {
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(3.0) + cast::<Ext, _>(g.base_interest_rate)) + Ext::from_f64(2.0) * rng.rnd()));
            } else {
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) + cast::<Ext, _>(g.base_interest_rate)));
            }
            g.econ_cycle_counter = -12;
            if 750 < g.housing_starts {
                g.housing_starts = to_long(Ext::from_f64(250.0) * rng.rnd() + Ext::from_f64(300.0));
            }
            scale_demand(g, 50, 1.25f32);
            g.ind_growth_forecast[50] = q16(Ext::from_f64(15.0) * rng.rnd() + Ext::from_f64(40.0));
            g.monetary_policy = 2;
            shock(g, rng, "GOLD", 50);
            g.futures_basis[2] = cast::<i16, _>(150 + cast::<i32, _>(g.futures_basis[2]));
            g.futures_basis[3] = cast::<i16, _>(150 + cast::<i32, _>(g.futures_basis[3]));
        }
        _ => {}
    }

    for industry in 3..=69i32 {
        if (cast::<i32, _>(g.ind_growth_forecast[industry])) < -30 {
            g.ind_growth_forecast[industry] = -30;
            g.ind_growth_forecast[industry] = q16(Ext::from_f64(15.0) * rng.rnd() + cast::<Ext, _>(g.ind_growth_forecast[industry]));
        }
        if (cast::<i32, _>(g.ind_growth_forecast[industry])) < -5 {
            g.ind_growth_forecast[industry] = cast::<i16, _>(3 + cast::<i32, _>(g.ind_growth_forecast[industry]));
        }
        if 65 < cast::<i32, _>(g.ind_growth_forecast[industry]) {
            g.ind_growth_forecast[industry] = cast::<i16, _>(g.rand_seed % 25 + 40);
        }
        if (cast::<i32, _>(g.ind_growth_rate[industry])) < -30 {
            g.ind_growth_rate[industry] = q16(-Ext::from_f64(30.0) + Ext::from_f64(5.0) * rng.rnd());
        }
        if 65 < cast::<i32, _>(g.ind_growth_rate[industry]) {
            g.ind_growth_rate[industry] = q16(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(55.0));
        }
        if (cast::<i32, _>(g.ind_long_term_growth[industry])) < -1 {
            g.ind_long_term_growth[industry] = g.ind_long_term_growth[industry].wrapping_add(1);
        }
        if 25 < cast::<i32, _>(g.ind_long_term_growth[industry]) {
            g.ind_long_term_growth[industry] = g.ind_long_term_growth[industry].wrapping_sub(1);
        }
        let diff = cast::<i32, _>(g.ind_growth_forecast[industry]) - cast::<i32, _>(g.ind_long_term_growth[industry]);
        if (cast::<Ext, _>(diff)).abs() >= Ext::from_f64(21.0) {
            if diff > 0 {
                g.ind_long_term_growth[industry] = g.ind_long_term_growth[industry].wrapping_sub(1);
            } else {
                g.ind_long_term_growth[industry] = g.ind_long_term_growth[industry].wrapping_add(1);
            }
        }
    }
    if 100 <= g.bubble_state {
        tech_bubble_cycle(g, ui, rng);
    }
    ent_id = 11;
    while ent_id <= 1600 {
        if g.entity_status[ent_id] == 1 {
            g.valuation_mode = 1;
            if 1 < g.difficulty_level {
                g.valuation_mode = 2;
            }
            calc_stock_price(g, ui, rng, ent_id, time_secs);
            if !(g.tmp_share_price <= Ext::from_f64(225.0)) && get_managing_player(g, ent_id) == 0 {
                check_stock_split(g, ent_id);
            }
            if get_managing_player(g, ent_id) <= 0 {
                let limit = cast::<Ext, _>(g.rand_seed % 3) + Ext::from_f64(7.0);
                if g.market_cap[ent_id] < limit {
                    if g.tmp_share_price < Ext::from_f64(5.0) {
                        g.shares_outstanding[ent_id] = sf(0.5f32) * g.shares_outstanding[ent_id];
                    }
                    if !(g.tmp_share_price <= Ext::from_f64(8.0) * rng.rnd() + Ext::from_f64(20.0)) {
                        g.shares_outstanding[ent_id] = Ext::from_f64(2.0) * g.shares_outstanding[ent_id];
                    }
                }
            }
        }
        ent_id += 1;
    }
    if g.oil_crisis_level != Ext::from_f32(0.0f32) {
        peak_oil_crisis_update(g, ui, rng);
    }
    if g.oil_crisis_level == Ext::from_f32(0.0f32)
        && g.subprime_crisis_state != -2
        && g.subprime_crisis_state <= 0
        && -2 < g.pandemic_state
        && g.pandemic_state < 1
    {
        if g.monetary_stance == -1 {
            if Ext::from_f64(2.0) > cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth_target)));
            }
        } else if g.monetary_stance == 1 && !(Ext::from_f64(3.0) >= cast::<Ext, _>(g.gdp_growth_target)) {
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(1.0)));
        }
    }
    if g.cur_year < 2 + g.start_year && !(sf(7.5f32) >= cast::<Ext, _>(g.interest_rate_target)) {
        if !(Ext::from_f64(3.0) * rng.rnd() <= Ext::from_f64(1.0)) {
            g.interest_rate_target = cast::<f32, _>(Ext::from_f32(7.0f32));
        }
    }
    if Ext::from_f64(3.0) == cast::<Ext, _>(g.prime_rate) {
        if g.rand_seed % 2 == 1 {
            g.base_interest_rate = cast::<f32, _>(4.2f32);
        } else {
            g.base_interest_rate = cast::<f32, _>(1.4f32);
        }
    }
}

/// GOSUB UpdateEconomy_IndGrowthForecasts (0x52cb42) of UpdateEconomy_52978d: resets
/// `IndGrowthForecast(industry)` to the long-term growth and adds the oil-price, interest-rate and
/// housing effects by industry group and a GDP effect (sensitivity by industry × `IndVolatility` ×
/// (`GdpGrowth` - 3), damped above 25). Skipped for tech-bubble industries while a bubble runs.
fn ind_growth_forecasts(g: &mut Globals, rng: &mut Rng, industry: i32) {
    let mut do_update = 1;
    if 100 <= g.bubble_state {
        if 200 <= g.bubble_state && g.bubble_state < 212 && (industry == 4 || industry == 21) {
            do_update = 0;
        }
        if g.bubble_state < 112 && matches!(industry, 10 | 24 | 26 | 30 | 35 | 37 | 39 | 40 | 61) {
            do_update = 0;
        }
    }
    if do_update == 0 {
        return;
    }
    g.ind_growth_forecast[industry] = g.ind_long_term_growth[industry];
    let mut growth_adj: f32 = cast::<f32, _>(Ext::from_f32(0.0f32)); // SINGLE local
    let oil = cast::<Ext, _>(g.oil_price);
    match industry {
        3 | 11 | 15 | 16 | 18 | 20 | 23 | 25 | 52 | 53 | 59 | 67 | 68 => {
            let metric = to_long(cast::<Ext, _>(g.ind_volatility[industry]) * rng.rnd());
            if !(sf(1.2f32) >= oil) {
                growth_adj = cast::<f32, _>(-(cast::<Ext, _>(metric)) - Ext::from_f64(5.0));
            }
            if sf(0.8f32) > oil {
                growth_adj = cast::<f32, _>(Ext::from_f64(5.0) + cast::<Ext, _>(metric));
            }
        }
        7 | 8 | 50 => {
            growth_adj = cast::<f32, _>((Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(30.0)) * (oil - Ext::from_f64(1.0)));
            if !(sf(1.2f32) >= oil) {
                growth_adj = cast::<f32, _>(Ext::from_f64(5.0) + cast::<Ext, _>(growth_adj));
            }
            if sf(0.9f32) > oil {
                growth_adj = cast::<f32, _>(cast::<Ext, _>(growth_adj) - Ext::from_f64(5.0));
            }
            if industry == 8 {
                if !(sf(1.1f32) >= oil) {
                    let a = cast::<Ext, _>(growth_adj) + cast::<Ext, _>(g.ind_growth_forecast[8]);
                    let b = cast::<Ext, _>(g.ind_growth_forecast[7]) - Ext::from_f64(5.0);
                    if !(b <= a) {
                        growth_adj = cast::<f32, _>(Ext::from_f64(9.0) + cast::<Ext, _>(growth_adj));
                    }
                }
                if sf(0.9f32) > oil {
                    let a = cast::<Ext, _>(growth_adj) + cast::<Ext, _>(g.ind_growth_forecast[8]);
                    let b = Ext::from_f64(5.0) + cast::<Ext, _>(g.ind_growth_forecast[7]);
                    if !(b >= a) {
                        growth_adj = cast::<f32, _>(cast::<Ext, _>(growth_adj) - Ext::from_f64(9.0));
                    }
                }
            }
            if industry == 50 && !(sf(1.2f32) >= oil) {
                shock_commodity_price(g, rng, b"GOLD", 26);
            }
        }
        5 | 6 | 37 | 60 | 69 => {
            growth_adj = cast::<f32, _>((sf(7.5f32) - cast::<Ext, _>(g.base_interest_rate)) * (cast::<Ext, _>(g.ind_volatility[industry]) * sf(1.5f32)));
            if industry == 5 || industry == 6 {
                if g.housing_starts < 800 {
                    growth_adj = cast::<f32, _>(cast::<Ext, _>(growth_adj) - Ext::from_f64(3.0));
                }
                if 1300 < g.housing_starts {
                    growth_adj = cast::<f32, _>(Ext::from_f64(3.0) + cast::<Ext, _>(growth_adj));
                }
            }
        }
        12 => {
            let metric = cast::<i32, _>(g.ind_growth_rate[67]) + cast::<i32, _>(g.ind_growth_forecast[3]) + cast::<i32, _>(g.ind_growth_rate[68]);
            if 18 < metric {
                growth_adj = cast::<f32, _>(Ext::from_f64(3.0) + cast::<Ext, _>(growth_adj));
            }
            if metric < 3 {
                growth_adj = cast::<f32, _>(cast::<Ext, _>(growth_adj) - Ext::from_f64(3.0));
            }
        }
        _ => {}
    }
    g.ind_growth_forecast[industry] = q16(cast::<Ext, _>(growth_adj) + cast::<Ext, _>(g.ind_growth_forecast[industry]));
    let gdp_sensitivity: Ext = match industry {
        3 | 5 | 6 | 9..=13 | 16 | 18 | 20 | 23..=25 | 35..=49 | 52..=54 | 58..=60 | 63 | 67 | 68 => {
            if industry == 37 {
                sf(1.2f32)
            } else {
                sf(0.7f32)
            }
        }
        7 | 8 | 15 | 26 | 33 | 51 | 61 => sf(0.4f32),
        4 | 14 | 19 | 27 | 29 | 30 | 55 | 57 | 64 | 65 | 66 | 69 => sf(0.2f32),
        17 | 21 | 22 | 28 | 31 | 32 | 34 | 56 => Ext::from_f64(0.0),
        50 | 62 => -sf(0.5f32),
        _ => Ext::from_f64(0.0),
    };
    let mut growth_adj: f32 =
        cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(g.ind_volatility[industry]) * gdp_sensitivity) * (cast::<Ext, _>(g.gdp_growth) - Ext::from_f64(3.0))));
    if !((cast::<Ext, _>(growth_adj)).abs() <= Ext::from_f64(25.0)) {
        growth_adj = cast::<f32, _>(sf(0.6f32) * cast::<Ext, _>(growth_adj));
    }
    g.ind_growth_forecast[industry] = q16(cast::<Ext, _>(growth_adj) + cast::<Ext, _>(g.ind_growth_forecast[industry]));
}

/// Port of UpdateIndustryLongTermGrowth (0x4a56ec).
/// Only when `SimCoCount = 999` and no tech bubble (`BubbleState < 100`): moves each industry's
/// `IndLongTermGrowth` one point toward `IndGrowthRateNow` when they differ by 3 or more; in the fourth
/// quarter of games older than 10 years, lowers `IndGrowthRateNow` by one when above `RND * 7 + 5`.
/// Keeps `IndLongTermGrowth(15)` at least 2.
pub fn update_industry_long_term_growth(g: &mut Globals, rng: &mut Rng) {
    if g.sim_co_count != 999 || 100 <= g.bubble_state {
        return;
    }
    for industry in 3..=69i32 {
        let diff = cast::<i32, _>(g.ind_growth_rate_now[industry]) - cast::<i32, _>(g.ind_long_term_growth[industry]);
        if diff > 2 {
            g.ind_long_term_growth[industry] = g.ind_long_term_growth[industry].wrapping_add(1);
        } else if diff < -2 {
            g.ind_long_term_growth[industry] = g.ind_long_term_growth[industry].wrapping_sub(1);
        }
        if 4 <= g.cur_quarter && g.cur_year - g.start_year > 10 {
            if Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(5.0) < cast::<Ext, _>(g.ind_growth_rate_now[industry]) {
                g.ind_growth_rate_now[industry] = g.ind_growth_rate_now[industry].wrapping_sub(1);
            }
        }
    }
    if (cast::<i32, _>(g.ind_long_term_growth[15])) < 2 {
        g.ind_long_term_growth[15] = 2;
    }
}

/// Port of AdjustIndLongTermGrowth (0x4f8f90).
/// Without a tech bubble, every 5 years from year 7 of the game (`(CurYear - StartYear) MOD 5 = 1`):
/// raises industry 15's long-term growth by one (while below the game's age and `RND * 6 + 10`);
/// for the other industries cuts a long-term growth above `RND * 7 + 8` to 80 %, and one below -5 to
/// 70 % plus one.
pub fn adjust_ind_long_term_growth(g: &mut Globals, rng: &mut Rng) {
    if 100 <= g.bubble_state {
        return;
    }
    let years = g.cur_year - g.start_year;
    if years < 7 || years % 5 != 1 {
        return;
    }
    for industry in 3..=69i32 {
        if industry == 15 {
            if (cast::<i32, _>(g.ind_long_term_growth[15])) < years {
                if !(Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(10.0) <= cast::<Ext, _>(g.ind_long_term_growth[15])) {
                    g.ind_long_term_growth[15] = g.ind_long_term_growth[15].wrapping_add(1);
                }
            }
        } else {
            if Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(8.0) < cast::<Ext, _>(g.ind_long_term_growth[industry]) {
                g.ind_long_term_growth[industry] = q16(sf(0.8f32) * cast::<Ext, _>(g.ind_long_term_growth[industry]));
            }
            if (cast::<i32, _>(g.ind_long_term_growth[industry])) < -5 {
                g.ind_long_term_growth[industry] = q16(sf(0.7f32) * cast::<Ext, _>(g.ind_long_term_growth[industry]));
                g.ind_long_term_growth[industry] = g.ind_long_term_growth[industry].wrapping_add(1);
            }
        }
    }
}

//! Industry demand and outlook: `UpdateIndustryOutlook` (0x6570e9) with `CalcIndustryDemandSwing`
//! (0x65b298), `CommodityDemandEffects` (0x4bce6a) and `IndustryFadNews` (0x4bc887).
//!
//! Ported from the machine code (the decompiled view swaps several branches). x87 expressions are
//! evaluated in f64; SINGLE locals/globals are rounded with `as f32` on every store; INTEGER globals
//! assigned from `fistp qword` keep the low 16 bits (`to_quad(x) as i16`).
use crate::pb::real::{Ext, cast};


use super::*;
use crate::logic::futures::update_metals_price_base;
use crate::logic::valuation::{
    calc_industry_avg_net_margin, calc_industry_outlook, price_pct_change_months, price_trend_direction,
};
use crate::pb::string::{lcase, mcase, CaseMode};

/// `(Silver - MetalsPriceBase * 0.0075) + (MetalsPriceBase * 0.25 + UpdateMetalsPriceBase()) / 100`
/// (the silver/gold indicator of UpdateIndustryOutlook; the base is read before the call).
fn metals_indicator(g: &mut Globals, ui: &mut dyn Ui) -> Ext {
    let x = cast::<Ext, _>(g.silver_price) - cast::<Ext, _>(g.metals_price_base) * sf(0.0075f32);
    let y = cast::<Ext, _>(g.metals_price_base) * sf(0.25f32);
    let ret = update_metals_price_base(g, ui);
    x + (y + ret) / Ext::from_f64(100.0)
}

/// `RND(hi, lo)` of the original (PB_RndInt).
fn rnd_int(rng: &mut Rng, hi: i32, lo: i32) -> i32 {
    rng.rnd_range(hi, lo)
}

/// Port of UpdateIndustryOutlook (0x6570e9).
/// The yearly-ish review of all operating industries 3..69: clamps `MarketValuation` to 0.7..1.3,
/// advances `IndDemandCycleIdx`, derives GDP / interest-rate / oil / grain / metals factors and, for
/// every industry (`CalcIndustryDemandSwing` first; skipped when `(CurQuarter + industry + 1) MOD 4 = 1`)
/// sets a new `IndGrowthForecast` from its sensitivities, volatility and linked industries, moves
/// `IndGrowthRate` one step towards it and applies commodity/rate-driven forecast shocks (gold mining,
/// 50, has its own rules). Posts a headline for the largest forecast revision (> 7 points; the rate
/// moves 3 points along) and, in games with more than 1400 companies, may boost one of five cyclical
/// industries (demand x1.15..1.22, forecast +20, rate +7; quick message if it is `SelIndustry`).
///
/// The ON ERROR handler only shows ShowRuntimeError (not reachable with valid industry tables).
pub fn update_industry_outlook(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // locals (zeroed at entry)
    let mut gdp_factor: Ext = Ext::from_f64(0.0);
    let mut rate_factor: Ext = Ext::from_f64(0.0);
    let mut oil_factor: Ext = Ext::from_f64(0.0);
    let mut grain_factor: Ext = Ext::from_f64(0.0);
    let mut metals_factor: Ext = Ext::from_f64(0.0);
    let mut price_tmp: Ext;
    let mut max_revision: i16 = 0;
    let mut max_rev_industry: i16 = 0;
    let mut industry_id: i16 = 0;

    if sf(0.7f32) > cast::<Ext, _>(g.market_valuation) {
        g.market_valuation = cast::<f32, _>(0.7f32);
        g.market_val_target = cast::<f64, _>(sf(0.58f32));
    }
    if !(sf(1.3f32) >= cast::<Ext, _>(g.market_valuation)) {
        g.market_valuation = cast::<f32, _>(1.3f32);
        g.market_val_target = cast::<f64, _>(sf(1.4f32));
    }
    if g.ind_demand_cycle_idx == 0 {
        g.ind_demand_cycle_idx = rnd_int(rng, 69, 3);
    }
    g.ind_demand_cycle_idx += 1;
    if g.ind_demand_cycle_idx > 69 {
        g.ind_demand_cycle_idx = 3;
    }
    if !(sf(3.3f32) >= cast::<Ext, _>(g.gdp_growth)) {
        gdp_factor = cast::<Ext, _>(g.gdp_growth) - sf(2.9f32);
    }
    if sf(1.5f32) > cast::<Ext, _>(g.gdp_growth) {
        gdp_factor = cast::<Ext, _>(g.gdp_growth) - sf(1.5f32);
    }
    if !(rng.rnd() * sf(1.2f32) + Ext::from_f64(5.0) >= cast::<Ext, _>(g.gdp_growth_target)) {
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(1.0)));
    }
    if -sf(21.5f32) > cast::<Ext, _>(g.gdp_growth_target) {
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(-Ext::from_f64(20.0) + sf(1.3f32)));
    }
    gdp_factor = Ext::from_f64(5.0) * gdp_factor;
    if -Ext::from_f64(45.0) > gdp_factor {
        gdp_factor = -Ext::from_f64(45.0) + Ext::from_f64(15.0) * rng.rnd();
    }
    if Ext::from_f64(8.0) < cast::<Ext, _>(g.base_interest_rate) || sf(5.75f32) > cast::<Ext, _>(g.base_interest_rate) {
        rate_factor = Ext::from_f64(7.0) - cast::<Ext, _>(g.base_interest_rate);
    }
    if Ext::from_f64(80.0) > cast::<Ext, _>(g.oil_price) {
        oil_factor = Ext::from_f64(85.0) - Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price);
    }
    if !(Ext::from_f64(120.0) >= cast::<Ext, _>(g.oil_price)) {
        oil_factor = Ext::from_f64(115.0) - Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price);
    }
    if !(Ext::from_f64(35.0) >= oil_factor) {
        oil_factor = Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(25.0);
    }
    if -Ext::from_f64(25.0) > oil_factor {
        oil_factor = -Ext::from_f64(25.0) + Ext::from_f64(5.0) * rng.rnd();
    }
    price_tmp = cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price);
    if !(Ext::from_f64(14.0) >= price_tmp) {
        grain_factor = price_tmp - Ext::from_f64(13.0);
    }
    if sf(8.3f32) > price_tmp {
        grain_factor = price_tmp - Ext::from_f64(11.0);
    }
    price_tmp = metals_indicator(g, ui);
    if !(Ext::from_f64(40.0) >= price_tmp) {
        metals_factor = Ext::from_f64(30.0) + price_tmp;
    }
    if Ext::from_f64(20.0) > price_tmp {
        metals_factor = price_tmp - Ext::from_f64(30.0);
    }

    for ind_loop in 3..=69i32 {
        calc_industry_demand_swing(g, rng, ind_loop);
        if (g.cur_quarter + ind_loop + 1) % 4 == 1 {
            continue;
        }
        industry_id = cast::<i16, _>(ind_loop);
        let i = ind_loop;
        let prev_forecast = g.ind_growth_forecast[i];
        if industry_id == 50 {
            price_tmp = update_metals_price_base(g, ui);
            if Ext::from_f64(950.0) - Ext::from_f64(100.0) * rng.rnd() > price_tmp {
                if prev_forecast > g.ind_long_term_growth[i] {
                    let r = cast::<Ext, _>(rnd_int(rng, 4, 1));
                    g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(cast::<Ext, _>(g.ind_growth_forecast[i]) * sf(0.6f32) - r));
                } else if prev_forecast > -5 {
                    let r = rnd_int(rng, 4, 1);
                    g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) - r);
                }
                if !(Ext::from_f64(10.0) >= g.ind_demand_gap[i]) {
                    g.ind_growth_forecast[i] = g.ind_growth_forecast[i].wrapping_sub(1);
                }
            } else if !(Ext::from_f64(300.0) * rng.rnd() + Ext::from_f64(1500.0) >= price_tmp) {
                if (cast::<i32, _>(prev_forecast)) < cast::<i32, _>(g.ind_long_term_growth[i]) + 3 {
                    let t = cast::<i32, _>(g.ind_growth_forecast[i]) + 4;
                    let r = rnd_int(rng, 4, 1);
                    g.ind_growth_forecast[i] = cast::<i16, _>(r + t);
                } else if !(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(15.0) >= cast::<Ext, _>(prev_forecast)) {
                    let f = rng.rnd() * sf(0.02f32) + Ext::from_f64(0.5);
                    g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(f * cast::<Ext, _>(g.ind_growth_forecast[i])));
                }
                if Ext::from_f64(15.0) > g.ind_demand_gap[i] {
                    g.ind_growth_forecast[i] = g.ind_growth_forecast[i].wrapping_add(1);
                }
            } else {
                g.ind_growth_forecast[i] =
                    cast::<i16, _>(to_quad(Ext::from_f64(0.5) * (cast::<Ext, _>(g.ind_long_term_growth[i]) + cast::<Ext, _>(prev_forecast))));
                let r = rnd_int(rng, 4, 0);
                g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) + 2 - r);
                if Ext::from_f64(12.0) > g.ind_demand_gap[i] {
                    g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) + 2);
                }
            }
            if g.ind_growth_rate[i] < -5 {
                g.ind_growth_rate[i] = -5;
            }
            if g.ind_growth_forecast[i] < -10 {
                g.ind_growth_forecast[i] = -8;
            }
            continue;
        }

        // --- sensitivity to GDP
        let mut sensitivity: Ext;
        sensitivity = match industry_id {
            4 | 17 | 21 | 22 | 28 | 31 | 32 | 34 | 46 | 55 | 56 => sf(0.07f32) * rng.rnd() + sf(0.05f32),
            14 | 19 | 27 | 29 | 30 | 43 | 64 | 65 | 66 | 69 => sf(0.1f32) * rng.rnd() + sf(0.15f32),
            7 | 8 | 15 | 26 | 41 | 61 | 63 => sf(0.2f32) * rng.rnd() + sf(0.3f32),
            3 | 5 | 6 | 9 | 12 | 13 | 16 | 18 | 20 | 23 | 25 | 33 | 35 | 36 | 42 | 47 | 52 | 59 | 60 | 67
            | 68 => sf(0.2f32) * rng.rnd() + sf(0.9f32),
            48 | 50 | 62 => {
                let mut s1 = -sf(0.1f32) - sf(0.15f32) * rng.rnd();
                if !(gdp_factor.abs() <= Ext::from_f64(15.0)) {
                    s1 = -sf(0.1f32);
                }
                if -sf(3.5f32) > cast::<Ext, _>(g.gdp_growth) || !(sf(5.5f32) >= cast::<Ext, _>(g.gdp_growth)) {
                    s1 = -s1;
                }
                s1
            }
            _ => sf(0.25f32) * rng.rnd() + Ext::from_f64(0.5),
        };
        let gdp_effect = cast::<f32, _>(sensitivity * gdp_factor);

        // --- sensitivity to interest rates
        sensitivity = match industry_id {
            5 | 6 => Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(8.0),
            60 => Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(5.0),
            50 => Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(2.0),
            69 => Ext::from_f64(1.0) * rng.rnd() + Ext::from_f64(1.0),
            _ => Ext::from_f64(0.5) * rng.rnd() + Ext::from_f64(0.5),
        };
        let mut rate_effect = cast::<f32, _>(sensitivity * rate_factor);
        if !(Ext::from_f64(50.0) >= cast::<Ext, _>(rate_effect)) {
            rate_effect = cast::<f32, _>(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(45.0));
        }
        if -Ext::from_f64(25.0) > cast::<Ext, _>(rate_effect) {
            rate_effect = cast::<f32, _>(-Ext::from_f64(25.0) + Ext::from_f64(15.0) * rng.rnd());
        }

        // --- sensitivity to oil (with direct oil effects on some industries)
        let oil = cast::<Ext, _>(g.oil_price);
        let gdp = cast::<Ext, _>(g.gdp_growth);
        sensitivity = match industry_id {
            7 | 8 => {
                let s1 = -sf(1.2f32) + sf(0.4f32) * rng.rnd();
                if sf(1.2f32) > oil && !(sf(0.8f32) >= oil) {
                    let v = oil * Ext::from_f64(10.0) - Ext::from_f64(10.0) + cast::<Ext, _>(g.ind_growth_forecast[i]);
                    g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(v));
                }
                if sf(0.88f32) > oil && Ext::from_f64(2.0) > gdp && g.ind_growth_forecast[i] > -1 {
                    g.ind_growth_forecast[i] = g.ind_growth_forecast[i].wrapping_sub(1);
                }
                if !(sf(1.32f32) >= oil) && !(-sf(9.5f32) >= gdp) && sf(5.2f32) > g.ind_demand_gap[i] {
                    g.ind_demand[i] = to_quad(sf(1.05f32) * cast::<Ext, _>(g.ind_demand[i]));
                    if g.ind_growth_rate[i] < 5 {
                        let t = cast::<i32, _>(g.ind_growth_rate[i]) + 2;
                        g.ind_growth_rate[i] = cast::<i16, _>(g.rand_seed % 5 + t);
                    }
                    if g.ind_growth_forecast[i] < 7 {
                        let t = cast::<i32, _>(g.ind_growth_forecast[i]) + 2;
                        g.ind_growth_forecast[i] = cast::<i16, _>(g.rand_seed % 5 + t);
                    }
                }
                s1
            }
            16 | 23 | 25 | 52 => {
                let s1 = sf(0.2f32) * rng.rnd() + sf(0.45f32);
                if sf(1.2f32) > oil && !(sf(0.8f32) >= oil) {
                    let v = cast::<Ext, _>(g.ind_growth_forecast[i]) - (oil * Ext::from_f64(10.0) - Ext::from_f64(10.0));
                    g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(v));
                }
                s1
            }
            3 | 51 | 53 | 59 | 67 | 68 => sf(0.2f32) * rng.rnd() + sf(0.4f32),
            9 | 11 | 12 => sf(0.3f32) * rng.rnd() + sf(0.3f32),
            15 | 18 | 20 | 50 => sf(0.2f32) * rng.rnd() + sf(0.25f32),
            5 | 6 | 33 | 57 | 69 => sf(0.15f32) * rng.rnd() + sf(0.1f32),
            63 => {
                let s1 = -sf(0.3f32) - sf(0.2f32) * rng.rnd();
                if !(sf(1.47f32) >= oil) && !(-sf(6.3f32) >= gdp) && sf(2.2f32) > g.ind_demand_gap[i] {
                    g.ind_demand[i] = to_quad(sf(1.03f32) * cast::<Ext, _>(g.ind_demand[i]));
                    if g.ind_growth_rate[i] < 4 {
                        let t = cast::<i32, _>(g.ind_growth_rate[i]) + 2;
                        g.ind_growth_rate[i] = cast::<i16, _>(g.rand_seed % 5 + t);
                    }
                    if g.ind_growth_forecast[i] < 6 {
                        let t = cast::<i32, _>(g.ind_growth_forecast[i]) + 2;
                        g.ind_growth_forecast[i] = cast::<i16, _>(g.rand_seed % 5 + t);
                    }
                }
                s1
            }
            _ => sf(0.07f32) * rng.rnd(),
        };
        let oil_effect = cast::<f32, _>(sensitivity * oil_factor);

        // --- sensitivity to grain prices
        sensitivity = match industry_id {
            15 => sf(0.6f32) * rng.rnd() + sf(1.7f32),
            28 => -sf(0.7f32) + sf(0.4f32) * rng.rnd(),
            57 => -sf(0.8f32) + sf(0.4f32) * rng.rnd(),
            _ => Ext::from_f64(0.0),
        };
        let mut grain_effect = cast::<f32, _>(sensitivity * grain_factor);
        if Ext::from_f64(8.0) > grain_factor && !(Ext::from_f64(0.0) >= cast::<Ext, _>(grain_effect)) {
            grain_effect = cast::<f32, _>(Ext::from_f64(2.0) + sf(2.5f32) * cast::<Ext, _>(grain_effect));
        }
        let metals_effect = if industry_id == 50 {
            cast::<f32, _>((sf(0.3f32) * rng.rnd() + sf(1.5f32)) * metals_factor)
        } else {
            0.0f32
        };

        let mut growth_target = cast::<f32, _>(cast::<Ext, _>(rate_effect)
            + cast::<Ext, _>(gdp_effect)
            + cast::<Ext, _>(oil_effect)
            + cast::<Ext, _>(grain_effect)
            + cast::<Ext, _>(metals_effect));
        growth_target = cast::<f32, _>(cast::<Ext, _>(g.ind_long_term_growth[i]) + cast::<Ext, _>(growth_target));
        if !(Ext::from_f64(50.0) >= cast::<Ext, _>(growth_target)) {
            growth_target = cast::<f32, _>(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(45.0));
        }
        if -Ext::from_f64(20.0) > cast::<Ext, _>(growth_target) {
            growth_target = cast::<f32, _>(-Ext::from_f64(25.0) + Ext::from_f64(5.0) * rng.rnd());
        }
        let mut new_growth = to_long(cast::<Ext, _>(growth_target));
        let vol_range = cast::<f32, _>(Ext::from_f64(5.0) + Ext::from_f64(2.0) * cast::<Ext, _>(g.ind_volatility[i]));
        let mut work = rnd_int(rng, cast::<i32, _>(to_quad(cast::<Ext, _>(vol_range))), 1);
        let half = (crate::pb::numeric::round_half_even(cast::<Ext, _>(vol_range)) / Ext::from_f64(2.0)).trunc();
        work = to_long(cast::<Ext, _>(work) - half);
        new_growth = new_growth.wrapping_add(work);
        if new_growth < -20 {
            new_growth = to_long(-Ext::from_f64(20.0) + Ext::from_f64(5.0) * rng.rnd());
        }
        if new_growth < -5 {
            new_growth = to_long(Ext::from_f64(3.0) * rng.rnd() + cast::<Ext, _>(new_growth));
        }
        if new_growth > 50 {
            new_growth = to_long(Ext::from_f64(11.0) * rng.rnd() + Ext::from_f64(40.0));
        }
        if industry_id == 69 && !((cast::<Ext, _>(new_growth) - cast::<Ext, _>(g.ind_long_term_growth[i])).abs() <= Ext::from_f64(4.0)) {
            let v = (cast::<Ext, _>(g.ind_long_term_growth[69]) - cast::<Ext, _>(new_growth)) * Ext::from_f64(0.5) + cast::<Ext, _>(new_growth);
            new_growth = to_long(v);
        }

        // --- linked industries
        let mut linked_adjust: i32 = 0;
        match industry_id {
            35 | 40 => {
                if g.ind_growth_rate[24] > 15 && g.ind_growth_forecast[24] > 10 && new_growth < 5 {
                    linked_adjust = 8;
                }
                if g.ind_growth_rate[24] < 5 && g.ind_growth_forecast[24] < 3 && new_growth > 13 {
                    linked_adjust = -8;
                }
            }
            12 => {
                let mut w = cast::<i32, _>(g.ind_growth_rate[67]) + cast::<i32, _>(g.ind_growth_rate[3]) + cast::<i32, _>(g.ind_growth_rate[68]);
                let forecast_sum = cast::<i32, _>(g.ind_growth_forecast[67])
                    + cast::<i32, _>(g.ind_growth_forecast[3])
                    + cast::<i32, _>(g.ind_growth_forecast[68]);
                w += forecast_sum;
                if w > 30 && !(Ext::from_f64(10.0) * rng.rnd() + -Ext::from_f64(7.0) <= cast::<Ext, _>(new_growth)) {
                    linked_adjust = 7;
                }
                if w < 10 && !(rng.rnd() + Ext::from_f64(4.0) + Ext::from_f64(4.0) >= cast::<Ext, _>(new_growth)) {
                    linked_adjust = -7;
                }
            }
            6 | 60 => {
                if g.ind_growth_rate[5] > 10 && g.ind_growth_forecast[5] > 5 && new_growth < 5 {
                    linked_adjust = 5;
                }
                if g.ind_growth_rate[5] < 0 && g.ind_growth_forecast[5] < 1 && new_growth > 9 {
                    linked_adjust = -5;
                }
                if industry_id == 60 {
                    linked_adjust = to_long(sf(0.6f32) * cast::<Ext, _>(linked_adjust));
                }
            }
            8 | 63 => {
                if g.ind_growth_rate[7] > 15 && g.ind_growth_forecast[7] > 8 && new_growth < 3 {
                    linked_adjust = 10;
                }
                if g.ind_growth_rate[7] < 4 && g.ind_growth_forecast[7] < 4 && new_growth > 10 {
                    linked_adjust = -10;
                }
                if industry_id == 63 {
                    linked_adjust = to_long(Ext::from_f64(0.5) * cast::<Ext, _>(linked_adjust));
                }
            }
            37 => {
                let mut w = price_trend_direction(g, 0, 5, 1);
                if w == 0 {
                    w = price_pct_change_months(g, 0, 6);
                    if !((cast::<Ext, _>(w)).abs() >= Ext::from_f64(10.0)) {
                        w = 0;
                    }
                }
                if w > 0 && !(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(5.0) <= cast::<Ext, _>(new_growth)) {
                    linked_adjust = 5;
                }
                if w < 0 && !(Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(15.0) >= cast::<Ext, _>(new_growth)) {
                    linked_adjust = -5;
                }
            }
            14 => {
                if cast::<i32, _>(g.ind_growth_rate[52]) + cast::<i32, _>(g.ind_growth_rate[16]) > 18
                    && cast::<i32, _>(g.ind_growth_forecast[52]) + cast::<i32, _>(g.ind_growth_forecast[16]) > 15
                {
                    linked_adjust = 6;
                }
                if (cast::<i32, _>(g.ind_growth_rate[52]) + cast::<i32, _>(g.ind_growth_rate[16])) < -2
                    && (cast::<i32, _>(g.ind_growth_forecast[52]) + cast::<i32, _>(g.ind_growth_forecast[16])) < 10
                {
                    linked_adjust = -8;
                }
            }
            45 => {
                if cast::<i32, _>(g.ind_growth_rate[58]) > new_growth + 5 {
                    linked_adjust = 5;
                }
                if (cast::<i32, _>(g.ind_growth_rate[58])) < new_growth - 5 {
                    linked_adjust = -5;
                }
            }
            _ => {}
        }
        new_growth = new_growth.wrapping_add(linked_adjust);
        if g.ind_volatility[i] < 4 {
            let ltg = cast::<i32, _>(g.ind_long_term_growth[i]);
            let vol = cast::<i32, _>(g.ind_volatility[i]);
            if new_growth - ltg > vol * 2 + 10 {
                let v = Ext::from_f64(7.0) + cast::<Ext, _>(ltg) + cast::<Ext, _>(vol * 2) + Ext::from_f64(6.0) * rng.rnd();
                new_growth = to_long(v);
            }
            if ltg - new_growth > vol * 2 + 5 {
                let v = cast::<Ext, _>(ltg) - (cast::<Ext, _>(vol * 2) + Ext::from_f64(5.0) + Ext::from_f64(6.0) * rng.rnd());
                new_growth = to_long(v);
            }
        }
        g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(Ext::from_f64(0.5) * (cast::<Ext, _>(g.ind_long_term_growth[i]) + cast::<Ext, _>(new_growth))));
        if g.ind_growth_forecast[i] > g.ind_growth_rate[i] {
            g.ind_growth_rate[i] = g.ind_growth_rate[i].wrapping_add(1);
        } else {
            g.ind_growth_rate[i] = g.ind_growth_rate[i].wrapping_sub(1);
        }
        if cast::<i32, _>(industry_id) == g.ind_demand_cycle_idx && !(Ext::from_f64(2.0) * rng.rnd() <= Ext::from_f64(1.0)) {
            if !(Ext::from_f64(5.0) * rng.rnd() <= cast::<Ext, _>(g.ind_growth_rate[i])) {
                let v = Ext::from_f64(5.0) + cast::<Ext, _>(g.ind_growth_forecast[i]) + Ext::from_f64(10.0) * rng.rnd();
                g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(v));
            }
            if !(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(20.0) >= cast::<Ext, _>(g.ind_growth_rate[i])) {
                let v = cast::<Ext, _>(g.ind_growth_forecast[i]) - (Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(10.0));
                g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(v));
            }
        }
        if g.ind_demand_gap[i] == Ext::from_f64(0.0) {
            let _outlook_result = calc_industry_outlook(g, rng, industry_id);
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
        }

        // --- commodity / rate shocks
        match industry_id {
            3 | 9 | 11 | 12 | 16 | 23 | 25 | 52 | 59 | 67 | 68 => {
                if !(sf(0.1f32) * rng.rnd() + sf(0.6f32) <= cast::<Ext, _>(g.oil_price))
                    && !(rng.rnd() + sf(0.8f32) >= cast::<Ext, _>(g.gdp_growth))
                    && !(sf(18.5f32) <= g.ind_demand_gap[i])
                    && !(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(5.0) <= cast::<Ext, _>(g.ind_growth_forecast[i]))
                {
                    g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(Ext::from_f64(12.0) * rng.rnd() + Ext::from_f64(5.0)));
                }
                if !(sf(0.2f32) * rng.rnd() + sf(1.2f32) <= cast::<Ext, _>(g.oil_price))
                    && !(sf(1.8f32) * rng.rnd() + sf(3.2f32) >= cast::<Ext, _>(g.gdp_growth))
                    && !(Ext::from_f64(15.0) <= g.ind_demand_gap[i])
                    && g.ind_growth_forecast[i] < 9
                {
                    g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) + 3);
                }
                if !(sf(0.14f32) * rng.rnd() + sf(1.2f32) >= cast::<Ext, _>(g.oil_price))
                    && !(rng.rnd() + sf(1.2f32) <= cast::<Ext, _>(g.gdp_growth))
                    && !(sf(7.5f32) >= g.ind_demand_gap[i])
                    && !(Ext::from_f64(5.0) - Ext::from_f64(7.0) * rng.rnd() >= cast::<Ext, _>(g.ind_growth_forecast[i]))
                {
                    let v = cast::<Ext, _>(g.ind_growth_forecast[i]) - Ext::from_f64(7.0) * rng.rnd();
                    g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(v));
                }
            }
            5 | 6 | 60 => {
                let (oil, gdp, base) = (cast::<Ext, _>(g.oil_price), cast::<Ext, _>(g.gdp_growth), cast::<Ext, _>(g.base_interest_rate));
                if sf(1.2f32) > oil
                    && !(sf(3.1f32) >= gdp)
                    && sf(5.5f32) > base
                    && !(Ext::from_f64(9.0) * rng.rnd() + Ext::from_f64(15.0) <= g.ind_demand_gap[i])
                {
                    if g.ind_growth_forecast[i] < 30 {
                        g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(15.0)));
                    }
                    if g.ind_growth_rate[i] < 7 {
                        g.ind_growth_rate[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_rate[i]) + 5);
                    }
                }
                if sf(1.3f32) > oil && !(sf(3.7f32) >= gdp) && sf(7.3f32) > base && g.ind_growth_forecast[i] < 7 {
                    g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) + 5);
                }
                if !(sf(0.9f32) >= oil) && sf(1.6f32) > gdp && !(sf(9.4f32) >= base) && g.ind_growth_forecast[i] > -5 {
                    let v = cast::<Ext, _>(g.ind_growth_forecast[i]) - Ext::from_f64(8.0) * rng.rnd();
                    g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(v));
                }
            }
            7 | 8 => {
                if !(sf(0.15f32) * rng.rnd() + sf(0.75f32) <= cast::<Ext, _>(g.oil_price))
                    && !(Ext::from_f64(2.0) * rng.rnd() + sf(1.3f32) <= cast::<Ext, _>(g.gdp_growth))
                    && !(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(10.0) >= g.ind_demand_gap[i])
                {
                    if g.ind_growth_forecast[i] > -20 {
                        let v = cast::<Ext, _>(g.ind_growth_forecast[i]) - (Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(5.0));
                        g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(v));
                    } else {
                        g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(Ext::from_f64(1.0) - Ext::from_f64(21.0) * rng.rnd()));
                    }
                    if g.ind_growth_rate[i] > -7 {
                        g.ind_growth_rate[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_rate[i]) - 10);
                    }
                    if !(Ext::from_f64(4.0) * rng.rnd() <= Ext::from_f64(3.0)) {
                        g.ind_demand[i] = to_quad(sf(0.99f32) * cast::<Ext, _>(g.ind_demand[i]));
                    }
                }
                if sf(0.7f32) > cast::<Ext, _>(g.oil_price)
                    && !(rng.rnd() + sf(2.4f32) <= cast::<Ext, _>(g.gdp_growth))
                    && !(sf(12.5f32) >= g.ind_demand_gap[i])
                    && g.ind_growth_forecast[i] > -10
                {
                    g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) - 17);
                }
                if sf(0.95f32) > cast::<Ext, _>(g.oil_price)
                    && !(sf(0.5f32) * rng.rnd() + sf(3.4f32) <= cast::<Ext, _>(g.gdp_growth))
                    && !(Ext::from_f64(15.0) >= g.ind_demand_gap[i])
                    && !(-Ext::from_f64(3.0) + Ext::from_f64(7.0) * rng.rnd() >= cast::<Ext, _>(g.ind_growth_forecast[i]))
                {
                    g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) - 9);
                }
                if !(sf(1.37f32) >= cast::<Ext, _>(g.oil_price))
                    && !(-Ext::from_f64(1.0) >= cast::<Ext, _>(g.gdp_growth))
                    && !(Ext::from_f64(20.0) <= g.ind_demand_gap[i])
                    && !(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(20.0) <= cast::<Ext, _>(g.ind_growth_forecast[i]))
                {
                    let v = Ext::from_f64(15.0) * rng.rnd() + cast::<Ext, _>(g.ind_growth_forecast[i]);
                    g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(v));
                }
            }
            15 => {
                let cw = cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price);
                if !(rng.rnd() + Ext::from_f64(13.0) >= cw)
                    && !(sf(1.4f32) * rng.rnd() >= cast::<Ext, _>(g.gdp_growth))
                    && !(sf(1.4f32) <= cast::<Ext, _>(g.oil_price))
                    && !(Ext::from_f64(15.0) <= g.ind_demand_gap[i])
                    && g.ind_growth_forecast[i] < 15
                {
                    g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) + 12);
                }
                let cw = cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price);
                if !(sf(1.5f32) * rng.rnd() + sf(6.5f32) <= cw)
                    && !(rng.rnd() + sf(3.1f32) <= cast::<Ext, _>(g.gdp_growth))
                    && !(sf(1.1f32) >= cast::<Ext, _>(g.oil_price))
                    && !(Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(10.0) >= g.ind_demand_gap[i])
                    && g.ind_growth_forecast[i] > -8
                {
                    g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) - 5);
                }
            }
            50 => {
                // (unreachable: industry 50 is handled above)
                let work = to_long(metals_indicator(g, ui));
                'up: {
                    if work > 40 && !(sf(22.5f32) <= cast::<Ext, _>(g.ind_growth_forecast[i])) {
                        if !(sf(1.55f32) < cast::<Ext, _>(g.oil_price))
                            && !(Ext::from_f64(20.0) > g.ind_demand_gap[i])
                            && g.ind_growth_rate[i] >= 25
                        {
                            break 'up;
                        }
                        let t = cast::<i32, _>(g.ind_growth_forecast[i]) + 7;
                        g.ind_growth_forecast[i] = cast::<i16, _>(rnd_int(rng, 5, 1) + t);
                        let t = cast::<i32, _>(g.ind_growth_rate[i]) + 2;
                        g.ind_growth_rate[i] = cast::<i16, _>(rnd_int(rng, 4, 1) + t);
                        if work > 45 && g.ind_growth_forecast[i] < 20 {
                            let r = rnd_int(rng, 10, 1);
                            g.ind_growth_forecast[i] = cast::<i16, _>(r + cast::<i32, _>(g.ind_growth_forecast[i]));
                            let r = rnd_int(rng, 7, 2);
                            g.ind_growth_rate[i] = cast::<i16, _>(r + cast::<i32, _>(g.ind_growth_rate[i]));
                        }
                    }
                }
                if !(Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(20.0) <= cast::<Ext, _>(work))
                    && g.ind_growth_forecast[i] > -10
                    && (sf(1.5f32) > cast::<Ext, _>(g.oil_price) || !(Ext::from_f64(10.0) >= g.ind_demand_gap[i]))
                {
                    let r = rnd_int(rng, 5, 1) + 7;
                    g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) - r);
                }
            }
            _ => {}
        }

        // --- general economy pull towards / away from the long-term growth
        if !matches!(
            industry_id,
            4 | 14 | 17 | 19 | 21 | 22 | 28 | 31 | 32 | 34 | 46 | 50 | 55 | 56 | 62 | 64 | 65 | 66
        ) {
            if !(rng.rnd() + Ext::from_f64(0.5) <= cast::<Ext, _>(g.gdp_growth)) {
                if cast::<i32, _>(g.ind_growth_forecast[i]) > cast::<i32, _>(g.ind_long_term_growth[i]) - 5 {
                    let r = rnd_int(rng, 7, 2);
                    g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_long_term_growth[i]) - r);
                }
                if g.ind_growth_rate[i] > g.ind_growth_forecast[i] {
                    let r = rnd_int(rng, 5, 2);
                    g.ind_growth_rate[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_rate[i]) - r);
                }
            } else if !(rng.rnd() + sf(3.5f32) >= cast::<Ext, _>(g.gdp_growth)) {
                if (cast::<i32, _>(g.ind_growth_forecast[i])) < cast::<i32, _>(g.ind_long_term_growth[i]) + 5 {
                    let r = rnd_int(rng, 7, 2);
                    g.ind_growth_forecast[i] = cast::<i16, _>(r + cast::<i32, _>(g.ind_long_term_growth[i]));
                }
                if g.ind_growth_rate[i] < g.ind_growth_forecast[i] {
                    let r = rnd_int(rng, 5, 2);
                    g.ind_growth_rate[i] = cast::<i16, _>(r + cast::<i32, _>(g.ind_growth_rate[i]));
                }
            }
        }
        let work = cast::<i32, _>(g.ind_growth_forecast[i]) - cast::<i32, _>(prev_forecast);
        if work.abs() > 7 && (work.abs()) > (cast::<i32, _>(max_revision)).abs() {
            max_revision = cast::<i16, _>(work);
            max_rev_industry = industry_id;
        }
    }

    if max_rev_industry > 2 {
        let m = cast::<i32, _>(max_rev_industry);
        let mut news_part1 = s(__content.string(4542));
        let news_part2;
        if max_revision > 0 {
            let mut p2 = s(__content.string(4543));
            if g.ind_name[m].len() < 20 {
                p2 = s(__content.string(4544));
                news_part1 = PbStr::new();
            }
            news_part2 = p2;
            g.ind_growth_rate[m] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_rate[m]) + 3);
        } else {
            news_part2 = s(__content.string(4545));
            g.ind_growth_rate[m] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_rate[m]) - 3);
        }
        g.news_msg = mcase(g.ind_name[m].as_bytes(), CaseMode::Ansi) + __content.string(4546) + &news_part1 + &news_part2;
        post_news(g, ui, 0, 0);
    }
    if g.sim_co_count > 1400 {
        let work = g.cur_year % 5 + 1;
        match work {
            1 => {
                industry_id = 9;
                g.news_msg = s(__content.string(4547));
            }
            2 => {
                industry_id = 11;
                g.news_msg = s(__content.string(4548));
            }
            3 => {
                industry_id = 12;
                g.news_msg = s(__content.string(4549));
            }
            4 => {
                industry_id = 23;
                g.news_msg = s(__content.string(4550));
            }
            5 => {
                industry_id = 25;
                g.news_msg = s(__content.string(4551));
            }
            _ => {}
        }
        let mut boost_flag = 0;
        let avg_net_margin = cast::<Ext, _>(calc_industry_avg_net_margin(g, rng, industry_id));
        if !(Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(7.0) <= avg_net_margin) && !(sf(0.7f32) * rng.rnd() + sf(1.7f32) >= cast::<Ext, _>(g.gdp_growth)) {
            boost_flag = 1;
        }
        if !(Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(13.0) <= avg_net_margin) && !(rng.rnd() + Ext::from_f64(3.0) >= cast::<Ext, _>(g.gdp_growth)) {
            boost_flag = 1;
        }
        if boost_flag == 0 {
            g.news_msg = PbStr::new();
        } else {
            let i = cast::<i32, _>(industry_id);
            let f = sf(0.07f32) * rng.rnd() + sf(1.15f32);
            g.ind_demand[i] = to_quad(f * cast::<Ext, _>(g.ind_demand[i]));
            g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) + 20);
            if g.ind_growth_rate[i] < 3 {
                g.ind_growth_rate[i] = 3;
            }
            g.ind_growth_rate[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_rate[i]) + 7);
            if i == g.sel_industry {
                let mut msg = g.news_msg.clone() + ".";
                show_quick_message(g, ui, &mut msg);
            }
            post_news(g, ui, 0, 0);
        }
    }
}

/// Port of CalcIndustryDemandSwing (0x65b298).
/// In a boom (GDP growth at least 3.2..4.2) or a recession (at most -1.5..-0.5) moves `IndDemand` of
/// `industry_id` by its GDP sensitivity (`(GdpGrowth - 3) * 5` % in a boom, `GdpGrowth` % otherwise,
/// times 0.15..1, capped at ±8 %), unless the industry's demand gap already exceeds 8..12 % (boom) or is
/// below 7..10 % (recession). Gold/silver-like industries 48, 50, 62 and industries without a demand gap
/// are left alone.
pub fn calc_industry_demand_swing(g: &mut Globals, rng: &mut Rng, industry_id: i32) {
    let ind = cast::<i16, _>(industry_id); // INTEGER local
    let i = cast::<i32, _>(ind);
    if !(rng.rnd() * Ext::from_f64(1.0) - Ext::from_f64(1.5) >= cast::<Ext, _>(g.gdp_growth)) && !(Ext::from_f64(1.0) * rng.rnd() + sf(3.2f32) <= cast::<Ext, _>(g.gdp_growth)) {
        return;
    }
    if g.ind_demand_gap[i] == Ext::from_f64(0.0) {
        return;
    }
    let sensitivity: Ext = match ind {
        4 | 17 | 21 | 22 | 28 | 31 | 32 | 34 | 46 | 55 | 56 => sf(0.15f32),
        14 | 19 | 27 | 29 | 30 | 43 | 64 | 65 | 66 | 69 => sf(0.3f32),
        7 | 8 | 15 | 26 | 41 | 61 | 63 => Ext::from_f64(0.5),
        3 | 5 | 6 | 9 | 12 | 13 | 16 | 18 | 20 | 23 | 25 | 33 | 35 | 36 | 42 | 47 | 52 | 59 | 60 | 67 | 68 => Ext::from_f64(1.0),
        48 | 50 | 62 => return,
        _ => sf(0.7f32),
    };
    if Ext::from_f64(0.0) < cast::<Ext, _>(g.gdp_growth) && !(Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(8.0) >= g.ind_demand_gap[i]) {
        return;
    }
    if Ext::from_f64(0.0) > cast::<Ext, _>(g.gdp_growth) && !(Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(7.0) <= g.ind_demand_gap[i]) {
        return;
    }
    let mut demand_change_pct: Ext = if Ext::from_f64(0.0) < cast::<Ext, _>(g.gdp_growth) {
        Ext::from_f64(5.0) * (cast::<Ext, _>(g.gdp_growth) - Ext::from_f64(3.0))
    } else {
        cast::<Ext, _>(g.gdp_growth)
    };
    demand_change_pct = sensitivity * demand_change_pct * sf(0.01f32);
    if !(sf(0.08f32) >= demand_change_pct) {
        demand_change_pct = sf(0.08f32);
    }
    if -sf(0.08f32) > demand_change_pct {
        demand_change_pct = -sf(0.08f32);
    }
    g.ind_demand[i] = to_quad((demand_change_pct + Ext::from_f64(1.0)) * cast::<Ext, _>(g.ind_demand[i]));
}

/// Port of CommodityDemandEffects (0x4bce6a).
/// With probability 1/5, for the industry `SimCoCount MOD 70` (3..69) of an active company being
/// simulated: if its demand gap (from `CalcIndustryOutlook` when not yet known) exceeds 26..35 %,
/// commodity prices and rates may hurt it (oil-sensitive 3/23/25/53/67/68: forecast -1; rate-sensitive
/// 5/6, oil 7/8/16/52/69, grain 15, metals 50: demand x0.995, growth rate -1..3, forecast -10).
pub fn commodity_demand_effects(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let mut hit = false;
    if !(Ext::from_f64(5.0) * rng.rnd() <= Ext::from_f64(1.0)) {
        return;
    }
    if g.entity_status[g.sim_co_count] != 1 || g.sim_co_count > 1600 {
        return;
    }
    let ind = cast::<i16, _>(g.sim_co_count % 70);
    if ind < 3 || ind > 69 {
        return;
    }
    let i = cast::<i32, _>(ind);
    let demand_gap: f32 = cast::<f32, _>(if g.ind_demand_gap[i] != Ext::from_f64(0.0) {
        cast::<f32, _>(g.ind_demand_gap[i])
    } else {
        calc_industry_outlook(g, rng, ind);
        let d = cast::<f32, _>(g.tmp_ext1);
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.tmp_ext2 = Ext::from_f64(0.0);
        g.tmp_ext3 = Ext::from_f64(0.0);
        d
    });
    if !(Ext::from_f64(9.0) * rng.rnd() + Ext::from_f64(26.0) <= cast::<Ext, _>(demand_gap)) {
        return;
    }
    match ind {
        3 | 23 | 25 | 53 | 67 | 68 => {
            if !(sf(0.2f32) * rng.rnd() + sf(1.2f32) >= cast::<Ext, _>(g.oil_price)) && g.ind_growth_forecast[i] > -10 {
                g.ind_growth_forecast[i] = g.ind_growth_forecast[i].wrapping_sub(1);
            }
        }
        5 | 6 => {
            if !(Ext::from_f64(2.0) * rng.rnd() + sf(9.2f32) >= cast::<Ext, _>(g.base_interest_rate)) {
                hit = true;
            }
        }
        7 | 8 => {
            if !(sf(0.15f32) * rng.rnd() + sf(0.65f32) <= cast::<Ext, _>(g.oil_price)) {
                hit = true;
            }
        }
        15 => {
            let x = sf(0.5f32) * rng.rnd() + Ext::from_f64(8.0);
            if !(cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price) >= x) && !(sf(0.92f32) >= cast::<Ext, _>(g.oil_price)) {
                hit = true;
            }
            let y = rng.rnd() + Ext::from_f64(5.0);
            if !(cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price) >= y) {
                hit = true;
            }
        }
        16 | 52 => {
            if !(sf(0.25f32) * rng.rnd() + sf(1.28f32) >= cast::<Ext, _>(g.oil_price)) {
                hit = true;
            }
        }
        50 => {
            let x = cast::<Ext, _>(g.silver_price) - cast::<Ext, _>(g.metals_price_base) * sf(0.01f32);
            let ret = update_metals_price_base(g, ui);
            let v = x + ret / Ext::from_f64(100.0);
            if !(sf(3.5f32) * rng.rnd() + Ext::from_f64(28.0) <= v) {
                hit = true;
            }
        }
        69 => {
            if !(sf(0.2f32) * rng.rnd() + sf(1.1f32) >= cast::<Ext, _>(g.oil_price))
                && !(Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(10.0) >= cast::<Ext, _>(g.base_interest_rate))
            {
                hit = true;
            }
        }
        _ => return,
    }
    if hit && g.ind_growth_rate[i] > -5 {
        g.ind_demand[i] = to_quad(sf(0.995f32) * cast::<Ext, _>(g.ind_demand[i]));
        let r = rnd_int(rng, 3, 1);
        g.ind_growth_rate[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_rate[i]) - r);
    }
    if hit && g.ind_growth_rate[i] > -15 && g.ind_growth_forecast[i] > -20 {
        g.ind_growth_forecast[i] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[i]) - 10);
    }
}

/// Port of IndustryFadNews (0x4bc887).
/// The industry fad (no fad during a bubble, `BubbleState > 0`): picks a fad industry (forecast >= 3,
/// up to 20 tries) when there is none or its `FadValuationPct` fell below 76, with a new peak
/// `IndFadPeak` = 160..260 (analyst headline when `TIME$` ends in 7); while the valuation is above the
/// peak it deflates by 3 (demand x0.95), else it climbs (+6 below 140, +2 above; odd values fall by
/// 2/10), with "Analysts say … stocks are over/undervalued" at 200/201 (demand x1.05 at 200). Rarely
/// (`RandSeed MOD 100 = 97`) drops a fad industry whose growth rate is below -3.
///
/// `time_secs`: the seconds of `TIME$` (`RIGHT$(TIME$, 1) = "7"`, `VAL(RIGHT$(TIME$, 2)) > 20`).
/// `IndAnalystPicked` is re-DIMmed (cleared) on every call.
pub fn industry_fad_news(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    g.ind_analyst_picked.reset(); // DIM IndAnalystPicked(0 TO 69)
    if g.bubble_state > 0 {
        return;
    }
    let mut tries = 0;
    if g.ind_fad_peak == 0 {
        let rand_peak = g.rand_seed % 51;
        g.ind_fad_peak = cast::<i16, _>((rand_peak + 80) * 2);
    }
    if g.rand_seed % 100 == 97 && g.fad_industry > 2 && g.ind_growth_rate[g.fad_industry] < -3 {
        g.fad_industry = 0;
    }
    if g.fad_industry == 0 || g.fad_valuation_pct < 76 {
        g.fad_valuation_pct = 0;
        g.fad_industry = 0;
        while tries < 20 {
            let ind_id = rnd_int(rng, 69, 1);
            if ind_id > 2 && g.ind_growth_forecast[ind_id] < 3 {
                tries += 1;
                continue;
            }
            if g.ind_analyst_picked[ind_id] == 0 {
                g.fad_industry = ind_id;
                g.ind_analyst_picked[ind_id] = 1;
                g.fad_valuation_pct = 80;
                let rand_peak = g.rand_seed % 51;
                g.ind_fad_peak = cast::<i16, _>((rand_peak + 80) * 2);
                if time_secs % 10 == 7 {
                    let word_text = if g.ind_fad_peak > 190 { "down" } else { "up" };
                    g.news_msg = s(__content.string(4552))
                        + word_text
                        + "grades "
                        + &lcase(g.ind_name[g.fad_industry].as_bytes(), CaseMode::Ansi)
                        + __content.string(1157);
                    post_news(g, ui, 0, 0);
                }
                return;
            }
            tries += 1;
        }
        return;
    }
    if (cast::<i32, _>(g.ind_fad_peak)) < g.fad_valuation_pct {
        g.fad_valuation_pct -= 3;
        if g.fad_industry > 2 {
            let f = g.fad_industry;
            g.ind_demand[f] = to_quad(sf(0.95f32) * cast::<Ext, _>(g.ind_demand[f]));
        }
    } else {
        if g.fad_valuation_pct == 200 || g.fad_valuation_pct == 201 {
            let word_text = if time_secs > 20 { "under" } else { "over" };
            g.news_msg = s(__content.string(4553))
                + &lcase(g.ind_name[g.fad_industry].as_bytes(), CaseMode::Ansi)
                + __content.string(4554)
                + word_text
                + "valued";
            post_news(g, ui, 0, 0);
            if g.fad_valuation_pct == 200 && g.fad_industry > 2 {
                let f = g.fad_industry;
                g.ind_demand[f] = to_quad(sf(1.05f32) * cast::<Ext, _>(g.ind_demand[f]));
            }
        }
        let val_step = if g.fad_valuation_pct % 2 == 0 {
            if g.fad_valuation_pct < 140 { 6 } else { 2 }
        } else if g.fad_valuation_pct < 105 {
            -2
        } else {
            -10
        };
        g.fad_valuation_pct += val_step;
    }
}

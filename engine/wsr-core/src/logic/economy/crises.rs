//! Scripted macro scenarios: `MonetaryPolicyScenario` (0x537b20), `PeakOilCrisisUpdate` (0x53a308),
//! `CountryDisasterShock` (0x539f98) and `TechBubbleCycle` (0x482549) with `Bubble_StageNews` (0x4831b8).
//!
//! The SELECT CASE / IF chains compare EXT copies of LONG/INTEGER values with integer pool constants;
//! they are written as integer comparisons here (exact). SINGLE pool constants are `sf(..)`.
use crate::pb::real::{Ext, cast};


use super::*;
use crate::logic::valuation::update_price_history_tick;
use crate::pb::string::{lcase, CaseMode};

/// `ShowQuickMessage(NewsMsg)`: the popup takes `NewsMsg` BYREF (it encodes line breaks in place).
fn quick_news(g: &mut Globals, ui: &mut dyn Ui) {
    let mut m = std::mem::take(&mut g.news_msg);
    show_quick_message(g, ui, &mut m);
    g.news_msg = m;
}

/// Port of MonetaryPolicyScenario (0x537b20).
/// Picks a macro scenario 1..100 (`scenario_id = 0`: keeps the running one with probability 1/3, else
/// draws an unused one from `MacroEventUsed` in up to 200 tries; otherwise `scenario_id` is forced) and
/// sets the targets for GDP growth, the interest rate, market valuation and commodity prices
/// (`ShockCommodityPrice` on oil / gold / wheat), then jitters all targets, nudges GDP by the tax rates
/// and announces the central bank's stance (`MonetaryPolicy` 0 easing / 1 neutral / 2 tight) as news.
///
/// Notes: `DIM MacroEventUsed(0 TO 100)` at entry leaves the existing global array as it is. In
/// 56..60 the `scenarioId = 59` branch is only reached with `scenarioId <= 0` (dead); in 71..75 the test
/// `… OR 75` is always true, so the `InterestRateTarget * 0.85` branch never runs (see BUGS.md). The ON
/// ERROR handler only shows ShowRuntimeError.
pub fn monetary_policy_scenario(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, scenario_id: i32) {
    let __content = crate::content::current();
    let p = scenario_id;
    let mut tries = 0;
    if p == 0 {
        if 0 < g.econ_scenario_id && rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
            return;
        }
        g.econ_scenario_id = 0;
        while tries < 200 && g.econ_scenario_id == 0 {
            tries += 1;
            g.econ_scenario_id = rng.rnd_range(100, 1);
            let id = g.econ_scenario_id;
            if g.macro_event_used[id] == 1 && tries < 200 {
                g.econ_scenario_id = 0;
            } else {
                g.macro_event_used[id] = 1;
            }
        }
    } else {
        g.econ_scenario_id = p;
        let id = g.econ_scenario_id;
        if !(0..=100).contains(&id) {
            return; // error 9 -> ON ERROR handler (ShowRuntimeError)
        }
        g.macro_event_used[id] = 1;
    }
    if g.econ_scenario_id < 1 {
        return;
    }
    let e = g.econ_scenario_id; // local copy (`scenario`)
    let base = |g: &Globals| cast::<Ext, _>(g.base_interest_rate);
    let irt = |g: &Globals| cast::<Ext, _>(g.interest_rate_target);
    match e {
        1..=5 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(3.7f32));
            if Ext::from_f64(1.0) > cast::<Ext, _>(g.oil_target_price) {
                shock_commodity_price(g, rng, b"OIL", 15);
            }
            if p == 1 || p == 2 {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(rng.rnd() * sf(1.2f32) + sf(2.2f32)));
            } else if p == 5 && g.next_market_cycle_year == g.cur_year && sf(8.5f32) > irt(g) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(13.0) * rng.rnd() + irt(g)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(sf(3.5f32) + base(g)));
            }
            shock_commodity_price(g, rng, b"GOLD", 10);
        }
        6..=10 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(4.1f32));
            if sf(1.25f32) > cast::<Ext, _>(g.oil_target_price) {
                shock_commodity_price(g, rng, b"OIL", 15);
            }
            if e == 8 {
                shock_commodity_price(g, rng, b"OIL", 14);
            }
            if p == 6 || rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * (base(g) + Ext::from_f64(3.0))));
            } else {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) * rng.rnd() + sf(2.3f32)));
            }
            shock_commodity_price(g, rng, b"GOLD", 19);
        }
        11..=15 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(3.9f32));
            shock_commodity_price(g, rng, b"OIL", -26);
            if p == 11 {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(rng.rnd() * sf(1.5f32) + sf(2.8f32)));
            } else if e == 12
                && rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0)
                && Ext::from_f64(8.0) > irt(g)
                && 1 + g.next_market_cycle_year == g.cur_year
            {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(10.5f32) + irt(g)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(sf(1.7f32) + base(g)));
            }
            shock_commodity_price(g, rng, b"GOLD", -10);
            shock_commodity_price(g, rng, b"WHEAT", -12);
        }
        16..=20 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(4.3f32));
            shock_commodity_price(g, rng, b"OIL", -20);
            if p == 19 || rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(14.0)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(sf(2.3f32) + base(g)));
            } else if p == 18 {
                if sf(6.5f32) < irt(g) {
                    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.8f32) * irt(g)));
                }
            } else {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(1.2f32));
            }
            shock_commodity_price(g, rng, b"GOLD", 10);
        }
        21..=25 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(2.0f32));
            if sf(1.2f32) > cast::<Ext, _>(g.oil_target_price) {
                shock_commodity_price(g, rng, b"OIL", 20);
            }
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(base(g) + sf(9.5f32)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(sf(1.8f32) + base(g)));
            } else if p == 21 || p == 25 {
                if g.next_market_cycle_year == g.cur_year {
                    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(17.0)));
                    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(4.0) + base(g)));
                } else {
                    g.interest_rate_target = cast::<f32, _>(Ext::from_f32(8.5f32));
                }
            } else if p == 22 {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(2.1f32));
            }
            if (cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(450.0)) < cast::<Ext, _>(g.gold_target_price) {
                shock_commodity_price(g, rng, b"GOLD", -20);
            }
        }
        26..=30 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(2.0f32));
            if sf(1.2f32) > cast::<Ext, _>(g.oil_target_price) {
                shock_commodity_price(g, rng, b"OIL", 20);
            }
            if p == 26 && rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(2.0) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(4.0) * rng.rnd() + sf(15.5f32)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(sf(2.4f32) + base(g)));
            } else if p == 27 && rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(3.0) + base(g)));
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.85f32) * irt(g)));
            } else if sf(4.5f32) < irt(g) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.6f32) * irt(g)));
            }
        }
        31..=35 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(2.0f32));
            shock_commodity_price(g, rng, b"OIL", -23);
            if p == 31 && rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(9.0) * rng.rnd() + Ext::from_f64(12.0)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(3.0) + base(g)));
            } else if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(2.0) {
                if 1 + g.next_market_cycle_year == g.cur_year {
                    g.interest_rate_target = cast::<f32, _>(Ext::from_f32(15.0f32));
                }
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) + base(g)));
            } else {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(2.5f32));
            }
        }
        36..=40 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(2.2f32));
            shock_commodity_price(g, rng, b"OIL", -20);
            if sf(3.5f32) < cast::<Ext, _>(g.gdp_growth_target) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt(g) + Ext::from_f64(1.0)));
            } else {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt(g) - Ext::from_f64(1.0)));
            }
            if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(3.0) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(rng.rnd() * sf(1.1f32) + sf(1.9f32)));
            }
            shock_commodity_price(g, rng, b"GOLD", -15);
        }
        41..=45 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(3.0f32));
            shock_commodity_price(g, rng, b"OIL", 18);
            g.market_val_target = cast::<f64, _>(Ext::from_f64(1.0));
            if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(3.0) && g.next_market_cycle_year == g.cur_year {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(10.0)));
                g.market_val_target = cast::<f64, _>(sf(1.2f32));
            } else {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt(g) + Ext::from_f64(1.0)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(base(g) + Ext::from_f64(1.0)));
            }
        }
        46..=50 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(3.5f32));
            shock_commodity_price(g, rng, b"OIL", -14);
            if 0 < p {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * (base(g) + sf(6.8f32))));
            } else {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(2.75f32));
                if Ext::from_f64(3.0) < base(g) {
                    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(base(g) - Ext::from_f64(1.0)));
                }
            }
            g.market_val_target = cast::<f64, _>(Ext::from_f64(1.0));
        }
        51..=55 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(2.8f32));
            shock_commodity_price(g, rng, b"OIL", 33);
            if p == 51 && rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                if g.next_market_cycle_year == g.cur_year {
                    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(base(g) + Ext::from_f64(10.0)));
                }
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(sf(3.2f32) + base(g)));
            } else {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(3.0) + irt(g)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(base(g) + Ext::from_f64(1.0)));
            }
            g.market_val_target = cast::<f64, _>(Ext::from_f64(1.0));
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.14f32) * cast::<Ext, _>(g.silver_target_price)));
        }
        56..=60 => {
            if p != 56 {
                g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(3.2f32));
                shock_commodity_price(g, rng, b"OIL", -40);
                if 0 < p || rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) {
                    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * (base(g) + sf(4.5f32))));
                } else if e == 58 && 0 < g.monetary_policy && 1 + g.next_market_cycle_year == g.cur_year {
                    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * (base(g) + sf(13.3f32))));
                    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) + base(g)));
                    g.market_val_target = cast::<f64, _>(sf(0.8f32));
                } else if p == 59 {
                    // unreachable: this branch only runs with scenarioId <= 0
                    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(base(g) - Ext::from_f64(1.0)));
                    g.interest_rate_target = cast::<f32, _>(Ext::from_f32(2.0f32));
                    g.market_val_target = cast::<f64, _>(sf(1.4f32));
                }
                shock_commodity_price(g, rng, b"GOLD", -20);
            }
        }
        61..=65 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(4.8f32));
            shock_commodity_price(g, rng, b"OIL", -30);
            if p == 62 || p == 63 || p == 64 {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * rng.rnd() + Ext::from_f64(3.0)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(base(g) + Ext::from_f64(1.0)));
            } else if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * (base(g) + sf(8.1f32))));
            }
            g.market_val_target = cast::<f64, _>(sf(1.35f32) * g.market_val_target);
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.15f32) * cast::<Ext, _>(g.market_valuation)));
        }
        66..=70 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(0.2f32));
            shock_commodity_price(g, rng, b"OIL", 30);
            if p == 70 {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(16.0)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(base(g) - Ext::from_f64(1.0)));
            }
            if p == 68 {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(3.0)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(base(g) + Ext::from_f64(1.0)));
            }
            g.market_val_target = cast::<f64, _>(sf(0.9f32) * g.market_val_target);
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.9f32) * cast::<Ext, _>(g.market_valuation)));
        }
        71..=75 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(5.8f32));
            shock_commodity_price(g, rng, b"OIL", 8);
            // `IF scenarioId = 71 OR scenarioId = 73 OR 75`: always true (the ELSE branch,
            // `InterestRateTarget * 0.85`, is unreachable)
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * (base(g) + sf(2.2f32))));
            g.market_val_target = cast::<f64, _>(sf(1.25f32) * g.market_val_target);
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.09f32) * cast::<Ext, _>(g.market_valuation)));
        }
        76..=80 => {
            g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(0.5f32));
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(11.0)));
            g.market_val_target = cast::<f64, _>(sf(0.9f32) * g.market_val_target);
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.9f32) * cast::<Ext, _>(g.market_valuation)));
            if p == 76 || p == 77 || p == 80 {
                if g.next_market_cycle_year == g.cur_year {
                    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(15.0)));
                } else {
                    g.interest_rate_target = cast::<f32, _>(Ext::from_f32(9.5f32));
                }
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(base(g) + Ext::from_f64(1.0)));
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.95f32) * cast::<Ext, _>(g.market_valuation)));
                g.market_val_target = cast::<f64, _>(sf(0.9f32) * g.market_val_target);
            }
            if p == 78 {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(2.5f32) * rng.rnd() + Ext::from_f64(2.0)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(base(g) + Ext::from_f64(1.0)));
            }
        }
        81..=83 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(7.6f32));
            shock_commodity_price(g, rng, b"OIL", -35);
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) * rng.rnd() + sf(3.5f32)));
            if p == 83 {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(1.8f32));
            }
            if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * (base(g) + sf(5.3f32))));
            }
            g.market_val_target = cast::<f64, _>(sf(1.3f32) * g.market_val_target);
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.15f32) * cast::<Ext, _>(g.market_valuation)));
            shock_commodity_price(g, rng, b"GOLD", -12);
        }
        84..=86 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(7.3f32));
            shock_commodity_price(g, rng, b"OIL", 30);
            if 0 < p || rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(3.0) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt(g) + Ext::from_f64(1.0)));
            } else {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(9.0)));
            }
            g.market_val_target = cast::<f64, _>(sf(1.21f32) * g.market_val_target);
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.1f32) * cast::<Ext, _>(g.market_valuation)));
            shock_commodity_price(g, rng, b"GOLD", 20);
        }
        87..=89 => {
            if check_index_hist_low(g) == 1 {
                if sf(0.5f32) < cast::<Ext, _>(g.gdp_growth_target) {
                    g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(5.0f32));
                } else {
                    g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(2.0f32));
                }
            } else {
                g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(13.0f32));
            }
            shock_commodity_price(g, rng, b"OIL", -42);
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * (base(g) + (rng.rnd() * sf(0.4f32) + sf(2.7f32)))));
            if p == 88 {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(8.0)));
            }
            g.market_val_target = cast::<f64, _>(sf(0.79f32) * g.market_val_target);
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.93f32) * cast::<Ext, _>(g.market_valuation)));
            shock_commodity_price(g, rng, b"GOLD", -24);
        }
        90..=92 => {
            if check_index_hist_low(g) == 1 {
                if sf(0.5f32) < cast::<Ext, _>(g.gdp_growth_target) {
                    g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(5.0f32));
                } else {
                    g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(1.0f32));
                }
            } else {
                g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(10.0f32));
            }
            shock_commodity_price(g, rng, b"OIL", 40);
            if sf(3.7f32) < cast::<Ext, _>(g.gdp_growth) && g.next_market_cycle_year == g.cur_year {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * (base(g) + sf(12.3f32))));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(base(g) - Ext::from_f64(1.0)));
            }
            g.market_val_target = cast::<f64, _>(sf(0.87f32) * g.market_val_target);
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.96f32) * cast::<Ext, _>(g.market_valuation)));
        }
        93..=100 => {
            g.gdp_growth_target = cast::<f32, _>(Ext::from_f32(3.0f32));
            g.oil_target_price = cast::<f32, _>(Ext::from_f32(1.0f32));
            if Ext::from_f64(4.0) < rng.rnd() * Ext::from_f64(5.0) {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(6.0f32));
            } else {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(4.0f32));
            }
            g.market_val_target = cast::<f64, _>(Ext::from_f64(1.0));
        }
        _ => {}
    }
    // jitter of all targets (a SINGLE local)
    let mut jitter = cast::<f32, _>(rng.rnd() * Ext::from_f64(1.0) - sf(0.5f32));
    g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(jitter) + cast::<Ext, _>(g.gdp_growth_target)));
    jitter = cast::<f32, _>(rng.rnd() * sf(0.2f32) - sf(0.1f32));
    g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(jitter) + cast::<Ext, _>(g.oil_target_price)));
    if sf(0.3f32) > cast::<Ext, _>(g.oil_target_price) {
        g.oil_target_price = cast::<f32, _>(Ext::from_f32(0.3f32));
    }
    jitter = cast::<f32, _>(rng.rnd() * Ext::from_f64(1.0) - sf(0.5f32));
    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(jitter) + irt(g)));
    jitter = cast::<f32, _>(rng.rnd() * Ext::from_f64(30.0) - Ext::from_f64(15.0));
    g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(jitter) + cast::<Ext, _>(g.gold_target_price)));
    jitter = cast::<f32, _>(rng.rnd() * Ext::from_f64(4.0) - Ext::from_f64(2.0));
    g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(jitter) + cast::<Ext, _>(g.silver_target_price)));
    jitter = cast::<f32, _>(rng.rnd() * Ext::from_f64(1.0) - sf(0.5f32));
    g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(jitter) + cast::<Ext, _>(g.wheat_target_price)));
    g.corn_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(jitter) + cast::<Ext, _>(g.corn_target_price)));
    if sf(2.5f32) > irt(g) {
        g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(rng.rnd() * sf(0.6f32) + sf(2.4f32)));
    }
    if Ext::from_f64(19.0) < irt(g) {
        g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(18.0)));
    }
    if rng.rnd() * Ext::from_f64(4.0) + Ext::from_f64(24.0) > cast::<Ext, _>(g.corp_tax_rate) || rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(15.0) > cast::<Ext, _>(g.cap_gains_tax_rate) {
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(rng.rnd() * Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth_target)));
    }
    if rng.rnd() * Ext::from_f64(4.0) + Ext::from_f64(38.0) < cast::<Ext, _>(g.corp_tax_rate) || rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(25.0) < cast::<Ext, _>(g.cap_gains_tax_rate) {
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - rng.rnd() * Ext::from_f64(1.0)));
    }
    let bank_name = g.central_bank_name.clone();
    let the = |tail: &str| s("The ") + &bank_name + tail;
    let is_fed = 0 < instr(1, g.central_bank_name.as_bytes(), __content.bytes(4373));
    if base(g) < irt(g) {
        match g.monetary_policy {
            0 => {
                g.news_msg = the(__content.string(4374));
                g.monetary_policy = 1;
                if sf(2.2f32) + base(g) < irt(g) {
                    g.news_msg = the(__content.string(4375));
                    g.monetary_policy = 2;
                }
                if is_fed && sf(2.2f32) > cast::<Ext, _>(g.gdp_growth) {
                    g.news_msg = s(__content.string(4376));
                }
            }
            1 => {
                g.news_msg = bank_name.clone() + __content.string(4377);
                g.monetary_policy = 2;
            }
            2 => {
                g.news_msg = bank_name.clone() + __content.string(4378);
            }
            _ => {}
        }
    } else {
        match g.monetary_policy {
            0 => {
                g.news_msg = the(__content.string(4379));
                if is_fed && sf(2.2f32) > cast::<Ext, _>(g.gdp_growth) && Ext::from_f64(6.0) > base(g) {
                    g.news_msg = s(__content.string(4380));
                }
            }
            1 => {
                g.news_msg = bank_name.clone() + __content.string(4381);
                g.monetary_policy = 0;
            }
            2 => {
                g.news_msg = the(__content.string(4382));
                g.monetary_policy = 1;
                if sf(2.2f32) + irt(g) < base(g) {
                    g.news_msg = bank_name.clone() + __content.string(4383);
                    g.monetary_policy = 0;
                }
            }
            _ => {}
        }
    }
    if 0 < instr(1, g.news_msg.as_bytes(), bank_name.as_bytes()) && g.suppress_popups == 0 {
        quick_news(g, ui);
    }
    post_news(g, ui, 0, 0);
}

/// Port of PeakOilCrisisUpdate (0x53a308).
/// Advances the "peak oil" crisis: while `PeakOilCrisis` is set (not -1) every call raises
/// `OilCrisisStage`, sets `OilCrisisLevel` / `OilTargetPrice`, cuts GDP and market targets, shifts industry
/// demand and long-term growth (energy 7/8 and 50/62 up, the rest down) and posts a crisis headline
/// (`LastOilCrisisHeadline`); in stages 4..17 the crisis may end (stage 18). With `PeakOilCrisis = -1`
/// (aftermath) the level steps down with matching news and extreme long-term growth rates are damped.
///
/// Note: the industry-14 test `stage > 2 OR stage < 13` is always true, so the "DEFENSE SPENDING RISES"
/// headline (stage 13) is never posted and industry 14 is not adjusted (see BUGS.md).
pub fn peak_oil_crisis_update(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if g.peak_oil_crisis == 0 {
        return;
    }
    let damp = |x: i16| cast::<i16, _>(to_quad(sf(0.7f32) * cast::<Ext, _>(x)));
    if g.peak_oil_crisis == -1 {
        if g.oil_crisis_level == Ext::from_f32(0.0f32) {
            return;
        }
        if g.oil_crisis_level > g.oil_target_price {
            g.oil_target_price = cast::<f32, _>(g.oil_crisis_level);
        }
        let mut price_text; // (the shared local priceText; only used for the headline)
        let sel = cast::<Ext, _>(g.oil_crisis_level);
        if sel > Ext::from_f64(7.0) {
            g.news_msg = s(__content.string(4384));
            g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_crisis_level) - sf(5.7f32)));
        } else if sel > Ext::from_f64(5.0) {
            g.news_msg = s(__content.string(4385));
            g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_crisis_level) - sf(3.2f32)));
        } else if sel > Ext::from_f64(3.0) {
            g.news_msg = s(__content.string(4386));
            g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_crisis_level) - sf(2.1f32) * rng.rnd()));
        } else if sel > Ext::from_f64(2.0) {
            g.oil_crisis_level = cast::<f32, _>(Ext::from_f32(1.3f32));
            price_text = fmt_trim(Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price), "####.##");
            g.news_msg = s(__content.string(4387)) + &price_text + __content.string(4388);
        } else if sel > Ext::from_f64(1.0) {
            g.news_msg = s(__content.string(4389));
            g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>(sf(0.65f32) * rng.rnd() + sf(0.4f32)));
        } else {
            g.news_msg = s(__content.string(4390));
            price_text = s(__content.string(4391));
            if g.rand_seed % 2 == 1 {
                price_text = s("OVER.");
            }
            g.news_msg = g.news_msg.clone() + &price_text;
            if Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
                g.news_msg = s(__content.string(4392));
            }
            g.oil_crisis_level = cast::<f32, _>(Ext::from_f32(0.0f32));
            if Ext::from_f64(4.0) * rng.rnd() > Ext::from_f64(3.0) {
                g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>(sf(0.7f32) * rng.rnd() + Ext::from_f64(2.0)));
            }
        }
        if g.econ_cycle_counter == 0 {
            g.econ_cycle_counter = 4;
        }
        post_news(g, ui, 0, 0);
        for industry in 3..=69 {
            if (cast::<Ext, _>(g.ind_long_term_growth[industry])).abs() > Ext::from_f64(10.0) {
                g.ind_long_term_growth[industry] = damp(g.ind_long_term_growth[industry]);
            }
        }
        return;
    }
    g.oil_crisis_stage += 1;
    g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(3.0)) + sf(0.2f32) * cast::<Ext, _>(g.oil_crisis_stage)));
    if g.oil_crisis_stage == 1 {
        g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>(sf(0.7f32) * rng.rnd() + Ext::from_f64(2.0)));
    }
    if Ext::from_f64(3.0) > cast::<Ext, _>(g.oil_target_price) && Ext::from_f64(4.0) > cast::<Ext, _>(g.oil_crisis_level) {
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_crisis_level) + cast::<Ext, _>(g.oil_target_price)));
    } else {
        g.oil_target_price = cast::<f32, _>(g.oil_crisis_level);
    }
    if -Ext::from_f64(7.0) < cast::<Ext, _>(g.gdp_growth_target) {
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(3.0)));
    }
    g.market_val_target = cast::<f64, _>(sf(0.3f32) * rng.rnd() + sf(0.6f32));
    g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.94f32) * cast::<Ext, _>(g.market_valuation)));
    if Ext::from_f64(8.0) * rng.rnd() < Ext::from_f64(1.0) {
        g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(9.0) * rng.rnd() + Ext::from_f64(13.0)));
    }
    for industry in 3..=69i32 {
        let ltg = |g: &Globals| cast::<Ext, _>(g.ind_long_term_growth[industry]);
        let demand = |g: &Globals| cast::<Ext, _>(g.ind_demand[industry]);
        let mut skip_adjust = false;
        if industry == 7 || industry == 8 {
            if g.oil_crisis_stage < 11 {
                g.ind_demand[industry] = to_quad((sf(1.15f32) - sf(0.14f32) * rng.rnd()) * demand(g));
                if Ext::from_f64(20.0) * rng.rnd() + Ext::from_f64(40.0) > ltg(g) {
                    g.ind_long_term_growth[industry] = cast::<i16, _>(to_quad(Ext::from_f64(8.0) * rng.rnd() + ltg(g)));
                }
            }
        } else if industry == 14 || industry == 50 || industry == 62 {
            if industry == 14 {
                // `IF OilCrisisStage > 2 OR OilCrisisStage < 13 THEN` skip: always true
                skip_adjust = true;
            }
            if !skip_adjust && g.oil_crisis_stage < 7 && industry > 14 {
                g.ind_demand[industry] = to_quad((sf(1.12f32) - sf(0.13f32) * rng.rnd()) * demand(g));
                if Ext::from_f64(20.0) * rng.rnd() + Ext::from_f64(30.0) > ltg(g) {
                    g.ind_long_term_growth[industry] = cast::<i16, _>(to_quad(Ext::from_f64(8.0) * rng.rnd() + ltg(g)));
                }
            }
        } else if g.oil_crisis_stage < 16 {
            g.ind_demand[industry] = to_quad((sf(0.14f32) * rng.rnd() + sf(0.88f32)) * demand(g));
            if -Ext::from_f64(20.0) - Ext::from_f64(10.0) * rng.rnd() < ltg(g) {
                g.ind_long_term_growth[industry] = cast::<i16, _>(to_quad(ltg(g) - Ext::from_f64(4.0) * rng.rnd()));
            }
        }
        if ltg(g).abs() > Ext::from_f64(35.0) {
            g.ind_long_term_growth[industry] = damp(g.ind_long_term_growth[industry]);
        }
    }
    let price_text = str_trim(cast::<Ext, _>(to_long(Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price))));
    let (chaos_region, defender_nation) = match to_long(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(1.0)) {
        1 => ("EUROPE", "MEXICAN"),
        2 => ("BRAZIL", "CANADIAN"),
        3 => ("INDIA", "INDONESIAN"),
        4 => ("RUSSIA", "IRANIAN"),
        _ => ("CALIFORNIA", "VENEZUELAN"),
    };
    let foreign_troops = match to_long(Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(1.0)) {
        1 => __content.string(4393),
        2 => __content.string(4394),
        3 => __content.string(4395),
        4 => __content.string(4396),
        5 => __content.string(4397),
        _ => __content.string(4398),
    };
    let (oil_nation, famine_region) = match to_long(Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(1.0)) {
        1 => ("LIBYAN", "VIETNAM"),
        2 => ("NIGERIAN", __content.string(4399)),
        3 => ("KUWAITI", "SIBERIA"),
        _ => ("IRAQI", "THAILAND"),
    };
    loop {
        if g.rand_seed > 10 {
            g.rand_seed -= 1;
        } else {
            g.rand_seed = 32000;
        }
        g.news_msg = s(__content.string(4400)) + &price_text + __content.string(4401);
        let last = g.last_oil_crisis_headline.clone();
        if g.rand_seed % 9 == 8 {
            g.news_msg = s(__content.string(4402));
        }
        if mid(last.as_bytes(), 1, 7) == "EXPERTS" {
            g.rand_seed -= 1;
        }
        if g.rand_seed % 9 == 7 {
            g.news_msg = s(defender_nation) + __content.string(4403) + foreign_troops;
        }
        if g.rand_seed % 9 == 6 {
            g.news_msg = s(__content.string(4404)) + chaos_region + __content.string(4405);
        }
        if g.rand_seed % 9 == 5 {
            g.news_msg = s(__content.string(4406)) + famine_region + __content.string(4407);
        }
        if mid(last.as_bytes(), 1, 6) == "HUNGRY" {
            g.rand_seed -= 1;
        }
        if g.rand_seed % 9 == 4 {
            g.news_msg = s(__content.string(4408));
        }
        if mid(last.as_bytes(), 1, 7) == "FAMINES" {
            g.rand_seed -= 1;
        }
        if g.rand_seed % 9 == 3 {
            g.news_msg = s(__content.string(4409));
        }
        if 0 < instr(1, last.as_bytes(), b"COLLAPSE") {
            g.rand_seed -= 1;
        }
        if g.rand_seed % 9 == 2 {
            g.news_msg = s(__content.string(4410)) + defender_nation + __content.string(4411);
        }
        if 0 < instr(1, last.as_bytes(), __content.bytes(4412)) {
            g.rand_seed -= 1;
        }
        if g.rand_seed % 9 == 1 {
            g.news_msg = s(foreign_troops) + __content.string(4413) + oil_nation + __content.string(4414);
        }
        if Ext::from_f64(8.0) * rng.rnd() < Ext::from_f64(1.0) && Ext::from_f64(6.0) > cast::<Ext, _>(g.oil_crisis_level) {
            g.news_msg = s(__content.string(4415));
            g.oil_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.7f32) * cast::<Ext, _>(g.oil_target_price)));
            g.oil_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_price) * sf(0.68f32)));
        }
        if !g.news_msg.is_empty() {
            break;
        }
    }
    if g.oil_crisis_stage == 1 {
        if g.news_msg.is_empty() || g.rand_seed % 5 == 2 {
            g.news_msg = s(__content.string(4416));
        }
        if g.rand_seed % 5 == 3 {
            g.news_msg = s(__content.string(4417));
        }
        if g.rand_seed % 5 == 4 {
            g.news_msg = s(__content.string(4418));
        }
    }
    g.last_oil_crisis_headline = g.news_msg.clone();
    post_news(g, ui, 0, 0);
    if 3 < g.oil_crisis_stage && g.oil_crisis_stage < 18 && Ext::from_f64(10.0) * rng.rnd() > Ext::from_f64(9.0) {
        g.oil_price = cast::<f32, _>(cast::<f32, _>(sf(0.79f32) * cast::<Ext, _>(g.oil_price)));
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.6f32) * cast::<Ext, _>(g.oil_target_price)));
        g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * cast::<Ext, _>(g.oil_crisis_level)));
        g.oil_crisis_stage = 18;
    }
}

/// Port of CountryDisasterShock (0x539f98).
/// A disaster in country `country_idx` (0..52): every active entity of that country loses margin (-12
/// points below industry 70) and three times its market cap in cash (an extraordinary loss) and 15 % of
/// its market cap; companies (>= 131) with business assets of at least `100 * ExchangeRate` also lose
/// 10..25 % of them (at least `100 * ExchangeRate`) and another 15 % of market cap. The price history of
/// every hit entity is updated (`UpdatePriceHistoryTick`).
pub fn country_disaster_shock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, country_idx: i32) {
    if !(0..=52).contains(&country_idx) {
        return;
    }
    for entity_id in 11..=1600 {
        if g.entity_status[entity_id] != 1 || country_idx != g.country_idx[entity_id] {
            continue;
        }
        if g.industry_num[entity_id] < 70 {
            g.profit_margin[entity_id] = g.profit_margin[entity_id].wrapping_sub(12);
        }
        let loss = g.market_cap[entity_id] * Ext::from_f64(3.0);
        g.cash[entity_id] = g.cash[entity_id] - loss;
        g.extraordinary_pl[entity_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[entity_id]) - loss);
        g.market_cap[entity_id] = g.market_cap[entity_id] * sf(0.85f32);
        if entity_id < 131 {
            update_price_history_tick(g, ui, rng, entity_id);
        }
        if entity_id < 131 {
            continue;
        }
        let floor = g.exchange_rate * Ext::from_f64(100.0);
        if floor <= g.business_assets[entity_id] {
            let mut asset_loss = g.business_assets[entity_id] * (rng.rnd() * sf(0.15f32) + sf(0.1f32));
            if asset_loss < g.exchange_rate * Ext::from_f64(100.0) {
                asset_loss = g.exchange_rate * Ext::from_f64(100.0);
            }
            g.business_assets[entity_id] = g.business_assets[entity_id] - asset_loss;
            g.extraordinary_pl[entity_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[entity_id]) - asset_loss);
            g.market_cap[entity_id] = g.market_cap[entity_id] * sf(0.85f32);
            update_price_history_tick(g, ui, rng, entity_id);
        }
    }
}

/// Port of TechBubbleCycle (0x482549) with its GOSUB Bubble_StageNews (0x4831b8).
/// Runs one stage of a tech (`BubbleState` 100..120) or biotech (200..220) stock bubble: sets the
/// sector's growth forecasts, growth rates, demand and long-term growth from a stage-dependent base,
/// posts stage news and advances `BubbleState` (-1 after stage 120). In small games (`SimCoCount <
/// 1400`) only the stage news runs.
pub fn tech_bubble_cycle(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if g.bubble_state < 100 {
        return;
    }
    let mut stage = g.bubble_state;
    let mut sector_name = s("Tech");
    if stage >= 200 {
        stage -= 100;
        sector_name = s("Biotech");
    }
    let lsec = lcase(sector_name.as_bytes(), CaseMode::Ansi);
    if g.sim_co_count < 1400 {
        bubble_stage_news(g, ui, rng, stage, &sector_name);
        if 211 < g.bubble_state && g.bubble_state < 218 {
            g.bubble_state += 1;
        }
        if 100 < g.bubble_state {
            return;
        }
    }
    let mut growth_base: i16 = 0;
    match stage {
        100 => growth_base = to_integer(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(25.0)),
        101 => {
            growth_base = to_integer(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(30.0));
            g.news_msg = s(__content.string(4419)) + &lsec + __content.string(4420);
        }
        102 => {
            growth_base = to_integer(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(34.0));
            g.news_msg = s(__content.string(4421));
            if Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
                g.news_msg = s(__content.string(4422));
            }
            g.news_msg = g.news_msg.clone() + __content.string(4423) + &lsec + __content.string(4424);
        }
        103 => {
            growth_base = to_integer(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(38.0));
            g.news_msg = sector_name.clone() + __content.string(4425);
            if Ext::from_f64(5.0) * rng.rnd() > Ext::from_f64(3.0) {
                g.news_msg = s(__content.string(4426)) + &lsec + __content.string(4427);
            }
        }
        104 => {
            growth_base = to_integer(Ext::from_f64(8.0) * rng.rnd() + Ext::from_f64(42.0));
            g.news_msg = sector_name.clone() + __content.string(4428);
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                g.news_msg = sector_name.clone() + __content.string(4429);
            }
        }
        105 => {
            g.news_msg = s(__content.string(4430));
            if sector_name == "Tech" {
                g.news_msg = s(__content.string(4431));
            }
            growth_base = to_integer(Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(45.0));
        }
        106 => {
            g.news_msg = s(__content.string(4432)) + &lsec + __content.string(4424);
            if Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
                g.news_msg = s(__content.string(4433)) + &lsec + __content.string(4424);
            }
            growth_base = to_integer(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(25.0));
        }
        107 => {
            growth_base = to_integer(Ext::from_f64(8.0) * rng.rnd());
            g.news_msg = s(__content.string(4434)) + &lsec + __content.string(4435);
            if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
                g.news_msg = s(__content.string(4436)) + &lsec + __content.string(4435);
            }
        }
        108 | 109 => growth_base = to_integer(-Ext::from_f64(20.0) + Ext::from_f64(10.0) * rng.rnd()),
        110 => growth_base = -5,
        111..=120 => {
            growth_base = cast::<i16, _>(stage - 108);
            if stage == 118 {
                g.news_msg = s(__content.string(4437)) + &lsec + __content.string(4438);
            }
        }
        _ => {}
    }
    if 103 < stage && Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
        g.news_msg = PbStr::new();
    }
    if 100 <= stage && stage < 107 && sf(4.5f32) > cast::<Ext, _>(g.gdp_growth_target) {
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) + Ext::from_f64(1.0)));
    }
    if !g.news_msg.is_empty() {
        if g.suppress_popups == 0 {
            quick_news(g, ui);
        }
        post_news(g, ui, 0, 0);
    }
    let gb = cast::<Ext, _>(growth_base);
    for industry_id in 4..=61i32 {
        let hit = if g.bubble_state < 200 {
            matches!(industry_id, 10 | 24 | 26 | 30 | 35 | 37 | 39 | 40 | 61)
        } else {
            industry_id == 4 || industry_id == 21
        };
        if !hit {
            continue;
        }
        if growth_base < 0 {
            let k = if g.bubble_state < 200 { Ext::from_f64(7.0) } else { Ext::from_f64(8.0) };
            g.ind_long_term_growth[industry_id] = cast::<i16, _>(to_quad(k * rng.rnd()));
        }
        g.ind_growth_forecast[industry_id] = cast::<i16, _>(to_quad((gb - Ext::from_f64(4.0)) + Ext::from_f64(8.0) * rng.rnd()));
        if 25 < growth_base && gb * sf(0.6f32) > cast::<Ext, _>(g.ind_growth_rate[industry_id]) {
            g.ind_growth_rate[industry_id] = g.ind_growth_rate[industry_id].wrapping_add(10);
        }
        if stage == 108 || stage == 109 {
            g.ind_demand[industry_id] = to_quad(sf(0.85f32) * cast::<Ext, _>(g.ind_demand[industry_id]));
        }
        if 110 < stage {
            g.ind_long_term_growth[industry_id] = cast::<i16, _>(to_quad((gb - Ext::from_f64(2.0)) + Ext::from_f64(4.0) * rng.rnd()));
        }
    }
    g.bubble_state += 1;
    if Ext::from_f64(6.0) * rng.rnd() < Ext::from_f64(1.0) && stage < 113 {
        g.bubble_state += 5;
    }
    g.econ_cycle_counter = g.econ_cycle_counter.wrapping_sub(2);
    if stage == 120 {
        g.bubble_state = -1;
    }
}

/// GOSUB Bubble_StageNews (0x4831b8) of TechBubbleCycle: stage news (stage 102: the bubble story
/// 490/491 of MSCNEWS.DAT once, `BubbleStartFired`); posts `NewsMsg` (whatever it holds, also a
/// headline left by the caller) with a popup in 1 of 3 new stages.
fn bubble_stage_news(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, stage: i32, sector_name: &PbStr) {
    let __content = crate::content::current();
    let tail = match stage {
        102 => {
            let story = if *sector_name == "Tech" { 491 } else { 490 };
            if g.bubble_start_fired == 0 {
                // the text argument is the caller's (zero) LONG local passed as a STRING: ""
                show_misc_news_story(g, ui, rng, story, b"");
            }
            g.bubble_start_fired = 1;
            None
        }
        105 => Some(__content.string(4439)),
        107 => Some(__content.string(4440)),
        108 => Some(__content.string(4441)),
        109 => Some(__content.string(4442)),
        110 => Some(__content.string(4443)),
        111 => Some(__content.string(4444)),
        112 => Some(__content.string(4445)),
        113 => Some(__content.string(4446)),
        117 => Some(__content.string(4447)),
        _ => None,
    };
    if let Some(t) = tail {
        g.news_msg = sector_name.clone() + t;
    }
    if !g.news_msg.is_empty() {
        if g.suppress_popups == 0 && stage != g.last_bubble_news_stage && Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
            quick_news(g, ui);
        }
        post_news(g, ui, 0, 0);
        g.last_bubble_news_stage = stage;
    }
}

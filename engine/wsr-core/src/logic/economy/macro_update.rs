//! The yearly/quarterly macro update `UpdateEconomy` (0x458a7a: rates, GDP, holding-company tax),
//! `UpdateInterestRates` (0x4af1d2) with its GOSUB `InterestRates_AvgBankROE` (0x4af90b),
//! `UpdateLoanLimitMultiple` (0x4bd563), `GovtTaxPolicyEvents` (0x45d2c8) and the helper
//! `SweepPlayerCashToLoan` (0x508f5c).
//!
//! All float expressions are evaluated in EXT (f64 here) like the x87 code; SINGLE globals are rounded
//! with `as f32` on every store. Constants of the original's pool are SINGLE (`sf(..)`) or INTEGER.

// Statements keep the original BASIC shape on purpose.
#![allow(clippy::manual_clamp)]
use crate::pb::real::{Ext, cast};


use super::*;
use crate::logic::finance::calc_gov_bond_yields;
use crate::logic::futures::update_metals_price_base;
use crate::logic::valuation::{price_pct_change_months, price_trend_direction, price_trend_streak};
use crate::pb::numeric::str_num as str_num_digits;
use crate::pb::string::{parse_any, replace as replace_str};

/// `IndGrowthForecast(i)` as EXT.
fn igf(g: &Globals, i: i32) -> Ext {
    cast::<Ext, _>(g.ind_growth_forecast[i])
}

/// `IndGrowthForecast(i) = IndGrowthForecast(i) + d` (INTEGER arithmetic, wraps like the 16-bit store).
fn add_igf(g: &mut Globals, i: i32, d: i32) {
    g.ind_growth_forecast[i] = cast::<i16, _>((cast::<i32, _>(g.ind_growth_forecast[i])).wrapping_add(d));
}

/// `IndGrowthForecast(i) = ROUND(x)` through a QUAD temporary (`fistp qword`, low word stored).
fn set_igf(g: &mut Globals, i: i32, x: Ext) {
    g.ind_growth_forecast[i] = cast::<i16, _>(to_quad(x));
}

/// `STR$(x)` of a LONG (PB_StrNum).
fn str_long(x: i32) -> PbStr {
    PbStr::from(str_num_digits(cast::<Ext, _>(x), 15).as_str())
}

/// Port of UpdateEconomy (0x458a7a).
/// The periodic macro update: moves `BaseInterestRate` toward `InterestRateTarget` (randomly; the target
/// itself drifts when they are close), runs `UpdateIndustryOutlook` in a running game, adjusts the
/// growth forecast of industry 37 by the stock-index trend and — only when no game is in progress
/// (`GameInProgress <> 1`) — the growth forecasts of many industries from oil, metals, grain and
/// housing conditions, interest rates and GDP. Then: quantitative easing / end of tight money in a
/// deep recession with high rates, `MarketValTarget` / `MarketValuation` from GDP, the index trend and
/// `MonetaryStance`, and the personal-holding-company excise tax (introduction, increases, repeal) with
/// news (`PostNews`, ShowQuickMessage, MSCNEWS story 474).
///
/// UpdateMetalsPriceBase is called (and re-evaluated) for every metals test as in the original. The
/// ON ERROR handler only shows ShowRuntimeError.
pub fn update_economy(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if g.yr_holding_co_tax_chg == 0 {
        g.yr_holding_co_tax_chg = g.cur_year - 2;
    }
    let mut delta: Ext = crate::pb::numeric::double(cast::<Ext, _>(g.interest_rate_target) - cast::<Ext, _>(g.base_interest_rate)); // DOUBLE local
    if delta.abs() < sf(0.5f32) * rng.rnd() {
        g.interest_rate_target = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(1.0) - Ext::from_f64(2.0) * rng.rnd()) + cast::<Ext, _>(g.interest_rate_target)));
    }
    delta = crate::pb::numeric::double(delta / Ext::from_f64(7.0));
    if g.rand_seed % 2 != 1 {
        if Ext::from_f64(9.0) * rng.rnd() > Ext::from_f64(1.0) {
            if Ext::from_f64(1.0) < delta {
                delta = crate::pb::numeric::double(Ext::from_f64(1.0));
            }
            if sf(0.5f32) > delta {
                if Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
                    delta = crate::pb::numeric::double(Ext::from_f64(2.0) * delta);
                }
            }
            g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.base_interest_rate)));
        } else {
            g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.base_interest_rate) - Ext::from_f64(2.0) * delta));
        }
    }
    if g.game_in_progress == 1 {
        update_industry_outlook(g, ui, rng);
    }
    let trend = price_trend_direction(g, 0, 4, 1);
    if 0 < trend {
        if Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(5.0) > igf(g, 37) {
            let x = Ext::from_f64(5.0) * rng.rnd() + igf(g, 37);
            set_igf(g, 37, x);
            g.ind_growth_rate[37] = g.ind_growth_rate[37].wrapping_add(1);
        }
    } else if trend < 0 {
        if Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(15.0) < igf(g, 37) {
            let x = igf(g, 37) - Ext::from_f64(5.0) * rng.rnd();
            set_igf(g, 37, x);
            g.ind_growth_rate[37] = g.ind_growth_rate[37].wrapping_sub(1);
        }
    }
    if g.game_in_progress != 1 {
        industry_forecasts(g, ui, rng);
    }
    // LAB_0045be73
    let bir = cast::<Ext, _>(g.base_interest_rate);
    let gdp = cast::<Ext, _>(g.gdp_growth);
    if Ext::from_f64(5.0) < cast::<Ext, _>(g.interest_rate_target) && Ext::from_f64(5.0) < bir && -Ext::from_f64(3.0) > gdp && Ext::from_f64(13.0) < gdp.abs() + bir {
        if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
            if 0 < g.monetary_policy {
                g.monetary_policy -= 1;
                g.news_msg = g.central_bank_name.clone() + __content.string(4555);
            } else if g.monetary_policy == 0 {
                g.monetary_policy -= 1;
                g.news_msg = g.central_bank_name.clone() + __content.string(4556);
            }
            post_news(g, ui, 0, 0);
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.interest_rate_target) - (Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(2.0))));
            if sf(1.5f32) > cast::<Ext, _>(g.interest_rate_target) {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(1.5f32));
            }
        }
    }
    if sf(0.5f32) > g.market_val_target {
        g.market_val_target = cast::<f64, _>(sf(0.5f32));
    } else if sf(1.5f32) < g.market_val_target {
        g.market_val_target = cast::<f64, _>(sf(1.5f32));
    }
    if Ext::from_f64(0.0) > cast::<Ext, _>(g.gdp_growth) && sf(0.7f32) < g.market_val_target {
        g.market_val_target = cast::<f64, _>(g.market_val_target - sf(0.05f32));
    } else if sf(4.8f32) < cast::<Ext, _>(g.gdp_growth)
        && sf(1.35f32) > g.market_val_target
        && sf(9.3f32) > cast::<Ext, _>(g.base_interest_rate)
    {
        g.market_val_target = cast::<f64, _>(sf(0.05f32) + g.market_val_target);
    }
    let trend = price_trend_streak(g, 0);
    let mut price_pct_chg = price_pct_change_months(g, 0, 1);
    if price_pct_chg == 0 {
        if price_pct_change_months(g, 0, 6) > 20 {
            price_pct_chg = price_pct_change_months(g, 0, 6);
        }
    }
    if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(5.0) < cast::<Ext, _>(trend) {
        if !(sf(0.6f32) > g.market_val_target) {
            g.market_val_target = cast::<f64, _>(g.market_val_target - sf(0.1f32));
            if sf(0.75f32) < cast::<Ext, _>(g.market_valuation) {
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.8f32) * cast::<Ext, _>(g.market_valuation)));
            }
        }
        if -Ext::from_f64(6.0) - Ext::from_f64(7.0) * rng.rnd() > cast::<Ext, _>(g.gdp_growth) && sf(0.7f32) < g.market_val_target {
            g.market_val_target = cast::<f64, _>(g.market_val_target - sf(0.1f32));
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.market_valuation) - sf(0.1f32)));
            if -sf(10.5f32) > cast::<Ext, _>(g.gdp_growth) && !(sf(0.6f32) > g.market_val_target) {
                g.market_val_target = cast::<f64, _>(g.market_val_target - sf(0.1f32));
            }
        }
    } else if -Ext::from_f64(5.0) - Ext::from_f64(5.0) * rng.rnd() > cast::<Ext, _>(trend) {
        if sf(0.9f32) > g.market_val_target {
            g.market_val_target = cast::<f64, _>(sf(1.25f32) * g.market_val_target);
            if sf(1.18f32) > cast::<Ext, _>(g.market_valuation) {
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.13f32) * cast::<Ext, _>(g.market_valuation)));
            }
        } else if sf(1.3f32) > g.market_val_target {
            g.market_val_target = cast::<f64, _>(sf(1.25f32));
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.05f32) + cast::<Ext, _>(g.market_valuation)));
        }
    } else {
        let lim = cast::<Ext, _>(rng.rnd_range(1, 4)) + Ext::from_f64(1.0);
        if (cast::<Ext, _>(price_pct_chg)).abs() > lim {
            if 0 < price_pct_chg && sf(0.8f32) < g.market_val_target {
                g.market_val_target = cast::<f64, _>(sf(0.8f32) * g.market_val_target);
            }
            if price_pct_chg < 0 && sf(1.2f32) > g.market_val_target {
                g.market_val_target = cast::<f64, _>(sf(1.25f32) * g.market_val_target);
            }
        } else {
            if Ext::from_f64(1.0) < g.market_val_target {
                g.market_val_target = cast::<f64, _>(sf(0.99f32) * g.market_val_target);
            }
            if Ext::from_f64(1.0) > g.market_val_target {
                g.market_val_target = cast::<f64, _>(sf(1.01f32) * g.market_val_target);
            }
        }
    }
    if g.monetary_stance == 1 {
        g.market_val_target = cast::<f64, _>(sf(0.7f32));
    } else if g.monetary_stance == -1 {
        g.market_val_target = cast::<f64, _>(sf(1.3f32));
    }
    if Ext::from_f64(8.0) < cast::<Ext, _>(g.base_interest_rate) && g.start_year + 5 < g.cur_year && g.holding_co_tax_rate == 0 {
        if Ext::from_f64(7.0) * rng.rnd() > Ext::from_f64(5.0) || g.difficulty_level < 3 {
            g.holding_co_tax_rate = -1;
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_target_price) * sf(0.9f32)));
        } else {
            g.holding_co_tax_rate = rng.rnd_range(1, 4);
            let tax_rate_text = str_long(g.holding_co_tax_rate);
            g.news_msg = s(__content.string(4557));
            post_news(g, ui, 0, 0);
            show_misc_news_story(g, ui, rng, 474, tax_rate_text.as_bytes());
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(2.0)));
            g.yr_holding_co_tax_chg = g.cur_year;
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_target_price) * sf(1.1f32)));
        }
    }
    let bir = cast::<Ext, _>(g.base_interest_rate);
    if Ext::from_f64(8.0) < bir
        && sf(9.75f32) > bir
        && g.yr_holding_co_tax_chg + 3 < g.cur_year
        && 0 < g.holding_co_tax_rate
        && g.holding_co_tax_rate < 4
    {
        g.holding_co_tax_rate += 1;
        g.yr_holding_co_tax_chg = g.cur_year;
        g.news_msg = s(__content.string(4558)) + &str_long(g.holding_co_tax_rate) + "%.";
        if Ext::from_f64(4.0) * rng.rnd() < Ext::from_f64(1.0) {
            g.holding_co_tax_rate = -1;
            g.news_msg = s(__content.string(4559));
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_target_price) * sf(0.9f32)));
        } else {
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_target_price) * sf(1.1f32)));
        }
        crate::logic::report::show_quick_news(g, ui);
        post_news(g, ui, 0, 0);
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(2.0)));
    }
}

/// UpdateEconomy, 0x458d84..0x45be73: the industry growth forecasts (only run when no game is in
/// progress).
fn industry_forecasts(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let gdp = |g: &Globals| cast::<Ext, _>(g.gdp_growth);
    // oil
    if sf(0.2f32) * rng.rnd() + sf(1.2f32) < cast::<Ext, _>(g.oil_price) && sf(1.25f32) < cast::<Ext, _>(g.oil_target_price) {
        if Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(20.0) > igf(g, 7) {
            add_igf(g, 7, 1);
        }
        if Ext::from_f64(9.0) * rng.rnd() + Ext::from_f64(24.0) > igf(g, 8) {
            add_igf(g, 8, 1);
        }
        if Ext::from_f64(9.0) * rng.rnd() + Ext::from_f64(12.0) > igf(g, 50) {
            add_igf(g, 50, 1);
        }
        if sf(3.5f32) > gdp(g) && Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth_target) {
            for (ind, k, c) in [
                (3, Ext::from_f64(3.0), Ext::from_f64(3.0)),
                (11, Ext::from_f64(4.0), Ext::from_f64(4.0)),
                (15, Ext::from_f64(5.0), Ext::from_f64(8.0)),
                (16, Ext::from_f64(9.0), Ext::from_f64(8.0)),
                (23, Ext::from_f64(9.0), Ext::from_f64(8.0)),
                (25, Ext::from_f64(9.0), Ext::from_f64(6.0)),
                (52, Ext::from_f64(9.0), Ext::from_f64(8.0)),
                (53, Ext::from_f64(9.0), Ext::from_f64(5.0)),
                (67, Ext::from_f64(9.0), Ext::from_f64(3.0)),
                (68, Ext::from_f64(9.0), Ext::from_f64(4.0)),
            ] {
                if k * rng.rnd() + c < igf(g, ind) {
                    add_igf(g, ind, -1);
                }
            }
            if Ext::from_f64(1.0) > gdp(g) {
                if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(6.0) < igf(g, 59) {
                    add_igf(g, 59, -1);
                }
            }
        }
    } else if sf(0.5f32) * rng.rnd() + sf(0.5f32) > cast::<Ext, _>(g.oil_price) && sf(0.82f32) > cast::<Ext, _>(g.oil_target_price) {
        if Ext::from_f64(12.0) * rng.rnd() + -Ext::from_f64(12.0) < igf(g, 7) {
            add_igf(g, 7, -1);
            if sf(3.3f32) > gdp(g) {
                add_igf(g, 7, -5);
            }
        }
        if Ext::from_f64(9.0) * rng.rnd() + -Ext::from_f64(9.0) < igf(g, 8) {
            add_igf(g, 8, -1);
            if sf(3.7f32) > gdp(g) {
                add_igf(g, 8, -7);
            }
        }
        if Ext::from_f64(9.0) * rng.rnd() + -Ext::from_f64(12.0) < igf(g, 50) && Ext::from_f64(2.0) < gdp(g) {
            add_igf(g, 50, -1);
        }
        if sf(1.2f32) < gdp(g) && sf(2.3f32) < cast::<Ext, _>(g.gdp_growth_target) {
            for (ind, k, c) in [
                (3, Ext::from_f64(3.0), -Ext::from_f64(4.0)),
                (11, Ext::from_f64(4.0), -Ext::from_f64(5.0)),
                (15, Ext::from_f64(3.0), -Ext::from_f64(7.0)),
                (16, Ext::from_f64(9.0), -Ext::from_f64(8.0)),
                (23, Ext::from_f64(2.0), -Ext::from_f64(4.0)),
                (25, Ext::from_f64(8.0), -Ext::from_f64(6.0)),
                (52, Ext::from_f64(5.0), -Ext::from_f64(4.0)),
                (53, Ext::from_f64(6.0), -Ext::from_f64(4.0)),
                (67, Ext::from_f64(3.0), -Ext::from_f64(4.0)),
                (68, Ext::from_f64(4.0), -Ext::from_f64(6.0)),
            ] {
                if k * rng.rnd() + c > igf(g, ind) {
                    add_igf(g, ind, 1);
                }
            }
            if sf(4.7f32) < gdp(g) {
                if Ext::from_f64(6.0) * rng.rnd() + -Ext::from_f64(7.0) > igf(g, 59) {
                    add_igf(g, 59, 1);
                }
                if Ext::from_f64(5.0) * rng.rnd() + -Ext::from_f64(1.0) < igf(g, 50) {
                    add_igf(g, 50, -2);
                }
            }
        }
    }
    // grain, GDP-driven industries
    if sf(8.3f32) < cast::<Ext, _>(g.wheat_price) && Ext::from_f64(7.0) < cast::<Ext, _>(g.corn_price) && 6 < g.ind_growth_forecast[28] {
        add_igf(g, 28, -1);
    }
    if sf(3.8f32) > cast::<Ext, _>(g.wheat_price) && sf(3.3f32) > cast::<Ext, _>(g.corn_price) && g.ind_growth_forecast[28] < 7 {
        add_igf(g, 28, 1);
    }
    if 5 < g.ind_growth_forecast[35] && Ext::from_f64(0.0) > gdp(g) {
        add_igf(g, 35, -1);
    }
    if g.ind_growth_forecast[35] < 8 && Ext::from_f64(0.0) < gdp(g) {
        add_igf(g, 35, 1);
    }
    if Ext::from_f64(0.0) < gdp(g) {
        if gdp(g) * Ext::from_f64(2.0) > igf(g, 36) {
            add_igf(g, 36, 1);
        }
    }
    if !(Ext::from_f64(0.0) < gdp(g)) {
        if -4 < g.ind_growth_forecast[36] {
            add_igf(g, 36, -1);
        }
    }
    if sf(2.7f32) < gdp(g) && 9 < g.ind_growth_forecast[48] {
        add_igf(g, 48, -1);
    }
    if sf(0.8f32) > gdp(g) && -Ext::from_f64(5.0) < gdp(g) && g.ind_growth_forecast[48] < 10 {
        add_igf(g, 48, 1);
    }
    if sf(0.5f32) > gdp(g) && -Ext::from_f64(6.0) < gdp(g) && g.ind_growth_forecast[55] < 5 {
        add_igf(g, 55, 1);
    }
    if sf(7.1f32) < cast::<Ext, _>(g.wheat_price) && Ext::from_f64(6.0) < cast::<Ext, _>(g.corn_price) && 2 < g.ind_growth_forecast[57] {
        let x = igf(g, 57) - Ext::from_f64(7.0) * rng.rnd();
        set_igf(g, 57, x);
    }
    if sf(4.2f32) > cast::<Ext, _>(g.wheat_price) && Ext::from_f64(4.0) > cast::<Ext, _>(g.corn_price) && g.ind_growth_forecast[57] < 7 {
        let x = Ext::from_f64(4.0) * rng.rnd() + igf(g, 57);
        set_igf(g, 57, x);
    }
    let mut avg: Ext = crate::pb::numeric::double((igf(g, 8) + igf(g, 7)) / Ext::from_f64(2.0)); // DOUBLE local
    avg = crate::pb::numeric::double(Ext::from_f64(1.0) + avg);
    if Ext::from_f64(6.0) + avg < igf(g, 63) {
        add_igf(g, 63, -1);
    }
    if avg - Ext::from_f64(6.0) > igf(g, 63) {
        add_igf(g, 63, 1);
    }
    if g.ind_growth_forecast[46] < -4 {
        add_igf(g, 46, 3);
    }
    if g.ind_growth_forecast[28] < -3 {
        add_igf(g, 28, 2);
    }
    for (ind, up, down) in [(18, Ext::from_f64(5.0), Ext::from_f64(3.0)), (33, Ext::from_f64(5.0), Ext::from_f64(2.0)), (42, Ext::from_f64(5.0), Ext::from_f64(3.0)), (47, Ext::from_f64(3.0), Ext::from_f64(3.0))] {
        if up + gdp(g) < igf(g, ind) {
            add_igf(g, ind, -1);
        }
        if gdp(g) - down > igf(g, ind) {
            add_igf(g, ind, 1);
        }
    }
    if sf(1.25f32) > cast::<Ext, _>(g.oil_price) && sf(2.5f32) < gdp(g) && Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(9.0) < igf(g, 50) {
        let x = igf(g, 50) - gdp(g);
        set_igf(g, 50, x);
    } else if sf(0.75f32) < cast::<Ext, _>(g.oil_price) && Ext::from_f64(0.0) > gdp(g) {
        if Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(2.0) > igf(g, 50) {
            let x = Ext::from_f64(4.0) * rng.rnd() + igf(g, 50);
            set_igf(g, 50, x);
        }
    }
    // metals
    if update_metals_price_base(g, ui) > Ext::from_f64(2100.0) {
        if Ext::from_f64(8.0) * rng.rnd() + Ext::from_f64(10.0) > igf(g, 50) {
            add_igf(g, 50, 3);
        }
        if update_metals_price_base(g, ui) > Ext::from_f64(2550.0) {
            if update_metals_price_base(g, ui) / Ext::from_f64(100.0) > igf(g, 50) {
                let x = (Ext::from_f64(10.0) + igf(g, 50)) + Ext::from_f64(5.0) * rng.rnd();
                set_igf(g, 50, x);
            }
        }
    } else if update_metals_price_base(g, ui) < Ext::from_f64(1350.0) {
        if Ext::from_f64(10.0) * rng.rnd() + -Ext::from_f64(15.0) < igf(g, 50) {
            add_igf(g, 50, -3);
        }
        if update_metals_price_base(g, ui) < Ext::from_f64(1080.0) && -1 < g.ind_growth_forecast[50] {
            let x = igf(g, 50) - (Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(5.0));
            set_igf(g, 50, x);
        }
        if update_metals_price_base(g, ui) < Ext::from_f64(875.0) && 0 < g.ind_growth_forecast[50] {
            let x = sf(0.6f32) * igf(g, 50);
            set_igf(g, 50, x);
        }
    }
    let silver_net = |g: &Globals| cast::<Ext, _>(g.silver_price) - cast::<Ext, _>(g.metals_price_base) * sf(0.01f32);
    if silver_net(g) > Ext::from_f64(29.0) {
        if Ext::from_f64(8.0) * rng.rnd() + Ext::from_f64(10.0) > igf(g, 50) {
            let x = (Ext::from_f64(5.0) + igf(g, 50)) + Ext::from_f64(5.0) * rng.rnd();
            set_igf(g, 50, x);
        }
        if silver_net(g) > Ext::from_f64(23.0) && cast::<Ext, _>(g.silver_price) > igf(g, 50) {
            add_igf(g, 50, 5);
        }
    } else if silver_net(g) < Ext::from_f64(17.0) {
        if Ext::from_f64(10.0) * rng.rnd() + -Ext::from_f64(15.0) < igf(g, 50) {
            let x = igf(g, 50) - (Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(3.0));
            set_igf(g, 50, x);
        }
        if silver_net(g) < sf(12.2f32) && 2 < g.ind_growth_forecast[50] {
            add_igf(g, 50, -7);
        }
    }
    if sf(6.8f32) < cast::<Ext, _>(g.wheat_price) {
        if Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(8.0) > igf(g, 15) {
            let x = sf(6.5f32) * rng.rnd() + igf(g, 15);
            set_igf(g, 15, x);
        }
        if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(3.0) < igf(g, 28) {
            let x = igf(g, 28) - sf(5.5f32) * rng.rnd();
            set_igf(g, 28, x);
        }
    } else if sf(4.4f32) > cast::<Ext, _>(g.wheat_price) {
        if Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(2.0) < igf(g, 15) {
            let x = igf(g, 15) - sf(6.5f32) * rng.rnd();
            set_igf(g, 15, x);
        }
        if Ext::from_f64(7.0) * rng.rnd() + -Ext::from_f64(2.0) > igf(g, 28) {
            let x = sf(5.5f32) * rng.rnd() + igf(g, 28);
            set_igf(g, 28, x);
        }
    }
    if sf(6.35f32) < cast::<Ext, _>(g.corn_price) {
        if Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(8.0) > igf(g, 15) {
            add_igf(g, 15, 2);
        }
        if Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(2.0) < igf(g, 28) {
            add_igf(g, 28, -2);
        }
    } else if sf(4.1f32) > cast::<Ext, _>(g.corn_price) {
        if Ext::from_f64(7.0) * rng.rnd() + -Ext::from_f64(12.0) > igf(g, 15) {
            add_igf(g, 15, -2);
        }
        if Ext::from_f64(6.0) * rng.rnd() + -Ext::from_f64(1.0) > igf(g, 28) {
            add_igf(g, 28, 2);
        }
    }
    // housing (5, 6, 60)
    let bir = |g: &Globals| cast::<Ext, _>(g.base_interest_rate);
    let irt = |g: &Globals| cast::<Ext, _>(g.interest_rate_target);
    if Ext::from_f64(200.0) * rng.rnd() + Ext::from_f64(1200.0) < cast::<Ext, _>(g.housing_starts) {
        if Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(20.0) > igf(g, 5) {
            add_igf(g, 5, 3);
            if sf(6.9f32) > bir(g) && sf(7.5f32) > irt(g) {
                add_igf(g, 5, 6);
            }
        }
        for ind in [6, 60] {
            if Ext::from_f64(9.0) * rng.rnd() + Ext::from_f64(24.0) > igf(g, ind) {
                add_igf(g, ind, 3);
                if sf(6.8f32) > bir(g) && sf(7.2f32) > irt(g) {
                    add_igf(g, ind, 5);
                } else if 17 < g.ind_growth_rate[5] && 15 < g.ind_growth_forecast[5] {
                    add_igf(g, ind, 1);
                }
            }
        }
    } else if Ext::from_f64(200.0) * rng.rnd() * Ext::from_f64(600.0) > cast::<Ext, _>(g.housing_starts) {
        // (sic) RND * 200 * 600: see BUGS.md
        if Ext::from_f64(12.0) * rng.rnd() + -Ext::from_f64(12.0) < igf(g, 5) {
            add_igf(g, 5, -3);
            if sf(8.8f32) < bir(g) && sf(9.4f32) < irt(g) {
                add_igf(g, 5, -7);
            }
        }
        for ind in [6, 60] {
            if Ext::from_f64(9.0) * rng.rnd() + -Ext::from_f64(9.0) < igf(g, ind) {
                add_igf(g, ind, -3);
                if Ext::from_f64(9.0) < bir(g) && sf(9.7f32) < irt(g) {
                    g.ind_growth_forecast[ind] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_forecast[5]) - 6);
                } else if g.ind_growth_rate[5] < -6 && g.ind_growth_forecast[5] < -4 {
                    add_igf(g, ind, -1);
                }
            }
        }
    } else {
        let trend = price_trend_streak(g, 1601);
        if sf(8.6f32) < bir(g) && sf(3.4f32) > gdp(g) && trend < -3 {
            if 3 < g.ind_growth_forecast[5] {
                let x = igf(g, 5) - sf(6.3f32) * rng.rnd();
                set_igf(g, 5, x);
            }
            if 4 < g.ind_growth_forecast[6] {
                let x = igf(g, 6) - Ext::from_f64(7.0) * rng.rnd();
                set_igf(g, 6, x);
            }
            if 2 < g.ind_growth_forecast[60] {
                let x = igf(g, 60) - Ext::from_f64(6.0) * rng.rnd();
                set_igf(g, 60, x);
            }
        } else if sf(5.9f32) > bir(g) && sf(0.2f32) < gdp(g) && 3 < trend {
            if g.ind_growth_forecast[5] < 13 {
                let x = sf(6.3f32) * rng.rnd() + igf(g, 5);
                set_igf(g, 5, x);
            }
            if g.ind_growth_forecast[6] < 14 {
                let x = Ext::from_f64(7.0) * rng.rnd() + igf(g, 6);
                set_igf(g, 6, x);
            }
            if g.ind_growth_forecast[60] < 12 {
                let x = Ext::from_f64(6.0) * rng.rnd() + igf(g, 60);
                set_igf(g, 60, x);
            }
        }
    }
    // interest rates (37, 60, 69)
    if sf(1.5f32) * rng.rnd() + sf(8.5f32) < bir(g) && sf(9.8f32) < irt(g) && -1 < g.monetary_policy {
        if Ext::from_f64(15.0) * rng.rnd() + Ext::from_f64(13.0) < igf(g, 37) {
            if Ext::from_f64(2.0) * rng.rnd() > gdp(g) {
                add_igf(g, 37, -1);
            }
        }
        if Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(5.0) < igf(g, 60) && sf(0.5f32) > gdp(g) {
            add_igf(g, 60, -1);
        }
        if Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(6.0) < igf(g, 69) {
            add_igf(g, 69, -1);
        }
    } else if sf(1.5f32) * rng.rnd() + Ext::from_f64(5.0) > bir(g) && sf(6.6f32) > irt(g) && g.monetary_policy < 1 {
        if Ext::from_f64(10.0) * rng.rnd() + -Ext::from_f64(8.0) > igf(g, 37) && sf(2.3f32) < gdp(g) {
            add_igf(g, 37, 1);
        }
        if Ext::from_f64(6.0) * rng.rnd() + -Ext::from_f64(6.0) > igf(g, 60) && sf(2.6f32) < gdp(g) {
            add_igf(g, 60, 1);
        }
        if Ext::from_f64(5.0) * rng.rnd() + -Ext::from_f64(3.0) > igf(g, 69) {
            add_igf(g, 69, 1);
        }
    }
    let sum = cast::<i32, _>(g.ind_growth_rate[67]) + cast::<i32, _>(g.ind_growth_rate[3]) + cast::<i32, _>(g.ind_growth_rate[68]);
    if 15 < sum && Ext::from_f64(3.0) < gdp(g) {
        if Ext::from_f64(5.0) * rng.rnd() + -Ext::from_f64(7.0) > igf(g, 12) {
            add_igf(g, 12, 1);
        }
    }
    if sum < 3 && sf(0.6f32) > gdp(g) {
        // (sic) RND + 4 + 4
        if (Ext::from_f64(4.0) + rng.rnd()) + Ext::from_f64(4.0) < igf(g, 12) {
            add_igf(g, 12, -1);
        }
    }
    for industry in 3..=68 {
        let do_adjust = !matches!(industry,
            4..=8 | 14..=17 | 19 | 21 | 22 | 26..=32 | 34 | 39 | 43 | 46 | 48 | 50 | 55..=57 | 61..=66);
        if !do_adjust {
            continue;
        }
        if g.ind_growth_rate_now[industry] < 2 {
            g.ind_growth_rate_now[industry] = 2;
        }
        let now = |g: &Globals| cast::<Ext, _>(g.ind_growth_rate_now[industry]);
        if sf(2.9f32) * rng.rnd() + -Ext::from_f64(2.0) > gdp(g) && Ext::from_f64(1.0) * rng.rnd() + -Ext::from_f64(5.0) < gdp(g) {
            if now(g) * sf(0.6f32) < igf(g, industry) && 0 < g.ind_growth_forecast[industry] {
                let x = sf(0.7f32) * igf(g, industry);
                set_igf(g, industry, x);
                let x = igf(g, industry) - Ext::from_f64(5.0) * rng.rnd();
                set_igf(g, industry, x);
            }
            continue;
        }
        if Ext::from_f64(1.0) * rng.rnd() + sf(4.2f32) < gdp(g) {
            if now(g) * sf(1.5f32) > igf(g, industry) && 0 < g.ind_growth_forecast[industry] {
                let x = sf(1.3f32) * igf(g, industry);
                set_igf(g, industry, x);
            } else if !(0 < g.ind_growth_forecast[industry]) {
                let x = now(g) * sf(0.5f32);
                set_igf(g, industry, x);
            }
            add_igf(g, industry, 1);
        } else if -Ext::from_f64(5.0) > gdp(g) {
            if !(0 < g.ind_growth_forecast[industry]) && -22 < g.ind_growth_forecast[industry] {
                add_igf(g, industry, -1);
            }
            if 0 < g.ind_growth_forecast[industry] && now(g) * sf(0.4f32) < igf(g, industry) {
                let x = sf(0.6f32) * igf(g, industry);
                set_igf(g, industry, x);
                let x = igf(g, industry) - Ext::from_f64(7.0) * rng.rnd();
                set_igf(g, industry, x);
            }
        }
    }
}

/// Port of UpdateInterestRates (0x4af1d2) with its GOSUB InterestRates_AvgBankROE (0x4af90b).
/// Runs when `SimCoCount MOD 35 = 0` (or `SofrRate` is not positive): sweeps players' cash into their
/// loans (`SweepPlayerCashToLoan`), updates the crypto prices, moves `BaseInterestRate` (noise, GDP,
/// toward `InterestRateTarget`, monetary policy) and the target (policy, corporate tax rate), derives
/// `BankCdRate` (rate bands, year/quarter term, average bank ROE), `SofrRate`, the government bond
/// yields (`CalcGovBondYields`, or fixed ratios without a long bond), `PrimeRate` (to a quarter point
/// when it is more than 0.365 off) and `HousingStarts`.
pub fn update_interest_rates(g: &mut Globals, rng: &mut Rng) {
    if g.sim_co_count % 35 != 0 && Ext::from_f64(0.0) < cast::<Ext, _>(g.sofr_rate) {
        return;
    }
    sweep_player_cash_to_loan(g);
    update_crypto_prices(g, rng);
    let bir0 = cast::<Ext, _>(g.base_interest_rate);
    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(bir0 * sf(0.98f32) + (sf(0.04f32) * rng.rnd()) * bir0));
    let mut gdp_clamped: f32 = cast::<f32, _>(g.gdp_growth); // SINGLE local
    if Ext::from_f64(7.0) < cast::<Ext, _>(gdp_clamped) {
        gdp_clamped = 7.0f32;
    }
    if -Ext::from_f64(1.0) > cast::<Ext, _>(gdp_clamped) {
        gdp_clamped = -1.0f32;
    }
    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(gdp_clamped) * sf(0.01f32) + cast::<Ext, _>(g.base_interest_rate)) - sf(0.03f32)));
    let irt = cast::<Ext, _>(g.interest_rate_target);
    if sf(3.3f32) < irt && sf(6.6f32) > irt {
        if g.monetary_policy == 0 {
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.interest_rate_target) - sf(0.04f32)));
        }
        if g.monetary_policy == 2 {
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.04f32) + cast::<Ext, _>(g.interest_rate_target)));
        }
    }
    let irt = cast::<Ext, _>(g.interest_rate_target);
    if Ext::from_f64(3.0) < irt && Ext::from_f64(7.0) > irt {
        if 42 < g.corp_tax_rate {
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.interest_rate_target) - sf(0.02f32)));
        }
        if g.corp_tax_rate < 28 {
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.02f32) + cast::<Ext, _>(g.interest_rate_target)));
        }
    }
    if Ext::from_f64(20.0) < cast::<Ext, _>(g.interest_rate_target) {
        g.interest_rate_target = cast::<f32, _>(Ext::from_f32(20.0f32));
    }
    if Ext::from_f64(20.0) < cast::<Ext, _>(g.base_interest_rate) {
        g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(19.0)));
    }
    if Ext::from_f64(2.5) > cast::<Ext, _>(g.interest_rate_target) {
        g.interest_rate_target = cast::<f32, _>(Ext::from_f32(2.5f32));
    }
    let mut rate_step: Ext = crate::pb::numeric::double(sf(0.05f32) * (cast::<Ext, _>(g.interest_rate_target) - cast::<Ext, _>(g.base_interest_rate))); // DOUBLE
    if Ext::from_f64(0.0) < rate_step {
        if sf(0.1f32) < rate_step {
            rate_step = crate::pb::numeric::double(sf(0.1f32));
        }
        if g.monetary_policy == 2 {
            g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(sf(0.03f32) + cast::<Ext, _>(g.base_interest_rate)));
        }
    } else {
        if rate_step.abs() > sf(0.1f32) {
            rate_step = crate::pb::numeric::double(-sf(0.1f32));
        }
        if g.monetary_policy == 0 {
            g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.base_interest_rate) - sf(0.03f32)));
        }
    }
    if Ext::from_f64(10.0) * rng.rnd() < Ext::from_f64(1.0) {
        rate_step = crate::pb::numeric::double(-Ext::from_f64(2.0) * rate_step);
    }
    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(rate_step + cast::<Ext, _>(g.base_interest_rate)));
    if Ext::from_f64(2.25) > cast::<Ext, _>(g.base_interest_rate) {
        g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.cur_quarter) * sf(0.15f32) + Ext::from_f64(2.0)));
    }
    let bir = cast::<Ext, _>(g.base_interest_rate);
    g.bank_cd_rate = cast::<f32, _>(cast::<f32, _>(bir * (sf(0.08f32) * rng.rnd() + sf(0.82f32))));
    for (lim, f) in [(Ext::from_f64(10.0), 0.96f32), (Ext::from_f64(12.0), 1.087f32), (Ext::from_f64(15.0), 1.03f32), (Ext::from_f64(17.0), 0.985f32), (Ext::from_f64(19.5), 0.97f32), (Ext::from_f64(22.0), 0.94f32)] {
        if lim < bir {
            g.bank_cd_rate = cast::<f32, _>(cast::<f32, _>(bir * sf(f)));
        }
    }
    for (lim, f) in [(8.0f32, 0.77f32), (7.2f32, 0.66f32), (6.5f32, 0.56f32)] {
        if sf(lim) > bir {
            g.bank_cd_rate = cast::<f32, _>(cast::<f32, _>(bir * sf(f)));
        }
    }
    if sf(5.5f32) > bir && Ext::from_f64(2.0) < bir {
        g.bank_cd_rate = cast::<f32, _>(cast::<f32, _>(sf(0.48f32) * (bir - Ext::from_f64(1.0))));
    }
    if sf(5.2f32) > bir && Ext::from_f64(2.5) < bir {
        g.bank_cd_rate = cast::<f32, _>(cast::<f32, _>(sf(0.41f32) * (bir - sf(1.5f32))));
    }
    if sf(4.9f32) > bir && sf(2.75f32) < bir {
        g.bank_cd_rate = cast::<f32, _>(cast::<f32, _>(sf(0.33f32) * (bir - sf(1.9f32))));
    }
    let term = cast::<f32, _>(fmod(cast::<Ext, _>(g.cur_year + g.cur_quarter), Ext::from_f64(4.0)) * sf(0.2f32)); // SINGLE local
    g.bank_cd_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(term) + cast::<Ext, _>(g.bank_cd_rate)));
    if sf(0.1f32) > cast::<Ext, _>(g.bank_cd_rate) {
        g.bank_cd_rate = cast::<f32, _>(Ext::from_f32(0.1f32));
    }
    // GOSUB InterestRates_AvgBankROE
    let mut cd_adjust: f32 = cast::<f32, _>(Ext::from_f32(0.0f32)); // SINGLE local
    let mut roe_sum: Ext = crate::pb::numeric::double(Ext::from_f64(0.0)); // DOUBLE
    let mut bank_count: i32 = 0;
    for bank_id in 11..=85 {
        if g.entity_status[bank_id] != 1 {
            continue;
        }
        if g.corp_prior_net_worth[bank_id] > Ext::from_f64(0.0) && g.earnings_y1[bank_id] != Ext::from_f64(0.0) {
            let mut bank_roe: Ext = (g.earnings_y1[bank_id] / g.corp_prior_net_worth[bank_id]) * Ext::from_f64(100.0);
            if Ext::from_f64(50.0) < bank_roe {
                bank_roe = Ext::from_f64(30.0);
            }
            if -Ext::from_f64(19.0) > bank_roe {
                bank_roe = -Ext::from_f64(5.0);
            }
            bank_count += 1;
            roe_sum = crate::pb::numeric::double(bank_roe + roe_sum);
        }
        if 0 < bank_count {
            let avg_roe: Ext = roe_sum / cast::<Ext, _>(bank_count);
            cd_adjust = if avg_roe < Ext::from_f64(10.0) {
                -0.5f32
            } else if avg_roe > Ext::from_f64(30.0) {
                1.25f32
            } else if avg_roe > Ext::from_f64(27.0) {
                1.05f32
            } else if avg_roe > Ext::from_f64(24.0) {
                0.9f32
            } else if avg_roe > Ext::from_f64(22.0) {
                0.8f32
            } else if avg_roe > Ext::from_f64(20.0) {
                0.7f32
            } else if avg_roe > Ext::from_f64(18.0) {
                0.5f32
            } else if avg_roe > Ext::from_f64(16.0) {
                0.3f32
            } else {
                0.0f32
            };
            if sf(3.8f32) > cast::<Ext, _>(g.bank_cd_rate) {
                cd_adjust = cast::<f32, _>(sf(1.2f32) + cast::<Ext, _>(cd_adjust));
            } else if sf(5.1f32) > cast::<Ext, _>(g.bank_cd_rate) {
                cd_adjust = cast::<f32, _>(sf(1.3f32) * cast::<Ext, _>(cd_adjust));
            }
        }
    }
    g.bank_cd_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(cd_adjust) + cast::<Ext, _>(g.bank_cd_rate)));
    let bir = cast::<Ext, _>(g.base_interest_rate);
    g.sofr_rate = cast::<f32, _>(cast::<f32, _>(((sf(0.01f32) * (bir + Ext::from_f64(110.0))) * cast::<Ext, _>(g.bank_cd_rate)) * sf(0.95f32)));
    if bir * sf(0.95f32) < cast::<Ext, _>(g.sofr_rate) {
        g.sofr_rate = cast::<f32, _>(cast::<f32, _>(bir * sf(0.93f32)));
    }
    if g.long_bond_desc.is_empty() {
        g.long_bond_yield = cast::<f32, _>(cast::<f32, _>(bir * sf(0.96f32)));
        g.short_bond_yield = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.long_bond_yield) * sf(0.95f32)));
        g.t_bill_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.short_bond_yield) * sf(0.88f32)));
    } else {
        calc_gov_bond_yields(g);
    }
    let bir = cast::<Ext, _>(g.base_interest_rate);
    if (cast::<Ext, _>(g.prime_rate) - bir).abs() > sf(0.365f32) {
        let quarters = to_integer(Ext::from_f64(4.0) * bir);
        g.prime_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(quarters) / Ext::from_f64(4.0)));
    }
    g.housing_starts = to_long(cast::<Ext, _>(g.gdp_level) * sf(0.01f32) + Ext::from_f64(1000.0));
    let factor = cast::<f32, _>(sf(0.1f32) * (sf(7.5f32) - bir) + Ext::from_f64(1.0)); // SINGLE local
    g.housing_starts = to_long(cast::<Ext, _>(factor) * cast::<Ext, _>(g.housing_starts));
    if g.housing_starts < 300 {
        g.housing_starts = to_long(Ext::from_f64(50.0) * rng.rnd() + Ext::from_f64(250.0));
    }
}

/// `x MOD y` on the x87 (`PB_FMod`, `fprem`): truncating remainder.
fn fmod(x: Ext, y: Ext) -> Ext {
    x % y
}

/// Port of SweepPlayerCashToLoan (0x508f5c).
/// For every player (not "...OUT...") with `SweepCashToLoan` set and positive cash: repays the bank
/// loan from the cash (the lending bank receives the payment) or, without a loan, moves the cash into
/// T-bills.
pub fn sweep_player_cash_to_loan(g: &mut Globals) {
    let n = g.num_players;
    for player_id in 1..=n {
        if g.player_name[player_id] == "...OUT..." || g.sweep_cash_to_loan[player_id] == 0 || !(g.cash[player_id] > Ext::from_f64(0.0)) {
            continue;
        }
        let mut sweep_amt = g.cash[player_id];
        if g.bank_debt[player_id] > Ext::from_f64(0.0) {
            if sweep_amt < g.bank_debt[player_id] {
                g.bank_debt[player_id] = g.bank_debt[player_id] - sweep_amt;
            } else {
                sweep_amt = g.bank_debt[player_id];
                g.bank_debt[player_id] = Ext::from_f64(0.0);
            }
            g.cash[player_id] = g.cash[player_id] - sweep_amt;
            let bank = cast::<i32, _>(g.lending_bank[player_id]);
            g.cash[bank] = sweep_amt + g.cash[bank];
            if g.cash[player_id].abs() < sf(1e-6f32) {
                g.cash[player_id] = Ext::from_f64(0.0);
            }
        } else {
            g.t_bills[player_id] = sweep_amt + g.t_bills[player_id];
            g.cash[player_id] = Ext::from_f64(0.0);
            g.bank_debt[player_id] = Ext::from_f64(0.0);
        }
    }
}

/// Port of UpdateLoanLimitMultiple (0x4bd563).
/// Sets `LoanLimitMultiple` (the companies' credit line as a multiple of net worth) from the prime
/// rate: `ROUND(25 + adj) * 0.1` with `adj` = `-0.7 * (prime - 8)` above 8 % (at most -5) or
/// `1.7 * (7 - prime)` below 6.75 % (at most +5). Shows a pending `NewsMsg`; when the multiple was set
/// before and the credit-line news was not shown yet, a falling multiple with a rising index ("As rates
/// rise, banks tighten…") or a rising one with a falling index (" [runtime resource wording] …") is shown
/// with probability 1/3 (`CreditLineNewsDone = 1`).
pub fn update_loan_limit_multiple(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mut mult_adj: Ext = crate::pb::numeric::double(Ext::from_f64(0.0)); // DOUBLE local
    let prev = crate::pb::numeric::double(g.loan_limit_multiple);
    g.loan_limit_multiple = cast::<f64, _>(sf(2.5f32));
    let prime = cast::<Ext, _>(g.prime_rate);
    if Ext::from_f64(8.0) < prime {
        mult_adj = crate::pb::numeric::double(sf(0.7f32) * (prime - Ext::from_f64(8.0)));
        if Ext::from_f64(5.0) < mult_adj {
            mult_adj = crate::pb::numeric::double(Ext::from_f64(5.0));
        }
        mult_adj = crate::pb::numeric::double(-mult_adj); // * -1
    } else if sf(6.75f32) > prime {
        mult_adj = crate::pb::numeric::double(sf(1.7f32) * (Ext::from_f64(7.0) - prime));
        if Ext::from_f64(5.0) < mult_adj {
            mult_adj = crate::pb::numeric::double(Ext::from_f64(5.0));
        }
    }
    let steps = to_long(mult_adj + Ext::from_f64(25.0));
    g.loan_limit_multiple = cast::<f64, _>(cast::<Ext, _>(steps) * sf(0.1f32));
    if !g.news_msg.is_empty() {
        crate::logic::report::show_quick_news(g, ui);
    }
    if Ext::from_f64(0.0) < prev && g.credit_line_news_done == 0 {
        let mut price_pct_chg = price_pct_change_months(g, 1602, 6);
        if (cast::<Ext, _>(price_pct_chg)).abs() < Ext::from_f64(10.0) {
            price_pct_chg = 0;
        }
        let streak = price_trend_streak(g, 1602);
        if prev > g.loan_limit_multiple && 0 < price_pct_chg && 2 < streak {
            g.news_msg = s(__content.string(4560));
        } else if prev < g.loan_limit_multiple && price_pct_chg < 0 && streak < 0 {
            g.news_msg = s(__content.string(4561));
        }
        if !g.news_msg.is_empty() {
            if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
                crate::logic::report::show_quick_news(g, ui);
                g.credit_line_news_done = 1;
            }
        }
    }
}

/// Port of GovtTaxPolicyEvents (0x45d2c8).
/// Quarterly at company counts 495/990/1485: `MacroOutlookNews` and the GDP step (toward
/// `GdpGrowthTarget` with noise, damped by the long-bond price, clamped to -20.7..6.8 and rounded to
/// 0.1 via `FORMAT$(…, "##.#")`). Counts down `EconAlertCooldown` / `OilTaxCutCooldown`. From
/// `NextNewTaxYear` on: introduces (or permanently declines: rate -1) and raises the wealth tax, the
/// health-care tax, the U.N. carbon tax and the oil windfall-profits tax (cut and repeal when oil is
/// cheap), with headlines (`PostNews`, ShowQuickMessage, MSCNEWS stories 472..476) and effects on
/// `GdpGrowthTarget`, `MarketValTarget` / `MarketValuation`, gold and oil targets and oil companies.
/// At company counts that are multiples of 297: fades `InvestorSentiment` (computer players' companies
/// get 50 before the last quarter of the game) and moves every industry's growth rate toward its
/// forecast (cut in a deep recession).
///
/// The ON ERROR handler only shows ShowRuntimeError.
pub fn govt_tax_policy_events(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    if g.yr_wealth_tax_chg == 0 {
        g.yr_wealth_tax_chg = g.cur_year - 2;
    }
    if g.yr_carbon_tax_chg == 0 {
        g.yr_carbon_tax_chg = g.cur_year;
    }
    if g.yr_oil_windfall_tax_chg == 0 {
        g.yr_oil_windfall_tax_chg = g.cur_year - 1;
    }
    if g.yr_health_tax_chg == 0 {
        g.yr_health_tax_chg = g.cur_year - 1;
    }
    if g.sim_co_count == 495 || g.sim_co_count == 990 || g.sim_co_count == 1485 {
        macro_outlook_news(g, ui, rng);
        let gdp_target: f32 = cast::<f32, _>(g.gdp_growth_target); // SINGLE local
        let mut growth_step: f32 = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(gdp_target) - cast::<Ext, _>(g.gdp_growth)) / Ext::from_f64(5.0))); // SINGLE
        let rand_adj: Ext = crate::pb::numeric::double(Ext::from_f64(2.0) * rng.rnd()); // DOUBLE
        if Ext::from_f64(3.0) < cast::<Ext, _>(growth_step) {
            growth_step = cast::<f32, _>((rand_adj + cast::<Ext, _>(growth_step)) - Ext::from_f64(1.0));
        } else if -Ext::from_f64(3.0) > cast::<Ext, _>(growth_step) {
            growth_step = cast::<f32, _>((rand_adj + cast::<Ext, _>(growth_step)) + Ext::from_f64(1.0));
        } else if (cast::<Ext, _>(gdp_target) - cast::<Ext, _>(g.gdp_growth)).abs() < sf(0.75f32) {
            let r = Ext::from_f64(2.0) * rng.rnd();
            g.gdp_growth = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth)) - r));
            let r = Ext::from_f64(6.0) * rng.rnd();
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(3.0) + cast::<Ext, _>(g.gdp_growth_target)) - r));
        } else {
            let r = Ext::from_f64(2.0) * rng.rnd();
            g.gdp_growth = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth)) - r));
            let r = Ext::from_f64(2.0) * rng.rnd();
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth_target)) - r));
        }
        if Ext::from_f64(6.0) * rng.rnd() < Ext::from_f64(1.0) {
            growth_step = -growth_step;
        }
        if cast::<Ext, _>(gdp_target) - Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth) {
            growth_step = growth_step.abs();
        } else if Ext::from_f64(3.0) + (cast::<Ext, _>(gdp_target)) < cast::<Ext, _>(g.gdp_growth) && Ext::from_f64(0.0) < cast::<Ext, _>(growth_step) {
            growth_step = -growth_step;
        }
        g.gdp_growth = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(growth_step) + cast::<Ext, _>(g.gdp_growth)));
        if cast::<Ext, _>(g.long_bond_price) * sf(0.08f32) < cast::<Ext, _>(g.gdp_growth)
            || sf(1.2f32) * rng.rnd() + Ext::from_f64(5.0) < cast::<Ext, _>(g.gdp_growth)
        {
            g.gdp_growth = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth) - (sf(0.4f32) * rng.rnd() + sf(0.8f32))));
        }
        if sf(6.9f32) < cast::<Ext, _>(g.gdp_growth) {
            g.gdp_growth = cast::<f32, _>(Ext::from_f32(6.8f32));
        }
        if -sf(21.5f32) > cast::<Ext, _>(g.gdp_growth) {
            g.gdp_growth = cast::<f32, _>(-Ext::from_f32(20.7f32));
        }
        let tax_text = format(cast::<Ext, _>(g.gdp_growth), b"##.#");
        g.gdp_growth = cast::<f32, _>(cast::<f32, _>(val(&tax_text)));
    }
    if 0 < g.econ_alert_cooldown {
        g.econ_alert_cooldown -= 1;
    }
    if 0 < g.oil_tax_cut_cooldown {
        g.oil_tax_cut_cooldown -= 1;
    }
    if !(g.cur_year < g.next_new_tax_year) {
        new_taxes(g, ui, rng);
    }
    // LAB_0045e87d
    if g.sim_co_count % 297 == 0 && 296 < g.sim_co_count {
        for loop_id in 11..=1600 {
            if g.entity_status[loop_id] != 1 {
                g.investor_sentiment[loop_id] = 0;
                continue;
            }
            if (cast::<Ext, _>(g.investor_sentiment[loop_id])).abs() > Ext::from_f64(50.0) {
                if 50 < g.investor_sentiment[loop_id] {
                    g.investor_sentiment[loop_id] = 50;
                }
                if -40 > g.investor_sentiment[loop_id] {
                    g.investor_sentiment[loop_id] = -40;
                }
            }
            if g.cur_quarter == 4 && !(g.game_end_year - 1 > g.cur_year) && !(g.num_players < g.corp_controller[loop_id]) {
                let controller_id = g.corp_controller[loop_id];
                if 0 < controller_id && g.is_computer_player[controller_id] == 1 {
                    g.investor_sentiment[loop_id] = 50;
                    continue;
                }
            }
            g.investor_sentiment[loop_id] = cast::<i32, _>(to_quad(sf(0.9f32) * cast::<Ext, _>(g.investor_sentiment[loop_id])));
            let pm = g.profit_margin[loop_id];
            if pm < -2 || pm > 5 {
                let sentiment_adj = to_long(cast::<Ext, _>(pm) * sf(0.5f32));
                g.investor_sentiment[loop_id] = sentiment_adj.wrapping_add(g.investor_sentiment[loop_id]);
            }
        }
        for loop_id in 3..=69 {
            if -Ext::from_f64(5.0) > cast::<Ext, _>(g.gdp_growth)
                && -Ext::from_f64(8.0) > cast::<Ext, _>(g.gdp_growth_target)
                && !matches!(loop_id, 14 | 17 | 28 | 31 | 32 | 46 | 50 | 62)
                && !(loop_id == 7 && g.peak_oil_crisis == 1)
                && !(loop_id == 8 && g.peak_oil_crisis == 1)
            {
                if -7 < g.ind_growth_forecast[loop_id] {
                    let x = igf(g, loop_id) - Ext::from_f64(7.0) * rng.rnd();
                    set_igf(g, loop_id, x);
                }
                if (cast::<i32, _>(g.ind_growth_rate[loop_id])) > cast::<i32, _>(g.ind_growth_forecast[loop_id]) - 3 {
                    let x = cast::<Ext, _>(g.ind_growth_rate[loop_id]) - (Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(3.0));
                    g.ind_growth_rate[loop_id] = cast::<i16, _>(to_quad(x));
                }
                if Ext::from_f64(1.0) - Ext::from_f64(5.0) * rng.rnd() < g.ind_demand_gap[loop_id] {
                    g.ind_demand[loop_id] = to_quad(sf(0.98f32) * cast::<Ext, _>(g.ind_demand[loop_id]));
                    if Ext::from_f64(15.0) < g.ind_demand_gap[loop_id] {
                        g.ind_demand[loop_id] = to_quad(sf(0.97f32) * cast::<Ext, _>(g.ind_demand[loop_id]));
                    }
                }
            }
            if Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(60.0) < igf(g, loop_id) {
                let x = Ext::from_f64(70.0) - Ext::from_f64(15.0) * rng.rnd();
                set_igf(g, loop_id, x);
            }
            if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(60.0) < cast::<Ext, _>(g.ind_growth_rate[loop_id]) {
                let x = Ext::from_f64(60.0) - Ext::from_f64(10.0) * rng.rnd();
                g.ind_growth_rate[loop_id] = cast::<i16, _>(to_quad(x));
            }
            if g.ind_growth_forecast[loop_id] == g.ind_growth_rate[loop_id] {
                let x = (Ext::from_f64(6.0) * rng.rnd() - Ext::from_f64(3.0)) + cast::<Ext, _>(g.ind_growth_rate[loop_id]);
                g.ind_growth_rate[loop_id] = cast::<i16, _>(to_quad(x));
            } else {
                let r = cast::<Ext, _>(rng.rnd_range(1, 3)) + Ext::from_f64(1.0);
                let mut growth_step: f32 =
                    cast::<f32, _>(cast::<f32, _>((igf(g, loop_id) - cast::<Ext, _>(g.ind_growth_rate[loop_id])) / r)); // SINGLE local
                if (cast::<Ext, _>(growth_step)).abs() < Ext::from_f64(1.0) {
                    growth_step = cast::<f32, _>(Ext::from_f64(2.0) * rng.rnd() - Ext::from_f64(1.0));
                }
                if Ext::from_f64(8.0) * rng.rnd() > Ext::from_f64(7.0) {
                    growth_step = -growth_step;
                }
                let x = cast::<Ext, _>(growth_step) + cast::<Ext, _>(g.ind_growth_rate[loop_id]);
                g.ind_growth_rate[loop_id] = cast::<i16, _>(to_quad(x));
            }
        }
    }
}

/// GovtTaxPolicyEvents, 0x45d67b..0x45e87d: the new-tax events (from `NextNewTaxYear` on).
fn new_taxes(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let gdp = |g: &Globals| cast::<Ext, _>(g.gdp_growth);
    let mul_gold = |g: &mut Globals, f: f32| g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_target_price) * sf(f)));
    // wealth tax
    if -Ext::from_f64(4.0) > gdp(g) && g.start_year < g.cur_year && g.wealth_tax_rate == 0 {
        let cp = g.cur_player;
        if g.t_bills[cp] + g.cash[cp] > Ext::from_f64(2500.0) {
            if g.rand_seed % 7 < 2 || g.difficulty_level < 3 {
                if g.wealth_tax_rate == 0 {
                    g.wealth_tax_rate = -1;
                }
                mul_gold(g, 0.9f32);
            } else {
                g.wealth_tax_rate = rng.rnd_range(1, 4);
                let tax_text = if g.currency_code == "UK" {
                    s(__content.string(4562))
                } else if g.currency_code == "USA" {
                    s(__content.string(4563))
                } else if g.currency_code == "AUS" {
                    s(__content.string(4564))
                } else if g.currency_code == "CAN" {
                    s("Canada")
                } else {
                    s(__content.string(4565))
                };
                let story_text = s(__content.string(4566)) + &tax_text + ".";
                g.tmp_news_str = str_trim(cast::<Ext, _>(g.wealth_tax_rate));
                g.yr_wealth_tax_chg = g.cur_year;
                g.news_msg = s(__content.string(4567)) + &str_long(g.wealth_tax_rate) + "%.";
                post_news(g, ui, 0, 0);
                show_misc_news_story(g, ui, rng, 476, story_text.as_bytes());
                g.next_new_tax_year = g.cur_year + 2;
                mul_gold(g, 1.1f32);
            }
        }
    }
    if -Ext::from_f64(8.0) > gdp(g)
        && g.yr_wealth_tax_chg + 3 < g.cur_year
        && 0 < g.wealth_tax_rate
        && g.wealth_tax_rate < 4
        && g.econ_alert_cooldown == 0
    {
        g.wealth_tax_rate += 1;
        g.yr_wealth_tax_chg = g.cur_year;
        g.news_msg = s(__content.string(4568)) + &str_long(g.wealth_tax_rate) + "%.";
        crate::logic::report::show_quick_news(g, ui);
        post_news(g, ui, 0, 0);
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(2.0)));
        g.econ_alert_cooldown = 700;
        mul_gold(g, 1.1f32);
    }
    // health-care tax
    if Ext::from_f64(0.0) > gdp(g) && g.start_year + 1 < g.cur_year && g.health_care_tax_rate == 0 {
        if g.rand_seed % 7 > 5 || g.difficulty_level < 3 {
            g.health_care_tax_rate = -1;
            mul_gold(g, 0.9f32);
        } else {
            g.health_care_tax_rate = rng.rnd_range(1, 3);
            let tax_text = str_long(g.health_care_tax_rate);
            g.tmp_news_str = rtrim(ltrim(g.currency_symbol.as_bytes()).as_bytes()) + "1 " + &g.million_prefix_lc;
            g.news_msg = s("NEW ") + &tax_text + __content.string(4569);
            show_misc_news_story(g, ui, rng, 472, tax_text.as_bytes());
            post_news(g, ui, 0, 0);
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(2.0)));
            g.yr_health_tax_chg = g.cur_year;
            if sf(0.7f32) < cast::<Ext, _>(g.market_valuation) {
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.market_valuation) - sf(0.1f32)));
            }
            g.market_val_target = cast::<f64, _>(sf(0.8f32));
            g.next_new_tax_year = g.cur_year + 2;
            mul_gold(g, 1.1f32);
        }
    }
    if Ext::from_f64(4.0) < gdp(g)
        && g.yr_health_tax_chg + 2 < g.cur_year
        && 0 < g.health_care_tax_rate
        && g.health_care_tax_rate < 5
        && g.econ_alert_cooldown == 0
    {
        g.health_care_tax_rate += 1;
        g.yr_health_tax_chg = g.cur_year;
        g.news_msg =
            s(__content.string(4570)) + &str_long(g.health_care_tax_rate) + "%.";
        if 2 < g.health_care_tax_rate && Ext::from_f64(5.0) * rng.rnd() > Ext::from_f64(4.0) {
            g.health_care_tax_rate -= 2;
            g.news_msg = s(__content.string(4571))
                + &str_long(g.health_care_tax_rate)
                + "%.";
            g.market_val_target = cast::<f64, _>(sf(1.25f32));
            mul_gold(g, 0.9f32);
        } else {
            g.market_val_target = cast::<f64, _>(sf(0.8f32));
            mul_gold(g, 1.1f32);
        }
        crate::logic::report::show_quick_news(g, ui);
        post_news(g, ui, 0, 0);
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(1.0)));
        g.econ_alert_cooldown = 600;
    }
    // carbon tax
    if Ext::from_f64(5.0) < gdp(g) && g.start_year + 3 < g.cur_year && g.carbon_tax_rate == 0 {
        if g.rand_seed % 7 < 2 || g.difficulty_level < 3 {
            g.carbon_tax_rate = -1;
        } else {
            g.carbon_tax_rate = rng.rnd_range(1, 3);
            let tax_text = str_long(g.carbon_tax_rate);
            g.news_msg = s(__content.string(4572));
            show_misc_news_story(g, ui, rng, 475, tax_text.as_bytes());
            post_news(g, ui, 0, 0);
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(3.0)));
            g.yr_carbon_tax_chg = g.cur_year;
            if sf(0.7f32) < cast::<Ext, _>(g.market_valuation) {
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.market_valuation) - sf(0.17f32)));
            }
            g.market_val_target = cast::<f64, _>(sf(0.7f32));
            g.next_new_tax_year = g.cur_year + 3;
        }
    }
    if Ext::from_f64(4.0) < gdp(g)
        && g.yr_carbon_tax_chg + 1 < g.cur_year
        && 0 < g.carbon_tax_rate
        && g.carbon_tax_rate < 5
        && g.econ_alert_cooldown == 0
    {
        g.carbon_tax_rate += 1;
        g.yr_carbon_tax_chg = g.cur_year;
        g.news_msg = s(__content.string(4573))
            + &str_long(g.carbon_tax_rate)
            + "%.";
        crate::logic::report::show_quick_news(g, ui);
        post_news(g, ui, 0, 0);
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(1.0)));
        g.econ_alert_cooldown = 400;
        g.market_val_target = cast::<f64, _>(sf(0.75f32));
    }
    // oil windfall-profits tax
    if g.difficulty_level < 3 {
        g.oil_windfall_tax_rate = -1;
    }
    let scc = g.sim_co_count;
    if sf(0.9f32) < cast::<Ext, _>(g.oil_price)
        && g.start_year + 4 < g.cur_year
        && 130 < scc
        && scc < 1601
        && g.oil_windfall_tax_rate == 0
    {
        let industry = g.industry_num[scc];
        if industry == 7 || industry == 8 || industry == 63 {
            if val(parse_any(g.ent_misc_data[scc].as_bytes(), b"|", 30).as_bytes()) > Ext::from_f64(40.0) {
                if g.rand_seed % 7 > 5 {
                    g.oil_windfall_tax_rate = -1;
                } else {
                    g.oil_windfall_tax_rate = rng.rnd_range(2, 5);
                    g.tmp_news_str = str_trim(cast::<Ext, _>(g.oil_windfall_tax_rate));
                    let tax_text = s("$100");
                    g.news_msg = s(__content.string(4574));
                    post_news(g, ui, 0, 0);
                    show_misc_news_story(g, ui, rng, 473, tax_text.as_bytes());
                    g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(1.0)));
                    g.yr_oil_windfall_tax_chg = g.cur_year;
                    if sf(0.8f32) < cast::<Ext, _>(g.market_valuation) {
                        g.market_valuation = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.market_valuation) - sf(0.17f32)));
                    }
                    g.market_val_target = cast::<f64, _>(sf(0.85f32));
                    for loop_id in 131..=1600 {
                        if g.entity_status[loop_id] != 1 {
                            continue;
                        }
                        let ind = g.industry_num[loop_id];
                        if ind == 7 || ind == 8 || ind == 63 {
                            g.market_cap[loop_id] = sf(0.93f32) * g.market_cap[loop_id];
                            g.investor_sentiment[loop_id] = g.investor_sentiment[loop_id].wrapping_sub(20);
                        }
                    }
                    g.next_new_tax_year = g.cur_year + 3;
                    g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_target_price) * sf(1.1f32)));
                    mul_gold(g, 1.1f32);
                }
            }
        }
    }
    if Ext::from_f64(2.0) < gdp(g)
        && 300 < g.sim_co_count
        && g.yr_oil_windfall_tax_chg + 2 < g.cur_year
        && 0 < g.oil_windfall_tax_rate
        && g.oil_windfall_tax_rate < 6
        && sf(1.3f32) < cast::<Ext, _>(g.oil_price)
        && g.econ_alert_cooldown == 0
    {
        g.oil_windfall_tax_rate += 1;
        g.yr_oil_windfall_tax_chg = g.cur_year;
        g.news_msg = s(__content.string(4575)) + &str_long(g.oil_windfall_tax_rate) + "%.";
        crate::logic::report::show_quick_news(g, ui);
        post_news(g, ui, 0, 0);
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(1.0)));
        g.market_val_target = cast::<f64, _>(sf(0.9f32));
        g.econ_alert_cooldown = 200;
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_target_price) * sf(1.1f32)));
        mul_gold(g, 1.1f32);
    }
    if 4 < g.oil_windfall_tax_rate && Ext::from_f64(1.0) > cast::<Ext, _>(g.oil_price) && g.sim_co_count < 700 && g.oil_tax_cut_cooldown == 0 {
        g.oil_windfall_tax_rate -= 1;
        g.news_msg =
            s(__content.string(4576)) + &str_long(g.oil_windfall_tax_rate) + __content.string(4577);
        if price_trend_direction(g, 6, 2, 1) > 0 {
            g.news_msg = replace_str(g.news_msg.as_bytes(), __content.bytes(4578), __content.bytes(4579));
        } else if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
            g.news_msg = replace_str(g.news_msg.as_bytes(), b"MODERATES", __content.bytes(4580));
            if Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
                g.news_msg = replace_str(g.news_msg.as_bytes(), b"STAYS", b"STILL");
            } else if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
                g.news_msg = replace_str(g.news_msg.as_bytes(), __content.bytes(4580), __content.bytes(4581));
            }
        }
        crate::logic::report::show_quick_news(g, ui);
        post_news(g, ui, 0, 0);
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_target_price) * sf(0.9f32)));
        g.oil_tax_cut_cooldown = 1200;
    }
    if 0 < g.oil_windfall_tax_rate
        && sf(0.2f32) * rng.rnd() + sf(0.4f32) > cast::<Ext, _>(g.oil_price)
        && 900 < g.sim_co_count
        && g.oil_tax_cut_cooldown == 0
    {
        g.oil_windfall_tax_rate = -1;
        g.news_msg = s(__content.string(4582));
        crate::logic::report::show_quick_news(g, ui);
        post_news(g, ui, 0, 0);
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) + cast::<Ext, _>(g.gdp_growth_target)));
        if sf(1.2f32) > g.market_val_target {
            g.market_val_target = cast::<f64, _>(sf(1.2f32));
        }
        g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.1f32) * cast::<Ext, _>(g.market_valuation)));
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_target_price) * sf(0.9f32)));
        g.oil_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_price) * sf(0.96f32)));
        mul_gold(g, 0.9f32);
        g.oil_tax_cut_cooldown = 650;
    }
}

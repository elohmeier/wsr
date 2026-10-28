//! The market-cycle planner `PlanMarketCycle` (0x4bfbd3) with `GenMarketCyclePlan` (0x4c4b18) and
//! `MarketCycle_CountPlayerFutures` (0x4c1a04), and the macro news events `MacroOutlookNews` (0x4c1bc7),
//! `MarketReversalEvent` (0x4c3454), `MarketAftershock` (0x4c3d74), `GovFiscalPolicyNews` (0x4c5d0b).
use crate::pb::real::{Ext, cast};


use super::*;
use crate::logic::finance::{get_controller_type, get_managing_player};
use crate::logic::futures::update_metals_price_base;
use crate::logic::market::update_market_display;
use crate::logic::valuation::{calc_industry_avg_net_margin, price_trend_streak, update_price_history_tick};
use crate::pb::numeric::{str_num_ext, Ext80};
use crate::pb::string::{parse_any, ucase, CaseMode};

/// One unit of the exact rate-band arithmetic in PlanMarketCycle: 2^-56.
const BAND_UNIT: i64 = 1 << 56;

/// `STR$` of an EXT local holding `units * 2^-56` (exact: the value fits the 64-bit mantissa).
fn band_str(units: i64) -> PbStr {
    let x = Ext80::from_i64(units).fdiv(Ext80::from_i64(BAND_UNIT));
    PbStr::from(str_num_ext(x, 15).as_str())
}

/// `RND` as `n * 2^-31`: the raw 31-bit draw.
fn rnd_raw(rng: &mut Rng) -> i64 {
    cast::<i64, _>(rng.next_raw())
}

/// A random rate band low in 2^-56 units (the identical blocks at 0x4c0345 and 0x4c04a1): `1.5 * RND +
/// 2.25`, `- 0.4 * RND` with loose money / `+ 0.5 * RND` with tight money, `- 0.5` when `GdpGrowthTarget
/// < -1` / `+ 0.5` when above 3.2, and `0.5 * RND + 1.9` when it ends below 2. Returns (low, high) with
/// `high = low + 1.75 + RND`. Every step is exact in the 80-bit FPU and in these units.
fn new_rate_band(g: &Globals, rng: &mut Rng) -> (i64, i64) {
    const U25: i64 = 1 << 25; // RND = n * 2^25 units
    // 1.5 * RND + 2.25
    let mut low = 3 * rnd_raw(rng) * (1 << 24) + 9 * (1 << 54);
    if g.monetary_policy == 0 {
        // 0.4! = 13421773 * 2^-25
        low -= 13_421_773 * rnd_raw(rng);
    } else if g.monetary_policy == 2 {
        low += rnd_raw(rng) * (1 << 24);
    }
    if -Ext::from_f64(1.0) > cast::<Ext, _>(g.gdp_growth_target) {
        low -= 1 << 55;
    } else if sf(3.2f32) < cast::<Ext, _>(g.gdp_growth_target) {
        low += 1 << 55;
    }
    if 2 * BAND_UNIT > low {
        // 0.5 * RND + 1.9! (1.9! = 15938355 * 2^-23)
        low = rnd_raw(rng) * (1 << 24) + 15_938_355 * (1 << 33);
    }
    // (1.75 + low) + 1 * RND
    let high = (7 * (1 << 54) + low) + rnd_raw(rng) * U25;
    (low, high)
}

/// Port of PlanMarketCycle (0x4bfbd3) with its GOSUB MarketCycle_CountPlayerFutures (0x4c1a04).
/// Steers the long-run market cycle while no crisis (oil, subprime, bubble, pandemic) runs:
///
/// * keeps `NextMarketCycleYear` 5..8 years ahead (clears `MonetaryStance` in that year and the next);
/// * only on the ticks `SimCoCount MOD 100 = 1` (>= 101): in the first quarters of a new game it nudges
///   `MarketValTarget`/`MarketValuation` toward a moderate index (after `FirstYearMktCounter` reached 10,
///   seeded from the number of computer-held stock-index futures); later in the first year it pushes the
///   rates, GDP target, valuation and (1 in 9) all caps against an index that ran away from
///   `StockIndexBase`;
/// * on ticks 301/701/1101 it keeps `InterestRateTarget` inside a random band (`RateTargetBandStr`),
///   then (half the time) follows `MarketCyclePlan` (bull/bear phase years from [`gen_market_cycle_plan`],
///   with a pop-up at a phase start): a growth corridor around `StockIndexBase` sets `MonetaryStance`,
///   `MarketValTarget`, GDP target and rates, shifts industry forecasts/growth (and computer-run companies'
///   growth rates) and moves a random range of market caps; an overheated market can start a new company
///   in the best-margin shortage industry (`StartNewCorpOrExpand`).
///
/// Posts `NewsMsg` (if any) except on the early exits. `time_secs`: `VAL(RIGHT$(TIME$, 2))` (for the new
/// company's `CalcStockPrice`).
pub fn plan_market_cycle(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    if g.oil_crisis_level != Ext::from_f32(0.0f32) || 0 < g.subprime_crisis_state || 0 < g.bubble_state || 0 < g.pandemic_state {
        return;
    }
    if g.cur_year == g.next_market_cycle_year || 1 + g.next_market_cycle_year == g.cur_year {
        g.monetary_stance = 0;
        return;
    }
    if g.cur_year > g.next_market_cycle_year {
        g.next_market_cycle_year = 0;
    }
    if g.next_market_cycle_year == 0 && 3 + g.start_year < g.cur_year {
        g.next_market_cycle_year = rng.rnd_range(8, 5) + g.cur_year;
    }
    if g.sim_co_count % 100 != 1 || g.sim_co_count < 101 {
        return;
    }
    // caller locals shared with the GOSUB (zeroed by the frame setup)
    let mut cpu_futures_count: i32 = 0;
    let count_player_futures = |g: &mut Globals, rng: &mut Rng, cpu_futures_count: &mut i32| {
        // GOSUB MarketCycle_CountPlayerFutures (0x4c1a04)
        if Ext::from_f64(0.0) < val(g.futures_recs[0].as_bytes()) {
            let n = to_long(val(g.futures_recs[0].as_bytes()));
            for ent_id in 1..=n {
                let rec = g.futures_recs[ent_id].clone();
                if rec.is_empty() {
                    continue;
                }
                if parse_any(rec.as_bytes(), b"|", 1) == "STOCKS" {
                    let holder_id = to_long(val(parse_any(rec.as_bytes(), b"|", 2).as_bytes()));
                    if get_controller_type(g, holder_id) == 1 {
                        *cpu_futures_count += 1;
                    }
                }
            }
            if 5 < *cpu_futures_count {
                *cpu_futures_count = 5;
            }
            if 1 < *cpu_futures_count && g.first_year_mkt_counter == 0 {
                g.first_year_mkt_counter = rng.rnd_range(8, *cpu_futures_count);
            } else if *cpu_futures_count == 0 {
                g.first_year_mkt_counter = 0;
            }
        }
    };

    if g.start_year == g.cur_year && g.cur_quarter < 3 {
        if g.first_year_mkt_counter == 0 {
            count_player_futures(g, rng, &mut cpu_futures_count);
        } else {
            g.first_year_mkt_counter += 1;
        }
        if g.first_year_mkt_counter < 10 {
            return;
        }
        let index_now = g.stock_index;
        if !(index_now > cast::<Ext, _>(g.stock_index_base) * sf(1.04f32)) {
            return;
        }
        let mut val_shift: Ext = Ext::from_f64(0.0); // EXT local (rateBandHigh), zero unless set here
        let r = rng.rnd();
        if index_now > cast::<Ext, _>(g.stock_index_base) * sf(0.84f32) + Ext::from_f64(1000.0) * (sf(0.2f32) * r) {
            val_shift = -sf(0.2f32);
            g.stock_index_divisor = sf(1.03f32) * g.stock_index_divisor;
        }
        g.market_val_target = cast::<f64, _>(val_shift + g.market_val_target);
        let delta = g.market_val_target - cast::<Ext, _>(g.market_valuation);
        g.market_valuation = cast::<f32, _>(cast::<f32, _>(delta * sf(0.5f32) + cast::<Ext, _>(g.market_valuation)));
        if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(1.0) < cast::<Ext, _>(g.gdp_growth_target) {
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) * cast::<Ext, _>(g.gdp_growth_target)));
        }
        if g.sim_co_count > 1450 && g.econ_cycle_counter < 18 {
            g.econ_cycle_counter = 21;
        }
        post_news(g, ui, 0, 0);
        return;
    }

    if g.start_year == g.cur_year {
        if g.first_year_mkt_counter == 0 {
            count_player_futures(g, rng, &mut cpu_futures_count);
        } else {
            g.first_year_mkt_counter += 1;
        }
        if g.first_year_mkt_counter < 10 {
            return;
        }
        let sib = cast::<Ext, _>(g.stock_index_base);
        let r = rng.rnd();
        if g.stock_index > sib * sf(1.16f32) + sib * (sf(0.2f32) * r) {
            // index ran up too fast: lean against it
            if sf(0.5f32) < cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - sf(0.5f32)));
            }
            if Ext::from_f64(16.0) > cast::<Ext, _>(g.interest_rate_target) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.interest_rate_target)));
            }
            if Ext::from_f64(15.0) > cast::<Ext, _>(g.base_interest_rate) {
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) + cast::<Ext, _>(g.base_interest_rate)));
            }
            if g.monetary_policy == 0 {
                g.monetary_policy += 1;
                if g.monetary_policy == 2 {
                    // unreachable (0x4bff22)
                    if price_trend_streak(g, 0) > 0 {
                        g.news_msg = g.central_bank_name.clone() + __content.string(4505);
                    }
                }
            }
            g.stock_index_divisor = sf(1.03f32) * g.stock_index_divisor;
            g.market_val_target = cast::<f64, _>(sf(0.8f32) * g.market_val_target);
            if sf(0.62f32) < cast::<Ext, _>(g.market_valuation) {
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.8f32) * cast::<Ext, _>(g.market_valuation)));
            }
            if g.first_year_mkt_counter % 9 == 1 && Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                for ent_id in 11..=1600 {
                    if g.entity_status[ent_id] != 1 || g.industry_num[ent_id] == 71 {
                        continue;
                    }
                    g.investor_sentiment[ent_id] = g.investor_sentiment[ent_id] - 10;
                    g.market_cap[ent_id] = sf(0.93f32) * g.market_cap[ent_id];
                }
            }
        } else {
            let r = rng.rnd();
            if g.stock_index < sib * sf(0.92f32) - Ext::from_f64(1000.0) * r {
                // index lagging: support it
                if sf(1.5f32) > cast::<Ext, _>(g.gdp_growth_target) {
                    g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth_target)));
                }
                if sf(4.75f32) < cast::<Ext, _>(g.interest_rate_target) {
                    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.interest_rate_target) - Ext::from_f64(1.0)));
                }
                g.market_val_target = cast::<f64, _>(sf(1.1f32) * g.market_val_target);
                if sf(1.2f32) > cast::<Ext, _>(g.market_valuation) {
                    g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.08f32) * cast::<Ext, _>(g.market_valuation)));
                }
                if g.monetary_policy == 2 {
                    g.monetary_policy -= 1;
                    if g.monetary_policy == 0 {
                        // unreachable (0x4c0190)
                        if price_trend_streak(g, 0) < 0 {
                            g.news_msg = g.central_bank_name.clone() + __content.string(4506);
                        }
                    }
                }
                g.stock_index_divisor = sf(0.97f32) * g.stock_index_divisor;
                if g.first_year_mkt_counter % 9 == 1 && Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                    for ent_id in 11..=1600 {
                        if g.entity_status[ent_id] != 1 || g.industry_num[ent_id] == 71 {
                            continue;
                        }
                        g.investor_sentiment[ent_id] = 10 + g.investor_sentiment[ent_id];
                        g.market_cap[ent_id] = sf(1.07f32) * g.market_cap[ent_id];
                    }
                }
            }
        }
        return;
    }

    if g.sim_co_count == 301 || g.sim_co_count == 701 || g.sim_co_count == 1101 {
        if g.rate_target_band_str.is_empty() || g.rand_seed % 15 == 5 {
            let (low, high) = new_rate_band(g, rng);
            g.rate_target_band_str = band_str(low) + "|" + &band_str(high);
        }
        let rate_band_low = val(parse_any(g.rate_target_band_str.as_bytes(), b"|", 1).as_bytes());
        let rate_band_high = val(parse_any(g.rate_target_band_str.as_bytes(), b"|", 2).as_bytes());
        if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(10.0) >= cast::<Ext, _>(g.interest_rate_target) || g.rand_seed % 2 <= 0 {
            let irt = cast::<Ext, _>(g.interest_rate_target);
            if rate_band_low > irt {
                let delta = rate_band_low - irt;
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(delta * sf(0.3f32) + irt));
                if sf(2.75f32) > cast::<Ext, _>(g.base_interest_rate) {
                    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(sf(0.25f32) + cast::<Ext, _>(g.base_interest_rate)));
                }
            } else if rate_band_high < irt {
                let delta = irt - rate_band_high;
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt - delta * sf(0.3f32)));
                if Ext::from_f64(9.0) < cast::<Ext, _>(g.base_interest_rate) {
                    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.base_interest_rate) - sf(0.75f32)));
                }
            }
        }
        if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
            return;
        }
        if Ext::from_f64(2.0) * rng.rnd() + -Ext::from_f64(5.0) > cast::<Ext, _>(g.gdp_growth_target) {
            let t = Ext::from_f64(2.0) + cast::<Ext, _>(g.gdp_growth_target);
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(t + sf(1.5f32) * rng.rnd()));
        }
        if g.econ_cycle_counter < 0 {
            return;
        }
        let mut cycle_phase: i32 = 0;
        if g.market_cycle_plan.is_empty() {
            g.market_cycle_plan = gen_market_cycle_plan(g, rng);
        } else {
            let plan = g.market_cycle_plan.clone();
            let field = |n: i32| to_long(val(parse_any(plan.as_bytes(), b"|", n).as_bytes()));
            let phase1_start = field(1);
            let phase1_end = field(2);
            let phase2_start = field(3);
            let phase2_end = field(4);
            if phase1_start <= g.next_market_cycle_year && phase1_end >= g.next_market_cycle_year {
                g.next_market_cycle_year = phase1_start - 1;
            }
            if phase2_start <= g.next_market_cycle_year && phase2_end >= g.next_market_cycle_year {
                g.next_market_cycle_year = phase2_start - 1;
            }
            let mut news_text = PbStr::new();
            if phase1_start <= g.cur_year && phase1_end >= g.cur_year {
                cycle_phase = -1;
                if phase1_start == g.cur_year && g.cur_quarter == 1 && g.sim_co_count == 701 {
                    news_text = s("***");
                }
            }
            if phase2_start <= g.cur_year && phase2_end >= g.cur_year {
                cycle_phase = 1;
                if phase2_start == g.cur_year && g.cur_quarter == 1 && g.sim_co_count == 301 {
                    news_text = s("***");
                }
            }
            if news_text == "***" {
                news_text = s(__content.string(4507));
                if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                    news_text = replace(news_text.as_bytes(), b"analys", b"exper");
                }
                if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
                    news_text = replace(news_text.as_bytes(), b"bull", b"bear");
                }
                show_quick_message(g, ui, &mut news_text);
            }
        }
        let qtrs_elapsed = (g.cur_year - g.start_year) * 4 + (g.cur_quarter - 1);
        let mut low_growth = Ext80::from_ext(sf(1.01227f32));
        let mut high_growth = Ext80::from_ext(sf(1.02178f32));
        if cycle_phase == 1 {
            if (g.cur_quarter + g.cur_year) % 4 != 1 {
                low_growth = Ext80::from_ext(sf(1.01469f32));
            }
        } else if cycle_phase == -1 {
            if (g.cur_quarter + g.cur_year) % 4 != 1 {
                high_growth = Ext80::from_ext(sf(1.01708f32));
            }
        } else if g.cur_year - g.start_year > 14
            && g.cur_year - g.start_year < 25
            && (g.cur_quarter + g.cur_year) % 3 != 1
        {
            low_growth = Ext80::from_ext(sf(1.01469f32));
        }
        let mut lower_band = Ext80::from_i64(cast::<i64, _>(g.stock_index_base));
        let mut upper_band = Ext80::from_i64(cast::<i64, _>(g.stock_index_base));
        for _ in 1..=qtrs_elapsed {
            lower_band = low_growth.fmul(lower_band);
            upper_band = high_growth.fmul(upper_band);
        }
        if cycle_phase == 0 {
            g.monetary_stance = 0;
        }
        if g.cur_year - g.start_year > 5 && (g.cur_quarter + g.cur_year) % 6 > 2 {
            lower_band = Ext80::from_ext(sf(0.9f32)).fmul(lower_band);
            upper_band = Ext80::from_ext(sf(1.1f32)).fmul(upper_band);
        }
        let lower_band = lower_band.into_ext();
        let upper_band = upper_band.into_ext();
        let mut growth_shift: i32 = 0;
        let mut sentiment_shift: i32 = 0;

        if g.stock_index > lower_band && g.stock_index < upper_band {
            if cycle_phase == 1 {
                g.monetary_stance = -1;
            } else if cycle_phase == -1 {
                g.monetary_stance = 1;
            }
            if Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(3.0) < cast::<Ext, _>(g.gdp_growth) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.interest_rate_target)));
            } else if Ext::from_f64(1.0) - Ext::from_f64(3.0) * rng.rnd() > cast::<Ext, _>(g.gdp_growth) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.interest_rate_target) - Ext::from_f64(1.0)));
            }
        }
        if g.stock_index > upper_band {
            if cycle_phase != 1 {
                g.market_val_target = cast::<f64, _>(sf(0.5f32));
                g.monetary_stance = 1;
                if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(8.0) > cast::<Ext, _>(g.base_interest_rate) {
                    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) + cast::<Ext, _>(g.interest_rate_target)));
                }
            }
        } else if g.stock_index < lower_band && cycle_phase != -1 {
            g.market_val_target = cast::<f64, _>(sf(1.5f32));
            g.monetary_stance = -1;
            if Ext::from_f64(5.0) * rng.rnd() + sf(3.5f32) < cast::<Ext, _>(g.base_interest_rate) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.interest_rate_target) - sf(0.5f32)));
            }
        }
        let delta = g.market_val_target - cast::<Ext, _>(g.market_valuation);
        g.market_valuation = cast::<f32, _>(cast::<f32, _>(delta * sf(0.5f32) + cast::<Ext, _>(g.market_valuation)));

        if g.stock_index > upper_band {
            if Ext::from_f64(2.0) * rng.rnd() + -sf(0.3f32) < cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(1.0)));
            }
            if g.stock_index > upper_band * sf(1.5f32) && cycle_phase <= 0 && Ext::from_f64(3.0) - Ext::from_f64(6.0) * rng.rnd() < cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - sf(1.5f32)));
            }
            growth_shift = -5;
            if 0 < g.monetary_policy || g.stock_index > upper_band * sf(1.3f32) {
                if Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(10.0) > cast::<Ext, _>(g.interest_rate_target) {
                    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) + cast::<Ext, _>(g.interest_rate_target)));
                }
                if g.monetary_policy == 0 {
                    g.monetary_policy += 1;
                    g.news_msg = g.central_bank_name.clone() + __content.string(4508);
                }
            }
            if Ext::from_f64(50.0) * rng.rnd() + Ext::from_f64(1300.0) >= cast::<Ext, _>(g.active_corp_count) {
                let mut best_margin: Ext = Ext::from_f64(0.0);
                let mut best_ind: i16 = 0;
                for ind_id in 3..=69i16 {
                    if !(Ext::from_f64(15.0) <= g.ind_demand_gap[cast::<i32, _>(ind_id)]) {
                        continue;
                    }
                    let ind_margin = calc_industry_avg_net_margin(g, rng, ind_id);
                    if best_margin < cast::<Ext, _>(ind_margin) && cast::<i32, _>(ind_id) != g.crowded_industry && Ext::from_f64(15.0) <= cast::<Ext, _>(ind_margin) {
                        best_margin = cast::<Ext, _>(ind_margin);
                        best_ind = ind_id;
                    }
                }
                if 2 < best_ind && best_ind < 70 {
                    g.tmp_ext1 = Ext::from_f64(0.0);
                    // rangeEnd = StartNewCorpOrExpand(bestInd, 0) (not read afterwards)
                    crate::logic::sim_entity::asset_sales::start_new_corp_or_expand(g, ui, rng, best_ind, 0, time_secs);
                }
            }
        } else if g.stock_index < lower_band {
            if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(2.0) > cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth_target)));
            }
            if (g.stock_index < lower_band * sf(0.67f32) || cycle_phase >= 0)
                && sf(2.5f32) * rng.rnd() + Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth_target)
            {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth_target)));
            }
            if Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) + cast::<Ext, _>(g.gdp_growth_target)));
            }
            growth_shift = 5;
            if g.stock_index < lower_band * sf(0.75f32) {
                growth_shift = 6;
                g.stock_index_divisor = sf(0.995f32) * g.stock_index_divisor;
            }
            if g.monetary_policy < 2 {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.interest_rate_target) - Ext::from_f64(2.0) * rng.rnd()));
            } else if !(sf(2.5f32) > cast::<Ext, _>(g.interest_rate_target)) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.interest_rate_target) - Ext::from_f64(1.0)));
            }
        }

        if growth_shift != 0 {
            for idx in 3..=69 {
                if idx == 50 {
                    continue;
                }
                if growth_shift > 0 && Ext::from_f64(25.0) > g.ind_demand_gap[idx] {
                    if (cast::<i32, _>(g.ind_growth_forecast[idx])) < growth_shift + cast::<i32, _>(g.ind_long_term_growth[idx]) {
                        g.ind_growth_forecast[idx] = g.ind_growth_forecast[idx].wrapping_add(3);
                    }
                    if g.monetary_stance == -1 && (g.cur_quarter + idx) % 3 != 2 {
                        if growth_shift == 6 {
                            g.ind_demand[idx] = to_quad(sf(1.01f32) * cast::<Ext, _>(g.ind_demand[idx]));
                        }
                        if (cast::<i32, _>(g.ind_long_term_growth[idx])) < cast::<i32, _>(g.ind_growth_rate_now[idx]) + 2 && Ext::from_f64(10.0) * rng.rnd() > Ext::from_f64(9.0) {
                            g.ind_long_term_growth[idx] = g.ind_long_term_growth[idx].wrapping_add(1);
                        }
                        for corp_id in 131..=1600 {
                            if g.entity_status[corp_id] != 1 || cast::<i32, _>(g.industry_num[corp_id]) != idx {
                                continue;
                            }
                            if get_managing_player(g, corp_id) > 0 || Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
                                continue;
                            }
                            let r = rng.rnd_range(3, 1);
                            if cast::<i32, _>(g.growth_rate[corp_id]) > r {
                                g.growth_rate[corp_id] = cast::<i16, _>(rng.rnd_range(11, 0) + -10);
                            }
                        }
                    }
                } else if growth_shift < 0 && Ext::from_f64(17.0) < g.ind_demand_gap[idx] {
                    if cast::<i32, _>(g.ind_growth_forecast[idx]) > growth_shift + cast::<i32, _>(g.ind_long_term_growth[idx]) {
                        g.ind_growth_forecast[idx] = g.ind_growth_forecast[idx].wrapping_sub(2);
                    }
                    if g.monetary_stance == 1 && (g.cur_quarter + idx) % 3 != 2 {
                        g.ind_demand[idx] = to_quad(sf(0.995f32) * cast::<Ext, _>(g.ind_demand[idx]));
                        if cast::<i32, _>(g.ind_long_term_growth[idx]) > cast::<i32, _>(g.ind_growth_rate_now[idx]) - 2 {
                            g.ind_long_term_growth[idx] = g.ind_long_term_growth[idx].wrapping_sub(1);
                        }
                        for corp_id in 131..=1600 {
                            if g.entity_status[corp_id] != 1 || cast::<i32, _>(g.industry_num[corp_id]) != idx {
                                continue;
                            }
                            if get_managing_player(g, corp_id) > 0 || Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
                                continue;
                            }
                            let r = rng.rnd_range(5, 1);
                            if (cast::<i32, _>(g.growth_rate[corp_id])) < r {
                                g.growth_rate[corp_id] = cast::<i16, _>(rng.rnd_range(11, 0) + cast::<i32, _>(g.growth_rate[corp_id]));
                            }
                        }
                    }
                }
                if (cast::<Ext, _>(g.ind_growth_forecast[idx])).abs() > Ext::from_f64(20.0) {
                    g.ind_growth_forecast[idx] = cast::<i16, _>(to_quad(sf(0.85f32) * cast::<Ext, _>(g.ind_growth_forecast[idx])));
                }
                if (cast::<Ext, _>(g.ind_growth_rate[idx])).abs() < Ext::from_f64(20.0) {
                    g.ind_growth_rate[idx] = cast::<i16, _>(to_quad(cast::<Ext, _>(growth_shift) * sf(0.4f32) + cast::<Ext, _>(g.ind_growth_rate[idx])));
                }
            }
        }

        if !(sf(0.7f32) < g.market_val_target) {
            sentiment_shift = -10;
        }
        if !(sf(1.3f32) > g.market_val_target) {
            sentiment_shift = 10;
        }
        let range_start = rng.rnd_range(799, 11);
        let range_end = rng.rnd_range(1600, 800);
        for idx in range_start..=range_end {
            if g.entity_status[idx] != 1 || g.industry_num[idx] == 71 {
                continue;
            }
            g.valuation_mode = 2;
            if sentiment_shift != 0 {
                g.investor_sentiment[idx] = sentiment_shift + g.investor_sentiment[idx];
            }
            let mut delta = g.market_cap[idx] * (sf(0.06f32) * rng.rnd() + sf(0.01f32));
            if g.monetary_stance == -1 {
                delta = -delta; // -1 * delta
            }
            g.market_cap[idx] = g.market_cap[idx] - delta;
        }
    }
    post_news(g, ui, 0, 0);
}

/// Port of GenMarketCyclePlan (0x4c4b18).
/// A random two-phase market cycle for games of 15+ years (`GameEndYear - StartYear`, capped at 50):
/// `"start1|end1|start2|end2"`, the phases 1..4 years long at 7..85 % of the game, not overlapping and
/// the second at most 4 years before the first; "" if no valid plan comes out (or the game is short).
pub fn gen_market_cycle_plan(g: &mut Globals, rng: &mut Rng) -> PbStr {
    let mut game_years = g.game_end_year - g.start_year;
    if game_years > 50 {
        game_years = 50;
    }
    if game_years < 15 {
        return PbStr::new();
    }
    let phase1_frac = sf(0.78f32) * rng.rnd() + sf(0.07f32);
    let phase1_start = to_long(cast::<Ext, _>(g.start_year) + cast::<Ext, _>(game_years) * phase1_frac);
    let mut plan_text = str_trim(cast::<Ext, _>(phase1_start)) + "|";
    let mut phase1_len = to_long(cast::<Ext, _>(game_years) * sf(0.13f32) + Ext::from_f64(1.0));
    if phase1_len > 4 {
        phase1_len = 4;
    }
    if phase1_len >= 3 {
        phase1_len = to_long(cast::<Ext, _>(phase1_len) - sf(2.4f32) * rng.rnd());
    }
    let phase1_end = phase1_len + phase1_start;
    plan_text = plan_text + &str_trim(cast::<Ext, _>(phase1_end)) + "|";
    let phase2_frac = sf(0.78f32) * rng.rnd() + sf(0.07f32);
    let phase2_start = to_long(cast::<Ext, _>(g.start_year) + cast::<Ext, _>(game_years) * phase2_frac);
    if !(phase1_start > phase2_start || phase1_end < phase2_start) {
        return PbStr::new();
    }
    if !(5 + phase2_start > phase1_start) {
        return PbStr::new();
    }
    plan_text = plan_text + &str_trim(cast::<Ext, _>(phase2_start)) + "|";
    let mut phase2_len = to_long(cast::<Ext, _>(game_years) * sf(0.15f32) + Ext::from_f64(1.0));
    if phase2_len > 4 {
        phase2_len = 4;
    }
    if phase2_len >= 3 {
        phase2_len = to_long(cast::<Ext, _>(phase2_len) - sf(2.4f32) * rng.rnd());
    }
    let mut phase2_end = phase2_len + phase2_start;
    let mut tries = 0;
    while phase1_start <= phase2_end && phase1_start > phase2_start {
        phase2_end -= 1;
        tries += 1;
        if tries > 50 {
            return PbStr::new();
        }
    }
    if !(2 + phase2_start < phase2_end) {
        return PbStr::new();
    }
    plan_text = plan_text + &str_trim(cast::<Ext, _>(phase2_end));
    plan_text
}

/// Port of MacroOutlookNews (0x4c1bc7).
/// Quarterly look at five years of history (skipped in a pandemic, oil or subprime crisis, when
/// `(CurYear + CurQuarter) MOD 4 = 3`, or before the oil history exists): a long run of high / low / middle
/// interest rates (series 1601, more than a random 40..50 of 60 months) moves `InterestRateTarget`,
/// `GdpGrowthTarget`, `BaseInterestRate` and `MonetaryPolicy` (with central-bank headlines); then a long
/// run of high or low oil, gold, silver and grain prices (series 6..9) rescales their target prices and
/// ranges with one "high/low [runtime resource wording] " outlook headline (each commodity once per game),
/// and in later years the index trend (series 1604) nudges the GDP target. Pops up and posts `NewsMsg`.
#[allow(unused_assignments)]
pub fn macro_outlook_news(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if (g.cur_year + g.cur_quarter) % 4 == 3 || 0 < g.pandemic_state || 0 < g.peak_oil_crisis || 0 < g.subprime_crisis_state {
        return;
    }
    if g.cap_hist_close[(48, 7)] == Ext::from_f64(0.0) {
        return;
    }
    let oil_headlines = [
        __content.string(4509),
        __content.string(4510),
        __content.string(4511),
        __content.string(4512),
        __content.string(4513),
    ];
    let pick = rng.rnd_range(5, 1);
    let mut outlook_msg = s(oil_headlines[cast::<usize, _>(pick - 1)]);
    let mut bank_short_name = ucase(g.central_bank_name.as_bytes(), CaseMode::Ansi);
    if g.currency_code == "USA" {
        bank_short_name = s("FED");
    }
    let (mut high_count, mut mid_count, mut low_count) = (0, 0, 0);
    for month in 1..=60 {
        if g.price_hist_high[(month, 1601)] > Ext::from_f64(6.0) {
            high_count += 1;
        } else if g.cap_hist_close[(month, 1601)] >= Ext::from_f64(4.0) && g.cap_hist_close[(month, 1601)] <= Ext::from_f64(6.0) {
            mid_count += 1;
        } else if g.price_hist_low[(month, 1601)] > Ext::from_f64(0.0) && g.price_hist_low[(month, 1601)] < Ext::from_f64(4.0) {
            low_count += 1;
        }
    }
    let threshold = rng.rnd_range(11, 1) + 39;
    let gdp = |g: &Globals| cast::<Ext, _>(g.gdp_growth);
    let irt = |g: &Globals| cast::<Ext, _>(g.interest_rate_target);
    if threshold < high_count && sf(4.2f32) < cast::<Ext, _>(g.base_interest_rate) {
        // long period of high rates
        if Ext::from_f64(2.0) * rng.rnd() + sf(2.5f32) < irt(g) {
            let x = Ext::from_f64(1.0) * rng.rnd() + sf(0.5f32);
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt(g) - x));
        }
        if Ext::from_f64(7.0) < irt(g) {
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt(g) - Ext::from_f64(3.0)));
        }
        if sf(3.4f32) < gdp(g) && Ext::from_f64(6.0) * rng.rnd() < Ext::from_f64(1.0) {
            let t = Ext::from_f64(1.0) + irt(g);
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(t + Ext::from_f64(2.0) * rng.rnd()));
        } else if sf(3.3f32) > gdp(g) && g.monetary_policy < 2 {
            if Ext::from_f64(5.0) * rng.rnd() < Ext::from_f64(1.0) && g.monetary_policy == 1 && sf(9.4f32) < cast::<Ext, _>(g.base_interest_rate) {
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.base_interest_rate) - sf(0.5f32)));
                g.monetary_policy = 0;
                g.news_msg = bank_short_name.clone() + __content.string(4514);
            }
            if -sf(8.5f32) > gdp(g) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) * rng.rnd() + sf(2.5f32)));
            }
        } else if sf(4.1f32) < gdp(g) && sf(8.5f32) < irt(g) {
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt(g) * sf(0.8f32)));
        }
    } else if threshold < low_count && sf(9.5f32) > cast::<Ext, _>(g.base_interest_rate) {
        // long period of low rates
        if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(10.0) > irt(g) {
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(2.5f32) * rng.rnd() + irt(g)));
        }
        if Ext::from_f64(6.0) > irt(g) {
            let t = Ext::from_f64(3.0) + irt(g);
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(t + cast::<Ext, _>(rng.rnd_range(2, 0))));
        }
        if Ext::from_f64(1.0) > gdp(g) && Ext::from_f64(6.0) * rng.rnd() < Ext::from_f64(1.0) {
            if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(7.0) < irt(g) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt(g) - Ext::from_f64(3.0)));
            }
        } else if sf(3.4f32) < gdp(g) && 0 < g.monetary_policy {
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) + irt(g)));
            if g.monetary_policy == 1 && sf(4.9f32) > cast::<Ext, _>(g.base_interest_rate) && Ext::from_f64(5.0) * rng.rnd() < Ext::from_f64(1.0) {
                g.news_msg = bank_short_name.clone() + __content.string(4515);
            } else if g.monetary_policy == 0 && Ext::from_f64(5.0) * rng.rnd() < Ext::from_f64(1.0) {
                g.news_msg = g.central_bank_name.clone() + __content.string(4516);
            }
            g.monetary_policy = 2;
            if sf(4.4f32) < gdp(g) && sf(5.8f32) > cast::<Ext, _>(g.base_interest_rate) {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.base_interest_rate) * sf(1.25f32)));
                g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(rng.rnd() + cast::<Ext, _>(g.base_interest_rate)));
            }
        }
    } else if threshold < mid_count {
        if low_count < high_count {
            let rate_target = crate::pb::numeric::double(sf(2.2f32) * rng.rnd() + Ext::from_f64(3.0)); // DOUBLE local
            if irt(g) > rate_target {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(rate_target));
            }
            g.monetary_policy = 0;
            if Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth_target)));
            }
        } else {
            let rate_target = Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(8.0);
            if irt(g) < rate_target {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(rate_target));
            }
            if Ext::from_f64(2.0) < cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(1.0)));
            }
        }
    } else if low_count + mid_count > 45 && g.monetary_stance == -1 {
        if Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth_target) && sf(3.5f32) > gdp(g) && Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth_target)));
        }
    } else if high_count + mid_count > 45 && g.monetary_stance == 1 {
        if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(2.0) < cast::<Ext, _>(g.gdp_growth_target) && sf(2.5f32) < gdp(g) && Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(1.0)));
        }
    } else if threshold >= low_count && threshold >= high_count && threshold >= mid_count {
        if Ext::from_f64(7.0) < irt(g) && sf(4.3f32) > gdp(g) {
            let x = Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(1.0);
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt(g) - x));
            if g.monetary_stance == -1 {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(irt(g) - Ext::from_f64(1.0)));
            }
        } else if Ext::from_f64(5.0) > irt(g) && Ext::from_f64(3.0) < gdp(g) {
            let t = Ext::from_f64(1.0) + irt(g);
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(t + rng.rnd()));
            if g.monetary_stance == 1 {
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + irt(g)));
            }
        }
    }
    post_news(g, ui, 0, 0);

    // oil (series 6)
    let (mut high_count, mut low_count) = (0, 0);
    for month in 1..=60 {
        if g.price_hist_high[(month, 6)] > Ext::from_f64(125.0) {
            high_count += 1;
        }
        if g.price_hist_low[(month, 6)] > Ext::from_f64(0.0) && g.price_hist_low[(month, 6)] < Ext::from_f64(80.0) {
            low_count += 1;
        }
    }
    if g.peak_oil_crisis != 1 {
        if threshold < high_count && sf(0.8f32) < cast::<Ext, _>(g.oil_target_price) {
            g.oil_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.77f32) * cast::<Ext, _>(g.oil_target_price)));
            if g.oil_outlook_news_done == 0 && sf(1.25f32) < cast::<Ext, _>(g.oil_price) {
                g.news_msg = outlook_msg.clone();
                outlook_msg = PbStr::new();
                g.oil_outlook_news_done = 1;
            }
        } else if threshold < low_count && sf(1.25f32) > cast::<Ext, _>(g.oil_target_price) {
            g.oil_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.3f32) * cast::<Ext, _>(g.oil_target_price)));
            if g.oil_outlook_news_done == 0 && sf(0.8f32) > cast::<Ext, _>(g.oil_price) {
                if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                    outlook_msg = replace(outlook_msg.as_bytes(), b"high", b"cheap");
                } else {
                    outlook_msg = replace(outlook_msg.as_bytes(), b"high", b"low");
                }
                g.news_msg = outlook_msg.clone();
                outlook_msg = PbStr::new();
                g.oil_outlook_news_done = 1;
            }
        }
    }
    'commodities: {
        if outlook_msg.is_empty() {
            break 'commodities;
        }
        // gold (series 7)
        let (mut high_count, mut low_count) = (0, 0);
        for month in 1..=60 {
            if g.price_hist_high[(month, 7)] > cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(2300.0) {
                high_count += 1;
            }
            if g.price_hist_low[(month, 7)] > Ext::from_f64(0.0) && g.price_hist_low[(month, 7)] < cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(1200.0) {
                low_count += 1;
            }
        }
        let mut low_check = true;
        if threshold < high_count && update_metals_price_base(g, ui) > Ext::from_f64(1225.0) {
            low_check = false;
            if (cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(900.0)) < cast::<Ext, _>(g.gold_target_price) {
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.83f32) * cast::<Ext, _>(g.gold_target_price)));
            }
            if (cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(500.0)) < cast::<Ext, _>(g.gold_price_min) {
                g.gold_price_min = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(500.0)));
            }
            if g.gold_outlook_news_done == 0 && update_metals_price_base(g, ui) > Ext::from_f64(2300.0) {
                outlook_msg = replace(outlook_msg.as_bytes(), b"oil", b"gold");
                g.news_msg = outlook_msg.clone();
                outlook_msg = PbStr::new();
                g.gold_outlook_news_done = 1;
            }
        }
        if low_check && threshold < low_count && update_metals_price_base(g, ui) < Ext::from_f64(1350.0) {
            if (cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(3200.0)) > cast::<Ext, _>(g.gold_target_price) {
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.2f32) * cast::<Ext, _>(g.gold_target_price)));
            }
            if (cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(4000.0)) > cast::<Ext, _>(g.gold_price_max) {
                g.gold_price_max = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(4200.0)));
            }
            if g.gold_outlook_news_done == 0 && update_metals_price_base(g, ui) < Ext::from_f64(1200.0) {
                outlook_msg = replace(outlook_msg.as_bytes(), b"oil", b"gold");
                outlook_msg = replace(outlook_msg.as_bytes(), b"high", b"low");
                g.news_msg = outlook_msg.clone();
                outlook_msg = PbStr::new();
                g.gold_outlook_news_done = 1;
            }
        }
        if outlook_msg.is_empty() {
            break 'commodities;
        }
        // silver (series 8)
        let base = |g: &Globals| cast::<Ext, _>(g.metals_price_base) * sf(0.01f32);
        let (mut high_count, mut low_count) = (0, 0);
        for month in 1..=60 {
            if g.price_hist_high[(month, 8)] > base(g) + Ext::from_f64(28.0) {
                high_count += 1;
            }
            if g.price_hist_low[(month, 8)] > Ext::from_f64(0.0) && g.price_hist_low[(month, 8)] < base(g) + Ext::from_f64(15.0) {
                low_count += 1;
            }
        }
        let silver = cast::<Ext, _>(g.silver_price);
        if threshold < high_count && base(g) + sf(18.5f32) < silver {
            if base(g) + Ext::from_f64(13.0) < cast::<Ext, _>(g.silver_target_price) {
                g.silver_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.75f32) * cast::<Ext, _>(g.silver_target_price)));
            }
            if base(g) + Ext::from_f64(9.0) < cast::<Ext, _>(g.silver_price_min) {
                g.silver_price_min = cast::<f32, _>(cast::<f32, _>(base(g) + Ext::from_f64(8.0)));
            }
            if g.silver_outlook_news_done == 0 && base(g) + Ext::from_f64(28.0) < silver {
                outlook_msg = replace(outlook_msg.as_bytes(), b"oil", b"silver");
                g.news_msg = outlook_msg.clone();
                outlook_msg = PbStr::new();
                g.silver_outlook_news_done = 1;
            }
        } else if threshold < low_count && base(g) + Ext::from_f64(19.0) > silver {
            if base(g) + Ext::from_f64(39.0) > cast::<Ext, _>(g.silver_target_price) {
                g.silver_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.2f32) * cast::<Ext, _>(g.silver_target_price)));
            }
            if base(g) + Ext::from_f64(67.0) < cast::<Ext, _>(g.silver_price_max) {
                g.silver_price_max = cast::<f32, _>(cast::<f32, _>(base(g) + Ext::from_f64(67.0)));
            }
            if g.silver_outlook_news_done == 0 && base(g) + Ext::from_f64(15.0) > silver {
                outlook_msg = replace(outlook_msg.as_bytes(), b"oil", b"silver");
                outlook_msg = replace(outlook_msg.as_bytes(), b"high", b"low");
                g.news_msg = outlook_msg.clone();
                outlook_msg = PbStr::new();
                g.silver_outlook_news_done = 1;
            }
        }
        if outlook_msg.is_empty() {
            break 'commodities;
        }
        // grain (series 9)
        let (mut high_count, mut low_count) = (0, 0);
        for month in 1..=60 {
            if g.price_hist_high[(month, 9)] > sf(8.8f32) {
                high_count += 1;
            }
            if g.price_hist_low[(month, 9)] > Ext::from_f64(0.0) && g.price_hist_low[(month, 9)] < sf(4.95f32) {
                low_count += 1;
            }
        }
        let wheat = cast::<Ext, _>(g.wheat_price);
        let corn = cast::<Ext, _>(g.corn_price);
        if threshold < high_count && sf(5.5f32) < wheat && sf(5.2f32) < corn {
            if sf(4.6f32) < cast::<Ext, _>(g.wheat_target_price) {
                g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.85f32) * cast::<Ext, _>(g.wheat_target_price)));
                g.corn_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.83f32) * cast::<Ext, _>(g.corn_target_price)));
            }
            if sf(2.4f32) < cast::<Ext, _>(g.wheat_price_min) {
                g.wheat_price_min = cast::<f32, _>(Ext::from_f32(1.7f32));
                g.corn_price_min = cast::<f32, _>(Ext::from_f32(1.8f32));
            }
            if g.wheat_outlook_news_done == 0 && sf(8.8f32) < wheat {
                outlook_msg = replace(outlook_msg.as_bytes(), b"oil", b"grain");
                g.news_msg = outlook_msg.clone();
                outlook_msg = PbStr::new();
                g.wheat_outlook_news_done = 1;
            }
        } else if threshold < low_count && sf(5.4f32) > wheat && Ext::from_f64(5.0) > corn {
            if Ext::from_f64(9.0) > cast::<Ext, _>(g.wheat_target_price) {
                g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.2f32) * cast::<Ext, _>(g.wheat_target_price)));
                g.corn_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.2f32) * cast::<Ext, _>(g.corn_target_price)));
            }
            if Ext::from_f64(14.0) > cast::<Ext, _>(g.wheat_price_max) {
                g.wheat_price_max = cast::<f32, _>(Ext::from_f32(14.7f32));
                g.corn_price_max = cast::<f32, _>(Ext::from_f32(13.9f32));
            }
            if g.wheat_outlook_news_done == 0 && sf(4.95f32) > wheat {
                outlook_msg = replace(outlook_msg.as_bytes(), b"oil", b"grain");
                outlook_msg = replace(outlook_msg.as_bytes(), b"high", b"low");
                g.news_msg = outlook_msg.clone();
                outlook_msg = PbStr::new();
                g.wheat_outlook_news_done = 1;
            }
        }
        // stock index trend (series 1604)
        if 4 + g.start_year < g.cur_year {
            let (mut high_count, mut low_count) = (0, 0);
            for month in 1..=60 {
                if g.cap_hist_close[(month, 1604)] > sf(2.3f32) {
                    high_count += 1;
                }
                if g.cap_hist_close[(month, 1604)] != Ext::from_f64(0.0) && g.cap_hist_close[(month, 1604)] < sf(0.4f32) {
                    low_count += 1;
                }
            }
            if threshold < high_count || low_count < 10 {
                if Ext::from_f64(1.0) < cast::<Ext, _>(g.gdp_growth_target) {
                    let r = cast::<Ext, _>(rng.rnd_range(4, 1));
                    g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - r));
                }
            } else if (threshold < low_count || high_count < 10) && Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth_target) {
                let r = cast::<Ext, _>(rng.rnd_range(3, 1));
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(r + cast::<Ext, _>(g.gdp_growth_target)));
            }
        }
    }
    if !g.news_msg.is_empty() && g.suppress_popups == 0 {
        let mut msg = std::mem::take(&mut g.news_msg);
        show_quick_message(g, ui, &mut msg);
        g.news_msg = msg;
    }
    post_news(g, ui, 0, 0);
}

/// Port of MarketReversalEvent (0x4c3454).
/// With `do_reversal = 0` only schedules the next reversal (`MarketReversalTick = SimCoCount + 200 +
/// 350 * RND`). Otherwise clears the schedule and, if the index has a trend streak (`PriceTrendStreak(0)`;
/// the longer the likelier), moves about two thirds of all stocks priced 4.80+ by 6..14 % against the
/// trend (sentiment +-15 for a third of them), shifts the index futures basis by +-150, sets
/// `MarketValTarget`/`MarketValuation` and builds a rally / sell-off headline; half the time it is posted
/// (for `CurPlayer`), the market display refreshed and an aftershock scheduled (`MarketAftershockTick`,
/// negative after a sell-off).
pub fn market_reversal_event(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, do_reversal: i32) {
    let __content = crate::content::current();
    if do_reversal == 0 {
        let base = Ext::from_f64(200.0) + cast::<Ext, _>(g.sim_co_count);
        g.market_reversal_tick = to_long(base + Ext::from_f64(350.0) * rng.rnd());
        return;
    }
    g.market_reversal_tick = 0;
    let streak = price_trend_streak(g, 0);
    if streak == 0 {
        return;
    }
    let pick = rng.rnd_range(60, 1);
    if Ext::from_f64(60.0) - (cast::<Ext, _>(streak)).abs() * Ext::from_f64(3.0) > cast::<Ext, _>(pick) {
        return;
    }
    let mut move_factor = (Ext::from_f64(8.0) * rng.rnd() + Ext::from_f64(6.0)) * sf(0.01f32) + Ext::from_f64(1.0);
    if sf(1.1f32) < move_factor && (cast::<Ext, _>(streak)).abs() < Ext::from_f64(5.0) {
        move_factor = sf(0.75f32) * move_factor;
    }
    if streak > 0 {
        move_factor = Ext::from_f64(1.0) - (move_factor - Ext::from_f64(1.0));
    }
    for corp_id in 11..=1600 {
        if g.entity_status[corp_id] != 1 {
            continue;
        }
        if !(g.market_cap[corp_id] / g.shares_outstanding[corp_id] >= sf(4.8f32)) || Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
            continue;
        }
        g.market_cap[corp_id] = move_factor * g.market_cap[corp_id];
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
            if Ext::from_f64(1.0) >= move_factor {
                g.investor_sentiment[corp_id] = -15 + g.investor_sentiment[corp_id];
            } else {
                g.investor_sentiment[corp_id] = 15 + g.investor_sentiment[corp_id];
            }
        }
    }
    if Ext::from_f64(1.0) > move_factor {
        g.futures_basis[6] = g.futures_basis[6].wrapping_sub(150);
        g.news_msg = s(__content.string(4517));
        if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
            g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(4518), __content.bytes(4519));
        }
        if (cast::<Ext, _>(streak)).abs() > Ext::from_f64(7.0) {
            g.news_msg = replace(g.news_msg.as_bytes(), b"winning", __content.bytes(4520));
        }
        if g.rand_seed % 4 == 1 {
            g.news_msg = s(__content.string(4521));
        }
        if g.rand_seed % 4 == 2 && g.monetary_policy == 2 && Ext::from_f64(9.0) < cast::<Ext, _>(g.base_interest_rate) {
            g.news_msg = s(__content.string(4522));
        }
        if g.rand_seed % 4 == 2 && Ext::from_f64(1.0) > cast::<Ext, _>(g.gdp_growth) {
            g.news_msg = s(__content.string(4523));
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                g.news_msg = replace(g.news_msg.as_bytes(), b"lower", __content.bytes(4524));
            }
        }
        if g.rand_seed % 4 == 3 {
            g.news_msg = s(__content.string(4525));
            if streak > 5 {
                g.news_msg = g.news_msg.clone() + __content.string(4526);
                if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
                    g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(4527), __content.bytes(4528));
                }
            } else {
                g.news_msg = g.news_msg.clone() + __content.string(4529);
            }
        }
        g.market_val_target = cast::<f64, _>(sf(0.8f32) * g.market_val_target);
        if sf(0.62f32) < cast::<Ext, _>(g.market_valuation) {
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.8f32) * cast::<Ext, _>(g.market_valuation)));
        }
    } else {
        g.futures_basis[6] = g.futures_basis[6].wrapping_add(150);
        g.news_msg = s(__content.string(4530));
        if Ext::from_f64(10.0) * rng.rnd() > Ext::from_f64(9.0) {
            g.news_msg = g.news_msg.clone() + __content.string(4531);
            if g.rand_seed % 3 == 1 {
                g.news_msg = replace(g.news_msg.as_bytes(), b"scant", b"no");
            }
        } else if sf(1.12f32) < move_factor {
            g.news_msg = replace(g.news_msg.as_bytes(), b"soar", __content.bytes(4532));
        } else if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
            g.news_msg = replace(g.news_msg.as_bytes(), b"soar", __content.bytes(4533));
            if g.rand_seed % 3 == 2 {
                g.news_msg = replace(g.news_msg.as_bytes(), b"mount", b"put on");
            }
        }
        if sf(3.5f32) < cast::<Ext, _>(g.gdp_growth) {
            g.news_msg = s(__content.string(4534));
        }
        if g.rand_seed % 7 == 1 {
            g.news_msg = s(__content.string(4535));
        }
        if g.rand_seed % 7 == 2 && streak < -5 {
            g.news_msg = s(__content.string(4536));
        }
        if g.rand_seed % 7 == 3 && Ext::from_f64(0.0) > cast::<Ext, _>(g.gdp_growth) {
            g.news_msg = s(__content.string(4537));
        }
        if g.rand_seed % 7 == 4 {
            g.news_msg = s(__content.string(4538));
        }
        g.market_val_target = cast::<f64, _>(sf(1.25f32) * g.market_val_target);
        if sf(1.25f32) > cast::<Ext, _>(g.market_valuation) {
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.2f32) * cast::<Ext, _>(g.market_valuation)));
        }
    }
    if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
        let cur = g.cur_player;
        post_news(g, ui, cur, 0);
        g.skip_index_move_news = 1;
        update_market_display(g, ui, rng, time_secs);
        let base = Ext::from_f64(100.0) + cast::<Ext, _>(g.sim_co_count);
        g.market_aftershock_tick = to_long(base + Ext::from_f64(150.0) * rng.rnd());
        if g.market_aftershock_tick > 1585 {
            g.market_aftershock_tick = to_long(Ext::from_f64(100.0) * rng.rnd() + Ext::from_f64(50.0));
        }
        if Ext::from_f64(1.0) > move_factor {
            g.market_aftershock_tick = g.market_aftershock_tick.wrapping_mul(-1);
        }
    }
}

/// Port of MarketAftershock (0x4c3d74).
/// Three times in four, moves about a third of all stocks priced 4.80+ by 8..14 % (down after a sell-off
/// three times in four, after a rally one time in four), recording a price tick for each and shifting the
/// sentiment of two thirds of them by `25 * factor`; then (`SimCoCount MOD 3 = 0`) schedules another
/// aftershock with the same sign. Clears `MarketAftershockTick` first.
pub fn market_aftershock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let prev_tick = g.market_aftershock_tick;
    g.market_aftershock_tick = 0;
    if Ext::from_f64(4.0) * rng.rnd() > Ext::from_f64(3.0) {
        return;
    }
    let mut move_factor = sf(0.01f32) * (Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(8.0)) + Ext::from_f64(1.0);
    if prev_tick < 0 {
        if Ext::from_f64(4.0) * rng.rnd() > Ext::from_f64(1.0) {
            move_factor = Ext::from_f64(1.0) - (move_factor - Ext::from_f64(1.0));
        }
    } else if Ext::from_f64(4.0) * rng.rnd() > Ext::from_f64(3.0) {
        move_factor = Ext::from_f64(1.0) - (move_factor - Ext::from_f64(1.0));
    }
    for corp_id in 11..=1600 {
        if g.entity_status[corp_id] != 1 {
            continue;
        }
        if !(g.market_cap[corp_id] / g.shares_outstanding[corp_id] >= sf(4.8f32)) || Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
            continue;
        }
        g.market_cap[corp_id] = move_factor * g.market_cap[corp_id];
        update_price_history_tick(g, ui, rng, corp_id);
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
            g.investor_sentiment[corp_id] = cast::<i32, _>(to_quad(Ext::from_f64(25.0) * move_factor + cast::<Ext, _>(g.investor_sentiment[corp_id])));
        }
    }
    if g.sim_co_count % 3 <= 0 {
        let base = Ext::from_f64(200.0) + cast::<Ext, _>(g.sim_co_count);
        g.market_aftershock_tick = to_long(base + Ext::from_f64(150.0) * rng.rnd());
        if g.market_aftershock_tick > 1585 || g.market_aftershock_tick < 25 {
            g.market_aftershock_tick = to_long(Ext::from_f64(100.0) * rng.rnd() + Ext::from_f64(50.0));
        }
        if prev_tick < 0 {
            g.market_aftershock_tick = g.market_aftershock_tick.wrapping_mul(-1);
        }
    }
}

/// Port of GovFiscalPolicyNews (0x4c5d0b).
/// Outside the subprime and oil crises, with GDP growth at most -2 % or at least 3.5 %: on tick 610 (not
/// in a tight-money stance) more than 14 of 60 index months below -2.5 bring deficit spending (a higher
/// `GdpGrowthTarget` and a headline when GDP falls below -2 %); otherwise (not in an easing stance) more
/// than 18 months above 4.1 cool the economy (lower target, inflation headline above 4 % growth).
/// Sets `NewsMsg` only (the caller posts it).
pub fn gov_fiscal_policy_news(g: &mut Globals, rng: &mut Rng) {
    let __content = crate::content::current();
    if !(-Ext::from_f64(2.0) >= cast::<Ext, _>(g.gdp_growth) || sf(3.5f32) <= cast::<Ext, _>(g.gdp_growth)) {
        return;
    }
    if 0 < g.subprime_crisis_state || !(Ext::from_f64(0.0) >= cast::<Ext, _>(g.oil_crisis_level)) {
        return;
    }
    let mut month_count = 0;
    if g.sim_co_count == 610 && g.monetary_stance <= 0 {
        for month in 1..=60 {
            if g.cap_hist_close[(month, 1604)] < -sf(2.5f32) {
                month_count += 1;
            }
        }
        if month_count > 14 {
            if -Ext::from_f64(2.0) > cast::<Ext, _>(g.gdp_growth_target) {
                let t = Ext::from_f64(2.0) + cast::<Ext, _>(g.gdp_growth_target);
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(t + sf(1.5f32) * rng.rnd()));
            }
            if -Ext::from_f64(2.0) > cast::<Ext, _>(g.gdp_growth) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(4.0) * rng.rnd()));
                let mut verb_text = s("seeks");
                if g.rand_seed % 3 == 1 {
                    verb_text = s("hopes");
                }
                if g.rand_seed % 3 == 2 {
                    verb_text = s("trying");
                }
                g.news_msg = s(__content.string(4539)) + &verb_text + __content.string(4540);
            }
        }
    } else if g.monetary_stance != -1 {
        for month in 1..=60 {
            if g.cap_hist_close[(month, 1604)] > sf(4.1f32) {
                month_count += 1;
            }
        }
        if month_count > 18 {
            if Ext::from_f64(3.0) < cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(sf(1.5f32) * rng.rnd()));
            }
            if Ext::from_f64(4.0) < cast::<Ext, _>(g.gdp_growth) && Ext::from_f64(2.0) < cast::<Ext, _>(g.gdp_growth_target) {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(-Ext::from_f64(2.0) + sf(2.5f32) * rng.rnd()));
                g.news_msg = g.central_bank_name.clone() + __content.string(4541);
            }
        }
    }
}

//! Dividend policy and distributions of the per-entity simulation (see [`super`]): `AiSetDividend`
//! (0x49b683), `AiCorpGrowthAndDividends` (0x6b2a08), `CalcConsolidatedTaxPayment` (0x4fa767),
//! `AdjustPriceForDistribution` (0x619ed2), `EtfAddToHoldings` (0x64f921), `AdjustOptionsForCorpAction`
//! (0x5f7680), `ChargeShortsForDistribution` (0x5e2f14).

// x * -1 and two-sided clamps keep the original BASIC shape.
#![allow(clippy::neg_multiply, clippy::manual_clamp)]
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::cashflow::{get_etf_adviser_human_status, show_cash_flow_projection};
use crate::logic::control::update_corp_control;
use crate::logic::earnings::balance_cash_t_bills;
use crate::logic::finance::{get_controller_type, get_managing_player};
use crate::logic::futures::update_metals_price_base;
use crate::logic::holdings::{find_holding_slot, get_largest_holding_id};
use crate::logic::report::number_to_words;
use crate::logic::shorts::find_short_positions;
use crate::logic::valuation::{
    calc_dividend_yield, calc_industry_outlook, get_mgmt_margin_vs_industry,
    update_price_history_tick,
};
use crate::pb::numeric::{Ext80, str_num_ext, val_ext};

// ---------------------------------------------------------------------------------------------------------
// AiSetDividend
// ---------------------------------------------------------------------------------------------------------

/// Locals of AiSetDividend shared with its GOSUB AiSetDividend_CalcPayout.
struct SetDivFrame {
    /// `yieldPct AS DOUBLE` (-0x90).
    yield_pct: Ext,
    /// `payoutPct AS INTEGER` (-0x94).
    payout_pct: i16,
}

/// GOSUB AiSetDividend_CalcPayout (0x49d403): dividend yield, `TmpSharePrice` and payout ratio (% of
/// last year's earnings, 150 if they were not positive). Returns false where the GOSUB jumps to the
/// routine's exit (no shares or no prior-year earnings).
fn set_div_calc_payout(g: &mut Globals, f: &mut SetDivFrame, corp: i32) -> bool {
    f.yield_pct = crate::pb::numeric::double(calc_dividend_yield(g, corp));
    if g.shares_outstanding[corp] == Ext::from_f64(0.0) || g.earnings_y1[corp] == Ext::from_f64(0.0) {
        return false;
    }
    g.tmp_share_price = g.market_cap[corp] / g.shares_outstanding[corp];
    let total_div: i64 = to_quad(Ext::from_f64(100.0) * g.dividend_per_share[corp] * g.shares_outstanding[corp]);
    let payout_ratio: Ext = cast::<Ext, _>(total_div) / g.earnings_y1[corp];
    f.payout_pct = to_integer(Ext::from_f64(100.0) * payout_ratio);
    if !(g.earnings_y1[corp] > Ext::from_f64(0.0)) {
        f.payout_pct = 150;
    }
    true
}

/// Port of AiSetDividend (0x49b683) with its GOSUB AiSetDividend_CalcPayout (0x49d403).
/// Adjusts `DividendPerShare(corp_id)` of an active non-fund corp from its payout ratio, dividend yield,
/// earnings (`qtr_earnings` = this quarter's net earnings), liquidity, credit rating, industry, net worth
/// and the monetary stance (tight money: cut, easy money: raise). `hold_flag <> 0` (a managing player)
/// keeps the dividend except for the forced cuts of a weak balance sheet. Every call site passes
/// `(entId, netEarnings, NetWorth, managingPlayer)` (all BYREF, none written).
/// Returns 1 / -1 if the dividend rose / fell by more than 0.2 %, else 0 — and 0 on the early exits
/// (inactive, fund, no shares or no prior-year earnings) and after the cash-rich special raise.
/// Also sets `TmpSharePrice` (market cap / shares).
pub fn ai_set_dividend(
    g: &mut Globals,
    rng: &mut Rng,
    corp_id: i32,
    qtr_earnings: Ext,
    net_worth: Ext,
    hold_flag: i32,
) -> i32 {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let c = corp_id;
    if c < 11 || g.entity_status[c] != 1 {
        return 0;
    }
    if g.industry_num[c] == 71 {
        return 0;
    }
    let old_div: Ext = g.dividend_per_share[c];
    let mut f = SetDivFrame {
        yield_pct: crate::pb::numeric::double(Ext::from_f64(0.0)),
        payout_pct: 0,
    };
    if !set_div_calc_payout(g, &mut f, c) {
        return 0;
    }
    let mut changed: i32 = 0;
    let mut action: i32 = 0;
    let mut raise_factor: Ext = Ext::from_f64(0.0);

    let mut stance_branch_done = false;
    if g.monetary_stance == 1 && get_controller_type(g, c) != 1 {
        stance_branch_done = true;
        if (cast::<i32, _>(f.payout_pct)) < rng.rnd_range(70, 40) && 3 < g.credit_rating_num[c] {
            if g.dividend_per_share[c] >= g.tmp_share_price * sf(0.01f32) {
                let r = rng.rnd_range(25, 10);
                g.dividend_per_share[c] = (sf(0.01f32) * cast::<Ext, _>(r) + Ext::from_f64(1.0)) * g.dividend_per_share[c];
            } else {
                g.dividend_per_share[c] = g.tmp_share_price * sf(0.02f32);
            }
        }
    }
    if !stance_branch_done && g.monetary_stance == -1 && hold_flag == 0 && g.industry_num[c] != 69 {
        if (cast::<i32, _>(f.payout_pct)) > rng.rnd_range(40, 20) {
            let r = rng.rnd_range(83, 65);
            g.dividend_per_share[c] = (sf(0.01f32) * cast::<Ext, _>(r)) * g.dividend_per_share[c];
        }
    }

    if hold_flag == 0 && g.monetary_stance == 0 {
        raise_factor = sf(0.2f32) * rng.rnd() + sf(1.1f32);
        action = 0;
        let qtr = qtr_earnings;
        let rated_path = qtr > g.earnings_y1[c] * sf(0.32f32)
            || 7 < g.credit_rating_num[c]
            || (cast::<Ext, _>(g.credit_rating_num[c]) - Ext::from_f64(3.0)) > f.yield_pct;
        if rated_path {
            // 0x49ba77
            if get_controller_type(g, c) != 1 {
                let sel = cast::<Ext, _>(g.industry_num[c]);
                let liquid = |g: &Globals| g.t_bills[c] + g.cash[c];
                let payout = cast::<Ext, _>(f.payout_pct);
                let yld = f.yield_pct;
                'sel: {
                    if sel == Ext::from_f64(1.0) {
                        if qtr > g.earnings_y1[c] * sf(0.32f32)
                            && 7 < g.credit_rating_num[c]
                            && (cast::<i32, _>(f.payout_pct)) < g.rand_seed % 20 + 40
                            && Ext::from_f64(8.0) > yld
                        {
                            action = 1;
                        }
                        if 8 < g.credit_rating_num[c] {
                            if Ext::from_f64(9.0) > yld && (cast::<i32, _>(f.payout_pct)) < 91 {
                                action = 1;
                            }
                            if g.exchange_rate * Ext::from_f64(100000.0) < liquid(g)
                                && (cast::<i32, _>(f.payout_pct)) < 130
                                && Ext::from_f64(12.0) > yld
                            {
                                raise_factor = sf(1.1f32) * raise_factor;
                                action = 1;
                                break 'sel;
                            }
                        }
                        if g.bank_debt[c] > Ext::from_f64(0.0) {
                            action = 0;
                        } else if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(4.0) < yld {
                            action = 0;
                        }
                        if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(6.0) < yld {
                            action = -1;
                        }
                    } else if sel == Ext::from_f64(2.0) {
                        if qtr > g.earnings_y1[c] * sf(0.3f32)
                            && 5 < g.credit_rating_num[c]
                            && g.exchange_rate * Ext::from_f64(6000.0) < liquid(g)
                        {
                            action = 1;
                        }
                        if 7 < g.credit_rating_num[c]
                            && g.ins_reserves[c] < liquid(g)
                            && sf(8.5f32) > yld
                        {
                            action = 1;
                        }
                        if net_worth > g.exchange_rate * Ext::from_f64(10000.0)
                            && g.exchange_rate * Ext::from_f64(10000.0) < liquid(g)
                            && Ext::from_f64(30.0) * rng.rnd() + Ext::from_f64(40.0) > payout
                        {
                            action = 1;
                        }
                        if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(6.0) < yld {
                            action = 0;
                        }
                        if Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(9.0) < yld {
                            action = -1;
                        }
                    } else if sel == Ext::from_f64(69.0) {
                        if qtr > g.exchange_rate * Ext::from_f64(100.0)
                            && 6 < g.credit_rating_num[c]
                            && Ext::from_f64(20.0) * rng.rnd() + Ext::from_f64(50.0) > payout
                        {
                            action = 1;
                        }
                        if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(7.0) < yld {
                            action = 0;
                        }
                        if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(8.0) < yld {
                            action = -1;
                        }
                        if g.exchange_rate * Ext::from_f64(40000.0) < liquid(g)
                            && Ext::from_f64(25.0) * rng.rnd() + Ext::from_f64(90.0) > payout
                            && Ext::from_f64(10.0) > yld
                        {
                            action = 1;
                        }
                    } else if sel == Ext::from_f64(70.0) {
                        if 7 < g.credit_rating_num[c] && Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(3.0) > yld {
                            action = 1;
                        }
                        if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(4.0) < yld {
                            action = 0;
                        }
                        if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(6.0) < yld {
                            action = -1;
                        }
                    } else {
                        let ind = cast::<i32, _>(g.industry_num[c]);
                        if 5 < g.credit_rating_num[c]
                            && g.business_assets[c] * sf(0.7f32) < liquid(g)
                            && g.ind_long_term_growth[ind] < 11
                        {
                            action = 1;
                        }
                        if g.business_assets[c] > g.exchange_rate * Ext::from_f64(100.0)
                            && 6 < g.credit_rating_num[c]
                            && Ext::from_f64(30.0) * rng.rnd() + Ext::from_f64(35.0) > payout
                        {
                            action = 1;
                        }
                        if qtr > g.earnings_y1[c] * sf(0.32f32) && 7 < g.credit_rating_num[c] {
                            action = 1;
                        }
                        if Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(20.0) < cast::<Ext, _>(g.growth_rate[c])
                            && g.ind_long_term_growth[ind] > 10
                        {
                            action = 0;
                        }
                        if Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(25.0) < cast::<Ext, _>(g.growth_rate[c])
                            || Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(2.0) < yld
                        {
                            action = 0;
                        }
                        if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(5.0) < yld {
                            action = -1;
                        }
                        if g.exchange_rate * Ext::from_f64(75000.0) < liquid(g)
                            && Ext::from_f64(25.0) * rng.rnd() + Ext::from_f64(90.0) > payout
                            && Ext::from_f64(9.0) > yld
                        {
                            action = 1;
                        }
                    }
                }
                // 0x49c5ef
                if action == 1 && (qtr < Ext::from_f64(0.0) || g.earnings_ytd[c] < Ext::from_f64(0.0)) {
                    action = 0;
                }
            }
        } else {
            // 0x49c656
            let yld = f.yield_pct;
            let payout = cast::<Ext, _>(f.payout_pct);
            let cut_checks = qtr < g.earnings_y1[c] * sf(0.15f32)
                || 6 > g.credit_rating_num[c]
                || Ext::from_f64(30.0) * rng.rnd() + Ext::from_f64(70.0) < payout
                || !(Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(10.0) >= yld);
            if cut_checks {
                // 0x49c708
                if qtr < Ext::from_f64(0.0) && 5 > g.credit_rating_num[c] && g.tax_loss_carry[c] < Ext::from_f64(0.0) {
                    action = -1;
                }
                if (cast::<Ext, _>(g.credit_rating_num[c]) + Ext::from_f64(1.0)) < yld
                    && !(g.cash[c] > Ext::from_f64(0.0))
                    && qtr < g.earnings_y1[c] * sf(0.1f32)
                {
                    action = -1;
                }
                if 5 > g.credit_rating_num[c] && (cast::<i32, _>(f.payout_pct)) > 100 {
                    action = -1;
                }
                if Ext::from_f64(8.0) < yld && 7 > g.credit_rating_num[c] && g.industry_num[c] != 69 {
                    action = -1;
                }
                if g.industry_num[c] == 1 && Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(3.0) < yld {
                    action = -1;
                }
                if Ext::from_f64(9.0) * rng.rnd() + Ext::from_f64(12.0) < yld {
                    action = -1;
                }
            } else {
                // 0x49c990
                let sel = cast::<Ext, _>(g.industry_num[c]);
                if sel == Ext::from_f64(1.0) || sel == Ext::from_f64(2.0) {
                    if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(5.0) < yld {
                        action = -1;
                    }
                } else if sel == Ext::from_f64(69.0) {
                    if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(7.0) < yld {
                        action = -1;
                    }
                } else if sel == Ext::from_f64(70.0) {
                    if Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(4.0) < yld {
                        action = -1;
                    }
                } else if Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(20.0) < cast::<Ext, _>(g.growth_rate[c])
                    || Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(6.0) < yld
                {
                    action = -1;
                }
                // 0x49cb0c
                if g.tax_loss_carry[c] < g.exchange_rate * -Ext::from_f64(1000.0) {
                    action = -1;
                }
            }
        }
        // 0x49cb53
        let liquid = g.t_bills[c] + g.cash[c];
        if g.exchange_rate * Ext::from_f64(100000.0) < liquid || 8 < g.credit_rating_num[c] {
            // 0x49cbd5
            if get_controller_type(g, c) != 1
                && g.monetary_stance != -1
                && g.industry_num[c] <= 69
                && 131 <= c
                && !(g.cash[c] < g.exchange_rate * Ext::from_f64(1000.0))
                && Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(6.0) >= f.yield_pct
            {
                if g.dividend_per_share[c] < g.tmp_share_price * sf(0.01f32) {
                    g.dividend_per_share[c] = g.tmp_share_price * sf(0.02f32);
                }
                let x = sf(0.1f32) * rng.rnd() + sf(1.2f32);
                g.dividend_per_share[c] = x * g.dividend_per_share[c];
                // jumps to the exit: the result is still 0
                return 0;
            }
        }
        // 0x49cd90
        if action == 1 && g.tax_loss_carry[c] < g.exchange_rate * -Ext::from_f64(200.0) {
            action = 0;
        }
        if action == 1 && raise_factor != Ext::from_f64(0.0) {
            if get_controller_type(g, c) != 1 {
                g.dividend_per_share[c] = raise_factor * g.dividend_per_share[c];
                if g.dividend_per_share[c] == Ext::from_f64(0.0) {
                    g.dividend_per_share[c] = (sf(0.01f32) * raise_factor) * g.tmp_share_price;
                }
                changed = 1;
            }
        } else if action == -1 {
            raise_factor = sf(0.3f32) * rng.rnd() + sf(0.5f32);
            if 5 > g.credit_rating_num[c] || Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(12.0) < f.yield_pct {
                raise_factor = sf(0.4f32) * raise_factor;
            }
            g.dividend_per_share[c] = raise_factor * g.dividend_per_share[c];
            changed = -1;
        }
    }
    // 0x49cfc1
    if changed != 0 && !set_div_calc_payout(g, &mut f, c) {
        return 0;
    }
    if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(8.0) < f.yield_pct && (cast::<i32, _>(f.payout_pct)) > 110 {
        action = -1;
    }
    if g.industry_num[c] > 1 {
        action = 0;
        let skip = !(net_worth < Ext::from_f64(0.0)) && 6 <= g.credit_rating_num[c];
        if !skip {
            if g.industry_num[c] == 2 {
                if qtr_earnings < g.earnings_y1[c] * sf(0.15f32)
                    && Ext::from_f64(15.0) * rng.rnd() + Ext::from_f64(40.0) < cast::<Ext, _>(f.payout_pct)
                {
                    action = -1;
                }
                if 5 > g.credit_rating_num[c] || !(Ext::from_f64(8.0) >= f.yield_pct) {
                    action = -1;
                }
            } else {
                if qtr_earnings < Ext::from_f64(0.0) && 5 > g.credit_rating_num[c] && Ext::from_f64(5.0) < f.yield_pct {
                    action = -1;
                }
                if 4 > g.credit_rating_num[c] && Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(4.0) < f.yield_pct {
                    action = -1;
                }
            }
            if net_worth > Ext::from_f64(0.0) {
                if action == -1 {
                    raise_factor = sf(0.3f32) * rng.rnd() + sf(0.3f32);
                    g.dividend_per_share[c] = raise_factor * g.dividend_per_share[c];
                }
            } else {
                g.dividend_per_share[c] = Ext::from_f64(0.0);
            }
        }
    }
    let _ = action;
    let _ = raise_factor;
    // 0x49d30d
    if g.dividend_per_share[c] < sf(0.02f32) {
        g.dividend_per_share[c] = Ext::from_f64(0.0);
    }
    changed = 0;
    if g.dividend_per_share[c] > old_div * sf(1.002f32) {
        changed = 1;
    }
    if g.dividend_per_share[c] < old_div * sf(0.998f32) {
        changed = -1;
    }
    changed
}

// ---------------------------------------------------------------------------------------------------------
// AiCorpGrowthAndDividends
// ---------------------------------------------------------------------------------------------------------

/// Locals of AiCorpGrowthAndDividends shared with its GOSUBs.
struct GrowthFrame {
    /// `corpId` (-0x88): copy of the BYREF argument.
    corp_id: i32,
    /// `industry AS INTEGER` (-0x94).
    industry: i16,
    /// `oldGrowth AS INTEGER` (-0x90).
    old_growth: i16,
    /// `marketSharePct AS SINGLE` (-0x98).
    market_share_pct: f32,
    /// operating score (-0xa8): margin + industry demand gap.
    score: i32,
    /// `targetGrowth` (-0xb8).
    target_growth: i32,
    /// cash-flow status from ShowCashFlowProjection (-0xbc).
    cf_status: i32,
    /// `payoutPct AS INTEGER` (-0xc0).
    payout_pct: i16,
    /// `divYield AS DOUBLE` (-0xc8).
    div_yield: Ext,
}

/// GOSUB AiCorpGrowth_CalcGoodCreditPct (0x6b3e47): `NonFinCorpCount`, `GoodCreditCount` and
/// `GoodCreditPct` (% of active non-financial operating corps 86..1600 rated above 6).
fn acg_calc_good_credit_pct(g: &mut Globals) {
    g.non_fin_corp_count = g.active_corp_count - 60;
    g.good_credit_count = 0;
    for scan_id in 86..=1600 {
        if g.entity_status[scan_id] == 1 {
            if g.industry_num[scan_id] > 69 {
                g.non_fin_corp_count -= 1;
            } else if 6 < g.credit_rating_num[scan_id] {
                g.good_credit_count += 1;
            }
        }
    }
    if 0 < g.non_fin_corp_count {
        g.good_credit_pct =
            to_long(Ext::from_f64(100.0) * (cast::<Ext, _>(g.good_credit_count) / cast::<Ext, _>(g.non_fin_corp_count)));
    }
}

/// GOSUB AiCorpGrowth_CalcPayout (0x6b3f1d): dividend yield and payout ratio (10 % / 100 % without
/// shares or prior-year earnings).
fn acg_calc_payout(g: &mut Globals, f: &mut GrowthFrame) {
    let c = f.corp_id;
    f.div_yield = crate::pb::numeric::double(calc_dividend_yield(g, c));
    if g.shares_outstanding[c] == Ext::from_f64(0.0) || g.earnings_y1[c] == Ext::from_f64(0.0) {
        f.div_yield = crate::pb::numeric::double(Ext::from_f64(10.0));
        f.payout_pct = 100;
        return;
    }
    let total_dividends = to_long(Ext::from_f64(100.0) * g.dividend_per_share[c] * g.shares_outstanding[c]);
    if g.earnings_y1[c] > Ext::from_f64(0.0) {
        f.payout_pct = to_integer(Ext::from_f64(100.0) * (cast::<Ext, _>(total_dividends) / g.earnings_y1[c]));
    } else {
        f.payout_pct = 100;
    }
}

/// `(SilverPrice - MetalsPriceBase * 0.01) + UpdateMetalsPriceBase() / 100` as evaluated in
/// AiCorpGrowth_IndustryAdjust.
fn acg_silver_level(g: &mut Globals, ui: &mut dyn Ui) -> Ext {
    let a = cast::<Ext, _>(g.silver_price) - cast::<Ext, _>(g.metals_price_base) * sf(0.01f32);
    let b = update_metals_price_base(g, ui);
    a + b / Ext::from_f64(100.0)
}

/// GOSUB AiCorpGrowth_IndustryAdjust (0x6b4028): commodity-driven growth targets for oil (7, 8),
/// agriculture (15) and gold (50).
fn acg_industry_adjust(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut GrowthFrame) {
    let ind = f.industry;
    if ind == 7 || ind == 8 {
        if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(10.0) < cast::<Ext, _>(g.ind_growth_rate[cast::<i32, _>(ind)])
            && sf(1.13f32) > cast::<Ext, _>(g.oil_price)
            && sf(4.9f32) > cast::<Ext, _>(g.gdp_growth)
            && cast::<Ext, _>(rng.rnd_range(10, 0)) + Ext::from_f64(15.0) > cast::<Ext, _>(f.market_share_pct)
            && f.target_growth < 30
        {
            f.target_growth += 10;
        }
    }
    if ind == 15
        && f.score > 25
        && Ext::from_f64(1.0) < cast::<Ext, _>(g.oil_price)
        && sf(4.5f32) > cast::<Ext, _>(g.gdp_growth)
        && (cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price)) < Ext::from_f64(11.0)
        && cast::<Ext, _>(rng.rnd_range(10, 0)) + Ext::from_f64(15.0) > cast::<Ext, _>(f.market_share_pct)
        && f.target_growth < 30
    {
        f.target_growth += 10;
    }
    if ind == 50 {
        if f.score > 27 && Ext::from_f64(12.0) * rng.rnd() + Ext::from_f64(25.0) > cast::<Ext, _>(g.ind_growth_rate[cast::<i32, _>(ind)]) {
            let x = acg_silver_level(g, ui);
            if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(30.0) > x
                && cast::<Ext, _>(rng.rnd_range(10, 0)) + Ext::from_f64(15.0) > cast::<Ext, _>(f.market_share_pct)
                && f.target_growth < 30
            {
                f.target_growth += 10;
            }
            return;
        }
        if f.score < 18 && Ext::from_f64(8.0) * rng.rnd() < cast::<Ext, _>(g.ind_growth_rate[cast::<i32, _>(ind)]) {
            let x = acg_silver_level(g, ui);
            if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(35.0) < x
                && cast::<Ext, _>(rng.rnd_range(10, 0)) + Ext::from_f64(15.0) > cast::<Ext, _>(f.market_share_pct)
                && Ext::from_f64(10.0) * rng.rnd() < cast::<Ext, _>(f.target_growth)
            {
                f.target_growth -= 10;
            }
        }
    }
}

/// Port of AiCorpGrowthAndDividends (0x6b2a08) with its GOSUBs AiCorpGrowth_CalcGoodCreditPct
/// (0x6b3e47), AiCorpGrowth_CalcPayout (0x6b3f1d) and AiCorpGrowth_IndustryAdjust (0x6b4028).
/// Quarterly growth/dividend policy of an active computer-run corp 86..1600 (`corp_ref`): drops the
/// dividend of corps rated below 2; may start or raise a dividend of well-rated corps (low payout, many
/// good credits around); for operating corps 131..1600 below industry 70 sets `GrowthRate` two thirds of
/// the way to a target from margin, industry demand gap, market share, cash-flow outlook
/// (`ShowCashFlowProjection(corp, 1)`), industry growth and the monetary stance (with commodity rules
/// for oil, agriculture and gold), may raise `DiscSpendPct`, nudge `ProfitMargin`, name the
/// `AntitrustDefendant` (> 49 % share). Human-controlled corps not managed personally are only throttled
/// to `PlayerGrowthThrottle`. Also updates `NonFinCorpCount` / `GoodCreditCount` / `GoodCreditPct`
/// (always when `NonFinCorpCount = 0`, else 1 in 10), `TmpIndGapAvg` / `TmpIndGap2`,
/// `TmpIndBoostEligible`, `RandSeed` (+5), clears `TmpExt1..3`.
pub fn ai_corp_growth_and_dividends(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_ref: i32,
) {
    let mut f = GrowthFrame {
        corp_id: corp_ref,
        industry: 0,
        old_growth: 0,
        market_share_pct: 0.0f32,
        score: 0,
        target_growth: 0,
        cf_status: 0,
        payout_pct: 0,
        div_yield: crate::pb::numeric::double(Ext::from_f64(0.0)),
    };
    let c = f.corp_id;
    if g.entity_status[c] != 1 || c < 86 {
        return;
    }
    if 2 > g.credit_rating_num[c] {
        g.dividend_per_share[c] = Ext::from_f64(0.0);
    }
    if get_managing_player(g, c) > 0 {
        return;
    }
    f.industry = g.industry_num[c];
    f.old_growth = g.growth_rate[c];
    if g.non_fin_corp_count == 0 || Ext::from_f64(10.0) * rng.rnd() > Ext::from_f64(9.0) {
        acg_calc_good_credit_pct(g);
    }
    acg_calc_payout(g, &mut f);
    let dividend_block = c <= 130
        || f.industry >= 70
        || g.business_assets[c] >= g.exchange_rate * Ext::from_f64(500.0)
        || g.market_cap[c] >= g.exchange_rate * Ext::from_f64(50.0);
    if dividend_block
        && 6 < g.credit_rating_num[c]
        && f.industry < 70
        && Ext::from_f64(35.0) * rng.rnd() + Ext::from_f64(55.0) > cast::<Ext, _>(f.payout_pct)
        && get_controller_type(g, c) != 1
        && Ext::from_f64(30.0) * rng.rnd() + Ext::from_f64(30.0) < cast::<Ext, _>(g.good_credit_pct)
        && Ext::from_f64(9.0) > f.div_yield
        && Ext::from_f64(20.0) * rng.rnd() + Ext::from_f64(70.0) > cast::<Ext, _>(f.payout_pct)
        && g.monetary_stance >= 0
    {
        if Ext::from_f64(0.0) < f.div_yield && Ext::from_f64(1.0) > f.div_yield {
            g.dividend_per_share[c] = Ext::from_f64(4.0) * g.dividend_per_share[c];
        } else if Ext::from_f64(0.0) == f.div_yield {
            g.dividend_per_share[c] = g.market_cap[c] * sf(0.005f32);
        }
        let x = sf(0.1f32) * rng.rnd() + sf(1.1f32);
        g.dividend_per_share[c] = x * g.dividend_per_share[c];
    }
    if c < 131 || f.industry > 69 {
        return;
    }
    if get_controller_type(g, c) == 1
        && get_managing_player(g, c) == 0
        && 6 > g.corp_controller[c]
        && g.growth_rate[c] >= g.player_growth_throttle[g.corp_controller[c]]
    {
        if g.growth_rate[c] > g.player_growth_throttle[g.corp_controller[c]] {
            g.growth_rate[c] -= 1;
        }
        return;
    }
    calc_industry_outlook(g, rng, f.industry);
    g.tmp_ind_gap_avg = to_long(g.tmp_ext1);
    g.tmp_ind_gap2 = to_long(g.tmp_ext2);
    g.tmp_ind_gap_avg = to_long((cast::<Ext, _>(g.tmp_ind_gap2) + cast::<Ext, _>(g.tmp_ind_gap_avg)) / Ext::from_f64(2.0));
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    if g.ind_total_assets > Ext::from_f64(0.0) {
        f.market_share_pct = cast::<f32, _>(Ext::from_f64(100.0) * (g.business_assets[c] / g.ind_total_assets));
    }
    let ms = cast::<Ext, _>(f.market_share_pct);
    let mut share_adj: Ext = ms * sf(0.2f32);
    if cast::<Ext, _>(g.rand_seed % 10) + Ext::from_f64(45.0) < ms {
        if g.antitrust_defendant == 0 && Ext::from_f64(49.0) < ms {
            g.antitrust_defendant = c;
            if 5 < g.growth_rate[c] {
                let r = rng.rnd_range(4, 0) + 3;
                g.growth_rate[c] = cast::<i16, _>(cast::<i32, _>(g.growth_rate[c]) - r);
            }
        }
    } else if 0 < g.non_fin_corp_count
        && 5 < g.credit_rating_num[c]
        && (cast::<Ext, _>(rng.rnd_range(25, 5))) > ms
        && g.monetary_stance >= 0
        && Ext::from_f64(30.0) * rng.rnd() + Ext::from_f64(45.0) < cast::<Ext, _>(g.good_credit_pct)
        && 15 > f.old_growth
    {
        let x = Ext::from_f64(5.0) + cast::<Ext, _>(f.old_growth) + Ext::from_f64(5.0) * rng.rnd();
        g.growth_rate[c] = cast::<i16, _>(to_quad(x));
        if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
            return;
        }
    }
    share_adj = share_adj - Ext::from_f64(2.0);
    if Ext::from_f64(8.0) < share_adj {
        share_adj = Ext::from_f64(5.0);
    }
    g.rand_seed += 5;
    let margin: i16 =
        to_integer(cast::<Ext, _>(g.profit_margin[c]) + share_adj - cast::<Ext, _>(g.disc_spend_pct[c]));
    f.score = cast::<i32, _>(margin) + g.tmp_ind_gap_avg;
    f.cf_status = to_long(show_cash_flow_projection(g, ui, rng, c, 1));
    g.tmp_ext1 = Ext::from_f64(0.0);
    if f.cf_status < 3 && !(Ext::from_f64(4.0) * rng.rnd() >= Ext::from_f64(1.0)) && f.score < 11 && 4 < g.credit_rating_num[c] {
        let r = rng.rnd_range(4, 0) + 3;
        if (cast::<i32, _>(g.disc_spend_pct[c])) < r {
            let lim = 1 - rng.rnd_range(6, 0);
            if get_mgmt_margin_vs_industry(g, c, 0) < lim {
                let r = rng.rnd_range(16, 4);
                g.disc_spend_pct[c] = cast::<i16, _>(r + cast::<i32, _>(g.disc_spend_pct[c]));
            }
        }
    }
    let ind = cast::<i32, _>(f.industry);
    if g.monetary_stance == 0 {
        if 3 <= g.econ_cycle_counter || Ext::from_f64(3.0) * rng.rnd() <= Ext::from_f64(1.0) {
            if f.score > 20 {
                let d = f.score - 20;
                let sel = ms;
                if sel > cast::<Ext, _>(rng.rnd_range(7, 0)) + Ext::from_f64(48.0) {
                    f.target_growth = rng.rnd_range(12, 0) + -6;
                } else if sel > Ext::from_f64(40.0) {
                    f.target_growth = rng.rnd_range(12, 0) + -3;
                } else if sel > Ext::from_f64(20.0) {
                    let r = rng.rnd_range(7, 0);
                    f.target_growth = (cast::<i32, _>(g.ind_long_term_growth[ind]) + 4) - r;
                } else if sel > Ext::from_f64(5.0) {
                    let x = sf(1.5f32) * cast::<Ext, _>(d) + cast::<Ext, _>(g.ind_long_term_growth[ind]);
                    f.target_growth = to_long(x + cast::<Ext, _>(rng.rnd_range(5, 0)));
                } else {
                    let base = 2 * d + cast::<i32, _>(g.ind_growth_rate[ind]);
                    f.target_growth = rng.rnd_range(15, 5) + base;
                }
            } else if f.score > 13 {
                let d = f.score - 12;
                if cast::<Ext, _>(rng.rnd_range(10, 1)) + Ext::from_f64(45.0) < ms {
                    f.target_growth = rng.rnd_range(d, 0) + -10;
                } else if Ext::from_f64(8.0) < ms {
                    let r = rng.rnd_range(8, 0);
                    f.target_growth = (cast::<i32, _>(g.ind_long_term_growth[ind]) + 4) - r;
                } else {
                    let r = rng.rnd_range(6, 0);
                    f.target_growth = r + cast::<i32, _>(g.ind_growth_rate[ind]) + d;
                }
            } else if f.score > 7 {
                if cast::<Ext, _>(rng.rnd_range(6, 0)) + Ext::from_f64(4.0) < ms {
                    f.target_growth = rng.rnd_range(10, 0) + -10;
                } else {
                    f.target_growth = rng.rnd_range(5, 0) + cast::<i32, _>(g.ind_growth_rate[ind]);
                }
            } else if Ext::from_f64(5.0) < ms {
                f.target_growth = rng.rnd_range(5, 0) + -10;
            } else {
                let r = rng.rnd_range(10, 1);
                f.target_growth = (cast::<i32, _>(g.ind_growth_rate[ind]) + 3) - r;
            }
            g.tmp_ind_boost_eligible = 0;
            if (c + g.cur_year) % 2 == 1 && g.cur_quarter > 2 {
                g.tmp_ind_boost_eligible = 1;
            }
            if (c + g.cur_year) % 2 == 0 && g.cur_quarter < 3 {
                g.tmp_ind_boost_eligible = 1;
            }
            if g.tmp_ind_boost_eligible == 1
                && Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(23.0) < cast::<Ext, _>(f.score)
                && Ext::from_f64(15.0) * rng.rnd() + Ext::from_f64(4.0) > ms
            {
                acg_industry_adjust(g, ui, rng, &mut f);
            }
            if matches!(
                f.industry,
                11 | 12
                    | 14
                    | 17
                    | 18
                    | 19
                    | 23
                    | 25
                    | 27
                    | 28
                    | 29
                    | 31
                    | 32
                    | 34
                    | 43
                    | 46
                    | 48
                    | 55
                    | 56
                    | 62
                    | 64
                    | 65
                    | 69
            ) && f.score < 26
                && Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(5.0) < cast::<Ext, _>(f.target_growth)
            {
                f.target_growth = to_long(cast::<Ext, _>(f.target_growth) - Ext::from_f64(7.0) * rng.rnd());
            }
            if f.cf_status > 6 || f.score < 3 {
                f.target_growth = -10;
            }
        }
    } else if g.monetary_stance == -1 {
        f.target_growth = 5;
        if Ext::from_f64(5.0) < ms && 2 < g.growth_rate[c] {
            f.target_growth -= rng.rnd_range(10, 3);
        } else if f.score < 30 {
            if 2 < g.growth_rate[c] {
                f.target_growth -= rng.rnd_range(10, 3);
            }
        } else if cast::<Ext, _>(rng.rnd_range(20, 10)) > ms {
            f.target_growth += rng.rnd_range(6, 2);
        }
        if f.cf_status > 6 || rng.rnd_range(5, 0) + 5 > f.score {
            f.target_growth = -15;
        }
        if rng.rnd_range(5, 0) + 5 > f.score {
            g.profit_margin[c] += 1;
        }
    } else {
        f.target_growth = 10;
        if Ext::from_f64(50.0) < ms && Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
            g.growth_rate[c] -= 1;
        } else if (cast::<Ext, _>(rng.rnd_range(50, 40))) < ms && Ext::from_f64(55.0) > ms {
            if 4 < g.credit_rating_num[c] && f.score > 15 {
                let base = 3 + f.target_growth;
                f.target_growth = rng.rnd_range(3, 0) + base;
            }
        } else if (cast::<Ext, _>(rng.rnd_range(40, 5))) < ms && Ext::from_f64(41.0) > ms {
            if f.score > 12 && 5 < g.credit_rating_num[c] {
                f.target_growth += rng.rnd_range(10, 3);
            }
        } else if !(Ext::from_f64(5.0) < ms) && 5 < g.credit_rating_num[c] {
            if 20 > g.growth_rate[c] && f.score > 10 {
                f.target_growth += 5;
            }
            if 45 > g.growth_rate[c] && Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(15.0) < cast::<Ext, _>(f.score) {
                f.target_growth += rng.rnd_range(15, 8);
            }
        }
        if f.cf_status == 8 && f.score < 0 {
            f.target_growth = -7;
        }
        if rng.rnd_range(5, 0) + 10 < f.score {
            g.profit_margin[c] -= 1;
        }
    }
    if f.industry == 69 && rng.rnd_range(10, 0) + 12 > f.score {
        f.target_growth = 2 - rng.rnd_range(12, 4);
    }
    let sum = f.target_growth * 2 + cast::<i32, _>(f.old_growth);
    g.growth_rate[c] = cast::<i16, _>(to_quad(cast::<Ext, _>(sum) / Ext::from_f64(3.0)));
    if -10 > g.growth_rate[c] {
        g.growth_rate[c] = -10;
    }
    if 60 < g.growth_rate[c] {
        g.growth_rate[c] = 60;
    }
}

// ---------------------------------------------------------------------------------------------------------
// CalcConsolidatedTaxPayment
// ---------------------------------------------------------------------------------------------------------

/// Port of CalcConsolidatedTaxPayment (0x4fa767).
/// Income tax of corp `corp_id` on the taxable income passed in `TmpExt1` (cleared, like `TmpExt2/3`),
/// at `CorpTaxRate` (as SINGLE `rate * 0.01`). Runs `UpdateCorpControl(corp_id)` first, which sets
/// `Parent80Owner` (an owner of 80 %+ for tax consolidation). Funds (industry 71) pay nothing.
///
/// Profits first use up a loss carryforward (`TaxLossCarry` < 0; savings returned in `TmpExt1`); a
/// consolidated parent (> 10) gets a loss carryback refund of a positive `TaxLossCarry` paid in cash.
/// With a parent the remaining income moves to the parent's `ConsolTaxIncYTD` and the tax is settled
/// in cash with the parent (a loss benefit capped by 50 x the parent's market cap / its own
/// `TaxLossCarry`; the uncovered part — returned in `TmpExt3` if > 10 — is charged back to
/// `EquityIncomeYTD` / `ExtraItemsYTD`), and the parent's stock basis in the corp is adjusted by the
/// after-tax amount. Otherwise the tax due (net of the carryforward) is paid in cash (`TmpExt2`) and the
/// rest of the income/loss goes to `TaxLossCarry` (positive carryforwards are cleared in Q4).
pub fn calc_consolidated_tax_payment(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let c = corp_id;
    let mut taxable_income: Ext = g.tmp_ext1;
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    let tax_rate: f32 = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.corp_tax_rate) * sf(0.01f32)));
    let rate = cast::<Ext, _>(tax_rate);
    update_corp_control(g, ui, rng, c);
    if g.industry_num[c] == 71 {
        return;
    }
    let mut carry_savings: Ext = Ext::from_f64(0.0);
    let mut excess_benefit: Ext = Ext::from_f64(0.0);
    let mut basis_adj: Ext = Ext::from_f64(0.0);
    if taxable_income > Ext::from_f64(0.0) {
        if g.tax_loss_carry[c] < Ext::from_f64(0.0) {
            if taxable_income < g.tax_loss_carry[c].abs() {
                g.tax_loss_carry[c] = taxable_income + g.tax_loss_carry[c];
                carry_savings = rate * taxable_income;
                taxable_income = Ext::from_f64(0.0);
            } else {
                taxable_income = g.tax_loss_carry[c] + taxable_income;
                carry_savings = (rate * g.tax_loss_carry[c]) * -Ext::from_f64(1.0);
                g.tax_loss_carry[c] = Ext::from_f64(0.0);
            }
        }
    } else if taxable_income < Ext::from_f64(0.0) && g.tax_loss_carry[c] > Ext::from_f64(0.0) && g.parent80_owner > 10 {
        let loss_refund: Ext;
        if taxable_income.abs() <= g.tax_loss_carry[c] {
            loss_refund = (taxable_income * rate) * -Ext::from_f64(1.0);
            g.tax_loss_carry[c] = taxable_income + g.tax_loss_carry[c];
            taxable_income = Ext::from_f64(0.0);
        } else {
            loss_refund = g.tax_loss_carry[c] * rate;
            taxable_income = g.tax_loss_carry[c] + taxable_income;
            g.tax_loss_carry[c] = Ext::from_f64(0.0);
        }
        g.cash[c] = loss_refund + g.cash[c];
    }
    let p = cast::<i32, _>(g.parent80_owner);
    if taxable_income > Ext::from_f64(0.0) && p > 10 {
        let parent_tax_xfer = rate * taxable_income;
        g.consol_tax_inc_ytd[p] = taxable_income + g.consol_tax_inc_ytd[p];
        g.cash[p] = parent_tax_xfer + g.cash[p];
        g.cash[c] = g.cash[c] - parent_tax_xfer;
        basis_adj = taxable_income - parent_tax_xfer;
        taxable_income = Ext::from_f64(0.0);
    }
    if taxable_income < Ext::from_f64(0.0) && p > 10 {
        let mut parent_tax_xfer = (taxable_income * rate) * -Ext::from_f64(1.0);
        g.consol_tax_inc_ytd[p] = taxable_income + g.consol_tax_inc_ytd[p];
        if parent_tax_xfer > Ext::from_f64(50.0) * g.market_cap[p] && parent_tax_xfer > g.tax_loss_carry[p] {
            parent_tax_xfer = Ext::from_f64(50.0) * g.market_cap[p];
            if g.tax_loss_carry[p] > parent_tax_xfer {
                parent_tax_xfer = g.tax_loss_carry[p];
            }
            excess_benefit = (taxable_income * rate) * -Ext::from_f64(1.0) - parent_tax_xfer;
            let epl = cast::<Ext, _>(g.extraordinary_pl[c]);
            if !(epl < Ext::from_f64(0.0)) {
                g.equity_income_ytd[c] = g.equity_income_ytd[c] - excess_benefit;
            } else if !(taxable_income - epl < Ext::from_f64(0.0)) {
                g.extra_items_ytd[c] = g.extra_items_ytd[c] - excess_benefit;
            } else if epl < Ext::from_f64(0.0) && taxable_income < Ext::from_f64(0.0) {
                let equity_share: Ext = (taxable_income - epl) / taxable_income;
                let extra_share: Ext = epl / taxable_income;
                g.equity_income_ytd[c] = g.equity_income_ytd[c] - equity_share * excess_benefit;
                g.extra_items_ytd[c] = g.extra_items_ytd[c] - extra_share * excess_benefit;
            }
        }
        g.cash[p] = g.cash[p] - parent_tax_xfer;
        g.cash[c] = parent_tax_xfer + g.cash[c];
        basis_adj = parent_tax_xfer + basis_adj;
        basis_adj = taxable_income + basis_adj;
        taxable_income = Ext::from_f64(0.0);
    }
    let loss_carry: Ext = g.tax_loss_carry[c];
    let mut tax_due: Ext = rate * taxable_income;
    if !(taxable_income > Ext::from_f64(0.0)) && !(loss_carry > Ext::from_f64(0.0)) {
        tax_due = Ext::from_f64(0.0);
    } else if !(taxable_income > Ext::from_f64(0.0)) && loss_carry > Ext::from_f64(0.0) {
        let carry_tax_value = loss_carry * rate;
        let net_tax = carry_tax_value + tax_due;
        if net_tax < Ext::from_f64(0.0) {
            tax_due = -Ext::from_f64(1.0) * carry_tax_value;
        }
    } else if taxable_income > Ext::from_f64(0.0) && loss_carry < Ext::from_f64(0.0) {
        let carry_tax_value = loss_carry * rate;
        let net_tax = carry_tax_value + tax_due;
        tax_due = if net_tax < Ext::from_f64(0.0) { Ext::from_f64(0.0) } else { net_tax };
    }
    g.tax_loss_carry[c] = taxable_income + loss_carry;
    if g.cur_quarter == 4 && g.tax_loss_carry[c] > Ext::from_f64(0.0) {
        g.tax_loss_carry[c] = Ext::from_f64(0.0);
    }
    g.cash[c] = g.cash[c] - tax_due;
    g.tmp_ext1 = carry_savings;
    g.tmp_ext2 = tax_due;
    if excess_benefit > Ext::from_f64(10.0) {
        g.tmp_ext3 = excess_benefit;
    }
    let parent_id = cast::<i32, _>(g.parent80_owner);
    if parent_id > 10 && basis_adj != Ext::from_f64(0.0) {
        let slot = find_holding_slot(g, parent_id, c);
        g.hold_stock_basis[(slot, parent_id)] = basis_adj + g.hold_stock_basis[(slot, parent_id)];
    }
}

// ---------------------------------------------------------------------------------------------------------
// AdjustPriceForDistribution / ChargeShortsForDistribution
// ---------------------------------------------------------------------------------------------------------

/// Port of AdjustPriceForDistribution (0x619ed2).
/// Marks the stock of `corp_id` down by a cash distribution of `amount_cents` (in hundredths of the
/// market-cap unit, i.e. `MarketCap -= amount_cents / 100`, floor 0.25), shifts its 60-month price
/// history the same way (non-empty months; floors 0.25 / 0.26 / 0.24, high >= 1.01 x close, low <=
/// 0.99 x close) and records the history tick (`UpdatePriceHistoryTick`).
pub fn adjust_price_for_distribution(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
    amount_cents: Ext,
) {
    let corp = corp_id;
    let d: Ext = amount_cents / Ext::from_f64(100.0);
    g.market_cap[corp] = g.market_cap[corp] - d;
    if g.market_cap[corp] < sf(0.25f32) {
        g.market_cap[corp] = sf(0.25f32);
    }
    for month in 1..=60 {
        if g.cap_hist_close[(month, corp)] == Ext::from_f64(0.0) {
            continue;
        }
        g.cap_hist_close[(month, corp)] = g.cap_hist_close[(month, corp)] - d;
        g.price_hist_high[(month, corp)] = g.price_hist_high[(month, corp)] - d;
        g.price_hist_low[(month, corp)] = g.price_hist_low[(month, corp)] - d;
        if g.cap_hist_close[(month, corp)] < sf(0.25f32) {
            g.cap_hist_close[(month, corp)] = sf(0.25f32);
        }
        if g.price_hist_high[(month, corp)] < sf(0.26f32) {
            g.price_hist_high[(month, corp)] = sf(0.26f32);
        }
        if g.price_hist_low[(month, corp)] < sf(0.24f32) {
            g.price_hist_low[(month, corp)] = sf(0.24f32);
        }
        if g.price_hist_high[(month, corp)] < g.cap_hist_close[(month, corp)] {
            g.price_hist_high[(month, corp)] = sf(1.01f32) * g.cap_hist_close[(month, corp)];
        }
        if g.price_hist_low[(month, corp)] > g.cap_hist_close[(month, corp)] {
            g.price_hist_low[(month, corp)] = sf(0.99f32) * g.cap_hist_close[(month, corp)];
        }
    }
    update_price_history_tick(g, ui, rng, corp);
}

/// Port of ChargeShortsForDistribution (0x5e2f14).
/// Players short `corp_id` pay the distribution `amount` (BYVAL EXT, same unit as `MarketCap`) on their
/// short position: for every active player with a negative `HoldStockPct` in the corp,
/// `amount * pct * 0.01` (negative) is added to `PlayerShortMarginBal` and `PlayerCapGains`. Nothing
/// happens when nobody is short the corp (`FindShortPositions(corp, 0) = -2`).
pub fn charge_shorts_for_distribution(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, amount: Ext, corp_id: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if find_short_positions(g, ui, rng, corp_id, 0) == -2 {
        return;
    }
    let n = g.num_players;
    for player_id in 1..=n {
        if g.entity_status[player_id] != 1 {
            continue;
        }
        if find_short_positions(g, ui, rng, corp_id, player_id) == -4 {
            continue;
        }
        for slot in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(slot, player_id)]) == corp_id
                && g.hold_stock_pct[(slot, player_id)] < 0
            {
                let pct = cast::<i32, _>(g.hold_stock_pct[(slot, player_id)]);
                let x: Ext = amount * (cast::<Ext, _>(pct) * sf(0.01f32));
                g.player_short_margin_bal[player_id] = x + g.player_short_margin_bal[player_id];
                g.player_cap_gains[player_id] = x + g.player_cap_gains[player_id];
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// EtfAddToHoldings
// ---------------------------------------------------------------------------------------------------------

/// Port of EtfAddToHoldings (0x64f921).
/// An index fund `etf_id` (1586..; active, more than 10 holdings, not run by a human adviser off
/// autopilot) moves its T-bills to cash and invests spare cash (cash - 1.1 x `reserve_cash` - positive
/// `ExtraordinaryPL`, while above half the starting cash) in its small holdings (< 10 %, market cap up
/// to 35 % of its cash, except the largest holding) via `BuyStock`, posts a news item about it and
/// rebalances cash/T-bills. Returns 1 if it bought anything, else 0. `time_secs`: seconds of `TIME$`
/// (for `BuyStock`).
pub fn etf_add_to_holdings(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    etf_id: i32,
    reserve_cash: Ext,
    time_secs: i32,
) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if 1586 > etf_id {
        return 0;
    }
    let fund_id = etf_id;
    if 11 > g.hold_stock_id[(0, fund_id)] || g.entity_status[fund_id] != 1 {
        return 0;
    }
    if get_etf_adviser_human_status(g, ui, rng, fund_id) > 0
        && g.auto_pilot[g.fund_adviser[fund_id]] == 0
    {
        return 0;
    }
    let largest_id: i16 = cast::<i16, _>(get_largest_holding_id(g, fund_id));
    g.cash[fund_id] = g.t_bills[fund_id] + g.cash[fund_id];
    g.t_bills[fund_id] = Ext::from_f64(0.0);
    let start_cash: Ext = g.cash[fund_id];
    let mut bought_any = 0;
    let mut buy_count: i32 = 0;
    let mut last_bought_id: i32 = 0;
    for slot in 1..=15 {
        let mut spare_cash: Ext = g.cash[fund_id] - reserve_cash * sf(1.1f32);
        if g.extraordinary_pl[fund_id] > 0 {
            spare_cash = spare_cash - cast::<Ext, _>(g.extraordinary_pl[fund_id]);
        }
        if spare_cash < start_cash * sf(0.5f32) {
            break;
        }
        let max_cost: Ext = g.cash[fund_id] * sf(0.35f32);
        let stock_id = cast::<i32, _>(g.hold_stock_id[(slot, fund_id)]);
        if stock_id < 11 || cast::<i32, _>(largest_id) == stock_id || g.hold_stock_pct[(slot, fund_id)] > 9 {
            continue;
        }
        if g.market_cap[stock_id] <= max_cost {
            last_bought_id = stock_id;
            buy_count += 1;
            g.tmp_ext1 = Ext::from_f64(1.0);
            g.tmp_ext2 = Ext::from_f64(2.0);
            crate::logic::trading::buy_stock(g, ui, rng, fund_id, stock_id, time_secs);
            bought_any = 1;
        }
    }
    let amount_spent: i64 = to_quad(start_cash - g.cash[fund_id]);
    if last_bought_id > 10 && cast::<i32, _>(largest_id) != last_bought_id && buy_count > 0 {
        let mut news_tail =
            PbStr::from(__content.string(10789)) + &number_to_words(buy_count, 0, 0) + " stocks";
        if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
            news_tail = PbStr::from(__content.string(10790))
                + &trim(g.currency_symbol.as_bytes())
                + &trim(&format(cast::<Ext, _>(amount_spent), b"* ###,"))
                + " "
                + &g.million_prefix_uc
                + __content.string(10791);
        }
        g.news_msg = PbStr::from(__content.string(4290)) + &g.ent_symbol[fund_id] + &news_tail;
        post_news(g, ui, fund_id, 0);
    }
    balance_cash_t_bills(g, fund_id);
    bought_any
}

// ---------------------------------------------------------------------------------------------------------
// AdjustOptionsForCorpAction
// ---------------------------------------------------------------------------------------------------------

/// Port of AdjustOptionsForCorpAction (0x5f7680).
/// Rewrites every option record (`OptionRecs(1..1000)`) on `corp_id` after a corporate action:
/// `adj_amount > 0` with `adj_pct > 0` scales the contract quantity by `adj_pct %` (at least 1 / -1)
/// and the strike by `adj_amount %`; `adj_amount < 0` (a distribution per share, negative) lowers the
/// strike by it (floor: one share's worth, `SharesOutstanding`). Builds the news ("option contracts
/// [runtime resource wording] issuance/buyback" or " [runtime resource wording] … distribution",
/// `dist_type` 1 capital gain, 2 return of capital, 3 + `adj_pct > 10` stock of corp `adj_pct`) and
/// posts it at the end (a pending `NewsMsg` is posted first). All parameters BYREF, none written.
///
/// Quirks: with `adj_amount > 0` but `adj_pct <= 0`, or `adj_amount = 0`, the quantity becomes 0; the
/// "quantity changed" flag is never reset between records. `SharesOutstanding = 0` raises ERROR 151 on
/// the first matching record: the ON ERROR handler shows the error and leaves the routine (the port
/// returns there without posting the news).
pub fn adjust_options_for_corp_action(
    g: &mut Globals,
    ui: &mut dyn Ui,
    corp_id: i32,
    adj_amount: Ext,
    adj_pct: i32,
    dist_type: i32,
) {
    let __content = crate::content::current();
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    let mut qty_changed: i32 = 0;
    for rec_idx in 1..=1000 {
        let mut new_qty: i32 = 0;
        if val(parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 1).as_bytes()) != cast::<Ext, _>(corp_id) {
            continue;
        }
        let qty: i32 = to_long(val(
            parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 4).as_bytes()
        ));
        if adj_amount > Ext::from_f64(0.0) && 0 < adj_pct {
            new_qty = to_long((cast::<Ext, _>(adj_pct) * sf(0.01f32)) * cast::<Ext, _>(qty));
            if (cast::<Ext, _>(new_qty)).abs() < Ext::from_f64(1.0) {
                new_qty = 1;
                if qty < 0 {
                    new_qty = -1;
                }
            }
            if new_qty != qty {
                qty_changed = 1;
            }
        } else if adj_amount < Ext::from_f64(0.0) {
            new_qty = qty;
        }
        // strike / newStrike are EXT: computed in 80-bit so that STR$ shows the original's digits
        let strike: Ext80 = Ext80::abs(val_ext(
            parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 5).as_bytes(),
        ));
        if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
            // ERROR 151 -> ON ERROR handler: ShowRuntimeError and exit
            return;
        }
        let mut new_strike: Ext80;
        if adj_amount > Ext::from_f64(0.0) {
            let pct = Ext80::from_ext(adj_amount).fmul(Ext80::from_ext(sf(0.01f32)));
            new_strike = Ext80::abs(strike.fmul(pct));
        } else {
            new_strike = Ext80::abs(Ext80::fadd(Ext80::from_ext(adj_amount), strike));
            if new_strike.into_ext() / g.shares_outstanding[corp_id] < Ext::from_f64(1.0) {
                new_strike = Ext80::from_ext((g.shares_outstanding[corp_id] * Ext::from_f64(1.0)).abs());
            }
        }
        let mut new_rec = PbStr::default();
        for field in 1..=8 {
            if field < 4 || field > 5 {
                new_rec = new_rec + &parse_any(g.option_recs[rec_idx].as_bytes(), b"|", field);
            } else if field == 4 {
                new_rec = new_rec + str_num(cast::<Ext, _>(new_qty), 15).as_str();
            } else if field == 5 {
                new_rec = new_rec + str_num_ext(Ext80::abs(new_strike), 15).as_str();
            }
            if field < 8 {
                new_rec = new_rec + "|";
            }
            if field == 8 {
                g.option_recs[rec_idx] = new_rec.clone();
                if adj_amount > Ext::from_f64(0.0) && qty_changed > 0 {
                    let reason = if 100 < adj_pct {
                        __content.string(10792)
                    } else {
                        __content.string(10793)
                    };
                    g.news_msg = g.ent_symbol[corp_id].clone()
                        + __content.string(10794)
                        + reason;
                } else if !(adj_amount > Ext::from_f64(0.0)) {
                    let mut reason = if dist_type == 3 && 10 < adj_pct {
                        g.ent_symbol[adj_pct].clone() + " stock "
                    } else {
                        PbStr::from(__content.string(10795))
                    };
                    if dist_type == 1 {
                        reason = PbStr::from(__content.string(10796));
                    }
                    if dist_type == 2 {
                        reason = PbStr::from(__content.string(10797));
                    }
                    g.news_msg = g.ent_symbol[corp_id].clone()
                        + __content.string(10798)
                        + &reason
                        + __content.string(10799);
                }
            }
        }
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, corp_id, 0);
    }
}

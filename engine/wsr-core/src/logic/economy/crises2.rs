//! The subprime-mortgage crisis `SubprimeCrisisUpdate` (0x53b4c7) and the pandemic `PandemicUpdate`
//! (0x53ca7b) with its GOSUBs `Pandemic_ScaleMarketCaps` (0x53dc8a) and `Pandemic_AdjustIndDemand`
//! (0x53dddb). `Pandemic_Stub` (0x53e1b4) is the routine's exit label (`PB_FrameRelease` / `RET`), not
//! code of its own.
//!
//! `DATE$` / `TIME$` are parameters: `date` is `DATE$` ("mm-dd-yyyy"), `time_secs` is
//! `VAL(RIGHT$(TIME$, 2))` (as for [`crate::logic::valuation::calc_stock_price`]).

// Statements keep the original BASIC shape on purpose.
#![allow(clippy::collapsible_match, clippy::needless_late_init)]
use crate::pb::real::{Ext, cast};


use super::*;
use crate::logic::futures::update_metals_price_base;
use crate::logic::report::number_to_words;
use crate::pb::string::{mcase, right, ucase, CaseMode};

/// `x87 fistp qword` of the 80-bit product `(mant * 2^exp) * q` (`fild qword q; fmul`): the exact
/// product is rounded to a 64-bit significand, then to an integer (both half-even).
fn ext_mul_quad_round(mant: i128, exp: i32, q: i64) -> i64 {
    let p = mant * q as i128;
    if p == 0 {
        return 0;
    }
    let neg = p < 0;
    let mut a = p.unsigned_abs();
    let mut e = exp;
    // round to 64 significant bits
    let bits = 128 - cast::<i32, _>(a.leading_zeros());
    if bits > 64 {
        let sh = cast::<u32, _>(bits - 64);
        a = round_shift(a, sh);
        e += cast::<i32, _>(sh);
    }
    // to an integer
    let v = if e >= 0 { a << e } else { round_shift(a, cast::<u32, _>(-e)) };
    let v = v as i128;
    cast::<i64, _>(if neg { -v } else { v })
}

/// `a / 2^sh` rounded half-even.
fn round_shift(a: u128, sh: u32) -> u128 {
    if sh == 0 {
        return a;
    }
    if sh >= 128 {
        return 0;
    }
    let q = a >> sh;
    let rem = a & ((1u128 << sh) - 1);
    let half = 1u128 << (sh - 1);
    if rem > half || (rem == half && q & 1 == 1) { q + 1 } else { q }
}

/// Exact `(mantissa, exponent)` of a finite f64.
fn f64_parts(x: Ext) -> (i128, i32) {
    if x == Ext::from_f64(0.0) {
        return (0, 0);
    }
    let b = x.to_bits();
    let sign = if b >> 63 == 1 { -1 } else { 1 };
    let ex = cast::<i32, _>((b >> 52) & 0x7ff);
    let frac = (b & ((1u64 << 52) - 1)) as i128;
    let (m, e) = if ex == 0 { (frac, -1074) } else { (frac | (1i128 << 52), ex - 1075) };
    (sign * m, e)
}

/// Exact sum of two dyadic rationals `(m, e)`.
fn add_parts(a: (i128, i32), b: (i128, i32)) -> (i128, i32) {
    if a.0 == 0 {
        return b;
    }
    if b.0 == 0 {
        return a;
    }
    let e = a.1.min(b.1);
    (a.0 * (1i128 << (a.1 - e)) + b.0 * (1i128 << (b.1 - e)), e)
}

/// `(m, e)` rounded to the nearest f64 (`fstp qword`; |m| < 2^127).
fn parts_to_f64(p: (i128, i32)) -> Ext {
    let (m, e) = p;
    if m == 0 {
        return Ext::from_f64(0.0);
    }
    let neg = m < 0;
    let mut a = m.unsigned_abs();
    let mut e = e;
    let bits = 128 - cast::<i32, _>(a.leading_zeros());
    if bits > 53 {
        let sh = cast::<u32, _>(bits - 53);
        a = round_shift(a, sh);
        e += cast::<i32, _>(sh);
    }
    let v = cast::<Ext, _>(a) * 2f64.powi(e);
    if neg { -v } else { v }
}

/// GOSUB `PandemicUpdate`'s `ShowQuickMessage` with a temporary (NewsMsg is not encoded).
fn quick_message_copy(g: &mut Globals, ui: &mut dyn Ui, text: PbStr) {
    let mut t = text;
    show_quick_message(g, ui, &mut t);
}

/// `ShowQuickMessage(NewsMsg)` (BYREF: the CR/LF encoding stays in `NewsMsg`).
fn quick_message_news(g: &mut Globals, ui: &mut dyn Ui) {
    let mut m = std::mem::take(&mut g.news_msg);
    show_quick_message(g, ui, &mut m);
    g.news_msg = m;
}

/// Port of SubprimeCrisisUpdate (0x53b4c7).
/// The subprime-mortgage crisis, run once per quarter (from SimScheduledEvents).
///
/// * `SubprimeCrisisState = -2` (armed) and no bubble (`BubbleState < 1`): with a tight monetary policy
///   (`MonetaryPolicy = 2`) after the first year, the crisis starts (state 5..10: "Foreclosures are
///   increasing …" news, GDP target -4..-6, rates +1 and target +2..+5, housing-industry (5) forecast -7
///   and growth -6); else, with rates below 12 and a neutral policy, 1 in 3 the central bank starts to
///   tighten (`MonetaryPolicy = 1`, target +3).
/// * State > 0: each quarter cuts `MarketValTarget` (x0.87, at least 0.75) and `MarketValuation` (x0.95),
///   sets GDP/interest targets from the state, counts the state down (twice, 1 in 4, above 5; 0 becomes
///   -1) and runs the stage script (`SELECT CASE` 9..1, -1): headlines, commodity shocks (CORN/GOLD/WHEAT,
///   ShockCommodityPrice), news stories (MSCNEWS.DAT 495, 470, 71), gold/silver/oil targets, monetary
///   policy, and in stage 5 a run on the banks (6% of CDs, 8% of demand deposits withdrawn from every
///   active bank). The headline (if any) pops up (except stage 5, upper-cased below 65 characters) and is
///   posted; 1 in 10 the crisis ends at once (else `MarketValuation - 0.1`). When it ends (state -1) the
///   cycle counter becomes 22, the valuation recovers (+0.2) and the GDP target is 2..7.
///
/// `date` is `DATE$` (its year + 1 fills `@TEXTSTRING` of stories 495 and 470). Draws `RND`. `SLEEP 2000`
/// after the start news is not ported; the ON ERROR handler only shows ShowRuntimeError.
pub fn subprime_crisis_update(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, date: &PbStr) {
    let __content = crate::content::current();
    if g.subprime_crisis_state == -1 {
        return;
    }
    if g.subprime_crisis_state <= 0 {
        if g.subprime_crisis_state != 0 && g.subprime_crisis_state != -2 {
            return;
        }
        if 0 < g.bubble_state {
            return;
        }
        if g.subprime_crisis_state == -2 && g.monetary_policy == 2 && g.start_year < g.cur_year {
            g.subprime_crisis_state = cast::<i16, _>(rng.rnd_range(8, 1) + 4);
            if 10 < g.subprime_crisis_state {
                g.subprime_crisis_state = 10;
            }
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(-Ext::from_f64(3.0) - cast::<Ext, _>(rng.rnd_range(3, 1))));
            if -Ext::from_f64(1.0) < cast::<Ext, _>(g.gdp_growth) {
                g.gdp_growth = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth) - Ext::from_f64(3.0) * rng.rnd()));
            }
            g.econ_cycle_counter = 5;
            g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.base_interest_rate)));
            g.interest_rate_target = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(2.0) + cast::<Ext, _>(g.base_interest_rate)) + Ext::from_f64(3.0) * rng.rnd()));
            g.news_msg = s(__content.string(4448));
            if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                g.news_msg = s(__content.string(4449));
            }
            g.news_msg = g.news_msg.clone() + __content.string(4450);
            g.ind_growth_forecast[5] = -7;
            if 0 < g.ind_growth_rate[5] {
                g.ind_growth_rate[5] = g.ind_growth_rate[5] - 6;
            }
            post_news(g, ui, 0, 0);
            // SLEEP 2000
        } else if Ext::from_f64(12.0) > cast::<Ext, _>(g.base_interest_rate) && g.subprime_crisis_state == -2 && g.monetary_policy == 0 {
            if Ext::from_f64(2.0) < Ext::from_f64(3.0) * rng.rnd() {
                g.monetary_policy = 1;
                g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(3.0) + cast::<Ext, _>(g.base_interest_rate)));
            }
        }
        return;
    }

    g.market_val_target = cast::<f64, _>(sf(0.87f32) * g.market_val_target);
    g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.95f32) * cast::<Ext, _>(g.market_valuation)));
    if sf(0.75f32) > g.market_val_target {
        g.market_val_target = cast::<f64, _>(sf(0.75f32));
    }
    if 1 < g.subprime_crisis_state {
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.subprime_crisis_state) - Ext::from_f64(17.0)));
    }
    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.subprime_crisis_state) + Ext::from_f64(2.0)));
    if 5 < g.subprime_crisis_state {
        if Ext::from_f64(3.0) < Ext::from_f64(4.0) * rng.rnd() {
            g.subprime_crisis_state -= 1;
        }
    }
    g.subprime_crisis_state -= 1;
    if g.subprime_crisis_state == 0 {
        g.subprime_crisis_state = -1;
    }
    let mut bank_short_name = g.central_bank_name.clone();
    if 0 < instr(1, bank_short_name.as_bytes(), __content.bytes(4373)) {
        bank_short_name = s("Fed");
    }
    let next_year_text = |date: &PbStr| -> PbStr {
        let story_text = right(date.as_bytes(), 4);
        let next_year = to_long(Ext::from_f64(1.0) + val(story_text.as_bytes()));
        str_trim(cast::<Ext, _>(next_year))
    };
    match g.subprime_crisis_state {
        9 => {
            if g.rand_seed % 2 == 1 {
                g.news_msg = s(__content.string(4451));
                if Ext::from_f64(2.0) < Ext::from_f64(3.0) * rng.rnd() {
                    g.news_msg = g.news_msg.clone() + __content.string(4452);
                }
            } else {
                g.news_msg = s(__content.string(4453));
                if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                    g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(2878), __content.bytes(4454));
                }
            }
            shock_commodity_price(g, rng, b"CORN", -24);
            shock_commodity_price(g, rng, b"GOLD", 22);
            if -Ext::from_f64(2.0) < cast::<Ext, _>(g.gdp_growth) && sf(2.2f32) > cast::<Ext, _>(g.gdp_growth) && Ext::from_f64(5.0) > cast::<Ext, _>(g.wheat_price) {
                let story_text = next_year_text(date);
                show_misc_news_story(g, ui, rng, 495, story_text.as_bytes());
            }
        }
        8 => {
            g.news_msg = s(__content.string(4455));
            if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                g.news_msg = s(__content.string(4456));
            }
            g.interest_rate_target = cast::<f32, _>(Ext::from_f32(15.0f32));
            g.monetary_policy = 2;
            shock_commodity_price(g, rng, b"WHEAT", 20);
            let story_text = next_year_text(date);
            show_misc_news_story(g, ui, rng, 470, story_text.as_bytes());
        }
        7 => {
            if g.rand_seed % 2 == 1 {
                g.news_msg = s(__content.string(4457));
            } else {
                g.news_msg = s(__content.string(4458));
            }
            g.gold_price_max = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(5200.0)));
            if g.rand_seed % 2 == 1 {
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1600.0) * rng.rnd() + Ext::from_f64(3400.0) + cast::<Ext, _>(g.metals_price_base)));
            } else {
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(630.0)));
            }
            if g.rand_seed % 3 == 1 && sf(1.47f32) < cast::<Ext, _>(g.oil_price) {
                g.oil_target_price = cast::<f32, _>(Ext::from_f32(0.6f32));
                g.oil_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_price) * sf(0.9f32)));
            }
        }
        6 => {
            g.news_msg = bank_short_name.clone() + __content.string(4459);
            if bank_short_name.len() < 16 {
                if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                    g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(4460), __content.bytes(4461));
                }
            }
            if g.rand_seed % 2 == 1 && -Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth) {
                if bank_short_name.len() < 14 {
                    g.news_msg = s(__content.string(4462)) + &bank_short_name;
                }
            }
            g.interest_rate_target = cast::<f32, _>(Ext::from_f32(11.0f32));
            g.monetary_policy = 2;
            g.silver_price_max = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) * sf(0.01f32) + Ext::from_f64(48.0)));
            if g.rand_seed % 2 == 1 {
                g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) * sf(0.01f32) + Ext::from_f64(30.0) + Ext::from_f64(10.0) * rng.rnd()));
            } else {
                g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) * sf(0.01f32) + Ext::from_f64(7.0)));
                g.silver_price_min = cast::<f32, _>(g.silver_target_price);
            }
            if g.rand_seed % 3 == 2 && -Ext::from_f64(6.0) > cast::<Ext, _>(g.gdp_growth) {
                g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(1.0f32));
            }
        }
        5 => {
            g.news_msg = s(__content.string(4463));
            if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                g.news_msg = replace(g.news_msg.as_bytes(), b"Banker", __content.bytes(4464));
                if g.rand_seed % 2 == 1 {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"jump", b"leap");
                }
            } else if g.rand_seed % 2 == 1 {
                g.news_msg = replace(g.news_msg.as_bytes(), b"scandal", __content.bytes(4465));
            }
            let story_text;
            if -Ext::from_f64(5.0) >= cast::<Ext, _>(g.gdp_growth) {
                g.tmp_news_str = PbStr::new();
                story_text = PbStr::new();
            } else {
                g.tmp_news_str = s(__content.string(4466));
                story_text = s(__content.string(4467));
            }
            show_misc_news_story(g, ui, rng, 71, story_text.as_bytes());
            for bank_id in 11..=85 {
                if g.entity_status[bank_id] == 1 {
                    let withdrawal = g.bank_c_ds[bank_id] * sf(0.06f32);
                    g.cash[bank_id] = g.cash[bank_id] - withdrawal;
                    g.bank_c_ds[bank_id] = g.bank_c_ds[bank_id] - withdrawal;
                    let withdrawal = g.bank_demand_deposits[bank_id] * sf(0.08f32);
                    g.cash[bank_id] = g.cash[bank_id] - withdrawal;
                    g.bank_demand_deposits[bank_id] = g.bank_demand_deposits[bank_id] - withdrawal;
                }
            }
            shock_commodity_price(g, rng, b"GOLD", 20);
        }
        4 => {
            g.news_msg = g.central_bank_name.clone() + __content.string(4468);
            if g.rand_seed % 2 == 1 {
                g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(4469), __content.bytes(4470));
            }
            if g.news_msg.len() < 62 {
                g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(4471), __content.bytes(4472));
            }
            g.interest_rate_target = cast::<f32, _>(Ext::from_f32(10.0f32));
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_target_price) * sf(1.25f32)));
        }
        3 => {
            g.news_msg = s(__content.string(4473));
            if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(4474), b"Barclay");
            }
            if bank_short_name.len() < 16 && g.rand_seed % 2 == 1 {
                g.news_msg = bank_short_name.clone() + __content.string(4475);
            }
            if Ext::from_f64(2.0) < Ext::from_f64(3.0) * rng.rnd() {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(7.5f32));
            }
            g.monetary_policy = 1;
            shock_commodity_price(g, rng, b"GOLD", -20);
            if g.rand_seed % 3 == 2 {
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(3.0)));
            }
        }
        2 => {
            if g.rand_seed % 2 == 1 {
                let n = rng.rnd_range(7, 2);
                let words = number_to_words(n, 0, 0);
                let words = mcase(words.as_bytes(), CaseMode::Ansi);
                g.news_msg = rtrim(ltrim(words.as_bytes()).as_bytes())
                    + __content.string(4476);
            } else {
                g.news_msg = s(__content.string(4477));
            }
            g.monetary_policy = 0;
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_target_price) * sf(0.7f32)));
        }
        1 => {
            g.news_msg = s(__content.string(4478));
            if sf(0.85f32) < cast::<Ext, _>(g.market_valuation) {
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.9f32) * cast::<Ext, _>(g.market_valuation)));
            }
            g.monetary_policy = 0;
            if Ext::from_f64(1.0) < Ext::from_f64(3.0) * rng.rnd() {
                g.interest_rate_target = cast::<f32, _>(Ext::from_f32(1.5f32));
            }
            if Ext::from_f64(6.0) < Ext::from_f64(7.0) * rng.rnd() {
                g.market_val_target = cast::<f64, _>(sf(1.2f32));
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.16f32) * cast::<Ext, _>(g.market_valuation)));
            }
        }
        -1 => {
            if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                g.news_msg = s(__content.string(4479));
            }
        }
        _ => {}
    }
    if !g.news_msg.is_empty() {
        if g.subprime_crisis_state != 5 {
            quick_message_news(g, ui);
            if g.news_msg.len() < 65 {
                g.news_msg = ucase(g.news_msg.as_bytes(), CaseMode::Ansi);
            }
        }
        post_news(g, ui, 0, 0);
    }
    if Ext::from_f64(9.0) < Ext::from_f64(10.0) * rng.rnd() {
        g.subprime_crisis_state = -1;
    } else {
        g.market_valuation = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.market_valuation) - sf(0.1f32)));
    }
    if g.subprime_crisis_state == -1 {
        g.econ_cycle_counter = 22;
        if sf(0.8f32) > g.market_val_target {
            g.market_val_target = cast::<f64, _>(sf(1.3f32) * g.market_val_target);
        } else if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
            g.market_val_target = cast::<f64, _>(sf(0.76f32) * g.market_val_target);
        }
        g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.2f32) + cast::<Ext, _>(g.market_valuation)));
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(2.0)));
    }
}

/// GOSUB Pandemic_ScaleMarketCaps (0x53dc8a): `MarketCap * (capChange + 1)` for every active company
/// 11..1580 outside industries 4, 21, 22 and 39.
fn pandemic_scale_market_caps(g: &mut Globals, cap_change: Ext) {
    for entity_id in 11..=1580 {
        if g.entity_status[entity_id] == 1 {
            let ind = g.industry_num[entity_id];
            if ind != 4 && ind != 21 && ind != 22 && ind != 39 {
                g.market_cap[entity_id] = (cap_change + Ext::from_f64(1.0)) * g.market_cap[entity_id];
            }
        }
    }
}

/// GOSUB Pandemic_AdjustIndDemand (0x53dddb): scales `IndDemand(3..69)`: the pandemic winners (4, 14,
/// 17, 21, 22, 26, 28..32, 34, 35, 39, 43, 46, 48, 50, 61, 62, 65) x1.2 (unchanged when the pandemic
/// ends, state -5), industries 16, 23, 53, 54, 57, 59 x0.9 (x0.47 at state 5, x1.4 at -5), the rest
/// x0.94 (x0.7 / x1.25).
fn pandemic_adjust_ind_demand(g: &mut Globals) {
    for industry in 3..=69 {
        let factor: f32 = cast::<f32, _>(match industry {
            4 | 14 | 17 | 21 | 22 | 26 | 28..=32 | 34 | 35 | 39 | 43 | 46 | 48 | 50 | 61 | 62 | 65 => {
                if g.pandemic_state != -5 {
                    g.ind_demand[industry] = ext_mul_f32_quad(1.2f32, g.ind_demand[industry]);
                }
                continue;
            }
            16 | 23 | 53 | 54 | 57 | 59 => {
                let mut f = Ext::from_f32(0.9f32);
                if g.pandemic_state == 5 {
                    f = Ext::from_f32(0.47f32);
                }
                if g.pandemic_state == -5 {
                    f = Ext::from_f32(1.4f32);
                }
                f
            }
            _ => {
                let mut f = Ext::from_f32(0.94f32);
                if g.pandemic_state == 5 {
                    f = Ext::from_f32(0.7f32);
                }
                if g.pandemic_state == -5 {
                    f = Ext::from_f32(1.25f32);
                }
                f
            }
        });
        g.ind_demand[industry] = ext_mul_f32_quad(factor, g.ind_demand[industry]);
    }
}

/// `fistp qword` of `SINGLE factor * QUAD q` in 80-bit precision.
fn ext_mul_f32_quad(factor: f32, q: i64) -> i64 {
    let (m, e) = f64_parts(cast::<Ext, _>(factor));
    ext_mul_quad_round(m, e, q)
}

/// The gold-price nudge `IF UpdateMetalsPriceBase() > limit THEN GoldPrice = GoldPrice - (rnd*a + b)`.
fn gold_drop_if_base_above(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, limit: Ext, a: Ext, b: Ext) {
    if update_metals_price_base(g, ui) > limit {
        g.gold_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_price) - (a * rng.rnd() + b)));
    }
}

/// Port of PandemicUpdate (0x53ca7b) with its GOSUBs Pandemic_ScaleMarketCaps (0x53dc8a) and
/// Pandemic_AdjustIndDemand (0x53dddb).
/// Runs the pandemic script of `PandemicState` (from the quarterly macro step):
///
/// * 0 / -1: nothing.
/// * -5 (pandemic over): caps +30%, demand recovery, "W.H [runtime resource wording] …" (pop-up +
///   news), state -1, gold target / price adjusted by `RandSeed MOD 3`.
/// * -2..-4 and below (a new virus scare): with `time_secs < 30` and -2 a false alarm (story 136, news,
///   caps +5%, GDP target +2, state 0, gold eased); with `time_secs > 30` (not -4) the epidemic is
///   contained (news, caps +5%, GDP target +2, `EventState(77) = 9` for -2, state 0, gold eased);
///   otherwise " [runtime resource wording] …'" (news, story 137 + state, caps -2%) and the pandemic starts
///   (state 5; the PIX.EXE picture is UI).
/// * 6 (Hainan virus): headline, story 138, then state 5 (`time_secs >= 20`) or contained (state 0,
///   `EventState(76) = 9`); pop-up + news.
/// * 5..1: the stages (lock-down: caps -41..-49%, `GdpLevel` cut, GDP -20, story 137; then death toll /
///   riots / collapse / burn-out headlines with caps -10..-30% or rallies, GDP cuts, industry demand
///   shifts, gold targets, gold-industry (50) growth and forecast); gold +200..+400 while the metals
///   base factor is below 3200; `GoldPriceMax` = base + 5000; pop-up below stage 5; above stage 1 the
///   valuation drops (x0.93, target 0.78, `MonetaryStance = 1`). The state counts down (twice, 1 in 3,
///   below 4) and becomes -5 at 0.
///
/// Then `EconCycleCounter = 5` and `PostNews`. Draws `RND`; `UpdateMetalsPriceBase` may post news.
/// `time_secs` = `VAL(RIGHT$(TIME$, 2))`. The ON ERROR handler only shows ShowRuntimeError.
pub fn pandemic_update(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let time_val = cast::<Ext, _>(time_secs);
    if g.pandemic_state == 0 || g.pandemic_state == -1 {
        return;
    }
    if g.pandemic_state < -1 {
        if g.pandemic_state == -5 {
            pandemic_scale_market_caps(g, sf(0.3f32));
            pandemic_adjust_ind_demand(g); // capChange = 0.6 (unused by the GOSUB)
            g.news_msg = s(__content.string(4480));
            if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                g.news_msg = replace(g.news_msg.as_bytes(), b"W.H.O.", b"U.N.");
            }
            let t = s(__content.string(4481)) + "\r\n" + "\r\n" + &g.news_msg;
            quick_message_copy(g, ui, t);
            post_news(g, ui, 0, 0);
            g.pandemic_state = -1;
            if g.rand_seed % 3 == 1 {
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(600.0) * rng.rnd() + Ext::from_f64(1400.0) + cast::<Ext, _>(g.metals_price_base)));
                gold_drop_if_base_above(g, ui, rng, Ext::from_f64(2500.0), Ext::from_f64(300.0), Ext::from_f64(400.0));
            } else if g.rand_seed % 3 == 2 {
                g.gold_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_price) - Ext::from_f64(73.0)));
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(4400.0)));
            }
            return;
        }
        if time_val < Ext::from_f64(30.0) && g.pandemic_state == -2 {
            show_misc_news_story(g, ui, rng, 136, b"");
            g.news_msg = s(__content.string(4482));
            post_news(g, ui, 0, 0);
            pandemic_scale_market_caps(g, sf(0.05f32));
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) + cast::<Ext, _>(g.gdp_growth_target)));
            g.pandemic_state = 0;
            if cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(1200.0) < cast::<Ext, _>(g.gold_target_price) {
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(1000.0)));
            }
            gold_drop_if_base_above(g, ui, rng, Ext::from_f64(1600.0), Ext::from_f64(200.0), Ext::from_f64(250.0));
            return;
        }
        let mut virus_name = PbStr::new();
        match g.pandemic_state {
            -2 => virus_name = s(__content.string(4483)),
            -3 => virus_name = s(__content.string(4484)),
            -4 => virus_name = s(__content.string(4485)),
            _ => {}
        }
        if g.pandemic_state == -2 || g.pandemic_state == -3 || g.pandemic_state == -4 {
            if time_val > Ext::from_f64(30.0) && g.pandemic_state != -4 {
                g.news_msg = virus_name.clone() + __content.string(4486);
                if g.pandemic_state == -4 {
                    g.news_msg = virus_name.clone() + __content.string(4487);
                }
                post_news(g, ui, 0, 0);
                pandemic_scale_market_caps(g, sf(0.05f32));
                g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) + cast::<Ext, _>(g.gdp_growth_target)));
                if g.pandemic_state == -2 {
                    g.event_state[77] = 9;
                }
                g.pandemic_state = 0;
                if cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(1200.0) < cast::<Ext, _>(g.gold_target_price) {
                    g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(900.0)));
                }
                gold_drop_if_base_above(g, ui, rng, Ext::from_f64(1400.0), Ext::from_f64(200.0), Ext::from_f64(150.0));
                return;
            }
            // (UI) DIALOG SHOW STATE hDlgMain, 0: SHELL DataPath + "PIX.EXE SWA|" + DataPath: DIALOG SHOW STATE 5
        }
        g.news_msg = s(__content.string(4488)) + &virus_name + __content.string(4489);
        post_news(g, ui, 0, 0);
        let story = cast::<i32, _>(g.pandemic_state) + 137;
        show_misc_news_story(g, ui, rng, story, b"");
        pandemic_scale_market_caps(g, -sf(0.02f32));
        g.pandemic_state = 5;
    } else if 0 < g.pandemic_state {
        if g.pandemic_state == 6 {
            g.news_msg = s(__content.string(4490));
            if Ext::from_f64(1.0) < Ext::from_f64(3.0) * rng.rnd() {
                g.news_msg = s(__content.string(4491));
            }
            show_misc_news_story(g, ui, rng, 138, b"");
            if time_val >= Ext::from_f64(20.0) {
                g.pandemic_state = 5;
            } else {
                g.pandemic_state = 0;
                g.event_state[76] = 9;
            }
            quick_message_news(g, ui);
            post_news(g, ui, 0, 0);
            return;
        }
        match g.pandemic_state {
            5 => {
                g.news_msg = s(__content.string(4492));
                if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                    g.news_msg = s(__content.string(4493));
                }
                post_news(g, ui, 0, 0);
                // capChange = -0.49 + 0.08 * RND (80-bit sum stored as DOUBLE)
                let r = rng.rnd();
                let m = f64_parts(sf(0.08f32));
                let prod = (m.0 * cast::<i128, _>(r * Ext::from_f64(2147483648.0)), m.1 - 31);
                let cap_change = parts_to_f64(add_parts(f64_parts(-sf(0.49f32)), prod));
                let a = add_parts(f64_parts(cap_change), (1, 0));
                g.gdp_level = ext_mul_quad_round(a.0, a.1, g.gdp_level);
                pandemic_scale_market_caps(g, cap_change);
                g.gdp_growth = cast::<f32, _>(cast::<f32, _>(-Ext::from_f64(20.0) + Ext::from_f64(2.0) * rng.rnd()));
                g.gdp_growth_target = cast::<f32, _>(-Ext::from_f32(20.0f32));
                pandemic_adjust_ind_demand(g);
                show_misc_news_story(g, ui, rng, 137, b"");
                if update_metals_price_base(g, ui) < Ext::from_f64(2300.0) {
                    g.gold_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(223.0) + cast::<Ext, _>(g.gold_price)));
                }
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(3200.0)));
            }
            4 => {
                g.news_msg = s(__content.string(4494));
                if g.rand_seed % 3 == 1 {
                    g.news_msg = s(__content.string(4495));
                }
                let cap_change;
                if g.rand_seed % 3 == 2 {
                    g.news_msg = s(__content.string(4496));
                    if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                        g.news_msg = replace(g.news_msg.as_bytes(), b"W.H.O.", b"U.N.");
                    }
                    cap_change = sf(0.35f32);
                    g.market_val_target = cast::<f64, _>(Ext::from_f64(1.5));
                    g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.21f32) * cast::<Ext, _>(g.market_valuation)));
                } else {
                    cap_change = -sf(0.1f32);
                }
                pandemic_scale_market_caps(g, cap_change);
                if -Ext::from_f64(10.0) < cast::<Ext, _>(g.gdp_growth) {
                    g.gdp_growth = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth) - Ext::from_f64(10.0)));
                }
                pandemic_adjust_ind_demand(g);
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(3600.0)));
                if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                    g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(1500.0)));
                }
                if g.ind_growth_rate[50] < 30 {
                    g.ind_growth_rate[50] = cast::<i16, _>(to_quad(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(30.0)));
                }
                g.ind_growth_forecast[50] = 50;
            }
            3 => {
                g.news_msg = s(__content.string(4497));
                if g.rand_seed % 4 == 1 {
                    g.news_msg = s(__content.string(4498));
                }
                if g.rand_seed % 4 == 2 {
                    g.news_msg = s(__content.string(4499));
                }
                if g.rand_seed % 4 == 3 {
                    g.news_msg = s(__content.string(4500));
                }
                if -Ext::from_f64(8.0) < cast::<Ext, _>(g.gdp_growth) {
                    g.gdp_growth = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth) - Ext::from_f64(11.0)));
                }
                pandemic_scale_market_caps(g, -sf(0.2f32));
                pandemic_adjust_ind_demand(g);
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(4000.0)));
                if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                    g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(1500.0)));
                }
                if g.ind_growth_rate[50] < 30 {
                    g.ind_growth_rate[50] = cast::<i16, _>(to_quad(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(30.0)));
                }
                g.ind_growth_forecast[50] = 40;
            }
            2 => {
                g.news_msg = s(__content.string(4501));
                if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                    g.news_msg = s(__content.string(4502));
                }
                if -Ext::from_f64(6.0) < cast::<Ext, _>(g.gdp_growth) {
                    g.gdp_growth = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth) - Ext::from_f64(8.0)));
                }
                pandemic_scale_market_caps(g, -sf(0.15f32));
                pandemic_adjust_ind_demand(g);
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(5000.0)));
                if Ext::from_f64(2.0) < Ext::from_f64(3.0) * rng.rnd() {
                    g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(1500.0)));
                }
                if g.ind_growth_rate[50] < 30 {
                    g.ind_growth_rate[50] = cast::<i16, _>(to_quad(Ext::from_f64(9.0) * rng.rnd() + Ext::from_f64(30.0)));
                }
                g.ind_growth_forecast[50] = 50;
            }
            1 => {
                g.news_msg = s(__content.string(4503));
                let mut cap_change = -sf(0.3f32);
                if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                    g.news_msg = s(__content.string(4500));
                    cap_change = sf(0.2f32);
                }
                pandemic_scale_market_caps(g, cap_change);
                if -Ext::from_f64(4.0) < cast::<Ext, _>(g.gdp_growth) {
                    g.gdp_growth = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth) - Ext::from_f64(5.0)));
                }
                if g.rand_seed % 5 > 2 {
                    g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(3000.0)));
                    g.ind_growth_forecast[50] = 20;
                } else {
                    g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(800.0)));
                    g.ind_growth_forecast[50] = -9;
                }
            }
            _ => {}
        }
        if update_metals_price_base(g, ui) < Ext::from_f64(3200.0) {
            g.gold_price = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(200.0) + cast::<Ext, _>(g.gold_price)) + Ext::from_f64(200.0) * rng.rnd()));
        }
        g.gold_price_max = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(5000.0)));
        if g.pandemic_state < 5 {
            let t = s(__content.string(4504)) + "\r\n" + "\r\n" + &g.news_msg;
            quick_message_copy(g, ui, t);
        }
        if 1 < g.pandemic_state {
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.93f32) * cast::<Ext, _>(g.market_valuation)));
            g.market_val_target = cast::<f64, _>(sf(0.78f32));
            g.monetary_stance = 1;
        }
        g.pandemic_state -= 1;
        if g.pandemic_state < 4 {
            if Ext::from_f64(2.0) < Ext::from_f64(3.0) * rng.rnd() {
                g.pandemic_state -= 1;
            }
        }
        if g.pandemic_state < 1 {
            g.pandemic_state = -5;
        }
    }
    g.econ_cycle_counter = 5;
    post_news(g, ui, 0, 0);
}

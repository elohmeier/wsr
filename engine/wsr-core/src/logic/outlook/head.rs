//! Operating industries (3..69): setup and the first paragraph (0x56f69a .. 0x5752c0).

#![allow(
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::nonminimal_bool,
    clippy::manual_range_contains
)]
use crate::pb::real::{Ext, cast};


use super::{Ctx, s, sf};
use crate::logic::earnings::calc_cur_month_of_year;
use crate::logic::futures::update_metals_price_base;
use crate::logic::outlook::commentary::{
    ind_outlook_cost_commentary, ind_outlook_growth_commentary, ind_outlook_slow_economy_commentary,
};
use crate::logic::report::number_to_words;
use crate::logic::valuation::{
    calc_industry_avg_net_margin, calc_industry_outlook, price_pct_change_months, price_trend_direction,
    price_trend_streak,
};
use crate::pb::ext80::Ext80;
use crate::pb::numeric::str_num;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{CaseMode, instr, lcase, mcase, remove, replace, right, ucase};
use crate::pb::numeric::{ceil, to_long};
use crate::state::Globals;
use crate::ui::Ui;

const CRLF: &str = "\r\n";

/// Price-history series: crude [runtime resource wording] corn, GDP growth.
pub(super) const SERIES_OIL: i32 = 6;
pub(super) const SERIES_GOLD: i32 = 7;
pub(super) const SERIES_SILVER: i32 = 8;
pub(super) const SERIES_WHEAT: i32 = 9;
pub(super) const SERIES_CORN: i32 = 10;
pub(super) const SERIES_GDP: i32 = 1604;

/// `x = REPLACE$(x, find, with)`.
pub(super) fn rep(x: &mut PbStr, find: &str, with: &str) {
    *x = replace(x.as_bytes(), find.as_bytes(), with.as_bytes());
}

/// `x = x + a`.
pub(super) fn app(x: &mut PbStr, a: &PbStr) {
    *x = x.clone() + a;
}

/// `INT(x)` (call 0x6bcd29, x87 floor) of an extended value, as EXT.
fn int80(x: Ext80) -> Ext {
    let mut r = x.frndint();
    if r.cmp_val(x) == std::cmp::Ordering::Greater {
        r = r - Ext80::from_i64(1);
    }
    Ext::from(r.into_ext())
}

/// `INT(x)` of a SINGLE local.
fn int_s(x: f32) -> f32 {
    cast::<f32, _>((cast::<Ext, _>(x)).floor())
}

/// `STR$(n)` of a LONG.
fn str_l(n: i32) -> PbStr {
    PbStr::from(str_num(cast::<Ext, _>(n), 15).as_str())
}

/// The `(industryId + CurQuarter) MOD m` selector of the phrase variants.
fn q_mod(c: &Ctx, m: i32) -> i32 {
    cast::<i32, _>(c.ind_plus_q) % m
}

/// Part of BuildIndustryOutlookText (0x56f33b): margins, supply/demand, industry name and the first
/// paragraph. Returns false where the original exits (no leading company, `topCorpId` < 131).
pub(super) fn first_paragraph(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> bool {
    let __content = crate::content::current();
    let ind = c.industry_id;
    let iu = cast::<i32, _>(ind);
    c.gdp_trend = price_trend_direction(g, SERIES_GDP, 3, 3);
    let sim_co_count = g.sim_co_count;
    c.month_val = calc_cur_month_of_year(g, sim_co_count);
    c.month_num = to_long(ceil(c.month_val));
    c.net_margin = calc_industry_avg_net_margin(g, rng, ind);
    c.top_corp_id = calc_industry_outlook(g, rng, ind);
    c.cur_slack_pct = cast::<f32, _>(g.tmp_ext1);
    c.proj_slack_pct = cast::<f32, _>(g.tmp_ext2);
    c.proj_supply = g.tmp_ext3;
    c.proj_margin = cast::<f32, _>((cast::<Ext, _>(c.net_margin) - cast::<Ext, _>(c.proj_slack_pct)) + cast::<Ext, _>(c.cur_slack_pct));
    // INT((1 - IndTotalAssets / IndDemand) * 100); the ratio is stored as a DOUBLE
    let one = Ext80::from_i64(1);
    let hundred = Ext80::from_i64(100);
    let demand = Ext80::from_i64(g.ind_demand[iu]);
    let ratio = crate::pb::numeric::double((Ext80::from_ext(g.ind_total_assets) / demand).into_ext());
    c.cur_slack_pct = cast::<f32, _>(int80((one - Ext80::from_ext(ratio)) * hundred));
    let proj_demand = (Ext80::from_i64(cast::<i64, _>(g.ind_growth_rate[iu])) * Ext80::from_f32(0.005f32)) * demand + demand;
    let proj_ratio = crate::pb::numeric::double((Ext80::from_ext(g.tmp_ext3) / proj_demand).into_ext());
    c.proj_slack_pct = cast::<f32, _>(int80((one - Ext80::from_ext(proj_ratio)) * hundred));
    let total_assets = g.ind_total_assets;
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    if c.top_corp_id < 131 {
        return false;
    }
    let top = c.top_corp_id;
    c.top_corp_share_pct =
        ((Ext80::from_ext(g.business_assets[top]) / Ext80::from_ext(total_assets)) * hundred).to_i32();
    c.ind_name = mcase(g.ind_name[iu].as_bytes(), CaseMode::Ansi);
    if 0 < instr(1, c.ind_name.as_bytes(), b"&") {
        rep(&mut c.ind_name, "&", "and");
    }
    if 0 < instr(1, c.ind_name.as_bytes(), b" -- ") {
        rep(&mut c.ind_name, " -- ", "--");
    }
    if 0 < instr(1, c.ind_name.as_bytes(), b"--") {
        rep(&mut c.ind_name, "--", " - ");
    }
    match ind {
        22 => c.ind_name = s(__content.string(7620)),
        29 => c.ind_name = s(__content.string(7621)),
        32 => c.ind_name = s(__content.string(7622)),
        39 => c.ind_name = s(__content.string(7623)),
        65 => c.ind_name = s(__content.string(7624)),
        69 => c.ind_name = s(__content.string(7625)),
        _ => {}
    }
    c.ind_name_lc = lcase(c.ind_name.as_bytes(), CaseMode::Ansi);
    if ind == 65 && 0 < instr(1, c.ind_name_lc.as_bytes(), b"tv") {
        rep(&mut c.ind_name_lc, "tv", "TV");
    }
    if g.ind_growth_rate[iu] < g.ind_growth_forecast[iu] {
        c.growth_trend = 1;
    }
    if g.ind_growth_forecast[iu] < g.ind_growth_rate[iu] {
        c.growth_trend = 2;
    }
    c.trend_val = q_mod(c, 4);
    c.head_text = s(__content.string(7626));
    if c.trend_val == 1 {
        c.head_text = s(__content.string(7627));
    }
    if c.trend_val == 2 {
        c.head_text = s(__content.string(7628));
    }
    if c.trend_val == 3 {
        c.head_text = s(__content.string(7629));
    }
    if !(19.4f32 < c.net_margin) {
        if !(10.4f32 < c.net_margin) {
            low_margin(g, ui, rng, c);
        } else {
            moderate_margin(g, rng, c);
        }
    } else {
        high_margin(g, ui, rng, c);
    }
    economy_comments(g, rng, c);
    boom_and_commentary(g, ui, rng, c);
    true
}

/// netMargin <= 10.4 (0x5718e1 ..): depressed industries.
fn low_margin(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = c.industry_id;
    let iu = cast::<i32, _>(ind);
    let top = c.top_corp_id;
    c.phrase_text = s(__content.string(7630));
    if g.rand_seed % 4 == 2 {
        c.phrase_text = s(__content.string(7631));
    }
    if g.rand_seed % 4 == 3 {
        c.phrase_text = s(__content.string(7632));
    }
    if c.net_margin < -Ext::from_f32(5.0f32) {
        c.phrase_text = s(__content.string(7633));
        if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
            c.phrase_text = s(" grim");
            c.cmp_val = 20;
            if c.ind_name.len() < c.cmp_val {
                c.phrase_text = s(" bleak");
            }
            c.phrase_text = c.phrase_text.clone() + " for ";
            if c.growth_trend == 2 {
                c.phrase_text = s(__content.string(7634)) + &c.phrase_text;
            }
        }
    }
    if !(int_s(c.proj_slack_pct) > int_s(c.cur_slack_pct)) {
        if int_s(c.proj_slack_pct) == int_s(c.cur_slack_pct) {
            c.outlook_rating = if c.proj_supply <= g.ind_total_assets { 8 } else { 7 };
            c.para_text = s(__content.string(7635))
                + __content.string(7636);
            if q_mod(c, 3) == 1 {
                rep(&mut c.para_text, __content.string(7637), __content.string(7638));
            }
            if q_mod(c, 3) == 2 {
                rep(&mut c.para_text, __content.string(7639), __content.string(7640));
            }
            if q_mod(c, 2) == 1 {
                rep(&mut c.para_text, "future", "term");
            }
            if q_mod(c, 3) == 0 {
                rep(&mut c.para_text, __content.string(2830), __content.string(7641));
            }
            c.outlook_rating = if c.proj_supply <= g.ind_total_assets { 10 } else { 9 };
        } else if c.proj_supply <= g.ind_total_assets {
            c.para_text = s(__content.string(7642));
            if matches!(ind, 26 | 27 | 29 | 36 | 37 | 39 | 62 | 65) {
                if g.cur_quarter < 3 {
                    rep(&mut c.para_text, __content.string(7643), __content.string(7644));
                } else {
                    rep(&mut c.para_text, __content.string(7643), __content.string(7645));
                }
            } else if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                rep(&mut c.para_text, "reduce", __content.string(7646));
            }
            if 3 < g.growth_rate[top] {
                c.para_text = s(", as ")
                    + &g.ent_name[top]
                    + __content.string(7647)
                    + "with a"
                    + &number_to_words(c.top_corp_share_pct, 0, 1)
                    + __content.string(7648);
            }
            c.outlook_rating = 10;
        } else {
            c.clause_text = s(__content.string(7649));
            g.rand_seed += 1;
            c.rand_pick = g.rand_seed % 6;
            match c.rand_pick {
                1 => c.clause_text = s("battle"),
                2 => c.clause_text = s("vie"),
                3 => c.clause_text = s("fight"),
                4 => c.clause_text = s("compete"),
                _ => {}
            }
            c.para_text = s(__content.string(7650)) + &c.clause_text + __content.string(7651);
            if g.ind_long_term_growth[iu] < g.growth_rate[top] && 8 < g.growth_rate[top] && 29 < c.top_corp_share_pct {
                c.para_text = c.para_text.clone()
                    + &g.ent_name[top]
                    + __content.string(7652)
                    + &str_l(c.top_corp_share_pct)
                    + __content.string(7653)
                    + __content.string(7654);
                if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                    rep(&mut c.para_text, __content.string(7655), "rival");
                }
                if matches!(ind, 8 | 36 | 37 | 63 | 65) {
                    rep(&mut c.para_text, "expand", __content.string(7656));
                }
                let share = c.top_corp_share_pct;
                if share < 45 {
                    if share < 42 {
                        if share < 38 {
                            if 34 < share && !matches!(ind, 9 | 14 | 15 | 50 | 69) {
                                rep(&mut c.para_text, __content.string(7657), __content.string(7658));
                                if matches!(ind, 11 | 16 | 23 | 25 | 30 | 36 | 37 | 52 | 59 | 65) {
                                    rep(&mut c.para_text, "prices", "rates");
                                }
                            }
                        } else {
                            rep(&mut c.para_text, __content.string(7659), __content.string(7660));
                        }
                    } else {
                        rep(
                            &mut c.para_text,
                            __content.string(7659),
                            __content.string(7661),
                        );
                    }
                } else {
                    rep(&mut c.para_text, __content.string(7662), __content.string(7663));
                    if g.cur_quarter == 3 {
                        rep(&mut c.para_text, "alarm", "concern");
                    }
                    if matches!(ind, 3 | 9 | 10 | 12 | 13 | 18 | 20 | 24 | 38 | 40 | 60 | 66) {
                        rep(&mut c.para_text, __content.string(7664), __content.string(7665));
                    }
                }
            }
            c.outlook_rating = 9;
        }
    } else {
        if c.proj_supply <= g.ind_total_assets {
            c.para_text = s(__content.string(7666))
                + __content.string(7667);
            if g.ind_growth_rate[iu] == 0 {
                rep(&mut c.para_text, "falling", __content.string(7668));
                c.para_text = remove(c.para_text.as_bytes(), __content.bytes(7669));
            } else if matches!(ind, 26 | 27 | 29 | 36 | 37 | 39 | 62 | 65) {
                rep(&mut c.para_text, __content.string(7670), "costs");
            }
            if 0 < g.ind_growth_rate[iu] {
                c.para_text = s(__content.string(7671));
                if 40 < ind && ind < 49 {
                    rep(&mut c.para_text, __content.string(7672), __content.string(7673));
                }
                c.clause_text = s(__content.string(7674));
                if ind == 5 {
                    rep(&mut c.clause_text, __content.string(7675), __content.string(7676));
                }
                if (g.cur_quarter + top) % 2 == 1 {
                    c.clause_text = s(__content.string(7677))
                        + __content.string(7678);
                    if 2 < g.cur_quarter {
                        rep(&mut c.clause_text, __content.string(7679), __content.string(7680));
                    }
                }
                if g.growth_rate[top] < -1 {
                    c.para_text = s(__content.string(7681))
                        + __content.string(7682)
                        + &g.ent_name[top]
                        + ", is "
                        + &c.clause_text;
                }
            }
            if 4 < g.growth_rate[top] && g.ind_growth_rate[iu] < g.growth_rate[top] {
                c.para_text = s(", as ")
                    + &g.ent_name[top]
                    + __content.string(7683)
                    + __content.string(7684);
            }
            c.outlook_rating = 8;
        } else {
            c.para_text = s(__content.string(7685));
            let competitive = matches!(ind, 4 | 22 | 26 | 27 | 29 | 36 | 37 | 39 | 41..=48 | 56..=58 | 62..=64);
            if g.growth_rate[top] < -2 && !(g.ind_growth_rate[iu] <= g.growth_rate[top]) {
                c.clause_text = PbStr::new();
                if g.ind_growth_rate[iu] < 0 {
                    c.clause_text = s(__content.string(7686));
                }
                c.para_text = s(__content.string(7687))
                    + &c.clause_text
                    + __content.string(7688)
                    + "leader "
                    + &g.ent_name[top]
                    + __content.string(7689);
                if rng.rnd() * Ext::from_f64(5000.0) + Ext::from_f64(10000.0) < g.market_cap[top] {
                    rep(&mut c.para_text, "leader", "titan");
                }
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    rep(&mut c.para_text, "reduce", "give up");
                }
                if competitive {
                    rep(&mut c.para_text, __content.string(7690), __content.string(7691));
                }
            } else if competitive {
                // LAB_005726dd
                rep(&mut c.para_text, __content.string(7637), __content.string(7691));
            }
            c.outlook_rating = 7;
        }
    }
    c.cmp_val = 0;
    if instr(1, c.para_text.as_bytes(), g.ent_name[top].as_bytes()) == c.cmp_val {
        if 46 < c.top_corp_share_pct && 14 < g.growth_rate[top] {
            c.para_text = c.para_text.clone()
                + __content.string(7692)
                + &g.ent_name[top]
                + __content.string(7693)
                + &number_to_words(c.top_corp_share_pct, 0, 1)
                + __content.string(7694)
                + __content.string(7695);
        }
        if 2 < g.cur_quarter {
            rep(&mut c.para_text, __content.string(7696), __content.string(7697));
        }
    }
    c.cond_flag = 0;
    match ind {
        7 | 8 => {
            if g.oil_price < Ext::from_f32(0.73f32) {
                c.cmp_val = -10;
                if price_pct_change_months(g, SERIES_OIL, 6) < c.cmp_val {
                    c.cmp_val = 0;
                    if price_trend_direction(g, SERIES_OIL, 2, 1) < c.cmp_val {
                        c.cond_flag = 1;
                    }
                }
            }
            c.body_text = s("oil");
        }
        15 => {
            if (cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price)) < sf(7.6f32) {
                c.cmp_val = -10;
                if price_pct_change_months(g, SERIES_WHEAT, 6) < c.cmp_val {
                    c.cmp_val = -5;
                    if price_pct_change_months(g, SERIES_CORN, 6) < c.cmp_val {
                        c.cond_flag = 1;
                    }
                }
            }
            c.body_text = s("grain");
        }
        50 => {
            if metals_index(g, ui) < Ext::from_f64(27.0) {
                c.cmp_val = -10;
                if price_pct_change_months(g, SERIES_GOLD, 6) < c.cmp_val {
                    c.cmp_val = -5;
                    if price_pct_change_months(g, SERIES_SILVER, 6) < c.cmp_val {
                        c.cond_flag = 1;
                    }
                }
            }
            c.body_text = s(__content.string(7698));
        }
        28 => {
            if sf(15.0f32) < (cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price)) {
                c.cmp_val = 10;
                if c.cmp_val < price_pct_change_months(g, SERIES_WHEAT, 6) {
                    c.cmp_val = 5;
                    if c.cmp_val < price_pct_change_months(g, SERIES_CORN, 6) {
                        c.cond_flag = 2;
                    }
                }
            }
            c.body_text = s(__content.string(7699));
        }
        11 | 16 | 23 | 25 | 52 | 69 => {
            c.body_text = s("oil");
            if Ext::from_f32(1.25f32) < g.oil_price {
                c.cmp_val = 10;
                if c.cmp_val < price_pct_change_months(g, SERIES_OIL, 6) {
                    c.cmp_val = 0;
                    if c.cmp_val < price_trend_direction(g, SERIES_OIL, 2, 1) {
                        c.cond_flag = 2;
                    }
                }
            }
        }
        _ => {}
    }
    if c.cond_flag == 1 {
        if ind == 8 {
            c.para_text = c.para_text.clone()
                + CRLF
                + CRLF
                + __content.string(7700)
                + __content.string(7701);
        } else {
            c.para_text = c.para_text.clone()
                + CRLF
                + CRLF
                + __content.string(6955)
                + &c.ind_name_lc
                + __content.string(7702)
                + __content.string(7703)
                + &c.body_text
                + __content.string(7704);
        }
    } else if c.cond_flag == 2 {
        c.para_text = c.para_text.clone()
            + CRLF
            + CRLF
            + __content.string(7705)
            + &c.body_text
            + __content.string(7706)
            + __content.string(7707);
        if ind == 28 {
            rep(&mut c.para_text, "fuel", __content.string(2644));
        }
    }
}

/// `(SilverPrice - MetalsPriceBase * .01) + UpdateMetalsPriceBase() / 100` (the silver term is
/// evaluated with the old `MetalsPriceBase`, before the call updates it).
fn metals_index(g: &mut Globals, ui: &mut dyn Ui) -> Ext {
    let silver = cast::<Ext, _>(g.silver_price) - cast::<Ext, _>(g.metals_price_base) * sf(0.01f32);
    let base = update_metals_price_base(g, ui);
    base / Ext::from_f64(100.0) + silver
}

/// 10.4 < netMargin <= 19.4: moderately profitable industries.
fn moderate_margin(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = c.industry_id;
    let iu = cast::<i32, _>(ind);
    c.phrase_text = s(__content.string(7708));
    match g.rand_seed % 5 {
        1 => rep(&mut c.phrase_text, __content.string(7709), __content.string(7710)),
        2 => rep(&mut c.phrase_text, __content.string(7709), __content.string(7711)),
        3 => rep(&mut c.phrase_text, __content.string(1426), __content.string(7712)),
        4 => rep(&mut c.phrase_text, __content.string(7709), __content.string(7713)),
        _ => {}
    }
    if !(int_s(c.proj_slack_pct) > int_s(c.cur_slack_pct)) {
        if int_s(c.proj_slack_pct) == int_s(c.cur_slack_pct) {
            c.para_text = s(__content.string(7714))
                + __content.string(7715);
            if q_mod(c, 3) == 1 {
                rep(&mut c.para_text, "stable", __content.string(7716));
            }
            if q_mod(c, 3) == 2 {
                rep(&mut c.para_text, "level", __content.string(7717));
            }
            if g.cur_quarter == 4 && 1000 < g.sim_co_count {
                c.para_text = s(__content.string(7718))
                    + __content.string(7719);
            }
            c.outlook_rating = if c.proj_supply <= g.ind_total_assets { 6 } else { 5 };
        } else {
            c.clause_text = s("erode");
            if g.rand_seed % 3 == 1 {
                c.clause_text = s("slip");
            }
            if g.rand_seed % 3 == 2 {
                c.clause_text = s("decline");
            }
            c.para_text = s(__content.string(7720)) + &c.clause_text + __content.string(7721);
            if c.growth_trend == 2 && g.ind_growth_rate[iu] < 0 {
                c.para_text = c.para_text.clone() + __content.string(7722);
            }
            c.outlook_rating = 6;
        }
    } else {
        c.para_text = s(__content.string(7723));
        if c.growth_trend == 1 && 0 < g.ind_growth_rate[iu] {
            c.para_text = c.para_text.clone() + __content.string(7724);
            if g.ind_growth_rate[iu] < 6 {
                rep(&mut c.para_text, __content.string(7725), __content.string(7726));
            }
            if c.proj_supply < g.ind_total_assets {
                c.para_text = c.para_text.clone() + __content.string(7727);
                if 40 < ind && ind < 49 {
                    rep(&mut c.para_text, __content.string(7670), __content.string(7728));
                }
            }
        }
        c.outlook_rating = 5;
    }
    c.para_text = c.para_text.clone() + ". ";
    c.clause_text = PbStr::new();
    if !(-2.2f32 > g.gdp_growth) || !(Ext::from_f32(13.0f32) < c.net_margin) {
        if 4.8f32 < g.gdp_growth && c.net_margin < Ext::from_f32(17.5f32) && Ext::from_f32(10.0f32) < c.proj_margin {
            c.clause_text = s(__content.string(7729))
                + &c.ind_name
                + __content.string(7730)
                + __content.string(7731);
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                rep(&mut c.clause_text, "reason", "respect");
            }
        }
    } else {
        if !(rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0)) || !(-7.5f32 > g.gdp_growth) || !(Ext::from_f32(12.0f32) < c.proj_margin) {
            c.clause_text = s(__content.string(7732))
                + __content.string(7733);
            if Ext::from_f32(15.0f32) < c.proj_margin {
                rep(&mut c.clause_text, __content.string(7734), "earning");
            }
            if g.rand_seed % 3 == 2 {
                rep(&mut c.clause_text, "weak", __content.string(1260));
            }
            if g.rand_seed % 3 == 1 {
                rep(&mut c.clause_text, "weak", __content.string(7735));
            }
        } else {
            c.clause_text = s(__content.string(7736));
            if 15.6f32 < c.proj_margin {
                rep(&mut c.clause_text, __content.string(7737), __content.string(7738));
            }
        }
    }
    c.para_text = c.para_text.clone() + &c.clause_text;
}

/// netMargin > 19.4 (0x56fcd2 ..): highly profitable industries.
fn high_margin(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = c.industry_id;
    let iu = cast::<i32, _>(ind);
    let top = c.top_corp_id;
    if !(int_s(c.proj_slack_pct) > int_s(c.cur_slack_pct)) {
        if int_s(c.proj_slack_pct) == int_s(c.cur_slack_pct) {
            c.phrase_text = s(__content.string(7739));
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                rep(&mut c.phrase_text, __content.string(7740), "bright");
            }
            c.para_text = s(__content.string(7741));
            if g.rand_seed % 2 == 1 {
                c.para_text = c.para_text.clone()
                    + __content.string(7742)
                    + __content.string(7743);
                if q_mod(c, 2) == 1 {
                    rep(&mut c.para_text, __content.string(7744), __content.string(7745));
                }
            } else {
                c.para_text = c.para_text.clone()
                    + __content.string(7746);
            }
            c.outlook_rating = if c.proj_supply <= g.ind_total_assets { 2 } else { 1 };
        } else {
            c.phrase_text = s(__content.string(7747));
            if Ext::from_f32(35.0f32) < c.net_margin {
                c.phrase_text = s(__content.string(7748));
            }
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) && !(Ext::from_f32(33.0f32) < c.net_margin) {
                c.phrase_text = s(__content.string(7749));
            }
            if c.proj_supply <= g.ind_total_assets {
                c.para_text = s(__content.string(7750));
                c.outlook_rating = 4;
            } else {
                c.para_text = s(__content.string(7751));
                if Ext::from_f32(25.0f32) < c.net_margin && c.net_margin < Ext::from_f32(29.0f32) {
                    rep(&mut c.para_text, __content.string(7752), __content.string(7753));
                }
                if !(Ext::from_f32(33.0f32) < c.net_margin) {
                    if Ext::from_f32(28.0f32) < c.net_margin {
                        rep(&mut c.para_text, "soon", __content.string(7754));
                    }
                } else {
                    rep(
                        &mut c.para_text,
                        "soon",
                        __content.string(7755),
                    );
                }
                if Ext::from_f32(33.0f32) < c.proj_margin {
                    rep(&mut c.para_text, __content.string(7756), __content.string(7757));
                }
                let mut to_227 = true;
                if ind == 50 {
                    to_227 = false;
                    if !(metals_index(g, ui) < Ext::from_f64(30.0)) {
                        to_227 = true;
                    } else {
                        c.cmp_val = 0;
                        if c.cmp_val <= price_pct_change_months(g, SERIES_GOLD, 3) {
                            to_227 = true;
                        } else {
                            c.cmp_val = 0;
                            if c.cmp_val <= price_pct_change_months(g, SERIES_SILVER, 3) {
                                to_227 = true;
                            } else {
                                c.cmp_val = 0;
                                if price_trend_streak(g, SERIES_GOLD) <= c.cmp_val {
                                    c.cmp_val = 0;
                                    if price_trend_streak(g, SERIES_SILVER) <= c.cmp_val {
                                        rep(&mut c.para_text, __content.string(7758), __content.string(7759));
                                    }
                                }
                            }
                        }
                    }
                }
                if to_227 {
                    // LAB_00571227
                    let mut done = false;
                    if ind == 28 && (cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price)) < sf(8.4f32) {
                        c.cmp_val = 0;
                        if c.cmp_val < price_trend_streak(g, SERIES_WHEAT) {
                            c.cmp_val = 0;
                            if c.cmp_val < price_trend_streak(g, SERIES_CORN) {
                                rep(&mut c.para_text, __content.string(7758), __content.string(7760));
                                done = true;
                            }
                        }
                    }
                    if !done && ind == 12 {
                        rep(&mut c.para_text, __content.string(7761), __content.string(7762));
                    }
                }
                c.outlook_rating = 3;
            }
        }
    } else {
        c.phrase_text = s(__content.string(7739));
        if q_mod(c, 4) == 1 {
            c.phrase_text = s(__content.string(7763));
            if Ext::from_f32(30.0f32) < c.net_margin {
                rep(&mut c.phrase_text, "high", __content.string(7764));
            }
            if Ext::from_f32(40.0f32) < c.net_margin {
                rep(&mut c.phrase_text, "very", __content.string(7765));
            }
        }
        if q_mod(c, 4) == 2 {
            c.phrase_text = s(__content.string(7766));
        }
        if q_mod(c, 4) == 3 {
            c.phrase_text = s(__content.string(7767));
        }
        if c.proj_supply <= g.ind_total_assets {
            c.para_text = s(__content.string(7768));
            if g.cur_quarter == 1 || g.cur_quarter == 3 {
                rep(&mut c.para_text, __content.string(7641), __content.string(2830));
            }
            if g.growth_rate[top] < -2 && q_mod(c, 2) == 1 {
                c.para_text = s(__content.string(7769))
                    + &g.ent_name[top]
                    + __content.string(7770);
                if 40 < ind && ind < 49 {
                    rep(&mut c.para_text, __content.string(7771), __content.string(7772));
                }
                if g.rand_seed % 2 == 1 {
                    rep(&mut c.para_text, __content.string(7773), __content.string(7774));
                }
            } else {
                c.clause_text = s(__content.string(7672));
                let cl = c.clause_text.to_display();
                match ind {
                    3 | 10 | 12 | 13 | 18 | 20 | 33 | 34 | 35 | 53..=55 | 60 | 61 | 66..=68 => {
                        if g.cur_quarter == 2 || g.cur_quarter == 4 {
                            rep(&mut c.para_text, __content.string(7670), __content.string(2579));
                        } else if iu % 2 == 1 {
                            rep(&mut c.para_text, &cl, __content.string(7775));
                            if g.cur_quarter == 2 {
                                rep(&mut c.para_text, "plants", __content.string(7776));
                            }
                            if g.cur_quarter == 3 {
                                rep(&mut c.para_text, "plants", __content.string(7777));
                            }
                        } else {
                            rep(&mut c.para_text, &cl, __content.string(7778));
                        }
                    }
                    41..=48 | 57 => {
                        if g.cur_quarter < 3 && ind != 57 {
                            rep(&mut c.para_text, __content.string(7670), __content.string(7779));
                        } else {
                            rep(&mut c.para_text, &cl, __content.string(7780));
                        }
                    }
                    69 => rep(&mut c.para_text, &cl, __content.string(7781)),
                    6 | 7 | 8 | 9 | 11 | 14 | 15 | 17 | 19 | 24 | 30 | 32 | 38 | 40 | 49 | 51 | 64 => {
                        rep(&mut c.para_text, &cl, __content.string(7782));
                        if q_mod(c, 2) == 1 {
                            rep(&mut c.para_text, "cutting", __content.string(7783));
                        }
                        if g.cur_quarter == 1 || g.cur_quarter == 3 {
                            rep(&mut c.para_text, __content.string(7784), __content.string(7785));
                        }
                    }
                    4 | 21 | 26..=29 | 36 | 37 | 39 | 56 | 59 | 62 | 63 | 65 => {
                        if g.cur_quarter < 3 {
                            rep(&mut c.para_text, &cl, __content.string(7786));
                        } else {
                            rep(&mut c.para_text, __content.string(7670), "staff");
                        }
                    }
                    _ => {}
                }
                c.clause_text = PbStr::new();
            }
            c.outlook_rating = 2;
        } else {
            if 0 < instr(1, c.phrase_text.as_bytes(), b"hefty") {
                rep(&mut c.phrase_text, "hefty", __content.string(7787));
            }
            c.para_text = s(__content.string(7788));
            c.cmp_val = cast::<i32, _>(g.ind_growth_rate[iu]);
            if cast::<i32, _>(g.growth_rate[top]) + 3 < c.cmp_val {
                c.para_text = c.para_text.clone()
                    + __content.string(7789)
                    + &g.ent_name[top]
                    + __content.string(7790)
                    + __content.string(7791);
                if rng.rnd() * Ext::from_f64(3000.0) + Ext::from_f64(5000.0) < g.market_cap[top] {
                    rep(&mut c.para_text, "leader", "giant");
                }
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    rep(&mut c.para_text, __content.string(7792), __content.string(7793));
                    if g.rand_seed % 2 == 1 {
                        rep(&mut c.para_text, "greater", "to grow");
                    }
                }
                if 3 < g.growth_rate[top] || (iu + g.cur_quarter) % 2 == 1 {
                    if matches!(ind, 3 | 5 | 11..=13 | 16 | 18 | 23..=25 | 32 | 35 | 36 | 52 | 57 | 59 | 62 | 65..=68) {
                        rep(&mut c.para_text, __content.string(7794), __content.string(7795));
                        match ind {
                            11 | 23 | 25 | 36 | 52 | 65 => rep(&mut c.para_text, "prices", "rates"),
                            16 => rep(&mut c.para_text, "prices", "fares"),
                            59 => rep(&mut c.para_text, "prices", __content.string(7796)),
                            _ => {}
                        }
                    }
                }
                if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(3.0) {
                    rep(&mut c.para_text, "profits", __content.string(7797));
                }
            }
            c.para_text = c.para_text.clone() + ". ";
            c.outlook_rating = 1;
        }
    }
    c.cond_flag = 0;
    match ind {
        7 => {
            c.cond_flag = cast::<i32, _>(1.2f32 < g.oil_price);
            c.body_text = s("oil");
        }
        15 => {
            c.cond_flag = cast::<i32, _>(sf(13.0f32) < cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price));
            c.body_text = s("grain");
        }
        50 => {
            if Ext::from_f64(43.0) < metals_index(g, ui) {
                c.cond_flag = 1;
            }
            c.body_text = s(__content.string(7798));
        }
        28 => {
            if (cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price)) < sf(8.4f32) {
                c.cmp_val = -10;
                if price_pct_change_months(g, SERIES_WHEAT, 6) < c.cmp_val {
                    c.cmp_val = -5;
                    if price_pct_change_months(g, SERIES_CORN, 6) < c.cmp_val {
                        c.cond_flag = 2;
                    }
                }
            }
            c.body_text = s("grain");
        }
        11 | 16 | 23 | 25 | 52 | 69 => {
            c.body_text = s("oil");
            if g.oil_price < Ext::from_f32(0.77f32) {
                c.cmp_val = -10;
                if price_pct_change_months(g, SERIES_OIL, 6) < c.cmp_val {
                    c.cmp_val = 0;
                    if price_trend_direction(g, SERIES_OIL, 2, 1) < c.cmp_val {
                        c.cond_flag = 2;
                    }
                }
            }
        }
        _ => {}
    }
    if ind == 23 && 3.6f32 < g.gdp_growth && g.gdp_growth < Ext::from_f32(4.8f32) {
        c.para_text = c.para_text.clone()
            + __content.string(7799)
            + __content.string(7800);
        c.comment_count += 1;
    }
    if c.cond_flag == 1 {
        c.para_text = c.para_text.clone()
            + CRLF
            + CRLF
            + __content.string(7801)
            + &c.body_text
            + __content.string(7802)
            + __content.string(7803);
    } else if c.cond_flag == 2 {
        c.para_text = c.para_text.clone()
            + CRLF
            + CRLF
            + __content.string(7804)
            + &c.body_text
            + __content.string(2520)
            + __content.string(7805);
        if ind == 28 {
            rep(&mut c.para_text, "fuel", "food");
        }
    }
    if ind == 61 && 4.8f32 < g.gdp_growth {
        c.para_text = c.para_text.clone()
            + __content.string(7806)
            + "\""
            + "cloud"
            + "\""
            + __content.string(7807)
            + __content.string(7808);
    }
}

/// The weak/strong economy remarks (after the margin branches, up to LAB_00574ba5).
fn economy_comments(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = c.industry_id;
    if matches!(ind, 5 | 6 | 17 | 22 | 27 | 28 | 29 | 31 | 32 | 34 | 46 | 50 | 55 | 56 | 65 | 69) {
        return;
    }
    if !(4.8f32 < g.gdp_growth) {
        if g.gdp_growth < Ext::from_f32(0.5f32) {
            if 4 < c.outlook_rating || !(Ext::from_f32(15.0f32) < c.proj_margin) {
                if c.outlook_rating < 7 || !(Ext::from_f32(14.0f32) > c.proj_margin) {
                    if 6 < c.outlook_rating && !(Ext::from_f32(14.0f32) > c.proj_margin) {
                        c.cmp_val = 65;
                        if c.cmp_val < c.para_text.len() {
                            c.para_text = c.para_text.clone() + CRLF + CRLF;
                        }
                        c.para_text = c.para_text.clone()
                            + __content.string(7809)
                            + __content.string(7810);
                        c.clause_text = s(__content.string(7811));
                        if c.net_margin < Ext::from_f32(0.0f32) {
                            c.clause_text = s(__content.string(7812));
                        }
                        if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) {
                            c.clause_text = s(__content.string(7813));
                        }
                        c.para_text = c.para_text.clone() + &c.clause_text;
                    }
                } else {
                    c.cmp_val = 100;
                    if c.cmp_val < c.para_text.len() {
                        c.para_text = c.para_text.clone() + CRLF + CRLF;
                    }
                    c.para_text =
                        c.para_text.clone() + __content.string(7814);
                    match g.rand_seed % 5 {
                        1 => rep(&mut c.para_text, "dragged", "held"),
                        2 => rep(&mut c.para_text, __content.string(7815), __content.string(7816)),
                        3 => rep(&mut c.para_text, __content.string(7815), __content.string(7817)),
                        4 => rep(&mut c.para_text, "dragged", "brought"),
                        _ => {}
                    }
                    if matches!(ind, 11 | 23 | 25 | 52) {
                        c.para_text = c.para_text.clone()
                            + __content.string(7818);
                        if 2 < g.cur_quarter {
                            rep(&mut c.para_text, "slacken", "slow");
                        }
                        if ind == 23 {
                            rep(&mut c.para_text, "rail", "sea");
                        }
                        if ind == 25 {
                            rep(&mut c.para_text, "rail", "truck");
                        }
                        if ind == 52 {
                            rep(&mut c.para_text, "rail", "air");
                        }
                        c.comment_count += 1;
                    } else if ind == 30 {
                        c.para_text = c.para_text.clone()
                            + CRLF
                            + CRLF
                            + __content.string(7819)
                            + __content.string(7820);
                    } else {
                        if ind == 9 && 1.2f32 < g.oil_price && g.ind_growth_rate[9] < 0 {
                            c.para_text = c.para_text.clone()
                                + CRLF
                                + CRLF
                                + __content.string(7821)
                                + __content.string(7822);
                            return; // LAB_00574ba5
                        }
                        if ind == 26 && g.ind_growth_rate[24] < -2 {
                            c.para_text = c.para_text.clone()
                                + __content.string(7823);
                        }
                    }
                }
            } else {
                if !c.para_text.is_empty() {
                    c.para_text = c.para_text.clone() + CRLF + CRLF;
                }
                c.clause_text = s(__content.string(7824))
                    + &c.ind_name
                    + __content.string(1975)
                    + __content.string(7825);
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    rep(&mut c.clause_text, "Even ", "Al");
                }
                if q_mod(c, 3) == 1 {
                    c.clause_text = s("The ")
                        + &c.ind_name
                        + __content.string(7826);
                } else if q_mod(c, 3) == 2 {
                    c.clause_text =
                        s(__content.string(7827)) + &c.ind_name + __content.string(7828);
                }
                if c.ind_name == __content.string(7829) {
                    rep(&mut c.clause_text, __content.string(7829), "drug");
                }
                c.para_text = c.para_text.clone() + &c.clause_text;
            }
        }
    } else {
        c.clause_text = PbStr::new();
        if 6 < c.outlook_rating && c.proj_margin < Ext::from_f32(12.0f32) {
            c.clause_text = s(__content.string(7830))
                + "in the "
                + &c.ind_name
                + __content.string(7831)
                + __content.string(7832);
            if g.rand_seed % 2 == 1 {
                rep(&mut c.clause_text, __content.string(7833), __content.string(7834));
            }
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                rep(&mut c.clause_text, __content.string(7835), __content.string(7836));
            }
            c.cmp_val = 60;
            if c.cmp_val < c.para_text.len() {
                c.clause_text = s(CRLF) + CRLF + &c.clause_text;
            }
            if q_mod(c, 2) != 1 || c.proj_margin < Ext::from_f32(1.0f32) {
                c.clause_text = PbStr::new();
            }
            c.para_text = c.para_text.clone() + &c.clause_text;
        }
    }
}

/// LAB_00574ba5: the economic-boom remark, the commentary builders and the assembly of the first
/// paragraph into `headText`.
fn boom_and_commentary(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = c.industry_id;
    // BOOM_REMARK
    if 4.8f32 < g.gdp_growth
        && c.outlook_rating < 5
        && Ext::from_f32(14.0f32) < c.proj_margin
        && 19.4f32 < c.net_margin
        && !matches!(
            ind,
            4 | 9 | 10 | 13 | 17 | 18 | 20 | 21 | 22 | 24 | 28 | 29 | 31 | 32 | 33 | 34 | 35 | 36 | 38 | 42 | 43 | 45
                | 46 | 47 | 49 | 50 | 54 | 55 | 56 | 58 | 60 | 62 | 63 | 65 | 69
        )
    {
        boom_remark(g, c);
    }
    c.body_text = PbStr::new();
    let gdp = g.gdp_growth;
    if !(1.8f32 < gdp) || 4.8f32 < gdp {
        if !(4.8f32 < gdp) || !(19.4f32 < c.net_margin) {
            if !(1.8f32 < gdp) && c.net_margin < Ext::from_f32(10.4f32) && c.comment_count == 0 {
                c.body_text = slow(g, ui, rng, c);
            }
        } else if c.comment_count == 0 {
            c.body_text = slow(g, ui, rng, c);
        }
    } else if c.net_margin < Ext::from_f32(10.4f32) || 19.4f32 < c.net_margin {
        let (mut r, mut nm, mut pm, mut nmname) = (c.outlook_rating, c.net_margin, c.proj_margin, c.ind_name.clone());
        c.body_text = ind_outlook_growth_commentary(g, ui, rng, ind, &mut r, &mut nm, &mut pm, &mut nmname);
        (c.outlook_rating, c.net_margin, c.proj_margin, c.ind_name) = (r, nm, pm, nmname);
    }
    if c.body_text.is_empty() && c.net_margin < Ext::from_f32(19.4f32) {
        let (mut r, mut nm, mut pm, mut nmname) = (c.outlook_rating, c.net_margin, c.proj_margin, c.ind_name.clone());
        c.body_text = ind_outlook_cost_commentary(g, ui, rng, ind, &mut r, &mut nm, &mut pm, &mut nmname);
        (c.outlook_rating, c.net_margin, c.proj_margin, c.ind_name) = (r, nm, pm, nmname);
    }
    if c.body_text.is_empty()
        && 4.8f32 < g.gdp_growth
        && c.net_margin < Ext::from_f32(10.4f32)
        && instr(1, c.para_text.as_bytes(), b"lagging") == 0
        && matches!(ind, 3..=6 | 8..=11 | 14 | 16 | 20..=34 | 36..=42 | 44..=50 | 52..=56 | 58..=62 | 64..=69)
    {
        c.clause_text = s(__content.string(7837));
        if c.net_margin < Ext::from_f32(0.0f32) {
            c.clause_text = s(__content.string(7838));
        }
        c.body_text =
            s(__content.string(7839)) + &c.clause_text + __content.string(7840);
    }
    c.cmp_val = 50;
    if c.cmp_val < c.para_text.len() && !c.body_text.is_empty() {
        c.clause_text = right(c.para_text.as_bytes(), 2);
        if instr(1, c.clause_text.as_bytes(), CRLF.as_bytes()) == 0 {
            c.body_text = s(CRLF) + CRLF + &c.body_text;
        }
    }
    c.para_text = c.para_text.clone() + &c.body_text;
    if c.para_text.is_empty() {
        c.para_text = s(".");
    }
    if c.phrase_text.is_empty() {
        c.head_text = PbStr::new();
    } else {
        c.head_text = c.head_text.clone()
            + &c.phrase_text
            + "the "
            + &ucase(c.ind_name.as_bytes(), CaseMode::Ansi)
            + __content.string(1157)
            + &c.para_text
            + CRLF
            + CRLF;
    }
    c.phrase_text = PbStr::new();
    c.body_text = PbStr::new();
    c.clause_text = PbStr::new();
}

fn slow(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> PbStr {
    let (mut r, mut nm, mut pm, mut nmname) = (c.outlook_rating, c.net_margin, c.proj_margin, c.ind_name.clone());
    let t = ind_outlook_slow_economy_commentary(g, ui, rng, c.industry_id, &mut r, &mut nm, &mut pm, &mut nmname);
    (c.outlook_rating, c.net_margin, c.proj_margin, c.ind_name) = (r, nm, pm, nmname);
    t
}

/// "Company profits in the … [runtime resource wording] " (0x574ba5 ..)
fn boom_remark(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    c.clause_text =
        s(__content.string(7841)) + &c.ind_name + __content.string(7842);
    if g.rand_seed % 5 == 1 {
        rep(&mut c.clause_text, "buoy", "lift");
    }
    if g.rand_seed % 5 == 2 {
        rep(&mut c.clause_text, "buoy", "aid");
    }
    if g.rand_seed % 5 == 3 {
        c.clause_text = s("The ") + &c.ind_name + __content.string(7843);
    }
    if g.rand_seed % 5 == 4 {
        c.clause_text = s(__content.string(7844))
            + &c.ind_name
            + __content.string(1037);
    }
    c.cmp_val = 60;
    if c.cmp_val < c.para_text.len() {
        c.clause_text = s(CRLF) + CRLF + &c.clause_text;
    }
    app(&mut c.para_text, &c.clause_text.clone());
}

//! The commentary sentence builders of BuildIndustryOutlookText: `IndOutlook_GrowthCommentary`
//! (0x583605), `IndOutlook_SlowEconomyCommentary` (0x586441) and `IndOutlook_CostCommentary` (0x5878e4).
//!
//! All three take their arguments BYREF but only copy them into locals at entry (none is written
//! back). SINGLE locals/pool constants are compared as `f32`/`sf(x)`; the comparison directions
//! (including equality) follow the machine code, not the decompiler.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::too_many_lines,
    clippy::nonminimal_bool,
    clippy::manual_range_contains
)]
use crate::pb::real::{Ext, cast};


use super::{s, sf};
use crate::logic::futures::update_metals_price_base;
use crate::logic::valuation::{price_pct_change_months, price_trend_direction, price_trend_streak};
use crate::pb::numeric::str_num;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, lcase, remove, replace, CaseMode};
use crate::pb::numeric::to_long;
use crate::state::Globals;
use crate::ui::Ui;

/// `x = REPLACE$(x, a, b)`.
fn rep(x: &mut PbStr, a: &str, b: &str) {
    *x = replace(x.as_bytes(), a.as_bytes(), b.as_bytes());
}

/// `CHR$(34) + x + CHR$(34)`.
fn quoted(x: &str) -> PbStr {
    s("\"") + x + "\""
}

/// `LCASE$(name)` with "tv" -> "TV" for industry 65 (shared prologue of the three builders).
fn lower_name(industry_id: i16, name: &PbStr) -> PbStr {
    let mut lc = lcase(name.as_bytes(), CaseMode::Ansi);
    if industry_id == 65 && 0 < instr(1, lc.as_bytes(), b"tv") {
        rep(&mut lc, "tv", "TV");
    }
    lc
}

fn growth(g: &Globals, ind: i32) -> i32 {
    cast::<i32, _>(g.ind_growth_rate[ind])
}

/// Port of IndOutlook_GrowthCommentary (0x583605).
/// Sentence for a moderately growing economy: with GDP growth >= 3 and a net margin below 10.4, why
/// the industry lags; with GDP growth below 3 and a margin above 19.4, why it does well anyway; else,
/// with a margin above 19.4 and nothing said yet, an industry-specific note on its strength. Empty if
/// nothing applies. Draws RND for some industries; industry 50 calls `UpdateMetalsPriceBase`.
#[allow(clippy::too_many_arguments)]
pub fn ind_outlook_growth_commentary(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    industry_id: i16,
    outlook_rating: &mut i32,
    net_margin: &mut f32,
    proj_margin: &mut f32,
    ind_name: &mut PbStr,
) -> PbStr {
    let __content = crate::content::current();
    let price_trend = price_trend_direction(g, 1604, 3, 3);
    let ind = industry_id;
    let rating = *outlook_rating;
    let margin = *net_margin;
    let proj = *proj_margin;
    let name = ind_name.clone();
    let name_lc = lower_name(ind, &name);
    let ind_q = cast::<i16, _>(cast::<i32, _>(ind) + g.cur_quarter);
    let oil = g.oil_price;
    let gdp = g.gdp_growth;
    let mut result = PbStr::new();
    let mut comment = PbStr::new();
    let mut phrase;
    let i = cast::<i32, _>(ind);

    if !(cast::<f32, _>(sf(19.4f32)) >= margin) && !(Ext::from_f32(3.0f32) <= gdp) {
        // profitable industry in a slow (< 3 %) economy
        if i == 50 || i == 53 {
            return result;
        }
        match i {
            3 | 67 | 68 => {
                comment = s(__content.string(6833))
                    + &name
                    + __content.string(1975)
                    + __content.string(6834);
            }
            4 => {
                comment = s(__content.string(6835));
            }
            5 | 6 => {
                comment = s(__content.string(6836));
                if ind == 6 {
                    rep(&mut comment, "housing", __content.string(6837));
                }
            }
            7 | 8 | 9 | 12 | 20 | 33 | 51 | 69 => {
                phrase = s("energy");
                if ind == 9 {
                    phrase = s(__content.string(6838));
                }
                if ind == 12 {
                    phrase = s("steel");
                }
                if ind == 20 {
                    phrase = s(__content.string(6839));
                }
                if ind == 33 {
                    phrase = s(__content.string(6840));
                }
                if ind == 51 {
                    phrase = s(__content.string(6841));
                }
                if ind == 69 {
                    phrase = s(__content.string(6842));
                }
                comment = s(__content.string(6843)) + &phrase + __content.string(6844);
                'oil: {
                    if ind < 9 && Ext::from_f32(30.0f32) < margin {
                        if sf(1.25f32) < cast::<Ext, _>(oil) {
                            rep(&mut comment, "strong", __content.string(6845));
                        }
                        if Ext::from_f32(20.0f32) < proj && (cast::<Ext, _>(oil)) < sf(1.1f32) && sf(0.75f32) < cast::<Ext, _>(oil) {
                            if 0 <= price_trend_streak(g, 6) {
                                if -10 <= price_pct_change_months(g, 6, 12) && sf(0.93f32) <= cast::<Ext, _>(oil) {
                                    break 'oil;
                                }
                            }
                            comment = comment
                                + ". The "
                                + &name_lc
                                + __content.string(6846)
                                + __content.string(6847);
                            if (cast::<Ext, _>(oil)) < sf(0.93f32) {
                                rep(&mut comment, __content.string(6848), "low");
                            }
                            if ind == 7 {
                                comment = comment
                                    + __content.string(6849);
                            } else {
                                comment = comment + __content.string(6850);
                            }
                        }
                    }
                }
                // LAB_00583d32
                let r = rng.rnd();
                if (cast::<Ext, _>(oil)) < r * sf(0.2f32) + sf(0.6f32) {
                    if growth(g, i) < 3 && (ind == 20 || ind == 51) {
                        comment = s(__content.string(6851))
                            + __content.string(6852);
                        if price_pct_change_months(g, 6, 6) < -14 {
                            if price_pct_change_months(g, 6, 3) < -5 {
                                rep(&mut comment, "low", "lower");
                            }
                        }
                    }
                }
                comment = comment + ". ";
            }
            11 => {
                if sf(1.22f32) < cast::<Ext, _>(oil) && price_trend < 0 {
                    if price_trend_direction(g, 1604, 2, 1) < 0 {
                        comment = s(__content.string(6853));
                    }
                }
            }
            13 | 49 | 54 => {
                comment = s(__content.string(6854))
                    + __content.string(6855)
                    + __content.string(6856)
                    + &name_lc
                    + __content.string(6857);
            }
            14 => {
                if growth(g, 16) < 3 {
                    comment = s(__content.string(6858))
                        + __content.string(6859);
                }
            }
            15 => {
                let v = to_long(cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price));
                if 9 < v && v < 12 {
                    comment = s(__content.string(6860))
                        + __content.string(6861);
                }
            }
            16 => {
                if sf(0.87f32) < cast::<Ext, _>(oil) && (cast::<Ext, _>(oil)) < sf(1.12f32) && price_trend < 0 {
                    comment = s(__content.string(6862));
                }
            }
            17 => {
                if price_trend < 0 && 0 < growth(g, i) {
                    comment = s(__content.string(6863));
                } else if growth(g, i) < 0 {
                    comment = s(__content.string(6864))
                        + __content.string(6865);
                }
            }
            23 => {
                let mut done = false;
                if price_trend < 0 {
                    if price_trend_direction(g, 1604, 2, 1) < 0 {
                        comment = s(__content.string(6866))
                            + __content.string(6867);
                        done = true;
                    }
                }
                if !done && 0 < price_trend {
                    if 0 < price_trend_direction(g, 1604, 2, 1) {
                        comment = s(__content.string(6868));
                    }
                }
            }
            24 | 26 | 35 | 40 => {
                if Ext::from_f32(18.0f32) < proj {
                    comment = s(__content.string(6869))
                        + __content.string(6870)
                        + &name
                        + __content.string(6871);
                }
            }
            25 => {
                comment = s(__content.string(6872));
            }
            27 => {
                if 4 < growth(g, i) {
                    comment = s(__content.string(6873))
                        + __content.string(6874);
                }
            }
            28 | 32 | 62 => {
                comment = s("The ")
                    + &name_lc
                    + __content.string(6875)
                    + __content.string(6876)
                    + __content.string(6877);
                phrase = s(__content.string(6878));
                if ind == 32 {
                    phrase = s(__content.string(6879));
                }
                if ind == 62 {
                    phrase = s(__content.string(6880));
                }
                comment = comment + &phrase;
            }
            29 | 65 => {
                comment = s("The ")
                    + &name_lc
                    + __content.string(6881)
                    + __content.string(6882);
                if ind == 65 {
                    rep(&mut comment, __content.string(6883), __content.string(6884));
                }
            }
            36 => {
                comment = name.clone()
                    + __content.string(6885)
                    + __content.string(6886);
            }
            37 => {
                comment = s("The ")
                    + &name_lc
                    + __content.string(6887)
                    + __content.string(6888);
            }
            38 => {
                comment = s(__content.string(6889))
                    + __content.string(6890);
            }
            _ if i < 41 || 48 < i => match i {
                52 => {
                    comment = s(__content.string(6891))
                        + __content.string(6892)
                        + __content.string(6893);
                }
                55 => {
                    comment = s(__content.string(6894))
                        + __content.string(6895)
                        + __content.string(6896);
                }
                57 => {
                    if price_trend < 0 {
                        if price_trend_direction(g, 1604, 2, 1) < 0 {
                            if 4 < growth(g, i) {
                                comment = s(__content.string(6897))
                                    + __content.string(6898);
                            }
                        }
                    }
                }
                58 => {
                    if 2 < growth(g, i) {
                        comment = s("The ")
                            + &name
                            + __content.string(6899);
                        if price_trend < 0 {
                            if price_trend_direction(g, 1604, 2, 1) < 0 {
                                rep(&mut comment, __content.string(1450), __content.string(6900));
                            }
                        }
                    }
                }
                59 => {
                    comment = s("The ")
                        + &name_lc
                        + __content.string(6901)
                        + __content.string(6902);
                }
                60 => {
                    if growth(g, 5) < 0 {
                        comment = s(__content.string(6903))
                            + __content.string(6904)
                            + __content.string(6905);
                    }
                }
                61 => {
                    if 6 < growth(g, 30) {
                        comment = s(__content.string(6906))
                            + __content.string(6907)
                            + str_num(cast::<Ext, _>(growth(g, 30)), 7).as_str()
                            + "%. ";
                    }
                }
                64 => {
                    comment = s(__content.string(6908))
                        + __content.string(6909)
                        + __content.string(6910);
                }
                66 => {
                    comment = name.clone()
                        + __content.string(6911)
                        + __content.string(6912)
                        + __content.string(6913);
                }
                _ => {}
            },
            _ => {
                // retail 41..48
                if growth(g, i) < 4 {
                    comment = s(__content.string(6914));
                }
            }
        }
        // LAB_00584bff
        if !comment.is_empty() {
            result = comment.clone();
        }
    } else if margin < cast::<f32, _>(sf(10.4f32)) && Ext::from_f32(3.0f32) <= gdp {
        // unprofitable industry in a growing (>= 3 %) economy
        match i {
            3 | 67 | 68 => {
                comment = s(__content.string(6915))
                    + &name
                    + __content.string(6916)
                    + __content.string(6917);
            }
            4 | 21 | 22 | 56 => {
                comment = s(__content.string(6918))
                    + __content.string(6919)
                    + __content.string(6920)
                    + &name_lc
                    + __content.string(1037);
            }
            5..=13 | 18 | 20 | 23..=26 | 33 | 36 | 38..=40 | 49 | 51..=54 | 58..=61 | 63 => {
                let o = cast::<Ext, _>(oil);
                let oil_ok = (sf(0.7f32) <= o && o <= sf(1.3f32))
                    || (ind != 7
                        && ind != 8
                        && ind != 11
                        && ind != 23
                        && ind != 25
                        && ind != 52
                        && ind != 53
                        && ind != 59
                        && ind != 63);
                let rate = cast::<Ext, _>(g.base_interest_rate);
                let rate_ok = (sf(5.1f32) <= rate && rate <= sf(9.2f32)) || (ind != 5 && ind != 6 && ind != 63);
                if oil_ok && rate_ok && proj < Ext::from_f32(12.0f32) && sf(0.7f32) < o && o < sf(1.3f32) {
                    comment = s(__content.string(6921))
                        + __content.string(6922)
                        + &name
                        + __content.string(1975);
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        rep(&mut comment, "humming", "rolling");
                    }
                    let variant = cast::<i32, _>(ind_q) % 4;
                    phrase = s(__content.string(6923));
                    if variant == 1 {
                        phrase = s(__content.string(6924));
                    }
                    if variant == 2 {
                        phrase = s(__content.string(6925));
                    }
                    if variant == 3 {
                        phrase = s(__content.string(6926));
                    }
                    if margin < -Ext::from_f32(5.0f32) && proj < -Ext::from_f32(10.0f32) {
                        phrase = s(__content.string(6927));
                        let mut clause = s(__content.string(6928));
                        if variant < 2 {
                            clause = s(__content.string(6929));
                        }
                        phrase = phrase + &clause + ". ";
                    }
                    comment = comment + &phrase;
                }
            }
            14 => {
                comment = s(__content.string(6930))
                    + __content.string(6931)
                    + &name
                    + __content.string(1157);
                if growth(g, 52) + growth(g, 16) < 7 {
                    phrase = s(". ");
                } else {
                    phrase = s(__content.string(6932));
                }
                comment = comment + &phrase;
            }
            15 => {
                if Ext::from_f64(12.0) < cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price) {
                    comment = s(__content.string(6933))
                        + __content.string(6934)
                        + __content.string(6935)
                        + __content.string(6936);
                }
            }
            16 => {
                comment = s(__content.string(6937))
                    + __content.string(6938);
                if g.cur_quarter < 3 {
                    comment = comment + __content.string(6939);
                } else {
                    comment = comment + __content.string(6940);
                }
            }
            17 => {
                comment = s(__content.string(6941))
                    + __content.string(6942)
                    + &name_lc
                    + __content.string(1037);
            }
            30 => {
                comment = s(__content.string(6943))
                    + __content.string(6944);
            }
            31 => {
                if rating < 9 {
                    if 6 < rating {
                        comment = s(__content.string(6945))
                            + __content.string(6946);
                    }
                } else {
                    comment = s(__content.string(6947))
                        + __content.string(6948);
                }
            }
            34 => {
                if growth(g, i) < 0 {
                    comment = s(__content.string(6949))
                        + __content.string(6950);
                }
            }
            50 => {
                // (SilverPrice - MetalsPriceBase * .01) + .01 * UpdateMetalsPriceBase() < 25
                let silver_net = cast::<Ext, _>(g.silver_price) - cast::<Ext, _>(g.metals_price_base) * sf(0.01f32);
                let base = update_metals_price_base(g, ui);
                if silver_net + sf(0.01f32) * base < Ext::from_f64(25.0) && (cast::<Ext, _>(g.base_interest_rate)) < Ext::from_f64(10.0) {
                    comment = s(__content.string(6951))
                        + __content.string(6952);
                }
            }
            62 => {
                comment = s(__content.string(6953))
                    + &name_lc
                    + __content.string(4980)
                    + __content.string(6954);
            }
            64 => {
                if proj < Ext::from_f32(13.0f32) {
                    comment = s(__content.string(6955))
                        + &name
                        + __content.string(6956)
                        + __content.string(6957)
                        + &name_lc
                        + __content.string(6958);
                }
            }
            65 => {
                comment = s(__content.string(6959))
                    + __content.string(6960);
            }
            69 if sf(0.91f32) < cast::<Ext, _>(oil) => {
                comment = s(__content.string(6961))
                    + __content.string(6962);
            }
            _ => {}
        }
        if !comment.is_empty() {
            result = comment.clone();
        }
    }

    // a profitable industry with nothing said yet
    if !(comment.is_empty() && sf(19.4f32) < cast::<Ext, _>(*net_margin)) {
        return result;
    }
    let oil = cast::<Ext, _>(g.oil_price);
    match i {
        3 | 67 | 68 => {
            if 3 < growth(g, i) && sf(0.88f32) < oil && oil < sf(1.13f32) {
                comment = s(__content.string(6963));
                if ind == 3 {
                    rep(&mut comment, "Asia", __content.string(6964));
                }
                if ind == 67 {
                    rep(&mut comment, "Asia", __content.string(6965));
                }
            }
        }
        10 => {
            comment = s(__content.string(6966))
                + __content.string(6967);
        }
        11 => {
            if oil < sf(1.05f32) {
                comment = s(__content.string(6968))
                    + __content.string(6969);
            }
        }
        15 => {
            comment = s(__content.string(6970));
            if price_pct_change_months(g, 9, 3) < 0 {
                if price_pct_change_months(g, 10, 3) < 0 && (cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price)) < Ext::from_f64(13.0) {
                    comment = remove(comment.as_bytes(), __content.bytes(6971));
                }
            }
        }
        16 => {
            if Ext::from_f32(16.0f32) < proj {
                comment = s(__content.string(6972))
                    + __content.string(6973);
            }
        }
        17 => {
            if 0 < growth(g, i) {
                comment = s(__content.string(6974))
                    + __content.string(6975);
            }
        }
        18 => {
            let v = growth(g, 67) + growth(g, 3) + growth(g, 68);
            if rating < 3 && 8 < v {
                comment = s(__content.string(6976));
                if !(sf(3.8f32) < cast::<Ext, _>(g.gdp_growth)) {
                    phrase = s(". ");
                } else {
                    phrase = s(__content.string(6977));
                }
                comment = comment + &phrase;
            }
        }
        19 => {
            comment = s(__content.string(6978));
            if g.cur_year % 2 == 1 {
                rep(&mut comment, __content.string(6979), __content.string(6980));
            }
        }
        21 => {
            comment = s(__content.string(6981))
                + __content.string(6982);
        }
        4 | 22 | 56 => {
            comment = s(__content.string(6983));
            if ind == 22 {
                rep(&mut comment, __content.string(6984), __content.string(6985));
            }
        }
        29 => {
            comment = s(__content.string(6986))
                + __content.string(6987);
        }
        30 => {
            comment = s(__content.string(6988))
                + __content.string(6989);
        }
        31 => {
            comment = s(__content.string(6990));
            if growth(g, 31) < 2 {
                rep(&mut comment, __content.string(6991), __content.string(6992));
            }
        }
        34 => {
            if growth(g, i) < 4 {
                if growth(g, i) < 0 {
                    comment = s(__content.string(6993));
                }
            } else {
                comment = s(__content.string(6994));
            }
        }
        39 => {
            comment = s(__content.string(6995));
        }
        43 => {
            comment = s(__content.string(6996))
                + &name
                + __content.string(6997)
                + __content.string(6998);
        }
        46 | 55 => {
            comment = s(__content.string(6999))
                + __content.string(7000);
            if ind == 55 {
                rep(&mut comment, __content.string(7001), __content.string(7002));
            }
        }
        59 => {
            comment = s(__content.string(7003))
                + __content.string(7004);
        }
        61 => {
            comment = s(__content.string(7005))
                + __content.string(7006);
        }
        63 => {
            let gdp = cast::<Ext, _>(g.gdp_growth);
            if sf(3.7f32) <= gdp || sf(1.22f32) < oil {
                if g.cur_quarter % 2 == 1 && sf(1.22f32) < oil {
                    if Ext::from_f64(39.0) < g.ind_demand_gap[8] + g.ind_demand_gap[7] {
                        comment = s(__content.string(7007))
                            + __content.string(7008)
                            + &name_lc
                            + __content.string(7009);
                    }
                }
            } else {
                comment = name.clone()
                    + __content.string(7010)
                    + __content.string(7011);
                if oil < sf(0.88f32) {
                    comment = comment
                        + __content.string(7012)
                        + __content.string(7013);
                }
            }
        }
        _ => {}
    }
    comment
}

/// Port of IndOutlook_SlowEconomyCommentary (0x586441).
/// Sentence for the economy's extremes: with GDP growth <= 1.8 and a net margin below 10.4, how the
/// slow economy hurts the industry; with GDP growth above 4.8 and a margin above 19.4, how the boom
/// helps it. Empty otherwise (and for some industries). `outlook_rating` is not used. Draws RND for
/// some industries.
#[allow(clippy::too_many_arguments)]
pub fn ind_outlook_slow_economy_commentary(
    g: &mut Globals,
    _ui: &mut dyn Ui,
    rng: &mut Rng,
    industry_id: i16,
    _outlook_rating: &mut i32,
    net_margin: &mut f32,
    proj_margin: &mut f32,
    ind_name: &mut PbStr,
) -> PbStr {
    let __content = crate::content::current();
    let price_trend = price_trend_direction(g, 1604, 3, 3);
    let ind = industry_id;
    let i = cast::<i32, _>(ind);
    let margin = *net_margin;
    let proj = *proj_margin;
    let name = ind_name.clone();
    let name_lc = lower_name(ind, &name);
    let ind_q = cast::<i16, _>(cast::<i32, _>(ind) + g.cur_quarter);
    let gdp = cast::<Ext, _>(g.gdp_growth);
    let mut comment = PbStr::new();
    let mut extra = PbStr::new();
    let mut phrase = PbStr::new();

    if !(sf(4.8f32) >= gdp) && !(cast::<f32, _>(sf(19.4f32)) >= margin) {
        // booming economy, profitable industry
        match i {
            4 | 17 | 29 | 31 | 32 | 46 | 55 | 62 | 65 => {
                comment = s("The ")
                    + &name
                    + __content.string(7014)
                    + __content.string(7015);
                if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                    comment = comment + ". ";
                } else {
                    comment = comment + __content.string(7016);
                }
            }
            10 | 13 | 20 | 24 | 33 | 35 | 36 | 38 | 40 | 42 | 45 | 47 | 49 | 54 | 58 | 60 | 63 => {
                if 6 < growth(g, i) {
                    return PbStr::new();
                }
                comment = name.clone()
                    + __content.string(7017)
                    + __content.string(7018);
                if ind == 45 {
                    phrase = s(__content.string(7019));
                }
                if ind == 60 {
                    rep(&mut comment, "great", __content.string(1398));
                }
                if ind == 63 {
                    if 11 < growth(g, 5) && (cast::<Ext, _>(g.base_interest_rate)) < sf(5.9f32) {
                        comment = comment
                            + __content.string(7020);
                    }
                }
                comment = comment + &phrase;
            }
            16 => {
                comment = s(__content.string(7021));
            }
            21 => {
                comment = s("The ")
                    + &name_lc
                    + __content.string(7022)
                    + __content.string(7023);
            }
            3 | 67 | 68 => {
                if Ext::from_f32(14.0f32) < proj {
                    return PbStr::new();
                }
                comment = s(__content.string(7024))
                    + __content.string(7025)
                    + &name
                    + __content.string(1037);
            }
            69 => {
                comment = s(__content.string(7026))
                    + __content.string(7027);
            }
            _ => {}
        }
        return comment;
    }
    if sf(1.8f32) < gdp || !(margin < cast::<f32, _>(sf(10.4f32))) {
        return comment;
    }
    // slow economy (<= 1.8 %), unprofitable industry
    match i {
        3 | 9..=13 | 15 | 18 | 19 | 20 | 23..=26 | 33 | 36 | 38..=40 | 49 | 51..=54 | 59 | 60 | 62 | 67 | 68 => {
            if sf(0.5f32) <= gdp {
                comment = s(__content.string(7028)) + &name + __content.string(1037);
            }
            if 0 < cast::<i32, _>(ind_q) % 3 {
                rep(&mut comment, "penaliz", "impact");
            }
            if margin < Ext::from_f32(0.0f32) && proj < Ext::from_f32(2.0f32) {
                rep(&mut comment, "strong", "severe");
            }
            if price_trend < 0 {
                if price_trend_direction(g, 1604, 2, 1) < 0 {
                    rep(&mut comment, "slow", __content.string(6900));
                }
            }
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                rep(&mut comment, __content.string(1059), "profits");
            }
        }
        5 | 6 => {
            phrase = s("housing");
            if ind == 6 {
                phrase = name_lc.clone();
            }
            if !(sf(9.2f32) < cast::<Ext, _>(g.base_interest_rate)) {
                if 5 < g.subprime_crisis_state {
                    comment = s(__content.string(7029))
                        + "in the "
                        + &phrase
                        + __content.string(7030)
                        + __content.string(7031);
                    if g.cur_quarter < 3 {
                        rep(&mut comment, "Growing", __content.string(7032));
                    }
                }
            } else {
                comment = s(__content.string(7033)) + &phrase + __content.string(7034);
            }
        }
        7 | 8 => {
            let r = rng.rnd();
            if r * sf(0.05f32) + sf(0.8f32) <= cast::<Ext, _>(g.oil_price) {
                if sf(0.5f32) <= gdp {
                    comment = s(__content.string(7035));
                }
            } else {
                phrase = PbStr::new();
                if (cast::<Ext, _>(g.oil_price)) < sf(0.55f32) && (margin < Ext::from_f32(5.0f32) || proj < Ext::from_f32(5.0f32)) {
                    phrase = s(__content.string(7036));
                }
                comment = s(__content.string(7037))
                    + &phrase
                    + __content.string(7038);
            }
            if !comment.is_empty() && ind == 8 {
                rep(&mut comment, "oil", __content.string(7039));
            }
        }
        14 => {
            comment = s(__content.string(7040));
        }
        16 => {
            // (the original's CASE 16, 59: 59 is already taken by the first CASE)
            if gdp < Ext::from_f64(1.0) {
                comment = s(__content.string(7041))
                    + __content.string(7042);
            }
        }
        22 => {
            comment = s(__content.string(7043))
                + __content.string(7044);
        }
        28 => {
            if -Ext::from_f64(2.0) <= gdp {
                if -1 < growth(g, i) {
                    comment = s("The ")
                        + &name_lc
                        + __content.string(7045)
                        + __content.string(7046)
                        + __content.string(7047);
                }
            }
        }
        30 => {
            if sf(0.5f32) < gdp {
                comment = s(__content.string(7048))
                    + __content.string(7049);
            }
        }
        _ if i < 41 || 48 < i => {
            if i == 55 && margin < Ext::from_f32(3.0f32) && proj < Ext::from_f32(0.0f32) {
                comment = s("The ")
                    + &name_lc
                    + __content.string(7050)
                    + __content.string(7051);
            }
        }
        _ => {
            // retail 41..48
            phrase = s(__content.string(7052));
            if gdp < -Ext::from_f64(1.0) {
                phrase = s(__content.string(1199));
            }
            comment = s(__content.string(7053))
                + &phrase
                + __content.string(7054);
            // (the original repeats `IF GdpGrowth < -1 THEN phrase = "recession"` here; dead store)
            if ind == 43 && gdp < sf(0.5f32) {
                comment = PbStr::new();
            }
            if ind == 47 {
                if growth(g, 47) < 0 {
                    comment = comment + __content.string(7055);
                }
            }
            let k = cast::<i32, _>(ind);
            match k {
                41 | 42 | 44 | 47 => {
                    extra = s("The ") + &name + __content.string(7056);
                    if gdp < sf(0.5f32) && k != 42 {
                        extra = PbStr::new();
                    }
                }
                43 | 46 => {
                    if gdp < Ext::from_f64(0.0) {
                        phrase = s(__content.string(7057));
                        if k == 46 {
                            phrase = s(__content.string(7058));
                        }
                        extra = phrase.clone()
                            + __content.string(7059)
                            + __content.string(7060)
                            + &lcase(phrase.as_bytes(), CaseMode::Ansi)
                            + __content.string(7061);
                    }
                }
                45 => {
                    extra = s(__content.string(7062));
                }
                48 => {
                    extra = s(__content.string(7063));
                }
                _ => {}
            }
            comment = comment + &extra;
        }
    }
    comment
}

/// Port of IndOutlook_CostCommentary (0x5878e4).
/// Sentence on cost or structural factors (overseas labour, regulation, taxes, litigation, input
/// prices, activists, a fading investor fad) for an industry whose margin is not clearly improving;
/// empty if the net margin exceeds the projected margin + 1 and is at least 10.4, or nothing applies.
/// Draws RND for some retail industries.
#[allow(clippy::too_many_arguments)]
pub fn ind_outlook_cost_commentary(
    g: &mut Globals,
    _ui: &mut dyn Ui,
    rng: &mut Rng,
    industry_id: i16,
    outlook_rating: &mut i32,
    net_margin: &mut f32,
    proj_margin: &mut f32,
    ind_name: &mut PbStr,
) -> PbStr {
    let __content = crate::content::current();
    let ind = industry_id;
    let i = cast::<i32, _>(ind);
    let rating = *outlook_rating;
    let margin = *net_margin;
    let proj = *proj_margin;
    let name = ind_name.clone();
    let name_lc = lower_name(ind, &name);
    let ind_q = cast::<i16, _>(cast::<i32, _>(ind) + g.cur_quarter);
    let q = cast::<i32, _>(ind_q);
    let oil = cast::<Ext, _>(g.oil_price);
    let gdp = cast::<Ext, _>(g.gdp_growth);
    let mut comment = PbStr::new();
    let mut phrase;

    // fld1; fadd proj; fcomp margin / fld 10.4; fcomp margin
    if !(cast::<Ext, _>(proj) + Ext::from_f64(1.0) >= cast::<Ext, _>(margin)) && !(sf(10.4f32) > cast::<Ext, _>(margin)) {
        return PbStr::new();
    }
    'sel: {
        match i {
            3 | 10 | 19 | 24 | 35 | 40 | 55 | 58 | 66 | 67 => {
                if margin < proj || margin < cast::<f32, _>(sf(10.4f32)) {
                    if (cast::<Ext, _>(margin)) < Ext::from_f64(19.0) - cast::<Ext, _>(q % 6) {
                        phrase = s("China");
                        if g.cur_year % 4 == 1 {
                            phrase = s(__content.string(7064));
                        }
                        if g.cur_year % 4 == 2 {
                            phrase = s(__content.string(5329));
                        }
                        if g.cur_year % 4 == 3 {
                            phrase = s(__content.string(7065));
                        }
                        comment = s(__content.string(7066))
                            + __content.string(7067)
                            + &phrase
                            + __content.string(7068);
                        if q % 2 == 1 {
                            rep(&mut comment, __content.string(7069), __content.string(7070));
                        }
                        if q % 4 == 1 {
                            rep(&mut comment, "unrest", "strife");
                        }
                        if q % 4 == 2 {
                            rep(&mut comment, "unrest", __content.string(7071));
                        }
                        if q % 4 == 3 {
                            rep(&mut comment, __content.string(7072), __content.string(7073));
                        }
                        if (i + g.cur_year + g.cur_quarter) % 2 == 1 {
                            rep(&mut comment, "labor", __content.string(7074));
                        }
                    }
                }
            }
            4 => {
                if sf(10.4f32) < cast::<Ext, _>(margin) || gdp < Ext::from_f64(3.0) {
                    comment = s(__content.string(7075))
                        + __content.string(7076)
                        + __content.string(7077);
                }
            }
            5 => {
                if (cast::<Ext, _>(margin)) < sf(10.4f32)
                    && gdp < Ext::from_f64(1.0)
                    && 2 < g.subprime_crisis_state
                    && g.subprime_crisis_state < 6
                {
                    comment = s(__content.string(7078))
                        + __content.string(7079)
                        + __content.string(7080)
                        + __content.string(7081);
                }
            }
            6 | 18 | 33 | 60 => {
                if margin < Ext::from_f32(16.0f32) {
                    comment = s(__content.string(7082));
                    if ind == 6 {
                        if growth(g, 5) < 0 && (cast::<Ext, _>(g.base_interest_rate)) < sf(9.2f32) {
                            comment = s("The ") + &name_lc + __content.string(7083);
                        }
                    }
                }
            }
            7 | 9 | 50 => {
                comment = s(__content.string(7084));
                if !(sf(1.1f32) < oil && ind == 7) {
                    if ind <= 7 {
                        comment = PbStr::new();
                    } else {
                        rep(&mut comment, __content.string(7085), __content.string(7086));
                        rep(&mut comment, __content.string(7087), __content.string(7088));
                    }
                }
                if Ext::from_f32(15.0f32) < margin {
                    comment = PbStr::new();
                }
            }
            8 => {
                if sf(1.1f32) < oil {
                    comment = s(__content.string(7089))
                        + &quoted(__content.string(7090))
                        + " ";
                }
            }
            11 => {
                comment = s(__content.string(7091))
                    + __content.string(7092);
            }
            12 => {
                if growth(g, i) < 2 {
                    comment = s(__content.string(7093))
                        + __content.string(7094);
                }
            }
            13 => {
                if 48 < g.corp_tax_rate && growth(g, i) < 0 {
                    comment = s(__content.string(7095))
                        + __content.string(7096);
                }
            }
            17 => {
                let grain = cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price);
                if sf(1.15f32) < oil && sf(12.7f32) < grain && !(sf(16.9f32) < grain) {
                    if price_trend_streak(g, 9) <= 0 {
                        if price_trend_streak(g, 10) <= 0 && grain <= Ext::from_f64(14.0) {
                            break 'sel;
                        }
                    }
                    comment = s(__content.string(7097))
                        + &name_lc
                        + __content.string(7098);
                }
            }
            20 | 51 => {
                comment = s(__content.string(7099))
                    + __content.string(7100);
                if ind == 51 {
                    rep(&mut comment, "Tough", "Strict");
                }
            }
            21 => {
                comment = s(__content.string(7101))
                    + __content.string(7102);
            }
            22 | 56 => {
                comment = s(__content.string(7103))
                    + &name_lc
                    + __content.string(7104)
                    + __content.string(7105)
                    + __content.string(7106);
                if ind == 56 {
                    rep(&mut comment, __content.string(7107), __content.string(7108));
                }
            }
            34 => {
                if rating == 5 || rating == 6 {
                    comment = s(__content.string(7109))
                        + __content.string(7110);
                }
            }
            41 | 42 | 44 | 45 | 46 | 48 | 57 => {
                if growth(g, i) < 5 {
                    phrase = s(__content.string(7111));
                    if q % 2 == 1 {
                        phrase = s(__content.string(7112));
                    }
                    comment = s(__content.string(7113))
                        + &phrase
                        + __content.string(7114);
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        rep(&mut comment, "limit", "slow");
                    }
                    if ind == 57 {
                        comment = remove(comment.as_bytes(), b" retail");
                        rep(&mut comment, __content.string(7115), __content.string(7116));
                    } else if q % 3 == 1 {
                        let bb = quoted("big box");
                        comment = replace(comment.as_bytes(), __content.bytes(7117), bb.as_bytes());
                    }
                }
            }
            63 => {
                if oil < sf(0.9f32) {
                    if growth(g, 7) + growth(g, 8) < 5 {
                        if price_pct_change_months(g, 6, 6) < 0 {
                            if growth(g, i) < 3 {
                                comment = s(__content.string(7118))
                                    + __content.string(7119)
                                    + __content.string(7120);
                            }
                        }
                    }
                }
            }
            65 => {
                if 4 < rating && rating < 7 && -Ext::from_f64(5.0) < gdp && gdp < Ext::from_f64(3.0) {
                    comment = s(__content.string(7121));
                }
            }
            69 if 6 < rating => {
                comment = s(__content.string(7122))
                    + __content.string(7123);
                if sf(1.18f32) < oil {
                    if 6 < price_trend_streak(g, 6) {
                        comment = comment
                            + __content.string(7124);
                    }
                }
                comment = comment + ". ";
            }
            _ => {}
        }
    }
    // LAB_00588b14
    if comment.is_empty() && i == g.fad_industry && g.fad_valuation_pct < 100 && g.fad_valuation_pct % 2 == 1 {
        comment = s(__content.string(7125))
            + __content.string(7126)
            + "in the "
            + &name_lc
            + __content.string(1037);
    }
    comment
}

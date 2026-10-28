//! Operating industries: the industry-specific cost/price paragraph (oil, metals, farm prices, interest
//! rates, the stock market, the economy …) of BuildIndustryOutlookText (bas view from
//! `caseVal = industryId; if (caseVal == 3 || 11 || 15 …` up to label LAB_0057f23a, 0x5752d2 .. 0x57f23a).
//!
//! Price series ids of the history helpers (from the machine code; the bas view drops them): 0 stock
//! index, 6 crude oil, 7 gold, 8 silver, 9 wheat, 10 corn, 1602 long-term rate, 1604 GDP growth. Every
//! `goto LAB_0057f23a` is a `return`.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_lines,
    clippy::nonminimal_bool,
    clippy::if_same_then_else
)]
use crate::pb::real::{Ext, cast};


use super::{Ctx, s, sf};
use crate::logic::futures::update_metals_price_base;
use crate::logic::report::number_to_words;
use crate::logic::valuation::{price_pct_change_months, price_trend_direction, price_trend_streak};
use crate::pb::numeric::str_num;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{CaseMode, instr, lcase, remove, replace, right, ucase};
use crate::pb::numeric::{int, to_long};
use crate::state::Globals;
use crate::ui::Ui;

/// `x = REPLACE$(x, a, b)`.
fn rep(x: &mut PbStr, a: &str, b: &str) {
    *x = replace(x.as_bytes(), a.as_bytes(), b.as_bytes());
}

/// `INSTR(1, x, a) > 0`.
fn has(x: &PbStr, a: &str) -> bool {
    instr(1, x.as_bytes(), a.as_bytes()) > 0
}

/// `NumberToWords(n, 0, pct)`.
fn ntw(n: i32, pct: i32) -> PbStr {
    number_to_words(n, 0, pct)
}

/// `STR$` of a LONG / EXT holding an integer.
fn str_l(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

fn igr(g: &Globals, i: i32) -> i32 {
    cast::<i32, _>(g.ind_growth_rate[i])
}

fn igf(g: &Globals, i: i32) -> i32 {
    cast::<i32, _>(g.ind_growth_forecast[i])
}

/// `CornPrice + WheatPrice` (x87, not stored).
fn grain(g: &Globals) -> Ext {
    cast::<Ext, _>(g.corn_price) + cast::<Ext, _>(g.wheat_price)
}

/// `SilverPrice - MetalsPriceBase * 0.01` (x87, not stored).
fn silver_x(g: &Globals) -> Ext {
    cast::<Ext, _>(g.silver_price) - cast::<Ext, _>(g.metals_price_base) * sf(0.01f32)
}

/// `CurQuarter * 0.05 * RND + add` (one RND draw).
fn rnd_q(g: &Globals, rng: &mut Rng, add: f32) -> Ext {
    let q = cast::<Ext, _>(g.cur_quarter) * sf(0.05f32);
    q * rng.rnd() + sf(add)
}

fn oil(g: &Globals) -> Ext {
    cast::<Ext, _>(g.oil_price)
}

fn gdp(g: &Globals) -> Ext {
    cast::<Ext, _>(g.gdp_growth)
}

/// The ", however" suffix logic shared by the grain price sentences of industry 15.
fn grain_however(c: &mut Ctx, v: i32) {
    let __content = crate::content::current();
    let up = ucase(c.body_text.as_bytes(), CaseMode::Ansi);
    let mut l816b = false;
    if instr(1, up.as_bytes(), __content.bytes(7127)) < 1 {
        if has(&c.body_text, __content.string(7128)) {
            l816b = true;
        } else if instr(1, lcase(c.body_text.as_bytes(), CaseMode::Ansi).as_bytes(), __content.bytes(7129)) > 0 {
            if instr(1, c.body_text.as_bytes(), __content.bytes(7130)) == 0 && 0 < v {
                c.body_text = c.body_text.clone() + __content.string(1387);
            }
        }
    } else {
        l816b = true;
    }
    if l816b && v < 0 {
        c.body_text = c.body_text.clone() + __content.string(1387);
    }
    c.body_text = c.body_text.clone() + ". ";
}

/// Part of BuildIndustryOutlookText (0x56f33b): the industry-specific paragraph (0x5752d2 .. 0x57f23a).
/// Leaves its text in `bodyText` (and scratch in `clauseText`/`paraText`); may draw RND and call
/// `UpdateMetalsPriceBase`.
pub(super) fn industry_detail(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    match ind {
        3 | 11 | 15 | 16 | 23 | 25 | 52 | 53 | 59 | 67 | 68 => {
            transport(g, rng, c);
        }
        4 | 21 => {}
        5 | 6 | 60 | 69 => interest_rates(g, rng, c),
        37 => brokerage(g, c),
        7 | 8 | 50 => {
            if energy(g, ui, rng, c) {
                metals(g, ui, rng, c);
            }
        }
        9 => {
            if !(gdp(g) <= cast::<Ext, _>(Ext::from_f64(3.5))) && !(4 < c.outlook_rating) {
                c.body_text = s(__content.string(7131))
                    + __content.string(7132);
            } else if gdp(g) < sf(0.8f32) && 6 < c.outlook_rating {
                c.body_text = s(__content.string(7133))
                    + __content.string(7134);
            }
        }
        12 => steel(g, c),
        10 | 13 | 20 | 24 | 33 | 35 | 36 | 38 | 40 | 42 | 45 | 47 | 49 | 54 | 58 | 63 => cyclical(g, rng, c),
        14 => aerospace(g, c),
        17 | 22 | 27 | 28 | 29 | 31 | 32 | 34 | 46 | 55 | 56 | 65 => consumer(g, rng, c),
        18 => tires(g, c),
        41 | 43 | 44 | 48 => retail(g, c),
        _ => others(g, ui, c),
    }
}

// ------------------------------------------------------------------------------- transport (0x5752d2)

/// Industries 3, 11, 15, 16, 23, 25, 52, 53, 59, 67, 68: fuel prices; then grain prices for 15.
fn transport(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    c.price_streak = price_trend_streak(g, 6);
    if c.price_streak.abs() < 5 {
        c.price_streak = 0;
    }
    c.pct_change = price_pct_change_months(g, 6, 12);
    if c.pct_change.abs() < 7 {
        c.pct_change = 0;
    }
    let x = rnd_q(g, rng, 0.7f32);
    'oil: {
        if x <= oil(g) {
            if sf(0.9f32) > oil(g) && price_trend_streak(g, 6) < 0 {
                c.body_text = s(__content.string(7135))
                    + &c.ind_name
                    + __content.string(7136)
                    + __content.string(7137);
                if ind == 16 || ind == 52 {
                    rep(&mut c.body_text, "fuel", __content.string(7138));
                }
                if ind == 3 || 52 < ind {
                    c.clause_text = s("sales");
                    if ind == 3 || ind == 67 {
                        c.clause_text = s("auto ") + &c.clause_text;
                    }
                    if ind == 68 {
                        c.clause_text = s(__content.string(3518));
                    }
                    if 3 < c.outlook_rating && c.outlook_rating < 7 {
                        c.clause_text = s("profits");
                    }
                    if ind == 59 {
                        c.clause_text = s(__content.string(7139));
                    }
                    c.clause_text = s(__content.string(7140)) + &c.clause_text;
                    if c.outlook_rating < 3 {
                        c.clause_text = s(__content.string(7141));
                    }
                    let cl = c.clause_text.to_display();
                    rep(&mut c.body_text, __content.string(7142), &cl);
                } else if ind == 15 {
                    rep(&mut c.body_text, "costs", __content.string(7143));
                }
                if 6 < c.outlook_rating && has(&c.head_text, __content.string(7144)) {
                    rep(&mut c.body_text, "Lower", __content.string(7145));
                }
                break 'oil;
            }
            if sf(1.03f32) < oil(g) && sf(1.1f32) > oil(g) && 0 < price_trend_streak(g, 6) {
                c.clause_text = s("ising");
                if c.price_streak < 1 {
                    c.clause_text = s("R") + &c.clause_text;
                } else {
                    c.clause_text = s(__content.string(7146)) + &c.clause_text;
                }
                c.body_text = c.clause_text.clone()
                    + __content.string(7147)
                    + &c.ind_name
                    + __content.string(7148)
                    + __content.string(7149);
                if 0 < c.price_streak {
                    rep(&mut c.body_text, "strong", "steady");
                }
                if ind == 16 || ind == 52 {
                    rep(&mut c.body_text, "fuel", __content.string(7138));
                }
                if ind == 3 || 52 < ind {
                    if c.outlook_rating < 7 {
                        c.clause_text = s(__content.string(7150));
                    } else {
                        c.clause_text = s(__content.string(7151));
                    }
                    if !c.clause_text.is_empty() {
                        let cl = c.clause_text.to_display();
                        rep(&mut c.body_text, __content.string(7152), &cl);
                    }
                }
                break 'oil;
            }
            let x2 = rnd_q(g, rng, 1.1f32);
            if x2 > oil(g) {
                if 4 < c.outlook_rating && sf(2.2f32) > gdp(g) && (ind == 67 || ind == 68) {
                    c.clause_text = s("Europe");
                    if ind == 68 {
                        c.clause_text = s(__content.string(7153));
                    }
                    c.body_text = s(__content.string(7154))
                        + &c.clause_text
                        + __content.string(7155);
                    if ind == 68 {
                        rep(&mut c.body_text, __content.string(7156), "Asian");
                    }
                }
                break 'oil;
            }
            high_fuel(g, rng, c);
        } else {
            low_fuel(g, rng, c);
        }
    }
    if ind == 15 {
        farm(g, rng, c);
    }
}

/// High fuel prices (0x5763a4 .. 0x577067).
fn high_fuel(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    c.clause_text = s(__content.string(7157));
    if ind == 15 {
        c.clause_text = s("Farm");
    }
    if ind == 16 {
        c.clause_text = s(__content.string(7158));
    }
    if ind == 52 {
        c.clause_text = s(__content.string(7159));
    }
    if ind == 23 {
        c.clause_text = s(__content.string(7160));
    }
    if ind == 25 {
        c.clause_text = s(__content.string(7161));
    }
    if ind == 53 {
        c.clause_text = s(__content.string(7162));
    }
    if c.outlook_rating < 7 {
        if c.outlook_rating < 5 {
            c.body_text = c.clause_text.clone() + __content.string(7163);
            if !(Ext::from_f64(15.0) > cast::<Ext, _>(c.proj_margin)) {
                let r = rng.rnd();
                if Ext::from_f64(2.0) * r > Ext::from_f64(1.0) {
                    rep(&mut c.body_text, __content.string(7164), __content.string(7165));
                }
            } else {
                rep(&mut c.body_text, __content.string(7166), __content.string(7167));
            }
            if ind == 11 && 4 < igr(g, 11) {
                c.body_text = c.body_text.clone()
                    + __content.string(7168)
                    + __content.string(7169)
                    + __content.string(7170)
                    + __content.string(7171);
            }
            if ind == 3 || 66 < ind {
                c.body_text = s(__content.string(7172));
            }
            if ind == 3 {
                rep(&mut c.body_text, "Asi", __content.string(7173));
            }
            if ind == 67 {
                rep(&mut c.body_text, "Asi", "Europe");
            }
            if ind == 53 {
                c.body_text = c.body_text.clone()
                    + __content.string(7174)
                    + __content.string(7175);
            }
        } else {
            c.body_text = c.clause_text.clone() + __content.string(7176);
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                rep(&mut c.body_text, __content.string(7177), "adverse");
            }
            if ind == 3 || 66 < ind {
                c.body_text = s(__content.string(7178));
            }
            if ind == 3 && Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                rep(&mut c.body_text, __content.string(7179), __content.string(7180));
            }
        }
    } else {
        c.body_text = c.clause_text.clone() + __content.string(7181);
        if ind == 16 || ind == 52 {
            rep(&mut c.body_text, __content.string(7182), __content.string(7138));
            if ind == 16 {
                c.body_text = c.body_text.clone() + __content.string(7183);
            }
        }
        if ind == 11 || ind == 53 {
            rep(&mut c.body_text, __content.string(7182), "fuel");
        }
        if ind == 11 && has(&c.head_text, __content.string(7184)) {
            rep(&mut c.body_text, __content.string(7185), __content.string(7186));
        }
        if ind == 25 {
            rep(&mut c.body_text, __content.string(7182), "diesel");
        }
        if c.outlook_rating < 9 {
            rep(&mut c.body_text, "crimped", __content.string(7187));
        } else {
            rep(&mut c.body_text, "crimp", "crush");
        }
        if cast::<i32, _>(c.ind_plus_q) % 4 != 1 && has(&c.body_text, "crushed") {
            rep(&mut c.body_text, "crushed", __content.string(7188));
        }
        if sf(1.5f32) < gdp(g) || has(&c.head_text, __content.string(7189)) {
            c.body_text = c.body_text.clone() + ". ";
        } else {
            c.body_text = c.body_text.clone() + __content.string(7190);
        }
        if ind == 3 || 66 < ind {
            c.clause_text = s("H");
            if sf(1.8f32) < oil(g) {
                c.clause_text = s("Sky-h");
            }
            c.body_text = c.clause_text.clone() + __content.string(7191);
            if ind == 3 {
                c.body_text = c.body_text.clone()
                    + __content.string(7192)
                    + __content.string(7193);
                rep(&mut c.body_text, __content.string(7194), __content.string(7195));
            } else if ind == 67 {
                c.body_text = c.body_text.clone() + __content.string(7196);
            } else {
                c.body_text = c.body_text.clone()
                    + __content.string(7197);
            }
            if g.peak_oil_crisis == 1 && sf(1.9f32) < oil(g) {
                c.body_text = c.body_text.clone() + __content.string(7198);
                if oil(g) <= sf(2.5f32) {
                    c.body_text = c.body_text.clone()
                        + __content.string(7199);
                } else {
                    c.body_text = c.body_text.clone()
                        + __content.string(7200);
                }
            }
        }
        if ind == 23 && Ext::from_f64(0.0) > gdp(g) {
            c.body_text = c.body_text.clone()
                + __content.string(7201)
                + __content.string(7202)
                + __content.string(7203);
        }
    }
    if ind == 59 {
        c.body_text = s(__content.string(7204))
            + __content.string(7205);
    }
    if c.price_streak < 1 {
        if 0 < c.pct_change {
            c.clause_text = ntw(c.pct_change, 1) + " since " + &g.month_name[c.month_num] + __content.string(7206);
        }
    } else {
        c.clause_text = s(__content.string(7207)) + &ntw(c.price_streak, 0) + __content.string(1095);
    }
    if 0 < c.price_streak || 0 < c.pct_change {
        c.body_text = c.body_text.clone() + __content.string(7208) + &c.clause_text;
    }
    if 12 < c.pct_change {
        if g.cur_quarter < 3 {
            rep(&mut c.body_text, "risen", "climbed");
        } else {
            rep(&mut c.body_text, "risen", "shot up");
        }
    }
}

/// Low fuel prices (0x575906 .. 0x575d69).
fn low_fuel(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    if c.outlook_rating < 7 {
        c.body_text = s(__content.string(7209));
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
            rep(&mut c.body_text, "help", "aid");
        }
        if sf(0.83f32) < oil(g) && c.price_streak < 0 {
            rep(&mut c.body_text, "low", "falling");
        }
        if ind == 3 || 66 < ind {
            c.body_text = s(__content.string(7210))
                + __content.string(7211);
            if ind == 67 {
                rep(&mut c.body_text, __content.string(7212), "petrol");
            }
        }
        if ind == 23 && sf(0.2f32) > gdp(g) && c.outlook_rating < 5 {
            c.body_text = c.body_text.clone()
                + __content.string(7213)
                + __content.string(7214);
        }
        if ind == 53 {
            c.body_text = s(__content.string(7215));
        }
    } else {
        c.body_text = s(__content.string(7216))
            + __content.string(7217);
        if ind == 3 || 66 < ind {
            c.body_text = s(__content.string(7218));
        }
        if ind == 25 && Ext::from_f64(0.0) > gdp(g) {
            c.body_text = c.body_text.clone()
                + __content.string(7219)
                + __content.string(7220);
        }
        if ind == 23 && sf(0.4f32) > gdp(g) {
            c.body_text = c.body_text.clone()
                + __content.string(7221)
                + __content.string(7222);
        }
    }
    if ind == 59 {
        c.body_text = s(__content.string(7223));
    }
    if c.price_streak < 0 {
        c.clause_text = s(" for") + &ntw(c.price_streak, 0) + __content.string(7224);
    } else if c.pct_change < 0 {
        c.clause_text = ntw(c.pct_change, 1)
            + " from "
            + &g.month_name[c.month_num]
            + ","
            + &str_l(cast::<Ext, _>(g.cur_year - 1))
            + ". ";
    }
    if c.price_streak < 0 || c.pct_change < 0 {
        c.body_text = c.body_text.clone() + __content.string(7225) + &c.clause_text;
    }
}

/// Industry 15: grain prices (LAB_00577175).
fn farm(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    c.clause_text = PbStr::new();
    c.trend_val = price_trend_streak(g, 10);
    if c.trend_val.abs() < 4 {
        c.trend_val = 0;
    }
    c.price_streak = price_trend_streak(g, 9);
    if c.price_streak.abs() < 4 {
        c.price_streak = 0;
    }
    c.price_trend = price_trend_direction(g, 10, 3, 1);
    if c.body_text.len() > 100 {
        c.body_text = c.body_text.clone() + "\r\n" + "\r\n";
    }
    let gp = grain(g);
    if gp <= Ext::from_f64(14.0) || 4 < c.outlook_rating || cast::<Ext, _>(c.net_margin) <= Ext::from_f64(13.0) {
        if c.outlook_rating < 5 {
            if -1 < c.price_streak || -1 < c.price_trend || Ext::from_f64(9.0) <= gp {
                if gp < sf(8.1f32) {
                    c.clause_text = s("low");
                }
            } else {
                c.clause_text = s("falling");
            }
            if !c.clause_text.is_empty() {
                c.body_text = c.body_text.clone()
                    + __content.string(2961)
                    + &c.clause_text
                    + __content.string(7226)
                    + &c.ind_name_lc
                    + __content.string(7227);
            }
        } else if c.outlook_rating < 7 || sf(8.4f32) <= gp {
            if c.outlook_rating == 5 || c.outlook_rating == 6 {
                if sf(13.5f32) < gp {
                    c.clause_text = s(__content.string(7228));
                }
                if sf(1.3f32) < oil(g) {
                    rep(&mut c.clause_text, "High", __content.string(7229));
                }
                if gp < sf(8.6f32) {
                    c.clause_text = s(__content.string(7230));
                    if oil(g) <= sf(0.75f32) {
                        rep(&mut c.clause_text, "Low", __content.string(7231));
                    }
                }
                if !c.clause_text.is_empty() {
                    c.clause_text = c.clause_text.clone() + __content.string(7232);
                    c.body_text = c.body_text.clone() + &c.clause_text;
                }
            }
        } else {
            c.body_text = s(__content.string(7233));
        }
    } else {
        c.body_text =
            c.body_text.clone() + __content.string(7234);
        if 0 < c.price_streak && 0 < c.price_trend {
            rep(&mut c.body_text, "High", __content.string(7235));
        }
    }
    c.clause_text = PbStr::new();
    if c.outlook_rating < 5 && 0 < c.trend_val {
        c.clause_text = s("risen");
    }
    if c.trend_val == 0 {
        if c.price_streak != 0 {
            c.clause_text = s("climbed");
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                c.clause_text = s("gone up");
            }
            if c.price_streak < 0 {
                c.clause_text = s("fallen");
            }
            if !c.body_text.is_empty() {
                c.body_text = c.body_text.clone()
                    + __content.string(7236)
                    + &c.clause_text
                    + __content.string(7237)
                    + &ntw(c.price_streak, 0)
                    + " months";
            }
            if c.price_streak < 1 || c.outlook_rating < 7 || grain(g) <= Ext::from_f64(14.0) {
                if c.price_streak < 0 && instr(1, c.body_text.as_bytes(), b"However") == 0 {
                    if sf(13.5f32) < grain(g) && c.outlook_rating < 5 && Ext::from_f64(13.0) < cast::<Ext, _>(c.net_margin) {
                        rep(&mut c.body_text, "Wheat", __content.string(7238));
                    }
                    return;
                }
                if !c.body_text.is_empty() && !(right(c.body_text.as_bytes(), 2) == ". ") {
                    c.body_text = c.body_text.clone() + ". ";
                }
            } else {
                c.body_text = c.body_text.clone()
                    + __content.string(7239)
                    + __content.string(7240);
                if sf(1.3f32) < oil(g) && instr(1, c.body_text.as_bytes(), b"However") == 0 {
                    rep(&mut c.body_text, "Wheat", __content.string(7238));
                }
                c.body_text = c.body_text.clone() + ". ";
            }
            return;
        }
        if (cast::<Ext, _>(price_pct_change_months(g, 9, 6))).abs() <= Ext::from_f64(10.0) {
            if (cast::<Ext, _>(price_pct_change_months(g, 10, 6))).abs() > Ext::from_f64(10.0) {
                c.price_trend = price_pct_change_months(g, 10, 6);
                c.clause_text = s("in");
                if c.price_trend < 0 {
                    c.clause_text = s("de");
                }
                if !c.body_text.is_empty() {
                    c.body_text = c.body_text.clone()
                        + __content.string(7241)
                        + &c.clause_text
                        + __content.string(1689)
                        + &ntw(c.price_trend, 1);
                    let v = c.price_trend;
                    grain_however(c, v);
                }
            }
            return;
        }
        c.price_streak = price_pct_change_months(g, 9, 6);
        c.clause_text = s("in");
        if c.price_streak < 0 {
            c.clause_text = s("de");
        }
        if c.body_text.is_empty() {
            return;
        }
        c.body_text = c.body_text.clone()
            + __content.string(7242)
            + &c.clause_text
            + __content.string(1689)
            + &ntw(c.price_streak, 1);
        let v = c.price_streak;
        grain_however(c, v);
    } else {
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
            c.clause_text = s(__content.string(7243));
        }
        if c.trend_val < 0 {
            c.clause_text = s("dropped");
        }
        if !c.clause_text.is_empty() && !c.body_text.is_empty() {
            c.body_text = c.body_text.clone()
                + __content.string(7244)
                + &c.clause_text
                + __content.string(1115)
                + &ntw(c.trend_val, 0)
                + " months";
            let v = c.trend_val;
            grain_however(c, v);
        }
    }
}

// ------------------------------------------------------------------- interest rates (5, 6, 60, 69)

fn interest_rates(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    let base = cast::<Ext, _>(g.base_interest_rate);
    let target = cast::<Ext, _>(g.interest_rate_target);
    c.price_trend = price_trend_streak(g, 1602);
    if c.price_trend.abs() < 4 {
        c.price_trend = 0;
    }
    if base <= sf(8.7f32) || target <= Ext::from_f64(8.5) || g.monetary_policy < 1 {
        if base <= cast::<Ext, _>(g.cur_quarter) * Ext::from_f64(0.5) + sf(8.7f32) || 6 < ind {
            if sf(6.8f32) <= base || Ext::from_f64(7.0) <= target {
                if ind != 69 || oil(g) <= sf(1.2f32) || c.outlook_rating < 5 || 6 < c.outlook_rating {
                    if ind == 60 && 6 < igr(g, 60) && sf(4.8f32) < gdp(g) {
                        c.body_text = s(
                            __content.string(7245),
                        );
                    }
                } else {
                    c.body_text = s(__content.string(7246));
                }
            } else {
                if c.outlook_rating < 7 || g.monetary_policy != 0 {
                    if c.outlook_rating < 7 || sf(5.9f32) <= base {
                        if c.outlook_rating < 5 {
                            c.body_text = s(__content.string(7247));
                            c.clause_text = PbStr::new();
                            if (ind == 6 || ind == 60) && 10 < igr(g, 5) {
                                c.clause_text = s(__content.string(7248));
                            }
                            if ind == 69 && oil(g) < sf(0.8f32) {
                                c.clause_text = s(__content.string(7249));
                            }
                            c.body_text = c.body_text.clone() + &c.clause_text + ". ";
                        } else {
                            c.body_text = s(__content.string(7250));
                        }
                    } else {
                        if ind == 69 && oil(g) < sf(0.83f32) {
                            c.clause_text = s(__content.string(7251));
                        }
                        c.body_text = s(__content.string(7252))
                            + &c.clause_text
                            + __content.string(7253)
                            + __content.string(7254);
                    }
                } else {
                    c.body_text = s(__content.string(7255))
                        + &g.central_bank_name
                        + ". ";
                }
                if c.price_trend < 0 {
                    c.body_text = c.body_text.clone()
                        + __content.string(7256)
                        + &ntw(c.price_trend, 0)
                        + __content.string(1095);
                }
            }
        } else {
            c.clause_text = s(". ");
            if c.outlook_rating < 5 {
                c.body_text = s(
                    __content.string(7257),
                );
                if 6 < igr(g, 5) {
                    c.clause_text = s(__content.string(7258));
                }
            } else {
                c.body_text = s(__content.string(7259));
                if g.monetary_policy == 0 {
                    c.clause_text = s(__content.string(7260));
                }
            }
            c.body_text = c.body_text.clone() + &c.clause_text;
            if ind == 6 {
                c.body_text = s(__content.string(7261))
                    + __content.string(7262);
            }
        }
    } else {
        if c.outlook_rating < 7 {
            if c.outlook_rating < 5 {
                c.body_text = s(__content.string(7263));
                if ind == 69 && sf(1.2f32) < oil(g) {
                    c.body_text = c.body_text.clone() + __content.string(7264);
                }
                c.body_text = c.body_text.clone() + ". ";
            } else {
                c.body_text = s(__content.string(7265));
                let x = rnd_q(g, rng, 1.15f32);
                if x < oil(g) && ind == 69 {
                    c.body_text = s(__content.string(7266))
                        + __content.string(7267);
                }
            }
        } else {
            c.body_text = s(__content.string(7268));
            if 2 < g.cur_quarter {
                rep(&mut c.body_text, __content.string(7269), __content.string(7270));
            }
            if g.monetary_policy == 2 {
                c.body_text = c.body_text.clone() + __content.string(7271) + &g.central_bank_name + __content.string(7272);
            } else {
                c.body_text = c.body_text.clone() + ". ";
            }
        }
        if 0 < c.price_trend {
            c.body_text = c.body_text.clone()
                + __content.string(7273)
                + &ntw(c.price_trend, 0)
                + __content.string(1095);
        }
    }
}

// ------------------------------------------------------------------------------- brokerage (37)

fn brokerage(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    c.growth_trend = 0;
    c.para_text = PbStr::new();
    if c.outlook_rating < 7 {
        if c.outlook_rating < 5 {
            if g.base_interest_rate < Ext::from_f32(6.8f32) && g.interest_rate_target < Ext::from_f32(6.5f32) && g.monetary_policy < 1 {
                c.body_text =
                    s(__content.string(7274));
            }
            c.trend_val = price_trend_streak(g, 0);
            if 5 < c.trend_val {
                c.growth_trend = 1;
            }
            c.price_streak = price_pct_change_months(g, 0, 12);
            if 10 < c.price_streak {
                c.growth_trend += 2;
            }
            if c.growth_trend == 1 || c.growth_trend == 3 {
                c.para_text = s(__content.string(7275)) + &ntw(c.trend_val, 0) + __content.string(1277);
            }
            if c.growth_trend == 2 {
                c.para_text = s(__content.string(7276)) + &ntw(c.price_streak, 1) + __content.string(7277);
            }
            if c.para_text.is_empty() {
                if c.trend_val < -5 || c.price_streak < -8 {
                    if c.price_streak < -8 {
                        c.para_text = ntw(c.price_streak, 1) + __content.string(7278);
                    }
                    if c.trend_val < -5 {
                        c.para_text = ntw(c.trend_val, 0) + __content.string(1084);
                    }
                    c.body_text = c.body_text.clone()
                        + __content.string(7279)
                        + &c.para_text;
                    if 2 < g.cur_quarter {
                        rep(&mut c.body_text, __content.string(1057), "fallen");
                    }
                }
            } else {
                c.body_text = c.body_text.clone()
                    + __content.string(7280)
                    + &c.para_text;
                if 23 < c.price_streak && 5 < c.trend_val {
                    c.body_text = c.body_text.clone()
                        + __content.string(7281)
                        + __content.string(7282);
                }
            }
        } else {
            c.trend_val = price_trend_streak(g, 0);
            if 6 < c.trend_val.abs() {
                c.para_text = s("up");
                c.clause_text = s("gaining");
                if c.trend_val < 0 {
                    c.para_text = s("down");
                    c.clause_text = s("sinking");
                }
                c.body_text = s(__content.string(7283))
                    + &c.para_text
                    + __content.string(7284)
                    + &c.clause_text
                    + " for"
                    + &ntw(c.trend_val, 0)
                    + __content.string(7285);
            }
        }
    } else {
        if sf(8.7f32) < cast::<Ext, _>(g.base_interest_rate) && sf(9.5f32) < cast::<Ext, _>(g.interest_rate_target) && 0 < g.monetary_policy {
            c.body_text = s(__content.string(7268));
            if g.monetary_policy == 2 {
                c.body_text = c.body_text.clone() + __content.string(7271) + &g.central_bank_name + __content.string(7272);
            } else {
                c.body_text = c.body_text.clone() + ". ";
            }
        }
        if price_pct_change_months(g, 0, 36) < -15 {
            c.growth_trend = 4;
        }
        if price_pct_change_months(g, 0, 24) < -10 {
            c.growth_trend += 2;
        }
        if price_pct_change_months(g, 0, 12) < -5 && c.growth_trend < 6 {
            c.growth_trend += 1;
        }
        if price_trend_direction(g, 0, 36, 12) < 0 {
            c.growth_trend = 7;
        }
        if c.growth_trend < 1 {
            c.trend_val = price_trend_streak(g, 0);
            if 7 < c.trend_val {
                c.body_text = c.body_text.clone()
                    + __content.string(7286)
                    + __content.string(7287)
                    + &ntw(c.trend_val, 0)
                    + __content.string(1095);
            }
        } else {
            if c.body_text.is_empty() {
                c.clause_text = s("Most");
            } else {
                c.clause_text = s(__content.string(7288));
            }
            c.body_text = c.body_text.clone()
                + &c.clause_text
                + __content.string(7289)
                + __content.string(7290);
            match c.growth_trend {
                1 => {
                    let v = (cast::<Ext, _>(price_pct_change_months(g, 0, 12))).abs();
                    c.para_text = s(__content.string(7291)) + &str_l(v) + "%";
                }
                2 | 3 => c.para_text = s(__content.string(7292)),
                4..=6 => c.para_text = s(__content.string(7293)),
                7 => c.para_text = s(__content.string(7294)),
                _ => {}
            }
            c.body_text = c.body_text.clone() + &c.para_text + ". ";
        }
    }
}

// ------------------------------------------------------------------------- energy (7, 8, 50)

/// Oil price paragraph of industries 7, 8 and 50; returns true for industry 50 (LAB_0057a598 → metals).
fn energy(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> bool {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    c.trend_val = price_trend_direction(g, 6, 2, 1);
    c.price_streak = price_trend_direction(g, 6, 12, 12);
    c.price_trend = price_trend_streak(g, 6);
    c.pct_change = price_pct_change_months(g, 6, 12);
    if c.pct_change.abs() < 10 {
        c.pct_change = 0;
    }
    c.trend_dir = 0;
    if c.price_streak + c.trend_val == 0 {
        if c.price_trend != 0 {
            if c.price_trend < 1 {
                c.trend_dir = -1;
            } else {
                c.trend_dir = 1;
            }
        }
    } else if c.price_streak + c.trend_val < 1 {
        c.trend_dir = -1;
    } else {
        c.trend_dir = 1;
    }
    let x = rnd_q(g, rng, 1.15f32);
    if oil(g) <= x {
        let x2 = rnd_q(g, rng, 0.65f32);
        if x2 <= oil(g) {
            if c.pct_change != 0 && ind < 9 {
                c.para_text = s(__content.string(1112));
                if c.pct_change < 0 {
                    c.para_text = s(__content.string(7295));
                }
                c.body_text = s(__content.string(7296))
                    + &c.para_text
                    + " by"
                    + &ntw(c.pct_change, 1)
                    + ". ";
            }
        } else {
            'a0d7: {
                if c.outlook_rating < 7 {
                    if c.outlook_rating < 5 {
                        if ind < 9 {
                            c.body_text = s(__content.string(7297))
                                + &c.ind_name_lc
                                + __content.string(7298)
                                + __content.string(7299);
                            if !has(&c.head_text, "OPEC") {
                                if ind == 7 {
                                    c.body_text = c.body_text.clone()
                                        + __content.string(7300)
                                        + __content.string(7301);
                                }
                                if ind == 8 {
                                    c.body_text = c.body_text.clone()
                                        + __content.string(7302)
                                        + __content.string(7303);
                                }
                            }
                        }
                    } else {
                        c.body_text = s(__content.string(7304));
                        if price_trend_streak(g, 6) < 0 {
                            rep(&mut c.body_text, "Low", "Falling");
                        }
                        if ind == 50 {
                            c.body_text = PbStr::new();
                        }
                    }
                } else {
                    c.body_text = s(__content.string(7305))
                        + __content.string(7306);
                    if price_trend_streak(g, 6) < 0 {
                        rep(&mut c.body_text, "Low", "Falling");
                    }
                    if ind == 50 {
                        let r = update_metals_price_base(g, ui);
                        if r < Ext::from_f64(1050.0) && silver_x(g) < Ext::from_f64(17.0) {
                            c.body_text = s(__content.string(7307));
                            break 'a0d7;
                        }
                    }
                    if ind == 50 {
                        c.body_text = PbStr::new();
                    }
                }
            }
            // LAB_0057a0d7
            if c.trend_dir == 1 && !c.body_text.is_empty() {
                if 0 < c.price_streak {
                    c.clause_text = s(" year");
                }
                if 0 < c.trend_val {
                    c.clause_text = s(__content.string(7308));
                }
                if 0 < c.price_trend {
                    c.clause_text = ntw(c.price_trend, 0) + __content.string(7309);
                }
                if c.pct_change < 1 || c.trend_val != 0 || c.price_trend != 0 {
                    if has(&c.body_text, __content.string(7310)) {
                        c.clause_text = c.clause_text.clone() + __content.string(1387);
                    }
                } else {
                    c.clause_text = s(__content.string(7311)) + &ntw(c.pct_change, 1);
                }
                c.body_text =
                    c.body_text.clone() + __content.string(7312) + &c.clause_text + ". ";
                if ind == 50 {
                    c.body_text = PbStr::new();
                }
                return ind == 50;
            }
            if c.trend_dir == -1 && !c.body_text.is_empty() {
                if c.price_streak < 0 {
                    c.clause_text = s(" year. ");
                }
                if c.trend_val < 0 {
                    c.clause_text = s(__content.string(7313));
                }
                if c.price_trend < 0 {
                    c.clause_text = ntw(c.price_trend, 0) + __content.string(7224);
                }
                if c.pct_change < 0 && c.trend_val == 0 && c.price_trend == 0 {
                    c.clause_text = s(__content.string(7314)) + &ntw(c.pct_change, 1) + " drop. ";
                }
                c.body_text =
                    c.body_text.clone() + __content.string(7315) + &c.clause_text;
                if ind == 50 {
                    c.body_text = PbStr::new();
                }
            }
        }
    } else {
        if c.outlook_rating < 7 {
            if c.outlook_rating < 5 {
                c.body_text = s(__content.string(7316));
                if ind == 8 {
                    c.body_text = c.body_text.clone()
                        + __content.string(7317);
                } else {
                    c.body_text = c.body_text.clone() + __content.string(7318);
                }
                if ind == 50 {
                    let r = update_metals_price_base(g, ui);
                    if r <= Ext::from_f64(1700.0) || silver_x(g) <= Ext::from_f64(22.0) {
                        c.body_text = PbStr::new();
                    } else {
                        c.body_text = s(__content.string(7319));
                    }
                }
            } else {
                c.body_text = s(__content.string(7320));
                if c.trend_dir == 1 {
                    rep(&mut c.body_text, "High", "Rising");
                }
                if ind == 50 {
                    let r = update_metals_price_base(g, ui);
                    if r <= Ext::from_f64(1700.0) || silver_x(g) <= Ext::from_f64(22.0) {
                        c.body_text = PbStr::new();
                    } else {
                        c.body_text = s(
                            __content.string(7321),
                        );
                    }
                }
            }
        } else if ind < 9 {
            c.body_text = s(__content.string(7322))
                + __content.string(7323);
            if int(cast::<Ext, _>(c.cur_slack_pct)) < int(cast::<Ext, _>(c.proj_slack_pct)) && c.trend_dir == 1 {
                c.body_text = c.body_text.clone() + __content.string(7324);
            }
            c.body_text = c.body_text.clone() + ". ";
        }
        if c.trend_dir == 1 && !c.body_text.is_empty() {
            c.para_text = PbStr::new();
            if 0 < c.price_streak {
                c.clause_text = s(" year. ");
            }
            if 0 < c.trend_val {
                c.clause_text = s(__content.string(7313));
            }
            if 0 < c.price_trend {
                c.clause_text = ntw(c.price_trend, 0) + __content.string(1277);
            }
            if 0 < c.pct_change {
                if c.trend_val == 0 && c.price_trend == 0 {
                    c.clause_text = s(__content.string(7325)) + &ntw(c.pct_change, 1) + __content.string(7326);
                } else {
                    c.para_text = str_l(cast::<Ext, _>(c.pct_change)) + "%";
                }
            }
            c.body_text =
                c.body_text.clone() + __content.string(7327) + &c.para_text + __content.string(1374) + &c.clause_text;
            if 40 < c.pct_change {
                rep(&mut c.body_text, __content.string(1080), __content.string(7328));
            }
            return ind == 50;
        }
        if c.trend_dir == -1 && !c.body_text.is_empty() {
            if c.price_streak < 0 {
                c.clause_text = s(" year. ");
            }
            if c.trend_val < 0 {
                c.clause_text = s(__content.string(7313));
            }
            if c.price_trend < 0 {
                c.clause_text = ntw(c.price_trend, 0) + __content.string(7329);
            }
            if c.pct_change < 0 && c.trend_val == 0 && c.price_trend == 0 {
                c.clause_text = s(__content.string(7330)) + &ntw(c.pct_change, 1) + __content.string(7331);
            }
            c.body_text =
                c.body_text.clone() + __content.string(7332) + &c.clause_text;
        }
    }
    ind == 50
}

/// Industry 50, precious metals (LAB_0057a598 .. 0x57b818).
fn metals(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    c.price_streak = price_trend_streak(g, 7);
    c.pct_change_a = price_pct_change_months(g, 7, 12);
    c.price_trend = price_trend_streak(g, 8);
    c.pct_change_b = price_pct_change_months(g, 8, 12);
    'ae5e: {
        let mut to_a9b1 = gdp(g) <= sf(3.8f32) || c.outlook_rating < 7;
        if !to_a9b1 {
            let r = update_metals_price_base(g, ui);
            if Ext::from_f64(1150.0) <= r || Ext::from_f64(16.0) <= silver_x(g) {
                to_a9b1 = true;
            } else {
                c.body_text = c.body_text.clone()
                    + __content.string(7333)
                    + __content.string(7334);
                c.recovery_text = s(__content.string(7335));
                if Ext::from_f64(2.0) * rng.rnd() == Ext::from_f64(1.0) {
                    c.recovery_text = s(__content.string(7336));
                }
                if !(cast::<Ext, _>(g.gold_target_price) <= cast::<Ext, _>(g.gold_price) * sf(1.15f32)) && 0 < price_pct_change_months(g, 7, 2)
                {
                    c.body_text = c.body_text.clone() + __content.string(7337) + &c.recovery_text;
                    break 'ae5e;
                }
                // LAB_0057a7ce
                if cast::<Ext, _>(g.silver_price) * sf(1.17f32) < cast::<Ext, _>(g.silver_target_price)
                    && 0 < price_pct_change_months(g, 8, 2)
                {
                    c.body_text = c.body_text.clone() + __content.string(7338) + &c.recovery_text;
                    break 'ae5e;
                }
                if c.price_streak < 0 {
                    c.body_text = c.body_text.clone()
                        + __content.string(7339)
                        + &ntw(c.price_streak, 0)
                        + __content.string(1095)
                        + __content.string(7340);
                } else if c.price_trend < 0 {
                    c.body_text = c.body_text.clone()
                        + __content.string(7341)
                        + &ntw(c.price_trend, 0)
                        + __content.string(1095)
                        + __content.string(7342);
                    if g.cur_quarter < 3 {
                        rep(&mut c.body_text, "do well", "profit");
                    }
                }
            }
        }
        if to_a9b1 {
            // LAB_0057a9b1
            if gdp(g) < Ext::from_f64(1.0) && c.outlook_rating < 5 {
                let r = update_metals_price_base(g, ui);
                if Ext::from_f64(1875.0) < r && sf(26.5f32) < silver_x(g) {
                    c.body_text = c.body_text.clone()
                        + __content.string(7343)
                        + __content.string(7344);
                    if c.price_streak < 1 {
                        if 0 < c.price_trend {
                            c.body_text = c.body_text.clone()
                                + __content.string(7345)
                                + &ntw(c.price_trend, 0)
                                + __content.string(1095);
                        }
                    } else {
                        c.body_text = c.body_text.clone()
                            + __content.string(7346)
                            + &ntw(c.price_streak, 0)
                            + __content.string(1095);
                    }
                    break 'ae5e;
                }
            }
            if 6 < c.outlook_rating {
                let sx = silver_x(g);
                let r = update_metals_price_base(g, ui);
                if sx + r / Ext::from_f64(100.0) < Ext::from_f64(31.0) {
                    c.body_text = c.body_text.clone() + __content.string(7347);
                    let lim = Ext::from_f64(200.0) * rng.rnd() + Ext::from_f64(1000.0);
                    let r2 = update_metals_price_base(g, ui);
                    if r2 < lim && c.pct_change_a < -9 {
                        c.body_text =
                            c.body_text.clone() + __content.string(7348);
                    }
                    c.body_text = c.body_text.clone() + ". ";
                    break 'ae5e;
                }
            }
            if cast::<Ext, _>(g.gold_price) * sf(1.1f32) < cast::<Ext, _>(g.gold_target_price)
                && 0 < price_trend_direction(g, 7, 2, 1)
                && cast::<Ext, _>(g.silver_price) * sf(1.12f32) < cast::<Ext, _>(g.silver_target_price)
                && 0 < price_trend_direction(g, 8, 2, 1)
            {
                c.body_text = c.body_text.clone()
                    + __content.string(7349);
                break 'ae5e;
            }
            if (cast::<Ext, _>(g.gold_target_price)) < cast::<Ext, _>(g.gold_price) * sf(0.95f32)
                && price_trend_direction(g, 7, 2, 1) < 0
                && (cast::<Ext, _>(g.silver_target_price)) < cast::<Ext, _>(g.silver_price) * sf(0.95f32)
                && price_trend_direction(g, 8, 2, 1) < 0
            {
                c.body_text = c.body_text.clone() + __content.string(7350);
            }
        }
    }
    // LAB_0057ae5e
    if 9 < c.pct_change_a.abs() {
        if c.pct_change_a < 1 {
            c.para_text = s("less");
        } else {
            c.para_text = s("more");
        }
        c.body_text = c.body_text.clone()
            + __content.string(7351)
            + &ntw(c.pct_change_a, 1)
            + " "
            + &c.para_text
            + __content.string(7352);
        c.cond_flag = 0;
        let mut afc8 = c.outlook_rating < 7;
        if !afc8 {
            let sx = silver_x(g);
            let r = update_metals_price_base(g, ui);
            if Ext::from_f64(31.0) <= sx + r / Ext::from_f64(100.0) || c.pct_change_a < 1 {
                afc8 = true;
            } else {
                c.cond_flag = 1;
            }
        }
        if afc8 && c.outlook_rating < 5 {
            let sx = silver_x(g);
            let r = update_metals_price_base(g, ui);
            if Ext::from_f64(44.0) < sx + r / Ext::from_f64(100.0) && c.pct_change_a < 0 {
                c.cond_flag = 1;
            }
        }
        if c.cond_flag == 1 {
            rep(&mut c.body_text, __content.string(7353), __content.string(7354));
        }
    }
    if 11 < c.pct_change_b.abs() {
        if c.pct_change_b < 1 {
            c.para_text = s("fallen");
            if c.pct_change_b < -14 {
                c.para_text = s("plunged");
            }
        } else {
            c.para_text = s("risen");
            if 14 < c.pct_change_b {
                c.para_text = s("shot up");
            }
        }
        c.body_text = c.body_text.clone()
            + __content.string(7355)
            + &c.para_text
            + &ntw(c.pct_change_b, 1)
            + __content.string(7356);
        c.trend_val = 0;
        if c.price_trend == 0 {
            c.price_trend = 3;
        }
        c.trend_val = price_pct_change_months(g, 8, c.price_trend);
        if c.pct_change_b < 21 || -8 < c.trend_val {
            if c.pct_change_b < -20 && 7 < c.trend_val {
                c.body_text = c.body_text.clone()
                    + __content.string(1378)
                    + &ntw(c.trend_val, 1)
                    + __content.string(1094)
                    + &ntw(c.price_trend, 0)
                    + " months";
            }
        } else {
            c.body_text = c.body_text.clone()
                + __content.string(7357)
                + &ntw(c.trend_val, 1)
                + __content.string(1094)
                + &ntw(c.price_trend, 0)
                + " months";
        }
        c.body_text = c.body_text.clone() + ". ";
        if sf(5.0f32) < gdp(g) && Ext::from_f64(26.0) < silver_x(g) && 0 < c.pct_change_b && 0 < price_pct_change_months(g, 8, 3) {
            c.body_text = c.body_text.clone()
                + __content.string(7358)
                + __content.string(7359);
            if c.pct_change_a < 0 && price_pct_change_months(g, 7, 6) <= 0 {
                if update_metals_price_base(g, ui) < Ext::from_f64(1330.0) {
                    c.body_text = c.body_text.clone() + __content.string(7360);
                }
            }
            c.body_text = c.body_text.clone() + ". ";
        }
    }
    c.para_text = s(__content.string(7361))
        + __content.string(7362)
        + __content.string(7363);
    if 4 < c.outlook_rating && sf(3.5f32) < gdp(g) {
        c.para_text = PbStr::new();
    }
    if c.outlook_rating == 5 || c.outlook_rating == 6 {
        c.para_text = PbStr::new();
    }
    if c.outlook_rating < 5 && gdp(g) < sf(3.8f32) {
        c.para_text = PbStr::new();
    }
    if 6 < c.outlook_rating && sf(0.6f32) < gdp(g) {
        c.para_text = PbStr::new();
    }
    let r = update_metals_price_base(g, ui);
    if Ext::from_f64(2000.0) < r && Ext::from_f64(24.0) < silver_x(g) && sf(1.2f32) < oil(g) {
        if 0 < price_pct_change_months(g, 6, 6) && 6 < c.outlook_rating {
            c.para_text = s(__content.string(7364))
                + __content.string(7365);
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                rep(&mut c.para_text, "risen", __content.string(7366));
            }
        }
    }
    if !c.body_text.is_empty() && !c.para_text.is_empty() {
        c.body_text = c.body_text.clone() + "\r\n" + "\r\n";
    }
    if !c.para_text.is_empty() {
        c.body_text = c.body_text.clone() + &c.para_text;
    }
}

// --------------------------------------------------------------------------------------- steel (12)

fn steel(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    c.trend_val = igr(g, 67) + igr(g, 3) + igr(g, 68);
    c.cond_flag = 0;
    if 15 < c.trend_val && sf(3.0f32) < gdp(g) {
        if c.outlook_rating < 5 {
            c.body_text = s(__content.string(7367));
        } else if c.outlook_rating < 7 {
            c.body_text = s(__content.string(7368))
                + __content.string(7369);
        } else {
            c.body_text = s(__content.string(7370))
                + __content.string(7371);
        }
        c.cond_flag = 1;
    }
    if c.trend_val < 3 && gdp(g) < sf(0.6f32) {
        if c.outlook_rating < 7 {
            c.body_text = s(__content.string(7372))
                + __content.string(7373);
        } else if 6 < c.outlook_rating {
            c.body_text = s(__content.string(7374))
                + __content.string(7375);
        }
        c.cond_flag = 2;
    }
    if sf(4.3f32) < gdp(g) && 6 < c.outlook_rating && (cast::<Ext, _>(c.net_margin)) < Ext::from_f64(9.0) && c.cond_flag == 0 {
        c.body_text = s(__content.string(7376))
            + __content.string(7377);
        if 0 < price_trend_streak(g, 1604) {
            c.body_text = c.body_text.clone() + __content.string(7378);
        }
        c.body_text = c.body_text.clone() + ". ";
    }
    if 0 < c.cond_flag {
        c.trend_val = price_trend_direction(g, 1604, 9, 3);
        if c.trend_val != 0 {
            c.para_text = s("improve");
            if c.cond_flag == 2 && c.trend_val < 0 {
                c.para_text = s("falter");
            }
            c.body_text =
                c.body_text.clone() + __content.string(7379) + &c.para_text + __content.string(7380);
        }
    }
}

// ------------------------------------------------------------------------- cyclical industries

/// Industries 10, 13, 20, 24, 33, 35, 36, 38, 40, 42, 45, 47, 49, 54, 58, 63.
fn cyclical(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    let mut bee5 = gdp(g) <= sf(3.8f32);
    if !bee5 {
        if igr(g, ind) < 7 || 4 < c.outlook_rating {
            bee5 = true;
        } else {
            c.body_text = s("The ")
                + &c.ind_name
                + __content.string(7381)
                + __content.string(7382)
                + __content.string(7383);
            if cast::<i32, _>(c.ind_plus_q) % 3 == 1 {
                rep(&mut c.body_text, __content.string(7384), __content.string(7385));
            }
            if cast::<i32, _>(c.ind_plus_q) % 3 == 2 {
                rep(&mut c.body_text, __content.string(7384), __content.string(7386));
            }
        }
    }
    if bee5 {
        if Ext::from_f64(1.0) > gdp(g)
            && igr(g, ind) < 1
            && 6 < c.outlook_rating
            && !has(&c.head_text, __content.string(7387))
            && c.gdp_trend < 0
            && price_trend_direction(g, 1604, 2, 1) < 0
        {
            c.body_text = s(__content.string(7388))
                + __content.string(7389)
                + &c.ind_name
                + __content.string(7390)
                + __content.string(7391);
            if cast::<i32, _>(c.ind_plus_q) % 2 == 1 {
                c.body_text = remove(c.body_text.as_bytes(), __content.bytes(7392));
            }
            if !(Ext::from_f64(14.0) > cast::<Ext, _>(c.proj_margin)) {
                rep(&mut c.body_text, __content.string(7393), __content.string(7394));
            }
            if -Ext::from_f64(1.0) > gdp(g) && -Ext::from_f64(8.0) < gdp(g) {
                rep(&mut c.body_text, __content.string(7395), __content.string(1199));
            }
            if has(&c.head_text, __content.string(7396)) {
                c.body_text = PbStr::new();
            }
        }
    }
    c.para_text = PbStr::new();
    if c.outlook_rating < 7 {
        if c.outlook_rating < 5 {
            if ind == 13 && sf(3.0f32) < gdp(g) && 11 < igr(g, 13) {
                c.para_text = s(__content.string(7397));
            }
            if ind == 20 && oil(g) < sf(0.8f32) {
                c.para_text =
                    s(__content.string(7398));
            }
            if ind == 35 && Ext::from_f64(23.0) < cast::<Ext, _>(c.net_margin) && sf(4.8f32) < gdp(g) {
                c.para_text = s(
                    __content.string(7399),
                );
            }
            if ind == 40 && 12 < igr(g, 40) && 15 < igr(g, 24) {
                c.para_text = s(__content.string(7400));
            }
            if ind == 42 && sf(3.1f32) < gdp(g) {
                c.para_text = s(__content.string(7401));
            }
            if ind == 49 && sf(4.9f32) < gdp(g) {
                c.para_text = s(__content.string(7402))
                    + __content.string(7403);
            }
            if (ind == 45 || ind == 58) && sf(3.2f32) < gdp(g) && gdp(g) < sf(4.8f32) && 5 < igr(g, ind) {
                c.para_text = s(__content.string(7404));
            }
        }
    } else {
        if ind == 13 && gdp(g) < Ext::from_f64(2.0) {
            c.para_text = s(__content.string(7405))
                + __content.string(7406);
        }
        if ind == 20 {
            let x = rng.rnd() * sf(0.2f32) + sf(1.1f32);
            if x < oil(g) {
                c.para_text = s(__content.string(7407));
            }
        }
        if ind == 24 {
            let r = rng.rnd();
            if (cast::<Ext, _>(igr(g, ind))) < r * Ext::from_f64(5.0) + Ext::from_f64(4.0) {
                c.para_text = s(__content.string(7408));
            }
        }
        if ind == 35 && c.outlook_rating == 9 {
            c.para_text =
                s(__content.string(7409));
        }
        if ind == 38 && igr(g, 38) < 5 && gdp(g) < Ext::from_f64(0.0) {
            c.para_text = s(__content.string(7410));
        }
        if ind == 40 && igr(g, 40) < 4 && igr(g, 24) < 7 {
            c.para_text =
                s(__content.string(7411));
        }
        if ind == 49 && igr(g, 49) < 3 {
            c.para_text = s(__content.string(7412))
                + "into "
                + &c.ind_name_lc
                + __content.string(7413);
        }
        if ind == 54 && igr(g, 54) < 2 && gdp(g) < sf(1.6f32) {
            c.para_text =
                s(__content.string(7414));
        }
        if (ind == 45 || ind == 58) && gdp(g) < sf(1.8f32) && igr(g, ind) < 3 {
            c.para_text = s(__content.string(7415))
                + __content.string(7416);
        }
    }
    c.body_text = c.body_text.clone() + &c.para_text;
    c.para_text = PbStr::new();
}

// ------------------------------------------------------------------------------------ aerospace (14)

fn aerospace(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    if igr(g, 14) < 0 && 6 < c.outlook_rating {
        c.body_text = s(__content.string(7417));
        if igr(g, 52) + igr(g, 16) < 3 {
            c.body_text = s(__content.string(7418))
                + __content.string(7419);
        }
    } else if igr(g, 14) < 6 || 4 < c.outlook_rating {
        if gdp(g) < Ext::from_f64(0.0) && c.outlook_rating < 6 && 0 < igr(g, 14) {
            c.body_text = s(__content.string(7420))
                + __content.string(7421);
            return;
        }
        if (cast::<Ext, _>(c.net_margin)) < Ext::from_f64(14.0) && 4 < c.outlook_rating {
            c.para_text = s(__content.string(7422));
            if oil(g) < sf(0.8f32) {
                c.para_text = s(__content.string(7423));
            }
            c.body_text = s(__content.string(7424)) + &c.para_text;
        }
    } else {
        c.body_text = s(__content.string(7425));
        c.para_text = s(__content.string(7426));
        if g.cur_quarter == 2 {
            c.para_text = s(__content.string(7427));
        }
        if g.cur_quarter == 3 {
            c.para_text = s(__content.string(7428));
        }
        if g.cur_quarter == 4 {
            c.para_text = s(__content.string(7429));
        }
        c.para_text = c.para_text.clone() + __content.string(7430);
        if g.cur_quarter < 3 {
            rep(&mut c.para_text, "months", "years");
        }
        if 20 < igr(g, 52) + igr(g, 16) {
            c.para_text = s(__content.string(7431));
        }
        c.body_text = c.body_text.clone() + &c.para_text;
        if instr(1, ucase(c.ind_name.as_bytes(), CaseMode::Ansi).as_bytes(), b"AEROSPACE") == 0 {
            c.body_text = s(__content.string(7432))
                + __content.string(7433)
                + __content.string(7434);
        }
    }
}

// ---------------------------------------------------------------- consumer staples / health / media

/// Industries 17, 22, 27, 28, 29, 31, 32, 34, 46, 55, 56, 65 (then packaged foods for 28).
fn consumer(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    if Ext::from_f64(0.0) <= gdp(g) || 4 < c.outlook_rating {
        if (c.outlook_rating == 5 || c.outlook_rating == 6) && (ind == 22 || ind == 56) {
            c.body_text =
                s(__content.string(7435));
            if g.cur_quarter == 2 || g.cur_quarter == 3 {
                rep(&mut c.body_text, __content.string(7436), "caps on");
            }
            if 2 < g.cur_quarter {
                rep(&mut c.body_text, __content.string(7437), __content.string(7438));
            }
        }
    } else {
        c.body_text = s(__content.string(7439))
            + &c.ind_name_lc
            + __content.string(4980)
            + __content.string(7440)
            + __content.string(7441);
        if Ext::from_f64(5.0) * rng.rnd() > Ext::from_f64(3.0) {
            rep(&mut c.body_text, __content.string(7442), __content.string(7443));
        }
    }
    c.para_text = PbStr::new();
    let mut d20a = c.outlook_rating < 7;
    if !d20a {
        if !c.body_text.is_empty() {
            d20a = true;
        } else {
            'cfca: {
                if ind == 17 {
                    if grain(g) <= sf(16.9f32) && !has(&c.head_text, "excise") {
                        c.para_text = s(__content.string(7444))
                            + __content.string(7445);
                        break 'cfca;
                    }
                    if (cast::<Ext, _>(c.net_margin)) < sf(16.2f32) && sf(16.9f32) < grain(g) {
                        c.para_text = s(__content.string(7446))
                            + &c.ind_name
                            + __content.string(1037);
                    }
                }
            }
            if ind == 27 && igr(g, 27) < 3 {
                c.para_text = s(__content.string(7447))
                    + __content.string(7448);
            }
            if ind == 34 && igr(g, 34) < 3 {
                c.para_text = s(__content.string(7449))
                    + __content.string(7450);
            }
            if ind == 29 {
                c.para_text = s(__content.string(7451))
                    + __content.string(7452);
            }
            if ind == 22 || ind == 56 {
                if igr(g, ind) < 5 {
                    c.para_text = s(__content.string(7453));
                }
                if igr(g, ind) < 1 {
                    rep(&mut c.para_text, "slow", "halt");
                }
            }
            if ind == 65 && igr(g, 65) < 2 {
                c.para_text = s(__content.string(7454))
                    + __content.string(7455)
                    + __content.string(7456);
            }
        }
    }
    if d20a {
        // LAB_0057d20a
        if c.outlook_rating < 5 && c.body_text.is_empty() {
            if ind == 31 && 2 < igr(g, 31) {
                c.para_text =
                    s(__content.string(7457));
            }
            if ind == 34 && 5 < igr(g, 34) {
                c.para_text =
                    s(__content.string(7458));
            }
            if ind == 46 && 2 < igr(g, 46) && gdp(g) < Ext::from_f64(1.0) {
                c.para_text = s(__content.string(7459));
                if sf(13.5f32) < grain(g) {
                    c.para_text = c.para_text.clone()
                        + __content.string(7460)
                        + __content.string(7461);
                }
            }
            if ind == 55 && 4 < igr(g, 55) && igr(g, 3) < 0 && igr(g, 67) < 1 && igr(g, 68) < 2 {
                c.para_text = s(__content.string(7462));
            }
            if (ind == 22 || ind == 56) && (gdp(g) < sf(1.8f32) || sf(4.8f32) < gdp(g)) {
                c.para_text = s(__content.string(7463))
                    + &c.ind_name_lc
                    + " "
                    + __content.string(7464);
                if ind == 56 {
                    rep(&mut c.para_text, "prosper", __content.string(7465));
                }
                if Ext::from_f64(0.0) <= gdp(g) {
                    c.para_text = c.para_text.clone() + ". ";
                } else {
                    c.para_text = c.para_text.clone() + __content.string(7466);
                }
            }
            if ind == 65 && 5 < igr(g, 65) {
                c.para_text = s(__content.string(7467));
                if gdp(g) < sf(1.75f32) {
                    c.para_text = c.para_text.clone() + __content.string(7468);
                }
                c.para_text = c.para_text.clone()
                    + __content.string(7469);
            }
        } else if (c.outlook_rating == 5 || c.outlook_rating == 6) && ind == 65 {
            if c.body_text.is_empty() && gdp(g) < -Ext::from_f64(5.0) {
                c.para_text = s(__content.string(7470))
                    + __content.string(7471);
            }
        }
    }
    // LAB_0057d716
    c.body_text = c.body_text.clone() + &c.para_text;
    if ind != 28 {
        return;
    }
    packaged_foods(g, c);
}

/// Industry 28, packaged foods: grain prices (0x57d73b .. 0x57dc20+).
fn packaged_foods(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    c.trend_val = 0;
    c.para_text = PbStr::new();
    c.clause_text = PbStr::new();
    c.price_streak = price_trend_streak(g, 9);
    if c.price_streak.abs() < 4 {
        c.price_streak = 0;
    }
    c.price_trend = price_trend_streak(g, 10);
    if c.price_trend.abs() < 4 {
        c.price_trend = 0;
    }
    c.trend_val = c.price_trend + c.price_streak;
    if 0 < c.price_streak && c.price_trend < 0 {
        c.trend_val = 0;
    }
    if c.price_streak < 0 && 0 < c.price_trend {
        c.trend_val = 0;
    }
    c.cond_flag = 0;
    'dacb: {
        if c.outlook_rating < 5 || grain(g) <= sf(14.5f32) {
            if c.outlook_rating < 5 {
                // Bug: `CornPrice < WheatPrice > 14.5` (a -1/0 comparison result against 14.5): never true.
                let cmp: Ext = if (cast::<Ext, _>(g.corn_price)) < cast::<Ext, _>(g.wheat_price) { -Ext::from_f64(1.0) } else { Ext::from_f64(0.0) };
                if cmp > sf(14.5f32) {
                    c.body_text = s(__content.string(7472))
                        + __content.string(7473)
                        + __content.string(7474)
                        + __content.string(7475);
                    if c.trend_val != 0 {
                        c.cond_flag = 1;
                    }
                    break 'dacb;
                }
            }
            if 6 < c.outlook_rating || sf(7.8f32) <= grain(g) {
                if 6 < c.outlook_rating && -Ext::from_f64(2.0) > gdp(g) {
                    c.body_text = c.body_text.clone()
                        + "The "
                        + &c.ind_name_lc
                        + __content.string(7476)
                        + __content.string(7477)
                        + __content.string(7478);
                }
            } else {
                c.body_text = c.body_text.clone()
                    + "The "
                    + &c.ind_name
                    + __content.string(7479)
                    + __content.string(7480);
                if c.trend_val < 0 {
                    c.cond_flag = 1;
                }
            }
        } else {
            c.body_text = s(__content.string(7481))
                + __content.string(7482);
            if c.outlook_rating < 7 {
                rep(&mut c.body_text, "under", __content.string(7483));
            }
            if 0 < c.trend_val {
                c.cond_flag = 1;
            }
        }
    }
    // LAB_0057dacb
    if c.cond_flag == 1 {
        c.trend_val = c.price_streak;
        c.para_text = s("wheat");
        if (cast::<Ext, _>(c.price_streak)).abs() < (cast::<Ext, _>(c.price_trend)).abs() {
            c.trend_val = c.price_trend;
            c.para_text = s("corn");
        }
        c.clause_text = s("increas");
        if c.trend_val < 0 {
            c.clause_text = s("declin");
        }
        c.body_text = c.body_text.clone()
            + __content.string(7484)
            + &c.para_text
            + __content.string(6422)
            + &c.clause_text
            + "ing for"
            + &ntw(c.trend_val, 0)
            + __content.string(1277);
    }
    if c.body_text.is_empty() {
        if sf(17.0f32) < grain(g) {
            c.para_text = s("High");
            c.clause_text = s("increas");
        }
        if grain(g) < sf(7.0f32) {
            c.para_text = s("Low");
            c.clause_text = s("reduc");
        }
        if !c.clause_text.is_empty() {
            c.body_text = c.para_text.clone()
                + __content.string(7485)
                + &c.clause_text
                + __content.string(7486);
        }
    }
}

// ----------------------------------------------------------------------------------------- tires (18)

fn tires(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    c.clause_text = s(". ");
    if -Ext::from_f64(1.0) <= gdp(g) && gdp(g) <= sf(4.8f32) {
        if igr(g, 18) == 0 {
            return;
        }
        c.clause_text = PbStr::new();
        let sum = igr(g, 67) + igr(g, 3) + igr(g, 68);
        c.trend_val = to_long(cast::<Ext, _>(sum) / Ext::from_f64(3.0));
        if c.trend_val < 4 || igr(g, 18) < 4 {
            // LAB_0057e0b9
            if c.trend_val < 0 && igr(g, 18) < 0 {
                c.clause_text = s("declin");
                if 2 < g.cur_quarter {
                    c.clause_text = s("shrink");
                }
            }
        } else {
            c.clause_text = s("grow");
        }
        if !c.clause_text.is_empty() {
            c.body_text = s(__content.string(7487))
                + &c.clause_text
                + __content.string(7488)
                + __content.string(7489);
        }
        return;
    }
    if -Ext::from_f64(1.0) <= gdp(g) || 0 < igr(g, ind) {
        // LAB_0057de55
        if Ext::from_f64(4.0) < gdp(g) && 4 < igr(g, ind) {
            c.clause_text = s(__content.string(7490));
            if sf(5.5f32) < gdp(g) {
                rep(&mut c.clause_text, "rapid", "torrid");
            }
        }
    } else {
        c.clause_text = s(__content.string(7491));
        if -Ext::from_f64(8.0) > gdp(g) {
            rep(&mut c.clause_text, "a ", __content.string(7492));
        }
    }
    if !has(&c.head_text, __content.string(7387)) {
        c.body_text = s(__content.string(7493))
            + __content.string(7494);
        if !(c.clause_text == ". ") {
            c.body_text = c.body_text.clone() + __content.string(7495);
        }
        c.body_text = c.body_text.clone() + &c.clause_text;
    }
}

// ----------------------------------------------------------------------------------- retail (41, 43, 44, 48)

fn retail(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    c.clause_text = PbStr::new();
    'e851: {
        let mut e285 = gdp(g) <= sf(3.75f32);
        if !e285 {
            if igr(g, ind) < 5 || 4 < c.outlook_rating {
                e285 = true;
            } else {
                c.body_text = s(__content.string(7496))
                    + __content.string(7497);
            }
        }
        if !e285 {
            break 'e851;
        }
        // LAB_0057e285
        if c.outlook_rating < 5 || 6 < c.outlook_rating {
            if -Ext::from_f64(1.0) > gdp(g) && igr(g, ind) < 0 && 6 < c.outlook_rating {
                c.body_text = s(__content.string(7498))
                    + __content.string(7499)
                    + &c.ind_name
                    + __content.string(1157);
                if gdp(g) <= -Ext::from_f64(8.0) {
                    rep(&mut c.body_text, __content.string(7500), __content.string(7501));
                } else {
                    rep(&mut c.body_text, __content.string(7500), __content.string(7502));
                }
                if ind == 44 {
                    rep(&mut c.body_text, __content.string(7437), __content.string(7503));
                }
                if int(cast::<Ext, _>(c.cur_slack_pct)) <= int(cast::<Ext, _>(c.proj_slack_pct)) {
                    c.clause_text = s(". ");
                } else {
                    c.clause_text = s(__content.string(7504));
                }
                break 'e851;
            }
            if gdp(g) < sf(0.7f32) && 2 < igr(g, ind) && c.outlook_rating < 5 {
                if ind == 48 {
                    c.body_text = s(__content.string(7505));
                } else if ind == 43 && 5 < igr(g, 43) {
                    c.body_text = s(
                        __content.string(7506),
                    );
                }
            }
        } else {
            if Ext::from_f64(1.0) < gdp(g) && gdp(g) < sf(2.3f32) && 0 < igr(g, ind) && igr(g, ind) < 4 {
                c.body_text = s(__content.string(7507)) + __content.string(7508);
                c.clause_text = s(__content.string(7509));
                if igr(g, ind) + 3 < igf(g, ind) {
                    c.clause_text = s(__content.string(7510));
                }
                if igf(g, ind) + 3 < igr(g, ind) {
                    c.clause_text = s(__content.string(7511));
                }
                break 'e851;
            }
            if gdp(g) <= Ext::from_f64(1.0) && -1 < igr(g, ind) && Ext::from_f64(12.0) < cast::<Ext, _>(c.proj_margin) {
                c.body_text = s(__content.string(7512))
                    + &c.ind_name
                    + __content.string(7513)
                    + __content.string(7514);
                if int(cast::<Ext, _>(c.proj_slack_pct)) <= int(cast::<Ext, _>(c.cur_slack_pct)) {
                    c.clause_text = s(". ");
                } else {
                    c.clause_text = s(__content.string(7515));
                }
                break 'e851;
            }
            if ind == 44 && sf(2.8f32) < gdp(g) && igr(g, 44) < 6 {
                c.body_text = s(__content.string(7516))
                    + __content.string(7517);
            }
        }
    }
    // LAB_0057e851
    if !c.body_text.is_empty() {
        c.body_text = c.body_text.clone() + &c.clause_text;
    }
}

// --------------------------------------------------------------- restaurants, network, security, …

/// The remaining industries (SELECT CASE 57 restaurants, 61 networking, 62 security, 64 pollution
/// control; others: nothing).
fn others(g: &mut Globals, ui: &mut dyn Ui, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = cast::<i32, _>(c.industry_id);
    match ind {
        57 => {
            c.price_streak = price_pct_change_months(g, 9, 6);
            c.price_trend = price_pct_change_months(g, 10, 6);
            if Ext::from_f64(0.0) > gdp(g) && 3 < igr(g, ind) && c.outlook_rating < 5 {
                c.body_text = s(__content.string(7518));
            }
            if sf(7.8f32) <= grain(g) {
                if sf(1.8f32) <= gdp(g) || c.outlook_rating < 7 {
                    if sf(4.8f32) < gdp(g) && c.outlook_rating < 5 {
                        c.body_text = s(__content.string(7519))
                            + __content.string(7520);
                    }
                } else {
                    c.clause_text = s(__content.string(7521));
                    if 1 < igr(g, ind) {
                        c.clause_text = s(__content.string(7522));
                    }
                    c.body_text = c.clause_text.clone()
                        + __content.string(7523);
                    if g.peak_oil_crisis == 1 {
                        c.body_text = c.body_text.clone()
                            + __content.string(7524)
                            + __content.string(7525);
                        if 2 < g.cur_quarter {
                            rep(&mut c.body_text, __content.string(7526), __content.string(7527));
                        }
                    }
                }
            } else {
                c.body_text = c.body_text.clone()
                    + __content.string(7528);
                if c.price_streak < -7 && c.price_trend < -7 {
                    rep(&mut c.body_text, "down", "falling");
                    c.trend_val = to_long((cast::<Ext, _>(c.price_streak)).abs());
                    c.clause_text = s("wheat");
                    if (cast::<Ext, _>(c.price_trend)).abs() > cast::<Ext, _>(c.trend_val) {
                        c.trend_val = to_long((cast::<Ext, _>(c.price_trend)).abs());
                        c.clause_text = s("corn");
                    }
                    c.body_text = c.body_text.clone()
                        + __content.string(7529)
                        + &c.clause_text
                        + __content.string(7530)
                        + &ntw(c.trend_val, 1)
                        + __content.string(7531);
                }
            }
            if sf(14.0f32) < grain(g) {
                c.body_text = c.body_text.clone() + __content.string(7532);
                c.pct_change = c.price_streak;
                c.para_text = s("Wheat");
                // Bug: compares ABS(priceTrend) with itself, so the corn branch is dead.
                if (cast::<Ext, _>(c.price_trend)).abs() > (cast::<Ext, _>(c.price_trend)).abs() {
                    c.pct_change = c.price_trend;
                    c.para_text = s("Corn");
                }
                if c.pct_change < 1 {
                    c.clause_text = s("down");
                } else {
                    c.clause_text = s("up");
                }
                if (cast::<Ext, _>(c.pct_change)).abs() > Ext::from_f64(10.0) {
                    c.body_text = c.body_text.clone()
                        + &c.para_text
                        + __content.string(7533)
                        + &c.clause_text
                        + &ntw(c.pct_change, 1)
                        + __content.string(7534);
                    if c.pct_change < 0 {
                        c.body_text = c.body_text.clone() + __content.string(1387);
                    }
                    c.body_text = c.body_text.clone() + ". ";
                }
            }
        }
        61 => {
            if 6 < c.outlook_rating && igr(g, ind) < 5 {
                c.clause_text = PbStr::new();
                if gdp(g) < sf(1.4f32) {
                    c.clause_text = s(__content.string(7535));
                }
                c.body_text = s(__content.string(7536))
                    + &c.clause_text
                    + __content.string(7537);
            }
        }
        62 => {
            if -Ext::from_f64(2.0) > gdp(g) && 3 < igr(g, ind) && c.outlook_rating < 7 {
                c.clause_text = s("rec");
                if -Ext::from_f64(15.0) > gdp(g) {
                    c.clause_text = s("depr");
                }
                c.body_text = s(__content.string(7538))
                    + &c.clause_text
                    + __content.string(7539)
                    + __content.string(7540)
                    + __content.string(7541);
                return;
            }
            let lim = gdp(g) + Ext::from_f64(2.0);
            if (cast::<Ext, _>(igr(g, ind))) < lim && sf(4.8f32) < gdp(g) && 6 < c.outlook_rating {
                c.body_text = s(__content.string(7542))
                    + __content.string(7543);
            }
        }
        64 => {
            if 4 < c.outlook_rating || sf(1.8f32) <= gdp(g) {
                if 4 < c.outlook_rating {
                    let r = update_metals_price_base(g, ui);
                    if Ext::from_f64(2300.0) < r && sf(24.5f32) < silver_x(g) {
                        c.body_text = s(__content.string(7544))
                            + __content.string(7545);
                    }
                }
            } else if 10 < igr(g, ind) {
                c.body_text = s(__content.string(7546))
                    + __content.string(7547);
                if Ext::from_f64(1.0) < gdp(g) {
                    c.body_text = remove(c.body_text.as_bytes(), __content.bytes(7548));
                }
            }
        }
        _ => {}
    }
}

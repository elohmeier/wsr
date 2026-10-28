//! Operating industries: fads, long-term growth, forecast, special taxes and the tech-bubble aftermath
//! (LAB_0057f23a .. 0x5825c0).

#![allow(clippy::collapsible_if, clippy::collapsible_else_if, clippy::neg_cmp_op_on_partial_ord)]
use crate::pb::real::{Ext, cast};


use super::head::rep;
use super::{Ctx, s};
use crate::logic::finance::{calc_capital_taxes, calc_entity_financials};
use crate::pb::numeric::str_num;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, ltrim, remove, right, rtrim};
use crate::state::Globals;
use crate::ui::Ui;

const CRLF: &str = "\r\n";

/// `RTRIM$(LTRIM$(STR$(n)))` of an INTEGER.
fn num(n: i16) -> PbStr {
    rtrim(ltrim(str_num(cast::<Ext, _>(n), 15).as_bytes()).as_bytes())
}

fn has_crlf(x: &PbStr) -> bool {
    instr(1, right(x.as_bytes(), 2).as_bytes(), CRLF.as_bytes()) != 0
}

/// Part of BuildIndustryOutlookText (0x56f33b) from LAB_0057f23a to the end.
pub(super) fn tail(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = c.industry_id;
    let iu = cast::<i32, _>(ind);
    // fads
    if iu == g.fad_industry && 150 < g.fad_valuation_pct && g.fad_valuation_pct % 2 == 0 {
        if c.outlook_rating < 5 {
            c.clause_text = PbStr::new();
            c.phrase_text = right(c.body_text.as_bytes(), 2);
            let check_head = if instr(1, c.phrase_text.as_bytes(), CRLF.as_bytes()) == 0 {
                c.cmp_val = 100;
                if c.body_text.len() <= c.cmp_val {
                    true
                } else {
                    c.clause_text = s(CRLF) + CRLF;
                    false
                }
            } else {
                true
            };
            if check_head && !has_crlf(&c.head_text) {
                c.clause_text = s(CRLF) + CRLF;
            }
            c.body_text = c.body_text.clone()
                + &c.clause_text
                + __content.string(7845)
                + &c.ind_name_lc
                + __content.string(4980)
                + __content.string(7846);
        } else if g.fad_valuation_pct < 160 && 6 < c.outlook_rating {
            c.body_text = c.body_text.clone()
                + __content.string(7847)
                + &c.ind_name_lc
                + __content.string(7848);
        }
    }
    c.clause_text = right(c.body_text.as_bytes(), 2);
    if instr(1, c.clause_text.as_bytes(), CRLF.as_bytes()) == 0 && !c.body_text.is_empty() {
        c.body_text = c.body_text.clone() + CRLF + CRLF;
    }
    c.head_text = c.head_text.clone() + &c.body_text;
    c.body_text = PbStr::new();
    c.clause_text = PbStr::new();
    long_term_growth(g, c);
    // LAB_0058069b
    if !c.body_text.is_empty() {
        c.body_text = c.body_text.clone() + &c.clause_text + CRLF + CRLF;
    }
    g.ind_outlook_text = c.head_text.clone() + &c.body_text;
    c.body_text = PbStr::new();
    forecast(g, rng, c);
    if !c.body_text.is_empty() {
        g.ind_outlook_text = g.ind_outlook_text.clone() + &c.body_text + CRLF + CRLF;
    }
    if 10 < g.sel_entity {
        let sel = g.sel_entity;
        calc_entity_financials(g, ui, rng, sel);
    }
    if 10 < g.sel_entity {
        let sel = g.sel_entity;
        if calc_capital_taxes(g, sel) > Ext::from_f64(0.0) && g.industry_num[g.sel_entity] == ind {
            special_taxes(g, c);
        }
    }
    c.body_text = PbStr::new();
    if 107 < g.bubble_state && g.bubble_state < 114 {
        if c.proj_margin < Ext::from_f32(4.0f32) && 6 < c.outlook_rating {
            if matches!(ind, 10 | 24 | 26 | 30 | 35) {
                c.body_text = s("The ")
                    + &c.ind_name
                    + __content.string(7849)
                    + "\""
                    + __content.string(7850)
                    + "\""
                    + " "
                    + __content.string(7851)
                    + __content.string(7852);
            } else if matches!(ind, 37 | 39 | 40 | 61) {
                c.body_text = s(__content.string(7853))
                    + &c.ind_name
                    + __content.string(7854)
                    + __content.string(7855)
                    + __content.string(7856)
                    + "\""
                    + __content.string(7857)
                    + "\""
                    + " ";
            }
        }
        if !c.body_text.is_empty() {
            c.body_text = c.body_text.clone() + CRLF + CRLF;
        }
        g.ind_outlook_text = g.ind_outlook_text.clone() + &c.body_text;
    }
}

/// The long-term growth paragraph (0x57f594 .. LAB_0058069b).
fn long_term_growth(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = c.industry_id;
    let iu = cast::<i32, _>(ind);
    let ltg = g.ind_long_term_growth[iu];
    let igr = g.ind_growth_rate[iu];
    if ind == 4 || ind == 21 {
        if ltg < 8 {
            if 207 < g.bubble_state && g.bubble_state < 219 && 6 < c.outlook_rating {
                c.body_text = s(__content.string(7858))
                    + __content.string(7859)
                    + __content.string(7860);
                c.clause_text = PbStr::new();
            }
        } else {
            c.body_text = s(__content.string(7861));
            if ind == 4 {
                c.body_text = s("Biotech") + &c.body_text;
            } else {
                c.body_text = s(__content.string(7862)) + &c.body_text;
            }
            c.clause_text = s(__content.string(7863));
            if (cast::<i32, _>(ltg) + 5) < cast::<i32, _>(igr) {
                c.clause_text = s(__content.string(7864))
                    + __content.string(7865);
            } else {
                let mut slowed = true;
                if cast::<i32, _>(ltg) <= cast::<i32, _>(igr) + 5 && 5 < igr {
                    if ind == 21 && 4 < c.outlook_rating {
                        c.clause_text =
                            s(__content.string(7866));
                    } else if c.outlook_rating < 5 {
                        c.clause_text =
                            s(__content.string(7867));
                        if ind == 21 {
                            rep(&mut c.clause_text, "benefit", __content.string(7868));
                        }
                    }
                    slowed = false;
                }
                if slowed {
                    c.clause_text = s(__content.string(7869));
                }
            }
            // LAB_0057f84c
            if g.bubble_state < 205 || 206 < g.bubble_state || 4 < c.outlook_rating {
                if 6 < c.outlook_rating && g.bubble_state < 1 && igr < 5 {
                    c.body_text = c.ind_name.clone()
                        + __content.string(7870)
                        + __content.string(7871);
                    c.clause_text = PbStr::new();
                }
            } else {
                c.body_text = s(__content.string(7872))
                    + __content.string(7873)
                    + __content.string(7874)
                    + "\""
                    + __content.string(7875)
                    + "\""
                    + __content.string(7876)
                    + __content.string(7877);
                c.clause_text = PbStr::new();
            }
        }
    } else if matches!(ind, 11 | 17 | 19 | 31 | 44 | 60 | 66) {
        if ltg < 3 {
            c.body_text =
                s("The ") + &c.ind_name + __content.string(7878) + __content.string(7879);
            if c.outlook_rating < 5 {
                c.clause_text = s(__content.string(7880))
                    + __content.string(7881);
            } else if 6 < c.outlook_rating {
                if ind == 19 || ind == 60 || ind == 66 {
                    c.clause_text = s(__content.string(7882))
                        + &c.ind_name_lc
                        + __content.string(7883)
                        + __content.string(7884);
                    if 2 < g.cur_quarter {
                        rep(&mut c.clause_text, __content.string(7885), __content.string(5792));
                    }
                    let k = (g.cur_quarter + g.cur_year) % 4;
                    if k == 1 {
                        rep(&mut c.clause_text, "Africa", __content.string(7886));
                    }
                    if k == 2 {
                        rep(&mut c.clause_text, "Africa", "Myanmar");
                    }
                    if k == 3 {
                        rep(&mut c.clause_text, "Africa", __content.string(6979));
                    }
                } else if ind == 31 {
                    c.clause_text = s(__content.string(7887))
                        + __content.string(7888);
                }
            }
        }
    } else {
        c.trend_val = cast::<i32, _>(igr) - cast::<i32, _>(ltg);
        if 5 < c.trend_val.abs() && 7 < ltg {
            c.body_text = c.ind_name.clone()
                + __content.string(7889)
                + &num(ltg)
                + __content.string(7890);
            if c.trend_val < 6 {
                if igr < 1 {
                    if igr == 0 {
                        c.body_text = c.body_text.clone() + __content.string(7891);
                    } else {
                        c.body_text = c.body_text.clone() + __content.string(7892) + &num(igr) + "%. ";
                    }
                } else {
                    c.body_text = c.body_text.clone() + __content.string(7893) + &num(igr) + "%. ";
                }
            } else {
                c.body_text = c.body_text.clone() + __content.string(7894) + &num(igr) + "%";
                c.clause_text = s(". ");
                if igr <= g.ind_growth_forecast[iu] {
                    c.clause_text = s(__content.string(7895));
                }
            }
            return;
        }
        if ltg < 8 {
            c.body_text = s("The ")
                + &c.ind_name
                + __content.string(7896)
                + __content.string(7897)
                + &num(ltg)
                + __content.string(7898)
                + __content.string(7899);
            if ltg < 0 {
                rep(&mut c.body_text, "mature", __content.string(1456));
                rep(&mut c.body_text, "grow", "shrink");
            }
            if ltg == 0 {
                c.body_text = s("The ") + &c.ind_name + __content.string(7900);
            }
            if 4 < ltg {
                c.body_text = s("The ")
                    + &c.ind_name
                    + __content.string(7901)
                    + __content.string(7902)
                    + &num(ltg)
                    + __content.string(7903);
            }
            if c.trend_val < 6 {
                if c.trend_val < -5 {
                    c.clause_text = s(__content.string(7904));
                    if igr < 0 {
                        c.clause_text = c.clause_text.clone() + __content.string(7905) + &num(igr) + "% " + __content.string(7906);
                    } else {
                        c.clause_text = c.clause_text.clone() + ". ";
                    }
                    if 0 < igr {
                        c.clause_text = s(__content.string(7907))
                            + __content.string(7908)
                            + &num(igr)
                            + __content.string(7909);
                    }
                }
            } else {
                c.clause_text = s(__content.string(7910))
                    + &num(igr)
                    + __content.string(7911);
                if igr < 0 {
                    c.clause_text = s(__content.string(7912))
                        + &num(igr)
                        + __content.string(7913)
                        + __content.string(7914);
                } else if igr == 0 {
                    c.clause_text = s(__content.string(7915))
                        + __content.string(7916);
                }
            }
        } else {
            c.body_text = s("The ")
                + &c.ind_name
                + __content.string(7917)
                + __content.string(7918)
                + &num(ltg)
                + __content.string(7919);
        }
    }
}

/// The one-year forecast remark (after LAB_0058069b).
fn forecast(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    let iu = cast::<i32, _>(c.industry_id);
    let igr = g.ind_growth_rate[iu];
    c.trend_val = cast::<i32, _>(g.ind_growth_forecast[iu]) - cast::<i32, _>(igr);
    if c.trend_val < 6 || igr < 1 {
        // LAB_005809b0
        if c.trend_val < -5 && 3 < igr {
            c.body_text = s(__content.string(7920))
                + &c.ind_name
                + __content.string(7921);
            if igr < 1 {
                rep(&mut c.body_text, "slow", __content.string(7922));
            } else if g.rand_seed % 5 == 2 {
                rep(&mut c.body_text, "slow", "soften");
            } else if g.rand_seed % 5 == 3 {
                rep(&mut c.body_text, "slow", "decline");
            } else if g.rand_seed % 5 == 4 {
                rep(&mut c.body_text, "slow", "weaken");
            }
        }
        return;
    }
    c.body_text = s(
        __content.string(7923),
    );
    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
        rep(&mut c.body_text, "expect", __content.string(7924));
    }
    if !(rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0)) || igr < 1 {
        // LAB_005808be
        if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) && 0 < igr {
            rep(&mut c.body_text, "improve", __content.string(7925));
        }
    } else {
        rep(&mut c.body_text, "improve", __content.string(7926));
    }
    if c.net_margin < c.proj_margin {
        c.body_text = c.body_text.clone()
            + CRLF
            + CRLF
            + __content.string(7927)
            + __content.string(7928);
    }
}

/// The carbon / health-care / oil windfall and personal holding company tax remarks for `SelEntity`'s
/// industry.
fn special_taxes(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    let ind = c.industry_id;
    c.body_text = PbStr::new();
    c.head_text = PbStr::new();
    let high_margin = |c: &Ctx| Ext::from_f32(18.0f32) < c.proj_margin && 19.4f32 < c.net_margin;
    if matches!(ind, 7 | 8 | 63) {
        if g.oil_windfall_tax_rate < 1 {
            if Ext::from_f64(0.0) < g.carbon_tax_qtr {
                c.head_text = s("A");
                if high_margin(c) {
                    c.head_text = s(__content.string(7929));
                }
                c.body_text = c.head_text.clone()
                    + __content.string(7930)
                    + __content.string(7931)
                    + &c.ind_name
                    + __content.string(1037);
            }
        } else {
            c.head_text = s(__content.string(7932));
            if g.oil_price < Ext::from_f32(0.8f32) {
                c.head_text = s(__content.string(7933));
            }
            c.para_text = s(__content.string(7934));
            if 2 < g.cur_quarter {
                c.para_text = s(__content.string(7935));
            }
            c.body_text = c.head_text.clone()
                + __content.string(7936)
                + &c.para_text
                + __content.string(7937)
                + "of the "
                + &c.ind_name
                + __content.string(1037);
        }
    } else if matches!(ind, 4 | 21 | 22 | 56) {
        if Ext::from_f64(0.0) < g.health_care_tax_qtr {
            c.head_text = s("A");
            if high_margin(c) {
                c.head_text = s(__content.string(7929));
            }
            c.body_text = c.head_text.clone()
                + __content.string(7938)
                + __content.string(7939);
        }
    } else if matches!(ind, 3 | 7 | 8 | 9 | 11 | 12 | 16 | 18 | 20 | 23 | 25 | 33 | 50 | 51 | 52 | 55 | 67 | 68 | 69)
        && Ext::from_f64(0.0) < g.carbon_tax_qtr
    {
        c.head_text = s("A");
        if high_margin(c) {
            c.head_text = s(__content.string(7929));
        }
        c.body_text = c.head_text.clone()
            + __content.string(7940)
            + __content.string(7931)
            + &c.ind_name
            + __content.string(1037);
    }
    if !c.body_text.is_empty() {
        g.ind_outlook_text = g.ind_outlook_text.clone() + &c.body_text + CRLF + CRLF;
    }
    c.body_text = PbStr::new();
    if Ext::from_f64(0.0) < g.holding_co_tax_due {
        c.body_text = s(__content.string(7941))
            + __content.string(7942)
            + __content.string(7943)
            + __content.string(7944)
            + &g.sel_corp_name
            + ". ";
        if g.capital_tax_count == 1 {
            c.body_text = remove(c.body_text.as_bytes(), b" also");
        }
    }
    if !c.body_text.is_empty() {
        g.ind_outlook_text = g.ind_outlook_text.clone() + &c.body_text + CRLF + CRLF;
    }
}

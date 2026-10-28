//! Mode 2 of BuildMenuAdviceText (0x557bbf), first part: the report header, monetary policy, crypto
//! prices, the industry outlook and the economy / GDP / stock market commentary.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::earnings::{calc_cur_month_of_year, calc_day_of_month};
use crate::logic::outlook::build_industry_outlook_text;
use crate::logic::report::number_to_words;
use crate::logic::valuation::{price_trend_direction, price_trend_streak};

/// Mode 2, first part (0x557bbf).
pub(super) fn mode2_economy(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> R {
    let __content = crate::content::current();
    if g.start_year == g.cur_year {
        g.help_text = g.help_text.clone()
            + __content.string(1167);
        c.phrase = s(
            __content.string(1168),
        );
        c.clause = PbStr::new();
        if 2 < g.sel_industry && g.sel_industry < 70 {
            c.clause = s(__content.string(1169));
        }
        if 0 < g.sel_industry {
            if g.sel_industry > 71 {
                return Err(Abort(9));
            }
            c.phrase = s(__content.string(1170))
                + &c.clause
                + __content.string(1171)
                + &g.ind_name[g.sel_industry]
                + ".";
        }
        if 0 < has(&c.phrase, "&") {
            repl(&mut c.phrase, "&", "AND");
        }
        g.help_text = g.help_text.clone() + &c.phrase + CRLF2;
    } else {
        c.month_in_qtr = crate::pb::numeric::double(calc_cur_month_of_year(g, g.sim_co_count));
        c.trend_a = to_long(ceil(c.month_in_qtr));
        c.month_frac = crate::pb::numeric::double(frac(c.month_in_qtr));
        c.day_of_month = calc_day_of_month(g, c.trend_a, c.month_frac);
        c.override_day = to_long(g.tmp_ext1);
        g.tmp_ext1 = Ext::from_f64(0.0);
        if 1499 < g.sim_co_count
            || g.sim_co_count <= g.num_players
            || g.sim_co_count == 500
            || g.sim_co_count == 1000
        {
            c.day_of_month = c.override_day;
        }
        if !(1..=12).contains(&c.trend_a) {
            return Err(Abort(9));
        }
        c.phrase = g.month_name[c.trend_a].clone()
            + &str_s(cast::<Ext, _>(c.day_of_month))
            + ","
            + &str_s(cast::<Ext, _>(g.cur_year));
        g.help_text = s("... ")
            + &c.phrase
            + __content.string(1172)
            + CRLF2;
    }
    c.phrase = s("Neutral");
    if g.monetary_policy == 0 {
        c.phrase = s(__content.string(1173));
    } else {
        g.qe_start_year = 0;
    }
    if g.monetary_policy == 0 {
        if g.qe_start_year == 0 {
            g.qe_start_year = g.cur_year;
        }
        if g.qe_start_year < g.cur_year {
            c.phrase = s(__content.string(1174)) + &c.phrase;
        }
    }
    if g.monetary_policy == 2 {
        c.phrase = s(__content.string(1175));
    }
    g.help_text = g.help_text.clone()
        + &ucase(g.central_bank_name.as_bytes(), CaseMode::Ansi)
        + __content.string(1176)
        + &c.phrase
        + CRLF2
        + __content.string(1177)
        + "\r\n"
        + __content.string(1178)
        + &fmt_trim(cast::<Ext, _>(g.bitcoin_price), "* ###,")
        + __content.string(1179)
        + &fmt_trim(cast::<Ext, _>(g.ethereum_price), "* ###,")
        + "..."
        + CRLF2;
    if 0 < g.sel_industry {
        g.ind_outlook_text = PbStr::new();
        build_industry_outlook_text(g, ui, rng, g.sel_industry);
        if !g.ind_outlook_text.is_empty() {
            c.phrase = s(__content.string(1180)) + &g.ind_outlook_text;
            if 69 < g.sel_industry {
                repl(&mut c.phrase, "OUTLOOK", "INFORMATION");
            }
            g.help_text = g.help_text.clone() + &c.phrase;
        }
    }
    economy_commentary(g, rng, c)?;
    Ok(())
}

fn economy_commentary(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) -> R {
    let __content = crate::content::current();
    c.phrase = s(__content.string(1181));
    c.clause = PbStr::new();
    c.paragraph = PbStr::new();
    c.trend_b = to_long(g.cap_hist_close[(0, 1604)] - Ext::from_f64(12.0));
    if c.trend_b < 1 {
        c.trend_b += 60;
    }
    if !(0..=60).contains(&c.trend_b) {
        return Err(Abort(9));
    }
    c.wealth_ratio = g.cap_hist_close[(c.trend_b, 1604)];
    if -Ext::from_f64(1.0) <= c.wealth_ratio {
        c.trend_b = to_long(g.cap_hist_close[(0, 1604)] - Ext::from_f64(6.0));
        if c.trend_b < 1 {
            c.trend_b += 60;
        }
        if !(0..=60).contains(&c.trend_b) {
            return Err(Abort(9));
        }
        c.wealth_ratio = g.cap_hist_close[(c.trend_b, 1604)];
        c.trend_b = if -Ext::from_f64(1.0) <= c.wealth_ratio { 0 } else { -6 };
        if Ext::from_f64(4.9) < c.wealth_ratio {
            c.trend_b = 666;
        }
    } else {
        c.trend_b = -12;
    }
    c.case_val = cast::<Ext, _>(g.gdp_growth);
    let growth = cast::<Ext, _>(g.gdp_growth);
    let target = cast::<Ext, _>(g.gdp_growth_target);
    if -Ext::from_f64(6.0) <= c.case_val {
        if -sf(0.05f32) <= c.case_val {
            if c.case_val == Ext::from_f64(0.0) {
                c.clause =
                    s(__content.string(1182));
                if Ext::from_f64(2.5) < target {
                    c.paragraph = s(__content.string(1183));
                }
            } else if Ext::from_f64(1.0) <= c.case_val {
                if Ext::from_f64(2.0) <= c.case_val {
                    if Ext::from_f64(4.5) <= c.case_val {
                        c.clause = s(__content.string(1184))
                            + &fmt_trim(growth, "##.0")
                            + __content.string(1185);
                        if 2 < g.cur_quarter {
                            repl(&mut c.clause, "rapid", "robust");
                        }
                        if growth + Ext::from_f64(1.0) < target {
                            c.paragraph = s(
                                __content.string(1186),
                            );
                            if sf(1.35f32) < cast::<Ext, _>(g.oil_price) {
                                repl(&mut c.paragraph, "higher", __content.string(1187));
                            }
                        }
                        if target < growth - Ext::from_f64(1.0) && price_trend_direction(g, 1604, 2, 1) < 0 {
                            c.paragraph = s(__content.string(1188));
                        }
                        if 0 < price_trend_direction(g, 1604, 2, 1)
                            && 0 < price_trend_direction(g, 1604, 4, 4)
                            && price_trend_direction(g, 1604, 6, 6) < 0
                        {
                            c.paragraph = s(
                                __content.string(1189),
                            );
                        }
                        if c.paragraph.is_empty() && rng.rnd() + Ext::from_f64(9.0) < cast::<Ext, _>(g.base_interest_rate) {
                            c.paragraph = s(__content.string(1190));
                            if Ext::from_f32(11.0f32) < g.base_interest_rate {
                                repl(&mut c.paragraph, "steep", "lofty");
                            }
                            if rng.rnd() * sf(0.15f32) + sf(1.3f32) < cast::<Ext, _>(g.oil_price) {
                                c.paragraph = c.paragraph.clone() + __content.string(1191);
                            }
                        }
                    } else {
                        c.clause =
                            s(__content.string(1192)) + &fmt_trim(growth, "##.0") + "%";
                        if sf(1.3f32) < cast::<Ext, _>(g.oil_target_price)
                            && sf(8.75f32) < cast::<Ext, _>(g.interest_rate_target)
                            && target < growth
                            && Ext::from_f64(1.0) < target
                        {
                            c.paragraph = s(__content.string(1193));
                            c.gdp_sustainable = s("N");
                        }
                        if target < Ext::from_f64(1.0) && price_trend_direction(g, 1604, 2, 1) < 0 {
                            c.paragraph = s(__content.string(1194));
                        }
                        if Ext::from_f64(4.5) < target && 0 < price_trend_direction(g, 1604, 2, 1) {
                            c.paragraph = s(__content.string(1195));
                        }
                        if c.paragraph.is_empty() {
                            c.paragraph = s(__content.string(1196));
                            c.gdp_sustainable = s("Y");
                        }
                        if c.trend_b < 0 {
                            c.paragraph = s(
                                __content.string(1197),
                            );
                            if c.trend_b == -6 {
                                repl(&mut c.paragraph, "year", __content.string(1198));
                            }
                            if c.trend_b == -12 && c.wealth_ratio < -Ext::from_f64(9.0) {
                                repl(&mut c.paragraph, __content.string(1199), __content.string(1200));
                            }
                        }
                    }
                } else {
                    c.clause = s("growing");
                    c.clause = c.clause.clone()
                        + if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                            __content.string(1201)
                        } else {
                            " slowly"
                        };
                    c.clause = c.clause.clone() + ", at " + &fmt_trim(growth, "##.0") + "%";
                    if target < Ext::from_f64(0.5) && price_trend_direction(g, 1604, 2, 1) < 0 {
                        c.paragraph = s(__content.string(1202));
                    }
                    if Ext::from_f64(3.0) < target && 0 < price_trend_direction(g, 1604, 2, 1) {
                        c.paragraph = s(__content.string(1203));
                    }
                    if c.trend_b < 0 {
                        c.paragraph = s(__content.string(1204));
                        if c.trend_b == -6 {
                            repl(&mut c.paragraph, "year", __content.string(1198));
                        }
                        if c.trend_b == -12 && c.wealth_ratio < -Ext::from_f64(9.0) {
                            repl(&mut c.paragraph, __content.string(1199), __content.string(1205));
                        }
                    }
                }
            } else {
                c.clause = s(__content.string(1206));
                if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                    c.clause = s(__content.string(1207));
                }
                c.clause = c.clause.clone() + " at a " + &fmt_trim(growth, "##.0") + "% rate";
                if target < -Ext::from_f64(1.0) {
                    c.paragraph = s(__content.string(1208));
                    if c.wealth_ratio < Ext::from_f64(0.0) {
                        c.paragraph = c.paragraph.clone() + " again";
                    }
                }
                if Ext::from_f64(1.0) < target && 0 < price_trend_direction(g, 1604, 2, 1) {
                    c.paragraph = s(__content.string(1209));
                }
            }
        } else {
            c.clause = s(__content.string(1210))
                + &fmt_trim(growth, "##.0")
                + "%";
            c.clause = s(if -Ext::from_f64(2.5) <= growth { __content.string(1211) } else { "in a" }) + &c.clause;
            if growth.abs() < sf(0.05f32) {
                c.clause = s(__content.string(1212));
            }
            if Ext::from_f64(2.0) < target || 0 < price_trend_streak(g, 1604) {
                c.paragraph = s(__content.string(1213));
                if 2 < g.cur_quarter && -Ext::from_f64(2.0) < growth {
                    c.paragraph = s(__content.string(1214));
                }
            }
            if target < growth - Ext::from_f64(3.0) {
                c.paragraph = s(__content.string(1215));
            }
            if c.trend_b == 666 && growth < -Ext::from_f64(1.0) {
                c.paragraph =
                    s(__content.string(1216))
                        + &fmt_trim(c.wealth_ratio, "##.0")
                        + __content.string(1217);
            }
        }
    } else {
        c.clause = s(__content.string(1218))
            + &fmt_trim(growth, "##.0")
            + __content.string(1219);
        c.paragraph = s(__content.string(1220));
        if Ext::from_f64(0.0) < target {
            c.paragraph = s(
                __content.string(1221),
            );
        }
        if target < growth - Ext::from_f64(3.0) {
            c.paragraph = s(__content.string(1222));
            if g.cur_quarter == 3 {
                repl(&mut c.paragraph, __content.string(1223), __content.string(1224));
            }
        }
        c.trend_a = price_trend_streak(g, 1604);
        if 0 < c.trend_a {
            c.paragraph =
                s(__content.string(1225)) + &str_s(cast::<Ext, _>(c.trend_a)) + __content.string(1226);
        }
        if g.subprime_crisis_state < 1 {
            if 0 < g.pandemic_state && growth < -Ext::from_f64(8.0) {
                c.paragraph = c.paragraph.clone()
                    + __content.string(1227);
            }
        } else {
            c.paragraph = c.paragraph.clone()
                + __content.string(1228);
        }
    }
    c.phrase = c.phrase.clone() + &c.clause + &c.paragraph + ". ";
    g.help_text = g.help_text.clone() + &c.phrase;
    c.trend_a = price_trend_streak(g, 1604);
    c.paragraph = PbStr::new();
    if c.trend_a != 0 {
        if c.trend_a < 1 {
            c.phrase = s("tumbled");
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                c.phrase = s(__content.string(1058));
            }
            if Ext::from_f64(0.0) < growth {
                c.phrase = s("slowed");
            }
            if sf(1.9f32) < growth && c.trend_a < -4 {
                c.phrase = s(__content.string(1229));
            }
            if sf(3.8f32) < growth {
                c.phrase = s(__content.string(1057));
            }
        } else {
            c.phrase = s(__content.string(1112));
            if growth <= Ext::from_f64(0.0) {
                c.phrase = s(__content.string(1060));
            }
        }
        c.paragraph = s(__content.string(1230))
            + &c.phrase
            + __content.string(1115)
            + &number_to_words(c.trend_a, 0, 0)
            + __content.string(1095);
    }
    c.gdp_trend = price_trend_direction(g, 1604, 3, 3);
    c.trend_b = price_trend_direction(g, 1604, 6, 6);
    if c.gdp_trend != 0 && c.paragraph.is_empty() {
        if c.gdp_trend < 1 {
            c.phrase = s(__content.string(1231));
        } else {
            c.phrase = s("faster ");
            if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                c.phrase = c.phrase.clone() + "clip";
            } else {
                if g.rand_seed % 3 < 2 {
                    c.phrase = s(__content.string(1232));
                }
                if sf(1.8f32) < growth && growth < Ext::from_f64(3.0) {
                    c.phrase = s(__content.string(1233));
                }
                c.phrase = c.phrase.clone() + "pace";
            }
        }
        if growth <= Ext::from_f64(0.0) || c.trend_a != 0 {
            if growth < Ext::from_f64(0.0)
                && 0 < c.trend_b
                && -1 < c.trend_a
                && 0 < price_trend_direction(g, 1604, 2, 1)
            {
                c.paragraph = c.paragraph.clone()
                    + __content.string(1234);
            }
        } else {
            c.paragraph = s(__content.string(1235))
                + &c.phrase
                + __content.string(1236);
            if c.gdp_trend == c.trend_b {
                c.paragraph = c.paragraph.clone() + __content.string(1237);
            } else {
                c.paragraph = c.paragraph.clone() + ". ";
            }
            if c.gdp_trend < 1 || Ext::from_f64(2.0) <= growth || Ext::from_f64(3.0) < target {
                if c.gdp_sustainable == "Y" && 0 < has(&c.paragraph, "slower") {
                    repl(&mut c.paragraph, ".", __content.string(1238));
                }
                if c.gdp_sustainable == "N" && 0 < has(&c.paragraph, "faster") {
                    repl(&mut c.paragraph, ".", __content.string(1238));
                }
            } else {
                repl(&mut c.paragraph, "The", __content.string(1239));
            }
        }
    }
    if Ext::from_f32(0.0f32) < g.oil_crisis_level && growth < -Ext::from_f64(5.0) && Ext::from_f32(2.0f32) < g.oil_price {
        c.paragraph = c.paragraph.clone()
            + __content.string(1240);
        if 0 < g.oil_crisis_stage && g.oil_crisis_stage < 15 {
            c.paragraph = c.paragraph.clone()
                + __content.string(1241);
        }
        c.paragraph = c.paragraph.clone() + ". ";
    }
    if 0 < g.subprime_crisis_state {
        c.paragraph = c.paragraph.clone() + CRLF2;
        if growth <= Ext::from_f64(3.0) {
            c.paragraph = c.paragraph.clone() + __content.string(1242);
            if growth < -Ext::from_f64(5.0) {
                repl(
                    &mut c.paragraph,
                    __content.string(1243),
                    __content.string(1244),
                );
            }
        } else {
            c.paragraph = c.paragraph.clone()
                + __content.string(1245);
        }
        c.paragraph = c.paragraph.clone()
            + __content.string(1246);
        if 6 < g.subprime_crisis_state {
            repl(&mut c.paragraph, "ongoing", "growing");
        }
    }
    g.help_text = g.help_text.clone() + &c.paragraph;
    Ok(())
}

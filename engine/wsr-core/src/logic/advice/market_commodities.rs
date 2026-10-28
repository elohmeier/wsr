//! C [runtime resource wording] commentary within BuildMenuAdviceText (0x557bbf).
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::report::number_to_words;
use crate::logic::valuation::{price_pct_change_months, price_trend_direction, price_trend_streak};

/// Commodity and crypto commentary following the stock-market section.
pub(super) fn run(g: &mut Globals, _ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> R {
    prediction_and_oil(g, rng, c);
    metals(g, rng, c);
    grain(g, rng, c);
    cryptocurrency(g, rng, c);
    Ok(())
}

fn prediction_and_oil(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    c.phrase = PbStr::new();
    c.paragraph = PbStr::new();
    if cast::<Ext, _>(g.gdp_growth) > sf(4.2f32)
        && (103..=106).contains(&g.bubble_state)
        && g.monetary_stance == 1
        && price_trend_streak(g, 0) > 6
    {
        c.phrase = s(__content.string(1247))
            + Q
            + "bubble"
            + Q
            + __content.string(1248);
    } else {
        c.trend_a = 0;
        c.amount_a = cast::<Ext, _>(g.oil_price) * Ext::from_f64(100.0);
        c.amount_b = cast::<Ext, _>(g.oil_target_price) * Ext::from_f64(100.0);
        if c.amount_b <= c.amount_a * sf(1.1f32) {
            if c.amount_b < c.amount_a * sf(0.9f32) {
                c.trend_a = -1;
            }
        } else {
            c.trend_a = 1;
        }
        if (cast::<Ext, _>(g.prime_rate)) * sf(1.07f32) < cast::<Ext, _>(g.interest_rate_target) && 0 < g.monetary_policy
        {
            c.trend_a += 100;
        }
        if (cast::<Ext, _>(g.interest_rate_target)) < (cast::<Ext, _>(g.prime_rate)) * sf(0.93f32)
            && g.monetary_policy < 2
        {
            c.trend_a -= 100;
        }
        c.clause = s(__content.string(1249));
        if g.cur_quarter == 3 && 900 < g.sim_co_count {
            c.trend_a += 1;
        }
        if g.cur_quarter == 2 && g.sim_co_count < 500 {
            c.trend_a += 1;
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                c.trend_a -= 2;
            }
        }
        if g.cur_quarter == 1 && g.sim_co_count < 550 {
            c.trend_a -= 1;
        }
        c.trend_a = match c.trend_a {
            -102 => 101,
            102 => -101,
            -98 => 99,
            98 => -99,
            2 => -1,
            -2 => 1,
            x => x,
        };
        c.case_val = cast::<Ext, _>(c.trend_a);
        c.phrase = match c.trend_a {
            -101 | 101 => {
                s(__content.string(1250))
                    + if c.trend_a == 101 { "rise" } else { "fall" }
                    + __content.string(1251)
            }
            -100 | 100 => {
                s("a ")
                    + if c.trend_a == 100 { "rise" } else { "drop" }
                    + __content.string(1252)
            }
            -99 => s(__content.string(1253)),
            99 => s(__content.string(1254)),
            -1 | 1 => {
                c.paragraph = s(__content.string(1255))
                    + if g.gdp_growth >= Ext::from_f32(1.0f32) {
                        __content.string(1256)
                    } else {
                        __content.string(1257)
                    };
                if c.trend_a == 1 {
                    c.paragraph = s(__content.string(1258));
                }
                s(__content.string(1259)) + &c.paragraph
            }
            _ => PbStr::new(),
        };
        if !c.phrase.is_empty() {
            c.phrase = c.clause.clone() + &c.phrase;
        }
    }
    c.paragraph = PbStr::new();
    if cast::<Ext, _>(g.oil_price) <= rng.rnd() * sf(0.15f32) + sf(1.25f32) {
        if (cast::<Ext, _>(g.oil_price)) < rng.rnd() * sf(0.15f32) + sf(0.55f32) {
            c.paragraph = s(__content.string(1260));
        }
    } else {
        c.paragraph = s(__content.string(1261));
    }
    if !c.paragraph.is_empty() && (cast::<Ext, _>(g.oil_price)) < sf(0.47f32) {
        c.paragraph = s(__content.string(1262)) + &c.paragraph;
    }
    if !c.paragraph.is_empty() {
        c.phrase = c.phrase.clone()
            + __content.string(1263)
            + &c.paragraph
            + __content.string(1264);
    }
    if !c.phrase.is_empty() {
        g.help_text = g.help_text.clone() + CRLF2 + &c.phrase;
    }

    c.phrase = PbStr::new();
    c.paragraph = PbStr::new();
    c.trend_b = 0;
    c.trend_a = price_trend_direction(g, 6, 12, 3);
    if c.trend_a != 0 {
        if c.trend_a < 1 {
            c.phrase = s("dropped");
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                c.phrase = s(__content.string(1265));
            }
        } else {
            c.phrase = s("risen");
        }
        c.trend_b = 4;
        c.clause = s(__content.string(1266));
        c.paragraph = s("OK");
    }
    if price_trend_streak(g, 6).abs() > 5 {
        c.trend_b = price_trend_streak(g, 6);
        c.phrase = s(if c.trend_b < 1 { "fallen" } else { "gone up" });
        c.clause = s("months");
        c.paragraph = s("OK");
    }
    c.pct_change2 = price_pct_change_months(g, 6, 12);
    if c.paragraph == "OK" {
        c.paragraph = s(__content.string(1267))
            + &c.phrase
            + __content.string(1115)
            + &number_to_words(c.trend_b, 0, 0)
            + " "
            + &c.clause;
        if c.clause == __content.string(1266) && c.pct_change2 != 0 {
            c.clause = s(if c.trend_a < 0 { "a de" } else { "an in" });
            c.paragraph = c.paragraph.clone()
                + ", "
                + &c.clause
                + __content.string(1268)
                + &number_to_words(c.pct_change2, 0, 1)
                + __content.string(1269);
        } else {
            c.paragraph = c.paragraph.clone() + ". ";
        }
        g.help_text = g.help_text.clone() + CRLF2 + &c.paragraph;
    } else {
        c.idx = c.cur_month;
        c.pct_change2 = price_pct_change_months(g, 6, c.idx);
        if 2 < c.cur_month && c.cur_month < 12 && 9 < c.pct_change2.abs() {
            c.clause = s(if c.pct_change2 < 0 { "down" } else { "up" });
            c.paragraph = s(__content.string(1270))
                + &c.clause
                + &number_to_words(c.pct_change2, 0, 1)
                + __content.string(1271)
                + &trim(&str_s(cast::<Ext, _>(g.cur_year)))
                + "). ";
            if 0 < has(&g.help_text, __content.string(1261)) && c.pct_change2 < 0 {
                repl(&mut c.paragraph, "The", __content.string(1239));
            }
            g.help_text = g.help_text.clone() + CRLF2 + &c.paragraph;
        }
    }
}

fn cryptocurrency(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    c.phrase = PbStr::new();
    c.pct_change = 1605;
    c.paragraph = s("Bitcoin");
    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
        c.pct_change = 1606;
        c.paragraph = s(__content.string(1272));
    }
    c.trend_a = price_trend_streak(g, c.pct_change);
    c.trend_b = price_pct_change_months(g, c.pct_change, 12);
    if (4 < c.trend_a.abs() || 40 < c.trend_b.abs()) && g.cap_hist_close[(59, c.pct_change)] != Ext::from_f64(0.0)
    {
        c.phrase = s(__content.string(1273)) + &c.paragraph + " has ";
        if c.trend_a.abs() <= 4 {
            c.clause = s(if c.trend_b < 1 { __content.string(1274) } else { "soared " });
            repl(&mut c.phrase, "In", "In wild");
            c.phrase = c.phrase.clone()
                + &c.clause
                + "by"
                + &number_to_words(c.trend_b, 0, 1)
                + __content.string(1275);
        } else {
            c.clause = s(if c.trend_a < 1 { "fallen " } else { "risen " });
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) && c.trend_a < 0 {
                repl(&mut c.clause, "fallen ", __content.string(1276));
            }
            c.phrase = c.phrase.clone()
                + &c.clause
                + "for"
                + &number_to_words(c.trend_a, 0, 0)
                + __content.string(1277);
        }
        if !c.phrase.is_empty() {
            g.help_text = g.help_text.clone() + CRLF2 + &c.phrase;
        }
    }
}

fn metals(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    c.trend_a = price_trend_direction(g, 7, 36, 12);
    c.trend_b = price_trend_streak(g, 7);
    c.pct_change2 = price_pct_change_months(g, 7, 12);
    c.paragraph = PbStr::new();
    if c.trend_a != 0 || 5 < c.trend_b.abs() || 14 < c.pct_change2.abs() {
        if c.trend_a == 0 {
            if 5 < c.trend_b.abs() {
                c.phrase = s(if c.trend_b < 0 { "fell" } else { "rose" });
                c.paragraph = s(__content.string(1278))
                    + &c.phrase
                    + __content.string(1279)
                    + &fmt_trim(cast::<Ext, _>(g.gold_price), "* ####")
                    + __content.string(1280)
                    + &number_to_words(c.trend_b, 0, 0)
                    + __content.string(1095);
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    repl(
                        &mut c.paragraph,
                        __content.string(1281),
                        __content.string(1282),
                    );
                }
                if c.trend_b < 0 {
                    c.phrase = s(__content.string(1283));
                    if (g.cur_quarter + g.cur_year) % 3 == 1 {
                        repl(&mut c.phrase, __content.string(1284), __content.string(1285));
                    }
                    if (g.cur_quarter + g.cur_year) % 3 == 2 {
                        c.phrase = s(__content.string(1286));
                    }
                    if g.cur_quarter == 2 {
                        c.phrase = s(__content.string(1287));
                    }
                    c.paragraph = c.paragraph.clone()
                        + __content.string(1288)
                        + &c.phrase;
                    if 1 < (g.cur_quarter + g.cur_year) % 4 {
                        repl(&mut c.paragraph, "London", __content.string(1289));
                    }
                    if 2 < g.cur_quarter {
                        repl(&mut c.paragraph, "dealers", "traders");
                    }
                } else {
                    c.phrase = s(__content.string(1290));
                    if g.cur_quarter < 3 {
                        repl(&mut c.phrase, "India", "China");
                    }
                    if g.cur_quarter == 3 {
                        c.phrase =
                            s(__content.string(1291));
                    }
                    if g.cur_quarter == 4 {
                        c.phrase = s(__content.string(1292));
                    }
                    c.paragraph = c.paragraph.clone()
                        + __content.string(1293)
                        + &c.phrase;
                    if 1 < (g.cur_quarter + g.cur_year) % 4 {
                        repl(&mut c.paragraph, __content.string(1294), "Chicago");
                    }
                }
            }
        } else {
            c.phrase = s(if c.trend_a < 0 { "low" } else { "high" });
            c.paragraph = s(__content.string(1295))
                + &c.phrase
                + __content.string(1296);
        }
        c.phrase = PbStr::new();
        if 14 < c.pct_change2.abs() {
            if c.pct_change2 < 1 {
                c.phrase = s(__content.string(1297))
                    + &number_to_words(c.pct_change2, 0, 1)
                    + __content.string(1298);
                if instr(
                    1,
                    ucase(c.paragraph.as_bytes(), CaseMode::Ansi).as_bytes(),
                    b"GOLD",
                ) == 0
                {
                    repl(&mut c.phrase, __content.string(1299), __content.string(1300));
                }
            } else {
                c.phrase = s(__content.string(1301))
                    + &number_to_words(c.pct_change2, 0, 1)
                    + __content.string(1302);
            }
            if c.paragraph.is_empty() {
                repl(&mut c.phrase, "The", __content.string(1303));
            }
            c.paragraph = c.paragraph.clone() + &c.phrase;
        }
        c.pct_change = price_pct_change_months(g, 8, 12);
        if c.pct_change2.abs() < c.pct_change.abs() && 25 < c.pct_change.abs() {
            c.paragraph = PbStr::new();
        }
        if !c.paragraph.is_empty() {
            g.help_text = g.help_text.clone() + CRLF2 + &c.paragraph;
        }
    }
    c.paragraph = PbStr::new();
    c.pct_change = 0;
    c.trend_b = price_trend_streak(g, 8).abs();
    if 2 < c.trend_b {
        c.pct_change = price_pct_change_months(g, 8, c.trend_b);
    }
    if c.pct_change.abs() < 10 {
        c.pct_change = price_pct_change_months(g, 8, 2);
    }
    if c.pct_change.abs() < 10 {
        c.pct_change = 0;
    }
    if has(&g.help_text, "gold") == 0 || c.pct_change != 0 {
        c.trend_a = price_trend_direction(g, 8, 36, 12);
        c.trend_b = price_trend_streak(g, 8);
        c.pct_change2 = price_pct_change_months(g, 8, 12);
        if c.trend_a != 0
            || 5 < c.trend_b.abs()
            || 24 < c.pct_change2.abs()
            || 9 < c.pct_change.abs()
        {
            if c.trend_a == 0 {
                if c.trend_b.abs() <= 5 {
                    if c.pct_change != 0 {
                        c.phrase = s(__content.string(1304));
                        if 15 < c.pct_change {
                            c.phrase = s(__content.string(1305));
                        }
                        if c.pct_change < 0 {
                            c.phrase = s("is down");
                        }
                        if c.pct_change < -15 {
                            c.phrase = s(__content.string(1306));
                            if 0 < has(&g.help_text, "tumbled") {
                                c.phrase = s(__content.string(1307));
                            }
                        }
                        c.paragraph = s(if has(&g.help_text, "gold") == 0 {
                            __content.string(1308)
                        } else {
                            __content.string(1309)
                        });
                        c.paragraph = c.paragraph.clone()
                            + __content.string(1310)
                            + &c.phrase
                            + &number_to_words(c.pct_change, 0, 1)
                            + __content.string(1311);
                    }
                } else {
                    c.phrase = s(if c.trend_b < 0 { "down" } else { "up" });
                    c.paragraph = s(__content.string(1312))
                        + &c.phrase
                        + __content.string(1313)
                        + &number_to_words(c.trend_b, 0, 0)
                        + __content.string(1314)
                        + &fmt_trim(cast::<Ext, _>(g.silver_price), "* ##.00")
                        + __content.string(1315);
                    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                        repl(&mut c.paragraph, __content.string(1316), "ongoing");
                    }
                    if c.trend_b < 0 {
                        c.phrase = s(if g.gdp_growth < Ext::from_f32(1.0f32) {
                            __content.string(1317)
                        } else {
                            __content.string(1318)
                        });
                        c.paragraph = c.paragraph.clone()
                            + __content.string(1319)
                            + &c.phrase;
                    } else {
                        c.phrase = s(
                            __content.string(1320),
                        );
                        if 20 < price_pct_change_months(g, 8, 2) {
                            c.phrase = s(
                                __content.string(1321),
                            );
                        }
                        c.paragraph = c.paragraph.clone() + __content.string(1322) + &c.phrase;
                    }
                }
            } else {
                c.phrase = s(if c.trend_a < 0 {
                    __content.string(1323)
                } else {
                    __content.string(1324)
                });
                c.paragraph = s(__content.string(1325))
                    + &c.phrase
                    + __content.string(1326);
                if price_trend_direction(g, 8, 48, 12) != 0 {
                    repl(&mut c.paragraph, "third", "fourth");
                }
            }
        }
        c.phrase = PbStr::new();
        if 24 < c.pct_change2.abs() && c.pct_change == 0 {
            if c.pct_change2 < 0 {
                c.phrase = s(__content.string(1327))
                    + &number_to_words(c.pct_change2, 0, 1)
                    + __content.string(1328);
                if c.paragraph.is_empty() && has(&g.help_text, __content.string(1329)) == 0 {
                    repl(&mut c.phrase, "The", __content.string(1330));
                }
                if 0 < c.trend_b {
                    c.phrase = c.phrase.clone() + __content.string(1331);
                    if c.trend_b < 6 {
                        c.pct_change2 = price_pct_change_months(g, 8, c.trend_b);
                        c.phrase = c.phrase.clone()
                            + __content.string(1332)
                            + &number_to_words(c.pct_change2, 0, 1)
                            + __content.string(1094)
                            + &number_to_words(c.trend_b, 0, 0);
                    } else {
                        c.phrase = c.phrase.clone() + __content.string(1333);
                    }
                    c.phrase = c.phrase.clone() + " months";
                }
                c.phrase = c.phrase.clone() + ". ";
            } else if 0 < c.pct_change2 {
                c.phrase = s(__content.string(1334))
                    + &number_to_words(c.pct_change2, 0, 1)
                    + __content.string(1335);
                if c.paragraph.is_empty() {
                    repl(&mut c.phrase, "Silver", __content.string(1336));
                }
                if c.trend_b < 0 {
                    c.phrase = c.phrase.clone()
                        + __content.string(1337)
                        + &number_to_words(c.trend_b, 0, 0)
                        + " months";
                }
                c.phrase = c.phrase.clone() + ". ";
            }
            c.paragraph = c.paragraph.clone() + &c.phrase;
        }
        if !c.paragraph.is_empty() {
            g.help_text = g.help_text.clone() + CRLF2 + &c.paragraph;
        }
    }
}

fn grain(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    c.trend_a = price_trend_direction(g, 9, 36, 12);
    c.trend_b = price_trend_streak(g, 9);
    c.paragraph = PbStr::new();
    if c.trend_a != 0 || 5 < c.trend_b.abs() {
        if c.trend_a == 0 {
            c.phrase = s(if c.trend_b < 0 { "off" } else { "up" });
            c.paragraph = s(__content.string(1338))
                + &c.phrase
                + __content.string(1279)
                + &fmt_trim(cast::<Ext, _>(g.wheat_price), "* ##.00")
                + __content.string(1339)
                + &number_to_words(c.trend_b, 0, 0)
                + __content.string(1095);
            if c.trend_b < 0 {
                if sf(5.75f32) <= cast::<Ext, _>(g.wheat_price) {
                    if sf(8.35f32) < cast::<Ext, _>(g.wheat_price) {
                        c.paragraph = c.paragraph.clone()
                            + __content.string(1340);
                    }
                } else {
                    c.paragraph = c.paragraph.clone()
                        + __content.string(1341);
                }
            } else {
                c.paragraph = c.paragraph.clone()
                    + __content.string(1342);
            }
        } else {
            c.phrase = s(if c.trend_a < 0 { "slide" } else { "rise" });
            c.paragraph = s(__content.string(1343))
                + &c.phrase
                + __content.string(1344);
            c.phrase = PbStr::new();
            if c.trend_b != 0 {
                c.phrase = s(__content.string(1345))
                    + &number_to_words(c.trend_b, 0, 0)
                    + " months";
            }
            if c.trend_a < 1 || -1 < c.trend_b {
                if c.trend_a < 0 && c.trend_b < 0 {
                    c.phrase = PbStr::new();
                } else if c.trend_a < 1 || c.trend_b < 1 {
                    if c.trend_a < 0 && 0 < c.trend_b {
                        repl(&mut c.phrase, "decline", "recover");
                    }
                } else {
                    c.phrase = PbStr::new();
                }
            }
            c.paragraph = c.paragraph.clone() + &c.phrase + ". ";
        }
    }
    c.pct_change = 12;
    c.pct_change2 = price_pct_change_months(g, 9, c.pct_change);
    if 10 < c.pct_change2
        && 5 < c.trend_b
        && c.pct_change2 < price_pct_change_months(g, 9, c.trend_b)
    {
        c.pct_change2 = price_pct_change_months(g, 9, c.trend_b);
        c.pct_change = c.trend_b;
    }
    if c.paragraph.is_empty() {
        if 19 < c.pct_change2.abs() {
            c.phrase = s(if c.pct_change2 < 0 { "down" } else { "up" });
            c.paragraph = s(__content.string(1346))
                + &c.phrase
                + &number_to_words(c.pct_change2, 0, 1)
                + __content.string(1347);
            if 100 < c.pct_change2 && c.pct_change2 < 150 {
                repl(&mut c.paragraph, "is", __content.string(1348));
            }
            if 200 < c.pct_change2 && c.pct_change2 < 250 {
                repl(&mut c.paragraph, "is", __content.string(1349));
            }
            c.trend_a = price_pct_change_months(g, 9, 3);
            if 0 < c.pct_change2 && c.trend_a < -7 {
                c.paragraph = c.paragraph.clone()
                    + __content.string(1350)
                    + &number_to_words(c.trend_a, 0, 1)
                    + __content.string(1351);
            }
            if c.pct_change2 < 0 && 7 < c.trend_a {
                c.phrase = match g.rand_seed % 4 {
                    1 => s(__content.string(1352)),
                    2 => s(__content.string(1353)),
                    3 => s(__content.string(1108)),
                    _ => s(__content.string(1080)),
                };
                if g.rand_seed % 4 == 3 && rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    c.phrase = c.phrase.clone() + __content.string(1354);
                }
                c.paragraph = c.paragraph.clone()
                    + __content.string(1355)
                    + &c.phrase
                    + &number_to_words(c.trend_a, 0, 1)
                    + __content.string(1351);
            }
            c.paragraph = c.paragraph.clone() + ". ";
        }
    } else if 10 < c.pct_change2.abs() {
        if c.pct_change2 < 1 {
            c.phrase = s(__content.string(1356))
                + &number_to_words(c.pct_change2, 0, 1)
                + __content.string(1357);
        } else {
            c.phrase = s(__content.string(1358))
                + &number_to_words(c.pct_change2, 0, 1)
                + __content.string(1094)
                + &number_to_words(c.pct_change, 0, 0)
                + __content.string(1095);
            if 100 < c.pct_change2 && c.pct_change2 < 150 {
                repl(&mut c.phrase, "gone", __content.string(1359));
            }
            if 200 < c.pct_change2 && c.pct_change2 < 250 {
                repl(&mut c.phrase, "gone", __content.string(1360));
            }
        }
        c.paragraph = c.paragraph.clone() + &c.phrase;
    }
    if !c.paragraph.is_empty() {
        g.help_text = g.help_text.clone() + CRLF2 + &c.paragraph;
        c.gdp_para_shown = 1;
    }
    c.trend_a = price_trend_direction(g, 10, 24, 12);
    c.trend_b = price_trend_streak(g, 10);
    c.idx = c.cur_month;
    c.pct_change2 = 0;
    if 3 < c.cur_month {
        c.pct_change2 = price_pct_change_months(g, 10, c.idx);
    }
    c.paragraph = PbStr::new();
    c.pct_change = 0;
    if c.trend_a != 0 || 5 < c.trend_b.abs() || 10 < c.pct_change2.abs() {
        if c.trend_a == 0 {
            if c.trend_b != 0 {
                c.phrase = s("up");
                c.pct_change = 1;
                if c.trend_b < 0 {
                    c.phrase = s("down");
                    c.pct_change = -1;
                }
                c.paragraph = s(__content.string(1361))
                    + &c.phrase
                    + __content.string(1362)
                    + &number_to_words(c.trend_b, 0, 0)
                    + __content.string(1095);
                if c.trend_b < 0 && (cast::<Ext, _>(g.corn_price)) < sf(4.4f32) {
                    c.paragraph = c.paragraph.clone()
                        + __content.string(1363);
                } else if 0 < c.trend_b {
                    c.phrase = s(
                        __content.string(1364),
                    );
                    if g.cur_quarter < 2 {
                        c.phrase = s(
                            __content.string(1365),
                        );
                    }
                    c.paragraph = c.paragraph.clone() + &c.phrase;
                }
            }
        } else {
            c.phrase = s(__content.string(1366));
            if c.trend_a < 0 {
                c.phrase = s(__content.string(1367));
                c.pct_change = -1;
            }
            c.snippet = s(if c.gdp_para_shown < 1 {
                __content.string(1368)
            } else {
                __content.string(1369)
            });
            c.paragraph = c.snippet.clone()
                + __content.string(1370)
                + &c.phrase
                + __content.string(1371)
                + &fmt_trim(cast::<Ext, _>(g.corn_price), "* ##.00")
                + __content.string(1372);
            if price_trend_direction(g, 10, 36, 12) != 0 {
                repl(&mut c.paragraph, "second", "third");
            }
        }
        c.phrase = PbStr::new();
        if c.pct_change2.abs() <= 17 {
            c.pct_change = 6;
            if 6 < c.trend_b.abs() {
                c.pct_change = c.trend_b.abs();
            }
            c.pct_change2 = price_pct_change_months(g, 10, c.pct_change);
            if c.pct_change < 12 && c.pct_change2.abs() < price_pct_change_months(g, 10, 12).abs() {
                c.pct_change2 = price_pct_change_months(g, 10, 12);
                c.pct_change = 12;
            }
            if 15 < c.pct_change2.abs() {
                c.phrase = s(__content.string(1373))
                    + &number_to_words(c.pct_change2, 0, 1)
                    + __content.string(1374)
                    + &number_to_words(c.pct_change, 0, 0)
                    + " months";
                if c.pct_change2 < 0 {
                    repl(&mut c.phrase, "is up", __content.string(1375));
                }
                if c.gdp_para_shown == 0 && c.paragraph.is_empty() {
                    repl(&mut c.phrase, "The", __content.string(1376));
                }
                c.trend_a = price_pct_change_months(g, 10, 3);
                if 0 < c.pct_change2 && c.trend_a < -7 {
                    c.phrase = c.phrase.clone()
                        + __content.string(1377)
                        + &number_to_words(c.trend_a, 0, 1)
                        + __content.string(1351);
                }
                if c.pct_change2 < 0 && 7 < c.trend_a {
                    c.phrase = c.phrase.clone()
                        + __content.string(1378)
                        + &number_to_words(c.trend_a, 0, 1)
                        + __content.string(1351);
                    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) && 13 < c.trend_a {
                        repl(&mut c.phrase, __content.string(1379), __content.string(1380));
                        if (cast::<Ext, _>(g.corn_price)) < sf(4.7f32) {
                            repl(&mut c.phrase, "back", __content.string(1381));
                        }
                        if 0 < g.rand_seed % 3 {
                            repl(&mut c.phrase, "roar", "ralli");
                        }
                    }
                    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                        repl(&mut c.phrase, __content.string(1375), __content.string(1382));
                    }
                }
                c.phrase = c.phrase.clone() + ". ";
            }
        } else if c.pct_change2 < 1 {
            c.phrase = s(__content.string(1383))
                + &number_to_words(c.pct_change2, 0, 1)
                + __content.string(1384)
                + &str_s(cast::<Ext, _>(g.cur_year))
                + ". ";
            if c.gdp_para_shown == 0 && c.paragraph.is_empty() {
                repl(&mut c.phrase, "The", __content.string(1385));
            }
        } else {
            c.phrase = s(__content.string(1386))
                + &number_to_words(c.pct_change2, 0, 1)
                + __content.string(1104);
            if c.pct_change == -1 {
                c.phrase = c.phrase.clone() + __content.string(1387);
            } else if c.pct_change == 0 && c.trend_b < 0 {
                c.phrase = c.phrase.clone()
                    + __content.string(1388)
                    + &number_to_words(c.trend_b, 0, 0)
                    + " months";
            }
            if c.gdp_para_shown == 0 && c.paragraph.is_empty() {
                repl(&mut c.phrase, "Corn", __content.string(1389));
            }
            if rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(30.0) < cast::<Ext, _>(c.pct_change2) {
                repl(&mut c.phrase, " up", __content.string(1390));
            }
            c.phrase = c.phrase.clone() + ". ";
        }
        c.paragraph = c.paragraph.clone() + &c.phrase;
        if !c.paragraph.is_empty() {
            g.help_text = g.help_text.clone() + CRLF2 + &c.paragraph;
        }
    }
}

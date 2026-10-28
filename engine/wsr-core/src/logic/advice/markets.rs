//! Mode 2 of BuildMenuAdviceText (0x557bbf): interest rates, markets, commodities and growth.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::report::number_to_words;
use crate::logic::valuation::{price_trend_direction, price_trend_streak};

/// Mode 2, market commentary (0x557bbf).
pub(super) fn mode2_markets(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> R {
    interest_rates(g, c);
    stock_market(g, rng, c);
    super::market_commodities::run(g, ui, rng, c)?;
    sustained_growth(g);
    Ok(())
}

/// Interest-rate commentary, beginning at 0x562529 in BuildMenuAdviceText.
fn interest_rates(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    let mut paragraph = PbStr::new();
    c.trend_a = price_trend_streak(g, 1602);
    if c.trend_a != 0 {
        let direction = if c.trend_a < 1 { "fallen" } else { "risen" };
        paragraph =
            s(__content.string(1391))
                + direction
                + " for"
                + &number_to_words(c.trend_a, 0, 0)
                + __content.string(1392);
        if cast::<Ext, _>(g.base_interest_rate) > Ext::from_f64(12.5)
            && (cast::<Ext, _>(g.gdp_growth)) < sf(1.9f32)
            && g.oil_crisis_level == Ext::from_f32(0.0f32)
        {
            paragraph = paragraph
                + __content.string(1393);
            if g.gdp_growth > Ext::from_f32(0.0f32) {
                repl(&mut paragraph, "malaise", __content.string(1394));
            }
        }
    }
    if c.trend_a.abs() < 6 {
        c.phrase = PbStr::new();
        c.trend_b = to_long(g.cap_hist_close[(0, 1601)] - Ext::from_f64(12.0));
        if c.trend_b < 1 {
            c.trend_b += 60;
        }
        c.amount = g.cap_hist_close[(c.trend_b, 1601)];
        if c.amount > Ext::from_f64(0.0) {
            c.clause = fmt_trim(c.amount, "#################.00") + "%";
            c.trend_a = price_trend_direction(g, 1601, 12, 12);
            let prime = cast::<Ext, _>(g.prime_rate);
            if c.trend_a > 0 && prime - c.amount > Ext::from_f64(2.0) {
                c.phrase = s("risen");
            } else if c.trend_a < 0 && c.amount - prime > Ext::from_f64(2.0) {
                c.phrase = s("fallen");
            }
            if !c.phrase.is_empty() {
                paragraph = s(__content.string(1395))
                    + &c.phrase
                    + __content.string(1396)
                    + &c.clause
                    + " a year ago, to "
                    + &fmt_trim(prime, "#################.00")
                    + __content.string(1397);
                if (c.amount - prime).abs() > sf(2.4f32) {
                    repl(&mut paragraph, __content.string(1398), "sharp");
                    if (c.amount - prime).abs() > sf(4.8f32) {
                        repl(&mut paragraph, "sharp", __content.string(1399));
                    }
                }
            }
            if cast::<Ext, _>(g.base_interest_rate) > Ext::from_f64(12.5)
                && (cast::<Ext, _>(g.gdp_growth)) < sf(1.9f32)
                && has(&paragraph, "malaise") == 0
                && has(&paragraph, "anemic") == 0
            {
                paragraph = paragraph + __content.string(1400);
                if (cast::<Ext, _>(g.gdp_growth)) < Ext::from_f64(2.0) {
                    paragraph = paragraph + __content.string(1401);
                    if (cast::<Ext, _>(g.gdp_growth)) < -Ext::from_f64(2.5) {
                        repl(
                            &mut paragraph,
                            __content.string(1402),
                            __content.string(1403),
                        );
                    }
                }
                paragraph = paragraph + ". ";
            }
        }
    }
    if !paragraph.is_empty() {
        g.help_text = g.help_text.clone() + CRLF2 + &paragraph;
    }
}

/// Final assessment after the market and commodity paragraphs.
fn sustained_growth(g: &mut Globals) {
    let __content = crate::content::current();
    let oil = cast::<Ext, _>(g.oil_price);
    let prime = cast::<Ext, _>(g.prime_rate);
    let tax = g.corp_tax_rate;
    let mut score = 0;
    if oil < sf(0.8f32) {
        score += 1;
    } else if oil >= sf(1.2f32) {
        score -= 1;
    }
    if prime < sf(6.5f32) {
        score += 1;
    } else if prime > sf(8.75f32) {
        score -= 1;
    }
    if tax < 26 {
        score += 1;
    } else if tax > 42 {
        score -= 1;
    }
    let phrase = if g.gdp_growth > Ext::from_f32(5.0f32) {
        __content.string(1404)
    } else if g.gdp_growth < Ext::from_f32(1.0f32) {
        __content.string(1405)
    } else {
        __content.string(1406)
    };
    let clause = if score >= 2 {
        let text = if score == 3 {
            __content.string(1407)
        } else if oil >= sf(0.8f32) {
            if oil >= sf(1.2f32) {
                __content.string(1408)
            } else {
                __content.string(1409)
            }
        } else if prime >= sf(6.5f32) {
            __content.string(1410)
        } else {
            __content.string(1411)
        };
        s(__content.string(1412)) + text
    } else if score <= -2 {
        let text = if score == -3 {
            if oil > Ext::from_f64(2.0) {
                __content.string(1413)
            } else {
                __content.string(1414)
            }
        } else if oil < sf(1.2f32) {
            __content.string(1415)
        } else if prime > sf(8.75f32) {
            if oil > Ext::from_f64(2.0) {
                __content.string(1416)
            } else {
                __content.string(1417)
            }
        } else if tax < 46 {
            if oil > Ext::from_f64(2.0) {
                __content.string(1418)
            } else {
                __content.string(1419)
            }
        } else if oil > Ext::from_f64(2.0) {
            __content.string(1420)
        } else {
            __content.string(1421)
        };
        s(__content.string(1422)) + text
    } else if oil > Ext::from_f64(1.5) && prime > Ext::from_f64(9.5) {
        s(
            __content.string(1423),
        )
    } else if oil < sf(0.6f32) && prime < Ext::from_f64(6.0) {
        s(__content.string(1424))
    } else {
        let energy = if oil > Ext::from_f64(2.0) {
            __content.string(1425)
        } else if oil >= sf(1.2f32) {
            "high"
        } else if oil < sf(0.8f32) {
            "low"
        } else {
            __content.string(1426)
        };
        let rates = if prime > sf(8.75f32) {
            "high"
        } else if prime < sf(6.5f32) {
            "low"
        } else {
            __content.string(1427)
        };
        let taxes = if tax < 26 {
            __content.string(1428)
        } else if tax > 46 {
            __content.string(1429)
        } else if tax > 32 {
            __content.string(1430)
        } else {
            __content.string(1426)
        };
        s(__content.string(1431))
            + energy
            + __content.string(1432)
            + rates
            + __content.string(1433)
            + taxes
            + __content.string(1434)
    };
    g.help_text = g.help_text.clone() + CRLF2 + phrase + &clause;
}

/// Stock-index commentary, including sustained trends and year-over-year changes.
fn stock_market(g: &mut Globals, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    use crate::logic::valuation::price_pct_change_months as pct;
    let mut a = price_trend_streak(g, 0);
    let mut b = pct(g, 0, 12);
    let mut years = 0;
    if price_trend_direction(g, 0, 36, 12) == 1 && pct(g, 0, 36) > 25 {
        years = 3;
    }
    if price_trend_direction(g, 0, 48, 12) == 1 && pct(g, 0, 48) > 35 {
        years = 4;
        if pct(g, 0, 59) > pct(g, 0, 48) && pct(g, 0, 59) > 45 {
            years = 5;
        }
    }
    let mut phrase = PbStr::new();
    let mut clause = PbStr::new();
    let mut paragraph = PbStr::new();
    if a.abs() <= 5 {
        if b.abs() <= 7 {
            if b.abs() < 4 && a == 0 && pct(g, 0, 1).abs() < 4 && g.start_year < g.cur_year {
                paragraph = s(
                    __content.string(1435),
                );
            } else if pct(g, 0, 1).abs() > 4 {
                b = pct(g, 0, 1);
                phrase = if b < -4 { s("off") } else { s("up") };
                paragraph = s(__content.string(1436))
                    + &phrase
                    + &number_to_words(b, 0, 1)
                    + __content.string(1437);
                if b < -4 && b > -7 {
                    repl(&mut paragraph, "are off", __content.string(1438));
                }
            }
        } else {
            phrase = s("risen");
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                phrase = s("gained");
            }
            if b > 15 {
                phrase = s("surged");
            }
            if b > 22 {
                phrase = s(__content.string(1439));
            }
            if b < 0 {
                phrase = s(__content.string(1057));
            }
            if b < -15 {
                phrase = s("plunged");
            }
            paragraph = s(__content.string(1440))
                + &phrase
                + &number_to_words(b, 0, 1)
                + __content.string(1347);
            if b > 0 && a > 0 {
                if g.peak_oil_crisis == 1 {
                    paragraph = paragraph
                        + __content.string(1441);
                } else if g.gdp_growth < -Ext::from_f32(3.0f32)
                    && (cast::<Ext, _>(g.base_interest_rate)) < sf(3.7f32)
                    && g.monetary_policy < 2
                {
                    paragraph = paragraph
                        + __content.string(1442);
                    if g.currency_code != "USA" {
                        paragraph =
                            replace(paragraph.as_bytes(), b"Fed", g.central_bank_name.as_bytes());
                    }
                    if cast::<Ext, _>(g.market_valuation) > sf(1.15f32)
                        && g.market_val_target < cast::<Ext, _>(g.market_valuation) + sf(0.03f32)
                    {
                        paragraph = paragraph
                            + __content.string(1443);
                    }
                } else if g.gdp_growth < -Ext::from_f32(2.0f32)
                    && g.gdp_growth > -Ext::from_f32(10.0f32)
                    && g.base_interest_rate < Ext::from_f32(8.0f32)
                    && b >= 16
                {
                    paragraph = paragraph
                        + __content.string(1444);
                    if g.currency_code != "USA" {
                        paragraph =
                            replace(paragraph.as_bytes(), b"Fed", g.central_bank_name.as_bytes());
                    }
                } else if g.gdp_growth <= -Ext::from_f32(10.0f32) && b >= 11 {
                    paragraph = paragraph + __content.string(1445);
                    if g.subprime_crisis_state > 0 {
                        paragraph = paragraph + __content.string(1446);
                    }
                    paragraph = paragraph + ". ";
                } else if g.gdp_growth > Ext::from_f32(4.7f32) && b >= 11 {
                    paragraph = paragraph
                        + if g.cur_quarter < 3 {
                            __content.string(1447)
                        } else {
                            __content.string(1448)
                        };
                } else if g.gdp_growth > -Ext::from_f32(2.5f32) && g.gdp_growth < Ext::from_f32(1.8f32) {
                    paragraph =
                        paragraph + __content.string(1449);
                    if g.cur_quarter >= 3 || has(&g.help_text, "anemic") > 0 {
                        repl(&mut paragraph, "anemic", __content.string(1450));
                    }
                }
            } else if b < -10
                && a < 0
                && (cast::<Ext, _>(g.market_valuation)) < sf(0.85f32)
                && (cast::<Ext, _>(g.market_valuation)) < g.market_val_target
            {
                paragraph = paragraph
                    + __content.string(1451);
            }
            if a == 0 {
                let direction = price_trend_direction(g, 0, 2, 1);
                if direction < 0 {
                    a = -2;
                } else if direction > 0 {
                    a = 2;
                }
            }
            if b > 0 && a < -1 {
                paragraph = paragraph
                    + __content.string(1452)
                    + &number_to_words(a, 0, 0)
                    + " months";
            } else if b < 0 && a > 0 {
                paragraph = paragraph
                    + __content.string(1453)
                    + &number_to_words(a, 0, 0)
                    + " months";
                if a < 3 {
                    paragraph = paragraph
                        + if g.cur_quarter % 2 == 0 {
                            __content.string(14392)
                        } else {
                            __content.string(1454)
                        };
                } else {
                    paragraph = paragraph + __content.string(1455);
                }
            }
            if right(paragraph.as_bytes(), 2) != ". " {
                paragraph = paragraph + ". ";
            }
        }
    } else {
        let extra;
        if a < 1 {
            phrase = s(match g.cur_quarter {
                1 => "down",
                2 => __content.string(1456),
                3 => __content.string(1457),
                _ => "sinking",
            });
            extra = s(match g.cur_quarter {
                1 => "continues to tumble",
                3 => "continues to fall",
                _ => __content.string(1458),
            });
            if c.gdp_trend > 0 {
                clause = s(if g.gdp_growth <= Ext::from_f32(0.0f32) {
                    __content.string(1459)
                } else {
                    __content.string(1460)
                });
            }
            if (cast::<Ext, _>(g.market_valuation)) < sf(0.83f32)
                && g.market_val_target - (cast::<Ext, _>(g.market_valuation)) > sf(0.07f32)
                && (cast::<Ext, _>(g.interest_rate_target)) < cast::<Ext, _>(g.base_interest_rate)
            {
                clause = clause
                    + __content.string(1461);
            }
            if clause.is_empty() {
                clause = s(". ");
            }
        } else {
            phrase = s(match g.cur_quarter {
                1 => "rising",
                2 => "up",
                3 => "surging",
                _ => __content.string(1462),
            });
            let mut x = s(__content.string(1463));
            if years > 2 {
                x = s(__content.string(1464)) + &number_to_words(years, 0, 0) + __content.string(1465);
            }
            if c.gdp_trend < 0 {
                x = s(__content.string(1466));
                clause = s(__content.string(1467));
                if g.gdp_growth < Ext::from_f32(1.0f32) {
                    repl(&mut clause, "slow", "weaken");
                }
                if g.gdp_growth < -Ext::from_f32(8.0f32) {
                    clause = s(__content.string(1468));
                } else if g.gdp_growth < -Ext::from_f32(3.0f32) {
                    clause = s(__content.string(1469));
                }
                if g.base_interest_rate < Ext::from_f32(4.0f32) && g.monetary_policy < 2 {
                    clause = clause
                        + __content.string(1470);
                    if g.currency_code != "USA" {
                        clause = replace(clause.as_bytes(), b"Fed", g.central_bank_name.as_bytes());
                    }
                }
            } else if c.gdp_trend > 0 && g.gdp_growth > Ext::from_f32(3.5f32) && b > 20 {
                x = s(__content.string(1471));
                clause = s(__content.string(1472));
                if g.base_interest_rate < Ext::from_f32(5.75f32) {
                    clause = clause + __content.string(1473);
                }
            }
            if g.gdp_growth < Ext::from_f32(0.0f32)
                && g.base_interest_rate < Ext::from_f32(4.0f32)
                && clause.is_empty()
                && g.monetary_policy == 0
            {
                clause = s(__content.string(1474))
                    + &g.central_bank_name
                    + __content.string(1475);
            }
            clause = clause + ". ";
            if g.bubble_state < 100
                && g.market_valuation > Ext::from_f32(1.2f32)
                && g.market_val_target - (cast::<Ext, _>(g.market_valuation)) < sf(0.05f32)
            {
                clause = clause + __content.string(1476);
                clause = clause
                    + if b < 26 {
                        __content.string(1477)
                    } else {
                        __content.string(1478)
                    };
            }
            extra = x;
        }
        paragraph = s(__content.string(1479))
            + &extra
            + __content.string(1480)
            + &phrase
            + __content.string(1481)
            + &number_to_words(a, 0, 0)
            + " months"
            + &clause;
        if price_trend_direction(g, 0, 36, 12) < 0 && pct(g, 0, 36) < -20 && a < 0 {
            paragraph = paragraph
                + if price_trend_direction(g, 0, 48, 12) < 0 {
                    __content.string(14393)
                } else {
                    __content.string(1482)
                };
        }
    }
    if phrase.is_empty() && clause.is_empty() && (-5..=-3).contains(&a) {
        clause = s(__content.string(1483))
            + &number_to_words(a, 0, 0)
            + __content.string(1095);
    } else {
        clause = PbStr::new();
    }
    phrase = s(__content.string(1484));
    if a < -5 || years < 3 {
        if a < 6 && price_trend_direction(g, 0, 36, 12) < 0 {
            years = if price_trend_direction(g, 0, 48, 12) < 0 {
                4
            } else {
                3
            };
            repl(&mut phrase, "bull", "bear");
            repl(&mut phrase, "climb", "fall");
        } else {
            phrase = PbStr::new();
        }
    } else {
        if has(&paragraph, "climb") > 0 {
            repl(&mut phrase, __content.string(1462), __content.string(1485));
        }
        phrase = phrase + &clause;
    }
    if has(&paragraph, __content.string(1486)) > 0 || has(&paragraph, "bear ") > 0 {
        phrase = PbStr::new();
    }
    if !phrase.is_empty() {
        if years == 4 {
            repl(&mut phrase, "three", "four");
        }
        if years == 5 {
            repl(&mut phrase, "three", "five");
        }
    }
    paragraph = paragraph + &phrase;
    let five_year = pct(g, 0, 59).abs();
    if five_year < 6 && five_year > 0 {
        paragraph = paragraph
            + __content.string(1487);
    }
    c.trend_a = a;
    c.trend_b = b;
    c.pct_change = years;
    c.phrase = phrase;
    c.clause = clause;
    if !paragraph.is_empty() {
        g.help_text = g.help_text.clone() + CRLF2 + &paragraph;
    }
}

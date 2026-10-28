//! Modes 3..7 of BuildMenuAdviceText (0x557bbf): advice for the borrowing / misc (3), buy-sell (4),
//! financing (5), management (6) and other transactions (7) menus.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::{End, ent, holding_co_tax_note};
use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_ceo_pay;
use crate::logic::finance::calc_entity_financials;
use crate::logic::sim_entity::ai_corp::is_recent_offering;
use crate::logic::valuation::{
    calc_analyst_rating, calc_industry_market_share, calc_industry_outlook,
    get_mgmt_margin_vs_industry,
};

/// Mode 3 (0x557bbf).
pub(super) fn mode3(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> R {
    let __content = crate::content::current();
    let e = ent(c.entity_id)?;
    let a = ent(g.active_entity)?;
    if c.entity_id < 11 {
        c.phrase = PbStr::new();
    } else if 85 < c.entity_id {
        c.phrase = s(__content.string(1488));
    }
    if c.entity_id < 11 || 85 < c.entity_id {
        g.help_text = s(__content.string(1489))
            + __content.string(1490)
            + &c.phrase
            + ", for "
            + &g.active_entity_name
            + "."
            + CRLF2;
    }
    calc_entity_financials(g, ui, rng, c.entity_id);
    if g.cur_player == g.active_entity || 85 < g.active_entity {
        c.phrase = s("you own");
        if 85 < g.active_entity {
            c.phrase = g.ent_symbol[a].clone() + " owns";
        }
        g.help_text = g.help_text.clone()
            + __content.string(1491)
            + Q
            + __content.string(1492)
            + Q
            + __content.string(1493)
            + &c.phrase
            + __content.string(1494);
        if g.active_entity < 131 || 69 < g.industry_num[a] {
            g.help_text = g.help_text.clone() + "." + CRLF2;
        } else {
            g.help_text = g.help_text.clone()
                + __content.string(1495)
                + Q
                + __content.string(1496)
                + Q
                + __content.string(1497)
                + CRLF2;
        }
        c.phrase = c.currency_prefix.clone()
            + &fmt(g.unused_credit_line, "#################,.00")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &c.currency_suffix
            + ". ";
        if g.unused_credit_line == Ext::from_f64(0.0) {
            c.phrase = s("ZERO. ");
            if 6 < g.credit_rating_num[e] {
                let lender = ent(g.lender_id)?;
                if g.credit_frozen[e] == 1 && g.cur_player == g.corp_controller[lender] {
                    g.credit_frozen[e] = 0;
                }
                if g.credit_frozen[e] == 1 && g.cur_player != g.corp_controller[lender] {
                    c.phrase = c.phrase.clone()
                        + &g.lender_bank_name
                        + __content.string(1498)
                        + &g.ent_symbol[e]
                        + __content.string(1499);
                } else {
                    c.phrase = c.phrase.clone()
                        + __content.string(1500)
                        + &g.lender_bank_name
                        + __content.string(1501);
                }
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) || c.entity_id < 11 {
                    c.phrase = c.phrase.clone()
                        + __content.string(1502);
                } else if 10 < c.entity_id && g.bonds_issued[e] == 0 {
                    c.phrase =
                        c.phrase.clone() + __content.string(1503);
                }
            }
        }
        g.help_text = g.help_text.clone()
            + &g.ent_symbol[e]
            + __content.string(1504)
            + &c.phrase
            + CRLF2;
        if g.cash[e] <= Ext::from_f64(0.0) && g.t_bills[e] == Ext::from_f64(0.0) {
            c.phrase = s(if 10 < c.entity_id {
                __content.string(1505)
            } else {
                __content.string(1506)
            });
            g.help_text = g.help_text.clone()
                + &c.phrase
                + __content.string(1507)
                + CRLF2;
        }
        if Ext::from_f64(100.0) < g.cash[e] && Ext::from_f64(100.0) < g.bank_debt[e] && 4 < g.credit_rating_num[e] {
            c.amount_a = cast::<Ext, _>(g.borrow_rate - g.t_bill_rate);
            c.phrase = g.ent_name[e].clone() + " has";
            c.clause = s("its");
            if g.cur_player == g.active_entity {
                c.phrase = s(__content.string(1506));
                c.clause = s("your");
            }
            g.help_text = s(__content.string(1508))
                + &c.phrase
                + __content.string(14394)
                + &c.clause
                + __content.string(1509);
            if Ext::from_f64(0.0) < g.t_bills[e]
                && g.cash[e] < g.bank_debt[e]
                && sf(0.05f32) < cast::<Ext, _>(g.borrow_rate - g.t_bill_rate)
            {
                c.phrase = g.ent_name[e].clone();
                if g.cur_player == g.active_entity {
                    c.phrase = s("You");
                }
                g.help_text = g.help_text.clone()
                    + " "
                    + &c.phrase
                    + __content.string(1510)
                    + &fmt_trim(c.amount_a, "##.00")
                    + __content.string(1511);
            }
            g.help_text = g.help_text.clone() + CRLF2;
        }
        if g.cur_player == g.active_entity {
            c.phrase = s(__content.string(1512))
                + Q
                + __content.string(1513)
                + Q
                + __content.string(1514);
            if Ext::from_f64(0.0) < g.tmp_player_advances {
                c.phrase = c.phrase.clone()
                    + __content.string(1515)
                    + Q
                    + __content.string(1516)
                    + Q
                    + __content.string(1517);
            }
            g.help_text = g.help_text.clone() + &c.phrase + CRLF2;
        }
    }
    if g.active_entity < 11 {
        g.help_text = g.help_text.clone()
            + __content.string(1518);
    } else {
        g.help_text = g.help_text.clone()
            + __content.string(1519)
            + Q
            + __content.string(1520)
            + Q
            + __content.string(1521)
            + &g.ent_name[e]
            + __content.string(1522);
    }
    g.help_text = g.help_text.clone() + CRLF2;
    if 10 < g.active_entity && g.active_entity < 86 {
        g.help_text = g.help_text.clone()
            + &g.active_entity_name
            + __content.string(1523)
            + Q
            + __content.string(1524)
            + Q
            + " and "
            + Q
            + __content.string(1525)
            + Q
            + __content.string(14395)
            + CRLF2
            + __content.string(1491)
            + Q
            + __content.string(1492)
            + Q
            + __content.string(1526)
            + &g.ent_symbol[a]
            + __content.string(1527)
            + CRLF2
            + __content.string(1528)
            + Q
            + "CLOSE"
            + Q
            + __content.string(1529)
            + &g.ent_symbol[a]
            + "."
            + CRLF2;
    }
    if 10 < g.sel_entity {
        let selected = ent(g.sel_entity)?;
        g.help_text = g.help_text.clone()
            + __content.string(1530)
            + &g.ent_symbol[selected]
            + __content.string(1531)
            + CRLF2;
    }
    g.help_text = g.help_text.clone()
        + __content.string(1532)
        + Q
        + __content.string(1533)
        + Q
        + __content.string(1534)
        + CRLF2
        + __content.string(1535);
    Ok(())
}

/// Mode 4 (0x557bbf). `End::Exit` where the original leaves without the final clean-up.
pub(super) fn mode4(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    c: &mut Ctx,
) -> Result<End, Abort> {
    let __content = crate::content::current();
    let a = ent(g.active_entity)?;
    let e = ent(c.entity_id)?;
    g.help_text = g.help_text.clone()
        + __content.string(1536)
        + &g.ent_symbol[a]
        + __content.string(1537);
    c.phrase =
        s(__content.string(1538));
    if 10 < g.active_entity {
        c.phrase = s(__content.string(1539));
    }
    g.help_text = g.help_text.clone() + &c.phrase + CRLF2;
    if g.active_entity < 11 {
        calc_entity_financials(g, ui, rng, g.active_entity);
    } else {
        c.analyst_rating = calc_analyst_rating(g, ui, rng, g.active_entity);
    }
    c.extra_text = s(") ");
    if g.active_entity < 11 || 85 < g.active_entity {
        c.extra_text = s(__content.string(1540));
    }
    c.phrase = s(__content.string(1541))
        + &g.ent_symbol[a]
        + __content.string(1542)
        + &c.extra_text
        + "of "
        + &c.currency_prefix
        + &fmt_trim(g.liquid_funds, "#################,.00")
        + " "
        + &g.million_prefix_lc
        + __content.string(1543)
        + Q
        + &g.credit_rating_str
        + "."
        + Q;
    if Ext::from_f64(0.0) < g.unused_credit_line {
        c.phrase = c.phrase.clone() + __content.string(1544);
        if Ext::from_f64(0.005) <= (g.cash[a] + g.t_bills[a]).abs() {
            c.phrase = c.phrase.clone()
                + &c.currency_prefix
                + &fmt_trim(g.cash[a] + g.t_bills[a], "#################,.00")
                + " "
                + &g.million_prefix_lc
                + "illion.";
        } else {
            c.phrase = c.phrase.clone() + "ZERO.";
        }
    }
    c.phrase = c.phrase.clone() + CRLF2;
    if 10 < g.active_entity {
        let ind = cast::<i32, _>(g.industry_num[a]);
        if !(0..=71).contains(&ind) {
            return Err(Abort(9));
        }
        c.phrase = c.phrase.clone()
            + __content.string(1545)
            + &g.active_entity_name
            + __content.string(1546)
            + &g.ind_name[ind]
            + __content.string(1547)
            + CRLF2;
    }
    c.clause = PbStr::new();
    c.extra_text = PbStr::new();
    if g.active_entity < 131 {
        if Ext::from_f32(6.0f32) <= g.base_interest_rate {
            if Ext::from_f32(9.5f32) < g.base_interest_rate
                && (g.interest_rate_target < Ext::from_f32(10.0f32) || Ext::from_f32(11.0f32) < g.base_interest_rate)
            {
                c.clause =
                    s(__content.string(1548))
                        + CRLF2;
            }
        } else {
            if Ext::from_f64(0.0) < g.tmp_corp_bonds_value {
                c.extra_text = s(__content.string(1549));
            }
            if g.tmp_corp_bonds_value < g.tmp_gov_bonds_value {
                c.extra_text = s(__content.string(1550));
            }
            if !c.extra_text.is_empty() {
                c.clause = s(__content.string(1551))
                    + &g.ent_symbol[a]
                    + __content.string(1552)
                    + &c.extra_text
                    + __content.string(1553)
                    + CRLF2;
            }
        }
    } else if g.industry_num[a] < 70 {
        c.trend_b = 0;
        for idx in 131..=1600 {
            if g.entity_status[idx] == 1
                && g.industry_num[idx] == g.industry_num[a]
                && g.active_entity != idx
                && g.cur_player == g.corp_controller[idx]
            {
                update_corp_control(g, ui, rng, idx);
                if cast::<i32, _>(g.wholly_owned_by) != g.active_entity {
                    update_corp_control(g, ui, rng, g.active_entity);
                    if cast::<i32, _>(g.wholly_owned_by) != idx {
                        c.trend_b = idx;
                        if g.rand_seed % 3 == 1 {
                            break;
                        }
                    }
                }
            }
        }
        if 130 < c.trend_b {
            c.clause = s(__content.string(1554))
                + &g.active_entity_name
                + " with "
                + &g.ent_name[c.trend_b]
                + __content.string(1555)
                + &g.ind_name[cast::<i32, _>(g.industry_num[a])]
                + __content.string(1547)
                + CRLF2;
        }
    }
    c.extra_text = PbStr::new();
    if 10 < g.active_entity && 1 < g.credit_rating_num[a] {
        c.amount = g.market_cap[a] * Ext::from_f64(100.0);
        if !(c.amount <= g.net_worth * Ext::from_f64(3.0) || c.analyst_rating < 3) {
            c.extra_text = g.ent_symbol[e].clone()
                + __content.string(1556)
                + CRLF2;
        } else if c.amount < g.net_worth * sf(0.82f32)
            && 4 < g.credit_rating_num[a]
            && 130 < g.active_entity
        {
            update_corp_control(g, ui, rng, g.active_entity);
            if 0 < g.public_pct && c.analyst_rating < 4 {
                c.extra_text = s(__content.string(1557))
                    + Q
                    + __content.string(1558)
                    + Q
                    + __content.string(1559)
                    + &g.active_entity_name
                    + __content.string(14396)
                    + Q
                    + __content.string(1560)
                    + Q
                    + __content.string(1561)
                    + CRLF2;
            }
        }
    }
    c.idx = 0;
    for slot in 1..=15 {
        if 10 < g.hold_stock_id[(slot, e)] && 0 < g.hold_stock_pct[(slot, e)] {
            c.idx += 1;
        }
    }
    if 0 < c.idx {
        c.trend_b = 0;
        while c.trend_b == 0 {
            c.trend_a = rng.rnd_range(15, 1);
            c.trend_b = cast::<i32, _>(g.hold_stock_id[(c.trend_a, e)]);
            if g.hold_stock_pct[(c.trend_a, e)] < 1 {
                c.trend_b = 0;
            }
            c.loop_guard += 1;
            if c.loop_guard == 999 {
                ui.msgbox(
                    &(s(__content.string(1562))
                        + Q
                        + "OK"
                        + Q
                        + __content.string(1563)),
                    0,
                    &PbStr::new(),
                );
                return Ok(End::Exit);
            }
        }
        calc_entity_financials(g, ui, rng, c.trend_b);
        if 1 < g.credit_rating_num[ent(c.trend_b)?] {
            c.trend_b = 0;
            for slot in 1..=c.idx {
                c.trend_b = cast::<i32, _>(g.hold_stock_id[(slot, e)]);
                if g.hold_stock_pct[(slot, e)] < 0 {
                    c.trend_b = 0;
                }
                if 10 < c.trend_b {
                    calc_entity_financials(g, ui, rng, c.trend_b);
                    if g.credit_rating_num[ent(c.trend_b)?] < 2 {
                        break;
                    }
                }
            }
        }
        let mut distressed = false;
        if 10 < c.trend_b && g.credit_rating_num[ent(c.trend_b)?] < 2 {
            let t = ent(c.trend_b)?;
            c.extra_text = c.extra_text.clone()
                + "The "
                + &g.ent_name[t]
                + __content.string(1564)
                + &g.ent_symbol[e]
                + __content.string(14397);
            c.pct_change2 = 0;
            for slot in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, a)]) == c.trend_b {
                    c.pct_change2 = cast::<i32, _>(g.hold_stock_pct[(slot, a)]);
                }
            }
            if g.difficulty_level < 2 || c.pct_change2 != 100 {
                c.extra_text = c.extra_text.clone() + __content.string(1565);
            } else {
                c.extra_text = c.extra_text.clone()
                    + __content.string(1566)
                    + &g.ent_symbol[t]
                    + __content.string(1567)
                    + &g.ent_name[t]
                    + __content.string(1568);
            }
            c.extra_text = c.extra_text.clone() + CRLF2;
            distressed = true;
        }
        if !distressed {
            c.trend_b = 0;
            c.loop_guard = 0;
            while c.trend_b == 0 {
                c.trend_a = rng.rnd_range(15, 1);
                c.trend_b = cast::<i32, _>(g.hold_stock_id[(c.trend_a, e)]);
                if g.hold_stock_pct[(c.trend_a, e)] < 1 {
                    c.trend_b = 0;
                }
                c.loop_guard += 1;
                if c.loop_guard == 999 {
                    ui.msgbox(
                        &(s(__content.string(1569))
                            + Q
                            + "OK"
                            + Q
                            + __content.string(1563)),
                        0,
                        &PbStr::new(),
                    );
                    return Ok(End::Exit);
                }
            }
            let t = ent(c.trend_b)?;
            c.trend_a = calc_analyst_rating(g, ui, rng, c.trend_b);
            update_corp_control(g, ui, rng, c.trend_b);
            if c.trend_a < 4 {
                if c.trend_a == 1 && 0 < g.public_pct {
                    calc_entity_financials(g, ui, rng, g.active_entity);
                    c.extra_text = c.extra_text.clone()
                        + __content.string(1570)
                        + &g.ent_name[t]
                        + __content.string(1571)
                        + Q
                        + __content.string(1572)
                        + Q
                        + " "
                        + &g.ent_name[e]
                        + __content.string(1573)
                        + &g.ent_symbol[t]
                        + ")";
                    if g.liquid_funds <= g.market_cap[t] * sf(1.1f32) {
                        c.extra_text = c.extra_text.clone()
                            + ", if "
                            + &g.ent_name[a]
                            + __content.string(1574);
                    }
                    c.extra_text = c.extra_text.clone() + "." + CRLF2;
                }
            } else if 0 < g.public_pct || g.difficulty_level == 1 {
                c.paragraph = if c.trend_a == 5 {
                    s("STRONG ")
                } else {
                    PbStr::new()
                };
                c.extra_text = c.extra_text.clone()
                    + &g.ent_name[t]
                    + __content.string(1575)
                    + Q
                    + &c.paragraph
                    + "SELL"
                    + Q
                    + __content.string(1576)
                    + &g.ent_symbol[e]
                    + __content.string(1577)
                    + &g.ent_symbol[t]
                    + __content.string(1578)
                    + CRLF2;
            }
        }
    }
    if c.entity_id < 131 {
        c.idx = g.hold_bond_id[(0, e)];
        if 0 < c.idx {
            c.trend_b = 0;
            while c.trend_b == 0 {
                c.trend_a = rng.rnd_range(15, 1);
                c.trend_b = g.hold_bond_id[(c.trend_a, e)];
            }
            if 0 < c.trend_b && Ext::from_f64(60.0) <= g.bond_price[ent(c.trend_b)?].get() && 3 < c.idx {
                c.trend_b = 0;
                while c.trend_b == 0 {
                    c.trend_a = rng.rnd_range(15, 1);
                    c.trend_b = g.hold_bond_id[(c.trend_a, e)];
                }
            }
            if 0 < c.trend_b && g.bond_price[ent(c.trend_b)?].get() < Ext::from_f64(60.0) {
                calc_entity_financials(g, ui, rng, c.trend_b);
                if g.credit_rating_str == "D" {
                    c.extra_text = c.extra_text.clone()
                        + "The "
                        + &g.ent_name[ent(c.trend_b)?]
                        + __content.string(1579)
                        + &g.ent_symbol[e]
                        + __content.string(1580)
                        + CRLF2;
                }
            }
        }
    }
    c.clause = c.clause.clone() + &c.extra_text;
    holding_co_tax_note(g, ui, rng, c);
    c.clause = c.clause.clone() + &c.holding_co_tax_note;
    if g.start_year != g.cur_year && !c.clause.is_empty() {
        g.help_text = c.phrase.clone() + __content.string(1508) + &c.clause;
    } else if g.start_year < g.cur_year {
        g.help_text = c.phrase.clone();
    } else {
        g.help_text = g.help_text.clone() + &c.phrase;
    }
    Ok(End::Cleanup)
}

/// Mode 5 (0x557bbf).
pub(super) fn mode5(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> R {
    let __content = crate::content::current();
    let a = ent(g.active_entity)?;
    let e = ent(c.entity_id)?;
    if g.active_entity < 11 {
        calc_entity_financials(g, ui, rng, g.active_entity);
    } else {
        c.analyst_rating = calc_analyst_rating(g, ui, rng, g.active_entity);
    }
    c.clause = PbStr::new();
    if g.cur_player == g.active_entity {
        c.phrase = s(__content.string(1581))
            + &g.ent_symbol[a]
            + __content.string(1582)
            + CRLF2;
    } else if 10 < g.active_entity {
        c.phrase = s(__content.string(1581))
            + &g.ent_symbol[a]
            + __content.string(1583)
            + Q
            + __content.string(1584)
            + Q
            + __content.string(1585)
            + CRLF2;
        if g.credit_rating_num[a] < 2 {
            c.clause = s(__content.string(1586))
                + &g.active_entity_name
                + __content.string(1587);
            if 1 < g.industry_num[a] && 0 < g.bonds_issued[a] && Ext::from_f64(0.0) < g.bank_debt[a] {
                c.clause = c.clause.clone() + __content.string(1588);
            }
            c.clause = c.clause.clone() + ".";
        } else if g.credit_rating_num[a] < 7
            && 0 < g.hold_stock_id[(0, a)]
            && !(2 < g.industry_num[a] && 4 < g.credit_rating_num[a])
        {
            c.clause = s(__content.string(1589))
                + &g.active_entity_name
                + __content.string(1590);
        }
        if !c.clause.is_empty() {
            c.clause = c.clause.clone() + CRLF2;
        }
    }
    g.help_text = g.help_text.clone() + &c.phrase + &c.clause;
    c.clause = PbStr::new();
    if 10 < g.active_entity {
        c.trend_a = is_recent_offering(g, g.active_entity);
        if c.trend_a == 0 {
            c.amount = g.market_cap[a] * Ext::from_f64(100.0);
            if g.net_worth * Ext::from_f64(3.0) < c.amount && 1 < g.credit_rating_num[a] && 2 < c.analyst_rating {
                c.clause = g.ent_symbol[e].clone()
                    + __content.string(1591)
                    + CRLF2;
            }
        } else {
            c.extra_text = s(if c.trend_a == 1 { "public" } else { "private" });
            c.clause = g.ent_symbol[e].clone()
                + __content.string(1592)
                + &c.extra_text
                + __content.string(1593);
            if c.trend_a == 1 {
                c.clause =
                    c.clause.clone() + __content.string(1594);
            }
            c.clause = c.clause.clone() + __content.string(1595) + CRLF2;
        }
    }
    c.extra_text = PbStr::new();
    if 10 < g.active_entity {
        if Ext::from_f32(6.25f32) <= g.base_interest_rate {
            if Ext::from_f32(9.25f32) < g.base_interest_rate
                && (Ext::from_f32(9.75f32) < g.interest_rate_target || Ext::from_f32(11.25f32) < g.base_interest_rate)
            {
                c.amount = g.bond_price[e].get();
                c.trend_a = 0;
                if c.amount < Ext::from_f64(88.0) && 0 < g.bonds_issued[e] && 0 < g.bonds_public_held {
                    c.trend_a = 1;
                }
                if c.amount < Ext::from_f64(84.0) && 0 < g.bonds_issued[e] {
                    c.trend_a = 1;
                }
                if c.trend_a == 1 {
                    c.extra_text = s(__content.string(1596))
                        + &fmt_trim(c.amount, "##")
                        + __content.string(1597)
                        + CRLF2;
                    if g.credit_rating_str == "D" && Ext::from_f64(0.0) < g.bank_debt[e] {
                        c.extra_text = PbStr::new();
                    }
                }
            }
        } else if g.bonds_issued[e] == 0 && 4 < g.credit_rating_num[a] {
            c.extra_text = s(__content.string(1598))
                + &g.active_entity_name
                + __content.string(1599)
                + CRLF2;
        }
    }
    holding_co_tax_note(g, ui, rng, c);
    c.clause = c.clause.clone() + &c.extra_text + &c.holding_co_tax_note;
    if g.start_year != g.cur_year && !c.clause.is_empty() {
        g.help_text = s(__content.string(1508)) + &c.clause;
    }
    Ok(())
}

/// Mode 6 (0x557bbf).
pub(super) fn mode6(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> R {
    let __content = crate::content::current();
    let a = ent(g.active_entity)?;
    let e = ent(c.entity_id)?;
    calc_entity_financials(g, ui, rng, g.active_entity);
    if g.cur_player == g.active_entity {
        c.phrase = s(__content.string(1600))
            + &g.active_entity_name
            + __content.string(1601)
            + Q
            + __content.string(1602)
            + Q
            + __content.string(1603)
            + Q
            + __content.string(1604)
            + Q
            + __content.string(1605);
    } else if 10 < g.active_entity {
        c.phrase = s(__content.string(1606))
            + &g.ent_symbol[a]
            + __content.string(1607)
            + CRLF2
            + __content.string(1608)
            + CRLF2;
        if g.active_entity < 86 {
            c.phrase = c.phrase.clone()
                + __content.string(1609)
                + Q
                + __content.string(1610)
                + Q
                + __content.string(1611)
                + CRLF2;
        }
        c.phrase = c.phrase.clone()
            + __content.string(1612)
            + &g.active_entity_name
            + __content.string(1613)
            + &g.active_entity_name
            + __content.string(1614);
    }
    c.phrase = c.phrase.clone() + CRLF2;
    c.clause = PbStr::new();
    c.best_ceo_corp = 0;
    c.best_ceo_pay = Ext::from_f64(0.0);
    for idx in 11..=1600 {
        if g.entity_status[idx] == 1 && g.cur_player == g.corp_controller[idx] {
            c.amount_a = calc_ceo_pay(g, ui, rng, idx, 0);
            c.amount_a += g.ceo_bonus_multiple * c.amount_a;
            if c.best_ceo_pay < c.amount_a {
                c.best_ceo_corp = idx;
                c.best_ceo_pay = c.amount_a;
            }
        }
    }
    c.trend_a = g.player_ceo_of[g.cur_player];
    if 10 < c.trend_a && g.entity_status[ent(c.trend_a)?] != 1 {
        c.trend_a = 0;
        g.player_ceo_of[g.cur_player] = 0;
        g.player_ceo_since_year[g.cur_player] = 0;
    }
    c.amount_b = calc_ceo_pay(g, ui, rng, c.trend_a, 0);
    c.amount_b += g.ceo_bonus_multiple * c.amount_b;
    if 10 < g.active_entity {
        c.amount_a = calc_ceo_pay(g, ui, rng, c.entity_id, 0);
        c.amount_a += g.ceo_bonus_multiple * c.amount_a;
        if g.player_ceo_of[g.cur_player] == 0 {
            c.clause = s(
                __content.string(1615),
            ) + &g.active_entity_name
                + __content.string(1616)
                + Q
                + __content.string(1617)
                + &g.ent_symbol[a]
                + Q
                + __content.string(1618)
                + CRLF2;
        } else if g.active_entity != g.player_ceo_of[g.cur_player] && c.amount_b < c.amount_a {
            c.clause = s(__content.string(1619))
                + &g.ent_symbol[ent(c.trend_a)?]
                + __content.string(14398)
                + &c.currency_prefix
                + &fmt_trim(c.amount_b, "###############,.0")
                + " "
                + &g.million_prefix_lc
                + __content.string(1620)
                + &c.currency_prefix
                + &fmt_trim(c.amount_a, "###############,.0")
                + " "
                + &g.million_prefix_lc
                + __content.string(1621)
                + &g.active_entity_name
                + " ("
                + &g.ent_symbol[a]
                + __content.string(1622)
                + CRLF2;
        }
    }
    c.extra_text = PbStr::new();
    if 10 < c.best_ceo_corp
        && c.best_ceo_corp != g.player_ceo_of[g.cur_player]
        && g.active_entity != c.best_ceo_corp
    {
        c.clause = s(__content.string(1623))
            + &c.currency_prefix
            + &fmt_trim(c.amount_b, "###############,.0")
            + " "
            + &g.million_prefix_lc
            + "illion";
        if 10 < g.player_ceo_of[g.cur_player] {
            c.clause =
                c.clause.clone() + ", from " + &g.ent_name[ent(g.player_ceo_of[g.cur_player])?];
        }
        c.clause = c.clause.clone() + ".";
        if g.player_ceo_of[g.cur_player] == 0 {
            c.clause = s(
                __content.string(1624),
            );
        }
        c.clause = c.clause.clone()
            + __content.string(1625)
            + &g.ent_name[c.best_ceo_corp]
            + " ("
            + &g.ent_symbol[c.best_ceo_corp]
            + ").";
        update_corp_control(g, ui, rng, c.best_ceo_corp);
        if g.public_pct == 0 {
            c.extra_text = s(__content.string(1626))
                + &g.ent_symbol[c.best_ceo_corp]
                + __content.string(1627);
        }
        c.clause = c.clause.clone() + &c.extra_text + CRLF2;
    }
    c.extra_text = PbStr::new();
    c.trend_b = get_mgmt_margin_vs_industry(g, c.entity_id, 0);
    if 10 < g.active_entity && c.trend_b < -6 && g.profit_margin[a] < -2 && g.industry_num[a] < 70 {
        c.extra_text = s(__content.string(1628)) + &g.active_entity_name;
        if g.active_entity == g.player_ceo_of[g.cur_player] {
            c.extra_text = c.extra_text.clone() + __content.string(1629);
        }
        c.extra_text = c.extra_text.clone() + __content.string(1630);
        c.paragraph = right((s("000") + &trim(&str_s(cast::<Ext, _>(c.entity_id)))).as_bytes(), 4);
        c.trend_a = has(&g.recent_mgmt_change_list, &c.paragraph.to_display());
        if c.trend_a == 0 {
            c.trend_a = has(&g.new_mgmt_list, &c.paragraph.to_display());
        }
        if c.trend_a == 0 {
            c.extra_text = c.extra_text.clone()
                + __content.string(1631)
                + Q
                + __content.string(1632)
                + Q
                + __content.string(1633);
        }
        c.paragraph = PbStr::new();
        if 2 < g.industry_num[a] && Ext::from_f64(200.0) <= g.business_assets[a] && 4 < g.credit_rating_num[a] {
            c.paragraph = s(__content.string(1634))
                + &g.ent_symbol[e]
                + __content.string(1635);
            if g.disc_spend_pct[a] < 6 {
                c.paragraph = c.paragraph.clone() + __content.string(1636);
            }
            c.paragraph = c.paragraph.clone() + ".";
        }
        if right(c.extra_text.as_bytes(), 13) == __content.string(1637) && c.paragraph.is_empty() {
            c.extra_text = PbStr::new();
        }
        if !c.extra_text.is_empty() {
            c.extra_text = c.extra_text.clone() + &c.paragraph + CRLF2;
        }
    }
    c.clause = c.clause.clone() + &c.extra_text;
    c.extra_text = PbStr::new();
    if 130 < g.active_entity && g.industry_num[a] < 70 {
        c.trend_b = 1;
        for idx in 1..=10 {
            if g.active_entity == g.recent_antitrust_filers[idx] {
                c.trend_b = 0;
            }
            c.paragraph = g.lawsuit_rec[idx].clone();
            if mid(c.paragraph.as_bytes(), 1, 1) == "1" {
                c.extra_text = parse_any(c.paragraph.as_bytes(), b"|", 3);
                if val(c.extra_text.as_bytes()) == cast::<Ext, _>(g.active_entity) {
                    c.trend_b = 0;
                }
            }
            if g.active_entity == g.antitrust_plaintiff {
                c.trend_b = 0;
            }
        }
        if c.trend_b == 1 {
            c.trend_a = calc_industry_outlook(g, rng, g.industry_num[e]);
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
            if 10 < c.trend_a && g.active_entity != c.trend_a {
                c.trend_b = g.corp_controller[ent(c.trend_a)?];
                if 0 < c.trend_b {
                    g.tmp_ext1 = cast::<Ext, _>(c.trend_b);
                    c.idx = calc_industry_market_share(g, g.industry_num[e]);
                    if g.private_antitrust_off == 1 {
                        c.idx = 0;
                    }
                    if 49 < c.idx && g.cur_player != c.trend_b {
                        c.extra_text = s("The ")
                            + &g.ind_name[cast::<i32, _>(g.industry_num[e])]
                            + __content.string(14399)
                            + &g.ent_name[c.trend_a]
                            + __content.string(1638)
                            + &g.ent_symbol[e]
                            + __content.string(1639)
                            + &g.ent_symbol[c.trend_a]
                            + __content.string(1640)
                            + CRLF2;
                        c.clause = c.clause.clone() + &c.extra_text;
                    }
                }
            }
        }
    }
    c.extra_text = PbStr::new();
    if 10 < c.entity_id && g.industry_num[e] == 1 {
        calc_entity_financials(g, ui, rng, c.entity_id);
        if g.total_assets * sf(0.1f32) < g.cash[e] {
            c.paragraph = s("bonds");
            if g.base_interest_rate < Ext::from_f32(7.0f32) {
                c.paragraph = s(__content.string(1641));
            }
            if Ext::from_f32(7.0f32) <= g.base_interest_rate && g.base_interest_rate < Ext::from_f32(9.0f32) {
                c.paragraph = s(__content.string(1642));
            }
            c.extra_text = g.ent_name[e].clone()
                + __content.string(1643)
                + &c.paragraph
                + __content.string(1644)
                + Q
                + __content.string(1645)
                + Q
                + __content.string(1646)
                + Q
                + __content.string(1647)
                + Q
                + __content.string(1648)
                + CRLF2;
            c.clause = c.clause.clone() + &c.extra_text;
        }
    }
    holding_co_tax_note(g, ui, rng, c);
    c.clause = c.clause.clone() + &c.holding_co_tax_note;
    if !c.clause.is_empty() {
        c.clause = s(__content.string(1508)) + &c.clause;
    }
    if g.cur_year != g.start_year && !c.clause.is_empty() {
        g.help_text = c.clause.clone();
    } else {
        g.help_text = g.help_text.clone() + &c.phrase + &c.clause;
    }
    Ok(())
}

/// Mode 7 (0x557bbf).
pub(super) fn mode7(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> R {
    let __content = crate::content::current();
    let a = ent(g.active_entity)?;
    c.phrase = s(
        __content.string(1649),
    );
    c.paragraph = s("you do");
    if 10 < g.active_entity {
        c.paragraph = g.active_entity_name.clone() + " does";
    }
    c.clause = s(__content.string(1650))
        + &c.paragraph
        + __content.string(1651)
        + CRLF2;
    if 10 < g.active_entity && g.active_entity < 86 {
        c.clause = s(__content.string(1652))
            + CRLF2
            + &g.active_entity_name
            + __content.string(1653)
            + Q
            + "freeze"
            + Q
            + __content.string(1654)
            + &g.ent_symbol[a]
            + __content.string(1655)
            + CRLF2;
    }
    g.help_text = g.help_text.clone() + &c.phrase + &c.clause;
    c.phrase = s(__content.string(1656));
    c.clause = PbStr::new();
    if 10 < g.active_entity {
        c.phrase = s("Or,");
        c.clause = g.ent_symbol[a].clone() + " ";
    }
    c.phrase = c.phrase.clone()
        + __content.string(1657)
        + Q
        + __content.string(1658)
        + &c.clause
        + __content.string(1659)
        + Q
        + __content.string(1660);
    if g.active_entity < 11 || 85 < g.active_entity {
        c.phrase = c.phrase.clone()
            + __content.string(1661)
            + Q
            + __content.string(1662)
            + &c.clause
            + __content.string(1659)
            + Q
            + __content.string(1663)
            + CRLF2;
    } else {
        c.phrase = c.phrase.clone()
            + __content.string(1664)
            + CRLF2;
    }
    c.phrase = c.phrase.clone()
        + __content.string(1665)
        + Q
        + __content.string(1666)
        + Q
        + __content.string(1667);
    g.help_text = g.help_text.clone()
        + &c.phrase
        + CRLF2
        + __content.string(1532)
        + Q
        + __content.string(1668)
        + Q
        + __content.string(1669)
        + CRLF2;
    if 85 < g.active_entity && g.industry_num[a] == 2 {
        g.help_text = g.help_text.clone()
            + __content.string(1670)
            + Q
            + __content.string(1671)
            + Q
            + __content.string(1672)
            + Q
            + __content.string(1673)
            + Q
            + __content.string(1674)
            + Q
            + __content.string(1675)
            + Q
            + __content.string(1676)
            + CRLF2;
    }
    calc_entity_financials(g, ui, rng, g.active_entity);
    c.amount_a = g.net_worth;
    c.trend_a = 0;
    c.trend_b = 0;
    c.phrase = PbStr::new();
    c.clause = PbStr::new();
    holding_co_tax_note(g, ui, rng, c);
    if !c.holding_co_tax_note.is_empty() {
        g.help_text = PbStr::new();
    }
    for idx in 1..=g.num_players {
        if g.entity_status[idx] == 1 && g.cur_player != idx {
            calc_entity_financials(g, ui, rng, idx);
            if g.exchange_rate * Ext::from_f64(3000.0) < c.amount_a
                && g.net_worth * Ext::from_f64(5.0) < c.amount_a
                && g.credit_rating_num[idx] < 5
            {
                c.trend_a = idx;
                c.amount_b = g.net_worth;
            }
            if g.net_worth * Ext::from_f64(10.0) < c.amount_a && g.net_worth < g.exchange_rate * Ext::from_f64(400.0) {
                c.trend_a = idx;
                c.amount_b = g.net_worth;
            }
        }
    }
    if 0 < c.trend_a {
        let t = ent(c.trend_a)?;
        c.phrase = s("Player ")
            + &g.ent_symbol[t]
            + __content.string(1677)
            + &g.ent_symbol[t]
            + __content.string(1678)
            + &g.ent_symbol[t];
        c.trend_b = cast::<i32, _>(g.hold_stock_id[(1, t)]);
        if g.hold_stock_pct[(1, t)] < 1 {
            c.trend_b = 0;
        }
        if 10 < c.trend_b {
            c.clause = s(__content.string(1679)) + &g.ent_name[ent(c.trend_b)?];
        }
        c.phrase = c.phrase.clone() + &c.clause + ". ";
        if c.amount_b * Ext::from_f64(0.5) < g.bank_debt[t] {
            let lender = ent(cast::<i32, _>(g.lending_bank[t]))?;
            c.extra_text = g.ent_name[lender].clone();
            c.clause = s(__content.string(1680))
                + &g.ent_symbol[t]
                + " ("
                + &c.extra_text
                + __content.string(1681);
            if g.cur_player == g.corp_controller[lender] {
                c.clause = s(__content.string(1682))
                    + &c.extra_text
                    + __content.string(1683)
                    + &g.ent_symbol[t]
                    + " owes to "
                    + &g.ent_symbol[lender]
                    + ".";
            }
            c.phrase = c.phrase.clone() + CRLF2 + &c.clause;
        }
        g.help_text = PbStr::new();
    }
    if g.active_entity < 11 || g.industry_num[a] != 1 {
        c.trend_a = cast::<i32, _>(g.lending_bank[a]);
        let lender = ent(c.trend_a)?;
        calc_entity_financials(g, ui, rng, c.trend_a);
        if g.credit_rating_num[lender] < 2 && Ext::from_f64(0.0) < g.cash[a] {
            c.clause = g.ent_name[a].clone() + "'s ";
            if g.active_entity < 11 {
                c.clause = s("your ");
            }
            c.phrase = g.ent_name[lender].clone()
                + __content.string(1684)
                + &c.clause
                + __content.string(1685)
                + &c.clause
                + __content.string(1686);
            g.help_text = PbStr::new();
        }
    }
    c.phrase = c.phrase.clone() + &c.holding_co_tax_note;
    if !c.phrase.is_empty() {
        c.phrase = s(__content.string(1508)) + &c.phrase;
    }
    g.help_text = g.help_text.clone() + &c.phrase;
    Ok(())
}

//! Mode 1 with a company selected (`SelEntity > 10`): industry, ETF strategy, earnings trend, stock
//! behaviour, management, analyst rating, market share and industry outlook (BuildMenuAdviceText
//! 0x557bbf).
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::{ent, find_industry_leader_pair};
use crate::logic::cashflow::show_cash_flow_projection;
use crate::logic::control::update_corp_control;
use crate::logic::finance::get_managing_player;
use crate::logic::outlook::build_industry_outlook_text;
use crate::logic::report::number_to_words;
use crate::logic::research::get_etf_description;
use crate::logic::valuation::{
    calc_analyst_rating, price_pct_change_months, price_trend_direction, price_trend_streak,
};

/// BuildMenuAdviceText (0x557bbf), mode 1, `SelEntity > 10` (up to the recent-news part).
pub(super) fn mode1_corp(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> R {
    let __content = crate::content::current();
    let entity = ent(c.entity_id)?;
    let selected = ent(g.sel_entity)?;
    g.help_text = g.help_text.clone()
        + &g.ent_name[selected]
        + " ("
        + &g.ent_symbol[entity]
        + __content.string(1052)
        + &ucase(c.industry_name.as_bytes(), CaseMode::Ansi)
        + __content.string(1037);
    if g.industry_num[selected] == 71 {
        g.help_text = g.help_text.clone()
            + __content.string(1053)
            + &get_etf_description(g.sel_entity)
            + ".";
        if right(g.help_text.as_bytes(), 1) == "." {
            g.help_text = g.help_text.clone() + " ";
        }
        if g.covered_call_fund_id == c.entity_id {
            g.help_text = g.help_text.clone()
                + __content.string(1054);
        }
    }
    g.help_text = g.help_text.clone() + CRLF2;
    c.paragraph = parse_any(g.ent_misc_data[entity].as_bytes(), b"|", 3);
    c.amount_a = val(c.paragraph.as_bytes());
    c.phrase = fmt_trim(
        (c.amount_a * sf(0.01f32)) / g.shares_outstanding[entity],
        "#################.00",
    );
    c.paragraph = parse_any(g.ent_misc_data[entity].as_bytes(), b"|", 1);
    c.amount_b = val(c.paragraph.as_bytes());
    c.clause = fmt_trim(
        (c.amount_b * sf(0.01f32)) / g.shares_outstanding[entity],
        "#################.00",
    );
    c.paragraph =
        s(__content.string(1055)) + &g.ent_symbol[entity] + __content.string(1056);
    if val(c.phrase.as_bytes()) == val(c.clause.as_bytes()) || val(c.clause.as_bytes()) == Ext::from_f64(0.0) {
        c.paragraph = PbStr::new();
    } else if val(c.phrase.as_bytes()) <= val(c.clause.as_bytes()) {
        if Ext::from_f64(0.0) <= c.amount_b {
            if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0) {
                c.paragraph = c.paragraph.clone()
                    + if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                        __content.string(1057)
                    } else {
                        "slid"
                    };
            } else {
                c.paragraph = c.paragraph.clone() + "fell";
            }
        } else {
            c.paragraph = c.paragraph.clone() + __content.string(1058);
            repl(&mut c.paragraph, __content.string(1059), "loss");
        }
    } else if Ext::from_f64(0.0) <= c.amount_a || Ext::from_f64(0.0) <= c.amount_b {
        if val(c.clause.as_bytes()) * Ext::from_f64(2.0) < val(c.phrase.as_bytes())
            && sf(0.2f32) < val(c.phrase.as_bytes())
        {
            c.paragraph = c.paragraph.clone() + "soared";
        } else {
            c.paragraph = c.paragraph.clone()
                + if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                    "rose"
                } else {
                    __content.string(1060)
                };
        }
    } else {
        c.paragraph = c.paragraph.clone() + __content.string(1061);
        repl(&mut c.paragraph, __content.string(1059), "losses");
    }
    if !c.paragraph.is_empty() {
        c.paragraph = c.paragraph.clone()
            + __content.string(1062)
            + &c.phrase
            + __content.string(1063)
            + &c.clause
            + __content.string(1064);
    }
    if g.proj_qtr_earnings[entity] != Ext::from_f64(0.0) {
        c.extra_text = fmt_trim(
            (g.proj_qtr_earnings[entity] * sf(0.01f32)) / g.shares_outstanding[entity],
            "#################.00",
        );
        c.clause = s(__content.string(1065));
        if val(c.extra_text.as_bytes()) < Ext::from_f64(0.0) {
            c.clause = s(__content.string(1066));
        }
        if val(c.extra_text.as_bytes()) < val(c.phrase.as_bytes()) {
            c.clause = s(__content.string(1067));
        }
        if val(c.extra_text.as_bytes()) == val(c.phrase.as_bytes())
            || val(c.phrase.as_bytes()) == Ext::from_f64(0.0)
        {
            c.clause = s("be ");
        }
        if c.paragraph.is_empty() {
            c.paragraph = s(__content.string(1068));
        }
        c.phrase = s(if c.entity_id < g.sim_co_count {
            "next"
        } else {
            "this"
        });
        c.paragraph = c.paragraph.clone()
            + &g.ent_symbol[entity]
            + __content.string(1069)
            + &c.phrase
            + __content.string(1070)
            + &c.clause
            + &c.extra_text
            + __content.string(1071);
    }
    if g.industry_num[entity] == 71 {
        c.paragraph = s(__content.string(1072))
            + &g.ent_name[entity]
            + __content.string(1073);
    }
    if !c.paragraph.is_empty() {
        c.paragraph = c.paragraph.clone() + CRLF2;
    }
    g.help_text = g.help_text.clone() + &c.paragraph;
    c.trend_a = calc_analyst_rating(g, ui, rng, c.entity_id);
    update_corp_control(g, ui, rng, c.entity_id);
    if 0 < g.public_pct {
        c.clause = match c.trend_a {
            1 => s(__content.string(1074)),
            3 => s("Hold"),
            4 => s("Sell"),
            5 => s(__content.string(1075)),
            _ => s("Buy"),
        };
        c.paragraph = s(__content.string(1076)) + &c.clause + CRLF2;
        g.help_text = g.help_text.clone() + &c.paragraph;
    }
    stock_and_control(g, rng, c, entity)?;
    finances_and_market_share(g, ui, rng, c, entity)?;

    c.trend_a = cast::<i32, _>(g.industry_num[entity]);
    if 0 < c.trend_a {
        g.ind_outlook_text = PbStr::new();
        build_industry_outlook_text(g, ui, rng, c.trend_a);
        if !g.ind_outlook_text.is_empty() {
            c.phrase = g.ent_symbol[entity].clone() + __content.string(1077) + &g.ind_outlook_text;
            if 69 < c.trend_a {
                repl(&mut c.phrase, "OUTLOOK", "INFORMATION");
            }
            g.help_text = g.help_text.clone() + &c.phrase;
        }
    }
    Ok(())
}

fn stock_and_control(g: &mut Globals, rng: &mut Rng, c: &mut Ctx, entity: i32) -> R {
    let __content = crate::content::current();
    c.paragraph = s(__content.string(1078));
    if g.public_pct == 0 {
        c.paragraph = c.paragraph.clone()
            + " "
            + &g.ent_symbol[entity]
            + __content.string(1079);
    }
    c.price_streak = price_trend_streak(g, entity);
    c.trend_a = c.price_streak;
    c.trend_b = price_trend_direction(g, entity, 12, 3);
    if 5 < c.trend_a.abs() || c.trend_b != 0 {
        c.phrase = s("risen");
        if c.trend_a.abs() <= 5 {
            c.phrase = s(if c.trend_b < 0 {
                __content.string(1057)
            } else {
                __content.string(1080)
            });
            c.paragraph = c.paragraph.clone()
                + &g.ent_symbol[entity]
                + __content.string(1081)
                + &c.phrase
                + __content.string(1082);
        } else {
            if c.trend_a < 0 {
                c.phrase = s("fallen");
            }
            c.clause = s(if g.industry_num[entity] == 71 {
                "fund"
            } else {
                "company"
            });
            c.paragraph = c.paragraph.clone()
                + "The "
                + &c.clause
                + __content.string(1083)
                + &c.phrase
                + " for"
                + &number_to_words(c.trend_a, 0, 0)
                + __content.string(1084);
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                repl(&mut c.paragraph, __content.string(1085), "a row");
            }
        }
    }
    c.pct_change = 0;
    if g.industry_num[entity] == 71 {
        c.pct_change = price_pct_change_months(g, entity, 24);
        if 20 < c.pct_change {
            c.paragraph = c.paragraph.clone()
                + __content.string(1086)
                + &number_to_words(c.pct_change, 0, 1)
                + __content.string(1087);
        }
    }
    c.idx = c.cur_month;
    c.pct_change = price_pct_change_months(g, entity, c.idx);
    if c.pct_change.abs() <= 5 || c.cur_month < 4 || 10 < c.cur_month {
        c.idx = c.price_streak.abs();
        if c.idx.abs() < 6 {
            c.idx = 6;
        }
        c.trend_a = price_pct_change_months(g, entity, c.idx);
        if c.trend_a.abs() < 7 {
            c.idx = 3;
            c.trend_a = price_pct_change_months(g, entity, c.idx);
        }
        if 6 < c.trend_a.abs() {
            c.clause = s(" is up");
            if 24 < c.trend_a {
                c.clause = s(__content.string(1088));
                if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                    c.clause = s(__content.string(1089));
                }
            }
            if c.trend_a < 0 {
                c.clause = s(__content.string(1090));
            }
            if c.trend_a < -20 {
                c.clause = s(__content.string(1091));
                if g.public_pct == 0 {
                    c.clause = c.clause.clone() + __content.string(1092);
                }
            }
            c.paragraph = c.paragraph.clone()
                + __content.string(1093)
                + &g.ent_name[entity]
                + &c.clause
                + &number_to_words(c.trend_a, 0, 1)
                + __content.string(1094)
                + &number_to_words(c.idx, 0, 0)
                + __content.string(1095);
            if 3 < c.idx && 6 < c.trend_a && price_pct_change_months(g, entity, 3) < 0 {
                c.paragraph =
                    c.paragraph.clone() + __content.string(1096);
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    repl(&mut c.paragraph, __content.string(1097), "it");
                }
                if g.rand_seed % 3 == 1 {
                    repl(&mut c.paragraph, __content.string(1098), __content.string(1099));
                }
            }
        }
    } else {
        c.phrase = s(if c.pct_change < 0 {
            __content.string(1057)
        } else {
            "risen"
        });
        c.clause = s(__content.string(1100));
        if g.public_pct == 0 {
            repl(&mut c.clause, "price", __content.string(1101));
        }
        if g.industry_num[entity] == 71 {
            c.clause = s(__content.string(1102));
        }
        c.paragraph = c.paragraph.clone()
            + "The "
            + &c.clause
            + __content.string(1103)
            + &c.phrase
            + " by"
            + &number_to_words(c.pct_change, 0, 1)
            + __content.string(1104);
        c.pct_change2 = price_pct_change_months(g, entity, 12);
        c.clause = PbStr::new();
        if c.pct_change < 1 || c.pct_change2 < 13 {
            if 0 < c.pct_change && c.pct_change2 < 0 {
                c.clause = s(__content.string(1105));
            }
        } else {
            c.clause = s(", and");
        }
        if c.pct_change < 0 && c.pct_change2 < -8 {
            c.clause = s(", and");
        } else if c.pct_change < 0 && 0 < c.pct_change2 {
            c.clause = s(__content.string(1106));
        }
        if !c.clause.is_empty() {
            c.clause = c.clause.clone()
                + &number_to_words(c.pct_change2, 0, 1)
                + __content.string(1107);
            c.paragraph = c.paragraph.clone() + &c.clause;
        }
        if 5 < c.cur_month && has(&c.paragraph, " but ") == 0 {
            c.pct_change2 = price_pct_change_months(g, entity, 3);
            c.clause = PbStr::new();
            c.extra_text = s("three");
            if c.pct_change2.abs() < 10 {
                c.pct_change2 = price_pct_change_months(g, entity, 2);
                if c.pct_change2.abs() < 8 {
                    c.pct_change2 = 0;
                }
                c.extra_text = s("two");
            }
            if c.pct_change2 < 0 && c.phrase == "risen" {
                c.clause = s("fallen");
            }
            if 0 < c.pct_change2 && c.phrase == __content.string(1057) {
                c.clause = s(__content.string(1108));
            }
            if !c.clause.is_empty() {
                c.paragraph = c.paragraph.clone()
                    + __content.string(1109)
                    + &c.clause
                    + &number_to_words(c.pct_change2, 0, 1)
                    + __content.string(1110)
                    + &c.extra_text
                    + __content.string(1111);
            }
        }
        c.paragraph = c.paragraph.clone() + ". ";
    }
    if c.paragraph == __content.string(1078) {
        if g.industry_num[entity] == 71 {
            c.trend_a = price_trend_streak(g, entity);
            c.trend_b = price_pct_change_months(g, entity, 24);
            if c.trend_a.abs() <= 2 {
                if c.trend_b < 20 {
                    c.paragraph = PbStr::new();
                }
            } else {
                c.clause = s("risen");
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    c.clause = s(__content.string(1112));
                }
                if c.trend_a < 0 {
                    c.clause = s(__content.string(1113));
                }
                c.paragraph = c.paragraph.clone()
                    + __content.string(1114)
                    + &c.clause
                    + __content.string(1115)
                    + &number_to_words(c.pct_change, 0, 0)
                    + __content.string(1095);
            }
            if 20 < c.trend_b {
                c.paragraph = c.paragraph.clone()
                    + __content.string(1116)
                    + &number_to_words(c.pct_change, 0, 1)
                    + __content.string(1117);
            }
        } else {
            c.trend_a = price_pct_change_months(g, entity, 6);
            if 3 < c.trend_a.abs() {
                c.trend_a = 0;
            }
            if c.trend_a == 0 {
                c.paragraph = PbStr::new();
            } else if 0 < g.public_pct {
                c.paragraph = c.paragraph.clone()
                    + &g.ent_name[entity]
                    + __content.string(1118);
            }
        }
    }
    if g.public_pct == 0 && 0 < has(&c.paragraph, "price") {
        repl(&mut c.paragraph, "price", "value");
    }
    if !c.paragraph.is_empty() {
        g.help_text = g.help_text.clone() + &c.paragraph + CRLF2;
    }
    c.clause = s("no one");
    if 0 < g.voting_controller && c.entity_id != g.voting_controller {
        c.clause = g.ent_name[ent(g.voting_controller)?].clone();
    }
    c.clause = c.clause.clone() + __content.string(1119);
    c.phrase = s(__content.string(1120)) + &g.ent_name[entity] + __content.string(1121) + &c.clause;
    if g.cur_player == g.voting_controller {
        c.phrase = s(__content.string(1122)) + &g.ent_name[entity] + ".";
        if g.cur_player == get_managing_player(g, c.entity_id) {
            repl(&mut c.phrase, "control", __content.string(1123));
        }
    }
    if g.industry_num[entity] == 71 && 10 < g.fund_adviser[entity] {
        let adviser = ent(g.fund_adviser[entity])?;
        c.phrase = s(__content.string(1124)) + &g.ent_name[adviser];
        if 0 < g.corp_controller[adviser] && adviser != g.corp_controller[adviser] {
            c.phrase = c.phrase.clone()
                + __content.string(1125)
                + &g.ent_name[ent(g.corp_controller[adviser])?]
                + ")";
        }
        c.phrase = c.phrase.clone() + ".";
    }
    g.help_text = g.help_text.clone() + &c.phrase + CRLF2;
    Ok(())
}

fn finances_and_market_share(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    c: &mut Ctx,
    e: i32,
) -> R {
    let __content = crate::content::current();
    if 85 < e && g.industry_num[e] < 71 {
        c.phrase = s(__content.string(1126));
        c.trend_b = to_long(show_cash_flow_projection(g, ui, rng, e, 1));
        if c.trend_b == 1 {
            c.phrase = c.phrase.clone()
                + __content.string(1127);
            let mut added = false;
            if 130 < e
                && g.industry_num[e] < 70
                && g.growth_rate[e] < 1
                && (cast::<Ext, _>(g.bonds_issued[e])) <= g.bank_debt[e]
            {
                c.phrase = c.phrase.clone()
                    + __content.string(1128);
                if g.exchange_rate * Ext::from_f64(10.0) < g.bank_debt[e] {
                    c.phrase = c.phrase.clone() + __content.string(1129);
                    if (e + g.cur_quarter) % 2 == 1 {
                        let bank = ent(cast::<i32, _>(g.lending_bank[e]))?;
                        let r = s(__content.string(1130)) + &g.ent_name[bank];
                        c.phrase = replace(c.phrase.as_bytes(), __content.bytes(1131), r.as_bytes());
                    }
                }
                added = true;
            }
            if !added && Ext::from_f64(10.0) < g.bank_debt[e] && (cast::<Ext, _>(g.bonds_issued[e])) < g.bank_debt[e] {
                let bank = ent(cast::<i32, _>(g.lending_bank[e]))?;
                c.phrase = c.phrase.clone()
                    + __content.string(1132)
                    + &g.ent_name[bank];
                added = true;
            }
            if !added
                && g.bank_debt[e] < cast::<Ext, _>(g.bonds_issued[e])
                && val(mid(g.bond_issue_info[e].as_bytes(), 1, 4).as_bytes())
                    < Ext::from_f64(6.0) + cast::<Ext, _>(g.cur_year)
                && g.bond_price[e].get() < Ext::from_f64(150.0)
            {
                c.phrase = c.phrase.clone()
                    + __content.string(1133)
                    + &mid(g.bond_issue_info[e].as_bytes(), 1, 4);
                if (cast::<Ext, _>(g.bonds_issued[e])) < g.cash[e] {
                    repl(&mut c.phrase, __content.string(1134), __content.string(1135));
                }
            }
        } else {
            c.clause = PbStr::new();
            c.paragraph = PbStr::new();
            if Ext::from_f64(0.0) < g.unused_credit_line {
                c.clause = s(__content.string(1136));
            }
            if 130 < e && g.industry_num[e] < 70 && 15 < g.growth_rate[e] {
                c.paragraph = s(__content.string(1137));
            }
            match c.trend_b {
                2 | 3 => {
                    c.phrase = c.phrase.clone()
                        + __content.string(1138)
                        + &g.ent_name[e]
                        + __content.string(1139)
                        + &c.clause
                        + &c.paragraph;
                    if c.trend_b == 3 && g.tmp_ext1 != -Ext::from_f64(99.0) {
                        c.phrase = c.phrase.clone()
                            + __content.string(1140);
                    }
                }
                4 | 5 => {
                    c.phrase = c.phrase.clone()
                        + __content.string(1141);
                    if g.tmp_ext1 == -Ext::from_f64(99.0) {
                        c.phrase = crate::pb::string::remove(
                            c.phrase.as_bytes(),
                            __content.bytes(1142),
                        );
                    }
                    if c.trend_b == 4 {
                        c.phrase = c.phrase.clone()
                            + __content.string(1143)
                            + &c.paragraph;
                    } else {
                        c.phrase = c.phrase.clone()
                            + __content.string(1144);
                    }
                }
                6..=8 => {
                    c.phrase = c.phrase.clone()
                        + &g.ent_symbol[e]
                        + __content.string(1145);
                    if g.cash[e] <= Ext::from_f64(0.0) {
                        repl(&mut c.phrase, __content.string(1146), __content.string(1147));
                    }
                    if c.trend_b == 6 || c.trend_b == 7 {
                        c.phrase = c.phrase.clone()
                            + __content.string(1148)
                            + &c.paragraph;
                        if c.trend_b == 7 {
                            c.phrase = c.phrase.clone()
                                + __content.string(1149);
                        }
                    } else {
                        c.clause = s(__content.string(1150));
                        g.lender_id = cast::<i32, _>(g.lending_bank[e]);
                        if g.credit_frozen[e] == 1 && 4 <= g.credit_rating_num[e] {
                            c.clause = c.clause.clone()
                                + __content.string(1151)
                                + &g.ent_name[ent(g.lender_id)?];
                        } else {
                            c.clause = s(__content.string(1152)) + &c.clause;
                        }
                        c.phrase = c.phrase.clone()
                            + __content.string(1153)
                            + &c.clause
                            + __content.string(1154);
                    }
                }
                _ => {}
            }
        }
        if g.tmp_ext1 == -Ext::from_f64(99.0) && 1 < c.trend_b {
            c.phrase = c.phrase.clone() + __content.string(1155);
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.help_text = g.help_text.clone() + &c.phrase + "." + CRLF2;
    }
    c.phrase = PbStr::new();
    c.industry_id = g.industry_num[e];
    c.trend_b = find_industry_leader_pair(g, c.industry_id);
    c.pct_change = to_long(g.tmp_ext2);
    g.tmp_ext2 = Ext::from_f64(0.0);
    if c.trend_b < 131 || c.pct_change < 131 {
        c.trend_b = 0;
        c.pct_change = 0;
    }
    if c.entity_id == c.trend_b {
        c.leader_label = g.ent_symbol[e].clone();
    } else if c.entity_id == c.pct_change {
        c.leader_label = g.ent_name[ent(c.trend_b)?].clone();
    }
    if len(c.leader_label.as_bytes()) == 1 {
        c.leader_label = g.ent_name[ent(c.trend_b)?].clone();
    }
    if c.entity_id == c.trend_b || c.entity_id == c.pct_change {
        c.phrase = c.leader_label.clone()
            + __content.string(1156)
            + &c.industry_name
            + __content.string(1157);
    }
    if c.entity_id == c.trend_b {
        let leader = ent(c.trend_b)?;
        let second = ent(c.pct_change)?;
        if g.growth_rate[second] == g.growth_rate[leader] {
            c.phrase = c.phrase.clone() + ". ";
        } else {
            if g.growth_rate[second] < g.growth_rate[leader] && 1 <= g.growth_rate[leader] {
                c.phrase = c.phrase.clone() + __content.string(1158);
            } else if g.growth_rate[leader] < g.growth_rate[second] && 0 < g.growth_rate[second] {
                c.phrase = c.phrase.clone() + __content.string(1159);
            } else {
                c.phrase = PbStr::new();
            }
            if !c.phrase.is_empty() {
                c.phrase = c.phrase.clone() + __content.string(1160) + &g.ent_name[second] + ". ";
            }
        }
    } else if c.entity_id == c.pct_change {
        let leader = ent(c.trend_b)?;
        let second = ent(c.pct_change)?;
        if g.growth_rate[leader] == g.growth_rate[second] {
            c.phrase = c.phrase.clone() + __content.string(1161) + &g.ent_name[e] + __content.string(1162);
        } else if g.growth_rate[second] < g.growth_rate[leader] && -1 < g.growth_rate[leader] {
            c.phrase = c.phrase.clone()
                + __content.string(1163)
                + &g.ent_name[e]
                + __content.string(1164)
                + &g.ent_name[leader]
                + ". ";
        } else if g.growth_rate[leader] < g.growth_rate[second] && 0 < g.growth_rate[second] {
            c.phrase = c.phrase.clone()
                + ", but "
                + &g.ent_name[e]
                + __content.string(1165);
        } else {
            c.phrase = PbStr::new();
        }
    } else {
        c.phrase = PbStr::new();
    }
    if !c.phrase.is_empty() {
        g.help_text = g.help_text.clone() + __content.string(1166) + &c.phrase + CRLF2;
    }
    Ok(())
}

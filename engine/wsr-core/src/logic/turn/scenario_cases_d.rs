//! Cases of RunScenarioEvent_689361 (0x689361): see [`super::scenario_event`].
//!
//! This file: the special stories 101..103 (equity fund fraud, worker slowdown/strike, state
//! securities commissioner bribe) with their outcomes -101..-103, the outcome -104 (management suicide
//! after a Japanese takeover; +104 has no proposal case and falls to CASE ELSE), and the general
//! stories 111..120 / -111..-120 (futures speculation, drug-cartel money, executive bonus, tax audit,
//! cooked books, Sen. Doofus reform bill, sweatshop offices, pension fund raid, foreign partnerships,
//! Ebola vaccine). Machine code 0x696801..0x699187.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::scenario_event::Ev;

/// Runs case `n` of the scenario SELECT if it is one of this file's; returns false otherwise.
pub(super) fn dispatch(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, n: i32) -> bool {
    let __content = crate::content::current();
    let corp = cast::<i32, _>(g.scenario_corp);
    match n {
        // case 101: equity fund managers (Pluggit & Beancounter); declining arms the fund fraud anyway
        101 => {
            set(&mut g.scenario_tag, __content.string(13402));
            if 35 < e.clock_sec {
                set(&mut g.scenario_tag, __content.string(13403));
            }
            e.subst_tokens(g, ui, rng);
            if g.scenario_cmd == "ABORT" {
                set(&mut g.scenario_cmd, "EQFUND");
                e.event_state = 1;
            }
            g.scenario_stage = 5;
        }
        // case -101: the company is looted by its management
        -101 => {
            if 130 < corp {
                if Ext::from_f64(0.0) < g.business_assets[corp] {
                    g.business_assets[corp] = g.business_assets[corp] * (sf(0.2f32) * rng.rnd());
                }
            } else {
                if 1 < g.long_gov_bond_face[corp] {
                    g.long_gov_bond_face[corp] = to_quad(cast::<Ext, _>(g.long_gov_bond_face[corp]) * (sf(0.5f32) * rng.rnd()));
                }
                if 1 < g.short_gov_bond_face[corp] {
                    g.short_gov_bond_face[corp] = to_quad(cast::<Ext, _>(g.short_gov_bond_face[corp]) * (sf(0.5f32) * rng.rnd()));
                }
            }
            if 0 < e.margin {
                e.margin = -8;
            }
            if Ext::from_f64(0.0) < g.cash[corp] {
                g.cash[corp] = g.cash[corp] * (sf(0.3f32) * rng.rnd());
            }
            if Ext::from_f64(0.0) < g.t_bills[corp] {
                g.t_bills[corp] = g.t_bills[corp] * (sf(0.3f32) * rng.rnd());
            }
            g.accrued_pl[corp] = g.accrued_pl[corp] - Ext::from_f64(25.0) * e.market_cap;
            let mc = e.market_cap;
            e.market_cap = mc * sf(0.1f32) + mc * (sf(0.2f32) * rng.rnd());
            e.subst_tokens(g, ui, rng);
            set(&mut e.news_tail, __content.string(13404));
        }
        // case 102: corporate changes; declining arms the restructuring follow-up
        102 => {
            e.subst_tokens(g, ui, rng);
            if g.scenario_cmd == "Y" {
                g.scenario_stage = 14;
                set(&mut e.news_tail, __content.string(13405));
                g.scenario_headline =
                    s(__content.string(13406)) + &e.corp_symbol;
            } else {
                set(&mut g.scenario_cmd, "RESTRUC");
            }
        }
        // case -102: mutinous workers strike
        -102 => {
            if e.industry < 3 || 69 < e.industry {
                set(&mut g.scenario_cmd, "ABORT");
            } else {
                set(&mut g.scenario_tag, __content.string(13407));
                if 30 < e.clock_sec {
                    set(&mut g.scenario_tag, __content.string(13408));
                }
                e.subst_tokens(g, ui, rng);
                e.margin = e.margin.wrapping_sub(12);
                set(&mut g.scenario_cmd, "STRIKE");
                e.market_cap = sf(0.91f32) * e.market_cap;
                g.news_msg = s(__content.string(13409)) + &e.corp_symbol + __content.string(13410);
            }
        }
        // case 103: bribe for the state securities commissioner (the "Arkansas" test is nested in the
        // dayNum % 4 = 2 branch: dead code, bug #111)
        103 => {
            set(&mut g.scenario_tag, __content.string(13411));
            if e.day_num % 4 == 1 {
                set(&mut g.scenario_tag, __content.string(13412));
            }
            if e.day_num % 4 == 2 {
                set(&mut g.scenario_tag, "Alabama");
                if e.day_num % 4 == 3 {
                    set(&mut g.scenario_tag, __content.string(13413));
                }
            }
            e.detail_text = g.currency_symbol.clone() + "5 " + &g.million_prefix_uc;
            e.subst_tokens(g, ui, rng);
            if g.scenario_cmd == "Y" {
                g.scenario_stage = 5;
                e.amount = Ext::from_f64(5.0);
                e.charge_expense(g);
                g.scenario_headline = g.scenario_tag.clone() + __content.string(13414);
            } else {
                set(&mut g.scenario_cmd, "REDNECK");
            }
        }
        // case -103: caught bribing the state official
        -103 => {
            e.cost_amt = to_quad(e.market_cap * Ext::from_f64(5.0));
            e.amount = cast::<Ext, _>(e.cost_amt);
            e.charge_expense(g);
            set(&mut e.aux_text, __content.string(13415));
            set(&mut e.news_tail, __content.string(13416));
            if 40 < e.clock_sec {
                set(&mut e.news_tail, __content.string(13417));
            }
            e.news_tail = e.news_tail.clone() + &e.aux_text;
            e.subst_tokens(g, ui, rng);
            e.market_cap = e.market_cap * sf(0.87f32);
        }
        // case -104:  [runtime resource wording] the foreign (Japanese) takeover
        -104 => {
            e.amount = e.market_cap * Ext::from_f64(15.0);
            e.margin = e.margin.wrapping_sub(10);
            if 130 < corp && Ext::from_f64(0.0) < g.business_assets[corp] {
                g.extraordinary_pl[corp] =
                    to_quad(cast::<Ext, _>(g.extraordinary_pl[corp]) - g.business_assets[corp] * sf(0.3f32));
                g.business_assets[corp] = g.business_assets[corp] * sf(0.7f32);
            } else if Ext::from_f64(0.0) < g.t_bills[corp] {
                g.extraordinary_pl[corp] = to_quad(cast::<Ext, _>(g.extraordinary_pl[corp]) - g.t_bills[corp] * sf(0.6f32));
                g.t_bills[corp] = g.t_bills[corp] * sf(0.4f32);
            }
            g.scenario_tag = PbStr::new();
            if len(e.corp_name.as_bytes()) < 14 {
                set(&mut g.scenario_tag, "have");
            }
            e.misc_text = e.player_name.clone();
            e.charge_extraordinary(g);
            e.subst_tokens(g, ui, rng);
            g.news_msg = s(__content.string(13418)) + &e.corp_symbol + __content.string(13419);
            set(&mut e.aux_text, __content.string(13420));
            if g.cur_year % 2 == 1 {
                set(&mut e.aux_text, "seppuku");
            }
            if 30 < e.clock_sec {
                g.news_msg = s(__content.string(13421)) + &e.aux_text + " at " + &e.corp_name + " (" + &e.corp_symbol + ")";
            }
            if 50 < e.clock_sec {
                g.news_msg = PbStr::new();
                set(&mut e.news_tail, __content.string(13422));
            }
            e.market_cap = e.market_cap * sf(0.55f32);
        }
        // case 111: speculation in corn / wheat / T-bill futures
        111 => {
            set(&mut e.phrase, "corn");
            e.month_text = s("July ") + &e.phrase;
            if e.clock_sec < 15 {
                set(&mut e.phrase, "wheat");
                e.month_text = s("May ") + &e.phrase;
            }
            set(&mut g.scenario_tag, __content.string(13423));
            e.event_year = 0;
            if 35 < e.clock_sec || e.industry < 3 {
                set(&mut g.scenario_tag, __content.string(13424));
                set(&mut e.phrase, "T-bill");
                e.month_text = e.phrase.clone();
            }
            e.detail_text = e.month_text.clone() + __content.string(2546);
            e.loss_text = uc(&e.phrase);
            e.subst_tokens(g, ui, rng);
            if g.scenario_cmd == "Y" {
                g.news_msg = s(__content.string(13425)) + &e.phrase + __content.string(13426);
                if mid(e.detail_text.as_bytes(), 1, 1) == "T" {
                    g.news_msg = s(__content.string(13427));
                }
                g.scenario_tag = e.phrase.clone() + __content.string(2546);
                g.scenario_stage = 32;
            }
        }
        // case -111: the futures speculation pays off (showText) or loses heavily
        -111 => {
            e.aux_text = g.scenario_tag.clone();
            e.event_year = 0;
            e.detail_text = e.aux_text.clone();
            e.loss_text = uc(&e.aux_text);
            set(&mut g.scenario_tag, __content.string(13423));
            if mid(e.aux_text.as_bytes(), 1, 1) == "T" {
                set(&mut g.scenario_tag, __content.string(13428));
            }
            if e.show_text == "Y" {
                e.amount = e.market_cap * -Ext::from_f64(6.0);
                e.news_tail = s(__content.string(13429)) + &e.aux_text;
            } else {
                e.amount = e.market_cap * Ext::from_f64(8.0);
                e.news_tail = s(__content.string(13430)) + &e.detail_text;
                e.market_cap = sf(0.8f32) * e.market_cap;
                set(&mut e.misc_text, "company");
                if e.industry == 1 {
                    set(&mut e.misc_text, "bank");
                }
            }
            if 0 < instr(1, e.aux_text.as_bytes(), b"wheat") || 0 < instr(1, e.aux_text.as_bytes(), b"corn") {
                if e.industry == 15 || e.industry == 28 {
                    e.charge_expense(g);
                } else {
                    e.charge_extraordinary(g);
                }
            } else {
                e.charge_extraordinary(g);
            }
            if e.amount < Ext::from_f64(0.0) {
                e.loss_amt = to_quad(e.amount.abs());
                e.loss_text = PbStr::new();
            }
            e.subst_tokens(g, ui, rng);
        }
        // case 112: drug-cartel money laundering (low-margin companies only)
        112 => {
            if 5 < e.margin || e.industry == 70 {
                set(&mut g.scenario_cmd, "ABORT");
            } else {
                set(&mut g.scenario_tag, __content.string(13431));
                if 15 < e.clock_sec {
                    set(&mut g.scenario_tag, __content.string(13432));
                }
                if 30 < e.clock_sec {
                    set(&mut g.scenario_tag, __content.string(13433));
                }
                if 45 < e.clock_sec {
                    set(&mut g.scenario_tag, __content.string(13434));
                }
                set(&mut e.detail_text, "U.S.");
                if 0 < e.country_idx {
                    e.nationality();
                    e.nation_name = e.nation_adj.clone();
                    e.detail_text = mc(&e.nation_name);
                }
                if e.country_idx == 50 {
                    set(&mut e.detail_text, "U.K.");
                }
                e.subst_tokens(g, ui, rng);
                if g.scenario_cmd == "Y" {
                    e.cost_amt = to_quad(e.market_cap * Ext::from_f64(3.0));
                    e.amount = -Ext::from_f64(1.0) * cast::<Ext, _>(e.cost_amt);
                    e.charge_expense(g);
                    e.margin = e.margin.wrapping_add(7);
                    g.scenario_stage = 3;
                    set(&mut e.news_tail, __content.string(13435));
                    g.scenario_headline = s(__content.string(13436))
                        + &e.corp_symbol
                        + " and "
                        + &g.scenario_tag;
                } else {
                    e.notoriety = e.notoriety.wrapping_sub(1);
                    e.amount = int(rng.rnd() * Ext::from_f64(10.0)) + Ext::from_f64(30.0);
                    e.charge_extraordinary(g);
                    set(&mut e.news_tail, __content.string(13437));
                }
            }
        }
        // case -112: the laundered money is seized
        -112 => {
            if e.industry < 70 {
                e.margin = e.margin.wrapping_sub(7);
            }
            e.amount = e.market_cap * Ext::from_f64(11.0);
            if Ext::from_f64(3600.0) < e.amount && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                e.amount = g.exchange_rate * Ext::from_f64(3000.0);
            }
            e.charge_extraordinary(g);
            e.market_cap = sf(0.83f32) * e.market_cap;
            set(&mut e.detail_text, __content.string(13438));
            if 0 < e.country_idx {
                set(&mut e.detail_text, __content.string(2879));
            }
            e.subst_tokens(g, ui, rng);
            e.news_tail = s("has ") + &e.amount_text + __content.string(13439);
        }
        // case 113: the president's lavish bonus (ScenarioCmd "100": +100 cash to the player)
        113 => {
            e.amount = Ext::from_f64(100.0);
            e.subst_tokens(g, ui, rng);
            e.amount = Ext::from_f64(0.0);
            if g.scenario_cmd == "Y" {
                e.amount = Ext::from_f64(100.0);
                e.charge_expense(g);
                g.scenario_stage = 10;
                g.news_msg = s(__content.string(13440))
                    + &e.corp_symbol
                    + __content.string(13441)
                    + &g.currency_symbol
                    + "100 "
                    + &g.million_prefix_lc
                    + __content.string(13442);
                g.scenario_headline =
                    s(__content.string(13443)) + &e.corp_symbol + __content.string(13444);
                set(&mut g.scenario_cmd, "100");
            }
        }
        // case -113: lawsuits over the bonus (ScenarioCmd "-150")
        -113 => {
            e.cost_amt = to_quad(rng.rnd() * Ext::from_f64(4.0) + Ext::from_f64(2.0));
            e.amount = Ext::from_f64(100.0) * cast::<Ext, _>(e.cost_amt);
            e.detail_text = g.currency_symbol.clone() + "100 " + &g.million_prefix_lc;
            e.misc_text = g.currency_symbol.clone() + "50 " + &g.million_prefix_lc;
            g.scenario_tag = PbStr::new();
            if corp != g.player_ceo_of[cast::<i32, _>(g.event_player)] {
                set(&mut g.scenario_tag, "former ");
            }
            e.subst_tokens(g, ui, rng);
            e.amount = e.amount - Ext::from_f64(150.0);
            e.charge_expense(g);
            set(&mut g.scenario_cmd, "-150");
            e.market_cap = sf(0.82f32) * e.market_cap;
            set(&mut e.news_tail, __content.string(13445));
        }
        // case 114: tax audit (fight it, or pay the back taxes)
        114 => {
            e.event_year = to_quad(cast::<Ext, _>(e.event_year) - Ext::from_f64(1.0));
            set(&mut g.scenario_tag, __content.string(13446));
            if 30 < e.clock_sec {
                set(&mut g.scenario_tag, __content.string(13447));
            }
            e.amount = e.market_cap * Ext::from_f64(2.0);
            if e.amount < Ext::from_f64(10.0) {
                e.amount = Ext::from_f64(10.0);
            }
            if Ext::from_f64(999.0) < e.amount {
                e.amount = Ext::from_f64(950.0);
            }
            e.nation_name = mc(&e.nation_name);
            if e.country_idx == 0
                || e.country_idx == 11
                || e.country_idx == 30
                || e.country_idx == 35
                || e.country_idx == 50
            {
                e.nation_name = s("the ") + &e.nation_name;
            }
            set(&mut e.misc_text, __content.string(2911));
            if 0 < e.country_idx {
                e.nationality();
                e.misc_text = e.nation_adj.clone() + __content.string(13448);
            }
            if e.country_idx == 50 {
                set(&mut e.misc_text, __content.string(2912));
            }
            g.scenario_headline = e.corp_symbol.clone() + __content.string(13449) + &e.misc_text;
            if e.country_idx == 0 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                g.scenario_headline = s(__content.string(13450))
                    + &e.corp_symbol
                    + __content.string(13451);
            }
            e.subst_tokens(g, ui, rng);
            if g.scenario_cmd == "Y" {
                g.scenario_stage = 5;
                g.scenario_tag = trim(&format(e.amount, b"####")) + " " + &g.million_prefix_uc;
            } else {
                g.cash[corp] = g.cash[corp] - e.amount;
                e.news_tail = s(__content.string(13452)) + &e.amount_text + ".";
                e.notoriety = e.notoriety.wrapping_sub(1);
            }
        }
        // case -114: [runtime resource wording]
        -114 => {
            e.amount = val(g.scenario_tag.as_bytes());
            if e.amount < Ext::from_f64(1.0) {
                e.amount = Ext::from_f64(10.0);
            }
            e.loss_amt = to_quad(Ext::from_f64(70.0) + e.amount * Ext::from_f64(2.0));
            e.detail_text = trim(g.currency_symbol.as_bytes()) + "70 " + &g.million_prefix_lc;
            set(&mut e.misc_text, __content.string(2911));
            if e.country_idx == 11 || e.country_idx == 30 || e.country_idx == 35 {
                e.nation_name = s("the ") + &e.nation_name;
            }
            if 0 < e.country_idx {
                e.misc_text = s(__content.string(13453)) + &mc(&e.nation_name);
            }
            if e.country_idx == 50 {
                set(&mut e.misc_text, __content.string(13454));
            }
            e.subst_tokens(g, ui, rng);
            e.amount = cast::<Ext, _>(e.loss_amt);
            e.charge_extraordinary(g);
            e.market_cap = sf(0.73f32) * e.market_cap;
            e.aux_text = e.loss_text.clone() + __content.string(13455);
            if e.country_idx == 0 {
                e.news_tail = s(__content.string(13456)) + &e.aux_text;
            } else {
                e.news_tail = s(__content.string(13457)) + &e.aux_text;
            }
        }
        // case 115: inflate the earnings (public companies only)
        115 => {
            e.update_control(g, ui, rng);
            if g.public_pct == 0 {
                set(&mut g.scenario_cmd, "ABORT");
            } else {
                e.amount = e.market_cap * Ext::from_f64(5.0);
                if e.amount < Ext::from_f64(50.0) {
                    e.amount = Ext::from_f64(50.0);
                }
                e.subst_tokens(g, ui, rng);
                if g.scenario_cmd == "Y" {
                    g.equity_income_ytd[corp] = e.amount + g.equity_income_ytd[corp];
                    g.scenario_stage = 6;
                    g.scenario_headline = s(__content.string(13458))
                        + &e.corp_symbol
                        + __content.string(13459);
                }
            }
        }
        // case -115: damages for the inflated earnings
        -115 => {
            e.amount = e.market_cap * Ext::from_f64(23.0);
            e.charge_expense(g);
            e.subst_tokens(g, ui, rng);
            e.news_tail = s(__content.string(13178)) + &e.amount_text + __content.string(13460);
            e.market_cap = sf(0.6f32) * e.market_cap;
        }
        // case 116: bribe Sen. Doofus against the reform bill
        116 => {
            if e.industry == 70 {
                set(&mut g.scenario_cmd, "ABORT");
            } else {
                e.amount = sf(3.57f32);
                set(&mut e.news_tail, __content.string(13461));
                g.scenario_tag = e.news_tail.clone();
                e.subst_tokens(g, ui, rng);
                if g.scenario_cmd == "Y" {
                    e.news_tail = PbStr::new();
                    e.charge_expense(g);
                    g.scenario_stage = 35;
                    g.scenario_headline = s(__content.string(13462))
                        + &e.corp_symbol
                        + __content.string(13463);
                    if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                        g.scenario_headline = s(__content.string(13464))
                            + &e.corp_name
                            + " ("
                            + &e.corp_symbol
                            + ")";
                    }
                } else {
                    e.margin = e.margin.wrapping_sub(6);
                }
            }
        }
        // case -116: the reform bill dies (showText) or passes
        -116 => {
            if e.industry == 70 {
                set(&mut g.scenario_cmd, "ABORT");
            } else {
                if e.show_text == "Y" {
                    set(&mut e.news_tail, __content.string(13465));
                    e.margin = e.margin.wrapping_add(7);
                } else {
                    e.news_tail = g.scenario_tag.clone();
                    e.margin = e.margin.wrapping_sub(9);
                }
                e.subst_tokens(g, ui, rng);
            }
        }
        // case 117: cut office costs ( [runtime resource wording] )
        117 => {
            e.amount = e.market_cap * -Ext::from_f64(4.0);
            e.subst_tokens(g, ui, rng);
            if g.scenario_cmd == "Y" {
                e.charge_expense(g);
                g.scenario_stage = 6;
                if e.industry < 70 {
                    e.margin = e.margin.wrapping_add(3);
                }
                set(&mut e.news_tail, __content.string(13466));
                g.scenario_headline =
                    e.corp_symbol.clone() + __content.string(13467);
            }
        }
        // case -117: employee lawsuit
        -117 => {
            e.amount = e.market_cap * Ext::from_f64(9.0);
            set(&mut e.misc_text, __content.string(13468));
            if 0 < e.country_idx {
                e.misc_text = s(__content.string(13469)) + &mc(&e.nation_name);
            }
            e.charge_expense(g);
            e.subst_tokens(g, ui, rng);
            e.news_tail = s(__content.string(13178)) + &e.amount_text + __content.string(13470);
            e.market_cap = sf(0.7f32) * e.market_cap;
            if e.industry < 70 {
                e.margin = e.margin.wrapping_sub(5);
            }
        }
        // case 118: tap the surplus pension funds
        118 => {
            e.amount = e.market_cap * Ext::from_f64(5.0);
            e.subst_tokens(g, ui, rng);
            if g.scenario_cmd == "Y" {
                e.amount = e.market_cap * -Ext::from_f64(5.0);
                if e.industry < 70 {
                    e.margin = e.margin.wrapping_add(7);
                }
                e.charge_expense(g);
                g.scenario_tag = e.amount_text.clone();
                g.scenario_stage = 5;
                set(&mut e.news_tail, __content.string(13471));
                set(&mut e.aux_text, __content.string(13472));
                if e.clock_sec % 2 == 1 {
                    set(&mut e.aux_text, __content.string(13473));
                }
                g.scenario_headline = s(__content.string(13474)) + &e.corp_symbol + __content.string(13475) + &e.aux_text;
            }
        }
        // case -118: sued for looting the pension plan
        -118 => {
            e.amount = e.market_cap * Ext::from_f64(12.0);
            if e.industry < 70 {
                e.margin = e.margin.wrapping_sub(11);
            }
            e.charge_expense(g);
            e.misc_text = lcase(g.scenario_tag.as_bytes(), CaseMode::Ansi);
            e.subst_tokens(g, ui, rng);
            set(&mut e.news_tail, __content.string(13476));
        }
        // case 119: hide losses in foreign partnerships
        119 => {
            e.subst_tokens(g, ui, rng);
            if g.scenario_cmd == "Y" {
                if e.industry < 70 {
                    e.margin = e.margin.wrapping_add(3);
                }
                if g.accrued_pl[corp] < Ext::from_f64(300.0) {
                    g.accrued_pl[corp] = Ext::from_f64(300.0);
                }
                g.equity_income_ytd[corp] = e.market_cap * Ext::from_f64(7.0) + g.equity_income_ytd[corp];
                g.scenario_stage = 4;
                g.scenario_headline = s(__content.string(13477)) + &e.corp_symbol + __content.string(13478);
                if e.country_idx == 50 {
                    repl(&mut g.scenario_headline, "S.E.C.", __content.string(13454));
                } else if 0 < e.country_idx {
                    repl(&mut g.scenario_headline, "S.E.C.", __content.string(13479));
                }
            }
        }
        // case -119: fraudulent earnings revealed (only with a positive EntMiscData field 3)
        -119 => {
            e.aux_text = parse_any(g.ent_misc_data[corp].as_bytes(), b"|", 3);
            if Ext::from_f64(0.0) < val(e.aux_text.as_bytes()) {
                e.amount = e.market_cap * Ext::from_f64(15.0);
                if e.industry < 70 {
                    e.margin = e.margin.wrapping_sub(11);
                }
                e.charge_expense(g);
                e.subst_tokens(g, ui, rng);
                set(&mut e.news_tail, __content.string(13480));
            } else {
                set(&mut g.scenario_cmd, "ABORT");
            }
        }
        // case 120: Ebola vaccine contract
        120 => {
            set(&mut g.scenario_tag, "Bolivia");
            if 15 < e.clock_sec {
                set(&mut g.scenario_tag, __content.string(13481));
            }
            if 30 < e.clock_sec {
                set(&mut g.scenario_tag, __content.string(13482));
            }
            if 45 < e.clock_sec {
                set(&mut g.scenario_tag, "Belarus");
            }
            set(&mut e.news_tail, __content.string(13483));
            e.amount = e.market_cap * Ext::from_f64(2.0);
            e.charge_expense(g);
            e.subst_tokens(g, ui, rng);
            if g.scenario_cmd == "Y" {
                e.news_tail = s(__content.string(13484)) + &g.scenario_tag;
                g.scenario_headline = g.scenario_tag.clone()
                    + __content.string(13485)
                    + &e.corp_symbol
                    + __content.string(13486);
                e.margin = e.margin.wrapping_add(5);
                g.scenario_stage = 10;
            }
        }
        // case -120: vaccine tragedy
        -120 => {
            e.amount = e.market_cap * Ext::from_f64(15.0);
            e.margin = e.margin.wrapping_sub(11);
            e.charge_expense(g);
            e.misc_text = e.player_name.clone();
            e.detail_text = uc(&g.scenario_tag);
            e.subst_tokens(g, ui, rng);
            e.news_tail = s(__content.string(13487)) + &g.scenario_tag;
        }
        _ => return false,
    }
    true
}

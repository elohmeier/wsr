//! Cases 39..70 and -39..-70 of RunScenarioEvent_689361 (0x689361, machine code 0x6929a4..0x696801):
//! the industry stories of industries 39 (web auctions), 42 (fake jeans), 46 (organic produce),
//! 48 (toys), 50 (gold mine), 51 (CFCs), 55 (airbags), 56 (nursing homes / hospitals), 57 (restaurants),
//! 58 (video games), 61 (servers / malware), 62 (security firm), 63 (dam / dump site), 64 (tire
//! recycling in Congo), 66 (Turkish operations), 67 (self-driving trucks), 68 (muscle car), 69 (wind
//! farm). Industries without a story (40, 41, 43..45, 47, 49, 52..54, 59, 60, 65, 70) show "No scenario
//! for n". See [`super::scenario_event`] for the frame and the common end of every case.

#![allow(unused_imports)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::scenario_event::Ev;
use crate::logic::report::show_quick_message;

/// `MSGBOX "No scenario for" + STR$(ScenarioNum), 0, ""`.
fn no_scenario(g: &Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let text = s(__content.string(13071)) + &str_s(cast::<Ext, _>(g.scenario_num));
    ui.msgbox(&text, 0, &PbStr::new());
}

/// `ScenarioCmd = "ABORT"` (outcome of a company no longer in the story's industry).
fn abort(g: &mut Globals) {
    set(&mut g.scenario_cmd, "ABORT");
}

fn yes(g: &Globals) -> bool {
    g.scenario_cmd == "Y"
}

/// Runs case `n` of the scenario SELECT if it is one of this file's; returns false otherwise.
pub(super) fn dispatch(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, n: i32) -> bool {
    let __content = crate::content::current();
    let cs = e.clock_sec;
    match n {
        // case 39: web auctions of babies / body parts / organs
        39 => {
            set(&mut g.scenario_tag, "babies");
            if 20 < cs {
                set(&mut g.scenario_tag, __content.string(13249));
            }
            if 40 < cs {
                set(&mut g.scenario_tag, __content.string(13250));
            }
            e.misc_text = uc(&g.scenario_tag);
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.amount = -Ext::from_f64(4.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 4;
                e.news_tail = PbStr::new();
                g.scenario_headline =
                    e.corp_symbol.clone() + __content.string(13251) + &g.scenario_tag + __content.string(13252);
                if 0 < cs % 3 {
                    g.scenario_headline = s(__content.string(13253))
                        + &e.corp_symbol
                        + __content.string(13254)
                        + &g.scenario_tag;
                }
                g.scenario_stage = 8;
            }
        }
        // case -39: auctions halted
        -39 => {
            if e.industry == 39 {
                e.misc_text = uc(&g.scenario_tag);
                set(&mut e.loss_text, " ");
                e.cmp_arg = 15;
                if len(e.corp_name.as_bytes()) < e.cmp_arg {
                    set(&mut e.loss_text, __content.string(13255));
                }
                e.amount = e.market_cap * Ext::from_f64(7.0);
                if e.amount < Ext::from_f64(8.0) {
                    e.amount = Ext::from_f64(8.0);
                }
                e.margin -= 9;
                e.subst_tokens(g, ui, rng);
                e.charge_extraordinary(g);
                e.news_tail =
                    s(__content.string(13256)) + &g.scenario_tag + __content.string(13257) + &e.amount_text + ".";
                e.market_cap = sf(0.66f32) * e.market_cap;
            } else {
                abort(g);
            }
        }
        // cases 40, 41, -40, -41: no scenario
        40 | 41 | -40 | -41 => no_scenario(g, ui),
        // case 42: discounting fake brand jeans
        42 => {
            e.amount = -sf(0.1f32) * g.business_assets[cast::<i32, _>(g.scenario_corp)];
            set(&mut g.scenario_tag, "Levi");
            if 14 < cs {
                set(&mut g.scenario_tag, __content.string(13258));
            }
            if 30 < cs {
                set(&mut g.scenario_tag, "Lee");
            }
            if 45 < cs {
                set(&mut g.scenario_tag, "Porker");
            }
            e.event_year = 0;
            e.detail_text = uc(&g.scenario_tag);
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.charge_expense(g);
                e.margin += 5;
                g.scenario_stage = 5;
                e.news_tail = PbStr::new();
                g.scenario_headline = e.corp_symbol.clone()
                    + __content.string(13259)
                    + &g.scenario_tag
                    + __content.string(13260);
                if 0 < cs % 3 {
                    g.scenario_headline = s(__content.string(13261))
                        + &g.scenario_tag
                        + __content.string(13262)
                        + &e.corp_symbol
                        + __content.string(13263);
                }
            }
        }
        // case -42: caught selling fakes
        -42 => {
            if e.industry == 42 {
                e.event_year = 0;
                e.detail_text = uc(&g.scenario_tag);
                e.amount = e.market_cap * Ext::from_f64(7.0);
                e.margin -= 12;
                e.subst_tokens(g, ui, rng);
                e.amount = e.market_cap * Ext::from_f64(9.0);
                e.charge_expense(g);
                e.market_cap = sf(0.7f32) * e.market_cap;
                e.news_tail = s(__content.string(13264)) + &g.scenario_tag + " jeans.";
            } else {
                abort(g);
            }
        }
        // cases 43..45, -43..-45: no scenario
        43..=45 | -45..=-43 => no_scenario(g, ui),
        // case 46: 'organic' produce
        46 => {
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.amount = -Ext::from_f64(5.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 4;
                e.news_tail = PbStr::new();
                g.scenario_headline =
                    e.corp_symbol.clone() + __content.string(13265);
                if 0 < cs % 3 {
                    g.scenario_headline = s(__content.string(13266))
                        + &e.corp_symbol
                        + " store";
                }
                g.scenario_stage = 8;
            } else {
                e.notoriety = e.notoriety.wrapping_sub(1);
            }
        }
        // case -46: Killer Tomatoes
        -46 => {
            if e.industry == 46 {
                e.amount = e.market_cap * Ext::from_f64(9.0);
                e.margin -= 9;
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                set(&mut e.news_tail, __content.string(13267));
                e.market_cap = sf(0.6f32) * e.market_cap;
            } else {
                abort(g);
            }
        }
        // cases 47, -47, 49, -49: no scenario
        47 | -47 | 49 | -49 => no_scenario(g, ui),
        // case 48: grey-market / toxic toys
        48 => {
            if e.show_text == "Y" {
                e.misc_text = g.million_prefix_lc.clone();
            }
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.amount = -Ext::from_f64(7.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 4;
                if e.show_text == "Y" {
                    set(&mut e.news_tail, __content.string(13268));
                    g.scenario_headline =
                        s(__content.string(13269)) + &e.corp_symbol;
                } else {
                    set(&mut e.news_tail, __content.string(13270));
                    g.scenario_headline = s(__content.string(13271))
                        + &e.corp_symbol
                        + __content.string(13272);
                    if cs % 2 == 1 {
                        g.scenario_headline = s(__content.string(13273))
                            + &e.corp_symbol
                            + __content.string(13274);
                    }
                }
                g.scenario_stage = 8;
            } else {
                if e.show_text == "Y" {
                    set(&mut e.news_tail, __content.string(13275));
                }
                e.notoriety = e.notoriety.wrapping_sub(1);
            }
        }
        // case -48: recalled goods / toxic toys
        -48 => {
            if e.industry == 48 {
                e.amount = e.market_cap * Ext::from_f64(9.0);
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
                    e.margin -= 9;
                    e.amount = e.market_cap * Ext::from_f64(2.0);
                }
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                set(&mut e.news_tail, __content.string(13276));
                if e.show_text == "Y" {
                    set(&mut e.news_tail, __content.string(13277));
                }
                e.market_cap = sf(0.6f32) * e.market_cap;
            } else {
                abort(g);
            }
        }
        // case 50: selling a defunct gold mine
        50 => {
            set(&mut g.scenario_tag, __content.string(13278));
            if 10 < e.day_num {
                set(&mut g.scenario_tag, "Denver");
            }
            e.amount = -Ext::from_f64(3.0) * e.market_cap;
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                if cs < 25 {
                    set(&mut g.scenario_tag, __content.string(13279));
                    g.scenario_stage = 40;
                } else {
                    e.charge_expense(g);
                    g.scenario_stage = 6;
                    g.news_msg = e.corp_symbol.clone() + __content.string(13280);
                    if 30 < cs {
                        g.news_msg = s(__content.string(13281)) + &e.corp_symbol;
                    }
                }
            }
        }
        // case -50: gold found / stock fraud verdict
        -50 => {
            if e.industry == 50 {
                set(&mut g.scenario_tag, __content.string(13278));
                if 10 < e.day_num {
                    set(&mut g.scenario_tag, "Denver");
                }
                e.event_year = 0;
                set(&mut e.detail_text, __content.string(13282));
                if g.scenario_tag == "Denver" {
                    set(&mut e.detail_text, __content.string(13283));
                }
                if e.show_text == "Y" {
                    e.margin += 4;
                    g.news_msg = s(__content.string(13284)) + &e.corp_symbol + __content.string(13285);
                } else {
                    e.amount = e.market_cap * Ext::from_f64(8.0);
                    e.charge_extraordinary(g);
                    g.news_msg = s(__content.string(13286)) + &e.corp_symbol + __content.string(13287);
                    e.market_cap = sf(0.8f32) * e.market_cap;
                }
                e.subst_tokens(g, ui, rng);
            } else {
                abort(g);
            }
        }
        // case 51: CFC production
        51 => {
            e.amount = -Ext::from_f64(6.0) * e.market_cap;
            set(&mut g.scenario_tag, "Bangkok");
            if 30 < cs {
                set(&mut g.scenario_tag, "Manila");
            }
            if 0 < e.country_idx {
                set(&mut e.misc_text, "t");
            }
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.charge_expense(g);
                g.scenario_stage = 5;
                e.margin += 3;
                g.scenario_headline =
                    s(__content.string(13288)) + &e.corp_symbol;
            }
        }
        // case -51: ozone fine (no industry check)
        -51 => {
            e.amount = e.market_cap * Ext::from_f64(9.0);
            e.charge_expense(g);
            e.subst_tokens(g, ui, rng);
            e.market_cap = sf(0.8f32) * e.market_cap;
            e.news_tail = s("fined ") + &e.amount_text + __content.string(13289);
            if e.industry < 70 {
                e.margin -= 7;
            }
        }
        // cases 52..54, -52..-54: no scenario
        52..=54 | -54..=-52 => no_scenario(g, ui),
        // case 55: cheap airbags
        55 => {
            set(&mut g.scenario_tag, "Bosnia");
            if 20 < cs {
                set(&mut g.scenario_tag, "Albania");
            }
            if 40 < cs {
                set(&mut g.scenario_tag, __content.string(13290));
            }
            set(&mut e.misc_text, "German");
            if 30 < cs {
                set(&mut e.misc_text, "Swedish");
            }
            e.amount = Ext::from_f64(5.0);
            e.subst_tokens(g, ui, rng);
            e.news_tail = s(__content.string(13291)) + &e.amount_text + ".";
            if yes(g) {
                e.amount = -Ext::from_f64(11.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 4;
                e.news_tail = s(__content.string(13292)) + &g.scenario_tag + __content.string(13293);
                g.scenario_headline =
                    s(__content.string(13294)) + &e.corp_symbol + __content.string(13295);
                if cs % 2 == 0 {
                    g.scenario_headline =
                        s(__content.string(13296)) + &e.corp_symbol;
                }
                g.scenario_stage = 8;
            } else {
                e.notoriety = e.notoriety.wrapping_sub(1);
                e.charge_extraordinary(g);
            }
        }
        // case -55: defective airbags
        -55 => {
            if e.industry == 55 {
                e.amount = e.market_cap * Ext::from_f64(12.0);
                e.margin -= 9;
                e.misc_text = uc(&g.scenario_tag);
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                e.news_tail = s(__content.string(13297)) + &e.amount_text + __content.string(13298);
                e.market_cap = sf(0.6f32) * e.market_cap;
            } else {
                abort(g);
            }
        }
        // case 56: nursing home / hospital turnaround
        56 => {
            e.subst_tokens(g, ui, rng);
            e.margin -= 4;
            set(&mut e.aux_text, __content.string(13299));
            if e.show_text == "Y" {
                set(&mut e.aux_text, __content.string(13300));
            }
            g.news_msg = e.corp_symbol.clone() + &e.aux_text + __content.string(13301);
            if yes(g) {
                e.amount = -Ext::from_f64(4.0) * e.amount;
                e.charge_expense(g);
                e.margin += 12;
                g.scenario_stage = 9;
                g.news_msg = e.corp_symbol.clone() + &e.aux_text + __content.string(13302);
                if e.show_text == "Y" {
                    g.scenario_headline =
                        s(__content.string(13303)) + &e.corp_symbol + __content.string(13304);
                    if cs % 3 == 1 {
                        g.scenario_headline = s(__content.string(13305))
                            + &e.corp_symbol
                            + __content.string(13306);
                    }
                    if cs % 3 == 2 {
                        g.scenario_headline = s(__content.string(13307))
                            + &e.corp_symbol
                            + __content.string(13308);
                    }
                } else {
                    g.scenario_headline = s(__content.string(13309))
                        + &e.corp_symbol
                        + __content.string(13310);
                    if cs % 3 == 1 {
                        g.scenario_headline = s(__content.string(13311))
                            + &e.corp_symbol
                            + __content.string(6298);
                    }
                }
            } else {
                e.charge_expense(g);
            }
        }
        // case -56: shut down
        -56 => {
            if e.industry == 56 {
                e.amount = e.market_cap * Ext::from_f64(8.0);
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                e.margin -= 15;
                e.market_cap = sf(0.5f32) * e.market_cap;
                e.news_tail = s(__content.string(13312)) + &e.amount_text + ".";
                if e.show_text == "Y" {
                    set(&mut e.news_tail, __content.string(13313));
                }
            } else {
                abort(g);
            }
        }
        // case 57: 'recycled' restaurant food
        57 => {
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.amount = -Ext::from_f64(7.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 4;
                g.scenario_headline =
                    e.corp_symbol.clone() + __content.string(13314);
                if cs % 2 == 1 {
                    g.scenario_headline = s(__content.string(13315))
                        + &e.corp_symbol
                        + __content.string(13316);
                }
                g.scenario_stage = 8;
            }
        }
        // case -57: caught recycling garbage
        -57 => {
            if e.industry == 57 {
                e.amount = e.market_cap * Ext::from_f64(9.0);
                e.margin -= 9;
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                set(&mut e.news_tail, __content.string(13317));
                e.market_cap = sf(0.6f32) * e.market_cap;
            } else {
                abort(g);
            }
        }
        // case 58: ultra-violent video game
        58 => {
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.amount = -Ext::from_f64(8.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 4;
                set(&mut e.aux_text, __content.string(13318));
                if e.show_text == "Y" {
                    set(&mut e.aux_text, __content.string(13319));
                }
                e.news_tail = s(__content.string(13320)) + &e.aux_text + " game";
                g.scenario_headline =
                    s(__content.string(13321)) + &e.corp_symbol + __content.string(13322);
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                    repl(&mut g.scenario_headline, "FBI", "Police");
                }
                if e.show_text == "Y" && cs % 2 == 1 {
                    g.scenario_headline = s(__content.string(13323))
                        + &e.corp_symbol
                        + __content.string(13324);
                }
                if g.rand_seed % 5 == 1 {
                    g.scenario_headline = s(__content.string(13325));
                }
                g.scenario_stage = 9;
            }
        }
        // case -58: [runtime resource wording]
        -58 => {
            if e.industry == 58 {
                e.amount = e.market_cap * Ext::from_f64(10.0);
                e.margin -= 12;
                e.nation_name = mc(&e.nation_name);
                e.prefix_the();
                e.event_year = e.event_year + 1;
                g.scenario_tag = PbStr::new();
                e.cmp_arg = 15;
                if len(e.corp_name.as_bytes()) < e.cmp_arg {
                    set(&mut g.scenario_tag, __content.string(13326));
                }
                e.cmp_arg = 9;
                if len(e.corp_name.as_bytes()) < e.cmp_arg {
                    set(&mut g.scenario_tag, __content.string(13327));
                }
                e.subst_tokens(g, ui, rng);
                e.charge_extraordinary(g);
                set(&mut e.aux_text, "video");
                if e.show_text == "Y" {
                    set(&mut e.aux_text, __content.string(13328));
                }
                e.news_tail = s(__content.string(13329)) + &e.aux_text + __content.string(13330);
            } else {
                abort(g);
            }
        }
        // cases 59, 60, -59, -60: no scenario
        59 | 60 | -59 | -60 => no_scenario(g, ui),
        // case 61: 'Stacheldraht' virus
        61 => {
            e.amount = Ext::from_f64(3.0);
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.amount = -Ext::from_f64(8.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 5;
                e.news_tail = PbStr::new();
                g.scenario_headline = e.corp_symbol.clone()
                    + __content.string(13331);
                e.aux_text = e.corp_symbol.clone();
                e.cmp_arg = 16;
                if len(e.corp_name.as_bytes()) < e.cmp_arg {
                    e.aux_text = e.corp_name.clone() + " (" + &e.corp_symbol + ")";
                }
                if cs % 3 < 2 {
                    g.scenario_headline = s(__content.string(13332)) + &e.aux_text;
                }
                g.scenario_stage = 13;
            } else {
                e.charge_extraordinary(g);
                e.news_tail = s(__content.string(12972)) + &e.amount_text + __content.string(13333);
            }
        }
        // case -61: malware traced to the company
        -61 => {
            if e.industry == 61 {
                e.amount = e.market_cap * Ext::from_f64(12.0);
                e.margin -= 9;
                e.cmp_arg = 19;
                if len(e.corp_name.as_bytes()) < e.cmp_arg {
                    set(&mut e.misc_text, "simply ");
                }
                e.detail_text = e.player_name.clone();
                if e.nation_name == __content.string(13334) {
                    set(&mut e.nation_name, "U.S.");
                } else {
                    e.nationality();
                }
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                g.news_msg = s(__content.string(13335)) + &e.corp_symbol;
                e.cmp_arg = 15;
                if len(e.corp_name.as_bytes()) < e.cmp_arg {
                    g.news_msg = g.news_msg.clone() + " (" + &mc(&e.corp_name) + ")";
                }
                e.market_cap = sf(0.5f32) * e.market_cap;
            } else {
                abort(g);
            }
        }
        // case 62: security firm selling clients' secrets
        62 => {
            set(&mut e.misc_text, "the FBI");
            if 0 < e.country_idx {
                set(&mut e.misc_text, __content.string(12899));
            }
            e.amount = e.market_cap * Ext::from_f64(8.0);
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.amount = -Ext::from_f64(8.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 5;
                set(&mut e.news_tail, __content.string(13336));
                g.scenario_headline =
                    s(__content.string(13337)) + &e.corp_symbol;
                if 30 < cs {
                    g.scenario_headline = s(__content.string(13338))
                        + &e.corp_symbol
                        + __content.string(13339);
                }
                g.scenario_stage = 10;
            } else {
                g.news_msg = s(__content.string(13340))
                    + &e.corp_symbol
                    + __content.string(13341);
                // ShowQuickMessage of a temporary copy
                let mut tmp = g.news_msg.clone() + __content.string(13342);
                show_quick_message(g, ui, &mut tmp);
                g.news_msg = e.corp_symbol.clone()
                    + __content.string(13343);
                post_news(g, ui, 0, 0);
                e.margin -= 3;
                e.market_cap = sf(0.9f32) * e.market_cap;
            }
        }
        // case -62: executives arrested
        -62 => {
            set(&mut e.misc_text, "the FBI");
            if 0 < e.country_idx {
                set(&mut e.misc_text, __content.string(12899));
            }
            g.scenario_tag = e.misc_text.clone();
            if e.industry == 62 {
                e.amount = e.market_cap * Ext::from_f64(11.0);
                e.margin -= 9;
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                set(&mut e.news_tail, __content.string(13344));
                e.market_cap = sf(0.8f32) * e.market_cap;
            } else {
                abort(g);
            }
        }
        // case 63: cheap concrete dam / Hanford dump site
        63 => {
            set(&mut g.scenario_tag, __content.string(6979));
            if 35 < cs {
                set(&mut g.scenario_tag, "Libya");
            }
            if e.show_text == "Y" {
                set(&mut g.scenario_tag, "Hanford");
                set(&mut e.news_tail, __content.string(13345));
            }
            e.amount = e.market_cap * Ext::from_f64(2.0);
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.margin += 4;
                e.amount = -Ext::from_f64(1.0) * e.amount;
                e.charge_expense(g);
                g.scenario_stage = 5;
                if e.show_text == "Y" {
                    e.news_tail = PbStr::new();
                } else {
                    set(&mut e.news_tail, __content.string(13346));
                    g.scenario_headline =
                        s(__content.string(13347)) + &e.corp_symbol + __content.string(13348);
                }
            }
        }
        // case -63: failed dam
        -63 => {
            if e.industry == 63 {
                e.amount = e.market_cap * Ext::from_f64(7.0);
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                e.margin -= 12;
                e.market_cap = sf(0.8f32) * e.market_cap;
                e.news_tail = s(__content.string(13031)) + &g.scenario_tag + __content.string(13349);
                if e.show_text == "Y" {
                    set(&mut e.news_tail, __content.string(13350));
                }
            } else {
                abort(g);
            }
        }
        // case 64: tire recycling plant in Congo
        64 => {
            set(&mut e.misc_text, "Maine");
            if 20 < cs {
                set(&mut e.misc_text, "Oregon");
            }
            if 40 < cs {
                set(&mut e.misc_text, "Alberta");
            }
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                g.news_msg = e.corp_symbol.clone()
                    + __content.string(13351)
                    + &e.misc_text
                    + __content.string(13352);
                if e.misc_text == "Alberta" {
                    repl(&mut g.news_msg, "E.P.A.", "Ottawa");
                }
                e.post_corp_news(g, ui);
            }
            set(&mut g.scenario_tag, "tire");
            set(&mut e.tire_word, "TIRE");
            if g.currency_code == "UK" || g.currency_code == "AUS" || g.currency_code == "NZ" {
                set(&mut g.scenario_tag, "tyre");
                set(&mut e.tire_word, "TYRE");
            }
            e.amount = -Ext::from_f64(8.0) * e.market_cap;
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.charge_expense(g);
                e.margin += 5;
                e.market_cap = sf(1.23f32) * e.market_cap;
                e.news_tail =
                    s(__content.string(13353)) + &g.scenario_tag + __content.string(13354);
                if 30 < cs {
                    set(&mut e.news_tail, __content.string(13355));
                }
                g.scenario_headline =
                    s(__content.string(13356)) + &e.corp_symbol + __content.string(13357);
                if 30 < cs {
                    g.scenario_headline = s(__content.string(13358))
                        + &e.corp_symbol
                        + __content.string(13359);
                }
                g.scenario_stage = cast::<i16, _>(rng.rnd_range(5, 0) + 15);
            } else {
                set(&mut e.news_tail, __content.string(13360));
                e.margin -= 3;
                e.amount = e.market_cap * Ext::from_f64(2.0);
                e.charge_expense(g);
                if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                    set(&mut e.news_tail, __content.string(13361));
                }
                e.market_cap = sf(0.9f32) * e.market_cap;
                e.notoriety = e.notoriety.wrapping_sub(1);
            }
        }
        // case -64: Congo plant revealed as polluter (or sold at a gain)
        -64 => {
            if e.industry != 64 {
                abort(g);
                return true;
            }
            set(&mut e.tire_word_lc, "'tire");
            set(&mut e.tire_word, "TIRE");
            if g.currency_code == "UK" || g.currency_code == "AUS" || g.currency_code == "NZ" {
                set(&mut e.tire_word_lc, "'tyre");
                set(&mut e.tire_word, "TYRE");
            }
            if Ext::from_f64(1.0) <= rng.rnd() * Ext::from_f64(4.0) {
                e.amount = e.market_cap * Ext::from_f64(9.0);
                e.market_cap = sf(0.85f32) * e.market_cap;
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
                    e.margin -= 6;
                    e.amount = e.market_cap * Ext::from_f64(2.0);
                }
                e.news_tail = e.tire_word_lc.clone()
                    + __content.string(13362);
                if cs < 25 {
                    set(&mut e.news_tail, __content.string(13363));
                }
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
            } else {
                e.amount = -Ext::from_f64(9.0) * e.market_cap;
                e.market_cap = sf(1.06f32) * e.market_cap;
                g.news_msg = e.corp_symbol.clone()
                    + __content.string(13364);
                e.charge_extraordinary(g);
                e.post_corp_news(g, ui);
            }
        }
        // cases 65, -65: no scenario
        65 | -65 => no_scenario(g, ui),
        // case 66: Turkish threats over a Kurdish artist
        66 => {
            if e.country_idx == 49 {
                abort(g);
            } else {
                e.subst_tokens(g, ui, rng);
                if yes(g) {
                    e.amount = -Ext::from_f64(4.0) * e.market_cap;
                    e.margin += 5;
                    e.charge_expense(g);
                    set(&mut e.news_tail, __content.string(13365));
                    g.scenario_headline =
                        e.corp_symbol.clone() + __content.string(13366);
                    if cs % 3 == 2 {
                        g.scenario_headline =
                            s(__content.string(13367)) + &e.corp_symbol;
                    }
                    g.scenario_stage = 12;
                    e.notoriety = e.notoriety.wrapping_add(2);
                } else {
                    if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                        e.margin += 4;
                        g.news_msg = s(__content.string(13368)) + &e.corp_symbol;
                    } else {
                        let c = cast::<i32, _>(g.scenario_corp);
                        e.amount = g.business_assets[c] * sf(0.15f32);
                        g.cash[c] = e.amount + g.cash[c];
                        g.business_assets[c] = g.business_assets[c] * sf(0.85f32);
                        e.margin -= 4;
                        set(&mut e.news_tail, __content.string(13369));
                        e.market_cap = sf(0.82f32) * e.market_cap;
                        e.charge_extraordinary(g);
                    }
                    e.notoriety = e.notoriety.wrapping_sub(2);
                }
            }
        }
        // case -66: record profits in Turkey
        -66 => {
            if e.industry == 66 {
                e.margin += 4;
                e.nation_name = mc(&e.nation_name);
                e.prefix_the();
                e.cmp_arg = 9;
                if len(e.corp_name.as_bytes()) < e.cmp_arg {
                    set(&mut e.misc_text, __content.string(13370));
                }
                let c = cast::<i32, _>(g.scenario_corp);
                if g.country_idx[c] < 1 {
                    set(&mut g.scenario_tag, __content.string(13371));
                } else {
                    g.scenario_tag = mc(&g.country_display_name[g.country_idx[c]]);
                }
                e.subst_tokens(g, ui, rng);
                set(&mut e.news_tail, __content.string(13372));
                e.notoriety = e.notoriety.wrapping_add(2);
            } else {
                abort(g);
            }
        }
        // case 67: self-driving trucks
        67 => {
            e.amount = e.market_cap * Ext::from_f64(5.0);
            e.event_year = e.event_year + 3;
            e.detail_text = PbStr::new();
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.amount = -Ext::from_f64(8.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 5;
                set(&mut e.news_tail, __content.string(13373));
                if 2 < g.rand_seed % 5 {
                    set(&mut e.news_tail, __content.string(13374));
                }
                g.scenario_headline =
                    s(__content.string(13375)) + &e.corp_symbol + __content.string(13376);
                if 10 < cs {
                    g.scenario_headline =
                        e.corp_symbol.clone() + __content.string(13377);
                }
                if 30 < cs {
                    g.scenario_headline = s(__content.string(13378))
                        + &e.corp_symbol
                        + __content.string(13379);
                }
                if 45 < cs {
                    g.scenario_headline = e.corp_symbol.clone()
                        + __content.string(13380);
                }
                g.scenario_stage = 10;
            } else {
                e.charge_extraordinary(g);
                set(&mut e.news_tail, __content.string(13381));
                e.market_cap = sf(0.85f32) * e.market_cap;
                e.margin -= 5;
                if cs % 4 == 1 {
                    e.margin += 7;
                }
            }
        }
        // case -67: trucks discontinued
        -67 => {
            if e.industry == 67 {
                e.amount = e.market_cap * Ext::from_f64(12.0);
                e.margin -= 9;
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                set(&mut e.news_tail, __content.string(13382));
                if 30 < cs {
                    set(&mut e.news_tail, __content.string(13383));
                }
                e.market_cap = sf(0.78f32) * e.market_cap;
            } else {
                abort(g);
            }
        }
        // case 68: 'Lightning' muscle car
        68 => {
            e.amount = e.market_cap * Ext::from_f64(5.0);
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                e.amount = -Ext::from_f64(8.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 5;
                set(&mut e.news_tail, __content.string(13384));
                g.scenario_headline =
                    s(__content.string(13385)) + &e.corp_symbol + __content.string(13386);
                if cs % 2 == 1 {
                    g.scenario_headline = s(__content.string(13387))
                        + &e.corp_symbol
                        + __content.string(13388);
                }
                g.scenario_stage = 9;
            } else {
                e.charge_extraordinary(g);
                e.news_tail = s("does ") + &e.amount_text + __content.string(13389);
            }
        }
        // case -68: 'Lightning' discontinued
        -68 => {
            if e.industry == 68 {
                e.amount = e.market_cap * Ext::from_f64(12.0);
                e.margin -= 9;
                e.nation_name = mc(&e.nation_name);
                e.prefix_the();
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                set(&mut e.news_tail, __content.string(13390));
                e.market_cap = sf(0.8f32) * e.market_cap;
            } else {
                abort(g);
            }
        }
        // case 69: wind farm vs. condors
        69 => {
            e.amount = e.market_cap * Ext::from_f64(6.0);
            e.loss_amt = to_quad(-Ext::from_f64(1.0) * e.amount);
            e.loss_text = PbStr::new();
            set(&mut g.scenario_tag, "ARIZONA");
            if cs % 2 == 1 {
                set(&mut g.scenario_tag, "UTAH");
            }
            e.detail_text = mc(&g.scenario_tag);
            e.subst_tokens(g, ui, rng);
            if yes(g) {
                g.scenario_stage = 6;
                e.amount = -Ext::from_f64(3.0) * e.market_cap;
                e.charge_expense(g);
                e.margin += 5;
                e.news_tail = s(__content.string(13391)) + &e.detail_text + __content.string(13392);
                g.scenario_headline = e.corp_symbol.clone()
                    + __content.string(13393)
                    + &mc(&g.scenario_tag)
                    + __content.string(13392);
                if cs % 3 == 1 {
                    set(&mut e.news_tail, __content.string(13394));
                    g.scenario_headline =
                        s(__content.string(13395)) + &e.corp_symbol + __content.string(13392);
                }
                if cs % 3 == 2 {
                    set(&mut e.news_tail, __content.string(13396));
                    g.scenario_headline = s(__content.string(13397))
                        + &e.corp_symbol
                        + __content.string(13398);
                }
            } else {
                e.charge_extraordinary(g);
                e.news_tail = s(__content.string(13399)) + &e.detail_text + __content.string(13400);
                e.notoriety = e.notoriety.wrapping_sub(1);
            }
        }
        // case -69: wind farm shuttered
        -69 => {
            if e.industry == 69 {
                e.amount = e.market_cap * Ext::from_f64(8.0);
                e.margin -= 12;
                e.loss_amt = to_quad(e.amount);
                e.loss_text = PbStr::new();
                e.detail_text = mc(&g.scenario_tag);
                e.subst_tokens(g, ui, rng);
                e.charge_expense(g);
                set(&mut e.news_tail, __content.string(13401));
                e.market_cap = sf(0.8f32) * e.market_cap;
            } else {
                abort(g);
            }
        }
        // cases 70, -70: no scenario
        70 | -70 => no_scenario(g, ui),
        _ => return false,
    }
    true
}

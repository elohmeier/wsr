//! The random news-event generator `RandomWorldEvents` (0x52d793) and `EventConfiscateGold` (0x6630ab)
//! with `AddRemovePhysCommodRec` (0x65f1bf).
//!
//! RandomWorldEvents picks one of 103 events (`EventState(1..103)` tracks each: 0 = unused, 9 = just
//! happened, 1..8 = repeats / stages, >4 = blocked), applies its effects (tax rates, interest rates,
//! commodity shocks, country disasters, lawsuits against companies, scripted crises …), posts a
//! headline and shows the story (`ShowMiscNewsStory` or a "FINANCIAL NEWS UPDATE" report built from the
//! local `storyLines()`), and returns an "economic effect" code for the caller.
//!
//! UI-only statements of the original are skipped with `// (UI)`: `DIALOG SHOW STATE hDlgMain`,
//! `SHELL DataPath + "PIX.EXE …"` (picture pop-ups) and `PauseTicker`.

// Statements keep the original BASIC shape on purpose.
#![allow(clippy::comparison_to_empty, clippy::neg_multiply)]
use crate::pb::real::{Ext, cast};


use super::*;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::control::update_corp_control;
use crate::logic::finance::{calc_entity_financials, get_phys_commod_value};
use crate::logic::futures::update_metals_price_base;
use crate::logic::valuation::{
    calc_industry_outlook, calc_stock_price, price_pct_change_months, price_trend_streak,
    update_main_commodity_display,
};
use crate::pb::string::{mcase, parse_any, ucase, CaseMode};

/// `STR$(x)` (PB_StrNum).
fn strs(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

fn uc(x: &PbStr) -> PbStr {
    ucase(x.as_bytes(), CaseMode::Ansi)
}

fn mc(x: &PbStr) -> PbStr {
    mcase(x.as_bytes(), CaseMode::Ansi)
}

/// `FORMAT$(x, mask)` (untrimmed).
fn fmt(x: Ext, mask: &str) -> PbStr {
    PbStr::from(format(x, mask.as_bytes()))
}

/// The routine's locals.
#[derive(Default)]
struct Frame {
    line_count: i32,
    event_id: i32,
    econ_effect: i32,
    /// QUAD locals.
    pick: i64,
    story_num: i64,
    top_corp: i64,
    fungus_damage: i64,
    ref_id: i32,
    country_id: i32,
    ent_id: i32,
    damage_amt: Ext,
    lost_value: Ext,
    demand_factor: Ext,
    name_text: PbStr,
    qualifier_text: PbStr,
    extra_text: PbStr,
    reason_text: PbStr,
    detail_text: PbStr,
    /// `DIM storyLines(1 TO 100)`.
    story_lines: Vec<PbStr>,
}

impl Frame {
    fn line(&mut self, i: i32, text: PbStr) {
        self.story_lines[cast::<usize, _>(i)] = text;
    }
}

/// Port of RandomWorldEvents (0x52d793).
/// Draws one random world event (see the module docs) and returns its economic-effect code (0 when
/// nothing happened). Nothing happens in the first quarter of a new game with at most 500 companies.
///
/// `time_secs` is passed to [`calc_stock_price`]. The ON ERROR handler only shows ShowRuntimeError.
pub fn random_world_events(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) -> i32 {
    let __content = crate::content::current();
    let mut f = Frame { story_lines: vec![PbStr::new(); 101], ..Frame::default() };
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 501 {
        return f.econ_effect;
    }
    loop {
        f.line_count += 1;
        f.event_id = rng.rnd_range(103, 1);
        if 4 < g.event_state[f.event_id] {
            f.event_id = -1;
        }
        if f.line_count == 1000 {
            f.econ_effect = 0;
            return f.econ_effect;
        }
        if 1 < g.cur_quarter && g.start_year < g.cur_year && g.crypto_ban_state != 1 {
            crypto_ban_rumor_event(g, ui, rng, 0);
        }
        if 0 < g.event_state[32] && g.event_state[32] < 5 && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(5.0) {
            f.event_id = 32;
        }
        if g.cur_year % 4 == 1 && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) && -1 < g.corp_shares_tax_country {
            f.event_id = 98;
        }
        if g.start_year + 4 < g.cur_year && rng.rnd() * Ext::from_f64(15.0) < Ext::from_f64(1.0) {
            if g.peak_oil_crisis == 0
                && g.subprime_crisis_state == 0
                && g.pandemic_state == 0
                && g.cur_year < g.start_year + 9
            {
                if g.event_state[13] == 0 {
                    f.event_id = 95;
                }
            } else if 0 < g.peak_oil_crisis {
                f.story_num = to_quad(rng.rnd() * Ext::from_f64(3.0));
                f.event_id = 96;
                if g.event_state[96] != 0 || (cast::<Ext, _>(f.story_num)) < Ext::from_f64(1.0) {
                    f.event_id = 97;
                }
                f.story_num = 0;
            }
        }
        if 0 < g.peak_oil_crisis && g.oil_crisis_stage == 20 {
            f.event_id = 97;
        }
        if g.event_state[32] == 7 && g.event_state[37] == 0 {
            f.event_id = 37;
        }
        if 0 < f.event_id {
            break;
        }
    }
    let ret_val = g.start_year + 3;
    if rng.rnd_range(5, 0) + ret_val < g.cur_year
        && g.peak_oil_crisis == 0
        && g.subprime_crisis_state == 0
        && g.pandemic_state == 0
    {
        if 9 < rng.rnd_range(10, 1) && g.event_state[76] == 0 {
            f.event_id = 76;
        }
    }
    if g.crypto_ban_state == -1 && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
        f.event_id = 102;
    }
    f.line_count = 0;
    let e = f.event_id;
    if g.event_state[e] == 0 {
        g.event_state[e] = 9;
    } else {
        g.event_state[e] += 1;
    }

    match e {
        1 => event_housing_boom(g, &mut f),
        2..=8 => event_tax_change(g, rng, &mut f),
        9..=31 => events_9_31(g, ui, rng, &mut f),
        32..=36 => event_trade_war(g, ui, rng, &mut f),
        37..=40 => events_37_40(g, ui, rng, time_secs, &mut f),
        41..=76 => event_world_news(g, ui, rng, &mut f),
        77 | 80..=87 => events_77_87(g, ui, rng, &mut f),
        88..=90 => event_product_liability(g, ui, rng, time_secs, &mut f),
        91..=102 => events_91_102(g, ui, rng, &mut f),
        _ => {
            // CASE ELSE
            f.line_count = 0;
            g.event_state[e] = 0;
        }
    }

    // LAB_00537877
    if g.news_msg != "" {
        post_news(g, ui, 0, 0);
    }
    if f.line_count < 1 {
        if f.line_count == 0 {
            f.econ_effect = 0;
        }
    } else {
        let end = f.line_count + 4;
        f.line(end, s("@END"));
        let mut i = 1;
        while i < 91 {
            if f.story_lines[cast::<usize, _>(i)] == "@END" {
                g.text_lines[i] = f.story_lines[cast::<usize, _>(i)].clone();
                i = 90;
            } else {
                g.text_lines[i] = s("   ") + &f.story_lines[cast::<usize, _>(i)];
            }
            i += 1;
        }
        g.text_box_title = s(__content.string(4583));
        g.entity_pick_mode = PbStr::new();
        // (UI) DIALOG SHOW STATE hDlgMain, 0
        g.text_has_links = 1;
        show_text_report(g, ui, rng);
        // (UI) DIALOG SHOW STATE hDlgMain, 5: PauseTicker(2)
    }
    f.econ_effect
}

/// CASE 1: housing boom.
fn event_housing_boom(g: &mut Globals, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    if g.event_state[e] == 9 {
        g.event_state[e] = 4;
    }
    f.line(5, s(__content.string(4584)));
    f.line(7, s(__content.string(4585)));
    f.line(8, s(__content.string(4586)));
    f.line(9, s(__content.string(4587)));
    f.line_count = 10;
    f.econ_effect = 75;
    g.housing_starts = to_long(sf(1.3f32) * cast::<Ext, _>(g.housing_starts));
    g.news_msg = s(__content.string(4588));
}

/// CASE 2 TO 8: corporate tax rates raised or lowered.
fn event_tax_change(g: &mut Globals, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    f.pick = to_quad(Ext::from_f64(52.0) - rng.rnd() * Ext::from_f64(8.0));
    if g.difficulty_level == 2 {
        f.pick = to_quad(Ext::from_f64(48.0) - rng.rnd() * Ext::from_f64(6.0));
    }
    if g.difficulty_level == 1 {
        f.pick = to_quad(Ext::from_f64(44.0) - rng.rnd() * Ext::from_f64(4.0));
    }
    if f.pick < cast::<i64, _>(g.corp_tax_rate) {
        shock_commodity_price(g, rng, b"GOLD", -20);
        f.pick = cast::<i64, _>(g.corp_tax_rate);
        g.corp_tax_rate = to_long(cast::<Ext, _>(g.corp_tax_rate) - (int(rng.rnd() * Ext::from_f64(5.0)) + Ext::from_f64(3.0)));
        if 12 < g.cap_gains_tax_rate {
            g.cap_gains_tax_rate -= 5;
        }
        f.detail_text = s("lowered");
        f.reason_text = s(__content.string(4589));
        if Ext::from_f64(1.5) < cast::<Ext, _>(g.gdp_growth) {
            f.reason_text = s(__content.string(4590));
            let t = s(__content.string(4591)) + &strs(cast::<Ext, _>(f.pick)) + __content.string(4592);
            f.line(7, t);
        }
        f.econ_effect = 7;
    } else {
        shock_commodity_price(g, rng, b"GOLD", 20);
        g.corp_tax_rate = rng.rnd_range(3, 1) + g.corp_tax_rate + g.difficulty_level;
        g.cap_gains_tax_rate =
            to_long((cast::<Ext, _>(g.difficulty_level) + cast::<Ext, _>(g.cap_gains_tax_rate)) + rng.rnd() * Ext::from_f64(4.0));
        f.detail_text = s("raised");
        if e == 2 {
            f.reason_text = s(__content.string(4593))
                + &strs(cast::<Ext, _>(g.cap_gains_tax_rate))
                + "%.";
            f.line(8, s(__content.string(4594)));
            f.line(9, s(__content.string(4595)));
            if (cast::<Ext, _>(g.gdp_growth)) < Ext::from_f64(1.0) {
                f.line(9, s(__content.string(4596)));
            }
        }
        if e == 3 {
            f.reason_text = s(__content.string(4597));
        }
        if e == 4 {
            f.reason_text = s(__content.string(4598));
            f.line(7, s(__content.string(4599)));
            if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                f.line(7, s(__content.string(4600)));
            }
        }
        f.pick = to_quad(int(rng.rnd() * Ext::from_f64(3.0)));
        if e == 5 || e == 6 {
            f.qualifier_text = s(__content.string(4601));
            if f.pick == 1 {
                f.qualifier_text = s(__content.string(4602));
            }
            if f.pick == 2 {
                f.qualifier_text = s(__content.string(4603));
            }
            f.reason_text = s(__content.string(4604)) + &f.qualifier_text;
            if f.pick < 2 {
                f.reason_text = s(__content.string(4605)) + &f.qualifier_text;
            }
        }
        if 6 < e {
            f.qualifier_text = s(__content.string(4606));
            if e == 7 && f.pick == 1 {
                f.qualifier_text = s(__content.string(4607));
                if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                    f.qualifier_text = s("housing");
                }
            }
            if e == 7 && f.pick == 2 {
                f.qualifier_text = s(__content.string(4608));
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                    f.qualifier_text = s(__content.string(4609));
                }
            }
            if e == 8 {
                f.qualifier_text = s(__content.string(4610));
                if f.pick == 2 {
                    f.qualifier_text = s(__content.string(4611));
                }
            }
            if e == 8 && f.pick == 3 {
                f.qualifier_text = s(__content.string(4612));
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                    f.qualifier_text = s(__content.string(4613));
                }
            }
            f.ent_id = rng.rnd_range(6, 1);
            f.extra_text = s(match f.ent_id {
                1 => __content.string(4614),
                2 => __content.string(4615),
                3 => __content.string(4616),
                4 => __content.string(4617),
                5 => __content.string(4618),
                _ => __content.string(4619),
            });
            f.reason_text = s(__content.string(4620));
            if 44 < f.qualifier_text.len() + f.extra_text.len() {
                f.reason_text = s("give ");
            }
            f.reason_text = s(__content.string(4621)) + &f.reason_text + &f.qualifier_text + " to " + &f.extra_text;
        }
        if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
            f.line_count = 0;
        } else {
            f.econ_effect = 68;
        }
    }
    f.qualifier_text = s("MAJOR ");
    f.extra_text = s("CUT");
    if f.detail_text == "raised" {
        f.extra_text = s("INCREASE");
        if 41 < g.corp_tax_rate {
            f.qualifier_text = s(__content.string(4622));
        }
        if 45 < g.corp_tax_rate {
            f.qualifier_text = s("YET ") + &f.qualifier_text;
        }
    }
    let t = s(&" ".repeat(18)) + &f.qualifier_text + "TAX " + &f.extra_text + __content.string(4623);
    f.line(3, t);
    let t = s(__content.string(4624))
        + &f.detail_text
        + __content.string(4625)
        + &strs(cast::<Ext, _>(g.corp_tax_rate))
        + "%,";
    f.line(5, t);
    let t = s("  ") + &f.reason_text;
    f.line(6, t);
    if 40 < g.corp_tax_rate && f.detail_text == "raised" {
        f.ent_id = 8;
        if e == 2 {
            f.ent_id = 12;
        }
        if e == 4 {
            f.ent_id = 9;
        }
        let q = "\"";
        let k = f.ent_id;
        if g.corp_tax_rate < 44 {
            f.line(k, s(__content.string(4626)));
            f.line(k + 1, s(__content.string(4627)) + q + __content.string(4628));
            f.line(k + 2, s(__content.string(4629)) + q);
        } else if g.corp_tax_rate < 47 {
            f.line(k, s(__content.string(4630)));
            f.line(k + 1, s("  that ") + q + __content.string(4631));
            f.line(k + 2, s(__content.string(4632)) + q);
        } else if g.corp_tax_rate < 52 {
            f.line(k, s(__content.string(4633)) + q + __content.string(4634));
            f.line(k + 1, s(__content.string(4635)) + q);
        }
    }
    f.line_count = 16;
    g.news_msg = s(__content.string(4636))
        + &f.detail_text
        + " to"
        + &strs(cast::<Ext, _>(g.corp_tax_rate))
        + "%.";
}

/// `SubprimeCrisisState = -2` with probability `1 - k/n` (`RND * n > k`) when no crisis is running.
fn maybe_start_subprime(g: &mut Globals, rng: &mut Rng, n: Ext, k: Ext) {
    if g.subprime_crisis_state == 0 && g.peak_oil_crisis == 0 && g.pandemic_state == 0 && k < rng.rnd() * n {
        g.subprime_crisis_state = -2;
    }
}

/// CASE 9 … 31.
fn events_9_31(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    match e {
        9 | 10 => {
            if g.corp_tax_rate < 32 || sf(2.5f32) < cast::<Ext, _>(g.gdp_growth) {
                g.event_state[e] = 0;
                return;
            }
            g.corp_tax_rate = to_long((rng.rnd() * sf(0.35f32) + sf(0.5f32)) * cast::<Ext, _>(g.corp_tax_rate));
            if 12 < g.cap_gains_tax_rate {
                g.cap_gains_tax_rate = to_long(sf(0.6f32) * cast::<Ext, _>(g.cap_gains_tax_rate));
            }
            f.line(3, s(__content.string(4637)));
            let t = s(__content.string(4638))
                + &strs(cast::<Ext, _>(5 + g.corp_tax_rate))
                + __content.string(4639);
            f.line(5, t);
            f.line(6, s(__content.string(4640)));
            f.line(7, s(__content.string(4641)));
            f.line(8, s(__content.string(4642)));
            f.detail_text = s(__content.string(4643));
            if sf(0.8f32) < cast::<Ext, _>(g.gdp_growth) {
                f.detail_text = s(__content.string(4644));
            }
            g.news_msg = s(__content.string(4645)) + &strs(cast::<Ext, _>(g.corp_tax_rate)) + "% " + &f.detail_text;
            f.econ_effect = 63;
            f.line_count = 9;
        }
        11 | 12 => {
            if e == 11 {
                if (cast::<Ext, _>(g.gdp_growth)) < Ext::from_f64(1.0) {
                    f.detail_text = s("SLASHES");
                    g.cap_gains_tax_rate = to_long(sf(0.65f32) * cast::<Ext, _>(g.cap_gains_tax_rate));
                    g.corp_tax_rate -= 5;
                    f.econ_effect = 73;
                }
            } else if Ext::from_f64(3.0) < cast::<Ext, _>(g.gdp_growth) {
                f.detail_text = s("BOOSTS");
                g.cap_gains_tax_rate = to_long(sf(1.3f32) * cast::<Ext, _>(g.cap_gains_tax_rate));
                f.econ_effect = 77;
            }
            if f.detail_text == "" {
                g.event_state[e] = 0;
            } else {
                g.news_msg = s(__content.string(4130)) + &f.detail_text + __content.string(4646);
            }
            if g.corp_tax_rate + 5 < g.cap_gains_tax_rate {
                g.cap_gains_tax_rate = g.corp_tax_rate + 3;
            }
            f.line_count = -1;
        }
        13 => {
            if g.cur_year < g.start_year + 6 {
                g.event_state[e] = 0;
                return;
            }
            f.pick = to_quad(cast::<Ext, _>(rng.rnd_range(3, 1)));
            g.econ_cycle_counter = -10;
            if 100 < g.bubble_state && g.bubble_state < 108 {
                f.pick = 0;
            }
            if sf(1.5f32) < cast::<Ext, _>(g.oil_price) {
                f.pick = 2;
            }
            if f.pick == 1 {
                f.detail_text = s(__content.string(4647));
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                    f.detail_text = s(__content.string(4648));
                }
                if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                    f.detail_text = s(__content.string(4649));
                }
                g.ind_name[14] = s(__content.string(4650));
                g.news_msg = s(__content.string(4651)) + &f.detail_text + ".";
                f.econ_effect = 10;
                show_misc_news_story(g, ui, rng, 67, f.detail_text.as_bytes());
                if 0 < g.peak_oil_crisis {
                    g.peak_oil_crisis = -1;
                    post_news(g, ui, 0, 0);
                    g.news_msg = s(__content.string(4652));
                    g.event_state[95] = 9;
                    g.event_state[96] = 9;
                    g.event_state[97] = 9;
                }
            } else if f.pick == 2 {
                g.news_msg = s(__content.string(4653));
                f.econ_effect = 59;
                show_misc_news_story(g, ui, rng, 41, f.name_text.as_bytes());
                if 0 < g.peak_oil_crisis {
                    g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_crisis_level) * sf(0.7f32)));
                }
                g.peak_oil_crisis = -1;
                g.event_state[95] = 9;
                g.event_state[96] = 9;
                g.event_state[97] = 9;
            } else {
                if 0 < g.peak_oil_crisis {
                    g.econ_cycle_counter = 0;
                    g.event_state[e] = 0;
                    return;
                }
                f.econ_effect = 83;
                g.news_msg = s(__content.string(4654));
                show_misc_news_story(g, ui, rng, 42, f.name_text.as_bytes());
            }
            f.line_count = -1;
        }
        14 => {
            f.detail_text = s("Cairo");
            f.reason_text = s("Iran");
            if g.rand_seed % 2 == 1 {
                f.detail_text = s(__content.string(4655));
            }
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                f.reason_text = s("Syria");
            }
            g.tmp_news_str = f.reason_text.clone();
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) || g.event_state[e] == 5 {
                f.qualifier_text = s(__content.string(4656));
                show_misc_news_story(g, ui, rng, 65, f.name_text.as_bytes());
            } else {
                g.event_state[e] = 4;
                f.qualifier_text = s(__content.string(4657));
                show_misc_news_story(g, ui, rng, 66, f.detail_text.as_bytes());
            }
            f.line_count = -1;
            f.econ_effect = 25;
            g.news_msg = s(__content.string(4658)) + &f.qualifier_text;
        }
        15 => {
            if 0 < g.peak_oil_crisis {
                g.event_state[15] = 0;
                return;
            }
            f.line_count = -1;
            f.econ_effect = 6;
            g.news_msg = s(__content.string(4659));
            show_misc_news_story(g, ui, rng, 43, f.name_text.as_bytes());
        }
        16 => {
            if 0 < g.monetary_policy {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            f.econ_effect = 80;
            g.news_msg = s(__content.string(4660));
            show_misc_news_story(g, ui, rng, 44, f.name_text.as_bytes());
            g.base_interest_rate = cast::<f32, _>(Ext::from_f32(4.5f32));
            shock_commodity_price(g, rng, b"GOLD", 24);
            if rng.rnd() * Ext::from_f64(5.0) < Ext::from_f64(2.0) {
                let base = update_metals_price_base(g, ui);
                if Ext::from_f64(200.0) < base && cast::<Ext, _>(g.metals_price_base) * sf(0.01f32) + sf(8.5f32) < cast::<Ext, _>(g.silver_price) {
                    g.gold_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.75f32) * cast::<Ext, _>(g.gold_price)));
                    g.gold_price_min = cast::<f32, _>(g.gold_target_price);
                    g.silver_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.7f32) * cast::<Ext, _>(g.silver_price)));
                    g.silver_price_min = cast::<f32, _>(g.silver_target_price);
                }
            }
            shock_commodity_price(g, rng, b"OIL", 26);
            shock_commodity_price(g, rng, b"CORN", 28);
        }
        17 => {
            if 0 < g.peak_oil_crisis || g.pandemic_state < -1 || 0 < g.pandemic_state {
                g.event_state[17] = 0;
                return;
            }
            if g.monetary_policy < 2 || -Ext::from_f64(1.0) < cast::<Ext, _>(g.gdp_growth) {
                g.event_state[e] = 0;
                return;
            }
            if 0 <= price_trend_streak(g, 7)
                || -10 < price_pct_change_months(g, 7, 12)
                || Ext::from_f64(1340.0) < update_metals_price_base(g, ui)
            {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            f.econ_effect = 56;
            g.news_msg = s(__content.string(4661));
            g.base_interest_rate = cast::<f32, _>(Ext::from_f32(10.0f32));
            shock_commodity_price(g, rng, b"GOLD", -34);
            shock_commodity_price(g, rng, b"OIL", -17);
            shock_commodity_price(g, rng, b"CORN", -10);
            show_misc_news_story(g, ui, rng, 45, f.name_text.as_bytes());
            if g.subprime_crisis_state == 0 && g.peak_oil_crisis == 0 && g.pandemic_state == 0 {
                g.subprime_crisis_state = -2;
            }
        }
        18 => {
            if 0 < g.monetary_policy || sf(1.5f32) > cast::<Ext, _>(g.gdp_growth) || check_index_hist_low(g) == 1 {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            f.econ_effect = 92;
            g.news_msg = s(__content.string(4662));
            g.base_interest_rate = cast::<f32, _>(Ext::from_f32(6.0f32));
            f.name_text = s("Kushner");
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                f.name_text = s("Cuomo");
            }
            // (UI) DIALOG SHOW STATE / SHELL PIX.EXE "SWA|"
            show_misc_news_story(g, ui, rng, 46, f.name_text.as_bytes());
            maybe_start_subprime(g, rng, Ext::from_f64(4.0), Ext::from_f64(3.0));
        }
        19 => {
            if 0 < g.peak_oil_crisis || g.cur_year < g.start_year + 4 {
                g.event_state[e] = 0;
                return;
            }
            if g.start_year + 12 < g.cur_year && g.bubble_state == 0 {
                g.bubble_state = -1;
            }
            if g.bubble_state == 0 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                g.bubble_state = -1;
            }
            g.news_msg = s(__content.string(4663));
            if g.bubble_state != 0 {
                g.event_state[e] = 9;
            } else {
                f.line_count = -1;
                f.econ_effect = 20;
                g.bubble_state = 200;
            }
        }
        20 => {
            if g.event_state[19] == 0 || g.bubble_state < 208 {
                g.event_state[20] = 0;
                return;
            }
            f.line_count = -1;
            f.econ_effect = 76;
            f.country_id = rng.rnd_range(52, 1);
            country_disaster_shock(g, ui, rng, f.country_id);
            f.name_text = mc(&g.country_name[f.country_id]);
            g.news_msg = s(__content.string(4664)) + &f.name_text + ".";
            shock_commodity_price(g, rng, b"WHEAT", 60);
            // (UI) SHELL PIX.EXE "SWA|"
            show_misc_news_story(g, ui, rng, 47, f.name_text.as_bytes());
        }
        21 => {
            if sf(3.5f32) > cast::<Ext, _>(g.gdp_growth) || check_index_hist_low(g) == 1 {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            f.econ_effect = 22;
            f.name_text = s(__content.string(4665));
            if g.rand_seed % 3 == 1 {
                f.name_text = s(__content.string(4666));
            }
            if g.rand_seed % 3 == 2 {
                f.name_text = s("Fatback");
            }
            g.news_msg = s(__content.string(4667)) + &f.name_text + __content.string(4668);
            // (UI) SHELL PIX.EXE "SWA|"
            show_misc_news_story(g, ui, rng, 48, f.name_text.as_bytes());
            maybe_start_subprime(g, rng, Ext::from_f64(2.0), Ext::from_f64(1.0));
        }
        22 | 23 => {
            if 0 < g.peak_oil_crisis {
                g.event_state[e] = 0;
                return;
            }
            g.event_state[22] = 9;
            g.event_state[23] = 9;
            f.line_count = -1;
            g.econ_cycle_counter = -5;
            f.econ_effect = 26;
            if e == 22 && rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                g.news_msg = s(__content.string(4669));
                f.pick = 49;
            } else {
                f.name_text = s(__content.string(4670));
                g.news_msg = s(__content.string(4671));
                f.pick = 50;
                if g.rand_seed % 3 == 1 {
                    f.detail_text = s(__content.string(4672));
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        f.detail_text = s(__content.string(4673));
                    }
                    if Ext::from_f64(3.0) < rng.rnd() * Ext::from_f64(4.0) {
                        f.detail_text = s(__content.string(4674));
                    }
                    f.name_text = s(__content.string(4675));
                    g.news_msg = s(__content.string(4676)) + &f.name_text + &f.detail_text;
                    f.name_text = f.name_text.clone() + &f.detail_text;
                } else if g.rand_seed % 3 == 2 {
                    f.name_text = s(__content.string(4677));
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        f.name_text = s(__content.string(4678));
                    }
                    g.news_msg = s(__content.string(4679)) + &f.name_text;
                }
            }
            g.news_msg = uc(&g.news_msg);
            f.ref_id = to_long(cast::<Ext, _>(f.pick));
            show_misc_news_story(g, ui, rng, f.ref_id, f.name_text.as_bytes());
        }
        24 | 25 => {
            let oil = cast::<Ext, _>(g.oil_price);
            if sf(0.8f32) < oil && oil < sf(1.3f32) && cast::<Ext, _>(g.base_interest_rate) <= sf(7.5f32) {
                g.event_state[e] = 0;
                return;
            }
            if sf(0.8f32) < oil || 1 <= g.peak_oil_crisis {
                if oil < sf(1.3f32) {
                    f.detail_text = s("Greece");
                    if g.rand_seed % 3 == 1 {
                        f.detail_text = s("Spain");
                    }
                    if g.rand_seed % 3 == 2 {
                        f.detail_text = s("Ireland");
                    }
                } else {
                    f.detail_text = s("Chile");
                    if e == 25 {
                        f.detail_text = s(__content.string(4680));
                    }
                    f.reason_text = s(__content.string(4681));
                    f.econ_effect = 1;
                }
            } else {
                f.detail_text = s("Mexico");
                if e == 25 {
                    f.detail_text = s(__content.string(4682));
                }
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) && e == 25 {
                    f.detail_text = s(__content.string(4683));
                }
                f.reason_text = s(__content.string(4684));
                f.econ_effect = 31;
            }
            f.extra_text = PbStr::new();
            if sf(9.7f32) < cast::<Ext, _>(g.base_interest_rate) {
                f.extra_text = s(__content.string(4685));
            }
            let t = uc(&f.detail_text) + __content.string(4686);
            f.line(2, t);
            let t = f.detail_text.clone() + __content.string(4687);
            f.line(5, t);
            if f.detail_text.len() < 9 {
                let t = f.story_lines[5].clone() + " to";
                f.line(5, t);
            }
            f.line(6, s(__content.string(4688)));
            let t = s(__content.string(4689)) + &f.detail_text + __content.string(4690);
            f.line(7, t);
            let t = s(__content.string(4691)) + &f.reason_text + __content.string(4692) + &f.extra_text + ".";
            f.line(8, t);
            let r4 = mid(f.reason_text.as_bytes(), 1, 4);
            g.news_msg = f.detail_text.clone()
                + __content.string(4693)
                + &rtrim(ltrim(r4.as_bytes()).as_bytes())
                + __content.string(4694);
            f.line_count = 9;
            if sf(0.8f32) < oil && oil < sf(1.3f32) {
                f.line(8, s(__content.string(4695)));
                f.line(9, s(__content.string(4696)));
                f.line_count = 10;
                g.news_msg = f.detail_text.clone() + __content.string(4697);
                g.market_val_target = cast::<f64, _>(sf(0.85f32) * g.market_val_target);
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.95f32) * cast::<Ext, _>(g.market_valuation)));
            }
        }
        26 | 27 => {
            if price_trend_streak(g, 1602) < 3 || Ext::from_f64(7.0) > cast::<Ext, _>(g.base_interest_rate) {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            f.econ_effect = 5;
            if e == 26 {
                f.pick = 51;
                g.news_msg = s(__content.string(4698));
            } else if e == 27 {
                f.pick = 52;
                g.news_msg = s(__content.string(4699));
            }
            f.ref_id = to_long(cast::<Ext, _>(f.pick));
            show_misc_news_story(g, ui, rng, f.ref_id, f.name_text.as_bytes());
        }
        28 | 29 => {
            if Ext::from_f64(2.0) > cast::<Ext, _>(g.gdp_growth) || update_metals_price_base(g, ui) < Ext::from_f64(1750.0) {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            f.econ_effect = if e == 28 { 19 } else { 21 };
            f.pick = to_quad(cast::<Ext, _>(rng.rnd_range(5, 1)));
            match f.pick {
                1 => {
                    f.name_text = s(__content.string(4700));
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        f.name_text = s(__content.string(4701));
                    }
                }
                2 => {
                    f.name_text = s(__content.string(4702));
                    if sf(1.5f32) < cast::<Ext, _>(g.oil_price) {
                        f.name_text = s(__content.string(4703));
                    }
                }
                3 => {
                    f.detail_text = s("Korea");
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        f.detail_text = s("France");
                    }
                    f.name_text = s(__content.string(4704)) + &f.detail_text;
                }
                4 => {
                    f.name_text = s(__content.string(4705));
                    shock_commodity_price(g, rng, b"GOLD", 20);
                    if Ext::from_f64(9.0) < cast::<Ext, _>(g.base_interest_rate) {
                        f.name_text = s(__content.string(4706));
                        shock_commodity_price(g, rng, b"GOLD", -14);
                    }
                }
                _ => {
                    f.name_text = s(__content.string(4707));
                }
            }
            f.detail_text = s("PANIC");
            if rng.rnd() * Ext::from_f64(2.0) < Ext::from_f64(1.0) {
                f.detail_text = s("NIGHTMARE");
            }
            g.news_msg = f.detail_text.clone() + __content.string(4708);
            if f.pick == 5 {
                g.news_msg = s(__content.string(4709));
            }
            if f.pick == 3 {
                g.news_msg = s(__content.string(4710));
            }
            f.pick = if e == 29 { 54 } else { 53 };
            f.ref_id = to_long(cast::<Ext, _>(f.pick));
            show_misc_news_story(g, ui, rng, f.ref_id, f.name_text.as_bytes());
            maybe_start_subprime(g, rng, Ext::from_f64(3.0), Ext::from_f64(2.0));
        }
        30 | 31 => {
            if Ext::from_f64(3.0) > cast::<Ext, _>(g.gdp_growth) {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            if e == 30 {
                f.econ_effect = 78;
            } else {
                f.line_count = 0;
            }
            g.news_msg = s(__content.string(4711));
        }
        _ => {}
    }
}

/// CASE 32 TO 36: the trade-war sequence (stage in `EventState(32)`).
fn event_trade_war(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    if (g.event_state[32] == 0 || g.event_state[32] == 9)
        && (Ext::from_f64(2.0) > cast::<Ext, _>(g.gdp_growth) || sf(5.2f32) < cast::<Ext, _>(g.gdp_growth))
    {
        g.event_state[32] = 0;
        return;
    }
    if 32 < e && g.event_state[32] < 6 {
        g.event_state[e] = 0;
    }
    if g.event_state[32] == 9 || g.event_state[32] == 0 {
        g.event_state[32] = 0;
        if Ext::from_f64(1.0) <= rng.rnd() * Ext::from_f64(4.0) {
            g.event_state[32] = 6;
        } else {
            g.event_state[32] = 0;
        }
    }
    if 5 < g.event_state[32] {
        return;
    }
    f.line_count = -1;
    if 6 <= g.event_state[32] {
        f.line_count = 0;
        return;
    }
    if 32 < e {
        g.event_state[32] += 1;
    }
    if g.event_state[32] == 0 {
        g.event_state[32] = 1;
    }
    f.pick = cast::<i64, _>(g.event_state[32]);
    if 5 < f.pick {
        f.pick = 5;
    }
    if 2 < f.pick && f.pick < 6 && g.cur_quarter == 4 {
        f.pick = 5;
        f.detail_text = s("Settle");
    }
    f.name_text = s("Rubio");
    // IF VAL(MID$(DATE$, 1, 2)) > 15 THEN nameText = "Warren": the month is never above 12.
    if g.news_msg != "" {
        post_news(g, ui, 0, 0);
    }
    match f.pick {
        1 => {
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(rng.rnd() * Ext::from_f64(1.0) + Ext::from_f64(3.0)));
            f.name_text = fmt_trim(cast::<Ext, _>(g.gdp_growth_target), "##.0");
            f.econ_effect = 11;
            g.news_msg = s(__content.string(4712));
            if (cast::<Ext, _>(g.gdp_growth)) < Ext::from_f64(3.0) {
                g.gdp_growth = cast::<f32, _>(cast::<f32, _>(sf(0.5f32) + cast::<Ext, _>(g.gdp_growth)));
            }
            if Ext::from_f64(4.0) < cast::<Ext, _>(g.gdp_growth) {
                g.gdp_growth = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth) - sf(0.6f32)));
            }
        }
        2 => {
            g.news_msg = s(__content.string(4713));
            f.econ_effect = 46;
        }
        3 => {
            g.news_msg = s(__content.string(4714));
            f.econ_effect = 27;
        }
        4 => {
            g.news_msg = s(__content.string(4715));
            f.econ_effect = 77;
        }
        5 => {
            if Ext::from_f64(1.0) <= rng.rnd() * Ext::from_f64(3.0) && f.detail_text != "Settle" {
                f.pick = 6;
                f.econ_effect = 87;
                g.news_msg = s(__content.string(4716));
                // (UI) SHELL PIX.EXE "SWA|"
                g.event_state[32] = 7;
            } else {
                f.econ_effect = 71;
                g.news_msg = s(__content.string(4717));
                g.event_state[32] = 6;
            }
        }
        _ => {}
    }
    let story = to_long(cast::<Ext, _>(f.pick + 54));
    show_misc_news_story(g, ui, rng, story, f.name_text.as_bytes());
    if g.news_msg != "" {
        post_news(g, ui, 0, 0);
    }
}

/// CASE 37 … 40.
fn events_37_40(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    match e {
        37 => {
            f.line_count = -1;
            if g.event_state[32] == 6 {
                return;
            }
            if g.event_state[32] != 7 {
                g.event_state[e] = 0;
                return;
            }
            f.econ_effect = 92;
            if g.corp_tax_rate < 55 {
                g.corp_tax_rate = 55;
                g.cap_gains_tax_rate = 40;
            }
            show_misc_news_story(g, ui, rng, 61, f.name_text.as_bytes());
            // (UI) SHELL PIX.EXE "ZIM|"
        }
        38 => {
            if g.cur_year < g.start_year + 5 || 2030 < g.cur_year {
                g.event_state[e] = 0;
                return;
            }
            let cc = &g.currency_code;
            if !(*cc == "JAP" || *cc == "SWE" || *cc == "NOR" || *cc == "DAN") {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            country_disaster_shock(g, ui, rng, 47);
            f.name_text = str_trim(cast::<Ext, _>(g.cur_year - 1949));
            g.news_msg = s(__content.string(4718));
            // (UI) SHELL PIX.EXE "SWA|"
            show_misc_news_story(g, ui, rng, 62, f.name_text.as_bytes());
        }
        39 => {
            if 0 < g.peak_oil_crisis {
                g.event_state[e] = 0;
                return;
            }
            if g.start_year + 12 < g.cur_year && g.bubble_state == 0 {
                g.bubble_state = -1;
            }
            if g.cur_year < g.start_year + 4 {
                g.event_state[e] = 0;
                return;
            }
            if g.bubble_state == 0 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                g.bubble_state = -1;
            }
            show_misc_news_story(g, ui, rng, 63, f.name_text.as_bytes());
            g.news_msg = s(__content.string(4719));
            f.econ_effect = 18;
            if g.bubble_state != 0 {
                g.event_state[e] = 9;
            } else {
                f.line_count = -1;
                g.bubble_state = 100;
            }
        }
        40 => event_malpractice(g, ui, rng, time_secs, f),
        _ => {}
    }
}

/// CASE 40: punitive damages against the largest HMO (industry 56), paid partly by a random insurer.
fn event_malpractice(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    if g.cur_year < g.start_year + 5 {
        g.event_state[e] = 0;
        return;
    }
    if g.event_state[e] == 9 {
        g.event_state[e] = 4;
    }
    f.damage_amt = Ext::from_f64(0.0);
    f.line_count = 0;
    f.econ_effect = 0;
    f.ent_id = 131;
    while f.ent_id <= 1600 {
        let id = f.ent_id;
        if g.industry_num[id] == 56 && g.entity_status[id] == 1 {
            f.line_count += 1;
            if f.damage_amt < g.market_cap[id] {
                f.damage_amt = g.market_cap[id];
                f.top_corp = cast::<i64, _>(id);
            }
            if f.line_count == 7 {
                f.ent_id = 1600;
            }
        }
        f.ent_id += 1;
    }
    if (cast::<Ext, _>(f.top_corp)) < Ext::from_f64(131.0) {
        g.event_state[e] = 0;
        return;
    }
    let top = to_long(cast::<Ext, _>(f.top_corp));
    f.pick = to_quad(rng.rnd() * Ext::from_f64(7000.0) + Ext::from_f64(5000.0));
    f.pick = to_quad(g.exchange_rate * cast::<Ext, _>(f.pick));
    f.damage_amt = cast::<Ext, _>(f.pick) * sf(0.35f32);
    g.cash[top] = g.cash[top] - f.damage_amt;
    g.extraordinary_pl[top] = to_quad(cast::<Ext, _>(g.extraordinary_pl[top]) - f.damage_amt);
    g.tmp_ext1 = cast::<Ext, _>(f.pick);
    g.tmp_ext2 = cast::<Ext, _>(f.top_corp);
    f.ent_id = to_long(cast::<Ext, _>(f.pick) * sf(0.65f32));
    f.name_text = rtrim(ltrim(g.currency_symbol.as_bytes()).as_bytes())
        + &fmt(sf(0.9f32) * cast::<Ext, _>(f.ent_id), "#####")
        + " "
        + &g.million_prefix_lc
        + "illion";
    g.news_msg = g.ent_symbol[top].clone()
        + __content.string(4720)
        + &g.currency_symbol
        + &fmt_trim(cast::<Ext, _>(f.pick), "#####")
        + " "
        + &g.million_prefix_lc
        + __content.string(4721);
    if g.event_state[e] == 4 {
        show_misc_news_story(g, ui, rng, 64, f.name_text.as_bytes());
        f.line_count = -1;
    } else {
        let t = uc(&g.news_msg);
        f.line(2, t);
        f.line(5, s(__content.string(4722)));
        f.line(6, s(__content.string(4723)));
        f.line(7, s(__content.string(4724)));
        let t = s(__content.string(4725)) + &g.ent_name[top] + __content.string(4726);
        f.line(8, t);
        let t = s("of ")
            + &g.currency_symbol
            + &fmt_trim(cast::<Ext, _>(f.pick), "#####")
            + " "
            + &g.million_prefix_lc
            + "illion.";
        f.line(9, t);
        f.line_count = 11;
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.tmp_ext2 = Ext::from_f64(0.0);
    }
    f.pick = to_quad(cast::<Ext, _>(rng.rnd_range(130, 86)));
    let ins = to_long(cast::<Ext, _>(f.pick));
    if g.entity_status[ins] == 1 {
        let amt = cast::<Ext, _>(f.ent_id);
        g.cash[ins] = g.cash[ins] - amt;
        g.insur_uw_profit_ytd[ins] = g.insur_uw_profit_ytd[ins] - amt;
        g.accrued_pl[ins] = g.accrued_pl[ins] - amt;
        f.ref_id = top;
        post_news(g, ui, f.ref_id, 0);
        g.news_msg = s(__content.string(4727))
            + &g.ent_symbol[ins]
            + __content.string(4728)
            + &f.name_text
            + " in "
            + &g.ent_symbol[top]
            + __content.string(4729);
        f.ref_id = ins;
        calc_projected_earnings(g, ui, rng, f.ref_id);
        g.investor_sentiment[ins] -= 15;
        g.valuation_mode = 3;
        f.ref_id = ins;
        calc_stock_price(g, ui, rng, f.ref_id, time_secs);
    } else {
        f.damage_amt = g.tmp_ext1 * sf(0.65f32);
        g.cash[top] = g.cash[top] - f.damage_amt;
        g.extraordinary_pl[top] = to_quad(cast::<Ext, _>(g.extraordinary_pl[top]) - f.damage_amt);
        f.ref_id = top;
        post_news(g, ui, f.ref_id, 0);
    }
    f.ref_id = top;
    calc_projected_earnings(g, ui, rng, f.ref_id);
    g.investor_sentiment[top] -= 20;
    g.valuation_mode = 3;
    f.ref_id = top;
    calc_stock_price(g, ui, rng, f.ref_id, time_secs);
}

/// CASE 41 TO 76: odd world news (at most one story until `WorldEventState` is reset by event 76).
fn event_world_news(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    if e == 76 {
        g.world_event_state = 0;
    }
    'sel: {
        if g.world_event_state != 0 || Ext::from_f64(1.0) <= rng.rnd() * Ext::from_f64(7.0) {
            break 'sel;
        }
        g.world_event_state = 1;
        f.pick = cast::<i64, _>(e - 40);
        let sy = g.start_year;
        let cy = g.cur_year;
        match f.pick {
            1 | 2 | 4 | 5 | 7..=10 | 14 | 16..=22 | 25 => {
                if cy < sy + 4 {
                    g.world_event_state = 0;
                } else if f.pick == 9 {
                    g.news_msg = s(__content.string(4730));
                } else if f.pick == 10 {
                    g.news_msg = s(__content.string(4731));
                } else if f.pick == 20 {
                    g.news_msg = s(__content.string(4732));
                }
            }
            3 => {
                if sy + 10 < cy || cy < sy + 5 {
                    g.world_event_state = 0;
                } else {
                    g.news_msg = s(__content.string(4733));
                }
            }
            6 => {
                shock_commodity_price(g, rng, b"WHEAT", 20);
                if g.pandemic_state == 0
                    && g.peak_oil_crisis < 1
                    && g.subprime_crisis_state < 1
                    && sf(1.5f32) < cast::<Ext, _>(g.gdp_growth)
                    && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0)
                {
                    g.pandemic_state = -3;
                    g.news_msg = s(__content.string(4734));
                }
            }
            11 => {
                g.news_msg = s(__content.string(4735));
            }
            13 => {
                g.ind_demand[3] = to_quad(sf(1.15f32) * cast::<Ext, _>(g.ind_demand[3]));
                g.ind_demand[62] = to_quad(sf(1.2f32) * cast::<Ext, _>(g.ind_demand[62]));
                g.ind_long_term_growth[62] = g.ind_long_term_growth[62].wrapping_add(3);
                g.news_msg = s(__content.string(4736));
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                    g.news_msg = s(__content.string(4737));
                }
            }
            15 => {
                if cy < sy + 5 || sy + 12 < cy || g.peak_oil_crisis == -1 {
                    g.world_event_state = 0;
                } else {
                    f.name_text = strs(cast::<Ext, _>(cy - 1));
                    g.news_msg = s(__content.string(4738));
                }
            }
            23 => {
                if cy < sy + 4 {
                    g.world_event_state = 0;
                } else {
                    f.ent_id = g.cur_quarter % 2 + 11;
                    let id = f.ent_id;
                    f.damage_amt = g.market_cap[id] * Ext::from_f64(6.0);
                    g.cash[id] = g.cash[id] - f.damage_amt;
                    g.extraordinary_pl[id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[id]) - f.damage_amt);
                    g.news_msg = g.ent_symbol[id].clone() + __content.string(4739);
                }
            }
            24 => {
                if cy < sy + 4 {
                    g.world_event_state = 0;
                } else {
                    country_disaster_shock(g, ui, rng, 50);
                    g.news_msg = s(__content.string(4740));
                }
            }
            26 => {
                if cy < sy + 4 {
                    g.world_event_state = 0;
                } else {
                    g.news_msg = s(__content.string(4741));
                }
            }
            27 => {
                if cy < sy + 5 || sy + 10 < cy {
                    g.world_event_state = 0;
                }
            }
            28 => {
                if cy < sy + 7 {
                    g.world_event_state = 0;
                }
                f.name_text = strs(cast::<Ext, _>(cy - 3));
            }
            29 => {
                if !(g.currency_code == "ICE" || g.currency_code == "SWI") {
                    g.world_event_state = 0;
                    break 'sel;
                }
                if cy < sy + 19 {
                    g.world_event_state = 0;
                }
                f.name_text = s("Wall");
                g.tmp_news_str = s(__content.string(4742));
                g.news_msg = s(__content.string(4743));
            }
            30 => {
                if cy < sy + 3 {
                    g.world_event_state = 0;
                } else {
                    f.story_num = to_quad(cast::<Ext, _>(rng.rnd_range(240, 181)));
                    let id = to_long(cast::<Ext, _>(f.story_num));
                    if g.entity_status[id] == 1 {
                        g.asbestos_corp = to_long(-Ext::from_f64(1.0) * cast::<Ext, _>(f.story_num));
                        g.news_msg = g.ent_symbol[id].clone() + __content.string(4744);
                    }
                }
            }
            31 => {
                if cy < sy + 2 || sy + 12 < cy {
                    g.world_event_state = 0;
                } else {
                    f.top_corp = 0;
                    f.story_num = 0;
                    for id in 131..=1600 {
                        if g.entity_status[id] == 1 && g.country_idx[id] == 27 {
                            f.damage_amt = g.market_cap[id] * Ext::from_f64(3.0) + g.business_assets[id] * sf(0.2f32);
                            if f.damage_amt < Ext::from_f64(100.0) {
                                f.damage_amt = Ext::from_f64(100.0);
                            }
                            if (cast::<Ext, _>(f.top_corp)) < f.damage_amt {
                                f.top_corp = to_quad(f.damage_amt);
                                f.story_num = cast::<i64, _>(id);
                            }
                        }
                    }
                    g.news_msg = s(__content.string(4745));
                    post_news(g, ui, 0, 0);
                    g.news_msg = s(__content.string(4746));
                    post_news(g, ui, 0, 0);
                    if 0 < f.story_num {
                        let id = to_long(cast::<Ext, _>(f.story_num));
                        g.news_msg =
                            g.ent_symbol[id].clone() + __content.string(4747);
                    }
                    country_disaster_shock(g, ui, rng, 27);
                    // (UI) SHELL PIX.EXE "SWA|"
                    f.econ_effect = 92;
                }
            }
            32 => {
                if cy < sy + 3 || sy + 13 < cy {
                    g.world_event_state = 0;
                } else {
                    g.news_msg = s(__content.string(4748));
                    country_disaster_shock(g, ui, rng, 26);
                    // (UI) SHELL PIX.EXE "SWA|"
                    f.econ_effect = 92;
                }
            }
            33 => {
                if cy < sy + 6 || sy + 9 < cy {
                    g.world_event_state = 0;
                } else {
                    g.news_msg = s(__content.string(4749));
                }
            }
            34 => {
                if cy < 2042 || 2043 < cy {
                    g.world_event_state = 0;
                } else {
                    f.story_num = 286;
                    if cy == 2043 {
                        f.story_num = 298;
                    }
                    f.name_text = str_trim(cast::<Ext, _>(f.story_num));
                }
            }
            35 => {
                if cy < sy + 3 || (cast::<Ext, _>(g.oil_target_price)) < sf(1.22f32) || g.peak_oil_crisis != 0 {
                    g.world_event_state = 0;
                } else {
                    shock_commodity_price(g, rng, b"OIL", -22);
                    g.news_msg = s(__content.string(4750));
                }
            }
            36 => {
                if g.pandemic_state != 0 {
                    break 'sel;
                }
                if cy < sy + 3
                    || g.oil_crisis_level != Ext::from_f32(0.0f32)
                    || 0 < g.subprime_crisis_state
                    || g.subprime_crisis_state < -1
                    || sf(1.5f32) > cast::<Ext, _>(g.gdp_growth)
                {
                    g.world_event_state = 0;
                } else {
                    g.news_msg = s(__content.string(4751));
                    g.pandemic_state = -2;
                    for id in 11..=1600 {
                        if g.entity_status[id] == 1 {
                            g.market_cap[id] = sf(0.992f32) * g.market_cap[id];
                        }
                    }
                    g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(3.0)));
                    // (UI) SHELL PIX.EXE "SWA|"
                }
            }
            _ => {}
        }
    }
    // LAB_00533710
    if g.world_event_state == 0 {
        g.event_state[e] = 0;
    } else {
        f.ref_id = to_long(cast::<Ext, _>(f.pick));
        if g.world_event_state != -1 {
            show_misc_news_story(g, ui, rng, f.ref_id, f.name_text.as_bytes());
        }
        g.world_event_state = -1;
        if f.pick == 30 || f.pick == 35 {
            g.world_event_state = 0;
        }
    }
    f.line_count = 0;
    if f.pick == 31 || f.pick == 32 {
        f.line_count = -1;
        g.base_interest_rate = cast::<f32, _>(Ext::from_f32(9.0f32));
    }
}

/// CASE 77, 80 … 87.
fn events_77_87(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    match e {
        77 => {
            if g.cur_year < g.start_year + 3 || sf(1.5f32) > cast::<Ext, _>(g.gdp_growth) {
                g.event_state[e] = 0;
                return;
            }
            if g.oil_crisis_level != Ext::from_f32(0.0f32) && g.pandemic_state == 0 {
                g.event_state[e] = 0;
                return;
            }
            if g.pandemic_state == 0 && g.subprime_crisis_state == 0 {
                g.news_msg = s(__content.string(4752));
                g.pandemic_state = 6;
                // (UI) SHELL PIX.EXE "SWA|"
            }
        }
        80 => {
            if Ext::from_f64(2.0) < cast::<Ext, _>(g.gdp_growth) {
                g.event_state[e] = 0;
                return;
            }
            if g.subprime_crisis_state == 0 && g.peak_oil_crisis == 0 && g.pandemic_state == 0 {
                g.subprime_crisis_state = -2;
            }
            f.line_count = -1;
            f.country_id = 17;
            if g.rand_seed % 4 == 1 {
                f.country_id = 15;
            }
            if g.rand_seed % 4 == 2 {
                f.country_id = 25;
            }
            if g.rand_seed % 4 == 3 {
                f.country_id = 43;
            }
            f.extra_text = g.country_display_name[f.country_id].clone();
            if f.country_id == 25 {
                f.extra_text = s("ROME");
            }
            g.news_msg = s(__content.string(4753)) + &f.extra_text;
            g.tmp_news_str = f.extra_text.clone();
            f.name_text = mc(&g.country_name[f.country_id]);
            country_disaster_shock(g, ui, rng, f.country_id);
            for id in 11..=1600 {
                if g.entity_status[id] == 1 {
                    f.story_num = cast::<i64, _>(g.country_idx[id]);
                    match f.story_num {
                        3 | 4 | 12 | 15..=17 | 23 | 25 | 30 | 37 | 43 => {
                            g.market_cap[id] = sf(0.86f32) * g.market_cap[id];
                        }
                        0 | 2 | 6 | 8 | 26 | 27 | 31 | 50 => {
                            g.market_cap[id] = sf(0.92f32) * g.market_cap[id];
                        }
                        _ => {}
                    }
                }
            }
            if sf(0.85f32) < cast::<Ext, _>(g.market_valuation) {
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.97f32) * cast::<Ext, _>(g.market_valuation)));
            }
            show_misc_news_story(g, ui, rng, 70, f.name_text.as_bytes());
            f.econ_effect = 70;
        }
        81 | 82 => {
            if g.peak_oil_crisis != 0 {
                g.event_state[81] = 9;
                g.event_state[82] = 9;
                return;
            }
            if (cast::<Ext, _>(g.oil_price)) < sf(1.1f32) && g.event_state[e] == 9 {
                g.event_state[e] = 0;
                return;
            }
            if f.event_id == 82 && g.event_state[81] == 0 {
                g.event_state[81] = 5;
                g.event_state[f.event_id] = 0;
                f.event_id = 81;
            }
            if f.event_id == 81 {
                f.line_count = -1;
                g.news_msg = s(__content.string(4754));
                f.econ_effect = 61;
            } else {
                f.line_count = -1;
                g.news_msg = s(__content.string(4755));
                show_misc_news_story(g, ui, rng, 101, f.name_text.as_bytes());
                f.econ_effect = 62;
            }
        }
        83 => {
            if g.cur_year < g.start_year + 15 || 0 < g.peak_oil_crisis || sf(1.8f32) < cast::<Ext, _>(g.gdp_growth) {
                g.event_state[e] = 0;
                return;
            }
            g.econ_cycle_counter = -10;
            f.line_count = -1;
            show_misc_news_story(g, ui, rng, 102, f.name_text.as_bytes());
            if g.peak_oil_crisis == -1 && Ext::from_f32(0.0f32) < g.oil_crisis_level && (cast::<Ext, _>(g.oil_crisis_level)) < sf(2.9f32) {
                g.oil_crisis_level = cast::<f32, _>(Ext::from_f32(2.9f32));
            }
            f.econ_effect = 90;
        }
        84 => {
            f.ent_id = to_long(int(rng.rnd() * Ext::from_f64(40.0)) + Ext::from_f64(144.0));
            let id = f.ent_id;
            f.fungus_damage = to_quad(g.business_assets[id] * sf(0.6f32));
            if !(g.entity_status[id] == 1
                && g.industry_num[id] == 4
                && g.exchange_rate * Ext::from_f64(10.0) <= cast::<Ext, _>(f.fungus_damage))
            {
                g.event_state[e] = 0;
                return;
            }
            g.cash[id] = g.cash[id] - cast::<Ext, _>(f.fungus_damage);
            g.extraordinary_pl[id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[id]) - cast::<Ext, _>(f.fungus_damage));
            calc_projected_earnings(g, ui, rng, id);
            f.line_count = 0;
            g.tmp_ext1 = cast::<Ext, _>(f.fungus_damage);
            g.tmp_ext2 = cast::<Ext, _>(id);
            f.name_text = g.ent_symbol[id].clone();
            show_misc_news_story(g, ui, rng, 103, f.name_text.as_bytes());
            g.news_msg = f.name_text.clone() + __content.string(4756);
            f.econ_effect = 51;
        }
        85 => {
            if g.cur_year < g.start_year + 8 || 0 < g.peak_oil_crisis {
                g.event_state[e] = 0;
                return;
            }
            if g.cur_year <= g.start_year + 13 {
                f.line_count = -1;
                show_misc_news_story(g, ui, rng, 104, f.name_text.as_bytes());
                f.econ_effect = 64;
                g.news_msg = s(__content.string(4757));
            }
        }
        86 => {
            if g.cur_quarter < 3 || g.event_state[85] == 0 {
                g.event_state[e] = 0;
                return;
            }
            if g.cur_quarter == 3 && g.sim_co_count < 1005 {
                g.event_state[e] = 0;
                return;
            }
            g.event_state[85] = 9;
            f.line_count = -1;
            show_misc_news_story(g, ui, rng, 105, f.name_text.as_bytes());
            f.econ_effect = 70;
            country_disaster_shock(g, ui, rng, 15);
            country_disaster_shock(g, ui, rng, 16);
            country_disaster_shock(g, ui, rng, 30);
            country_disaster_shock(g, ui, rng, 50);
            g.news_msg = s(__content.string(4758));
        }
        87 => {
            if (cast::<Ext, _>(g.base_interest_rate)) < sf(11.5f32)
                || g.corp_tax_rate < 40
                || Ext::from_f64(1.0) < cast::<Ext, _>(g.gdp_growth)
                || (cast::<Ext, _>(g.gdp_growth)) < -Ext::from_f64(14.0)
            {
                g.event_state[e] = 0;
                return;
            }
            f.story_num = to_quad(rng.rnd() * Ext::from_f64(40.0) + Ext::from_f64(20.0));
            f.name_text = str_trim(cast::<Ext, _>(f.story_num));
            f.damage_amt = cast::<Ext, _>(f.story_num) * sf(0.01f32);
            for id in 1..=1600 {
                f.pick = 0;
                f.lost_value = Ext::from_f64(0.0);
                if !(id <= g.num_players || 10 < id) || g.entity_status[id] != 1 {
                    continue;
                }
                if Ext::from_f64(2.0) < g.t_bills[id] {
                    f.lost_value = g.t_bills[id] * f.damage_amt;
                    g.t_bills[id] = g.t_bills[id] * (Ext::from_f64(1.0) - f.damage_amt);
                    f.pick = to_quad(f.lost_value);
                }
                if id < 131 {
                    if 2 < g.long_gov_bond_face[id] {
                        g.long_gov_bond_face[id] = to_quad(cast::<Ext, _>(g.long_gov_bond_face[id]) * (Ext::from_f64(1.0) - f.damage_amt));
                        f.lost_value = g.lt_gov_bond_cost[id] * f.damage_amt;
                        g.lt_gov_bond_cost[id] = g.lt_gov_bond_cost[id] - f.lost_value;
                        f.pick = to_quad(cast::<Ext, _>(f.pick) + f.lost_value);
                    }
                    if 2 < g.short_gov_bond_face[id] {
                        g.short_gov_bond_face[id] =
                            to_quad(cast::<Ext, _>(g.short_gov_bond_face[id]) * (Ext::from_f64(1.0) - f.damage_amt));
                        f.lost_value = g.st_gov_bond_cost[id] * f.damage_amt;
                        g.st_gov_bond_cost[id] = g.st_gov_bond_cost[id] - f.lost_value;
                        f.pick = to_quad(cast::<Ext, _>(f.pick) + f.lost_value);
                    }
                }
                if 0 < f.pick {
                    if id < 11 {
                        g.player_cap_gains[id] = g.player_cap_gains[id] - cast::<Ext, _>(f.pick);
                    } else {
                        g.extraordinary_pl[id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[id]) - cast::<Ext, _>(f.pick));
                    }
                }
            }
            f.line_count = -1;
            g.tmp_news_str = g.currency_name.clone();
            // (UI) SHELL PIX.EXE "SWA|"
            show_misc_news_story(g, ui, rng, 106, f.name_text.as_bytes());
            f.econ_effect = 91;
            g.tmp_news_str = uc(&g.tmp_news_str);
            if 0 < instr(1, g.tmp_news_str.as_bytes(), b"EUROPEAN") {
                g.tmp_news_str = s("EURO");
            }
            g.news_msg = s(__content.string(4759)) + &g.tmp_news_str + __content.string(4760);
        }
        _ => {}
    }
}

/// CASE 88 TO 90: a product- [runtime resource wording] a random company of an exposed industry.
fn event_product_liability(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    f.line_count = 0;
    loop {
        f.pick = to_quad(cast::<Ext, _>(rng.rnd_range(1600, 131)));
        if g.entity_status[to_long(cast::<Ext, _>(f.pick))] != 1 {
            f.pick = 0;
        }
        if 130 < f.pick {
            let ind = g.industry_num[to_long(cast::<Ext, _>(f.pick))];
            match ind {
                3 | 4 | 6 | 15 | 16 | 17 | 20 | 21 | 24 | 28 | 29 | 30 | 34 | 51 | 57 | 67..=69 => {
                    if 0 < g.drug_cancel_news_done && (ind == 4 || ind == 21) {
                        f.pick = 0;
                    }
                    if 130 < f.pick && g.entity_status[to_long(cast::<Ext, _>(f.pick))] != 1 {
                        f.pick = 0;
                    }
                }
                _ => f.pick = 0,
            }
        }
        f.line_count += 1;
        if 130 < f.pick || 500 < f.line_count {
            break;
        }
    }
    f.line_count = 0;
    if f.pick == 0 {
        g.event_state[e] = 0;
        return;
    }
    let id = to_long(cast::<Ext, _>(f.pick));
    if cast::<i32, _>(g.industry_num[id]) == g.last_rand_event_ind {
        g.event_state[e] = 0;
        return;
    }
    g.last_rand_event_ind = cast::<i32, _>(g.industry_num[id]);
    f.ref_id = id;
    calc_entity_financials(g, ui, rng, f.ref_id);
    if g.net_worth < g.exchange_rate * Ext::from_f64(500.0) {
        if g.event_state[e] == 9 {
            g.event_state[e] = 0;
        }
        if 0 < g.event_state[e] {
            g.event_state[e] -= 1;
        }
        return;
    }
    f.damage_amt = g.business_assets[id] * sf(0.3f32) + g.net_worth * (sf(0.8f32) * rng.rnd() + sf(0.4f32));
    f.damage_amt = sf(0.5f32) * f.damage_amt;
    f.name_text = rtrim(ltrim(g.currency_symbol.as_bytes()).as_bytes())
        + &fmt_trim(f.damage_amt, "* #################")
        + " "
        + &g.million_prefix_uc
        + ".";
    f.detail_text = PbStr::new();
    if g.event_state[e] == 9 {
        g.event_state[e] = 1;
    }
    let sym = g.ent_symbol[id].clone();
    match g.industry_num[id] {
        3 | 67 | 68 => {
            f.story_num = 107;
            f.detail_text = sym + __content.string(4761);
            g.tmp_news_str = s("makes");
            if g.ent_name[id].len() < 15 {
                g.tmp_news_str = s(__content.string(4762));
            }
        }
        4 | 21 => {
            f.story_num = 130;
            g.tmp_news_str = s(__content.string(4763));
            if g.industry_num[id] == 21 {
                g.tmp_news_str = s(__content.string(4764));
            }
            if g.ent_name[id].len() < 17 {
                g.tmp_news_str = g.tmp_news_str.clone() + __content.string(4765);
            }
            if g.ent_name[id].len() < 11 {
                g.tmp_news_str = g.tmp_news_str.clone() + " today";
            }
            f.detail_text = sym + __content.string(4766);
            g.drug_cancel_news_done = 1;
        }
        6 => f.story_num = 108,
        15 => f.story_num = 109,
        16 => {
            f.story_num = 110;
            f.detail_text = s(__content.string(4767)) + &sym + __content.string(4768);
            f.name_text = rtrim(ltrim(g.currency_symbol.as_bytes()).as_bytes())
                + &fmt_trim(f.damage_amt, "* #################")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + ".";
        }
        17 => {
            f.story_num = 111;
            f.detail_text = s(__content.string(4769)) + &sym;
        }
        20 | 51 => {
            f.story_num = 112;
            f.detail_text = s(__content.string(4770)) + &sym + __content.string(4771);
        }
        24 => f.story_num = 113,
        28 => f.story_num = 114,
        29 => {
            f.story_num = 115;
            f.detail_text = sym + __content.string(4772);
        }
        30 => f.story_num = 116,
        34 => f.story_num = 117,
        57 => {
            f.story_num = 118;
            f.detail_text = s(__content.string(4773)) + &sym + __content.string(4774);
        }
        69 => {
            f.story_num = 119;
            f.detail_text = sym + __content.string(4775);
        }
        _ => f.story_num = 0,
    }
    if f.story_num == 0 {
        g.event_state[e] = 0;
        return;
    }
    f.line_count = -1;
    g.cash[id] = g.cash[id] - f.damage_amt;
    g.extraordinary_pl[id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[id]) - f.damage_amt);
    g.investor_sentiment[id] = -40;
    g.valuation_mode = 3;
    f.ref_id = id;
    calc_stock_price(g, ui, rng, f.ref_id, time_secs);
    calc_projected_earnings(g, ui, rng, f.ref_id);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = cast::<Ext, _>(f.pick);
    f.ref_id = to_long(cast::<Ext, _>(f.story_num));
    show_misc_news_story(g, ui, rng, f.ref_id, f.name_text.as_bytes());
    if f.detail_text == "" {
        g.news_msg = g.ent_symbol[id].clone()
            + __content.string(4776)
            + &f.name_text
            + __content.string(4777);
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
            g.news_msg =
                g.ent_symbol[id].clone() + __content.string(4778) + &f.name_text + __content.string(4779);
        }
    } else {
        g.news_msg = f.detail_text.clone();
    }
    // IF pick = CalcIndustryOutlook(IndustryNum(pick)) OR RND * 2 > 1 (see BUGS.md): the outlook is
    // compared with the company id, so in practice only the coin flip decides.
    let ind = g.industry_num[id];
    let outlook = calc_industry_outlook(g, rng, ind);
    if cast::<Ext, _>(outlook) == cast::<Ext, _>(f.pick) || Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
        let ind = cast::<i32, _>(g.industry_num[id]);
        g.ind_demand[ind] = to_quad(cast::<Ext, _>(g.ind_demand[ind]) * sf(0.9f32));
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    maybe_start_subprime(g, rng, Ext::from_f64(3.0), Ext::from_f64(2.0));
}

/// CASE 91 … 102.
fn events_91_102(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    match e {
        91 => {
            if sf(4.75f32) < cast::<Ext, _>(g.base_interest_rate)
                || sf(1.3f32) < cast::<Ext, _>(g.gdp_growth)
                || instr(1, g.central_bank_name.as_bytes(), __content.bytes(4373)) == 0
                || (cast::<Ext, _>(g.gdp_growth)) < cast::<Ext, _>(g.gdp_growth_target)
                || 0 < g.monetary_policy
            {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            show_misc_news_story(g, ui, rng, 68, f.name_text.as_bytes());
            g.news_msg = s(__content.string(4780));
            shock_commodity_price(g, rng, b"GOLD", 70);
            if g.oil_crisis_level == Ext::from_f32(0.0f32) && (cast::<Ext, _>(g.oil_price)) < sf(1.7f32) {
                shock_commodity_price(g, rng, b"OIL", 50);
                g.futures_basis[1] = 285;
            }
            // (UI) SHELL PIX.EXE "BEN|"
        }
        92 => {
            if g.cur_year < g.start_year + 5 {
                g.event_state[e] = 0;
                return;
            }
            f.demand_factor = sf(0.85f32);
            g.ind_growth_forecast[31] = g.ind_growth_forecast[31].wrapping_sub(8);
            g.ind_long_term_growth[31] = g.ind_long_term_growth[31].wrapping_sub(2);
            if g.rand_seed % 5 < 3 {
                f.demand_factor = sf(1.15f32);
                g.ind_growth_forecast[31] = g.ind_growth_forecast[31].wrapping_add(18);
                g.ind_long_term_growth[31] = g.ind_long_term_growth[31].wrapping_add(4);
            }
            g.ind_demand[31] = to_quad(f.demand_factor * cast::<Ext, _>(g.ind_demand[31]));
            f.line_count = -1;
            show_misc_news_story(g, ui, rng, 69, f.name_text.as_bytes());
            g.news_msg = s(__content.string(4781));
            if f.demand_factor < Ext::from_f64(1.0) {
                g.news_msg = s(__content.string(4782));
            }
        }
        93 => {
            if g.cur_year <= g.start_year + 9 {
                f.line_count = -1;
                show_misc_news_story(g, ui, rng, 120, f.name_text.as_bytes());
                g.news_msg = s(__content.string(4783));
            }
        }
        94 => {
            if g.cur_year < g.start_year + 10 || g.event_state[93] == 0 || check_index_hist_low(g) == 1 {
                g.event_state[e] = 0;
                return;
            }
            if g.cur_year <= g.start_year + 15 {
                f.line_count = -1;
                // (UI) SHELL PIX.EXE "SWA|"
                show_misc_news_story(g, ui, rng, 121, f.name_text.as_bytes());
                f.econ_effect = 88;
                country_disaster_shock(g, ui, rng, 0);
                country_disaster_shock(g, ui, rng, 5);
                g.news_msg = s(__content.string(4784));
            }
        }
        95 => {
            if g.cur_year < g.start_year + 5 || g.start_year + 8 < g.cur_year || (cast::<Ext, _>(g.oil_price)) < sf(0.85f32) {
                g.event_state[95] = 0;
                return;
            }
            if !(g.pandemic_state == 0 && g.subprime_crisis_state == 0 && g.peak_oil_crisis == 0) {
                g.event_state[95] = 0;
                return;
            }
            f.line_count = -1;
            // (UI) SHELL PIX.EXE "SWA|"
            show_misc_news_story(g, ui, rng, 122, f.name_text.as_bytes());
            f.econ_effect = 91;
            g.peak_oil_crisis = 1;
            g.oil_crisis_level = cast::<f32, _>(Ext::from_f32(3.0f32));
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                g.peak_oil_crisis = -1;
                g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>(sf(0.7f32) * rng.rnd() + Ext::from_f64(2.0)));
                let base = Ext::from_f64(1.0) + cast::<Ext, _>(g.oil_price);
                g.oil_price = cast::<f32, _>(cast::<f32, _>(base + Ext::from_f64(2.0) * rng.rnd()));
            }
            if 200 < g.bubble_state {
                if g.bubble_state < 208 {
                    g.bubble_state = 207;
                }
            } else if 100 < g.bubble_state && g.bubble_state < 108 {
                g.bubble_state = 107;
            }
        }
        96 => {
            if g.peak_oil_crisis == 0 {
                g.event_state[96] = 0;
                return;
            }
            f.line_count = -1;
            if 0 <= g.peak_oil_crisis {
                g.news_msg = s(__content.string(4785));
                f.econ_effect = 91;
                show_misc_news_story(g, ui, rng, 123, f.name_text.as_bytes());
            }
        }
        97 => {
            if g.peak_oil_crisis == 0 {
                g.event_state[97] = 0;
                return;
            }
            if g.peak_oil_crisis < 0 {
                return;
            }
            g.peak_oil_crisis = -1;
            g.oil_crisis_stage = 0;
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                g.oil_crisis_level = cast::<f32, _>(cast::<f32, _>((sf(0.2f32) * rng.rnd() + sf(0.4f32)) * cast::<Ext, _>(g.oil_crisis_level)));
                g.oil_price = cast::<f32, _>(cast::<f32, _>((sf(0.3f32) * rng.rnd() + sf(0.5f32)) * cast::<Ext, _>(g.oil_price)));
            }
            for industry in 3i32..=69 {
                match industry {
                    7 | 8 => {
                        g.ind_demand[industry] = to_quad(sf(0.8f32) * cast::<Ext, _>(g.ind_demand[industry]));
                        g.ind_long_term_growth[industry] =
                            cast::<i16, _>(to_quad(sf(0.7f32) * cast::<Ext, _>(g.ind_long_term_growth[industry])));
                    }
                    50 | 62 => {
                        g.ind_demand[industry] = to_quad(sf(0.85f32) * cast::<Ext, _>(g.ind_demand[industry]));
                        g.ind_long_term_growth[industry] =
                            cast::<i16, _>(to_quad(sf(0.7f32) * cast::<Ext, _>(g.ind_long_term_growth[industry])));
                    }
                    _ => {
                        g.ind_demand[industry] = to_quad(sf(1.2f32) * cast::<Ext, _>(g.ind_demand[industry]));
                        let ltg = cast::<Ext, _>(g.ind_long_term_growth[industry]);
                        g.ind_long_term_growth[industry] = cast::<i16, _>(to_quad(ltg.abs() * sf(0.35f32) + ltg));
                    }
                }
            }
            f.line_count = -1;
            show_misc_news_story(g, ui, rng, 124, f.name_text.as_bytes());
        }
        98 => event_shares_tax(g, ui, rng, f),
        99 => {
            if (cast::<Ext, _>(g.wheat_price)) < sf(6.5f32) {
                shock_commodity_price(g, rng, b"WHEAT", 37);
                g.futures_basis[4] = 230;
                g.futures_basis[5] = 240;
                if g.cur_year < g.start_year + 5 || g.start_year + 10 < g.cur_year {
                    f.reason_text = PbStr::new();
                    f.story_num = 126;
                    f.detail_text = s(__content.string(4786));
                } else {
                    f.reason_text = s("CORN");
                    f.story_num = 132;
                    f.detail_text = s(__content.string(4787));
                }
                g.ind_demand[15] = to_quad(sf(1.1f32) * cast::<Ext, _>(g.ind_demand[15]));
            } else if sf(8.2f32) < cast::<Ext, _>(g.wheat_price) {
                shock_commodity_price(g, rng, b"WHEAT", -35);
                g.futures_basis[4] = -55;
                g.futures_basis[5] = -60;
                f.story_num = 127;
                f.detail_text = s(__content.string(4788));
                g.ind_demand[15] = to_quad(sf(0.9f32) * cast::<Ext, _>(g.ind_demand[15]));
                g.ind_demand[28] = to_quad(sf(1.07f32) * cast::<Ext, _>(g.ind_demand[28]));
            } else {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            f.name_text = fmt_trim(cast::<Ext, _>(g.wheat_price), "##.00");
            g.news_msg = s(__content.string(4789)) + &f.detail_text;
            if f.reason_text == "CORN" {
                g.news_msg = f.detail_text.clone();
                f.name_text = fmt_trim(cast::<Ext, _>(g.corn_price), "##.00");
            }
            f.ref_id = to_long(cast::<Ext, _>(f.story_num));
            show_misc_news_story(g, ui, rng, f.ref_id, f.name_text.as_bytes());
        }
        100 => {
            if sf(1.15f32) < cast::<Ext, _>(g.oil_price) && Ext::from_f64(6.0) > cast::<Ext, _>(g.corn_price) {
                shock_commodity_price(g, rng, b"CORN", 48);
                g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.9f32) * cast::<Ext, _>(g.wheat_target_price)));
                g.futures_basis[5] = 275;
                g.futures_basis[4] = 190;
                f.story_num = 128;
                f.detail_text = s(__content.string(4790));
                g.ind_demand[15] = to_quad(sf(1.1f32) * cast::<Ext, _>(g.ind_demand[15]));
                g.ind_demand[28] = to_quad(sf(0.9f32) * cast::<Ext, _>(g.ind_demand[28]));
            } else if sf(0.8f32) > cast::<Ext, _>(g.oil_price) && sf(6.5f32) < cast::<Ext, _>(g.corn_price) {
                shock_commodity_price(g, rng, b"CORN", -40);
                g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.1f32) * cast::<Ext, _>(g.wheat_target_price)));
                g.futures_basis[4] = -80;
                g.futures_basis[5] = -35;
                f.story_num = 129;
                f.detail_text = s(__content.string(4791));
                g.ind_demand[15] = to_quad(sf(0.9f32) * cast::<Ext, _>(g.ind_demand[15]));
                g.ind_demand[28] = to_quad(sf(1.1f32) * cast::<Ext, _>(g.ind_demand[28]));
            } else {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            f.name_text = fmt_trim(cast::<Ext, _>(g.corn_price), "##.00");
            f.ref_id = to_long(cast::<Ext, _>(f.story_num));
            show_misc_news_story(g, ui, rng, f.ref_id, f.name_text.as_bytes());
            g.news_msg = s(__content.string(4792)) + &f.detail_text;
        }
        101 => {
            if update_metals_price_base(g, ui) < Ext::from_f64(2200.0) || sf(0.5f32) < cast::<Ext, _>(g.gdp_growth) {
                g.event_state[e] = 0;
                return;
            }
            f.story_num = to_quad(rng.rnd() * Ext::from_f64(21.0) + Ext::from_f64(40.0));
            f.ref_id = to_long(cast::<Ext, _>(f.story_num));
            if event_confiscate_gold(g, ui, rng, f.ref_id) == 0 {
                g.event_state[e] = 0;
                return;
            }
            f.line_count = -1;
            f.name_text = s(__content.string(4793));
            if g.currency_code == "USA" {
                f.name_text = s(__content.string(2911));
            }
            if g.currency_code == "UK" {
                f.name_text = s(__content.string(2912));
            }
            show_misc_news_story(g, ui, rng, 131, f.name_text.as_bytes());
            g.news_msg =
                s(__content.string(4794)) + &strs(cast::<Ext, _>(f.story_num)) + __content.string(4795);
            g.gold_price = cast::<f32, _>(cast::<f32, _>(sf(0.87f32) * cast::<Ext, _>(g.gold_price)));
            f.econ_effect = 80;
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) && g.event_state[101] == 9 {
                g.event_state[101] = 4;
                let k = cast::<Ext, _>(f.story_num);
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(k * cast::<Ext, _>(g.gold_target_price) * sf(0.01f32)));
                g.gold_price_max = cast::<f32, _>(cast::<f32, _>(k * cast::<Ext, _>(g.gold_price_max) * sf(0.01f32)));
                g.gold_price_min = cast::<f32, _>(cast::<f32, _>(k * cast::<Ext, _>(g.gold_price_min) * sf(0.01f32)));
            }
        }
        102 => {
            if 0 < g.crypto_ban_state {
                return;
            }
            if (cast::<Ext, _>(g.bitcoin_price)) < Ext::from_f64(1000.0) * (Ext::from_f64(10.0) * rng.rnd()) + Ext::from_f64(120000.0) {
                g.event_state[e] = 0;
                return;
            }
            crypto_ban_rumor_event(g, ui, rng, 1);
            if g.crypto_ban_state == 0 {
                g.event_state[e] = 0;
                return;
            }
            if Ext::from_f64(3.0) < rng.rnd() * Ext::from_f64(10.0) {
                g.crypto_ban_state = 2;
                return;
            }
            f.line_count = -1;
            f.name_text = s("China");
            if g.rand_seed % 3 == 1 {
                f.name_text = s(__content.string(4796));
            }
            if g.rand_seed % 3 == 2 {
                f.name_text = s(__content.string(4797));
            }
            g.news_msg = uc(&f.name_text) + __content.string(4798);
            if f.name_text == "China" {
                f.name_text = s(__content.string(4799));
            }
            // (UI) SHELL PIX.EXE "SWA|"
            show_misc_news_story(g, ui, rng, 139, f.name_text.as_bytes());
            g.bitcoin_target_price = to_long(cast::<Ext, _>(g.bitcoin_price) * sf(0.08f32));
            g.bitcoin_price = to_long(cast::<Ext, _>(g.bitcoin_price) * sf(0.09f32));
            g.crypto_ban_state = 1;
            g.ethereum_target_price = to_long(cast::<Ext, _>(g.ethereum_price) * sf(0.12f32));
            g.ethereum_price = to_long(cast::<Ext, _>(g.ethereum_price) * sf(0.14f32));
            update_main_commodity_display(g);
            f.econ_effect = 80;
        }
        _ => {
            // CASE ELSE
            f.line_count = 0;
            g.event_state[e] = 0;
        }
    }
}

/// CASE 98: a foreign country taxes corporate stock (`CorpSharesTaxCountry`), or a court kills the tax.
fn event_shares_tax(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.event_id;
    if g.corp_shares_tax_country < 0 {
        g.event_state[e] = 5;
        return;
    }
    if g.cur_year < g.start_year + 1 {
        g.event_state[e] = 0;
        return;
    }
    if g.rand_seed % 4 == 1 || 0 < g.corp_shares_tax_country {
        if 0 < g.corp_shares_tax_country {
            if g.news_msg != "" {
                post_news(g, ui, 0, 0);
            }
            g.news_msg = s(__content.string(4800));
            post_news(g, ui, 0, 0);
            f.name_text = mc(&g.country_name[g.corp_shares_tax_country]);
            show_misc_news_story(g, ui, rng, 471, f.name_text.as_bytes());
        }
        g.corp_shares_tax_country = -1;
        g.event_state[e] = 5;
        g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.gdp_growth_target)));
    } else {
        f.pick = to_quad(cast::<Ext, _>(rng.rnd_range(52, 1)));
        if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
            f.pick = 52;
        }
        g.corp_shares_tax_country = to_long(cast::<Ext, _>(f.pick));
        f.detail_text = g.country_name[to_long(cast::<Ext, _>(f.pick))].clone();
        f.name_text = mc(&f.detail_text);
        if f.pick == 11 || f.pick == 30 || f.pick == 35 || f.pick == 50 {
            f.name_text = s("the ") + &f.name_text;
        }
        if g.news_msg != "" {
            post_news(g, ui, 0, 0);
        }
        g.news_msg = s(__content.string(4801)) + &f.name_text + __content.string(4802);
        post_news(g, ui, 0, 0);
        g.news_msg = s(__content.string(4803));
        g.tmp_news_str = f.name_text.clone();
        if f.name_text.len() < 12 {
            f.name_text = s("...uh, ") + &f.name_text;
        }
        show_misc_news_story(g, ui, rng, 125, f.name_text.as_bytes());
        if -Ext::from_f64(3.0) < cast::<Ext, _>(g.gdp_growth_target) {
            g.gdp_growth_target = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gdp_growth_target) - Ext::from_f64(3.0)));
        }
    }
    f.line_count = -1;
}

/// Port of EventConfiscateGold (0x6630ab).
/// Confiscates every physical-gold record (`PhysCommodRec`) of players and of companies that are not
/// funds (industry 50) or wholly owned by a bank/insurer (11..130): the owner gets
/// `pct_of_value`% of the record's value (`GetPhysCommodValue`) in cash and the record is removed.
/// Posts a headline for the last owner and returns its id (0 if nothing was confiscated).
///
/// Errors (an owner id outside the arrays) go to the ON ERROR handler (ShowRuntimeError): the routine
/// stops there and returns 0.
pub fn event_confiscate_gold(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, pct_of_value: i32) -> i32 {
    let __content = crate::content::current();
    let payout_fraction = sf(0.01f32) * cast::<Ext, _>(pct_of_value); // fstp qword: exact
    let mut last_owner_id = 0;
    let mut rec_idx = 1;
    loop {
        if g.phys_commod_rec[rec_idx] != "" {
            let rec_text = g.phys_commod_rec[rec_idx].clone();
            let head = rtrim(ltrim(rec_text.as_bytes()).as_bytes());
            if mid(head.as_bytes(), 1, 4) == "GOLD" {
                let owner_id = to_long(val(parse_any(rec_text.as_bytes(), b"|", 2).as_bytes()));
                if owner_id < 1 || 1600 < owner_id {
                    return 0; // error 9 (Cash / IndustryNum) -> ON ERROR handler
                }
                let mut confiscate = owner_id < 11;
                if !confiscate {
                    if g.industry_num[owner_id] != 50 {
                        update_corp_control(g, ui, rng, owner_id);
                        if g.wholly_owned_by < 11 || 130 < g.wholly_owned_by {
                            confiscate = true;
                        }
                    }
                }
                if confiscate {
                    let mut gold_value = get_phys_commod_value(g, owner_id, &s("GOLD"));
                    gold_value = payout_fraction * gold_value;
                    g.cash[owner_id] = gold_value + g.cash[owner_id];
                    if !add_remove_phys_commod_rec(g, b"", rec_idx) {
                        return 0;
                    }
                    rec_idx -= 1;
                    last_owner_id = owner_id;
                }
            }
        }
        rec_idx += 1;
        if 1000 < rec_idx {
            break;
        }
    }
    if last_owner_id == 0 {
        return 0;
    }
    g.news_msg = g.ent_symbol[last_owner_id].clone()
        + __content.string(4804)
        + &strs(cast::<Ext, _>(pct_of_value))
        + __content.string(4805);
    post_news(g, ui, last_owner_id, 0);
    last_owner_id
}

/// Port of AddRemovePhysCommodRec (0x65f1bf).
/// `rec_idx < 1`: stores `rec_text` in the first empty `PhysCommodRec(1..1000)` slot and increments the
/// count in `PhysCommodRec(0)`. Otherwise clears record `rec_idx`, moves the last record (index = count,
/// if above 1 and not empty) into the gap and decrements the count.
///
/// Returns false when the original would raise an error (the count is not a valid index; the ON ERROR
/// handler only shows ShowRuntimeError).
pub fn add_remove_phys_commod_rec(g: &mut Globals, rec_text: &[u8], rec_idx: i32) -> bool {
    let new_rec = PbStr::from(rec_text);
    if rec_idx < 1 {
        for slot in 1..=1000 {
            if g.phys_commod_rec[slot] == "" {
                g.phys_commod_rec[slot] = new_rec;
                let n = to_long(val(g.phys_commod_rec[0].as_bytes()));
                g.phys_commod_rec[0] = str_trim(cast::<Ext, _>(n + 1));
                break;
            }
        }
    } else {
        if 1000 < rec_idx {
            return false;
        }
        g.phys_commod_rec[rec_idx] = PbStr::new();
        let mut last_idx = to_long(val(g.phys_commod_rec[0].as_bytes()));
        if !(0..=1000).contains(&last_idx) {
            return false;
        }
        if g.phys_commod_rec[last_idx] != "" && 1 < last_idx {
            g.phys_commod_rec[rec_idx] = g.phys_commod_rec[last_idx].clone();
            g.phys_commod_rec[last_idx] = PbStr::new();
        }
        if last_idx < 1 {
            last_idx = 0;
        } else {
            last_idx -= 1;
        }
        g.phys_commod_rec[0] = str_trim(cast::<Ext, _>(last_idx));
    }
    true
}

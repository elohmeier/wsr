//! Cases 1..17 (proposals) and -1..-17 (outcomes) of RunScenarioEvent_689361 (0x689361, machine code
//! 0x68a99b..0x68e86f): the industry stories of banks (1), insurers (2), autos (3), drugs (4), real
//! estate (5), forestry (6), shipping (7), oil (8), mining (9), defense (10), railroads (11), steel
//! (12), nanotech (13), robots (14), waste (15), airlines (16) and brewers (17). See
//! [`super::scenario_event`].

#![allow(unused_imports)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::scenario_event::Ev;

/// Runs case `n` of the scenario SELECT if it is one of this file's; returns false otherwise.
pub(super) fn dispatch(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, n: i32) -> bool {
    match n {
        1 => c1(e, g, ui, rng),
        -1 => c_1(e, g, ui, rng),
        2 => c2(e, g, ui, rng),
        -2 => c_2(e, g, ui, rng),
        3 => c3(e, g, ui, rng),
        -3 => c_3(e, g, ui, rng),
        4 => c4(e, g, ui, rng),
        -4 => c_4(e, g, ui, rng),
        5 => c5(e, g, ui, rng),
        -5 => c_5(e, g, ui, rng),
        6 => c6(e, g, ui, rng),
        -6 => c_6(e, g, ui, rng),
        7 => c7(e, g, ui, rng),
        -7 => c_7(e, g, ui, rng),
        8 => c8(e, g, ui, rng),
        -8 => c_8(e, g, ui, rng),
        9 => c9(e, g, ui, rng),
        -9 => c_9(e, g, ui, rng),
        10 => c10(e, g, ui, rng),
        -10 => c_10(e, g, ui, rng),
        11 => c11(e, g, ui, rng),
        -11 => c_11(e, g, ui, rng),
        12 => c12(e, g, ui, rng),
        -12 => c_12(e, g, ui, rng),
        13 => c13(e, g, ui, rng),
        -13 => c_13(e, g, ui, rng),
        14 => c14(e, g, ui, rng),
        -14 => c_14(e, g, ui, rng),
        15 => c15(e, g, ui, rng),
        -15 => c_15(e, g, ui, rng),
        16 => c16(e, g, ui, rng),
        -16 => c_16(e, g, ui, rng),
        17 => c17(e, g, ui, rng),
        -17 => c_17(e, g, ui, rng),
        _ => return false,
    }
    true
}

fn yes(g: &Globals) -> bool {
    g.scenario_cmd == "Y"
}

fn abort(g: &mut Globals) {
    set(&mut g.scenario_cmd, "ABORT");
}

fn ba(g: &Globals) -> Ext {
    g.business_assets[cast::<i32, _>(g.scenario_corp)]
}

// case 1: bank outreach to seniors / foreign loan guarantees
fn c1(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    e.amount = Ext::from_f64(250.0);
    e.subst_tokens(g, ui, rng);
    e.amount = Ext::from_f64(0.0);
    if !yes(g) {
        return;
    }
    if e.show_text == "Y" {
        g.scenario_stage = 8;
        set(&mut e.news_tail, __content.string(12912));
        set(&mut e.aux_text, __content.string(12913));
        g.scenario_headline = s(__content.string(12914)) + &e.corp_symbol + ")" + &e.aux_text;
        if e.clock_sec % 2 == 1 {
            g.scenario_headline = e.corp_symbol.clone() + " denies" + &e.aux_text + __content.string(12915);
        }
    } else {
        g.scenario_stage = 12;
        e.amount = -Ext::from_f64(350.0);
        e.charge_expense(g);
        set(&mut e.news_tail, __content.string(12916));
        set(&mut e.aux_text, "Somalia");
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
            set(&mut e.aux_text, "Haiti");
        }
        g.scenario_headline =
            e.aux_text.clone() + __content.string(12917) + &e.corp_symbol + __content.string(12918);
    }
    e.margin += 5;
}

// case -1: elderly-client scandal / loan guarantees called
fn c_1(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    e.amount = Ext::from_f64(15.0) * e.market_cap;
    if e.amount < g.exchange_rate * Ext::from_f64(300.0) {
        e.amount = Ext::from_f64(300.0) * rng.rnd() * g.exchange_rate + g.exchange_rate * Ext::from_f64(300.0);
    }
    if g.exchange_rate * Ext::from_f64(8000.0) < e.amount {
        e.amount = g.exchange_rate * Ext::from_f64(5000.0);
    }
    if !(e.show_text == "Y") {
        set(&mut e.aux_text, "of ");
        if e.country_idx == 0 || e.country_idx == 11 || e.country_idx == 30 || e.country_idx == 35 {
            set(&mut e.aux_text, "of the ");
        }
        if e.country_idx == 50 {
            set(&mut e.nation_name, __content.string(4562));
        }
        g.scenario_tag = e.aux_text.clone() + &mc(&e.nation_name) + " ";
        e.event_year = 0;
        set(&mut e.detail_text, "Haiti");
        set(&mut e.loss_text, __content.string(12919));
        if 20 < e.clock_sec {
            set(&mut e.detail_text, "Somalia");
            set(&mut e.loss_text, __content.string(12920));
        }
        if 40 < e.clock_sec {
            set(&mut e.detail_text, "Mali");
            set(&mut e.loss_text, __content.string(12921));
        }
    }
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(12922));
        e.notoriety = e.notoriety.wrapping_add(3);
    } else {
        e.news_tail = s(__content.string(12923)) + &e.amount_text + __content.string(12924);
    }
    e.market_cap = e.market_cap * sf(0.73f32);
    e.margin -= 8;
}

// case 2: insurer denies E.R. claims / offshore flood exposure
fn c2(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.show_text == "Y" {
        e.amount = e.market_cap * Ext::from_f64(7.0);
        e.subst_tokens(g, ui, rng);
        e.margin -= 3;
        // (typo "critcism": bug #113)
        set(&mut e.news_tail, __content.string(12925));
        if !yes(g) {
            return;
        }
        e.margin += 4;
        e.amount = -Ext::from_f64(7.0) * e.market_cap;
        e.charge_extraordinary(g);
        set(&mut e.news_tail, __content.string(12926));
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
            repl(&mut e.news_tail, "serious", "stingy");
        }
        g.scenario_headline =
            s(__content.string(12927)) + &e.corp_symbol + __content.string(12928);
        if e.clock_sec % 2 == 1 {
            g.scenario_headline =
                e.corp_symbol.clone() + __content.string(12929);
        }
    } else {
        set(&mut e.misc_text, "Miami");
        if 0 < e.country_idx {
            set(&mut e.misc_text, "Bermuda");
        }
        e.subst_tokens(g, ui, rng);
        if !yes(g) {
            return;
        }
        e.cost_amt = to_quad(e.market_cap * Ext::from_f64(5.0));
        e.amount = cast::<Ext, _>(e.cost_amt) * -Ext::from_f64(1.0);
        e.charge_expense(g);
        e.margin += 3;
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(12930);
        if e.clock_sec % 3 == 1 {
            g.scenario_headline =
                e.corp_symbol.clone() + __content.string(12931);
        }
        if e.clock_sec % 3 == 2 && e.country_idx == 0 {
            g.scenario_headline =
                e.corp_symbol.clone() + __content.string(12932);
        }
    }
    g.scenario_stage = 5;
}

// case -2: insurer fraud / losing licenses
fn c_2(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    e.amount = e.market_cap * Ext::from_f64(9.0);
    e.margin -= 10;
    e.market_cap = sf(0.8f32) * e.market_cap;
    if e.show_text == "Y" {
        e.charge_extraordinary(g);
        e.subst_tokens(g, ui, rng);
        set(&mut e.news_tail, __content.string(12933));
    } else {
        e.charge_expense(g);
        set(&mut g.scenario_tag, __content.string(12934));
        set(&mut e.misc_text, __content.string(5349));
        if 20 < e.clock_sec {
            set(&mut g.scenario_tag, __content.string(12935));
            set(&mut e.misc_text, __content.string(5350));
        }
        if 40 < e.clock_sec {
            set(&mut g.scenario_tag, __content.string(12936));
            set(&mut e.misc_text, "flood");
        }
        e.subst_tokens(g, ui, rng);
        set(&mut e.news_tail, __content.string(12937));
    }
}

// case 3:  [runtime resource wording] / 'Bronto' SUV
fn c3(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    e.amount = ba(g) * sf(0.2f32);
    if e.amount < Ext::from_f64(150.0) {
        e.amount = Ext::from_f64(200.0);
    }
    e.event_year = to_quad(cast::<Ext, _>(e.event_year) - Ext::from_f64(1.0));
    e.subst_tokens(g, ui, rng);
    e.margin += 3;
    set(&mut e.news_tail, __content.string(12938));
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(12939));
    }
    if !yes(g) {
        e.charge_extraordinary(g);
        e.notoriety = e.notoriety.wrapping_sub(1);
        return;
    }
    g.scenario_tag = e.detail_text.clone();
    g.scenario_stage = 6;
    if e.show_text == "Y" {
        set(&mut g.scenario_tag, "BRONTO");
        set(&mut e.news_tail, __content.string(12940));
        g.scenario_headline =
            s(__content.string(12941)) + &e.corp_symbol + __content.string(12942);
        if e.clock_sec % 2 == 1 {
            g.scenario_headline =
                s(__content.string(12943)) + &e.corp_symbol + __content.string(12944);
        }
    } else {
        set(&mut e.news_tail, __content.string(12945));
        g.scenario_headline = s(__content.string(12946))
            + &e.corp_symbol
            + __content.string(12947);
        if e.clock_sec % 3 == 2 {
            g.scenario_headline =
                e.corp_symbol.clone() + __content.string(12948);
        }
    }
    e.amount = -Ext::from_f64(1.0) * e.amount;
    e.charge_expense(g);
}

// case -3: auto defect coverup
fn c_3(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 3 {
        abort(g);
        return;
    }
    e.margin -= 7;
    e.amount = ba(g) * sf(0.3f32);
    e.charge_expense(g);
    e.market_cap = sf(0.73f32) * e.market_cap;
    set(&mut e.misc_text, __content.string(12949));
    if 0 < e.country_idx {
        set(&mut e.misc_text, __content.string(1550));
    }
    e.subst_tokens(g, ui, rng);
    set(&mut e.aux_text, __content.string(12950));
    set(&mut e.phrase, __content.string(12951));
    if e.show_text == "Y" {
        set(&mut e.phrase, __content.string(12952));
    }
    e.news_tail = e.aux_text.clone() + &e.phrase + ".";
}

// case 4: bioweapons / concealed cancer cure
fn c4(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.show_text == "Y" {
        set(&mut g.scenario_tag, "TLAXACOATL");
    } else {
        if 0 < e.country_idx {
            abort(g);
            return;
        }
        set(&mut g.scenario_tag, "virus");
        if 40 < e.clock_sec {
            set(&mut g.scenario_tag, __content.string(12953));
        }
    }
    e.amount = e.market_cap * Ext::from_f64(5.0);
    e.cost_amt = to_quad(e.amount * sf(0.01f32));
    e.amount = cast::<Ext, _>(e.cost_amt) * Ext::from_f64(100.0);
    e.cost_amt = to_quad(e.amount);
    if e.amount < Ext::from_f64(500.0) {
        e.amount = Ext::from_f64(500.0);
    }
    if Ext::from_f64(5000.0) < e.amount {
        e.amount = Ext::from_f64(1000.0);
    }
    e.subst_tokens(g, ui, rng);
    if !(e.show_text == "Y") {
        e.amount = Ext::from_f64(0.0);
    }
    if !yes(g) {
        if e.show_text == "Y" {
            e.margin -= 7;
            set(&mut e.news_tail, __content.string(12954));
            e.notoriety = e.notoriety.wrapping_sub(2);
        }
        return;
    }
    e.amount = -Ext::from_f64(1.0) * cast::<Ext, _>(e.cost_amt);
    e.charge_expense(g);
    g.scenario_stage = 11;
    g.scenario_headline = s(__content.string(12955)) + &e.corp_symbol + __content.string(12956);
    if e.clock_sec % 2 == 1 {
        g.scenario_headline = s(__content.string(12957)) + &e.corp_symbol + __content.string(12958);
    }
    if e.show_text == "Y" {
        g.scenario_stage = 7;
        e.margin += 5;
        g.news_msg = PbStr::new();
        e.notoriety = e.notoriety.wrapping_add(2);
        set(&mut e.news_tail, __content.string(12959));
        if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
            set(&mut e.news_tail, __content.string(12960));
        }
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(12961);
    }
}

// case -4: bioweapons fine / cancer cure boycott (may start a pandemic scare)
fn c_4(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 4 {
        abort(g);
        return;
    }
    if e.show_text == "Y" {
        e.amount = e.market_cap * Ext::from_f64(9.0);
    } else {
        e.amount = cast::<Ext, _>(10 * e.clock_sec + 200);
    }
    e.charge_expense(g);
    e.subst_tokens(g, ui, rng);
    e.market_cap = e.market_cap * sf(0.85f32);
    e.margin -= 5;
    e.news_tail = s("fined ") + &e.amount_text + __content.string(12962);
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(12963));
    } else if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0)
        && g.pandemic_state == 0
        && g.oil_crisis_level == Ext::from_f32(0.0f32)
        && g.subprime_crisis_state <= 0
        && -2 < g.subprime_crisis_state
        && Ext::from_f64(1.0) < cast::<Ext, _>(g.gdp_growth)
    {
        g.pandemic_state = -4;
    }
}

// case 5: housing development on a toxic site
fn c5(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    set(&mut g.scenario_tag, __content.string(12964));
    if 30 < e.clock_sec {
        set(&mut g.scenario_tag, __content.string(12965));
    }
    e.event_year = 0;
    e.detail_text = uc(&g.scenario_tag);
    e.amount = ba(g) * sf(0.2f32);
    e.subst_tokens(g, ui, rng);
    if yes(g) {
        e.amount = sf(-0.4f32) * ba(g);
        e.charge_expense(g);
        g.scenario_stage = 8;
        e.margin += 3;
        e.news_tail = s(__content.string(12966)) + &g.scenario_tag + __content.string(12967);
        g.scenario_headline =
            s(__content.string(12968)) + &e.corp_symbol + __content.string(12969);
        if e.clock_sec % 2 == 1 {
            g.scenario_headline =
                s(__content.string(12970)) + &e.corp_symbol + __content.string(12971) + &g.scenario_tag;
        }
    } else {
        e.charge_extraordinary(g);
        e.news_tail = s(__content.string(12972)) + &g.scenario_tag + __content.string(12973);
        e.notoriety = e.notoriety.wrapping_sub(1);
    }
}

// case -5: Superfund cleanup
fn c_5(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 5 {
        abort(g);
        return;
    }
    e.event_year = 0;
    e.detail_text = uc(&g.scenario_tag);
    e.amount = e.market_cap * cast::<Ext, _>(e.clock_sec + 5);
    e.charge_expense(g);
    e.subst_tokens(g, ui, rng);
    e.market_cap = sf(0.43f32) * e.market_cap;
    e.margin -= 12;
    e.news_tail = s(__content.string(12923)) + &e.amount_text + __content.string(12974);
    set(&mut g.scenario_cmd, "SUPRFUND");
}

// case 6: clear-cutting a national forest
fn c6(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    set(&mut g.scenario_tag, "Oregon");
    if 20 < e.clock_sec {
        set(&mut g.scenario_tag, "Idaho");
    }
    if 40 < e.clock_sec {
        set(&mut g.scenario_tag, "Montana");
    }
    e.amount = e.market_cap * (Ext::from_f64(8.0) * rng.rnd() + Ext::from_f64(3.0));
    e.subst_tokens(g, ui, rng);
    if !yes(g) {
        return;
    }
    g.scenario_stage = 5;
    e.amount = -Ext::from_f64(1.0) * e.amount;
    e.charge_expense(g);
    set(&mut e.news_tail, __content.string(12975));
    g.scenario_headline =
        s(__content.string(12976)) + &e.corp_symbol;
    if e.clock_sec % 2 == 1 {
        g.scenario_headline = s(__content.string(12977)) + &e.corp_symbol;
    }
}

// case -6: fined for 'land rape'
fn c_6(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 6 {
        abort(g);
        return;
    }
    e.event_year = 0;
    e.detail_text = uc(&g.scenario_tag);
    e.amount = e.market_cap * Ext::from_f64(10.0);
    e.charge_expense(g);
    e.subst_tokens(g, ui, rng);
    e.market_cap = sf(0.72f32) * e.market_cap;
    e.news_tail = s("fined ") + &e.amount_text + __content.string(12978);
}

// case 7: reflagged tanker fleet
fn c7(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    e.amount = ba(g) * sf(0.2f32);
    e.subst_tokens(g, ui, rng);
    if yes(g) {
        e.margin += 3;
        g.scenario_stage = 6;
        set(&mut e.news_tail, __content.string(12979));
        g.scenario_headline =
            s(__content.string(12980)) + &e.corp_symbol + __content.string(12981);
    } else {
        e.charge_expense(g);
        set(&mut e.news_tail, __content.string(12982));
        e.notoriety = e.notoriety.wrapping_sub(1);
    }
}

// case -7: oil spill
fn c_7(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 7 {
        abort(g);
        return;
    }
    set(&mut g.scenario_tag, __content.string(12983));
    set(&mut e.misc_text, __content.string(12984));
    if 15 < e.clock_sec {
        set(&mut g.scenario_tag, __content.string(12985));
    }
    if 30 < e.clock_sec {
        set(&mut g.scenario_tag, __content.string(12986));
    }
    if 45 < e.clock_sec {
        set(&mut g.scenario_tag, __content.string(12987));
        e.misc_text = PbStr::new();
    }
    e.event_year = 0;
    e.detail_text = uc(&g.scenario_tag);
    e.amount = ba(g) * sf(0.3f32);
    e.charge_expense(g);
    e.subst_tokens(g, ui, rng);
    e.market_cap = sf(0.6f32) * e.market_cap;
    e.margin -= 7;
    g.news_msg = s(__content.string(12988)) + &e.corp_symbol + " " + &e.amount_text + "il.";
    if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
        set(&mut g.scenario_cmd, "SUPRFUND");
    }
}

// case 8: fracking pollution
fn c8(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    e.amount = ba(g) * sf(0.34f32);
    e.subst_tokens(g, ui, rng);
    if yes(g) {
        g.scenario_stage = 6;
        e.aux_text = e.corp_symbol.clone();
        e.cmp_arg = 16;
        if len(e.corp_name.as_bytes()) < e.cmp_arg {
            e.aux_text = e.corp_name.clone() + " (" + &e.corp_symbol + ")";
        }
        g.scenario_headline = e.aux_text.clone() + __content.string(12989);
        if e.clock_sec % 3 == 0 {
            repl(&mut g.scenario_headline, __content.string(12990), "denies");
        } else if e.clock_sec % 3 == 1 {
            g.scenario_headline =
                e.corp_symbol.clone() + __content.string(12991);
        }
    } else {
        e.charge_expense(g);
        e.margin -= 3;
        e.notoriety = e.notoriety.wrapping_sub(1);
        g.news_msg = e.corp_symbol.clone() + __content.string(12992);
    }
}

// case -8: liable for polluting a town's water
fn c_8(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 8 {
        abort(g);
        return;
    }
    e.amount = e.market_cap * Ext::from_f64(9.0);
    e.charge_expense(g);
    e.subst_tokens(g, ui, rng);
    e.market_cap = sf(0.8f32) * e.market_cap;
    g.news_msg = s(__content.string(12993)) + &e.corp_symbol + __content.string(12994);
    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
        set(&mut g.scenario_cmd, "SUPRFUND");
    }
}

// case 9: rainforest copper mine (or the BRE-X hoax)
fn c9(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let c = cast::<i32, _>(g.scenario_corp);
    if g.exchange_rate * Ext::from_f64(150.0) < g.market_cap[c] || 8 < e.margin {
        abort(g);
        return;
    }
    e.amount = g.exchange_rate * Ext::from_f64(1000.0);
    e.subst_tokens(g, ui, rng);
    if !yes(g) {
        e.amount = Ext::from_f64(0.0);
        e.notoriety = e.notoriety.wrapping_sub(1);
        set(&mut e.news_tail, __content.string(12995));
        return;
    }
    g.scenario_headline = e.corp_symbol.clone() + __content.string(12996);
    if 1 < e.clock_sec % 5 {
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(12997);
    }
    if e.clock_sec % 3 == 2 {
        set(&mut g.scenario_tag, "BRE-X");
        g.scenario_stage = 40;
        e.margin += 10;
        g.business_assets[c] = e.amount * Ext::from_f64(2.0) + g.business_assets[c];
        g.cash[c] = g.cash[c] - e.amount;
    } else {
        e.margin += 30;
        g.business_assets[c] = e.amount * Ext::from_f64(2.0) + g.business_assets[c];
        g.cash[c] = g.cash[c] - e.amount;
        g.scenario_stage = 11;
        e.notoriety = e.notoriety.wrapping_add(1);
    }
    e.news_tail = s(__content.string(12998)) + &e.amount_text;
}

// case -9: phony copper mine / mine closed under U.N. pressure
fn c_9(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 9 {
        abort(g);
        return;
    }
    let c = cast::<i32, _>(g.scenario_corp);
    if e.show_text == "Y" {
        e.amount = g.exchange_rate * Ext::from_f64(1000.0);
        if g.exchange_rate * Ext::from_f64(1500.0) < g.business_assets[c] {
            g.cash[c] = e.amount + g.cash[c];
        }
        g.business_assets[c] = g.business_assets[c] - g.exchange_rate * Ext::from_f64(2000.0);
        if g.business_assets[c] < Ext::from_f64(0.0) {
            g.business_assets[c] = Ext::from_f64(50.0);
        }
        e.margin -= 15;
        e.aux_text = trim(&format(g.exchange_rate * Ext::from_f64(1000.0), b"######")) + " " + &g.million_prefix_uc;
        e.news_tail = s("loses ") + &e.aux_text + __content.string(12999);
    } else {
        e.amount = g.exchange_rate * Ext::from_f64(2000.0);
        if g.exchange_rate * Ext::from_f64(1500.0) < g.business_assets[c] {
            g.cash[c] = e.amount * sf(0.5f32) + g.cash[c];
        }
        g.business_assets[c] = g.business_assets[c] - g.exchange_rate * Ext::from_f64(2000.0);
        if g.business_assets[c] < Ext::from_f64(0.0) {
            g.business_assets[c] = g.exchange_rate * Ext::from_f64(50.0);
        }
        e.margin -= 22;
        set(&mut e.news_tail, __content.string(13000));
        e.misc_text = g.currency_suffix.clone();
    }
    e.market_cap = sf(0.45f32) * e.market_cap;
    e.subst_tokens(g, ui, rng);
    e.charge_extraordinary(g);
}

// case 10: child sexbots / anti-missile system
fn c10(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if 0 < e.country_idx && !(e.show_text == "Y") {
        abort(g);
        return;
    }
    e.amount = e.market_cap * (Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(3.0));
    e.subst_tokens(g, ui, rng);
    if yes(g) {
        e.margin += 3;
        e.amount = -Ext::from_f64(4.0) * e.market_cap;
        e.charge_expense(g);
        if e.show_text == "Y" {
            set(&mut e.news_tail, __content.string(13001));
            g.scenario_stage = 4;
            if e.clock_sec % 2 == 1 {
                g.scenario_stage = 15;
            }
            g.scenario_headline =
                s(__content.string(13002)) + &e.corp_symbol + __content.string(13003);
        } else {
            g.news_msg = s(__content.string(13004)) + &e.corp_symbol + __content.string(13005);
            g.scenario_headline = e.corp_symbol.clone() + __content.string(13006);
            set(&mut e.aux_text, __content.string(13007));
            if e.clock_sec % 5 == 2 {
                set(&mut e.aux_text, __content.string(13008));
            }
            if e.clock_sec % 5 == 1 {
                set(&mut e.aux_text, __content.string(13009));
            }
            g.scenario_headline = g.scenario_headline.clone() + &e.aux_text;
            g.scenario_stage = 8;
        }
    } else {
        if e.show_text == "Y" {
            e.news_tail = s(__content.string(13010)) + &e.amount_text;
        } else {
            e.news_tail = s("takes ") + &e.amount_text + __content.string(13011);
        }
        e.charge_extraordinary(g);
        e.notoriety = e.notoriety.wrapping_sub(1);
    }
}

// case -10: sexbot scandal / useless defense system
fn c_10(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 10 {
        abort(g);
        return;
    }
    e.amount = e.market_cap * Ext::from_f64(8.0);
    if g.exchange_rate * Ext::from_f64(950.0) < e.amount {
        e.amount = g.exchange_rate * Ext::from_f64(900.0);
    }
    if e.show_text == "Y" {
        e.amount = sf(0.1f32) * e.amount;
    }
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    if e.show_text == "Y" {
        e.market_cap = sf(0.9f32) * e.market_cap;
        if e.clock_sec % 5 < 2 {
            e.margin -= 8;
            set(&mut e.news_tail, __content.string(13012));
        } else {
            e.margin += 1;
            set(&mut e.news_tail, __content.string(13013));
        }
    } else {
        e.margin -= 7;
        e.market_cap = sf(0.57f32) * e.market_cap;
        e.news_tail = s(__content.string(13014)) + &e.amount_text + __content.string(13015);
    }
}

// case 11: railroad cost cutting
fn c11(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    e.amount = ba(g) * sf(0.08f32);
    e.subst_tokens(g, ui, rng);
    if yes(g) {
        e.margin += 5;
        g.scenario_headline =
            s(__content.string(13016)) + &e.corp_symbol + __content.string(13017);
        if e.clock_sec % 2 == 1 {
            g.scenario_headline =
                e.corp_symbol.clone() + __content.string(13018);
        }
        g.scenario_stage = 6;
    } else {
        e.margin -= 4;
    }
}

// case -11: train crash
fn c_11(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 11 {
        abort(g);
        return;
    }
    e.amount = e.market_cap * Ext::from_f64(7.0);
    if Ext::from_f64(2850.0) < e.amount {
        e.amount = Ext::from_f64(2500.0);
    }
    set(&mut g.scenario_tag, __content.string(13019));
    if 0 < e.country_idx {
        set(&mut g.scenario_tag, __content.string(13020));
    }
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.margin -= 8;
    e.market_cap = sf(0.78f32) * e.market_cap;
    g.news_msg = s(__content.string(13021)) + &e.corp_symbol + " over " + &e.amount_text + ".";
}

// case 12: steel for the [runtime resource wording]
fn c12(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    set(&mut g.scenario_tag, __content.string(13022));
    if 15 < e.clock_sec {
        set(&mut g.scenario_tag, __content.string(13023));
    }
    if 30 < e.clock_sec {
        set(&mut g.scenario_tag, __content.string(13024));
    }
    if 45 < e.clock_sec {
        set(&mut g.scenario_tag, __content.string(13025));
    }
    e.amount = e.market_cap * Ext::from_f64(3.0);
    e.subst_tokens(g, ui, rng);
    set(&mut e.news_tail, __content.string(13026));
    if !yes(g) {
        return;
    }
    e.margin += 5;
    e.amount = -Ext::from_f64(1.0) * e.amount;
    e.charge_expense(g);
    g.scenario_stage = 5;
    set(&mut e.news_tail, __content.string(13027));
    g.scenario_headline = s(__content.string(13028)) + &e.corp_symbol + __content.string(13029);
    if e.clock_sec % 3 == 1 {
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13030);
    }
}

// case -12: bridge fiasco
fn c_12(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 12 {
        abort(g);
        return;
    }
    e.amount = e.market_cap * Ext::from_f64(9.0);
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.margin -= 14;
    e.market_cap = sf(0.72f32) * e.market_cap;
    e.news_tail = s(__content.string(13031)) + &g.scenario_tag + __content.string(13032);
}

// case 13: nanorobot tests on prisoners
fn c13(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    e.amount = Ext::from_f64(700.0);
    e.subst_tokens(g, ui, rng);
    e.amount = Ext::from_f64(200.0);
    e.news_tail = s(__content.string(13033))
        + &g.currency_symbol
        + "200 "
        + &g.million_prefix_uc
        + ".";
    e.charge_expense(g);
    if !yes(g) {
        return;
    }
    e.news_tail = PbStr::new();
    g.scenario_headline = e.corp_symbol.clone() + __content.string(13034);
    if e.clock_sec % 3 == 1 {
        g.scenario_headline =
            s(__content.string(13035)) + &e.corp_symbol + __content.string(13036);
    }
    if e.clock_sec % 3 == 2 {
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13037);
    }
    e.amount = -Ext::from_f64(900.0);
    e.margin += 9;
    e.charge_expense(g);
    g.scenario_stage = 10;
}

// case -13: execs executed in Iran
fn c_13(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 13 {
        abort(g);
        return;
    }
    e.subst_tokens(g, ui, rng);
    g.news_msg = s("Four ") + &e.corp_symbol + __content.string(13038);
    e.amount = e.market_cap * Ext::from_f64(19.0);
    if e.amount < Ext::from_f64(1300.0) {
        e.amount = Ext::from_f64(1300.0);
    }
    e.market_cap = sf(0.6f32) * e.market_cap;
    e.margin -= 12;
    e.charge_extraordinary(g);
}

// case 14: killer robots / [runtime resource wording]
fn c14(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if 0 < e.country_idx {
        abort(g);
        return;
    }
    e.amount = sf(0.1333f32) * ba(g);
    if e.show_text == "Y" {
        e.subst_tokens(g, ui, rng);
        set(&mut e.news_tail, __content.string(13039));
        if !yes(g) {
            return;
        }
        e.amount = -Ext::from_f64(1.0) * e.amount;
        e.news_tail = PbStr::new();
        g.news_msg = s(__content.string(4553)) + &e.corp_symbol + __content.string(13040);
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13041);
        if e.clock_sec % 2 == 1 {
            g.scenario_headline =
                s(__content.string(13042)) + &e.corp_symbol + __content.string(13043);
        }
    } else {
        set(&mut e.misc_text, "Swiss");
        if 10 < e.clock_sec {
            set(&mut e.misc_text, "Caymans");
        }
        if 20 < e.clock_sec {
            set(&mut e.misc_text, "Panama");
        }
        e.subst_tokens(g, ui, rng);
        if !yes(g) {
            return;
        }
        g.scenario_tag = e.amount_text.clone();
        e.amount = -Ext::from_f64(1.0) * e.amount;
        g.scenario_stage = 13;
        g.scenario_headline =
            s(__content.string(13044)) + &e.corp_symbol;
        if e.clock_sec % 2 == 0 {
            g.scenario_headline =
                s(__content.string(13045)) + &e.corp_symbol + __content.string(13046);
        }
    }
    e.margin += 6;
    e.charge_expense(g);
    g.scenario_stage = 1;
    if e.clock_sec % 2 == 1 {
        g.scenario_stage = 30;
    }
}

// case -14: killer robots run amok / caught selling secrets
fn c_14(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 14 {
        abort(g);
        return;
    }
    e.amount = sf(0.4f32) * ba(g);
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.margin -= 8;
    e.market_cap = sf(0.7f32) * e.market_cap;
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13047));
    } else {
        set(&mut e.news_tail, __content.string(13048));
    }
}

// case 15: cheap land from a Mafia waste firm
fn c15(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    e.amount = sf(0.1f32) * ba(g);
    if e.amount < Ext::from_f64(50.0) {
        e.amount = Ext::from_f64(50.0);
    }
    e.subst_tokens(g, ui, rng);
    if yes(g) {
        e.amount = -Ext::from_f64(1.0) * e.amount;
        e.charge_expense(g);
        e.margin += 4;
        g.scenario_stage = 5;
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13049);
        if e.clock_sec % 2 == 0 {
            g.scenario_headline =
                s(__content.string(13050)) + &e.corp_symbol + __content.string(13051);
        }
    } else {
        set(&mut e.news_tail, __content.string(13052));
        e.amount = Ext::from_f64(35.0);
        e.charge_expense(g);
        e.notoriety = e.notoriety.wrapping_sub(1);
    }
}

// case -15: nuclear waste dumping
fn c_15(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 15 {
        abort(g);
        return;
    }
    e.amount = sf(0.3f32) * ba(g);
    if e.amount < Ext::from_f64(350.0) {
        e.amount = Ext::from_f64(500.0);
    }
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.margin -= 12;
    e.market_cap = sf(0.66f32) * e.market_cap;
    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
        set(&mut g.scenario_cmd, "SUPRFUND");
    }
    set(&mut e.news_tail, __content.string(13053));
}

// case 16: airline maintenance cuts
fn c16(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    set(&mut e.misc_text, __content.string(13054));
    set(&mut e.detail_text, __content.string(13055));
    if 0 < e.country_idx {
        set(&mut e.misc_text, __content.string(13056));
        set(&mut e.detail_text, __content.string(13057));
    }
    e.subst_tokens(g, ui, rng);
    if yes(g) {
        e.margin += 5;
        g.scenario_stage = 6;
        set(&mut e.news_tail, __content.string(13058));
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13059);
        if e.clock_sec % 2 == 0 {
            g.scenario_headline =
                s(__content.string(13060)) + &e.corp_symbol + __content.string(13061);
        }
    } else {
        e.margin -= 2;
    }
}

// case -16: air crashes
fn c_16(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 16 {
        abort(g);
        return;
    }
    e.amount = sf(0.1f32) * ba(g);
    e.charge_expense(g);
    e.subst_tokens(g, ui, rng);
    e.margin -= 20;
    e.market_cap = sf(0.73f32) * e.market_cap;
    set(&mut e.news_tail, __content.string(13062));
}

// case 17: brewery health violations
fn c17(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    e.amount = sf(0.05f32) * ba(g);
    if g.exchange_rate * Ext::from_f64(3000.0) < e.amount {
        let k = cast::<Ext, _>(rng.rnd_range(21, 1));
        e.amount = (k * Ext::from_f64(1000.0) + Ext::from_f64(2500.0)) * g.exchange_rate;
    }
    e.subst_tokens(g, ui, rng);
    if !yes(g) {
        e.margin -= 2;
        e.charge_expense(g);
        return;
    }
    e.margin += 5;
    if e.day_num % 2 == 1 {
        e.amount = Ext::from_f64(5.0);
        e.charge_expense(g);
    }
    g.scenario_stage = 5;
    g.scenario_headline =
        e.corp_symbol.clone() + __content.string(13063);
    if e.clock_sec % 2 == 0 {
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13064);
    }
}

// case -17: brewery bribery fine
fn c_17(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if e.industry != 17 {
        abort(g);
        return;
    }
    set(&mut g.scenario_tag, __content.string(13065));
    e.event_year = 0;
    set(&mut e.detail_text, __content.string(13066));
    if e.day_num % 2 == 0 {
        set(&mut g.scenario_tag, __content.string(13067));
        e.detail_text = s(__content.string(13068))
            + &g.currency_symbol
            + "2 "
            + &g.million_prefix_uc
            + __content.string(13069);
    }
    e.amount = ba(g) * sf(0.15f32);
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    if g.exchange_rate * Ext::from_f64(9000.0) < e.amount {
        let k = cast::<Ext, _>(rng.rnd_range(60, 1));
        e.amount = (k * Ext::from_f64(1000.0) + Ext::from_f64(7500.0)) * g.exchange_rate;
    }
    e.margin -= 7;
    e.market_cap = sf(0.7f32) * e.market_cap;
    e.news_tail = s("fined ") + &e.amount_text + __content.string(13070);
}

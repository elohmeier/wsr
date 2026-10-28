//! Cases of RunScenarioEvent_689361 (0x689361): see [`super::scenario_event`].
//!
//! This file: the industry scenarios 18..35 (positive = the proposal memo, negative = the outcome;
//! machine code 0x68e875..0x6928e7) — tire strike (18), sweatshop textiles (19), Biocide X (20),
//! Dedzone / Zombex-II drugs (21), artificial heart / organ trade (22), drug-smuggling ships (23), Moon
//! Shuttle computer bug (24), Teamsters (25), air-traffic software / Sonderkommando game (26), author
//! royalties / nuke (27), contaminated food / sawdust (28), snuff flick (29), telecom bribes (30),
//! youth cigarettes (31), extortion plot / cyanide (32), paper-plant pollution (33), No-Count sweetener /
//! Zing! Cola (34), Mars gigachip (35) — and the industries without a story, 36..38 / -36..-38
//! ("No scenario for" message box; 0x6928e7..0x6929a4).
//!
//! All fractional pool constants of these cases are SINGLE (`sf`).

#![allow(unused_imports)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::scenario_event::Ev;

/// How a case left the SELECT other than through the common end: `Err(EXIT_ROUTINE)` = the routine
/// returns at once (`RunScenarioEvent_Exit`, no common end); `Err(n > 0)` = runtime error `n`, i.e. the
/// routine's ON ERROR handler (message box, then exit).
pub(super) type R = Result<(), i32>;

/// `Err` value of [`R`]: leave the routine without the common end.
pub(super) const EXIT_ROUTINE: i32 = -1;

/// Runs case `n` of the scenario SELECT if it is one of this file's: `Ok(false)` = not one of this
/// file's cases; `Err` = left the routine without the common end (see [`R`]).
pub(super) fn dispatch_result(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, n: i32) -> Result<bool, i32> {
    let __content = crate::content::current();
    match n {
        18 => case_18(e, g, ui, rng)?,
        -18 => case_m18(e, g, ui, rng)?,
        19 => case_19(e, g, ui, rng)?,
        -19 => case_m19(e, g, ui, rng)?,
        20 => case_20(e, g, ui, rng)?,
        -20 => case_m20(e, g, ui, rng)?,
        21 => case_21(e, g, ui, rng)?,
        -21 => case_m21(e, g, ui, rng)?,
        22 => case_22(e, g, ui, rng)?,
        -22 => case_m22(e, g, ui, rng)?,
        23 => case_23(e, g, ui, rng)?,
        -23 => case_m23(e, g, ui, rng)?,
        24 => case_24(e, g, ui, rng)?,
        -24 => case_m24(e, g, ui, rng)?,
        25 => case_25(e, g, ui, rng)?,
        -25 => case_m25(e, g, ui, rng)?,
        26 => case_26(e, g, ui, rng)?,
        -26 => case_m26(e, g, ui, rng)?,
        27 => case_27(e, g, ui, rng)?,
        -27 => case_m27(e, g, ui, rng)?,
        28 => case_28(e, g, ui, rng)?,
        -28 => case_m28(e, g, ui, rng)?,
        29 => case_29(e, g, ui, rng)?,
        -29 => case_m29(e, g, ui, rng)?,
        30 => case_30(e, g, ui, rng)?,
        -30 => case_m30(e, g, ui, rng)?,
        31 => case_31(e, g, ui, rng)?,
        -31 => case_m31(e, g, ui, rng)?,
        32 => case_32(e, g, ui, rng)?,
        -32 => case_m32(e, g, ui, rng)?,
        33 => case_33(e, g, ui, rng)?,
        -33 => case_m33(e, g, ui, rng)?,
        34 => case_34(e, g, ui, rng)?,
        -34 => case_m34(e, g, ui, rng)?,
        35 => case_35(e, g, ui, rng)?,
        -35 => case_m35(e, g, ui, rng)?,
        36..=38 | -38..=-36 => {
            // case 36..38 / -36..-38: no scenario for these industries
            let text = s(__content.string(13071)) + &str_s(cast::<Ext, _>(g.scenario_num));
            ui.msgbox(&text, 0, &PbStr::new());
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn is_y(g: &Globals) -> bool {
    g.scenario_cmd == "Y"
}

fn abort(g: &mut Globals) {
    set(&mut g.scenario_cmd, "ABORT");
}

/// `BusinessAssets(c)`: `DIM (131 TO 1600)`, error 9 for banks and insurers (11..130).
fn ba(g: &Globals, c: i32) -> Result<Ext, i32> {
    if !(131..=1600).contains(&c) {
        return Err(9);
    }
    Ok(g.business_assets[c])
}

fn corp(g: &Globals) -> i32 {
    cast::<i32, _>(g.scenario_corp)
}

/// `RTRIM$(LTRIM$(CurrencySymbol))`.
fn cur_sym(g: &Globals) -> PbStr {
    trim(g.currency_symbol.as_bytes())
}

// case 18: rubber workers' strike at a tire maker.
fn case_18(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    e.amount = e.market_cap * Ext::from_f64(4.0);
    set(&mut e.news_tail, __content.string(13072));
    e.margin += 3;
    e.subst_tokens(g, ui, rng);
    if is_y(g) {
        if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
            set(&mut g.scenario_tag, __content.string(12905));
        } else {
            set(&mut g.scenario_tag, "scabs");
        }
        e.news_tail = s(__content.string(13073)) + &g.scenario_tag + __content.string(13074);
        if Ext::from_f64(1.0) <= rng.rnd() * Ext::from_f64(3.0) {
            e.margin += 8;
            g.scenario_stage = 11;
        } else {
            set(&mut g.scenario_cmd, "STRIKE");
        }
        g.scenario_headline = s(__content.string(13075)) + &e.corp_symbol + __content.string(13076);
        if e.clock_sec % 2 == 1 {
            g.scenario_headline = s(__content.string(13077)) + &e.corp_symbol + " CEO";
        }
    } else {
        e.charge_expense(g);
    }
    Ok(())
}

// case -18: [runtime resource wording]  made by strike-breakers; ends a strike at this company.
fn case_m18(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 18 {
        abort(g);
        return Ok(());
    }
    if g.scenario_tag == "scabs" {
        set(&mut e.misc_text, __content.string(13078));
    } else {
        set(&mut e.misc_text, __content.string(13079));
    }
    e.amount = e.market_cap * Ext::from_f64(8.0);
    e.charge_extraordinary(g);
    e.subst_tokens(g, ui, rng);
    e.margin -= 13;
    e.news_tail = s(__content.string(13080)) + &e.amount_text + __content.string(13081);
    if 10000 < g.strike_corp_code && corp(g) == cast::<i32, _>(g.strike_corp_code) % 10000 {
        g.strike_corp_code = 0;
    }
    Ok(())
}

// case 19: foreign sweatshops for textiles.
fn case_19(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    e.subst_tokens(g, ui, rng);
    e.margin -= 4;
    if !is_y(g) {
        return Ok(());
    }
    let c = corp(g);
    e.amount = ba(g, c)? * -sf(0.1f32);
    e.charge_expense(g);
    e.margin += 10;
    g.scenario_stage = 5;
    g.scenario_headline = e.corp_symbol.clone() + __content.string(13082);
    if 0 < e.clock_sec % 3 {
        set(&mut e.aux_text, __content.string(13083));
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
            set(&mut e.aux_text, "Amazon");
        }
        g.scenario_headline = e.aux_text.clone() + __content.string(13084) + &e.corp_symbol + __content.string(13085);
        if e.show_text == "Y" {
            g.scenario_headline = g.scenario_headline.clone() + __content.string(13086);
            if 2 < g.cur_quarter {
                g.scenario_headline =
                    e.corp_symbol.clone() + __content.string(13087);
            }
        } else {
            g.scenario_headline = g.scenario_headline.clone() + __content.string(13088);
            if g.cur_quarter < 3 {
                g.scenario_headline =
                    e.corp_symbol.clone() + __content.string(13089);
            }
        }
    }
    Ok(())
}

// case -19: textiles confiscated / Saipan sweatshops closed.
fn case_m19(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 19 {
        abort(g);
        return Ok(());
    }
    e.amount = e.market_cap * Ext::from_f64(7.0);
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.margin -= 14;
    e.market_cap = sf(0.65f32) * e.market_cap;
    set(&mut e.news_tail, __content.string(13090));
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13091));
    }
    Ok(())
}

// case 20: release the pesticide 'Biocide X' despite objections.
fn case_20(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    let c = corp(g);
    e.amount = ba(g, c)? * sf(0.1f32);
    e.subst_tokens(g, ui, rng);
    e.news_tail = s(__content.string(13092)) + &e.amount_text + __content.string(13093);
    if is_y(g) {
        e.amount = -Ext::from_f64(3.0) * e.amount;
        e.charge_expense(g);
        e.margin += 7;
        g.scenario_stage = 5;
        let c = corp(g);
        g.cash[c] = g.cash[c] - Ext::from_f64(3.0);
        set(&mut e.news_tail, __content.string(13094));
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13095);
        if e.clock_sec % 2 == 1 {
            g.scenario_headline =
                s(__content.string(13096)) + &e.corp_symbol + __content.string(13097);
        }
    } else {
        e.charge_extraordinary(g);
        e.notoriety = e.notoriety.wrapping_sub(1);
    }
    Ok(())
}

// case -20: Biocide X poisoning lawsuits.
fn case_m20(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 20 {
        abort(g);
        return Ok(());
    }
    e.amount = e.market_cap * Ext::from_f64(8.0);
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.margin -= 15;
    e.market_cap = sf(0.6f32) * e.market_cap;
    set(&mut e.news_tail, __content.string(13098));
    Ok(())
}

// case 21: the 'Dedzone' drug (FDA bribe) or the Zombex-II drug.
fn case_21(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    let c = corp(g);
    e.amount = ba(g, c)? * sf(0.1f32);
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13099));
    } else {
        set(&mut g.scenario_tag, __content.string(13100));
        if 12 < e.clock_sec {
            set(&mut g.scenario_tag, "heart");
        }
        if 24 < e.clock_sec {
            set(&mut g.scenario_tag, "ulcer");
        }
        if 36 < e.clock_sec {
            set(&mut g.scenario_tag, "cancer");
        }
        if 48 < e.clock_sec {
            set(&mut g.scenario_tag, __content.string(13101));
        }
        e.event_year = 0;
        e.detail_text = uc(&g.scenario_tag);
        e.news_tail = s(__content.string(13102)) + &g.scenario_tag + __content.string(13103);
    }
    e.subst_tokens(g, ui, rng);
    if is_y(g) {
        e.amount = -Ext::from_f64(3.0) * e.amount;
        if e.amount.abs() < Ext::from_f64(300.0) {
            e.amount = e.amount - Ext::from_f64(200.0);
        }
        e.charge_expense(g);
        e.margin += 12;
        g.scenario_stage = 7;
        set(&mut e.news_tail, __content.string(13104));
        g.scenario_headline = s(__content.string(13105)) + &e.corp_symbol + __content.string(13106);
        if e.clock_sec % 2 == 1 {
            g.scenario_headline =
                s(__content.string(13107)) + &e.corp_symbol + __content.string(13108);
        }
        if e.show_text == "Y" {
            set(&mut e.news_tail, __content.string(13109));
            g.scenario_headline = e.corp_symbol.clone()
                + __content.string(13110);
            if e.clock_sec % 2 == 0 {
                g.scenario_headline =
                    s(__content.string(13111)) + &e.corp_symbol + __content.string(13112);
            }
        }
    } else {
        e.charge_extraordinary(g);
    }
    Ok(())
}

// case -21: Zombex-II banned / [runtime resource wording] the new drug.
fn case_m21(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 21 {
        abort(g);
        return Ok(());
    }
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13113));
        e.prefix_the();
    } else {
        e.event_year = 0;
        e.detail_text = uc(&g.scenario_tag);
        e.news_tail = s(__content.string(13114)) + &g.scenario_tag + " drug";
    }
    e.margin -= 15;
    e.amount = e.market_cap * Ext::from_f64(8.0);
    if g.exchange_rate * Ext::from_f64(15000.0) < e.amount {
        e.amount = sf(0.23f32) * e.amount;
    }
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.market_cap = sf(0.7f32) * e.market_cap;
    Ok(())
}

// case 22: the ' [runtime resource wording] , or organ trading in Puerto Rico ("Australia").
fn case_22(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.show_text == "Y" {
        set(&mut g.scenario_tag, __content.string(4564));
    } else {
        e.amount = Ext::from_f64(20.0);
    }
    e.subst_tokens(g, ui, rng);
    e.amount = Ext::from_f64(0.0);
    if !(e.show_text == "Y") {
        g.news_msg = e.corp_symbol.clone() + __content.string(13115);
        e.amount = e.market_cap * Ext::from_f64(2.0);
    }
    if is_y(g) {
        e.amount = -Ext::from_f64(11.0) * e.market_cap;
        if -Ext::from_f64(200.0) < e.amount {
            e.amount = -Ext::from_f64(300.0);
        }
        e.charge_expense(g);
        e.margin += 3;
        g.scenario_stage = 6;
        if e.show_text == "Y" {
            set(&mut e.news_tail, __content.string(13116));
            g.scenario_headline =
                s(__content.string(13117)) + &e.corp_symbol + __content.string(13118);
            if e.clock_sec % 2 == 1 {
                g.scenario_headline =
                    s(__content.string(13119)) + &e.corp_symbol + __content.string(13120);
            }
        } else {
            g.news_msg = s(__content.string(13121)) + &e.corp_symbol + __content.string(13122);
            g.scenario_headline =
                s(__content.string(13123)) + &e.corp_symbol + __content.string(13122);
            if 2 < e.clock_sec % 5 {
                g.scenario_headline = s(__content.string(13124))
                    + &e.corp_symbol
                    + __content.string(13125);
            }
        }
    } else {
        e.charge_extraordinary(g);
    }
    Ok(())
}

// case -22: organ trafficking sanctions / artificial-heart lawsuits.
fn case_m22(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 22 {
        abort(g);
        return Ok(());
    }
    if e.show_text == "Y" {
        if 20 < e.clock_sec {
            set(&mut g.scenario_tag, __content.string(5833));
        }
        if 40 < e.clock_sec {
            set(&mut g.scenario_tag, __content.string(5831));
        }
        set(&mut e.news_tail, __content.string(13126));
    } else {
        e.margin -= 15;
        set(&mut e.news_tail, __content.string(13127));
    }
    e.amount = e.market_cap * Ext::from_f64(8.0);
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.market_cap = sf(0.53f32) * e.market_cap;
    Ok(())
}

// case 23: a shipping company asked to carry drugs.
fn case_23(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    let c = corp(g);
    e.amount = -sf(0.2f32) * ba(g, c)?;
    e.subst_tokens(g, ui, rng);
    g.news_msg = e.corp_symbol.clone() + __content.string(13128);
    e.margin += 3;
    if is_y(g) {
        g.news_msg = PbStr::new();
        e.charge_expense(g);
        e.margin += 7;
        g.scenario_stage = 10;
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13129);
    } else {
        set(&mut g.scenario_tag, __content.string(13130));
        e.notoriety = e.notoriety.wrapping_sub(2);
        g.scenario_stage = 5;
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13131);
    }
    // the story runs either way
    e.event_state = 0;
    set(&mut g.scenario_cmd, "Y");
    Ok(())
}

// case -23: ships confiscated by the DEA.
fn case_m23(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 23 {
        abort(g);
        return Ok(());
    }
    let c = corp(g);
    if e.show_text == "Y" {
        e.margin -= 7;
        e.amount = ba(g, c)? * sf(0.18f32);
        if Ext::from_f64(600.0) < e.amount {
            e.amount = Ext::from_f64(500.0);
        }
        set(&mut e.aux_text, __content.string(13132));
    } else {
        e.amount = ba(g, c)? * sf(0.35f32);
        e.margin -= 15;
        set(&mut e.aux_text, __content.string(13133));
    }
    e.news_tail = s(__content.string(13134)) + &e.aux_text;
    e.subst_tokens(g, ui, rng);
    let c = corp(g);
    g.extraordinary_pl[c] = to_quad(cast::<Ext, _>(g.extraordinary_pl[c]) - e.amount);
    g.business_assets[c] = ba(g, c)? - e.amount;
    e.market_cap = sf(0.7f32) * e.market_cap;
    Ok(())
}

// case 24: a bug in the Moon Shuttle / [runtime resource wording] .
fn case_24(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    e.amount = e.market_cap * Ext::from_f64(2.0);
    set(&mut e.news_tail, __content.string(13135));
    e.subst_tokens(g, ui, rng);
    if is_y(g) {
        e.amount = -Ext::from_f64(5.0) * e.market_cap;
        e.charge_expense(g);
        e.margin += 4;
        e.news_tail = PbStr::new();
        g.scenario_headline =
            s(__content.string(13136)) + &e.corp_symbol + __content.string(13137);
        if e.clock_sec % 2 == 1 {
            g.scenario_headline =
                e.corp_symbol.clone() + __content.string(13138);
        }
        g.scenario_stage = 5;
    } else {
        e.charge_extraordinary(g);
        e.notoriety = e.notoriety.wrapping_sub(1);
    }
    Ok(())
}

// case -24: the computer bug cover-up.
fn case_m24(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 24 {
        abort(g);
        return Ok(());
    }
    e.amount = e.market_cap * Ext::from_f64(8.0);
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.margin -= 15;
    e.market_cap = sf(0.6f32) * e.market_cap;
    set(&mut e.news_tail, __content.string(13139));
    Ok(())
}

// case 25: speed for truck drivers, or paying off the Teamsters.
fn case_25(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    e.month_text = PbStr::new();
    if e.show_text == "Y" {
        set(&mut g.scenario_tag, "TEAMSTERS");
        e.amount = e.market_cap * Ext::from_f64(2.0);
        set(&mut e.news_tail, __content.string(13140));
        set(&mut e.month_text, __content.string(13141));
    } else {
        set(&mut e.news_tail, __content.string(13142));
    }
    e.margin -= 3;
    e.subst_tokens(g, ui, rng);
    if is_y(g) {
        e.amount = -Ext::from_f64(4.0) * e.market_cap;
        e.charge_expense(g);
        e.margin += 7;
        g.scenario_stage = 5;
        if e.show_text == "Y" {
            g.scenario_headline = s(__content.string(13143))
                + &e.corp_symbol
                + __content.string(13144);
        } else {
            g.scenario_headline = e.corp_symbol.clone() + __content.string(13145);
            let injured = cast::<Ext, _>(rng.rnd_range(3, 2));
            e.aux_text = str_s(injured + Ext::from_f64(1.0));
            if e.clock_sec % 3 < 2 {
                g.scenario_headline = e.corp_symbol.clone()
                    + __content.string(13146)
                    + &e.aux_text
                    + __content.string(13147);
            }
        }
    } else {
        e.news_tail = e.month_text.clone();
        e.charge_expense(g);
    }
    Ok(())
}

// case -25: school-bus crash / union bribery case.
fn case_m25(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 25 {
        abort(g);
        return Ok(());
    }
    e.loss_amt = to_quad(e.market_cap * Ext::from_f64(6.0));
    e.amount = cast::<Ext, _>(e.loss_amt);
    if g.scenario_tag == "TEAMSTERS" {
        e.amount = cast::<Ext, _>(e.loss_amt + 100);
        e.misc_text = cur_sym(g) + "100 " + &g.million_prefix_lc;
    }
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    set(&mut e.news_tail, __content.string(13148));
    if g.scenario_tag == "TEAMSTERS" {
        e.news_tail = s(__content.string(12923)) + &e.amount_text + __content.string(13149);
        set(&mut g.scenario_cmd, "STRIKE");
    }
    e.market_cap = sf(0.75f32) * e.market_cap;
    e.margin -= 15;
    Ok(())
}

// case 26: flawed air-traffic software, or the 'Sonderkommando' computer game.
fn case_26(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    e.amount = e.market_cap * Ext::from_f64(3.0);
    set(&mut e.news_tail, __content.string(13150));
    e.misc_text = cur_sym(g) + "300 " + &g.million_prefix_uc;
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13151));
    }
    e.subst_tokens(g, ui, rng);
    if is_y(g) {
        e.amount = -(e.amount + Ext::from_f64(300.0));
        e.charge_expense(g);
        g.scenario_stage = 5;
        e.margin += 4;
        if e.show_text == "Y" {
            set(&mut e.aux_text, __content.string(13152));
            g.scenario_headline =
                s(__content.string(13153)) + &e.corp_symbol;
            if 2 < g.rand_seed % 5 {
                g.scenario_headline = s(__content.string(13154))
                    + &e.corp_symbol
                    + __content.string(13155);
            }
        } else {
            set(&mut e.aux_text, __content.string(13156));
            g.scenario_headline =
                s(__content.string(13157)) + &e.corp_symbol + __content.string(13158);
        }
        e.news_tail = s(__content.string(13159)) + &e.aux_text + ".";
    } else {
        e.amount = Ext::from_f64(300.0);
        e.charge_extraordinary(g);
    }
    Ok(())
}

// case -26: air-traffic software disaster / [runtime resource wording]
fn case_m26(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 26 {
        abort(g);
        return Ok(());
    }
    e.amount = e.market_cap * Ext::from_f64(9.0);
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.market_cap = sf(0.7f32) * e.market_cap;
    e.margin -= 8;
    set(&mut e.news_tail, __content.string(13160));
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13161));
    }
    Ok(())
}

// case 27: cheat [runtime resource wording] , or a nuclear device in a publisher's hands.
fn case_27(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    e.subst_tokens(g, ui, rng);
    if !is_y(g) {
        return Ok(());
    }
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13162));
        e.margin += 3;
        g.scenario_stage = 12;
        set(&mut g.scenario_headline, __content.string(13163));
        if e.clock_sec % 2 == 1 {
            set(&mut g.scenario_headline, __content.string(13164));
        }
    } else {
        e.margin += 5;
        g.scenario_stage = 3;
        set(&mut e.news_tail, __content.string(13165));
        g.scenario_headline = s(__content.string(13166)) + &e.corp_symbol;
    }
    Ok(())
}

// case -27: assets seized by the government / royalty fraud verdict.
fn case_m27(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 27 {
        abort(g);
        return Ok(());
    }
    let c = corp(g);
    if e.show_text == "Y" {
        if e.country_idx < 1 {
            set(&mut e.nation_adj, "Federal");
        } else {
            e.nationality();
        }
        e.amount = g.cash[c] * sf(0.8f32) + ba(g, c)? * sf(0.7f32) + g.t_bills[c];
        g.business_assets[c] = ba(g, c)? * sf(0.3f32);
        g.cash[c] = g.cash[c] * sf(0.2f32);
        g.t_bills[c] = Ext::from_f64(0.0);
        e.market_cap = sf(0.13f32) * e.market_cap;
        e.news_tail = s(__content.string(13167)) + &e.nation_adj + __content.string(13168);
    } else {
        e.amount = sf(0.18f32) * ba(g, c)?;
        e.market_cap = sf(0.8f32) * e.market_cap;
        set(&mut e.news_tail, __content.string(13169));
    }
    e.margin -= 11;
    e.subst_tokens(g, ui, rng);
    e.charge_extraordinary(g);
    Ok(())
}

// case 28: [runtime resource wording]  ("road kill"), or sawdust in desserts.
fn case_28(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.show_text == "Y" {
        set(&mut g.scenario_tag, "SAWDUST");
    } else {
        e.amount = Ext::from_f64(200.0);
        e.margin += 3;
        e.news_tail = s(__content.string(13170)) + &g.currency_symbol + "200 " + &g.million_prefix_uc
            + __content.string(13171);
    }
    e.subst_tokens(g, ui, rng);
    if is_y(g) {
        if e.show_text == "Y" {
            e.margin += 3;
            set(&mut e.news_tail, __content.string(13172));
            g.scenario_headline =
                e.corp_symbol.clone() + __content.string(13173);
        } else {
            g.scenario_headline = e.corp_symbol.clone() + __content.string(13174);
            if 1 < e.clock_sec % 5 {
                set(&mut g.scenario_headline, __content.string(13175));
            }
            e.news_tail = PbStr::new();
        }
        g.scenario_stage = 5;
    } else {
        e.charge_extraordinary(g);
    }
    Ok(())
}

// case -28: [runtime resource wording] -up / sawdust exposed.
fn case_m28(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 28 {
        abort(g);
        return Ok(());
    }
    if !(e.show_text == "Y") {
        e.amount = rng.rnd() * Ext::from_f64(150.0) + Ext::from_f64(1000.0);
    }
    e.margin -= 10;
    set(&mut e.misc_text, __content.string(13176));
    if 0 < e.country_idx {
        e.misc_text = e.nation_name.clone() + __content.string(13177);
    }
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.market_cap = sf(0.63f32) * e.market_cap;
    e.news_tail = s(__content.string(13178)) + &g.currency_symbol + "1000 " + &g.million_prefix_lc
        + __content.string(13179);
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13180));
    }
    Ok(())
}

// case 29: a [runtime resource wording] .
fn case_29(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if 0 < instr(1, e.corp_name.as_bytes(), b"FOX") {
        e.aux_text = PbStr::new(); // overwritten below: dead code (bug #114)
    }
    set(&mut e.misc_text, "Netflix");
    if 0 < instr(1, e.corp_name.as_bytes(), b"NETFLIX") {
        set(&mut e.misc_text, "Hulu");
    }
    set(&mut e.aux_text, __content.string(13181));
    e.news_tail = s(__content.string(13182)) + &e.aux_text;
    e.subst_tokens(g, ui, rng);
    if !is_y(g) {
        return Ok(());
    }
    let c = corp(g);
    g.cash[c] = g.cash[c] - Ext::from_f64(65.0);
    e.news_tail = PbStr::new();
    g.scenario_headline = e.corp_symbol.clone() + __content.string(13183);
    if e.clock_sec % 3 == 1 {
        g.scenario_headline =
            s(__content.string(13184)) + &e.corp_symbol + __content.string(13185);
    }
    if 35 < e.clock_sec {
        set(&mut g.scenario_tag, "Disney");
    }
    g.scenario_stage = 15;
    e.notoriety = e.notoriety.wrapping_add(2);
    Ok(())
}

// case -29: the snuff film is a hit ("Disney") / public boycott.
fn case_m29(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 29 {
        abort(g);
        return Ok(());
    }
    if e.show_text == "Y" {
        if 0 < instr(1, e.corp_name.as_bytes(), b"DISNEY") {
            set(&mut g.scenario_tag, "Fox");
        }
        e.amount = -Ext::from_f64(600.0);
        e.margin += 5;
        e.charge_expense(g);
        set(&mut e.news_tail, __content.string(13186));
        e.notoriety = e.notoriety.wrapping_add(1);
    } else {
        e.margin += 3;
        let c = corp(g);
        g.accrued_pl[c] = g.accrued_pl[c] - Ext::from_f64(65.0);
        e.margin -= 10;
        g.news_msg = s(__content.string(13187)) + &e.corp_symbol + __content.string(13188);
        e.market_cap = sf(0.8f32) * e.market_cap;
    }
    e.subst_tokens(g, ui, rng);
    Ok(())
}

// case 30: bribes for a South American telecom contract.
fn case_30(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    set(&mut g.scenario_tag, "Guyana");
    if 20 < e.clock_sec {
        set(&mut g.scenario_tag, __content.string(13189));
    }
    if 40 < e.clock_sec {
        set(&mut g.scenario_tag, "Ecuador");
    }
    e.loss_text = uc(&g.scenario_tag);
    e.amount = e.market_cap * (rng.rnd() * Ext::from_f64(2.0) + Ext::from_f64(2.0));
    if g.exchange_rate * Ext::from_f64(2500.0) < e.amount {
        let k = cast::<Ext, _>(rng.rnd_range(15, 1));
        e.amount = Ext::from_f64(100.0) * k * g.exchange_rate + Ext::from_f64(2000.0);
    }
    e.subst_tokens(g, ui, rng);
    e.news_tail = s(__content.string(13190)) + &g.scenario_tag + ".";
    if !is_y(g) {
        return Ok(());
    }
    e.amount = -(e.amount - Ext::from_f64(15.0));
    e.charge_expense(g);
    e.margin += 3;
    g.scenario_stage = 5;
    e.news_tail = s(__content.string(13191)) + &g.scenario_tag + __content.string(13192);
    g.scenario_headline = s(__content.string(13193))
        + &e.corp_symbol
        + __content.string(13194)
        + &g.scenario_tag
        + __content.string(13195);
    if e.clock_sec % 2 == 1 {
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13196) + &g.scenario_tag;
    }
    Ok(())
}

// case -30: losses (and executives) in the telecom venture.
fn case_m30(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 30 {
        abort(g);
        return Ok(());
    }
    e.loss_text = uc(&g.scenario_tag);
    e.amount = e.market_cap * (rng.rnd() * Ext::from_f64(6.0) + Ext::from_f64(4.0));
    if g.exchange_rate * Ext::from_f64(5000.0) < e.amount {
        let k = cast::<Ext, _>(rng.rnd_range(25, 1));
        e.amount = Ext::from_f64(100.0) * k * g.exchange_rate + Ext::from_f64(4000.0);
    }
    e.nation_name = mc(&e.nation_name);
    e.misc_text = e.nation_name.clone();
    e.prefix_the();
    e.charge_expense(g);
    e.subst_tokens(g, ui, rng);
    e.margin -= 7;
    e.market_cap = sf(0.6f32) * e.market_cap;
    e.news_tail = s(__content.string(13197)) + &g.scenario_tag + __content.string(13198) + &e.amount_text + __content.string(13199);
    Ok(())
}

// case 31: [runtime resource wording] children, or drug-laced cigarettes.
fn case_31(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    e.margin -= 4;
    set(&mut e.news_tail, __content.string(13200));
    e.subst_tokens(g, ui, rng);
    if !is_y(g) {
        return Ok(());
    }
    e.margin += 7;
    let c = corp(g);
    e.amount = -sf(0.05f32) * ba(g, c)?;
    e.charge_expense(g);
    g.scenario_stage = 5;
    set(&mut e.news_tail, __content.string(13201));
    g.scenario_headline = s(__content.string(13202)) + &e.corp_symbol + __content.string(13203);
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13204));
        g.scenario_headline = s(__content.string(13205)) + &e.corp_symbol + __content.string(13206);
        if 2 < g.cur_quarter {
            g.scenario_headline =
                s(__content.string(13207)) + &e.corp_symbol + __content.string(13208);
        }
    }
    Ok(())
}

// case -31: first lost cancer lawsuit / drug-laced cigarettes halted.
fn case_m31(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 31 {
        abort(g);
        return Ok(());
    }
    set(&mut g.scenario_tag, "Ohio");
    if 30 < e.clock_sec {
        set(&mut g.scenario_tag, "Iowa");
    }
    let c = corp(g);
    let assets = ba(g, c)?;
    e.amount = (sf(0.1f32) * rng.rnd() + sf(0.12f32)) * assets;
    e.charge_expense(g);
    e.subst_tokens(g, ui, rng);
    e.margin -= 15;
    e.market_cap = sf(0.7f32) * e.market_cap;
    set(&mut e.news_tail, __content.string(13209));
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13210));
    }
    Ok(())
}

// case 32: extortion threat to poison a product.
fn case_32(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    set(&mut g.scenario_tag, __content.string(13211));
    if e.day_num < 8 || 23 < e.day_num {
        set(&mut g.scenario_tag, __content.string(13212));
    }
    e.amount = e.market_cap;
    e.subst_tokens(g, ui, rng);
    e.news_tail = s("pays ") + &e.amount_text + __content.string(13213);
    if !is_y(g) {
        e.charge_expense(g);
        return Ok(());
    }
    e.news_tail = s(__content.string(13214)) + &g.scenario_tag + __content.string(13215);
    if 30 < e.clock_sec {
        set(&mut g.scenario_tag, "CYANIDE");
        g.event_status[32] = 1;
        g.scenario_num = 32;
        g.scenario_stage = 85;
        // the original leaves RunScenarioEvent_689361 here (jmp 0x69af5e), skipping the common end
        // (NewsMsg, event status, notoriety, TmpExt1, MarketCap, price tick)
        return Err(EXIT_ROUTINE);
    }
    e.notoriety = e.notoriety.wrapping_add(1);
    if e.clock_sec % 5 == 4 {
        e.event_state = 2;
    } else {
        g.scenario_stage = 35;
    }
    Ok(())
}

// case -32: the cyanide poisoning (or the hoax that enhanced the company's reputation).
fn case_m32(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 32 {
        abort(g);
        return Ok(());
    }
    if g.scenario_tag == "CYANIDE" {
        set(&mut e.show_text, "Y");
    }
    set(&mut g.scenario_tag, __content.string(13211));
    if e.day_num < 8 || 23 < e.day_num {
        set(&mut g.scenario_tag, __content.string(13212));
    }
    if e.show_text == "Y" {
        e.event_state = 0;
        e.amount = sf(0.5f32) * e.market_cap;
        e.subst_tokens(g, ui, rng);
        e.news_tail = s("does ") + &e.amount_text + __content.string(13216);
        if is_y(g) {
            g.scenario_stage = 7;
            set(&mut e.news_tail, __content.string(13217));
            if 30 < e.clock_sec {
                set(&mut e.news_tail, __content.string(13218));
            }
        } else {
            e.amount = sf(0.5f32) * e.market_cap;
            e.charge_expense(g);
            e.notoriety = e.notoriety.wrapping_sub(1);
            e.margin += 5;
        }
    } else {
        e.amount = e.market_cap * Ext::from_f64(4.0);
        e.margin -= 12;
        if e.country_idx == 0 || e.country_idx == 30 || e.country_idx == 35 || e.country_idx == 50 {
            set(&mut e.misc_text, " the");
        }
        e.subst_tokens(g, ui, rng);
        e.charge_expense(g);
        e.market_cap = sf(0.7f32) * e.market_cap;
        e.news_tail = s(__content.string(13219)) + &e.amount_text + "illion";
    }
    Ok(())
}

// case 33: dumping paper-plant waste into a river.
fn case_33(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    let c = corp(g);
    e.amount = ba(g, c)? * sf(0.02f32);
    if e.amount < Ext::from_f64(50.0) {
        e.amount = Ext::from_f64(70.0);
    }
    if e.country_idx == 0 {
        set(&mut e.misc_text, __content.string(13220));
    }
    e.subst_tokens(g, ui, rng);
    e.news_tail = s("pays ") + &e.amount_text + __content.string(13221);
    if is_y(g) {
        e.amount = Ext::from_f64(5.0);
        e.charge_expense(g);
        e.margin += 5;
        g.scenario_stage = 5;
        e.news_tail = PbStr::new();
        g.scenario_headline = e.corp_symbol.clone() + __content.string(13222);
        if e.clock_sec % 2 == 1 {
            g.scenario_headline =
                s(__content.string(13223)) + &e.corp_symbol + __content.string(13224);
        }
    } else {
        e.margin -= 5;
        e.charge_extraordinary(g);
    }
    Ok(())
}

// case -33: the water-pollution case is settled (bribery rumor or hidden pipeline).
fn case_m33(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 33 {
        abort(g);
        return Ok(());
    }
    e.event_year = 0;
    let c = corp(g);
    if e.clock_sec < 21 {
        e.amount = ba(g, c)? * sf(0.04f32);
        set(&mut e.loss_text, "un");
        set(&mut e.detail_text, __content.string(13225));
        set(&mut e.aux_text, __content.string(13226));
        if 0 < e.country_idx {
            set(&mut e.aux_text, __content.string(13227));
        }
        g.scenario_tag = s(__content.string(13228)) + &e.aux_text;
        e.margin -= 3;
    } else {
        e.amount = ba(g, c)? * sf(0.12f32);
        set(&mut e.detail_text, __content.string(13229));
        set(&mut g.scenario_tag, __content.string(13230));
        e.margin -= 8;
    }
    e.market_cap = sf(0.75f32) * e.market_cap;
    e.charge_expense(g);
    e.subst_tokens(g, ui, rng);
    set(&mut e.aux_text, "EPA ");
    if 0 < e.country_idx {
        e.aux_text = PbStr::new();
    }
    e.news_tail = s(__content.string(13231)) + &e.aux_text + __content.string(13232) + &e.amount_text + ".";
    if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
        set(&mut g.scenario_cmd, "SUPRFUND");
    }
    Ok(())
}

// case 34: the 'No-Count' sweetener, or Zing! Cola in schools.
fn case_34(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    let c = corp(g);
    if e.show_text == "Y" {
        e.amount = ba(g, c)? * sf(0.08f32);
    } else {
        e.amount = ba(g, c)? * sf(0.06f32);
        e.margin += 5;
        set(&mut e.news_tail, __content.string(13233));
    }
    if g.exchange_rate * Ext::from_f64(1200.0) < e.amount {
        let k = cast::<Ext, _>(rng.rnd_range(10, 1));
        e.amount = (Ext::from_f64(100.0) * k + Ext::from_f64(700.0)) * g.exchange_rate;
    }
    e.subst_tokens(g, ui, rng);
    if is_y(g) {
        e.margin += 3;
        g.scenario_stage = 6;
        if e.show_text == "Y" {
            e.margin += 4;
            set(&mut e.news_tail, __content.string(13234));
            g.scenario_headline =
                s(__content.string(13235)) + &e.corp_symbol + __content.string(13236);
            if e.clock_sec % 2 == 1 {
                g.scenario_headline = s(__content.string(13237))
                    + &e.corp_symbol
                    + __content.string(13238);
            }
        } else {
            set(&mut e.news_tail, __content.string(13239));
            g.scenario_headline =
                s(__content.string(13240)) + &e.corp_symbol + __content.string(13241);
        }
    } else {
        e.charge_extraordinary(g);
        e.notoriety = e.notoriety.wrapping_sub(1);
    }
    Ok(())
}

// case -34: damage suit over the sweetener / Zing! Cola banned in schools.
fn case_m34(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 34 {
        abort(g);
        return Ok(());
    }
    e.margin -= 12;
    let c = corp(g);
    e.amount = ba(g, c)? * sf(0.1f32);
    if g.exchange_rate * Ext::from_f64(3000.0) < e.amount {
        let k = cast::<Ext, _>(rng.rnd_range(10, 1));
        e.amount = (Ext::from_f64(100.0) * k + Ext::from_f64(2500.0)) * g.exchange_rate;
    }
    e.subst_tokens(g, ui, rng);
    e.charge_expense(g);
    e.market_cap = sf(0.7f32) * e.market_cap;
    e.news_tail = s("hit by ") + &e.amount_text + __content.string(13242);
    if e.show_text == "Y" {
        set(&mut e.news_tail, __content.string(13243));
    }
    Ok(())
}

// case 35: a flawed gigachip for the manned Mars mission.
fn case_35(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    let c = corp(g);
    e.amount = ba(g, c)? * sf(0.11f32);
    e.event_year = to_quad(cast::<Ext, _>(e.event_year) + Ext::from_f64(13.0));
    e.margin += 5;
    set(&mut e.news_tail, __content.string(13244));
    e.subst_tokens(g, ui, rng);
    if is_y(g) {
        e.margin += 4;
        let c = corp(g);
        e.amount = -sf(0.1f32) * ba(g, c)?;
        e.charge_expense(g);
        g.scenario_stage = 3;
        e.news_tail = PbStr::new();
        g.scenario_headline =
            e.corp_symbol.clone() + __content.string(13245);
        if e.clock_sec % 2 == 0 {
            g.scenario_headline =
                s(__content.string(13246)) + &e.corp_symbol + __content.string(13247);
        }
    } else {
        e.charge_expense(g);
        e.notoriety = e.notoriety.wrapping_sub(1);
    }
    Ok(())
}

// case -35: the doomed Mars Explorer.
fn case_m35(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> R {
    let __content = crate::content::current();
    if e.industry != 35 {
        abort(g);
        return Ok(());
    }
    e.amount = e.market_cap * Ext::from_f64(15.0);
    e.margin -= 15;
    e.charge_expense(g);
    e.subst_tokens(g, ui, rng);
    e.market_cap = sf(0.65f32) * e.market_cap;
    e.news_tail = s(__content.string(13178)) + &e.amount_text + __content.string(13248);
    Ok(())
}
